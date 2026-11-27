//! Connection pool and migrations. Row-level security: every request runs in a
//! transaction that first sets `app.tenant_id` (see `scoped`).

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{PgConnection, Postgres, Transaction};

pub type Pool = PgPool;
pub type Tx = Transaction<'static, Postgres>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(url: &str, max: u32) -> Result<Pool, sqlx::Error> {
    PgPoolOptions::new().max_connections(max).connect(url).await
}

/// Run migrations with the owner role (not the runtime role).
pub async fn migrate(owner_url: &str) -> Result<(), String> {
    let pool = connect(owner_url, 2).await.map_err(|e| e.to_string())?;
    MIGRATOR.run(&pool).await.map_err(|e| e.to_string())?;
    // The runtime roles need rights on the tables.
    for role in ["leads_app", "leads_worker"] {
        let exists: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM pg_roles WHERE rolname = $1")
            .bind(role)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?;
        if exists.is_some() {
            for stmt in [
                format!("GRANT USAGE ON SCHEMA public TO {role}"),
                format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
                format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
                // Append-only: the activity log is never updated or deleted.
                format!("REVOKE UPDATE, DELETE ON activity_logs FROM {role}"),
            ] {
                sqlx::query(&stmt).execute(&pool).await.map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Start a transaction for one tenant. The tenant id comes only from the session.
pub async fn scoped(pool: &Pool, tenant_id: &str) -> Result<Tx, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

pub async fn ping(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(conn).await.map(|_| ())
}
