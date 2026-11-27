//! Campaigns: start, pause, and the planner that writes the next email of each person.

use crate::app::{need, App};
use crate::compliance::draft::{self, Rules};
use crate::error::{Error, Result};
use crate::outbox::merge;
use crate::{activity, notices, settings};
use auth::Actor;
use sqlx::{PgConnection, Row};
use std::collections::HashMap;

pub async fn start(app: &App, actor: &Actor, id: &str) -> Result<i64> {
    need(actor, "campaign.run")?;
    let mut tx = app.tx(actor).await?;
    let t = &actor.tenant_id;
    let s = settings::all(&mut tx, t).await?;
    // Sample check: campaigns cannot start until the owner marked 5 researched companies.
    if !settings::flag(&s, "sample_check_done") {
        return Err(Error::Conflict("Campaigns cannot start until the owner has checked 5 researched companies (sample check, Settings General).".into()));
    }
    let c = sqlx::query("SELECT status, sequence_id::text, audience_id::text, mailbox_id::text, subject_variants FROM campaigns WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(id).bind(t).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let status: String = c.get(0);
    if status == "running" || status == "done" {
        return Err(Error::Conflict("This campaign is already running or finished.".into()));
    }
    let (Some(seq), Some(aud), Some(mb)) = (c.get::<Option<String>, _>(1), c.get::<Option<String>, _>(2), c.get::<Option<String>, _>(3)) else {
        return Err(Error::BadRequest("Pick a sequence, an audience and a mailbox first.".into()));
    };
    let steps: i64 = sqlx::query_scalar("SELECT count(*) FROM sequence_steps WHERE sequence_id = $1::uuid AND template_id IS NOT NULL").bind(&seq).fetch_one(&mut *tx).await?;
    if steps == 0 {
        return Err(Error::BadRequest("The sequence has no steps with a template.".into()));
    }
    let mb_status: String = sqlx::query_scalar("SELECT status FROM mailboxes WHERE id = $1::uuid").bind(&mb).fetch_one(&mut *tx).await?;
    if mb_status != "active" {
        return Err(Error::Conflict("The mailbox is not active.".into()));
    }
    let variants = c.get::<String, _>(4).lines().filter(|l| !l.trim().is_empty()).count().max(1) as i32;
    let n = sqlx::query(
        "INSERT INTO campaign_contacts (campaign_id, contact_id, tenant_id, variant, next_step, next_send_at) \
         SELECT $1::uuid, c.id, c.tenant_id, abs(hashtext(c.id::text)) % $4, 1, now() \
         FROM contacts c JOIN audiences a ON a.id = $2::uuid AND a.tenant_id = c.tenant_id \
         LEFT JOIN companies co ON co.id = c.company_id \
         WHERE c.tenant_id = $3::uuid AND c.score >= a.min_score AND c.stage = a.stage AND c.email_status = 'verified' AND c.do_not_contact = false \
           AND c.email IS NOT NULL AND (a.industry = '' OR co.industry ILIKE a.industry) AND (a.city = '' OR c.city ILIKE a.city) \
           AND NOT EXISTS (SELECT 1 FROM suppression_list sl WHERE sl.tenant_id = c.tenant_id AND lower(sl.email) = lower(c.email)) \
         ON CONFLICT DO NOTHING",
    )
    .bind(id).bind(&aud).bind(t).bind(variants).execute(&mut *tx).await?.rows_affected() as i64;
    if n == 0 {
        return Err(Error::BadRequest("No verified people match this audience yet.".into()));
    }
    sqlx::query("UPDATE campaigns SET status = 'running', started_at = COALESCE(started_at, now()), updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid").bind(id).execute(&mut *tx).await?;
    activity::log(&mut tx, t, &actor.email, "campaign.started", "campaign", id, &format!("Campaign started with {n} people"), serde_json::json!({"people": n})).await?;
    tx.commit().await?;
    Ok(n)
}

pub async fn set_paused(app: &App, actor: &Actor, id: &str, pause: bool) -> Result<()> {
    need(actor, "campaign.run")?;
    let mut tx = app.tx(actor).await?;
    let (from, to) = if pause { ("running", "paused") } else { ("paused", "running") };
    let n = sqlx::query("UPDATE campaigns SET status = $3, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid AND status = $4")
        .bind(id).bind(&actor.tenant_id).bind(to).bind(from).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::Conflict(format!("Only a {from} campaign can be set to {to}.")));
    }
    activity::log(&mut tx, &actor.tenant_id, &actor.email, if pause { "campaign.paused" } else { "campaign.resumed" }, "campaign", id, &format!("Campaign {to}"), serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

/// A reply, bounce or unsubscribe always stops the sequence.
pub async fn stop_contact(conn: &mut PgConnection, tenant: &str, contact: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE campaign_contacts SET status = 'stopped', stop_reason = $3, next_send_at = NULL WHERE tenant_id = $1::uuid AND contact_id = $2::uuid AND status = 'active'")
        .bind(tenant).bind(contact).bind(reason).execute(&mut *conn).await?;
    // Emails already planned but not yet sent are stopped too.
    sqlx::query("UPDATE emails SET status = 'blocked', blocked_reason = $3, updated_at = now() WHERE tenant_id = $1::uuid AND contact_id = $2::uuid AND direction = 'out' AND conversation_id IS NULL AND status IN ('queued','waiting_approval')")
        .bind(tenant).bind(contact).bind(format!("Stopped: {reason}.")).execute(&mut *conn).await?;
    Ok(())
}

fn first_sentence(note: &str) -> String {
    let n = note.trim().replace('\n', " ");
    let cut = n.find(['.', '!', '?']).map(|i| i + 1).unwrap_or(n.len());
    let s: String = n[..cut].chars().take(200).collect();
    s.trim().to_string()
}

/// Write the next email for every person who is due. Returns how many emails were planned.
pub async fn plan_due(app: &App, tenant: &str) -> Result<usize> {
    let mut tx = app.tx_for_tenant(tenant).await?;
    let s = settings::all(&mut tx, tenant).await?;
    let auto = settings::flag(&s, "auto_send");
    let rules = Rules { ban_dashes: settings::flag(&s, "ban_dashes") };
    let due = sqlx::query(
        "SELECT cc.campaign_id::text, cc.contact_id::text, cc.next_step, cc.variant, ca.sequence_id::text, ca.mailbox_id::text, ca.subject_variants, \
                c.first_name, COALESCE(co.name,''), c.city, c.research_note, c.email, \
                st.template_id::text, st.day_offset \
         FROM campaign_contacts cc \
         JOIN campaigns ca ON ca.id = cc.campaign_id AND ca.status = 'running' \
         JOIN contacts c ON c.id = cc.contact_id \
         LEFT JOIN companies co ON co.id = c.company_id \
         JOIN sequence_steps st ON st.sequence_id = ca.sequence_id AND st.step_no = cc.next_step \
         WHERE cc.tenant_id = $1::uuid AND cc.status = 'active' AND cc.next_send_at <= now() \
         ORDER BY cc.next_send_at LIMIT 50 FOR UPDATE OF cc SKIP LOCKED",
    )
    .bind(tenant)
    .fetch_all(&mut *tx)
    .await?;
    let mut planned = 0;
    for r in due {
        let (campaign, contact, step, variant): (String, String, i32, i32) = (r.get(0), r.get(1), r.get(2), r.get(3));
        let (seq, mailbox, variants): (String, Option<String>, String) = (r.get(4), r.get(5), r.get(6));
        let first: String = r.get(7);
        let company: String = r.get(8);
        let note: String = r.get(10);
        let recipient: String = r.get::<Option<String>, _>(11).unwrap_or_default();
        let template_id: Option<String> = r.get(12);
        let Some(template_id) = template_id else { continue };
        let tpl = sqlx::query("SELECT subject, body FROM templates WHERE id = $1::uuid AND tenant_id = $2::uuid").bind(&template_id).bind(tenant).fetch_optional(&mut *tx).await?;
        let Some(tpl) = tpl else { continue };
        let (mut subject, body_t): (String, String) = (tpl.get(0), tpl.get(1));
        if step == 1 {
            let vs: Vec<&str> = variants.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            if !vs.is_empty() {
                subject = vs[(variant as usize) % vs.len()].to_string();
            }
        }
        let mut f: HashMap<&str, String> = HashMap::new();
        f.insert("first_name", first);
        f.insert("company", company);
        f.insert("city", r.get(9));
        f.insert("personal_line", first_sentence(&note));
        f.insert("sender_name", settings::text(&s, "sender_name").to_string());
        let merged = merge(&body_t, &f).and_then(|b| merge(&subject, &f).map(|s| (s, b)));
        let (subject, body) = match merged {
            Ok(x) => x,
            Err(missing) if missing == ["personal_line"] => {
                // The AI writes the personal line from research: ask Claude Desktop through the queue.
                let open: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ai_tasks WHERE tenant_id = $1::uuid AND kind = 'research_note' AND status IN ('waiting','claimed') AND payload->>'contact_id' = $2)")
                    .bind(tenant).bind(&contact).fetch_one(&mut *tx).await?;
                if !open {
                    sqlx::query("INSERT INTO ai_tasks (tenant_id, kind, title, payload) VALUES ($1::uuid, 'research_note', $2, $3::jsonb)")
                        .bind(tenant).bind("Write a research note for the personal line of email 1").bind(serde_json::json!({"contact_id": contact}).to_string()).execute(&mut *tx).await?;
                }
                sqlx::query("UPDATE campaign_contacts SET next_send_at = now() + interval '1 hour' WHERE campaign_id = $1::uuid AND contact_id = $2::uuid").bind(&campaign).bind(&contact).execute(&mut *tx).await?;
                continue;
            }
            Err(missing) => {
                let why = format!("Missing data for {}.", missing.join(", "));
                insert_email(&mut tx, tenant, &campaign, &contact, mailbox.as_deref(), step, &recipient, "", "", "blocked", &why).await?;
                sqlx::query("UPDATE campaign_contacts SET status = 'stopped', stop_reason = 'email blocked', next_send_at = NULL WHERE campaign_id = $1::uuid AND contact_id = $2::uuid").bind(&campaign).bind(&contact).execute(&mut *tx).await?;
                notices::push(&mut tx, tenant, None, "blocked", "An email was blocked", &why, "emails").await?;
                continue;
            }
        };
        let problems = draft::check(&subject, &body, step as u32, &rules);
        if !problems.is_empty() {
            let why = problems.join(" ");
            insert_email(&mut tx, tenant, &campaign, &contact, mailbox.as_deref(), step, &recipient, &subject, &body, "blocked", &why).await?;
            sqlx::query("UPDATE campaign_contacts SET status = 'stopped', stop_reason = 'email blocked', next_send_at = NULL WHERE campaign_id = $1::uuid AND contact_id = $2::uuid").bind(&campaign).bind(&contact).execute(&mut *tx).await?;
            notices::push(&mut tx, tenant, None, "blocked", "An email was blocked", &why, "emails").await?;
            continue;
        }
        let status = if auto { "queued" } else { "waiting_approval" };
        insert_email(&mut tx, tenant, &campaign, &contact, mailbox.as_deref(), step, &recipient, &subject, &body, status, "").await?;
        // Next step: wait until the next day offset, or finish.
        let next = sqlx::query("SELECT day_offset FROM sequence_steps WHERE sequence_id = $1::uuid AND step_no = $2")
            .bind(&seq).bind(step + 1).fetch_optional(&mut *tx).await?;
        match next {
            Some(n) => {
                let cur: i32 = r.get(13);
                let days = (n.get::<i32, _>(0) - cur).max(0);
                sqlx::query("UPDATE campaign_contacts SET next_step = next_step + 1, next_send_at = now() + make_interval(days => $3) WHERE campaign_id = $1::uuid AND contact_id = $2::uuid")
                    .bind(&campaign).bind(&contact).bind(days).execute(&mut *tx).await?;
            }
            None => {
                sqlx::query("UPDATE campaign_contacts SET status = 'done', next_send_at = NULL WHERE campaign_id = $1::uuid AND contact_id = $2::uuid").bind(&campaign).bind(&contact).execute(&mut *tx).await?;
            }
        }
        planned += 1;
    }
    // A campaign with nobody left is done.
    sqlx::query("UPDATE campaigns SET status = 'done', updated_at = now() WHERE tenant_id = $1::uuid AND status = 'running' AND NOT EXISTS (SELECT 1 FROM campaign_contacts cc WHERE cc.campaign_id = campaigns.id AND cc.status = 'active')")
        .bind(tenant).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(planned)
}

#[allow(clippy::too_many_arguments)]
async fn insert_email(conn: &mut PgConnection, tenant: &str, campaign: &str, contact: &str, mailbox: Option<&str>, step: i32, recipient: &str, subject: &str, body: &str, status: &str, why: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO emails (tenant_id, contact_id, campaign_id, mailbox_id, direction, step_no, recipient, subject, body, status, blocked_reason, written_by, scheduled_at) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, 'out', $5, $6, $7, $8, $9, $10, 'template', now()) \
         ON CONFLICT (campaign_id, contact_id, step_no) WHERE direction = 'out' AND campaign_id IS NOT NULL DO NOTHING",
    )
    .bind(tenant).bind(contact).bind(campaign).bind(mailbox).bind(step).bind(recipient).bind(subject).bind(body).bind(status).bind(why)
    .execute(&mut *conn).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::first_sentence;

    #[test]
    fn first_sentence_is_cut_and_limited() {
        assert_eq!(first_sentence("Sells fabric online. Uses Excel for stock."), "Sells fabric online.");
        assert_eq!(first_sentence("  no full stop here  "), "no full stop here");
        assert_eq!(first_sentence(""), "");
        assert!(first_sentence(&"a".repeat(500)).len() <= 200);
    }
}

/// Pause all sending (or resume). Needs the `sending.pause` right, which managers also have.
pub async fn set_pause_all(app: &App, actor: &Actor, pause: bool) -> Result<()> {
    need(actor, "sending.pause")?;
    let mut tx = app.tx(actor).await?;
    crate::settings::set_raw(&mut tx, &actor.tenant_id, Some(&actor.user_id), "sending_paused", if pause { "true" } else { "false" }).await?;
    crate::activity::log(&mut tx, &actor.tenant_id, &actor.email, if pause { "sending.paused" } else { "sending.resumed" }, "settings", "", if pause { "All sending paused" } else { "Sending resumed" }, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}
