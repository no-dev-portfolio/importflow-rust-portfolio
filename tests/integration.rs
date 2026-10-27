#[tokio::test]
async fn competing_batches_do_not_partially_import() {
    let f = Fixture::new().await;
    let j1 = submit(
        &f.store,
        "conflict-batch-1",
        "sku,name,quantity,unit_price\nSHARED,共通,1,1\nUNIQUE-A,A,1,1\n",
    )
    .await;
    let j2 = submit(
        &f.store,
        "conflict-batch-2",
        "sku,name,quantity,unit_price\nUNIQUE-B,B,1,1\nSHARED,共通,1,1\n",
    )
    .await;
    let s1 = f.store.clone();
    let s2 = f.store.clone();
    let (a, b) = tokio::join!(worker::tick(&s1), worker::tick(&s2));
    assert!(a.unwrap());
    assert!(b.unwrap());
    let a = f.store.get("factory-a", j1.id).await.unwrap();
    let b = f.store.get("factory-a", j2.id).await.unwrap();
    assert!(
        (a.status == "completed" && b.status == "rejected")
            || (a.status == "rejected" && b.status == "completed")
    );
    assert_eq!(f.store.catalog("factory-a", 100, 0).await.unwrap().len(), 2);
    f.close().await;
}
#[tokio::test]
async fn summary_money_is_exact_beyond_javascript_precision() {
    let f = Fixture::new().await;
    let csv = format!(
        "sku,name,quantity,unit_price\n{}",
        (0..1000)
            .map(|i| format!("SKU-{i},部品,100000,100000000\n"))
            .collect::<String>()
    );
    let j = submit(&f.store, "big-integer-sum", &csv).await;
    worker::tick(&f.store).await.unwrap();
    assert_eq!(
        f.store.get("factory-a", j.id).await.unwrap().row_count,
        1000
    );
    assert_eq!(
        f.store.summary("factory-a").await.unwrap()["catalog_value"],
        "10000000000000000"
    );
    f.close().await;
}
#[tokio::test]
async fn http_media_size_pagination_and_schema() {
    let f = Fixture::new().await;
    for (content, body, expected) in [
        ("application/jsonp", "{}".to_string(), 415),
        ("application/json", "x".repeat(1024 * 1024 + 1), 413),
        (
            "application/json",
            r#"{"label":"a","csv":"x"} {}"#.to_string(),
            400,
        ),
    ] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/jobs")
            .header("authorization", "Bearer demo-manager-a")
            .header("content-type", content)
            .body(Body::from(body))
            .unwrap();
        assert_eq!(
            api::router(f.store.clone())
                .oneshot(req)
                .await
                .unwrap()
                .status()
                .as_u16(),
            expected
        );
    }
    assert_eq!(
        request(
            &f.store,
            "GET",
            "/api/jobs?limit=101",
            "demo-manager-a",
            json!({})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let req = Request::builder()
        .uri("/openapi.json")
        .body(Body::empty())
        .unwrap();
    let res = api::router(f.store.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(spec["openapi"], "3.0.3");
    f.close().await;
}

#[tokio::test]
async fn review_claim_skips_locked_exhausted_job() {
    let f = Fixture::new().await;
    let exhausted = submit(&f.store, "review-exhausted", SAMPLE).await;
    let ready = submit(&f.store, "review-ready-job", SAMPLE).await;
    sqlx::query("UPDATE jobs SET status='running',attempts=3,token=$1,lease_until=clock_timestamp()-interval '1 second' WHERE id=$2")
        .bind(Uuid::new_v4()).bind(exhausted.id).execute(&f.store.pool).await.unwrap();
    let mut blocker = f.store.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM jobs WHERE id=$1 FOR UPDATE")
        .bind(exhausted.id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_millis(350), f.store.claim()).await;
    let elapsed = started.elapsed();
    blocker.rollback().await.unwrap();
    let ready_status = f.store.get("factory-a", ready.id).await.unwrap().status;
    println!(
        "claim_completed={} elapsed_ms={} unrelated_ready_status={}",
        result.is_ok(),
        elapsed.as_millis(),
        ready_status
    );
    let claimed = result
        .as_ref()
        .expect("claim must skip a locked exhausted job")
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(claimed.id, ready.id);
    assert_eq!(ready_status, "running");
    assert!(f.store.claim().await.unwrap().is_none());
    assert_eq!(
        f.store.get("factory-a", exhausted.id).await.unwrap().status,
        "failed"
    );
    let audit_count = f
        .store
        .audits("factory-a", exhausted.id)
        .await
        .unwrap()
        .iter()
        .filter(|a| a.action == "failed")
        .count();
    assert_eq!(audit_count, 1);
    assert!(f.store.claim().await.unwrap().is_none());
    assert_eq!(
        f.store
            .audits("factory-a", exhausted.id)
            .await
            .unwrap()
            .iter()
            .filter(|a| a.action == "failed")
            .count(),
        1
    );
    println!("unrelated_job_claimed_while_locked=true expired_reaped_once_after_release=true");
    f.close().await;
    assert!(
        result.is_ok(),
        "claim blocked behind a locked exhausted job instead of claiming unrelated queued work"
    );
}

#[tokio::test]
async fn expired_cleanup_is_bounded_and_not_repeated() {
    let f = Fixture::new().await;
    for i in 0..55 {
        let j = submit(&f.store, &format!("bounded-cleanup-{i:03}"), SAMPLE).await;
        f.store
            .action(&manager(), j.id, "cancel", j.version)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE jobs SET status='running',attempts=3,token=$1,lease_until=clock_timestamp()-interval '1 second'")
        .bind(Uuid::new_v4()).execute(&f.store.pool).await.unwrap();
    assert!(f.store.claim().await.unwrap().is_none());
    assert_eq!(f.store.summary("factory-a").await.unwrap()["failed"], 50);
    assert!(f.store.claim().await.unwrap().is_none());
    assert_eq!(f.store.summary("factory-a").await.unwrap()["failed"], 55);
    assert!(f.store.claim().await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit WHERE action='failed'")
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 55);
    f.close().await;
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use importflow::{
    api,
    domain::{Actor, SAMPLE, Submission, validate_csv},
    store::{Job, Store},
    worker,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    store: Store,
    admin: PgPool,
    schema: String,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL is required; no integration-test skip");
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let setting = schema.clone();
        let pool=PgPoolOptions::new().max_connections(8).after_connect(move|c,_|{let setting=setting.clone();Box::pin(async move{sqlx::query("SELECT set_config('search_path',$1,false),set_config('statement_timeout','5000',false),set_config('lock_timeout','4000',false)").bind(&setting).execute(c).await?;Ok(())})}).connect(&url).await.unwrap();
        let store = Store { pool };
        store.init().await.unwrap();
        Self {
            store,
            admin,
            schema,
        }
    }
    async fn close(self) {
        self.store.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}
fn manager() -> Actor {
    Actor {
        tenant: "factory-a".into(),
        role: "manager".into(),
    }
}
fn input(csv: &str) -> Submission {
    Submission {
        label: "検証用".into(),
        csv: csv.into(),
        simulate_failures: 0,
    }
}
async fn submit(s: &Store, key: &str, csv: &str) -> Job {
    s.submit(&manager(), key, input(csv)).await.unwrap().0
}

#[tokio::test]
async fn successful_import_and_replay() {
    let f = Fixture::new().await;
    let j = submit(&f.store, "success-001", SAMPLE).await;
    assert!(worker::tick(&f.store).await.unwrap());
    let done = f.store.get("factory-a", j.id).await.unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.row_count, 3);
    let (replay, yes) = f
        .store
        .submit(&manager(), "success-001", input(SAMPLE))
        .await
        .unwrap();
    assert!(yes);
    assert_eq!(replay.id, j.id);
    assert_eq!(f.store.catalog("factory-a", 100, 0).await.unwrap().len(), 3);
    assert_eq!(f.store.audits("factory-a", j.id).await.unwrap().len(), 3);
    f.close().await;
}
#[tokio::test]
async fn bad_csv_rolls_back_all() {
    let f = Fixture::new().await;
    let j = submit(
        &f.store,
        "invalid-001",
        &format!("{SAMPLE}BAD,不備,-1,10\n"),
    )
    .await;
    worker::tick(&f.store).await.unwrap();
    let job = f.store.get("factory-a", j.id).await.unwrap();
    assert_eq!(job.status, "rejected");
    assert_eq!(job.row_count, 0);
    assert!(
        f.store
            .catalog("factory-a", 100, 0)
            .await
            .unwrap()
            .is_empty()
    );
    f.close().await;
}
#[tokio::test]
async fn existing_sku_rejects_new_rows_atomically() {
    let f = Fixture::new().await;
    submit(&f.store, "existing-001", SAMPLE).await;
    worker::tick(&f.store).await.unwrap();
    let j = submit(
        &f.store,
        "existing-002",
        "sku,name,quantity,unit_price\nAAA,先に追加する行,1,1\nPART-101,既存,1,1\n",
    )
    .await;
    worker::tick(&f.store).await.unwrap();
    assert_eq!(
        f.store.get("factory-a", j.id).await.unwrap().status,
        "rejected"
    );
    assert_eq!(f.store.catalog("factory-a", 100, 0).await.unwrap().len(), 3);
    f.close().await;
}
#[tokio::test]
async fn same_key_concurrently_creates_one_job() {
    let f = Fixture::new().await;
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let s = f.store.clone();
        tasks.push(tokio::spawn(async move {
            s.submit(&manager(), "parallel-key", input(SAMPLE))
                .await
                .unwrap()
        }));
    }
    let mut ids = std::collections::HashSet::new();
    let mut created = 0;
    for task in tasks {
        let (j, replay) = task.await.unwrap();
        ids.insert(j.id);
        created += usize::from(!replay);
    }
    assert_eq!(ids.len(), 1);
    assert_eq!(created, 1);
    assert!(
        f.store
            .submit(&manager(), "parallel-key", input("different"))
            .await
            .is_err()
    );
    f.close().await;
}
#[tokio::test]
async fn parallel_workers_claim_once() {
    let f = Fixture::new().await;
    let j = submit(&f.store, "worker-parallel", SAMPLE).await;
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let s = f.store.clone();
        tasks.push(tokio::spawn(async move { worker::tick(&s).await.unwrap() }));
    }
    let mut claimed = 0;
    for task in tasks {
        claimed += usize::from(task.await.unwrap());
    }
    assert_eq!(claimed, 1);
    assert_eq!(f.store.get("factory-a", j.id).await.unwrap().attempts, 1);
    f.close().await;
}
#[tokio::test]
async fn cancel_fences_running_worker() {
    let f = Fixture::new().await;
    submit(&f.store, "cancel-running", SAMPLE).await;
    let job = f.store.claim().await.unwrap().unwrap();
    let cancelled = f
        .store
        .action(&manager(), job.id, "cancel", job.version)
        .await
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(
        !f.store
            .finish(&job, validate_csv(SAMPLE), false)
            .await
            .unwrap()
    );
    assert!(
        f.store
            .catalog("factory-a", 100, 0)
            .await
            .unwrap()
            .is_empty()
    );
    let again = f
        .store
        .action(&manager(), job.id, "cancel", 1)
        .await
        .unwrap();
    assert_eq!(again.version, cancelled.version);
    f.close().await;
}
#[tokio::test]
async fn expired_lease_fences_old_worker_and_recovers() {
    let f = Fixture::new().await;
    submit(&f.store, "lease-expired", SAMPLE).await;
    let old = f.store.claim().await.unwrap().unwrap();
    sqlx::query("UPDATE jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(old.id)
        .execute(&f.store.pool)
        .await
        .unwrap();
    let new = f.store.claim().await.unwrap().unwrap();
    assert_ne!(old.token, new.token);
    assert_eq!(new.attempts, 2);
    assert!(
        !f.store
            .finish(&old, validate_csv(SAMPLE), false)
            .await
            .unwrap()
    );
    assert!(
        f.store
            .finish(&new, validate_csv(SAMPLE), false)
            .await
            .unwrap()
    );
    assert_eq!(f.store.catalog("factory-a", 100, 0).await.unwrap().len(), 3);
    f.close().await;
}
#[tokio::test]
async fn lease_expires_while_waiting_catalog_lock() {
    let f = Fixture::new().await;
    submit(&f.store, "lease-lock-wait", SAMPLE).await;
    let job = f.store.claim().await.unwrap().unwrap();
    let mut blocker = f.store.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO catalog VALUES('factory-a','PART-101','blocker',1,1,$1)")
        .bind(job.id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE jobs SET lease_until=clock_timestamp()+interval '200 milliseconds' WHERE id=$1",
    )
    .bind(job.id)
    .execute(&f.store.pool)
    .await
    .unwrap();
    let s = f.store.clone();
    let old = job.clone();
    let work = tokio::spawn(async move { s.finish(&old, validate_csv(SAMPLE), false).await });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    blocker.rollback().await.unwrap();
    assert!(!work.await.unwrap().unwrap());
    assert!(
        f.store
            .catalog("factory-a", 100, 0)
            .await
            .unwrap()
            .is_empty()
    );
    f.close().await;
}
#[tokio::test]
async fn retry_backoff_and_exhaustion() {
    let f = Fixture::new().await;
    submit(&f.store, "retry-backoff", SAMPLE).await;
    for attempt in 1..=3 {
        let j = f.store.claim().await.unwrap().unwrap();
        assert_eq!(j.attempts, attempt);
        f.store
            .finish(&j, validate_csv(SAMPLE), true)
            .await
            .unwrap();
        if attempt < 3 {
            assert!(f.store.claim().await.unwrap().is_none());
            sqlx::query("UPDATE jobs SET next_attempt=clock_timestamp()-interval '1 second'")
                .execute(&f.store.pool)
                .await
                .unwrap();
        }
    }
    let job = f.store.list("factory-a", 50, 0).await.unwrap().remove(0);
    assert_eq!(job.status, "failed");
    let retry = f
        .store
        .action(&manager(), job.id, "retry", job.version)
        .await
        .unwrap();
    assert_eq!(retry.attempts, 0);
    worker::tick(&f.store).await.unwrap();
    assert_eq!(
        f.store.get("factory-a", job.id).await.unwrap().status,
        "completed"
    );
    f.close().await;
}
#[tokio::test]
async fn expired_third_lease_becomes_failed() {
    let f = Fixture::new().await;
    let j = submit(&f.store, "exhausted-lease", SAMPLE).await;
    sqlx::query("UPDATE jobs SET status='running',attempts=3,token=$1,lease_until=clock_timestamp()-interval '1 second' WHERE id=$2").bind(Uuid::new_v4()).bind(j.id).execute(&f.store.pool).await.unwrap();
    assert!(f.store.claim().await.unwrap().is_none());
    assert_eq!(
        f.store.get("factory-a", j.id).await.unwrap().status,
        "failed"
    );
    f.close().await;
}
#[tokio::test]
async fn tenant_roles_and_versions() {
    let f = Fixture::new().await;
    let j = submit(&f.store, "tenant-role", SAMPLE).await;
    let b = Actor {
        tenant: "factory-b".into(),
        role: "manager".into(),
    };
    let v = Actor {
        tenant: "factory-a".into(),
        role: "viewer".into(),
    };
    assert!(f.store.get(&b.tenant, j.id).await.is_err());
    assert!(f.store.list(&b.tenant, 50, 0).await.unwrap().is_empty());
    assert!(f.store.action(&b, j.id, "cancel", 1).await.is_err());
    assert!(
        f.store
            .submit(&v, "viewer-001", input(SAMPLE))
            .await
            .is_err()
    );
    assert!(f.store.action(&v, j.id, "cancel", 1).await.is_err());
    assert!(
        f.store
            .action(&manager(), j.id, "cancel", 99)
            .await
            .is_err()
    );
    assert!(f.store.action(&manager(), j.id, "cancel", 0).await.is_err());
    f.close().await;
}
#[tokio::test]
async fn queue_capacity_and_replay_at_capacity() {
    let f = Fixture::new().await;
    for i in 0..20 {
        submit(&f.store, &format!("queue-{i:03}"), SAMPLE).await;
    }
    assert!(
        f.store
            .submit(&manager(), "queue-over", input(SAMPLE))
            .await
            .is_err()
    );
    assert!(
        f.store
            .submit(&manager(), "queue-000", input(SAMPLE))
            .await
            .unwrap()
            .1
    );
    f.close().await;
}
async fn request(
    s: &Store,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("idempotency-key", "api-request-001")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = api::router(s.clone()).oneshot(req).await.unwrap();
    let status = response.status();
    assert!(response.headers().contains_key("x-request-id"));
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let data = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, data)
}
#[tokio::test]
async fn http_auth_json_and_graphql_isolation() {
    let f = Fixture::new().await;
    assert_eq!(
        request(&f.store, "GET", "/api/summary", "bad", json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let body = json!({"label":"API取込","csv":SAMPLE});
    let (status, job) = request(
        &f.store,
        "POST",
        "/api/jobs",
        "demo-manager-a",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(job.get("csv").is_none());
    assert_eq!(
        request(
            &f.store,
            "POST",
            "/api/jobs",
            "demo-manager-a",
            body.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&f.store, "POST", "/api/jobs", "demo-viewer-a", body)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &f.store,
            "POST",
            "/api/jobs",
            "demo-manager-a",
            json!({"label":"x","csv":SAMPLE,"tenant":"factory-b"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let query = json!({"query":"{ jobs { id label status rowCount } }"});
    let (_, a) = request(
        &f.store,
        "POST",
        "/api/graphql",
        "demo-manager-a",
        query.clone(),
    )
    .await;
    assert_eq!(a["data"]["jobs"].as_array().unwrap().len(), 1);
    let (_, b) = request(&f.store, "POST", "/api/graphql", "demo-manager-b", query).await;
    assert!(b["data"]["jobs"].as_array().unwrap().is_empty());
    let (_, bad) = request(
        &f.store,
        "POST",
        "/api/graphql",
        "demo-manager-a",
        json!({"query":"{ jobs(limit: 999) { id } }"}),
    )
    .await;
    assert!(bad.get("errors").is_some());
    assert_eq!(
        request(&f.store, "GET", "/api/metrics", "demo-viewer-a", json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    f.close().await;
}
