//! Research: Claude CLI web research, sources policy, candidates that need review,
//! and the sample-check cap. Candidates are never contacts until a person saves them.

use crate::app::{need, App};
use crate::error::{Error, Result};
use crate::{activity, settings};
use auth::Actor;
use sqlx::{PgConnection, Row};

pub mod session;

struct ResearchCompany {
    name: String,
    website: String,
    industry: String,
    city: String,
    source_url: String,
}

fn web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && lower.split_once("://").is_some_and(|(_, rest)| {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            host.contains('.') && !host.contains(char::is_whitespace)
        })
}

async fn save_competitors(
    conn: &mut PgConnection,
    tenant: &str,
    companies: &[ResearchCompany],
) -> Result<(usize, usize)> {
    let mut saved = 0;
    let mut duplicates = 0;
    for company in companies {
        let domain = crate::records::domain_of(&company.website).unwrap_or_default();
        let source_note = format!("Research source: {}", company.source_url);
        if !domain.is_empty() {
            let inserted: bool = sqlx::query_scalar(
                "INSERT INTO companies (tenant_id, name, website, domain, industry, city, is_competitor, source, notes) \
                 VALUES ($1::uuid, $2, $3, $4, $5, $6, true, 'claude_cli', $7) \
                 ON CONFLICT (tenant_id, lower(domain)) WHERE domain IS NOT NULL AND domain <> '' \
                 DO UPDATE SET is_competitor = true, \
                    website = CASE WHEN companies.website = '' THEN EXCLUDED.website ELSE companies.website END, \
                    industry = CASE WHEN companies.industry = '' THEN EXCLUDED.industry ELSE companies.industry END, \
                    city = CASE WHEN companies.city = '' THEN EXCLUDED.city ELSE companies.city END, \
                    source = 'claude_cli', \
                    notes = CASE WHEN position(EXCLUDED.notes in companies.notes) > 0 THEN companies.notes \
                        ELSE concat_ws(E'\\n', NULLIF(companies.notes, ''), EXCLUDED.notes) END, \
                    updated_at = now(), row_version = companies.row_version + 1 \
                 RETURNING (xmax = 0)",
            )
            .bind(tenant)
            .bind(company.name.trim())
            .bind(company.website.trim())
            .bind(&domain)
            .bind(company.industry.trim())
            .bind(company.city.trim())
            .bind(&source_note)
            .fetch_one(&mut *conn)
            .await?;
            if inserted {
                saved += 1;
            } else {
                duplicates += 1;
            }
        } else {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT id::text FROM companies WHERE tenant_id = $1::uuid AND lower(name) = lower($2) LIMIT 1",
            )
            .bind(tenant)
            .bind(company.name.trim())
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(id) = existing {
                sqlx::query(
                    "UPDATE companies SET is_competitor = true, source = 'claude_cli', \
                     notes = CASE WHEN position($3 in notes) > 0 THEN notes \
                         ELSE concat_ws(E'\\n', NULLIF(notes, ''), $3) END, updated_at = now(), \
                     row_version = row_version + 1 WHERE tenant_id = $1::uuid AND id = $2::uuid",
                )
                .bind(tenant)
                .bind(id)
                .bind(&source_note)
                .execute(&mut *conn)
                .await?;
                duplicates += 1;
            } else {
                sqlx::query(
                    "INSERT INTO companies (tenant_id, name, website, industry, city, is_competitor, source, notes) \
                     VALUES ($1::uuid, $2, $3, $4, $5, true, 'claude_cli', $6)",
                )
                .bind(tenant)
                .bind(company.name.trim())
                .bind(company.website.trim())
                .bind(company.industry.trim())
                .bind(company.city.trim())
                .bind(&source_note)
                .execute(&mut *conn)
                .await?;
                saved += 1;
            }
        }
    }
    Ok((saved, duplicates))
}

/// What to do after a source answers. 403 or 429: stop and pause 1 hour, then 24 hours, then 30 days.
pub fn pause_hours(previous_pauses: i32) -> i64 {
    match previous_pauses {
        i if i <= 0 => 1,
        1 => 24,
        _ => 24 * 30,
    }
}

/// 502 to 504 get one retry; 403 and 429 stop the run.
pub fn status_action(code: u16, already_retried: bool) -> &'static str {
    match code {
        200..=299 => "ok",
        403 | 429 => "stop",
        502..=504 if !already_retried => "retry",
        _ => "fail",
    }
}

pub async fn record_block(
    conn: &mut PgConnection,
    tenant: &str,
    source: &str,
    code: u16,
) -> Result<i64> {
    let n: i32 = sqlx::query_scalar(
        "SELECT pause_count FROM sources WHERE id = $1::uuid AND tenant_id = $2::uuid",
    )
    .bind(source)
    .bind(tenant)
    .fetch_one(&mut *conn)
    .await?;
    let hours = pause_hours(n);
    sqlx::query("UPDATE sources SET status = 'blocked', paused_until = now() + make_interval(hours => $3::int), pause_count = pause_count + 1, updated_at = now() WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(source).bind(tenant).bind(hours as i32).execute(&mut *conn).await?;
    activity::log(
        conn,
        tenant,
        "system",
        "source.paused",
        "source",
        source,
        &format!("Source paused for {hours} hours after HTTP {code}"),
        serde_json::json!({"code": code}),
    )
    .await?;
    Ok(hours)
}

#[allow(clippy::too_many_arguments)]
pub async fn add_candidate(
    conn: &mut PgConnection,
    tenant: &str,
    run: Option<&str>,
    name: &str,
    website: &str,
    email: &str,
    industry: &str,
    city: &str,
    source_url: &str,
    licence: &str,
) -> Result<bool> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(Error::BadRequest(
            "A candidate needs a name of 1 to 200 characters.".into(),
        ));
    }
    let s = settings::all(conn, tenant).await?;
    let sample_done = settings::flag(&s, "sample_check_done");
    let cap = settings::int(&s, "sample_cap");
    if !sample_done {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM research_candidates WHERE tenant_id = $1::uuid",
        )
        .bind(tenant)
        .fetch_one(&mut *conn)
        .await?;
        if n >= cap {
            return Err(Error::Conflict(format!(
                "Research is capped at {cap} until the sample check is done."
            )));
        }
    }
    let domain = crate::records::domain_of(website).unwrap_or_default();
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM research_candidates WHERE tenant_id = $1::uuid AND status <> 'rejected' AND (lower(name) = lower($2) OR ($3 <> '' AND lower(website) LIKE '%' || $3 || '%'))) \
         OR ($3 <> '' AND EXISTS (SELECT 1 FROM companies WHERE tenant_id = $1::uuid AND lower(domain) = $3))",
    )
    .bind(tenant).bind(name).bind(&domain).fetch_one(&mut *conn).await?;
    if dup {
        return Ok(false);
    }
    sqlx::query("INSERT INTO research_candidates (tenant_id, run_id, name, website, email, industry, city, source_url, licence) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9)")
        .bind(tenant).bind(run).bind(name).bind(website.trim()).bind(email.trim().to_lowercase()).bind(industry).bind(city).bind(source_url).bind(licence).execute(&mut *conn).await?;
    Ok(true)
}

/// A person approves a candidate: it becomes a company (and a person when it has an email).
pub async fn approve(app: &App, actor: &Actor, id: &str) -> Result<()> {
    need(actor, "record.write")?;
    let mut tx = app.tx(actor).await?;
    let t = &actor.tenant_id;
    let r = sqlx::query("SELECT name, website, email, industry, city, status FROM research_candidates WHERE id = $1::uuid AND tenant_id = $2::uuid FOR UPDATE").bind(id).bind(t).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let status: String = r.get(5);
    if status != "needs_review" {
        return Err(Error::Conflict(
            "This candidate was already handled.".into(),
        ));
    }
    let (name, website, email, industry, city): (String, String, String, String, String) =
        (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4));
    let domain = crate::records::domain_of(&website);
    let company: String = sqlx::query_scalar(
        "INSERT INTO companies (tenant_id, name, website, domain, industry, city) VALUES ($1::uuid, $2, $3, $4, $5, $6) \
         ON CONFLICT (tenant_id, lower(domain)) WHERE domain IS NOT NULL AND domain <> '' DO UPDATE SET updated_at = now() RETURNING id::text",
    )
    .bind(t).bind(&name).bind(&website).bind(&domain).bind(&industry).bind(&city).fetch_one(&mut *tx).await?;
    if !email.is_empty() {
        let owner = crate::assignment::pick_owner(&mut tx, t, actor).await?;
        let parts: Vec<&str> = name.splitn(2, ' ').collect();
        sqlx::query("INSERT INTO contacts (tenant_id, company_id, first_name, last_name, email, owner_id, stage) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::uuid, 'new') ON CONFLICT DO NOTHING")
            .bind(t).bind(&company).bind(parts[0]).bind(parts.get(1).copied().unwrap_or("")).bind(&email).bind(owner).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE research_candidates SET status = 'saved' WHERE id = $1::uuid")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    activity::log(
        &mut tx,
        t,
        &actor.email,
        "candidate.saved",
        "candidate",
        id,
        &format!("Candidate {name} saved"),
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn reject(app: &App, actor: &Actor, id: &str) -> Result<()> {
    need(actor, "record.write")?;
    let mut tx = app.tx(actor).await?;
    let n = sqlx::query("UPDATE research_candidates SET status = 'rejected' WHERE id = $1::uuid AND tenant_id = $2::uuid AND status = 'needs_review'").bind(id).bind(&actor.tenant_id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    activity::log(
        &mut tx,
        &actor.tenant_id,
        &actor.email,
        "candidate.rejected",
        "candidate",
        id,
        "Candidate rejected",
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The owner marks the sample of researched companies. After this the cap is lifted.
pub async fn mark_sample(app: &App, actor: &Actor, good: i64) -> Result<()> {
    need(actor, "settings.admin")?;
    if good < 5 {
        return Err(Error::BadRequest(
            "Mark at least 5 companies as good before the sample check is done.".into(),
        ));
    }
    let mut tx = app.tx(actor).await?;
    settings::set_raw(
        &mut tx,
        &actor.tenant_id,
        Some(&actor.user_id),
        "sample_check_done",
        "true",
    )
    .await?;
    activity::log(
        &mut tx,
        &actor.tenant_id,
        &actor.email,
        "sample.done",
        "settings",
        "",
        "Sample check marked done",
        serde_json::json!({"good": good}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_grow() {
        assert_eq!(
            (
                pause_hours(0),
                pause_hours(1),
                pause_hours(2),
                pause_hours(5)
            ),
            (1, 24, 720, 720)
        );
    }

    #[test]
    fn status_rules() {
        assert_eq!(status_action(200, false), "ok");
        assert_eq!(status_action(429, false), "stop");
        assert_eq!(status_action(403, true), "stop");
        assert_eq!(status_action(503, false), "retry");
        assert_eq!(status_action(503, true), "fail");
    }
}
