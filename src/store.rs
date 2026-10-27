use crate::domain::{Actor, Fault, Issue, Submission, ValidatedBatch};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
}
#[derive(Clone, Debug, sqlx::FromRow, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub tenant: String,
    pub label: String,
    pub status: String,
    pub attempts: i32,
    pub version: i32,
    pub row_count: i32,
    pub issues: Value,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub csv: String,
    #[serde(skip_serializing)]
    pub token: Option<Uuid>,
    #[serde(skip_serializing)]
    pub simulate_failures: i32,
}
#[derive(Serialize, sqlx::FromRow)]
pub struct CatalogRow {
    pub sku: String,
    pub name: String,
    pub quantity: i32,
    pub unit_price: i64,
    pub job_id: Uuid,
}
#[derive(Serialize, sqlx::FromRow)]
pub struct Audit {
    pub action: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

async fn event(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    id: Uuid,
    action: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit(tenant,job_id,action,actor) VALUES($1,$2,$3,$4)")
        .bind(tenant)
        .bind(id)
        .bind(action)
        .bind(actor)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
impl Store {
    pub async fn init(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(7945111)")
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(include_str!("../schema.sql"))
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }
    pub async fn get(&self, tenant: &str, id: Uuid) -> Result<Job, Fault> {
        sqlx::query_as("SELECT * FROM jobs WHERE tenant=$1 AND id=$2")
            .bind(tenant)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Fault::NotFound)
    }
    pub async fn list(&self, tenant: &str, limit: i64, offset: i64) -> Result<Vec<Job>, Fault> {
        Ok(sqlx::query_as(
            "SELECT * FROM jobs WHERE tenant=$1 ORDER BY created_at DESC,id LIMIT $2 OFFSET $3",
        )
        .bind(tenant)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn catalog(
        &self,
        tenant: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CatalogRow>, Fault> {
        Ok(
            sqlx::query_as("SELECT * FROM catalog WHERE tenant=$1 ORDER BY sku LIMIT $2 OFFSET $3")
                .bind(tenant)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?,
        )
    }
    pub async fn audits(&self, tenant: &str, id: Uuid) -> Result<Vec<Audit>, Fault> {
        self.get(tenant, id).await?;
        Ok(sqlx::query_as("SELECT action,actor,created_at FROM audit WHERE tenant=$1 AND job_id=$2 ORDER BY id DESC LIMIT 50").bind(tenant).bind(id).fetch_all(&self.pool).await?)
    }
    pub async fn summary(&self, tenant: &str) -> Result<Value, Fault> {
        let (queued,running,completed,rejected,failed,cancelled):(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE status='queued'),count(*) FILTER(WHERE status='running'),count(*) FILTER(WHERE status='completed'),count(*) FILTER(WHERE status='rejected'),count(*) FILTER(WHERE status='failed'),count(*) FILTER(WHERE status='cancelled') FROM jobs WHERE tenant=$1").bind(tenant).fetch_one(&self.pool).await?;
        let (products,value):(i64,String)=sqlx::query_as("SELECT count(*),COALESCE(sum(quantity::bigint*unit_price),0)::text FROM catalog WHERE tenant=$1").bind(tenant).fetch_one(&self.pool).await?;
        Ok(
            json!({"queued":queued,"running":running,"completed":completed,"rejected":rejected,"failed":failed,"cancelled":cancelled,"products":products,"catalog_value":value}),
        )
    }
    pub async fn submit(
        &self,
        a: &Actor,
        key: &str,
        input: Submission,
    ) -> Result<(Job, bool), Fault> {
        if !a.write() {
            return Err(Fault::Forbidden);
        }
        if !(8..=80).contains(&key.len())
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(Fault::Invalid(
                "要求キーは8〜80文字の英数字・_・-で指定してください。".into(),
            ));
        }
        let input = input.normalize()?;
        let hash = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&input)
                    .map_err(|_| Fault::Invalid("入力が不正です。".into()))?
            )
        );
        let mut tx = self.pool.begin().await?;
        // One tenant admission lock serializes idempotency lookup and queue capacity.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,42))")
            .bind(&a.tenant)
            .execute(&mut *tx)
            .await?;
        if let Some((id, old_hash)) = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id,content_hash FROM jobs WHERE tenant=$1 AND request_key=$2",
        )
        .bind(&a.tenant)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if old_hash != hash {
                return Err(Fault::Conflict("同じ要求キーで内容が異なります。".into()));
            }
            let job = sqlx::query_as("SELECT * FROM jobs WHERE tenant=$1 AND id=$2")
                .bind(&a.tenant)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok((job, true));
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE tenant=$1 AND status IN ('queued','running')",
        )
        .bind(&a.tenant)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 20 {
            return Err(Fault::Capacity);
        }
        let id = Uuid::new_v4();
        let job=sqlx::query_as("INSERT INTO jobs(id,tenant,label,request_key,content_hash,csv,simulate_failures,status) VALUES($1,$2,$3,$4,$5,$6,$7,'queued') RETURNING *").bind(id).bind(&a.tenant).bind(input.label).bind(key).bind(hash).bind(input.csv).bind(i32::from(input.simulate_failures)).fetch_one(&mut *tx).await?;
        event(&mut tx, &a.tenant, id, "submitted", &a.role).await?;
        tx.commit().await?;
        Ok((job, false))
    }
    pub async fn action(
        &self,
        a: &Actor,
        id: Uuid,
        command: &str,
        version: i32,
    ) -> Result<Job, Fault> {
        if !a.write() || (command == "retry" && a.role != "manager") {
            return Err(Fault::Forbidden);
        }
        if version < 1 {
            return Err(Fault::Invalid("versionは正の整数です。".into()));
        }
        if command != "cancel" && command != "retry" {
            return Err(Fault::Invalid("不正な操作です。".into()));
        }
        let mut tx = self.pool.begin().await?;
        let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE tenant=$1 AND id=$2 FOR UPDATE")
            .bind(&a.tenant)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Fault::NotFound)?;
        if command == "cancel" && job.status == "cancelled" {
            tx.commit().await?;
            return Ok(job);
        }
        if job.version != version
            || (command == "retry" && job.status != "failed")
            || (command == "cancel" && job.status != "queued" && job.status != "running")
        {
            return Err(Fault::Conflict(
                "状態が変わりました。最新の情報で操作してください。".into(),
            ));
        }
        if command == "retry" {
            // Retried failed jobs also participate in admission control.
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,42))")
                .bind(&a.tenant)
                .execute(&mut *tx)
                .await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE tenant=$1 AND status IN ('queued','running')",
            )
            .bind(&a.tenant)
            .fetch_one(&mut *tx)
            .await?;
            if count >= 20 {
                return Err(Fault::Capacity);
            }
        }
        let status = if command == "cancel" {
            "cancelled"
        } else {
            "queued"
        };
        let job=sqlx::query_as("UPDATE jobs SET status=$3,attempts=CASE WHEN $3='queued' THEN 0 ELSE attempts END,token=NULL,lease_until=NULL,version=version+1,message='',next_attempt=clock_timestamp(),updated_at=clock_timestamp() WHERE tenant=$1 AND id=$2 RETURNING *").bind(&a.tenant).bind(id).bind(status).fetch_one(&mut *tx).await?;
        event(&mut tx, &a.tenant, id, command, &a.role).await?;
        tx.commit().await?;
        Ok(job)
    }
    pub async fn claim(&self) -> Result<Option<Job>, Fault> {
        let mut tx = self.pool.begin().await?;
        // Recovery must not block every worker behind a busy expired job.
        // Bound the batch and acquire only rows that can be locked immediately.
        sqlx::query("WITH candidates AS(SELECT id FROM jobs WHERE status='running' AND lease_until<=clock_timestamp() AND attempts>=3 ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 50), expired AS(UPDATE jobs j SET status='failed',token=NULL,lease_until=NULL,version=version+1,message='処理期限切れが上限に達しました。',updated_at=clock_timestamp() FROM candidates c WHERE j.id=c.id RETURNING j.tenant,j.id) INSERT INTO audit(tenant,job_id,action,actor) SELECT tenant,id,'failed','worker' FROM expired").execute(&mut *tx).await?;
        let job:Option<Job>=sqlx::query_as("WITH candidate AS(SELECT id FROM jobs WHERE attempts<3 AND ((status='queued' AND next_attempt<=clock_timestamp()) OR (status='running' AND lease_until<=clock_timestamp())) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE jobs j SET status='running',attempts=attempts+1,token=$1,lease_until=clock_timestamp()+interval '30 seconds',version=version+1,updated_at=clock_timestamp() FROM candidate WHERE j.id=candidate.id RETURNING j.*").bind(Uuid::new_v4()).fetch_optional(&mut *tx).await?;
        if let Some(j) = &job {
            event(&mut tx, &j.tenant, j.id, "claimed", "worker").await?;
        }
        tx.commit().await?;
        Ok(job)
    }
    pub async fn finish(
        &self,
        job: &Job,
        result: Result<ValidatedBatch, Vec<Issue>>,
        transient: bool,
    ) -> Result<bool, Fault> {
        let mut tx = self.pool.begin().await?;
        let valid:Option<Uuid>=sqlx::query_scalar("SELECT id FROM jobs WHERE id=$1 AND tenant=$2 AND status='running' AND token=$3 AND lease_until>clock_timestamp() FOR UPDATE").bind(job.id).bind(&job.tenant).bind(job.token).fetch_optional(&mut *tx).await?;
        if valid.is_none() {
            return Ok(false);
        }
        // Recheck after row-lock waits; an old worker must never commit after expiry.
        let alive: bool =
            sqlx::query_scalar("SELECT lease_until>clock_timestamp() FROM jobs WHERE id=$1")
                .bind(job.id)
                .fetch_one(&mut *tx)
                .await?;
        if !alive {
            return Ok(false);
        }
        let mut status = "completed";
        let mut count = 0;
        let mut issues = json!([]);
        let mut message = String::new();
        if transient {
            status = if job.attempts >= 3 {
                "failed"
            } else {
                "queued"
            };
            message = "模擬的な一時障害。再試行待ちです。".into();
        } else {
            match result {
                Err(errors) => {
                    status = "rejected";
                    message = "CSVに不備があるため全件取り込みませんでした。".into();
                    issues = json!(errors);
                }
                Ok(rows) => {
                    // Savepoint permits conflict rejection while rolling back every inserted SKU.
                    sqlx::query("SAVEPOINT batch_rows")
                        .execute(&mut *tx)
                        .await?;
                    for row in rows.iter() {
                        let inserted=sqlx::query("INSERT INTO catalog(tenant,sku,name,quantity,unit_price,job_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(tenant,sku) DO NOTHING").bind(&job.tenant).bind(&row.sku).bind(&row.name).bind(row.quantity).bind(row.unit_price).bind(job.id).execute(&mut *tx).await?;
                        if inserted.rows_affected() == 0 {
                            status = "rejected";
                            message = "既存SKUとの重複があるため全件取り込みませんでした。".into();
                            break;
                        }
                    }
                    if status == "rejected" {
                        sqlx::query("ROLLBACK TO SAVEPOINT batch_rows")
                            .execute(&mut *tx)
                            .await?;
                    } else {
                        count = rows.len() as i32;
                    }
                }
            }
        }
        // All catalogue locks have now been acquired. Time can pass while awaiting them.
        let alive: bool =
            sqlx::query_scalar("SELECT lease_until>clock_timestamp() FROM jobs WHERE id=$1")
                .bind(job.id)
                .fetch_one(&mut *tx)
                .await?;
        if !alive {
            return Ok(false);
        }
        sqlx::query("UPDATE jobs SET status=$2,row_count=$3,issues=$4,message=$5,token=NULL,lease_until=NULL,version=version+1,updated_at=clock_timestamp(),next_attempt=clock_timestamp()+make_interval(secs=>$6) WHERE id=$1").bind(job.id).bind(status).bind(count).bind(issues).bind(message).bind(2_f64.powi(job.attempts-1)).execute(&mut *tx).await?;
        event(&mut tx, &job.tenant, job.id, status, "worker").await?;
        tx.commit().await?;
        Ok(true)
    }
}
