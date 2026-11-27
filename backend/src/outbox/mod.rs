//! Outgoing email: merge fields, footer, the send gate and the sender.

use crate::app::{App, OutgoingMail};
use crate::compliance::gate::{self, GateInput};
use crate::error::{Error, Result};
use crate::{activity, notices, settings};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;

/// Replace {fields} in text. Never leaves a placeholder: a missing value is an error that
/// names the field, so the email is not written.
pub fn merge(text: &str, fields: &HashMap<&str, String>) -> std::result::Result<String, Vec<String>> {
    let mut out = String::new();
    let mut missing = vec![];
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match fields.get(name) {
                    Some(v) if !v.trim().is_empty() => out.push_str(v.trim()),
                    _ => missing.push(name.to_string()),
                }
                rest = &after[end + 1..];
            }
            None => {
                missing.push("unclosed {".to_string());
                rest = "";
            }
        }
    }
    out.push_str(rest);
    if missing.is_empty() { Ok(out) } else { missing.sort(); missing.dedup(); Err(missing) }
}

pub fn footer(company: &str, sender: &str, address: &str, site: &str, unsubscribe_url: &str) -> String {
    format!("--\n{sender}, {company}\n{address}, {site}\nTo stop emails, reply \"unsubscribe\" or open {unsubscribe_url}")
}

/// Signed unsubscribe link: nothing is stored, so a link in an old email always works.
/// Token format: tenant id, person id and a signature, joined by dots.
pub fn unsubscribe_url(app: &App, tenant: &str, contact: &str) -> String {
    let data = format!("{tenant}.{contact}");
    let sig = auth::sign_value(&app.cfg.secret_key, &format!("unsub:{data}"));
    format!("{}/unsubscribe/{}.{}", app.cfg.base_url.trim_end_matches('/'), data, sig)
}

/// Check an unsubscribe token. Returns (tenant id, person id).
pub fn parse_unsubscribe_token(app: &App, token: &str) -> Option<(String, String)> {
    let mut p = token.split('.');
    let (t, c, sig) = (p.next()?, p.next()?, p.next()?);
    if p.next().is_some() || t.len() != 36 || c.len() != 36 {
        return None;
    }
    if auth::verify_signed(&app.cfg.secret_key, &format!("unsub:{t}.{c}"), sig) { Some((t.to_string(), c.to_string())) } else { None }
}

/// Someone used the link in an email. Always honoured: the address is suppressed, the person
/// is stopped in every campaign, and the same link can be used again without error.
pub async fn unsubscribe(app: &App, token: &str) -> crate::error::Result<()> {
    let (tenant, contact) = parse_unsubscribe_token(app, token).ok_or(crate::error::Error::NotFound)?;
    let mut tx = app.tx_for_tenant(&tenant).await?;
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM contacts WHERE id = $1::uuid AND tenant_id = $2::uuid").bind(&contact).bind(&tenant).fetch_optional(&mut *tx).await?.flatten();
    let Some(email) = email else { return Err(crate::error::Error::NotFound) };
    sqlx::query("INSERT INTO suppression_list (tenant_id, email, reason, words) VALUES ($1::uuid, lower($2), 'unsubscribed by link', '') ON CONFLICT DO NOTHING").bind(&tenant).bind(&email).execute(&mut *tx).await?;
    sqlx::query("UPDATE contacts SET do_not_contact = true, stage = 'closed_out', updated_at = now() WHERE id = $1::uuid").bind(&contact).execute(&mut *tx).await?;
    crate::campaigns::stop_contact(&mut tx, &tenant, &contact, "unsubscribed").await?;
    crate::activity::log(&mut tx, &tenant, "contact", "unsubscribe.link", "contact", &contact, "Unsubscribed with the link in an email", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

pub struct EmailRow {
    pub id: String,
    pub contact_id: String,
    pub campaign_id: Option<String>,
    pub mailbox_id: Option<String>,
    pub conversation_id: Option<String>,
    pub step_no: i32,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub is_reply: bool,
}

fn hour_in_window(h: i64, from: i64, to: i64) -> bool {
    if from <= to { h >= from && h < to } else { h >= from || h < to }
}

pub async fn karachi_hour(conn: &mut PgConnection) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i32>("SELECT extract(hour FROM (now() AT TIME ZONE 'Asia/Karachi'))::int").fetch_one(&mut *conn).await? as i64)
}

pub(crate) fn valid_email_address(value: &str) -> bool {
    value.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty()
            && !domain.contains('@')
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !value.chars().any(char::is_whitespace)
    })
}

pub async fn compose(app: &App, actor: &auth::Actor, to: &str, subject: &str, body: &str) -> Result<String> {
    crate::app::need(actor, "conversation.reply")?;
    let to = to.trim().to_lowercase();
    let subject = subject.trim();
    let body = body.trim();
    if !valid_email_address(&to) || subject.is_empty() || subject.chars().count() > 200 || body.is_empty() || body.chars().count() > 10_000 {
        return Err(Error::BadRequest("Enter a valid recipient, a subject up to 200 characters, and a message up to 10,000 characters.".into()));
    }

    let mut tx = app.tx(actor).await?;
    let contact = sqlx::query(
        "SELECT id::text FROM contacts WHERE tenant_id = $1::uuid AND lower(email) = $2 \
         AND ($3::uuid IS NULL OR owner_id = $3::uuid)",
    )
    .bind(&actor.tenant_id)
    .bind(&to)
    .bind(crate::records::scope(actor))
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| Error::BadRequest("Compose can send only to an existing contact that you can access.".into()))?;
    let contact_id: String = sqlx::Row::get(&contact, 0);
    let mailbox: Option<String> = sqlx::query_scalar(
        "SELECT id::text FROM mailboxes WHERE tenant_id = $1::uuid AND status = 'active' ORDER BY created_at LIMIT 1",
    )
    .bind(&actor.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let email = EmailRow {
        id: String::new(),
        contact_id: contact_id.clone(),
        campaign_id: None,
        mailbox_id: mailbox.clone(),
        conversation_id: None,
        step_no: 1,
        recipient: to.clone(),
        subject: subject.to_string(),
        body: body.to_string(),
        is_reply: false,
    };
    let (reasons, _) = evaluate(&mut tx, app, &actor.tenant_id, &email).await?;
    let hard: Vec<&str> = reasons.iter().map(String::as_str).filter(|reason| !gate::TEMPORARY.contains(reason)).collect();
    if !hard.is_empty() {
        return Err(Error::BadRequest(format!("This email did not pass the send checks: {}", hard.join(" "))));
    }
    let id: String = sqlx::query_scalar(
        "INSERT INTO emails (tenant_id, contact_id, mailbox_id, direction, step_no, recipient, subject, body, status, written_by, scheduled_at) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, 'out', 1, $4, $5, $6, 'queued', 'employee', now()) RETURNING id::text",
    )
    .bind(&actor.tenant_id)
    .bind(&contact_id)
    .bind(&mailbox)
    .bind(&to)
    .bind(subject)
    .bind(body)
    .fetch_one(&mut *tx)
    .await?;
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "email.composed", "email", &id, "Email queued after send checks", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(id)
}

/// Run the gate for one stored email. The footer is built here, with a fresh unsubscribe link.
pub async fn evaluate(conn: &mut PgConnection, app: &App, tenant: &str, e: &EmailRow) -> Result<(Vec<String>, String)> {
    let s = settings::all(conn, tenant).await?;
    let c = sqlx::query("SELECT email, email_status, do_not_contact, first_name FROM contacts WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(&e.contact_id)
        .bind(tenant)
        .fetch_one(&mut *conn)
        .await?;
    let email: String = c.try_get::<Option<String>, _>(0)?.unwrap_or_default();
    let suppressed: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM suppression_list WHERE tenant_id = $1::uuid AND lower(email) = lower($2))")
        .bind(tenant)
        .bind(&email)
        .fetch_one(&mut *conn)
        .await?;
    let mb = match &e.mailbox_id {
        Some(id) => sqlx::query(
            "SELECT status, daily_limit, CASE WHEN sent_day = current_date THEN sent_today ELSE 0 END FROM mailboxes WHERE id = $1::uuid AND tenant_id = $2::uuid",
        )
        .bind(id)
        .bind(tenant)
        .fetch_optional(&mut *conn)
        .await?,
        None => None,
    };
    let (mb_active, mb_limit, mb_sent) = match &mb {
        Some(r) => (r.get::<String, _>(0) == "active", r.get::<i32, _>(1) as i64, r.get::<i32, _>(2) as i64),
        None => (false, 0, 0),
    };
    let max = settings::int(&s, "max_daily_limit");
    let hour = karachi_hour(conn).await?;
    let url = unsubscribe_url(app, tenant, &e.contact_id);
    let ft = footer(settings::text(&s, "company_name"), settings::text(&s, "sender_name"), settings::text(&s, "company_address"), settings::text(&s, "company_site"), &url);
    let lead_domain = email.split('@').nth(1).unwrap_or("").to_lowercase();
    let booking_host = settings::text(&s, "booking_url").trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").to_lowercase();
    let base_host = app.cfg.base_url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase();
    let first: String = c.get(3);
    let input = GateInput {
        do_not_contact: c.get(2),
        suppressed,
        email_status: &c.get::<String, _>(1),
        subject: &e.subject,
        body: &e.body,
        footer: &ft,
        unsubscribe_url: &url,
        mailbox_active: mb_active,
        mailbox_sent_today: mb_sent,
        mailbox_daily_limit: mb_limit.min(max),
        in_send_window: hour_in_window(hour, settings::int(&s, "send_from_hour"), settings::int(&s, "send_to_hour")),
        paused: settings::flag(&s, "sending_paused"),
        step: if e.is_reply { 0 } else { e.step_no as u32 },
        ban_dashes: settings::flag(&s, "ban_dashes"),
        allowed_domains: vec![settings::text(&s, "company_site").to_lowercase(), booking_host, lead_domain, base_host],
        contact_first_name: &first,
        is_reply: e.is_reply,
    };
    Ok((gate::check(&input), ft))
}

async fn load_email(conn: &mut PgConnection, tenant: &str, id: &str) -> Result<(EmailRow, String)> {
    let r = sqlx::query(
        "SELECT id::text, contact_id::text, campaign_id::text, mailbox_id::text, conversation_id::text, step_no, recipient, subject, body, status, written_by \
         FROM emails WHERE id = $1::uuid AND tenant_id = $2::uuid AND direction = 'out'",
    )
    .bind(id)
    .bind(tenant)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(Error::NotFound)?;
    let conv: Option<String> = r.get(4);
    let status: String = r.get(9);
    Ok((
        EmailRow {
            id: r.get(0), contact_id: r.get(1), campaign_id: r.get(2), mailbox_id: r.get(3), step_no: r.get(5),
            recipient: r.get(6), subject: r.get(7), body: r.get(8), is_reply: conv.is_some() && r.get::<Option<String>, _>(2).is_none(),
            conversation_id: conv,
        },
        status,
    ))
}

async fn block(conn: &mut PgConnection, tenant: &str, e: &EmailRow, reasons: &[String]) -> Result<()> {
    let why = reasons.join(" ");
    sqlx::query("UPDATE emails SET status = 'blocked', blocked_reason = $3, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(&e.id)
        .bind(tenant)
        .bind(&why)
        .execute(&mut *conn)
        .await?;
    if let (Some(c), None) = (&e.campaign_id, &e.conversation_id) {
        sqlx::query("UPDATE campaign_contacts SET status = 'stopped', stop_reason = 'email blocked', next_send_at = NULL WHERE campaign_id = $1::uuid AND contact_id = $2::uuid AND tenant_id = $3::uuid")
            .bind(c)
            .bind(&e.contact_id)
            .bind(tenant)
            .execute(&mut *conn)
            .await?;
    }
    notices::push(conn, tenant, None, "blocked", "An email was blocked", &why, "emails").await?;
    activity::log(conn, tenant, "system", "email.blocked", "email", &e.id, "Email blocked by a safety check", serde_json::json!({"reasons": reasons})).await?;
    Ok(())
}

/// Send one due email, if any. Returns true if one was handled (sent, blocked or waiting).
pub async fn send_next(app: &App, tenant: &str) -> Result<bool> {
    let mut tx = app.tx_for_tenant(tenant).await?;
    let id: Option<String> = sqlx::query_scalar(
        "SELECT id::text FROM emails WHERE tenant_id = $1::uuid AND direction = 'out' AND status = 'queued' AND scheduled_at <= now() AND is_test = false \
         ORDER BY scheduled_at FOR UPDATE SKIP LOCKED LIMIT 1",
    )
    .bind(tenant)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = id else {
        tx.commit().await?;
        return Ok(false);
    };
    let (e, _) = load_email(&mut tx, tenant, &id).await?;
    let (reasons, ft) = evaluate(&mut tx, app, tenant, &e).await?;
    if !reasons.is_empty() {
        if gate::only_temporary(&reasons) {
            // Paused, outside the window or at the daily limit: the email waits.
            // Push its time forward a little so one waiting email does not hold the queue.
            sqlx::query("UPDATE emails SET scheduled_at = now() + interval '5 minutes' WHERE id = $1::uuid").bind(&id).execute(&mut *tx).await?;
        } else {
            block(&mut tx, tenant, &e, &reasons).await?;
        }
        tx.commit().await?;
        return Ok(true);
    }
    let from: String = sqlx::query_scalar("SELECT email FROM mailboxes WHERE id = $1::uuid").bind(e.mailbox_id.as_deref()).fetch_one(&mut *tx).await?;
    let message_id = format!("<{}@{}>", uuid::Uuid::new_v4(), from.split('@').nth(1).unwrap_or("local"));
    let unsub = ft.rsplit(' ').next().unwrap_or("").to_string();
    let mail = OutgoingMail {
        from: from.clone(), to: e.recipient.clone(), subject: e.subject.clone(), body: format!("{}\n\n{}", e.body, ft),
        message_id: message_id.clone(), list_unsubscribe: unsub,
    };
    match app.transport.send(&mail).await {
        Ok(()) => {
            sqlx::query("UPDATE emails SET status = 'sent', sent_at = now(), sender = $3, message_id = $4, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid")
                .bind(&id).bind(tenant).bind(&from).bind(&message_id).execute(&mut *tx).await?;
            sqlx::query("UPDATE mailboxes SET sent_today = CASE WHEN sent_day = current_date THEN sent_today + 1 ELSE 1 END, sent_day = current_date WHERE id = $1::uuid").bind(e.mailbox_id.as_deref()).execute(&mut *tx).await?;
            if !e.is_reply {
                sqlx::query("UPDATE contacts SET stage = 'contacted', updated_at = now() WHERE id = $1::uuid AND tenant_id = $2::uuid AND stage IN ('new','to_write')").bind(&e.contact_id).bind(tenant).execute(&mut *tx).await?;
            }
            activity::log(&mut tx, tenant, "system", "email.sent", "email", &id, "Email sent", serde_json::json!({"step": e.step_no})).await?;
        }
        Err(err) => {
            sqlx::query("UPDATE emails SET status = 'failed', blocked_reason = $3, updated_at = now() WHERE id = $1::uuid AND tenant_id = $2::uuid").bind(&id).bind(tenant).bind(&err).execute(&mut *tx).await?;
            notices::push(&mut tx, tenant, None, "failed", "An email could not be sent", &err, "emails").await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

/// A person changes the text of a blocked, waiting or queued email. It is checked again.
pub async fn edit(app: &App, actor: &auth::Actor, id: &str, subject: &str, body: &str) -> Result<Vec<String>> {
    crate::app::need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    let (mut e, status) = load_email(&mut tx, &actor.tenant_id, id).await?;
    if !matches!(status.as_str(), "blocked" | "waiting_approval" | "queued") {
        return Err(Error::Conflict("This email was already sent.".into()));
    }
    e.subject = subject.trim().to_string();
    e.body = body.trim().to_string();
    let (reasons, _) = evaluate(&mut tx, app, &actor.tenant_id, &e).await?;
    let hard: Vec<String> = reasons.iter().filter(|r| !gate::TEMPORARY.contains(&r.as_str())).cloned().collect();
    let next = if !hard.is_empty() { "blocked" } else { "queued" };
    sqlx::query("UPDATE emails SET subject = $3, body = $4, status = $5, blocked_reason = $6, written_by = 'employee', scheduled_at = now(), updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(id).bind(&actor.tenant_id).bind(&e.subject).bind(&e.body).bind(next).bind(hard.join(" ")).execute(&mut *tx).await?;
    if next == "queued" {
        if let (Some(c), None) = (&e.campaign_id, &e.conversation_id) {
            sqlx::query("UPDATE campaign_contacts SET status = 'active', stop_reason = '' WHERE campaign_id = $1::uuid AND contact_id = $2::uuid AND status = 'stopped' AND stop_reason = 'email blocked'")
                .bind(c).bind(&e.contact_id).execute(&mut *tx).await?;
        }
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "email.edited", "email", id, "Email text changed", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(hard)
}

/// Approval mode: a person lets a waiting email go. The gate still runs before sending.
pub async fn approve(app: &App, actor: &auth::Actor, id: &str) -> Result<()> {
    crate::app::need(actor, "campaign.run")?;
    let mut tx = app.tx(actor).await?;
    let n = sqlx::query("UPDATE emails SET status = 'queued', scheduled_at = now(), updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid AND status = 'waiting_approval'")
        .bind(id).bind(&actor.tenant_id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "email.approved", "email", id, "Email approved", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("first_name", "Sana".to_string());
        m.insert("company", "Rose Mart".to_string());
        m.insert("personal_line", "".to_string());
        m
    }

    #[test]
    fn compose_address_validation_rejects_malformed_values() {
        assert!(valid_email_address("person@example.com"));
        assert!(!valid_email_address("person@localhost"));
        assert!(!valid_email_address("person @example.com"));
        assert!(!valid_email_address("@example.com"));
    }

    #[test]
    fn merge_fills_fields() {
        assert_eq!(merge("Hi {first_name}, {company} looks busy.", &f()).unwrap(), "Hi Sana, Rose Mart looks busy.");
    }

    #[test]
    fn merge_never_leaves_a_placeholder() {
        assert_eq!(merge("{personal_line} and {nothing}", &f()).unwrap_err(), vec!["nothing".to_string(), "personal_line".to_string()]);
        assert!(merge("broken {first_name", &f()).is_err());
    }

    #[test]
    fn footer_has_unsubscribe_line_and_address() {
        let t = footer("Quickanalytics", "Rahat Hussain", "Karachi, Sindh, Pakistan", "quickanalytics.ai", "https://x/u/1");
        assert!(t.contains("unsubscribe") && t.contains("https://x/u/1") && t.contains("Karachi"));
    }

    #[test]
    fn send_window_wraps_midnight() {
        assert!(hour_in_window(10, 9, 17));
        assert!(!hour_in_window(17, 9, 17));
        assert!(hour_in_window(23, 22, 6));
        assert!(hour_in_window(2, 22, 6));
        assert!(!hour_in_window(12, 22, 6));
    }
}
