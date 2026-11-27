//! Job queue in PostgreSQL: SKIP LOCKED claiming, a lease, retries after 1, 5 and 15 minutes,
//! then dead (shown on the Today page). Every job must be safe to run twice.

use crate::app::App;
use crate::error::Result;
use crate::{activity, notices};
use sqlx::{PgConnection, Row};

pub const RETRY_MINUTES: [i32; 3] = [1, 5, 15];
pub const LEASE_MINUTES: i32 = 5;

pub struct Job {
    pub id: String,
    pub tenant_id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
}

pub async fn enqueue(conn: &mut PgConnection, tenant: &str, kind: &str, payload: serde_json::Value, dedupe_key: Option<&str>) -> Result<bool> {
    let n = sqlx::query("INSERT INTO jobs (tenant_id, kind, payload, dedupe_key) VALUES ($1::uuid, $2, $3::jsonb, $4) ON CONFLICT DO NOTHING")
        .bind(tenant).bind(kind).bind(payload.to_string()).bind(dedupe_key).execute(&mut *conn).await?.rows_affected();
    Ok(n > 0)
}

/// Take the next due job. A job whose lease ran out (worker died) becomes dead-or-retry first.
pub async fn claim(app: &App, worker: &str) -> Result<Option<Job>> {
    let mut tx = app.pool.begin().await?;
    // Lease expired: the worker stopped before finishing.
    let stale = sqlx::query("SELECT id::text, tenant_id::text FROM jobs WHERE status = 'running' AND locked_until < now() FOR UPDATE SKIP LOCKED LIMIT 20")
        .fetch_all(&mut *tx).await?;
    for s in stale {
        fail_in(&mut tx, &s.get::<String, _>(0), &s.get::<String, _>(1), "The worker stopped before finishing (lease expired).").await?;
    }
    let r = sqlx::query(
        "SELECT id::text, tenant_id::text, kind, payload::text, attempts FROM jobs WHERE status IN ('queued','failed') AND run_at <= now() \
         ORDER BY run_at FOR UPDATE SKIP LOCKED LIMIT 1",
    )
    .fetch_optional(&mut *tx).await?;
    let Some(r) = r else {
        tx.commit().await?;
        return Ok(None);
    };
    let job = Job {
        id: r.get(0), tenant_id: r.get(1), kind: r.get(2),
        payload: serde_json::from_str(&r.get::<String, _>(3)).unwrap_or_default(), attempts: r.get(4),
    };
    sqlx::query("UPDATE jobs SET status = 'running', attempts = attempts + 1, locked_by = $2, locked_until = now() + make_interval(mins => $3), updated_at = now() WHERE id = $1::uuid")
        .bind(&job.id).bind(worker).bind(LEASE_MINUTES).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(job))
}

pub async fn done(app: &App, job: &Job) -> Result<()> {
    sqlx::query("UPDATE jobs SET status = 'done', locked_by = NULL, locked_until = NULL, updated_at = now() WHERE id = $1::uuid").bind(&job.id).execute(&app.pool).await?;
    Ok(())
}

async fn fail_in(conn: &mut PgConnection, id: &str, tenant: &str, error: &str) -> Result<()> {
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM jobs WHERE id = $1::uuid").bind(id).fetch_one(&mut *conn).await?;
    if attempts as usize >= RETRY_MINUTES.len() + 1 || attempts >= 3 {
        sqlx::query("UPDATE jobs SET status = 'dead', last_error = $2, locked_by = NULL, locked_until = NULL, updated_at = now() WHERE id = $1::uuid").bind(id).bind(error).execute(&mut *conn).await?;
        let kind: String = sqlx::query_scalar("SELECT kind FROM jobs WHERE id = $1::uuid").bind(id).fetch_one(&mut *conn).await?;
        notices::push(conn, tenant, None, "dead_job", &format!("A job failed for good: {kind}"), error, "today").await?;
        activity::log(conn, tenant, "system", "job.dead", "job", id, &format!("Job {kind} is dead"), serde_json::json!({"error": error})).await?;
    } else {
        let wait = RETRY_MINUTES[(attempts.max(1) as usize - 1).min(RETRY_MINUTES.len() - 1)];
        sqlx::query("UPDATE jobs SET status = 'failed', last_error = $2, run_at = now() + make_interval(mins => $3), locked_by = NULL, locked_until = NULL, updated_at = now() WHERE id = $1::uuid")
            .bind(id).bind(error).bind(wait).execute(&mut *conn).await?;
    }
    Ok(())
}

pub async fn fail(app: &App, job: &Job, error: &str) -> Result<()> {
    let mut tx = app.pool.begin().await?;
    fail_in(&mut tx, &job.id, &job.tenant_id, error).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delays_grow() {
        assert_eq!(RETRY_MINUTES, [1, 5, 15]);
    }
}
