use crate::{
    domain::{Actor, Fault, Submission},
    store::Store,
};
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};
use axum::{
    Extension, Json, Router,
    extract::{
        DefaultBodyLimit, MatchedPath, Path, Query, Request, State, rejection::JsonRejection,
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid(_) => (400, "invalid_input"),
            Self::Unauthorized => (401, "unauthorized"),
            Self::Forbidden => (403, "forbidden"),
            Self::NotFound => (404, "not_found"),
            Self::Conflict(_) => (409, "conflict"),
            Self::Capacity => (429, "queue_full"),
            Self::Database(_) => (503, "temporarily_unavailable"),
        };
        if let Self::Database(e) = &self {
            tracing::error!(error=%e,"database_error");
        }
        (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":{"code":code,"message":self.to_string()}})),
        )
            .into_response()
    }
}
pub fn authenticate(headers: &HeaderMap) -> Result<Actor, Fault> {
    let key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(Fault::Unauthorized)?;
    let digest = Sha256::digest(key.as_bytes());
    for (token, tenant, role) in [
        ("demo-manager-a", "factory-a", "manager"),
        ("demo-operator-a", "factory-a", "operator"),
        ("demo-viewer-a", "factory-a", "viewer"),
        ("demo-manager-b", "factory-b", "manager"),
    ] {
        if bool::from(
            digest
                .as_slice()
                .ct_eq(Sha256::digest(token.as_bytes()).as_slice()),
        ) {
            return Ok(Actor {
                tenant: tenant.into(),
                role: role.into(),
            });
        }
    }
    Err(Fault::Unauthorized)
}
async fn auth(mut req: Request, next: Next) -> Response {
    match authenticate(req.headers()) {
        Ok(a) => {
            req.extensions_mut().insert(a);
            let mut r = next.run(req).await;
            r.headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            r
        }
        Err(e) => e.into_response(),
    }
}
async fn observe(req: Request, next: Next) -> Response {
    let id = Uuid::new_v4().to_string();
    let now = std::time::Instant::now();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    let method = req.method().to_string();
    let mut response=match tokio::time::timeout(std::time::Duration::from_secs(10),next.run(req)).await{Ok(r)=>r,Err(_)=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":{"code":"request_timeout","message":"同じ要求キーで再試行してください。"}}))).into_response()};
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).unwrap_or(HeaderValue::from_static("unavailable")),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert("x-frame-options", HeaderValue::from_static("DENY"));
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'"));
    tracing::info!(request_id=%id,%method,%route,status=response.status().as_u16(),duration_ms=now.elapsed().as_millis(),"http_request");
    response
}
fn parse_json<T>(input: Result<Json<T>, JsonRejection>) -> Result<T, Box<Response>> {
    input.map(|Json(v)|v).map_err(|e|{let status=e.status();Box::new((status,Json(json!({"error":{"code":"invalid_json","message":"JSON形式・型・項目名・Content-Type・本文上限を確認してください。"}}))).into_response())})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}
impl Page {
    fn bounds(&self) -> Result<(i64, i64), Fault> {
        let (l, o) = (self.limit.unwrap_or(50), self.offset.unwrap_or(0));
        if !(1..=100).contains(&l) || !(0..=10000).contains(&o) {
            return Err(Fault::Invalid(
                "limitは1〜100、offsetは0〜10000です。".into(),
            ));
        }
        Ok((l, o))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    version: i32,
}

pub fn router(store: Store) -> Router {
    let private =
        Router::new()
            .route(
                "/session",
                get(|Extension(a): Extension<Actor>| async move { Json(a) }),
            )
            .route(
                "/summary",
                get(
                    |State(s): State<Store>, Extension(a): Extension<Actor>| async move {
                        s.summary(&a.tenant).await.map(Json)
                    },
                ),
            )
            .route("/jobs", get(list_jobs).post(submit))
            .route(
                "/jobs/{id}",
                get(
                    |State(s): State<Store>,
                     Extension(a): Extension<Actor>,
                     Path(id): Path<Uuid>| async move {
                        s.get(&a.tenant, id).await.map(Json)
                    },
                ),
            )
            .route(
                "/jobs/{id}/audit",
                get(
                    |State(s): State<Store>,
                     Extension(a): Extension<Actor>,
                     Path(id): Path<Uuid>| async move {
                        s.audits(&a.tenant, id).await.map(Json)
                    },
                ),
            )
            .route("/jobs/{id}/cancel", post(cancel))
            .route("/jobs/{id}/retry", post(retry))
            .route("/catalog", get(catalog))
            .route("/graphql", post(graphql))
            .route("/metrics", get(metrics))
            .layer(middleware::from_fn(auth));
    Router::new()
        .nest("/api", private)
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route(
            "/readyz",
            get(|State(s): State<Store>| async move {
                sqlx::query("SELECT 1")
                    .execute(&s.pool)
                    .await
                    .map(|_| Json(json!({"status":"ready"})))
                    .map_err(Fault::Database)
            }),
        )
        .route(
            "/",
            get(|| async { Html(include_str!("../web/dist/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("../web/dist/app.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [("content-type", "text/css; charset=utf-8")],
                    include_str!("../web/app.css"),
                )
            }),
        )
        .route(
            "/openapi.json",
            get(|| async {
                (
                    [("content-type", "application/json")],
                    include_str!("../openapi.json"),
                )
            }),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn(observe))
        .with_state(store)
}
async fn list_jobs(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    Query(p): Query<Page>,
) -> Result<Json<serde_json::Value>, Fault> {
    let (l, o) = p.bounds()?;
    Ok(Json(
        json!({"data":s.list(&a.tenant,l,o).await?,"limit":l,"offset":o}),
    ))
}
async fn catalog(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    Query(p): Query<Page>,
) -> Result<Json<serde_json::Value>, Fault> {
    let (l, o) = p.bounds()?;
    Ok(Json(
        json!({"data":s.catalog(&a.tenant,l,o).await?,"limit":l,"offset":o}),
    ))
}
async fn submit(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    headers: HeaderMap,
    input: Result<Json<Submission>, JsonRejection>,
) -> Response {
    let input = match parse_json(input) {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let key = headers
        .get("idempotency-key")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    match s.submit(&a, key, input).await {
        Ok((job, replay)) => {
            let status = if replay {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            };
            (
                [(
                    "idempotency-replayed",
                    if replay { "true" } else { "false" },
                )],
                (status, Json(job)),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}
async fn cancel(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    Path(id): Path<Uuid>,
    input: Result<Json<Action>, JsonRejection>,
) -> Response {
    action(s, a, id, "cancel", input).await
}
async fn retry(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    Path(id): Path<Uuid>,
    input: Result<Json<Action>, JsonRejection>,
) -> Response {
    action(s, a, id, "retry", input).await
}
async fn action(
    s: Store,
    a: Actor,
    id: Uuid,
    command: &str,
    input: Result<Json<Action>, JsonRejection>,
) -> Response {
    let input = match parse_json(input) {
        Ok(v) => v,
        Err(e) => return *e,
    };
    s.action(&a, id, command, input.version)
        .await
        .map(Json)
        .into_response()
}
async fn metrics(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
) -> Result<Response, Fault> {
    if a.role != "manager" {
        return Err(Fault::Forbidden);
    }
    let summary = s.summary(&a.tenant).await?;
    let mut text = String::new();
    for name in [
        "queued",
        "running",
        "completed",
        "rejected",
        "failed",
        "cancelled",
        "products",
    ] {
        text.push_str(&format!(
            "# TYPE importflow_{name} gauge\nimportflow_{name} {}\n",
            summary[name]
        ));
    }
    Ok(([("content-type", "text/plain; version=0.0.4")], text).into_response())
}

struct Queries;
#[derive(SimpleObject)]
struct JobInfo {
    id: String,
    label: String,
    status: String,
    attempts: i32,
    row_count: i32,
    version: i32,
}
#[Object]
impl Queries {
    async fn jobs(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] limit: i32,
    ) -> async_graphql::Result<Vec<JobInfo>> {
        if !(1..=50).contains(&limit) {
            return Err("limit must be 1..50".into());
        }
        let s = ctx.data::<Store>()?;
        let a = ctx.data::<Actor>()?;
        Ok(s.list(&a.tenant, i64::from(limit), 0)
            .await?
            .into_iter()
            .map(|j| JobInfo {
                id: j.id.to_string(),
                label: j.label,
                status: j.status,
                attempts: j.attempts,
                row_count: j.row_count,
                version: j.version,
            })
            .collect())
    }
}
async fn graphql(
    State(s): State<Store>,
    Extension(a): Extension<Actor>,
    input: Result<Json<async_graphql::Request>, JsonRejection>,
) -> Response {
    let request = match parse_json(input) {
        Ok(v) => v,
        Err(e) => return *e,
    };
    if request.query.len() > 4096 {
        return Fault::Invalid("GraphQLは4096バイトまでです。".into()).into_response();
    }
    let schema = Schema::build(Queries, EmptyMutation, EmptySubscription)
        .data(s)
        .limit_depth(4)
        .limit_complexity(100)
        .finish();
    Json(schema.execute(request.data(a)).await).into_response()
}
