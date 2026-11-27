//! CSV import for people and companies. A bad row saves nothing (one transaction).
//! A second upload of the same file adds no duplicates. The template is built from the table columns.

use crate::app::{need, App};
use crate::error::{Error, Result};
use crate::{activity, records};
use auth::Actor;
use query_engine::registry::{self, Module};
use shared::ImportReport;
use std::collections::BTreeMap;

pub const MAX_ROWS: usize = 5000;
pub const MAX_BYTES: usize = 2 * 1024 * 1024;

pub fn template(key: &str) -> Result<Vec<u8>> {
    let m = records::find(key)?;
    if !m.can_import {
        return Err(Error::NotFound);
    }
    Ok(query_engine::csv_template(m))
}

/// Map CSV headers to field names, by label or by name, ignoring case.
pub fn map_headers(m: &Module, headers: &[String]) -> std::result::Result<Vec<Option<&'static str>>, String> {
    let mut out = vec![];
    let mut unknown = vec![];
    for h in headers {
        let hl = h.trim().to_lowercase();
        let f = m.fields.iter().find(|f| f.editable() && (f.label.to_lowercase() == hl || f.name == hl));
        match f {
            Some(f) => out.push(Some(f.name)),
            None if hl.is_empty() => out.push(None),
            None => unknown.push(h.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(format!("These columns are not known: {}. Use the template.", unknown.join(", ")));
    }
    Ok(out)
}

pub async fn run(app: &App, actor: &Actor, key: &str, data: &[u8]) -> Result<ImportReport> {
    let m = records::find(key)?;
    need(actor, m.write_permission)?;
    if !m.can_import {
        return Err(Error::NotFound);
    }
    if data.len() > MAX_BYTES {
        return Err(Error::BadRequest("The file is too large (2 MB at most).".into()));
    }
    let mut rd = csv::ReaderBuilder::new().flexible(false).trim(csv::Trim::All).from_reader(data);
    let headers: Vec<String> = rd.headers().map_err(|e| Error::BadRequest(format!("This is not a readable CSV file: {e}")))?.iter().map(String::from).collect();
    let map = map_headers(m, &headers).map_err(Error::BadRequest)?;
    let mut report = ImportReport { added: 0, updated: 0, skipped_duplicates: 0, errors: vec![] };
    let mut rows: Vec<BTreeMap<String, String>> = vec![];
    for (i, rec) in rd.records().enumerate() {
        let line = i + 2;
        if rows.len() >= MAX_ROWS {
            return Err(Error::BadRequest(format!("The file has more than {MAX_ROWS} rows.")));
        }
        match rec {
            Ok(r) => {
                let mut v = BTreeMap::new();
                for (c, cell) in r.iter().enumerate() {
                    if let Some(Some(name)) = map.get(c) {
                        v.insert((*name).to_string(), cell.to_string());
                    }
                }
                match query_engine::validate(m, &v, true) {
                    Ok(_) => rows.push(v),
                    Err(e) => report.errors.push(format!("Row {line}: {}", e.values().cloned().collect::<Vec<_>>().join(" "))),
                }
            }
            Err(e) => report.errors.push(format!("Row {line}: {e}")),
        }
    }
    if !report.errors.is_empty() {
        // A bad row saves nothing.
        return Ok(report);
    }
    let mut tx = app.tx(actor).await?;
    for v in rows {
        if m.key == "people" && person_exists(&mut tx, &actor.tenant_id, &v).await? {
            report.skipped_duplicates += 1;
            continue;
        }
        if m.key == "companies" && company_exists(&mut tx, &actor.tenant_id, &v).await? {
            report.skipped_duplicates += 1;
            continue;
        }
        // A duplicate email is found by the unique index, inside a savepoint so it does not abort the batch.
        let mut sp = sqlx::Connection::begin(&mut *tx).await?;
        match query_engine::create(&mut sp, m, &actor.tenant_id, &v).await {
            Ok(r) => {
                sp.commit().await?;
                report.added += 1;
                post(&mut tx, actor, m, &r.id, &v).await?;
            }
            Err(query_engine::QueryError::Duplicate(_)) => {
                sp.rollback().await?;
                report.skipped_duplicates += 1;
            }
            Err(e) => {
                sp.rollback().await?;
                return Err(e.into());
            }
        }
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "import.csv", m.key, "", &format!("Imported {} rows into {}", report.added, m.title),
        serde_json::json!({"added": report.added, "duplicates": report.skipped_duplicates})).await?;
    tx.commit().await?;
    Ok(report)
}

/// Companies have no unique name, so a second upload is matched by website domain, or by name when there is no website.
async fn company_exists(conn: &mut sqlx::PgConnection, tenant: &str, v: &BTreeMap<String, String>) -> Result<bool> {
    let site = v.get("links").or_else(|| v.get("website")).and_then(|w| w.lines().map(str::trim).find(|line| !line.is_empty())).unwrap_or("");
    if let Some(d) = records::domain_of(site) {
        return Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM companies WHERE tenant_id = $1::uuid AND lower(domain) = $2)").bind(tenant).bind(d).fetch_one(&mut *conn).await?);
    }
    let name = v.get("name").map(|n| n.trim()).unwrap_or("");
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM companies WHERE tenant_id = $1::uuid AND lower(name) = lower($2) AND COALESCE(domain,'') = '')").bind(tenant).bind(name).fetch_one(&mut *conn).await?)
}

async fn person_exists(conn: &mut sqlx::PgConnection, tenant: &str, v: &BTreeMap<String, String>) -> Result<bool> {
    let email = v.get("emails").and_then(|emails| emails.lines().map(str::trim).find(|line| !line.is_empty()))
        .or_else(|| v.get("email").map(String::as_str).filter(|email| !email.trim().is_empty()));
    let Some(email) = email else { return Ok(false) };
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM contacts WHERE tenant_id = $1::uuid AND lower(email) = lower($2))")
        .bind(tenant).bind(email).fetch_one(&mut *conn).await?)
}

async fn post(conn: &mut sqlx::PgConnection, actor: &Actor, m: &Module, id: &str, v: &BTreeMap<String, String>) -> Result<()> {
    // Same hooks as a single save (domain, owner, score).
    let _ = registry::module(m.key);
    records::after_import(conn, actor, m, id, v).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_map_by_label_or_name() {
        let m = registry::module("people").unwrap();
        let h = vec!["First name".to_string(), "email".to_string(), "Job title".to_string()];
        assert_eq!(map_headers(m, &h).unwrap(), vec![Some("first_name"), Some("email"), Some("job_title")]);
    }

    #[test]
    fn unknown_or_computed_columns_are_refused() {
        let m = registry::module("people").unwrap();
        assert!(map_headers(m, &["Score".to_string()]).is_err());
        assert!(map_headers(m, &["password".to_string()]).is_err());
    }
}
