//! Quick: the built-in assistant. A rules planner answers simple requests with the asking
//! person's own rights. Anything else becomes a task for Claude Desktop. Every message is kept.

use crate::app::{need, App};
use crate::error::{Error, Result};
use crate::{ai, records};
use auth::Actor;
use shared::QuickMessage;
use sqlx::Row;

#[derive(Debug, PartialEq)]
pub enum Plan {
    Count(&'static str),
    Pause,
    Unknown,
}

pub fn plan(text: &str) -> Plan {
    let t = text.to_lowercase();
    if t.contains("pause") && (t.contains("sending") || t.contains("all")) {
        return Plan::Pause;
    }
    if t.contains("how many") || t.starts_with("count") {
        for (word, key) in [("people", "people"), ("contacts", "people"), ("leads", "people"), ("compan", "companies"), ("deal", "deals"), ("task", "tasks"), ("campaign", "campaigns")] {
            if t.contains(word) {
                return Plan::Count(key);
            }
        }
    }
    Plan::Unknown
}

pub async fn history(app: &App, actor: &Actor) -> Result<Vec<QuickMessage>> {
    let mut tx = app.tx(actor).await?;
    let rows = sqlx::query("SELECT role, body, to_char(at, 'YYYY-MM-DD HH24:MI') FROM quick_messages WHERE tenant_id = $1::uuid AND user_id = $2::uuid ORDER BY id DESC LIMIT 100")
        .bind(&actor.tenant_id).bind(&actor.user_id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let mut v: Vec<QuickMessage> = rows.iter().map(|r| QuickMessage { role: r.get(0), body: r.get(1), at: r.get(2) }).collect();
    v.reverse();
    Ok(v)
}

pub async fn ask(app: &App, actor: &Actor, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 1000 {
        return Err(Error::BadRequest("Please write 1 to 1000 characters.".into()));
    }
    let answer = match plan(text) {
        Plan::Count(key) => {
            need(actor, "record.read")?;
            let m = records::find(key)?;
            let mut tx = app.tx(actor).await?;
            let r = query_engine::list(&mut tx, m, &actor.tenant_id, records::scope(actor), &shared::QueryRequest { page_size: 1, ..Default::default() }).await?;
            tx.commit().await?;
            format!("There are {} in {}.", r.total, m.title)
        }
        Plan::Pause => {
            crate::campaigns::set_pause_all(app, actor, true).await?;
            "Sending is paused for everyone.".to_string()
        }
        Plan::Unknown => {
            need(actor, "ai.use")?;
            let mut tx = app.tx(actor).await?;
            sqlx::query("INSERT INTO ai_tasks (tenant_id, kind, title, payload) VALUES ($1::uuid, 'quick_request', $2, $3::jsonb)")
                .bind(&actor.tenant_id).bind(text.chars().take(120).collect::<String>()).bind(serde_json::json!({"text": text, "user_id": actor.user_id}).to_string()).execute(&mut *tx).await?;
            let st = ai::status(&mut tx, &actor.tenant_id).await?;
            tx.commit().await?;
            match st.state {
                "never" => "I saved this for Claude Desktop. It has not checked the queue yet, so please ask it to check.".to_string(),
                "late" => "I saved this for Claude Desktop. It is late in checking the queue.".to_string(),
                _ => "I saved this for Claude Desktop. It will pick it up at its next check.".to_string(),
            }
        }
    };
    let mut tx = app.tx(actor).await?;
    for (role, body) in [("user", text), ("quick", answer.as_str())] {
        sqlx::query("INSERT INTO quick_messages (tenant_id, user_id, role, body) VALUES ($1::uuid, $2::uuid, $3, $4)").bind(&actor.tenant_id).bind(&actor.user_id).bind(role).bind(body).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_reads_simple_requests() {
        assert_eq!(plan("How many people do we have?"), Plan::Count("people"));
        assert_eq!(plan("how many deals"), Plan::Count("deals"));
        assert_eq!(plan("Pause all sending"), Plan::Pause);
        assert_eq!(plan("write a note about Acme"), Plan::Unknown);
    }
}
