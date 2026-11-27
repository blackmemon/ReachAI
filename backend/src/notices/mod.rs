//! In-app notices (the bell). Repeated events are grouped into one notice.

use crate::error::{Error, Result};
use auth::Actor;
use shared::Notice;
use sqlx::{PgConnection, Row};

/// Add a notice for one user, or for everyone in the company when `user` is None.
pub async fn push(conn: &mut PgConnection, tenant: &str, user: Option<&str>, kind: &str, title: &str, body: &str, link: &str) -> Result<()> {
    // Group: an unread notice of the same kind and title in the last 5 minutes is updated.
    let existing = sqlx::query(
        "SELECT id::text, body FROM notices WHERE tenant_id = $1::uuid AND kind = $2 AND title = $3 AND read_at IS NULL \
         AND COALESCE(user_id::text, '') = COALESCE($4::text, '') AND created_at > now() - interval '5 minutes' LIMIT 1",
    )
    .bind(tenant)
    .bind(kind)
    .bind(title)
    .bind(user)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(r) = existing {
        let old: String = r.get(1);
        let n = old.strip_prefix("Happened ").and_then(|s| s.split(' ').next()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1) + 1;
        sqlx::query("UPDATE notices SET body = $2 WHERE id = $1::uuid")
            .bind(r.get::<String, _>(0))
            .bind(format!("Happened {n} times in the last few minutes."))
            .execute(&mut *conn)
            .await?;
        return Ok(());
    }
    sqlx::query("INSERT INTO notices (tenant_id, user_id, kind, title, body, link) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6)")
        .bind(tenant)
        .bind(user)
        .bind(kind)
        .bind(title)
        .bind(body)
        .bind(link)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn list(conn: &mut PgConnection, actor: &Actor, kind: Option<&str>) -> Result<Vec<Notice>> {
    let rows = sqlx::query(
        "SELECT id::text, kind, title, body, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), read_at IS NULL \
         FROM notices WHERE tenant_id = $1::uuid AND (user_id IS NULL OR user_id = $2::uuid) AND ($3::text IS NULL OR kind = $3) \
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(&actor.tenant_id)
    .bind(&actor.user_id)
    .bind(kind)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.iter().map(|r| Notice { id: r.get(0), kind: r.get(1), title: r.get(2), body: r.get(3), when: r.get(4), unread: r.get(5) }).collect())
}

pub async fn mark_read(conn: &mut PgConnection, actor: &Actor, id: Option<&str>) -> Result<()> {
    if let Some(i) = id {
        if i.len() != 36 {
            return Err(Error::NotFound);
        }
    }
    sqlx::query(
        "UPDATE notices SET read_at = now() WHERE tenant_id = $1::uuid AND read_at IS NULL AND (user_id IS NULL OR user_id = $2::uuid) \
         AND ($3::text IS NULL OR id = $3::uuid)",
    )
    .bind(&actor.tenant_id)
    .bind(&actor.user_id)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
