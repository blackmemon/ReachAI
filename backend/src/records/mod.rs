//! Generic record service on top of the Universal Data Query Engine. Permission checks,
//! owner scope for employees, small business hooks per module, and the activity log.

use crate::app::{need, App};
use crate::error::{Error, Result};
use crate::{activity, notices, scoring, settings};
use auth::Actor;
use query_engine::registry::{self, Module};
use shared::{ModuleInfo, QueryRequest, QueryResult, Record, SaveRecord, SaveResult};
use sqlx::PgConnection;
use std::collections::BTreeMap;

pub fn find(key: &str) -> Result<&'static Module> {
    registry::module(key).ok_or(Error::NotFound)
}

pub fn infos() -> Vec<ModuleInfo> {
    registry::MODULES.iter().map(registry::info).collect()
}

pub fn info(key: &str) -> Result<ModuleInfo> {
    Ok(registry::info(find(key)?))
}

/// An employee sees only the records they own (role-based-access.md of the leads skill).
pub fn scope(actor: &Actor) -> Option<&str> {
    if actor.role == "employee" { Some(actor.user_id.as_str()) } else { None }
}

/// Domain of a website address, lower case, no `www.`, no path or port.
pub fn domain_of(website: &str) -> Option<String> {
    let w = website.trim().to_lowercase();
    let w = w.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    let host = w.split(['/', '?', '#']).next().unwrap_or("").split(':').next().unwrap_or("");
    if host.contains('.') && !host.contains(char::is_whitespace) { Some(host.to_string()) } else { None }
}

/// Webhook targets must be https and must not point at this machine or a private network.
pub fn webhook_url_ok(url: &str) -> bool {
    let l = url.trim().to_lowercase();
    let Some(rest) = l.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
    if host.is_empty() || host == "localhost" || host.ends_with(".local") || host.ends_with(".internal") || !host.contains('.') {
        return false;
    }
    if host.chars().all(|c| c.is_ascii_digit() || c == '.') {
        let p: Vec<u32> = host.split('.').filter_map(|x| x.parse().ok()).collect();
        if p.len() != 4 {
            return false;
        }
        let private = p[0] == 10 || p[0] == 127 || p[0] == 0 || (p[0] == 169 && p[1] == 254) || (p[0] == 172 && (16..=31).contains(&p[1])) || (p[0] == 192 && p[1] == 168);
        return !private;
    }
    true
}

pub async fn list(app: &App, actor: &Actor, key: &str, req: &QueryRequest) -> Result<QueryResult> {
    need(actor, "record.read")?;
    let m = find(key)?;
    if key == "users" || key == "activity" {
        need(actor, if key == "users" { "user.manage" } else { "settings.admin" })?;
    }
    let mut tx = app.tx(actor).await?;
    let r = query_engine::list(&mut tx, m, &actor.tenant_id, scope(actor), req).await?;
    tx.commit().await?;
    Ok(r)
}

pub async fn get(app: &App, actor: &Actor, key: &str, id: &str) -> Result<(Record, i64)> {
    need(actor, "record.read")?;
    let m = find(key)?;
    let mut tx = app.tx(actor).await?;
    let r = query_engine::get(&mut tx, m, &actor.tenant_id, scope(actor), id).await?;
    if key == "emails" {
        sqlx::query(
            "INSERT INTO email_user_state (tenant_id, user_id, email_id) \
             SELECT tenant_id, $3::uuid, id FROM emails \
             WHERE tenant_id = $1::uuid AND id = $2::uuid AND direction = 'in' \
             ON CONFLICT (user_id, email_id) DO UPDATE SET read_at = now()",
        )
        .bind(&actor.tenant_id)
        .bind(id)
        .bind(&actor.user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(r)
}

async fn pre_save(conn: &mut PgConnection, actor: &Actor, m: &Module, values: &BTreeMap<String, String>) -> Result<()> {
    let mut errors = BTreeMap::new();
    match m.key {
        "webhooks" => {
            if let Some(u) = values.get("url") {
                if !webhook_url_ok(u) {
                    errors.insert("url".to_string(), "Use an https address on the public internet.".to_string());
                }
            }
        }
        "mailboxes" => {
            if let Some(v) = values.get("daily_limit") {
                let max = settings::int(&settings::all(conn, &actor.tenant_id).await?, "max_daily_limit");
                match v.trim().parse::<i64>() {
                    Ok(n) if (1..=max).contains(&n) => {}
                    _ => {
                        errors.insert("daily_limit".to_string(), format!("Use a number from 1 to {max}."));
                    }
                }
            }
        }
        _ => {}
    }
    if errors.is_empty() { Ok(()) } else { Err(Error::Fields(errors)) }
}

async fn post_save(conn: &mut PgConnection, actor: &Actor, m: &Module, id: &str, created: bool, values: &BTreeMap<String, String>) -> Result<()> {
    let t = &actor.tenant_id;
    sync_people_company_fields(conn, m.key, id, values).await?;
    match m.key {
        "companies" => {
            if values.contains_key("website") || values.contains_key("links") {
                let w: String = sqlx::query_scalar("SELECT website FROM companies WHERE id = $1::uuid").bind(id).fetch_one(&mut *conn).await?;
                let d = domain_of(&w);
                sqlx::query("UPDATE companies SET domain = $2 WHERE id = $1::uuid").bind(id).bind(d).execute(&mut *conn).await.map_err(|e| {
                    query_engine::QueryError::from(e)
                })?;
            }
        }
        "people" => {
            if created {
                if let Some(owner) = crate::assignment::pick_owner(conn, t, actor).await? {
                    sqlx::query("UPDATE contacts SET owner_id = $2::uuid WHERE id = $1::uuid AND owner_id IS NULL").bind(id).bind(owner).execute(&mut *conn).await?;
                }
            }
            scoring::rescore_contact(conn, t, id).await?;
        }
        "sources" => {
            // A protected or blocked source pauses for 30 days and tells the admin and manager.
            if let Some(st) = values.get("status") {
                if st == "protected" || st == "blocked" {
                    sqlx::query("UPDATE sources SET paused_until = now() + interval '30 days' WHERE id = $1::uuid").bind(id).execute(&mut *conn).await?;
                    notices::push(conn, t, None, "source", "A source is protected or blocked", "The source is paused for 30 days.", "research").await?;
                } else if st == "open" {
                    sqlx::query("UPDATE sources SET paused_until = NULL WHERE id = $1::uuid").bind(id).execute(&mut *conn).await?;
                }
            }
        }
        _ => {}
    }
    let kind = format!("{}.{}", m.key, if created { "created" } else { "changed" });
    let fields: Vec<&String> = values.keys().collect();
    activity::log(
        conn, t, &actor.email, &kind, m.key, id,
        &format!("{} {}", if created { "Added" } else { "Changed" }, m.singular),
        serde_json::json!({"fields": fields}),
    )
    .await?;
    Ok(())
}

/// Hooks for a row added by a CSV import (domain, owner, score) without a log line per row.
pub async fn after_import(conn: &mut PgConnection, actor: &Actor, m: &Module, id: &str, values: &BTreeMap<String, String>) -> Result<()> {
    sync_people_company_fields(conn, m.key, id, values).await?;
    match m.key {
        "companies" => {
            if values.contains_key("website") || values.contains_key("links") {
                let w: String = sqlx::query_scalar("SELECT website FROM companies WHERE id = $1::uuid").bind(id).fetch_one(&mut *conn).await?;
                sqlx::query("UPDATE companies SET domain = $2 WHERE id = $1::uuid").bind(id).bind(domain_of(&w)).execute(&mut *conn).await?;
            }
        }
        "people" => {
            if let Some(owner) = crate::assignment::pick_owner(conn, &actor.tenant_id, actor).await? {
                sqlx::query("UPDATE contacts SET owner_id = $2::uuid WHERE id = $1::uuid AND owner_id IS NULL").bind(id).bind(owner).execute(&mut *conn).await?;
            }
            scoring::rescore_contact(conn, &actor.tenant_id, id).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn sync_people_company_fields(conn: &mut PgConnection, key: &str, id: &str, values: &BTreeMap<String, String>) -> Result<()> {
    match key {
        "people" => {
            if let Some(name) = values.get("name") {
                let mut parts = name.splitn(2, char::is_whitespace);
                let first_name = parts.next().unwrap_or("");
                let last_name = parts.next().unwrap_or("").trim();
                sqlx::query("UPDATE contacts SET first_name = $2, last_name = $3 WHERE id = $1::uuid")
                    .bind(id).bind(first_name).bind(last_name).execute(&mut *conn).await?;
            } else if values.contains_key("first_name") || values.contains_key("last_name") {
                sqlx::query("UPDATE contacts SET display_name = full_name WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
            if values.contains_key("emails") {
                sqlx::query("UPDATE contacts SET email = NULLIF(split_part(emails, E'\\n', 1), '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            } else if values.contains_key("email") {
                sqlx::query("UPDATE contacts SET emails = COALESCE(email, '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
            if values.contains_key("phone_numbers") {
                sqlx::query("UPDATE contacts SET phone = split_part(phone_numbers, E'\\n', 1) WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            } else if values.contains_key("phone") {
                sqlx::query("UPDATE contacts SET phone_numbers = COALESCE(phone, '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
        }
        "companies" => {
            if values.contains_key("phone_numbers") {
                sqlx::query("UPDATE companies SET phone = split_part(phone_numbers, E'\\n', 1) WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            } else if values.contains_key("phone") {
                sqlx::query("UPDATE companies SET phone_numbers = COALESCE(phone, '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
            if values.contains_key("links") {
                sqlx::query("UPDATE companies SET website = split_part(links, E'\\n', 1) WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            } else if values.contains_key("website") {
                sqlx::query("UPDATE companies SET links = COALESCE(website, '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
            if values.contains_key("industries") {
                sqlx::query("UPDATE companies SET industry = split_part(industries, E'\\n', 1) WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            } else if values.contains_key("industry") {
                sqlx::query("UPDATE companies SET industries = COALESCE(industry, '') WHERE id = $1::uuid")
                    .bind(id).execute(&mut *conn).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub async fn create(app: &App, actor: &Actor, key: &str, values: &BTreeMap<String, String>) -> Result<SaveResult> {
    let m = find(key)?;
    need(actor, m.write_permission)?;
    let mut tx = app.tx(actor).await?;
    pre_save(&mut tx, actor, m, values).await?;
    let r = query_engine::create(&mut tx, m, &actor.tenant_id, values).await?;
    post_save(&mut tx, actor, m, &r.id, true, values).await?;
    tx.commit().await?;
    Ok(r)
}

pub async fn update(app: &App, actor: &Actor, key: &str, id: &str, save: &SaveRecord) -> Result<SaveResult> {
    let m = find(key)?;
    need(actor, m.write_permission)?;
    let mut tx = app.tx(actor).await?;
    pre_save(&mut tx, actor, m, &save.values).await?;
    let r = query_engine::update(&mut tx, m, &actor.tenant_id, scope(actor), id, &save.values, save.row_version).await?;
    post_save(&mut tx, actor, m, &r.id, false, &save.values).await?;
    tx.commit().await?;
    Ok(r)
}

pub async fn remove_competitor(app: &App, actor: &Actor, id: &str) -> Result<()> {
    need(actor, "record.write")?;
    let mut tx = app.tx(actor).await?;
    let updated = sqlx::query(
        "UPDATE companies SET is_competitor = false, updated_at = now(), row_version = row_version + 1 \
         WHERE tenant_id = $1::uuid AND id = $2::uuid AND is_competitor",
    )
    .bind(&actor.tenant_id)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(Error::NotFound);
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "competitor.removed", "company", id, "Company removed from competitors", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn export(app: &App, actor: &Actor, key: &str, req: &QueryRequest) -> Result<Vec<u8>> {
    need(actor, "record.export")?;
    let m = find(key)?;
    if !m.can_export {
        return Err(Error::NotFound);
    }
    let mut tx = app.tx(actor).await?;
    let bytes = query_engine::export_csv(&mut tx, m, &actor.tenant_id, scope(actor), req).await?;
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "export.csv", m.key, "", &format!("Exported {} to CSV", m.title), serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_from_website() {
        assert_eq!(domain_of("https://www.Example.pk/about?x=1").as_deref(), Some("example.pk"));
        assert_eq!(domain_of("http://shop.example.pk:8080/").as_deref(), Some("shop.example.pk"));
        assert_eq!(domain_of("not a site"), None);
        assert_eq!(domain_of(""), None);
    }

    #[test]
    fn webhook_urls_must_be_public_https() {
        assert!(webhook_url_ok("https://hooks.example.com/abc"));
        assert!(!webhook_url_ok("http://hooks.example.com/abc"));
        assert!(!webhook_url_ok("https://localhost/x"));
        assert!(!webhook_url_ok("https://127.0.0.1/x"));
        assert!(!webhook_url_ok("https://10.0.0.5/x"));
        assert!(!webhook_url_ok("https://192.168.1.2/x"));
        assert!(!webhook_url_ok("https://169.254.169.254/latest"));
        assert!(!webhook_url_ok("https://intranet/x"));
        assert!(!webhook_url_ok("https://user@internal.local/x"));
        assert!(webhook_url_ok("https://8.8.8.8/x"));
    }

    #[test]
    fn only_employees_are_scoped() {
        let mk = |r: &str| Actor { user_id: "u".into(), tenant_id: "t".into(), role: r.into(), name: "n".into(), email: "e".into() };
        assert!(scope(&mk("employee")).is_some());
        assert!(scope(&mk("manager")).is_none());
        assert!(scope(&mk("admin")).is_none());
    }
}
