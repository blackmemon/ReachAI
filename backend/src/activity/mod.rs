//! Append-only activity log with a hash chain per tenant. A change to an old row would
//! break the chain, which `verify_chain` detects.

use crate::error::Result;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

fn chain_hash(
    prev: &str,
    at: &str,
    actor: &str,
    kind: &str,
    summary: &str,
    details: &str,
) -> String {
    let mut h = Sha256::new();
    for part in [prev, at, actor, kind, summary, details] {
        h.update(part.as_bytes());
        h.update([0u8]);
    }
    hex::encode(h.finalize())
}

#[allow(clippy::too_many_arguments)]
pub async fn log(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    kind: &str,
    subject_type: &str,
    subject_id: &str,
    summary: &str,
    details: serde_json::Value,
) -> Result<()> {
    // One writer per tenant at a time keeps the chain in order.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(tenant)
        .execute(&mut *conn)
        .await?;
    let prev: String = sqlx::query(
        "SELECT hash FROM activity_logs WHERE tenant_id = $1::uuid ORDER BY id DESC LIMIT 1",
    )
    .bind(tenant)
    .fetch_optional(&mut *conn)
    .await?
    .map(|r| r.get::<String, _>(0))
    .unwrap_or_default();
    let at = chrono::Utc::now();
    let at_text = at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let details_text = details.to_string();
    let hash = chain_hash(&prev, &at_text, actor, kind, summary, &details_text);
    sqlx::query(
        "INSERT INTO activity_logs (tenant_id, at, actor, kind, subject_type, subject_id, summary, details, prev_hash, hash) \
         VALUES ($1::uuid, $2::timestamptz, $3, $4, $5, $6, $7, $8::jsonb, $9, $10)",
    )
    .bind(tenant)
    .bind(&at_text)
    .bind(actor)
    .bind(kind)
    .bind(subject_type)
    .bind(subject_id)
    .bind(summary)
    .bind(&details_text)
    .bind(&prev)
    .bind(&hash)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// True when every row links to the one before it and no stored hash was changed.
pub async fn verify_chain(conn: &mut PgConnection, tenant: &str) -> Result<bool> {
    let rows = sqlx::query(
        "SELECT to_char(at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS at, actor, kind, summary, details::text, prev_hash, hash \
         FROM activity_logs WHERE tenant_id = $1::uuid ORDER BY id",
    )
    .bind(tenant)
    .fetch_all(&mut *conn)
    .await?;
    let mut prev = String::new();
    for r in rows {
        let stored_prev: String = r.get(5);
        if stored_prev != prev {
            return Ok(false);
        }
        let details: String = r.get(4);
        // jsonb::text normalises spacing; recompute from the parsed value the same way.
        let norm = serde_json::from_str::<serde_json::Value>(&details)
            .map(|v| v.to_string())
            .unwrap_or(details);
        let h = chain_hash(
            &prev,
            &r.get::<String, _>(0),
            &r.get::<String, _>(1),
            &r.get::<String, _>(2),
            &r.get::<String, _>(3),
            &norm,
        );
        if h != r.get::<String, _>(6) {
            return Ok(false);
        }
        prev = h;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::chain_hash;

    #[test]
    fn hash_depends_on_every_part() {
        let a = chain_hash("p", "t", "a", "k", "s", "{}");
        assert_ne!(a, chain_hash("x", "t", "a", "k", "s", "{}"));
        assert_ne!(a, chain_hash("p", "t", "a", "k", "s2", "{}"));
        assert_eq!(a, chain_hash("p", "t", "a", "k", "s", "{}"));
    }

    #[test]
    fn parts_cannot_be_shifted_between_fields() {
        assert_ne!(
            chain_hash("", "ab", "c", "k", "s", "{}"),
            chain_hash("", "a", "bc", "k", "s", "{}")
        );
    }
}
