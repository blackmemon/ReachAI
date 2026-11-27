//! Lead assignment: round robin, least open leads, or manual (settings: Lead assignment).

use crate::error::Result;
use crate::settings;
use auth::Actor;
use sqlx::PgConnection;

/// Pick an owner for a new person, or None for manual assignment.
pub async fn pick_owner(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &Actor,
) -> Result<Option<String>> {
    let s = settings::all(conn, tenant).await?;
    let mode = settings::text(&s, "assign_mode").to_string();
    let max_open = settings::int(&s, "assign_max_open");
    if mode == "manual" {
        return Ok(None);
    }
    // Employees and managers can own leads. An employee who adds a person owns it.
    if actor.role == "employee" {
        return Ok(Some(actor.user_id.clone()));
    }
    let order = if mode == "least_open" {
        "open_count ASC, u.id"
    } else {
        // Round robin: the user whose most recent assignment is oldest goes next.
        "last_assigned ASC NULLS FIRST, u.id"
    };
    let sql = format!(
        "SELECT u.id::text FROM users u \
         LEFT JOIN LATERAL (SELECT count(*) AS open_count, max(c.created_at) AS last_assigned FROM contacts c \
            WHERE c.owner_id = u.id AND c.tenant_id = u.tenant_id AND c.stage NOT IN ('won','closed_out')) x ON true \
         WHERE u.tenant_id = $1::uuid AND u.status = 'active' AND u.role IN ('employee','manager') AND x.open_count < $2 \
         ORDER BY {order} LIMIT 1"
    );
    let id: Option<String> = sqlx::query_scalar(&sql)
        .bind(tenant)
        .bind(max_open)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(id)
}
