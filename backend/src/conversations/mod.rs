//! Replies: classify, stop sequences, unsubscribe handling, the employee timer,
//! hand over, and employee or AI replies. The AI never sends without the gate.

use crate::app::{need, App};
use crate::campaigns;
use crate::compliance::draft::{self, Rules};
use crate::error::{Error, Result};
use crate::{activity, notices, settings};
use auth::Actor;
use sqlx::{PgConnection, Row};

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub label: &'static str,
    pub urgent: bool,
}

const UNSUB: [&str; 8] = ["unsubscribe", "stop emailing", "remove me", "do not email", "don't email", "stop sending", "opt out", "take me off"];
const OOO: [&str; 5] = ["out of office", "automatic reply", "auto-reply", "autoreply", "away from the office"];
const DECLINE: [&str; 7] = ["not interested", "not now", "not a priority", "maybe later", "next quarter", "no thanks", "no thank you"];
const PRICE: [&str; 7] = ["price", "pricing", "how much", "cost", "quote", "fee", "budget"];
const INTEREST: [&str; 10] = ["interested", "tell me more", "sounds good", "let's talk", "lets talk", "call me", "schedule", "yes please", "send details", "happy to"];
const URGENT: [&str; 4] = ["urgent", "asap", "immediately", "today"];

/// Rules classifier. Order matters: bounce, out of office, unsubscribe, decline, price, interest.
pub fn classify(from: &str, subject: &str, body: &str) -> Class {
    let f = from.to_lowercase();
    let text = format!("{} {}", subject.to_lowercase(), body.to_lowercase());
    let urgent = URGENT.iter().any(|w| text.contains(w));
    let label = if f.contains("mailer-daemon") || f.contains("postmaster") || text.contains("delivery status notification") || text.contains("undeliverable") || text.contains("delivery failure") {
        "bounce"
    } else if OOO.iter().any(|w| text.contains(w)) {
        "out_of_office"
    } else if UNSUB.iter().any(|w| text.contains(w)) {
        "unsubscribe"
    } else if DECLINE.iter().any(|w| text.contains(w)) {
        "not_now"
    } else if PRICE.iter().any(|w| text.contains(w)) {
        "price"
    } else if INTEREST.iter().any(|w| text.contains(w)) {
        "interested"
    } else if text.contains('?') {
        "question"
    } else {
        "other"
    };
    Class { label, urgent }
}

#[derive(Debug, PartialEq)]
pub enum Received {
    Unknown,
    Duplicate,
    Saved { conversation_id: String, label: String },
}

/// Save an incoming email and apply the rules. Used by the inbox reader and by
/// the "record a reply" action. Safe to run twice for the same message id.
pub async fn receive(conn: &mut PgConnection, tenant: &str, from: &str, subject: &str, body: &str, message_id: Option<&str>) -> Result<Received> {
    let from_addr = from.trim().to_lowercase();
    let c = sqlx::query("SELECT id::text, stage, owner_id::text FROM contacts WHERE tenant_id = $1::uuid AND lower(email) = $2")
        .bind(tenant).bind(&from_addr).fetch_optional(&mut *conn).await?;
    let Some(c) = c else {
        activity::log(conn, tenant, "system", "reply.unknown_sender", "email", "", "A message arrived from an address that is not a contact", serde_json::json!({})).await?;
        return Ok(Received::Unknown);
    };
    let (contact, stage, owner): (String, String, Option<String>) = (c.get(0), c.get(1), c.get(2));
    if let Some(m) = message_id {
        let dup: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM emails WHERE tenant_id = $1::uuid AND message_id = $2)").bind(tenant).bind(m).fetch_one(&mut *conn).await?;
        if dup {
            return Ok(Received::Duplicate);
        }
    }
    let s = settings::all(conn, tenant).await?;
    let class = classify(&from_addr, subject, body);
    let minutes = if class.urgent { settings::int(&s, "urgent_timer_minutes") } else { settings::int(&s, "employee_timer_minutes") };

    let normalized_subject = subject.trim_start_matches("Re: ").trim();
    let conv: Option<String> = sqlx::query_scalar(
        "SELECT cv.id::text FROM conversations cv \
         JOIN conversation_participants cp ON cp.tenant_id = cv.tenant_id AND cp.conversation_id = cv.id \
         WHERE cv.tenant_id = $1::uuid AND cp.contact_id = $2::uuid AND cv.status <> 'closed' \
           AND lower(cv.subject) = lower($3) ORDER BY cv.last_message_at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(&contact)
    .bind(normalized_subject)
    .fetch_optional(&mut *conn)
    .await?;
    let conv = match conv {
        Some(id) => id,
        None => sqlx::query_scalar("INSERT INTO conversations (tenant_id, contact_id, subject, assigned_to) VALUES ($1::uuid, $2::uuid, $3, $4::uuid) RETURNING id::text")
            .bind(tenant).bind(&contact).bind(normalized_subject).bind(&owner).fetch_one(&mut *conn).await?,
    };
    sqlx::query("INSERT INTO conversation_participants (tenant_id, conversation_id, contact_id) VALUES ($1::uuid, $2::uuid, $3::uuid) ON CONFLICT DO NOTHING")
        .bind(tenant).bind(&conv).bind(&contact).execute(&mut *conn).await?;
    sqlx::query("INSERT INTO emails (tenant_id, contact_id, conversation_id, direction, sender, recipient, subject, body, status, message_id, written_by) VALUES ($1::uuid, $2::uuid, $3::uuid, 'in', $4, '', $5, $6, 'received', $7, 'contact')")
        .bind(tenant).bind(&contact).bind(&conv).bind(&from_addr).bind(subject).bind(body).bind(message_id).execute(&mut *conn).await?;

    let rank = |st: &str| crate::stage_rank(st);
    let mut new_stage: Option<&str> = None;
    let mut status = "open";
    let mut timer = true;
    match class.label {
        "unsubscribe" => {
            sqlx::query("INSERT INTO suppression_list (tenant_id, email, reason, words) VALUES ($1::uuid, $2, 'unsubscribed by reply', $3) ON CONFLICT DO NOTHING")
                .bind(tenant).bind(&from_addr).bind(body.chars().take(200).collect::<String>()).execute(&mut *conn).await?;
            sqlx::query("UPDATE contacts SET do_not_contact = true WHERE id = $1::uuid").bind(&contact).execute(&mut *conn).await?;
            campaigns::stop_contact(conn, tenant, &contact, "unsubscribed").await?;
            new_stage = Some("closed_out");
            status = "closed";
            timer = false;
        }
        "bounce" => {
            sqlx::query("UPDATE contacts SET email_status = 'invalid' WHERE id = $1::uuid").bind(&contact).execute(&mut *conn).await?;
            campaigns::stop_contact(conn, tenant, &contact, "bounced").await?;
            status = "closed";
            timer = false;
            notices::push(conn, tenant, None, "bounce", "An email bounced", &from_addr, "emails").await?;
        }
        "out_of_office" => {
            // The sequence waits N days instead of stopping.
            let days = settings::int(&s, "ooo_wait_days") as i32;
            sqlx::query("UPDATE campaign_contacts SET next_send_at = GREATEST(next_send_at, now()) + make_interval(days => $3) WHERE tenant_id = $1::uuid AND contact_id = $2::uuid AND status = 'active'")
                .bind(tenant).bind(&contact).bind(days).execute(&mut *conn).await?;
            status = "closed";
            timer = false;
        }
        "not_now" => {
            campaigns::stop_contact(conn, tenant, &contact, "replied: not now").await?;
            new_stage = Some("closed_out");
            status = "closed";
            timer = false;
        }
        other => {
            campaigns::stop_contact(conn, tenant, &contact, "replied").await?;
            new_stage = Some(if other == "interested" { "interested" } else { "replied" });
            if other == "price" {
                // The AI never states a price that is not approved: a person takes over.
                status = "handover";
                timer = false;
                notices::push(conn, tenant, owner.as_deref(), "handover", "Handover: a price question", &from_addr, "conversations").await?;
            }
        }
    }
    if let Some(ns) = new_stage {
        if rank(ns) > rank(&stage) || ns == "closed_out" {
            sqlx::query("UPDATE contacts SET stage = $2, updated_at = now() WHERE id = $1::uuid").bind(&contact).bind(ns).execute(&mut *conn).await?;
        }
    }
    sqlx::query(
        "UPDATE conversations SET label = $2, urgent = $3, status = $4, last_message_at = now(), updated_at = now(), row_version = row_version + 1, \
         timer_due_at = CASE WHEN $5 THEN now() + make_interval(mins => $6) ELSE NULL END WHERE id = $1::uuid",
    )
    .bind(&conv).bind(class.label).bind(class.urgent).bind(status).bind(timer).bind(minutes as i32).execute(&mut *conn).await?;
    if timer {
        notices::push(conn, tenant, owner.as_deref(), "reply", "New reply", &format!("{from_addr} replied."), "conversations").await?;
    }
    activity::log(conn, tenant, "system", "reply.received", "conversation", &conv, &format!("Reply received ({})", class.label), serde_json::json!({"label": class.label})).await?;
    Ok(Received::Saved { conversation_id: conv, label: class.label.to_string() })
}

pub async fn take_over(app: &App, actor: &Actor, conv: &str) -> Result<()> {
    need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    let n = sqlx::query("UPDATE conversations SET status = 'employee', assigned_to = $3::uuid, timer_due_at = NULL, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid AND status <> 'closed' AND ($4::uuid IS NULL OR assigned_to = $4::uuid)")
        .bind(conv).bind(&actor.tenant_id).bind(&actor.user_id).bind(crate::records::scope(actor)).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "conversation.taken_over", "conversation", conv, "An employee took over", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn create_group(app: &App, actor: &Actor, subject: &str, emails: &[String]) -> Result<String> {
    need(actor, "conversation.reply")?;
    let subject = subject.trim();
    if subject.is_empty() || subject.chars().count() > 200 {
        return Err(Error::BadRequest("Enter a group subject up to 200 characters.".into()));
    }
    let recipients = normalize_group_recipients(emails)?;

    let mut tx = app.tx(actor).await?;
    let mut contacts = Vec::with_capacity(recipients.len());
    for email in &recipients {
        let contact = sqlx::query(
            "SELECT id::text, owner_id::text FROM contacts WHERE tenant_id = $1::uuid AND lower(email) = $2 \
             AND ($3::uuid IS NULL OR owner_id = $3::uuid)",
        )
        .bind(&actor.tenant_id)
        .bind(email)
        .bind(crate::records::scope(actor))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::BadRequest(format!("No accessible contact has the email {email}.")))?;
        contacts.push((sqlx::Row::get::<String, _>(&contact, 0), sqlx::Row::get::<Option<String>, _>(&contact, 1)));
    }

    let first_contact = &contacts[0].0;
    let assigned_to = if actor.role == "employee" {
        Some(actor.user_id.clone())
    } else {
        contacts[0].1.clone()
    };
    let id: String = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, contact_id, subject, assigned_to) VALUES ($1::uuid, $2::uuid, $3, $4::uuid) RETURNING id::text",
    )
    .bind(&actor.tenant_id)
    .bind(first_contact)
    .bind(subject)
    .bind(&assigned_to)
    .fetch_one(&mut *tx)
    .await?;
    for (contact_id, _) in contacts {
        sqlx::query("INSERT INTO conversation_participants (tenant_id, conversation_id, contact_id) VALUES ($1::uuid, $2::uuid, $3::uuid)")
            .bind(&actor.tenant_id)
            .bind(&id)
            .bind(contact_id)
            .execute(&mut *tx)
            .await?;
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "conversation.group_created", "conversation", &id, "Group conversation created", serde_json::json!({"participants": recipients.len()})).await?;
    tx.commit().await?;
    Ok(id)
}

fn normalize_group_recipients(emails: &[String]) -> Result<Vec<String>> {
    let mut recipients = Vec::new();
    for email in emails {
        let email = email.trim().to_lowercase();
        if email.is_empty() {
            continue;
        }
        if !crate::outbox::valid_email_address(&email) {
            return Err(Error::BadRequest(format!("Enter a valid contact email address: {email}.")));
        }
        if !recipients.contains(&email) {
            recipients.push(email);
        }
    }
    if !(2..=20).contains(&recipients.len()) {
        return Err(Error::BadRequest("A group needs 2 to 20 different contact email addresses.".into()));
    }
    Ok(recipients)
}

pub async fn toggle_archive(app: &App, actor: &Actor, conv: &str) -> Result<bool> {
    need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    let archived: Option<bool> = sqlx::query_scalar(
        "INSERT INTO conversation_user_state (tenant_id, user_id, conversation_id, archived) \
         SELECT tenant_id, $3::uuid, id, true FROM conversations \
         WHERE id = $1::uuid AND tenant_id = $2::uuid AND ($4::uuid IS NULL OR assigned_to = $4::uuid) \
         ON CONFLICT (user_id, conversation_id) DO UPDATE SET archived = NOT conversation_user_state.archived \
         RETURNING archived",
    )
    .bind(conv)
    .bind(&actor.tenant_id)
    .bind(&actor.user_id)
    .bind(crate::records::scope(actor))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(archived) = archived else {
        return Err(Error::NotFound);
    };
    tx.commit().await?;
    Ok(archived)
}

pub async fn hand_back(app: &App, actor: &Actor, conv: &str) -> Result<()> {
    need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    let s = settings::all(&mut tx, &actor.tenant_id).await?;
    let n = sqlx::query("UPDATE conversations SET status = 'open', timer_due_at = now() + make_interval(mins => $3), updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid AND status IN ('employee','handover')")
        .bind(conv).bind(&actor.tenant_id).bind(settings::int(&s, "employee_timer_minutes") as i32).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, "conversation.handed_back", "conversation", conv, "Handed back to the AI queue", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

/// Opening a draft extends the employee timer by 10 minutes, once.
pub async fn extend_timer(app: &App, actor: &Actor, conv: &str) -> Result<bool> {
    need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    let n = sqlx::query("UPDATE conversations SET timer_due_at = timer_due_at + interval '10 minutes', timer_extended = true WHERE id = $1::uuid AND tenant_id = $2::uuid AND status = 'open' AND timer_due_at IS NOT NULL AND timer_extended = false")
        .bind(conv).bind(&actor.tenant_id).execute(&mut *tx).await;
    // `timer_extended` is added by migration 0002.
    let n = n?.rows_affected();
    tx.commit().await?;
    Ok(n > 0)
}

async fn queue_message(conn: &mut PgConnection, app: &App, tenant: &str, conv: &str, body: &str, written_by: &str, actor_email: &str) -> Result<Vec<String>> {
    let c = sqlx::query(
        "SELECT cv.subject, \
                COALESCE((SELECT mailbox_id::text FROM emails e WHERE e.conversation_id = cv.id AND e.direction = 'out' AND e.mailbox_id IS NOT NULL ORDER BY created_at DESC LIMIT 1), \
                         (SELECT id::text FROM mailboxes WHERE tenant_id = cv.tenant_id AND status = 'active' ORDER BY created_at LIMIT 1)) \
         FROM conversations cv WHERE cv.id = $1::uuid AND cv.tenant_id = $2::uuid",
    )
    .bind(conv)
    .bind(tenant)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(Error::NotFound)?;
    let subject = format!("Re: {}", c.get::<String, _>(0).trim_start_matches("Re: "));
    let mailbox: Option<String> = c.get(1);
    let participants = sqlx::query(
        "SELECT c.id::text, c.email FROM conversation_participants cp \
         JOIN contacts c ON c.id = cp.contact_id AND c.tenant_id = cp.tenant_id \
         WHERE cp.tenant_id = $1::uuid AND cp.conversation_id = $2::uuid ORDER BY cp.added_at",
    )
    .bind(tenant)
    .bind(conv)
    .fetch_all(&mut *conn)
    .await?;
    if participants.is_empty() {
        return Err(Error::Conflict("This conversation has no participants.".into()));
    }

    let group_message_id = uuid::Uuid::new_v4().to_string();
    let mut hard = Vec::new();
    for participant in participants {
        let contact: String = participant.get(0);
        let to: String = participant.get::<Option<String>, _>(1).unwrap_or_default();
        let email = crate::outbox::EmailRow {
            id: String::new(),
            contact_id: contact.clone(),
            campaign_id: None,
            mailbox_id: mailbox.clone(),
            conversation_id: Some(conv.to_string()),
            step_no: 0,
            recipient: to.clone(),
            subject: subject.clone(),
            body: body.to_string(),
            is_reply: true,
        };
        let (reasons, _) = crate::outbox::evaluate(conn, app, tenant, &email).await?;
        let blocked: Vec<String> = reasons
            .iter()
            .filter(|reason| !crate::compliance::gate::TEMPORARY.contains(&reason.as_str()))
            .cloned()
            .collect();
        let status = if blocked.is_empty() { "queued" } else { "blocked" };
        sqlx::query(
            "INSERT INTO emails (tenant_id, contact_id, mailbox_id, conversation_id, group_message_id, direction, step_no, recipient, subject, body, status, blocked_reason, written_by, scheduled_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'out', 0, $6, $7, $8, $9, $10, $11, now())",
        )
        .bind(tenant)
        .bind(&contact)
        .bind(&mailbox)
        .bind(conv)
        .bind(&group_message_id)
        .bind(&to)
        .bind(&subject)
        .bind(body)
        .bind(status)
        .bind(blocked.join(" "))
        .bind(written_by)
        .execute(&mut *conn)
        .await?;
        hard.extend(blocked);
    }
    hard.sort();
    hard.dedup();
    if hard.is_empty() {
        sqlx::query("UPDATE conversations SET last_message_at = now(), updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid")
            .bind(conv)
            .bind(tenant)
            .execute(&mut *conn)
            .await?;
        activity::log(conn, tenant, actor_email, "reply.queued", "conversation", conv, &format!("Reply by {written_by} queued"), serde_json::json!({})).await?;
    } else {
        sqlx::query("UPDATE emails SET status = 'blocked', blocked_reason = $2, updated_at = now() WHERE group_message_id = $1::uuid AND tenant_id = $3::uuid")
            .bind(&group_message_id)
            .bind(hard.join(" "))
            .bind(tenant)
            .execute(&mut *conn)
            .await?;
        notices::push(conn, tenant, None, "blocked", "A reply was blocked", &hard.join(" "), "emails").await?;
        sqlx::query("UPDATE conversations SET status = 'handover' WHERE id = $1::uuid").bind(conv).execute(&mut *conn).await?;
    }
    Ok(hard)
}

/// An employee writes the reply. The same writing rules and gate apply.
pub async fn employee_reply(app: &App, actor: &Actor, conv: &str, body: &str) -> Result<Vec<String>> {
    need(actor, "conversation.reply")?;
    let mut tx = app.tx(actor).await?;
    if let Some(_other) = crate::records::scope(actor) {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM conversations WHERE id = $1::uuid AND tenant_id = $2::uuid AND assigned_to = $3::uuid)")
            .bind(conv).bind(&actor.tenant_id).bind(&actor.user_id).fetch_one(&mut *tx).await?;
        if !ok {
            return Err(Error::NotFound);
        }
    }
    let hard = queue_message(&mut tx, app, &actor.tenant_id, conv, body.trim(), "employee", &actor.email).await?;
    if hard.is_empty() {
        sqlx::query("UPDATE conversations SET status = 'employee', assigned_to = COALESCE(assigned_to, $3::uuid), timer_due_at = NULL, updated_at = now() WHERE id = $1::uuid AND tenant_id = $2::uuid")
            .bind(conv).bind(&actor.tenant_id).bind(&actor.user_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(hard)
}

pub async fn messages(app: &App, actor: &Actor, conv: &str) -> Result<Vec<shared::ConversationMessage>> {
    need(actor, "record.read")?;
    let mut tx = app.tx(actor).await?;
    let marked = sqlx::query(
        "INSERT INTO conversation_user_state (tenant_id, user_id, conversation_id, last_read_at) \
         SELECT tenant_id, $3::uuid, id, now() FROM conversations \
         WHERE id = $1::uuid AND tenant_id = $2::uuid AND ($4::uuid IS NULL OR assigned_to = $4::uuid) \
         ON CONFLICT (user_id, conversation_id) DO UPDATE SET last_read_at = now()",
    )
    .bind(conv)
    .bind(&actor.tenant_id)
    .bind(&actor.user_id)
    .bind(crate::records::scope(actor))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if marked == 0 {
        return Err(Error::NotFound);
    }
    let rows = if crate::records::scope(actor).is_some() {
        sqlx::query(
            "SELECT e.direction, string_agg(DISTINCT e.sender, ', '), string_agg(DISTINCT e.recipient, ', '), e.subject, e.body, \
                    to_char(min(e.created_at) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
             FROM emails e JOIN conversations cv ON cv.id = e.conversation_id \
             WHERE cv.tenant_id = $1::uuid AND cv.id = $2::uuid AND cv.assigned_to = $3::uuid \
             GROUP BY COALESCE(e.group_message_id, e.id), e.direction, e.subject, e.body \
             ORDER BY min(e.created_at) DESC LIMIT 100",
        )
        .bind(&actor.tenant_id)
        .bind(conv)
        .bind(&actor.user_id)
        .fetch_all(&mut *tx)
        .await?
    } else {
        sqlx::query(
            "SELECT e.direction, string_agg(DISTINCT e.sender, ', '), string_agg(DISTINCT e.recipient, ', '), e.subject, e.body, \
                    to_char(min(e.created_at) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
             FROM emails e JOIN conversations cv ON cv.id = e.conversation_id \
             WHERE cv.tenant_id = $1::uuid AND cv.id = $2::uuid \
             GROUP BY COALESCE(e.group_message_id, e.id), e.direction, e.subject, e.body \
             ORDER BY min(e.created_at) DESC LIMIT 100",
        )
        .bind(&actor.tenant_id)
        .bind(conv)
        .fetch_all(&mut *tx)
        .await?
    };
    let mut messages: Vec<shared::ConversationMessage> = rows
        .iter()
        .map(|r| shared::ConversationMessage {
            direction: r.get(0),
            sender: r.get(1),
            recipient: r.get(2),
            subject: r.get(3),
            body: r.get(4),
            at: r.get(5),
        })
        .collect();
    messages.reverse();
    tx.commit().await?;
    Ok(messages)
}

/// The AI queue: when no employee replied in time and the reply is safe, ask Claude Desktop.
pub async fn timers_due(app: &App, tenant: &str) -> Result<usize> {
    let mut tx = app.tx_for_tenant(tenant).await?;
    let s = settings::all(&mut tx, tenant).await?;
    let ai_on = settings::flag(&s, "auto_reply");
    let rows = sqlx::query("SELECT id::text, label, contact_id::text, assigned_to::text FROM conversations WHERE tenant_id = $1::uuid AND status = 'open' AND timer_due_at <= now() FOR UPDATE SKIP LOCKED LIMIT 50")
        .bind(tenant).fetch_all(&mut *tx).await?;
    let mut n = 0;
    for r in &rows {
        let (id, label): (String, String) = (r.get(0), r.get(1));
        if ai_on && matches!(label.as_str(), "interested" | "question" | "other") {
            sqlx::query("UPDATE conversations SET status = 'ai', timer_due_at = NULL, updated_at = now() WHERE id = $1::uuid").bind(&id).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO ai_tasks (tenant_id, kind, title, payload) VALUES ($1::uuid, 'write_reply', $2, $3::jsonb)")
                .bind(tenant).bind("Write a reply for a conversation").bind(serde_json::json!({"conversation_id": id}).to_string()).execute(&mut *tx).await?;
            activity::log(&mut tx, tenant, "system", "reply.ai_requested", "conversation", &id, "No employee replied in time: the AI was asked to write a reply", serde_json::json!({})).await?;
        } else {
            // Not safe for the AI: a person must answer.
            sqlx::query("UPDATE conversations SET status = 'handover', timer_due_at = NULL, updated_at = now() WHERE id = $1::uuid").bind(&id).execute(&mut *tx).await?;
            notices::push(&mut tx, tenant, r.get::<Option<String>, _>(3).as_deref(), "handover", "Handover: needs an employee", "The AI does not reply to this kind of message.", "conversations").await?;
        }
        n += 1;
    }
    tx.commit().await?;
    Ok(n)
}

/// Claude Desktop returns a drafted reply through the MCP tool. It is checked, then queued.
pub async fn ai_reply_ready(conn: &mut PgConnection, app: &App, tenant: &str, conv: &str, body: &str) -> Result<Vec<String>> {
    let s = settings::all(conn, tenant).await?;
    let rules = Rules { ban_dashes: settings::flag(&s, "ban_dashes") };
    // If an employee replied while the AI was working, the AI stays out of the thread.
    let status: String = sqlx::query_scalar("SELECT status FROM conversations WHERE id = $1::uuid AND tenant_id = $2::uuid").bind(conv).bind(tenant).fetch_optional(&mut *conn).await?.ok_or(Error::NotFound)?;
    if status != "ai" {
        return Err(Error::Conflict("An employee already handles this conversation.".into()));
    }
    let problems = draft::check("", body, 0, &rules);
    if !problems.is_empty() {
        sqlx::query("UPDATE conversations SET status = 'handover' WHERE id = $1::uuid").bind(conv).execute(&mut *conn).await?;
        notices::push(conn, tenant, None, "blocked", "An AI reply was blocked", &problems.join(" "), "conversations").await?;
        return Ok(problems);
    }
    queue_message(conn, app, tenant, conv, body, "ai", "claude desktop").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_order_and_labels() {
        let c = |f: &str, s: &str, b: &str| classify(f, s, b).label;
        assert_eq!(c("mailer-daemon@x.com", "Delivery Status Notification", ""), "bounce");
        assert_eq!(c("a@b.com", "Automatic reply", "I am out of office until Monday"), "out_of_office");
        assert_eq!(c("a@b.com", "Re: hello", "Please unsubscribe me"), "unsubscribe");
        assert_eq!(c("a@b.com", "Re: hello", "We are not interested, thanks"), "not_now");
        assert_eq!(c("a@b.com", "Re: hello", "How much does this cost?"), "price");
        assert_eq!(c("a@b.com", "Re: hello", "Sounds good, tell me more"), "interested");
        assert_eq!(c("a@b.com", "Re: hello", "Who are you?"), "question");
        assert_eq!(c("a@b.com", "Re: hello", "Thanks."), "other");
    }

    #[test]
    fn not_interested_is_not_read_as_interested() {
        assert_eq!(classify("a@b.com", "", "not interested").label, "not_now");
    }

    #[test]
    fn urgent_words_set_the_flag() {
        assert!(classify("a@b.com", "", "Please call me asap").urgent);
        assert!(!classify("a@b.com", "", "Call me next week").urgent);
    }

    #[test]
    fn group_recipients_are_validated_deduplicated_and_bounded() {
        let valid = vec![
            " A@example.com ".to_string(),
            "a@example.com".to_string(),
            "b@example.com".to_string(),
        ];
        assert_eq!(normalize_group_recipients(&valid).unwrap(), ["a@example.com", "b@example.com"]);
        assert!(normalize_group_recipients(&["a@example.com".into(), "a@example.com".into()]).is_err());
        assert!(normalize_group_recipients(&["bad-address".into(), "b@example.com".into()]).is_err());
        let too_many = (0..21).map(|i| format!("person{i}@example.com")).collect::<Vec<_>>();
        assert!(normalize_group_recipients(&too_many).is_err());
    }
}
