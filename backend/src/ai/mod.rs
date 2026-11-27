//! AI queue and the MCP server. All AI work runs through Claude Desktop, which connects to
//! this app only through the MCP tools below. The app never calls an AI model over an API.
//! Claude acts with the rights of the person who made the token, and nothing more.

use crate::app::{need, App};
use crate::error::{Error, Result};
use crate::{activity, conversations, records, scoring, settings};
use auth::Actor;
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};

pub struct AiStatus {
    pub state: &'static str,
    pub last_check_seconds: Option<i64>,
    pub expected_minutes: i64,
    pub waiting: i64,
    pub oldest_wait_seconds: i64,
}

pub async fn heartbeat(conn: &mut PgConnection, tenant: &str) -> Result<()> {
    sqlx::query("INSERT INTO ai_heartbeats (tenant_id) VALUES ($1::uuid)")
        .bind(tenant)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "DELETE FROM ai_heartbeats WHERE tenant_id = $1::uuid AND at < now() - interval '30 days'",
    )
    .bind(tenant)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// On time, late (more than twice the expected interval) or never checked.
pub fn state_for(last_seconds: Option<i64>, expected_minutes: i64) -> &'static str {
    match last_seconds {
        None => "never",
        Some(s) if s > expected_minutes.max(1) * 60 * 2 => "late",
        Some(_) => "on_time",
    }
}

pub async fn status(conn: &mut PgConnection, tenant: &str) -> Result<AiStatus> {
    let s = settings::all(conn, tenant).await?;
    let expected = settings::int(&s, "ai_check_minutes");
    let last: Option<i64> = sqlx::query_scalar("SELECT extract(epoch FROM now() - max(at))::bigint FROM ai_heartbeats WHERE tenant_id = $1::uuid").bind(tenant).fetch_one(&mut *conn).await?;
    let r = sqlx::query("SELECT count(*), COALESCE(extract(epoch FROM now() - min(created_at))::bigint, 0) FROM ai_tasks WHERE tenant_id = $1::uuid AND status = 'waiting'")
        .bind(tenant).fetch_one(&mut *conn).await?;
    Ok(AiStatus {
        state: state_for(last, expected),
        last_check_seconds: last,
        expected_minutes: expected,
        waiting: r.get(0),
        oldest_wait_seconds: r.get(1),
    })
}

pub async fn create_token(app: &App, actor: &Actor, label: &str) -> Result<String> {
    need(actor, "ai.use")?;
    let token = format!("bmk_{}", auth::new_token());
    let mut tx = app.tx(actor).await?;
    sqlx::query("INSERT INTO mcp_tokens (tenant_id, user_id, token_hash, label) VALUES ($1::uuid, $2::uuid, $3, $4)")
        .bind(&actor.tenant_id).bind(&actor.user_id).bind(auth::token_hash(&token)).bind(label.chars().take(80).collect::<String>()).execute(&mut *tx).await?;
    activity::log(
        &mut tx,
        &actor.tenant_id,
        &actor.email,
        "mcp.token_created",
        "mcp_token",
        "",
        "An MCP token was created",
        json!({}),
    )
    .await?;
    tx.commit().await?;
    // The token is shown once. Only its hash is stored.
    Ok(token)
}

pub async fn actor_for_token(app: &App, token: &str) -> Result<Option<Actor>> {
    let r = sqlx::query(
        "SELECT u.id::text, u.tenant_id::text, u.role, u.name, u.email FROM mcp_tokens t JOIN users u ON u.id = t.user_id \
         JOIN tenants te ON te.id = t.tenant_id WHERE t.token_hash = $1 AND t.revoked_at IS NULL AND u.status = 'active' AND te.status = 'active'",
    )
    .bind(auth::token_hash(token))
    .fetch_optional(&app.pool).await?;
    Ok(r.map(|r| Actor {
        user_id: r.get(0),
        tenant_id: r.get(1),
        role: r.get(2),
        name: r.get(3),
        email: r.get(4),
    }))
}

pub async fn complete_task(
    conn: &mut PgConnection,
    app: &App,
    actor: &Actor,
    task: &str,
    result: &Value,
) -> Result<Value> {
    need(actor, "ai.use")?;
    let t = &actor.tenant_id;
    let r = sqlx::query("SELECT kind, payload::text, status FROM ai_tasks WHERE id = $1::uuid AND tenant_id = $2::uuid FOR UPDATE").bind(task).bind(t).fetch_optional(&mut *conn).await?.ok_or(Error::NotFound)?;
    let (kind, payload, status): (String, String, String) = (r.get(0), r.get(1), r.get(2));
    if status == "done" {
        return Err(Error::Conflict("This task is already done.".into()));
    }
    let payload: Value = serde_json::from_str(&payload).unwrap_or_default();
    let mut out = json!({"ok": true});
    let mut final_status = "done";
    let mut note = String::new();
    match kind.as_str() {
        "write_reply" => {
            need(actor, "conversation.reply")?;
            let conv = payload["conversation_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let body = result["body"].as_str().unwrap_or_default();
            let problems = conversations::ai_reply_ready(conn, app, t, &conv, body).await?;
            if !problems.is_empty() {
                final_status = "failed";
                note = problems.join(" ");
                out = json!({"ok": false, "problems": problems});
            }
        }
        "research_note" => {
            need(actor, "record.write")?;
            let contact = payload["contact_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let text = result["note"].as_str().unwrap_or_default().trim();
            if text.is_empty() || text.chars().count() > 1000 {
                return Err(Error::BadRequest(
                    "The note must be 1 to 1000 characters.".into(),
                ));
            }
            save_note(conn, actor, &contact, text).await?;
        }
        _ => {
            note = result["text"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(2000)
                .collect();
        }
    }
    sqlx::query("UPDATE ai_tasks SET status = $2, result = $3, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid")
        .bind(task).bind(final_status).bind(&note).execute(&mut *conn).await?;
    activity::log(
        conn,
        t,
        &actor.email,
        "ai.task_done",
        "ai_task",
        task,
        &format!("AI task {kind} {final_status}"),
        json!({}),
    )
    .await?;
    Ok(out)
}

pub async fn save_note(
    conn: &mut PgConnection,
    actor: &Actor,
    contact: &str,
    text: &str,
) -> Result<()> {
    let t = &actor.tenant_id;
    let scope = records::scope(actor);
    let n = sqlx::query("UPDATE contacts SET research_note = $3, updated_at = now(), row_version = row_version + 1 WHERE id = $1::uuid AND tenant_id = $2::uuid AND ($4::uuid IS NULL OR owner_id = $4::uuid)")
        .bind(contact).bind(t).bind(text).bind(scope).execute(&mut *conn).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    scoring::rescore_contact(conn, t, contact).await?;
    // The email that waited for its personal line can go now.
    sqlx::query("UPDATE campaign_contacts SET next_send_at = now() WHERE tenant_id = $1::uuid AND contact_id = $2::uuid AND status = 'active' AND next_send_at > now()").bind(t).bind(contact).execute(&mut *conn).await?;
    Ok(())
}

pub fn tools() -> Value {
    let obj = |props: Value, req: Vec<&str>| json!({"type": "object", "properties": props, "required": req});
    json!([
        {"name": "list_ai_tasks", "description": "List tasks that wait for Claude. Also records that Claude Desktop checked the queue.",
         "inputSchema": obj(json!({"limit": {"type": "integer"}}), vec![])},
        {"name": "complete_ai_task", "description": "Finish a task. write_reply needs result.body. research_note needs result.note. Other kinds take result.text.",
         "inputSchema": obj(json!({"task_id": {"type": "string"}, "result": {"type": "object"}}), vec!["task_id", "result"])},
        {"name": "search_people", "description": "Search people by name, email, title or company. Returns at most 20.",
         "inputSchema": obj(json!({"query": {"type": "string"}}), vec!["query"])},
        {"name": "get_person", "description": "Get one person by id.",
         "inputSchema": obj(json!({"id": {"type": "string"}}), vec!["id"])},
        {"name": "save_research_note", "description": "Save a short research note for a person (1 to 1000 characters). Use only facts from public pages.",
         "inputSchema": obj(json!({"contact_id": {"type": "string"}, "note": {"type": "string"}}), vec!["contact_id", "note"])},
        {"name": "add_candidate", "description": "Save a researched company as a candidate that needs review. It is never saved as a contact.",
         "inputSchema": obj(json!({"name": {"type": "string"}, "website": {"type": "string"}, "email": {"type": "string"}, "industry": {"type": "string"}, "city": {"type": "string"}, "source_url": {"type": "string"}}), vec!["name", "source_url"])}
    ])
}

fn text_result(v: Value, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": v.to_string()}], "isError": is_error})
}

async fn log_call(app: &App, actor: &Actor, tool: &str, ok: bool, summary: &str) {
    if let Ok(mut tx) = app.tx(actor).await {
        let _ = sqlx::query("INSERT INTO mcp_tool_calls (tenant_id, user_id, tool, ok, summary) VALUES ($1::uuid, $2::uuid, $3, $4, $5)")
            .bind(&actor.tenant_id).bind(&actor.user_id).bind(tool).bind(ok).bind(summary.chars().take(300).collect::<String>()).execute(&mut *tx).await;
        let _ = tx.commit().await;
    }
}

async fn call_tool(app: &App, actor: &Actor, name: &str, args: &Value) -> Result<Value> {
    need(actor, "ai.use")?;
    let mut tx = app.tx(actor).await?;
    let t = actor.tenant_id.clone();
    let out = match name {
        "list_ai_tasks" => {
            heartbeat(&mut tx, &t).await?;
            let limit = args["limit"].as_i64().unwrap_or(20).clamp(1, 50);
            let rows = sqlx::query("SELECT id::text, kind, title, payload::text FROM ai_tasks WHERE tenant_id = $1::uuid AND status = 'waiting' ORDER BY created_at LIMIT $2")
                .bind(&t).bind(limit).fetch_all(&mut *tx).await?;
            json!(rows.iter().map(|r| json!({"id": r.get::<String, _>(0), "kind": r.get::<String, _>(1), "title": r.get::<String, _>(2), "payload": serde_json::from_str::<Value>(&r.get::<String, _>(3)).unwrap_or_default()})).collect::<Vec<_>>())
        }
        "complete_ai_task" => {
            let id = args["task_id"].as_str().unwrap_or_default();
            complete_task(&mut tx, app, actor, id, &args["result"]).await?
        }
        "search_people" => {
            need(actor, "record.read")?;
            let q = args["query"].as_str().unwrap_or_default();
            let req = shared::QueryRequest {
                search: q.to_string(),
                page_size: 20,
                ..Default::default()
            };
            let m = records::find("people")?;
            let r = query_engine::list(&mut tx, m, &t, records::scope(actor), &req).await?;
            json!(r
                .rows
                .iter()
                .map(|row| json!({"id": row.id, "cells": row.cells}))
                .collect::<Vec<_>>())
        }
        "get_person" => {
            need(actor, "record.read")?;
            let m = records::find("people")?;
            let (rec, _) = query_engine::get(
                &mut tx,
                m,
                &t,
                records::scope(actor),
                args["id"].as_str().unwrap_or_default(),
            )
            .await?;
            json!({"id": rec.id, "values": rec.values})
        }
        "save_research_note" => {
            need(actor, "record.write")?;
            save_note(
                &mut tx,
                actor,
                args["contact_id"].as_str().unwrap_or_default(),
                args["note"].as_str().unwrap_or_default().trim(),
            )
            .await?;
            json!({"ok": true})
        }
        "add_candidate" => {
            need(actor, "record.write")?;
            let name = args["name"].as_str().unwrap_or_default().trim();
            let src = args["source_url"].as_str().unwrap_or_default().trim();
            if name.is_empty() || src.is_empty() {
                return Err(Error::BadRequest(
                    "name and source_url are required.".into(),
                ));
            }
            crate::research::add_candidate(
                &mut tx,
                &t,
                None,
                name,
                args["website"].as_str().unwrap_or(""),
                args["email"].as_str().unwrap_or(""),
                args["industry"].as_str().unwrap_or(""),
                args["city"].as_str().unwrap_or(""),
                src,
                "Public web page; check the page before saving",
            )
            .await?;
            json!({"ok": true, "status": "needs_review"})
        }
        _ => return Err(Error::BadRequest(format!("Unknown tool: {name}"))),
    };
    tx.commit().await?;
    Ok(out)
}

/// One JSON-RPC message from Claude Desktop. Returns None for notifications.
pub async fn handle_rpc(app: &App, actor: &Actor, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg["method"].as_str().unwrap_or_default();
    let reply = |result: Value| {
        id.clone()
            .map(|i| json!({"jsonrpc": "2.0", "id": i, "result": result}))
    };
    let error = |code: i64, m: &str| {
        id.clone()
            .map(|i| json!({"jsonrpc": "2.0", "id": i, "error": {"code": code, "message": m}}))
    };
    match method {
        "initialize" => reply(
            json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "blackmemon-leads", "version": "0.1.0"}}),
        ),
        "notifications/initialized" | "notifications/cancelled" => None,
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({"tools": tools()})),
        "tools/call" => {
            let name = msg["params"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let args = msg["params"]["arguments"].clone();
            match call_tool(app, actor, &name, &args).await {
                Ok(v) => {
                    log_call(app, actor, &name, true, "ok").await;
                    reply(text_result(v, false))
                }
                Err(e) => {
                    let msg = e.body().message;
                    log_call(app, actor, &name, false, &msg).await;
                    reply(text_result(json!({"error": msg}), true))
                }
            }
        }
        _ => error(-32601, "Method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_state() {
        assert_eq!(state_for(None, 5), "never");
        assert_eq!(state_for(Some(200), 5), "on_time");
        assert_eq!(state_for(Some(600), 5), "on_time");
        assert_eq!(state_for(Some(601), 5), "late");
    }

    #[test]
    fn every_tool_has_a_schema() {
        let t = tools();
        let arr = t.as_array().unwrap();
        assert_eq!(arr.len(), 6);
        for x in arr {
            assert!(x["name"].is_string() && x["inputSchema"]["type"] == "object");
        }
    }
}
