use crate::app::{need, App};
use crate::error::{Error, Result};
use auth::Actor;
use sqlx::PgConnection;

const DEFAULTS: [&str; 4] = ["Personal", "Work", "Important", "Updates"];

fn clean_name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(Error::BadRequest("Label names must be 1 to 40 characters and cannot contain line breaks.".into()));
    }
    Ok(name.to_string())
}

fn validate_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "email" => Ok("email"),
        "conversation" => Ok("conversation"),
        _ => Err(Error::NotFound),
    }
}

async fn ensure_defaults(conn: &mut PgConnection, tenant: &str, kind: &str) -> Result<()> {
    for name in DEFAULTS {
        sqlx::query("INSERT INTO labels (tenant_id, kind, name) VALUES ($1::uuid, $2, $3) ON CONFLICT DO NOTHING")
            .bind(tenant)
            .bind(kind)
            .bind(name)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn names(conn: &mut PgConnection, tenant: &str, kind: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT name FROM labels WHERE tenant_id = $1::uuid AND kind = $2 ORDER BY lower(name), name")
        .bind(tenant)
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

pub async fn list(app: &App, actor: &Actor, kind: &str) -> Result<Vec<String>> {
    need(actor, "record.read")?;
    let kind = validate_kind(kind)?;
    let mut tx = app.tx(actor).await?;
    ensure_defaults(&mut tx, &actor.tenant_id, kind).await?;
    let result = names(&mut tx, &actor.tenant_id, kind).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn create(app: &App, actor: &Actor, kind: &str, name: &str) -> Result<Vec<String>> {
    need(actor, "conversation.reply")?;
    let kind = validate_kind(kind)?;
    let name = clean_name(name)?;
    let mut tx = app.tx(actor).await?;
    ensure_defaults(&mut tx, &actor.tenant_id, kind).await?;
    sqlx::query("INSERT INTO labels (tenant_id, kind, name) VALUES ($1::uuid, $2, $3) ON CONFLICT DO NOTHING")
        .bind(&actor.tenant_id)
        .bind(kind)
        .bind(name)
        .execute(&mut *tx)
        .await?;
    let result = names(&mut tx, &actor.tenant_id, kind).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn replace(
    app: &App,
    actor: &Actor,
    kind: &str,
    record_id: &str,
    requested: &[String],
) -> Result<Vec<String>> {
    need(actor, "conversation.reply")?;
    let kind = validate_kind(kind)?;
    let mut labels = Vec::new();
    for value in requested {
        let name = clean_name(value)?;
        if !labels.iter().any(|existing: &String| existing.eq_ignore_ascii_case(name.as_str())) {
            labels.push(name);
        }
    }
    if labels.len() > 12 {
        return Err(Error::BadRequest("A record can have at most 12 labels.".into()));
    }

    let mut tx = app.tx(actor).await?;
    ensure_defaults(&mut tx, &actor.tenant_id, kind).await?;
    let allowed: bool = match kind {
        "email" => sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM emails WHERE tenant_id = $1::uuid AND id = $2::uuid)")
            .bind(&actor.tenant_id)
            .bind(record_id)
            .fetch_one(&mut *tx)
            .await?,
        _ => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM conversations WHERE tenant_id = $1::uuid AND id = $2::uuid AND ($3::uuid IS NULL OR assigned_to = $3::uuid))",
        )
        .bind(&actor.tenant_id)
        .bind(record_id)
        .bind(crate::records::scope(actor))
        .fetch_one(&mut *tx)
        .await?,
    };
    if !allowed {
        return Err(Error::NotFound);
    }

    match kind {
        "email" => {
            sqlx::query("DELETE FROM email_labels WHERE tenant_id = $1::uuid AND email_id = $2::uuid")
                .bind(&actor.tenant_id)
                .bind(record_id)
                .execute(&mut *tx)
                .await?;
        }
        _ => {
            sqlx::query("DELETE FROM conversation_labels WHERE tenant_id = $1::uuid AND conversation_id = $2::uuid")
                .bind(&actor.tenant_id)
                .bind(record_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for name in &labels {
        let id: Option<String> = sqlx::query_scalar("SELECT id::text FROM labels WHERE tenant_id = $1::uuid AND kind = $2 AND lower(name) = lower($3)")
            .bind(&actor.tenant_id)
            .bind(kind)
            .bind(name)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(id) = id else {
            return Err(Error::BadRequest(format!("The label '{name}' does not exist.")));
        };
        match kind {
            "email" => {
                sqlx::query("INSERT INTO email_labels (tenant_id, email_id, label_id) VALUES ($1::uuid, $2::uuid, $3::uuid)")
                    .bind(&actor.tenant_id)
                    .bind(record_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            _ => {
                sqlx::query("INSERT INTO conversation_labels (tenant_id, conversation_id, label_id) VALUES ($1::uuid, $2::uuid, $3::uuid)")
                    .bind(&actor.tenant_id)
                    .bind(record_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    let result = names(&mut tx, &actor.tenant_id, kind).await?;
    tx.commit().await?;
    Ok(result.into_iter().filter(|name| labels.iter().any(|v| v.eq_ignore_ascii_case(name))).collect())
}

pub async fn delete(app: &App, actor: &Actor, kind: &str, value: &str) -> Result<Vec<String>> {
    need(actor, "conversation.reply")?;
    let kind = validate_kind(kind)?;
    let name = clean_name(value)?;
    if DEFAULTS.iter().any(|default| default.eq_ignore_ascii_case(&name)) {
        return Err(Error::BadRequest("The default labels cannot be deleted.".into()));
    }
    let mut tx = app.tx(actor).await?;
    let deleted = sqlx::query("DELETE FROM labels WHERE tenant_id = $1::uuid AND kind = $2 AND lower(name) = lower($3)")
        .bind(&actor.tenant_id)
        .bind(kind)
        .bind(name)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(Error::NotFound);
    }
    let result = names(&mut tx, &actor.tenant_id, kind).await?;
    tx.commit().await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_names_are_single_line_and_bounded() {
        assert_eq!(clean_name("  My label  ").unwrap(), "My label");
        assert!(clean_name("").is_err());
        assert!(clean_name("two\nlines").is_err());
        assert!(clean_name(&"x".repeat(41)).is_err());
    }
}
