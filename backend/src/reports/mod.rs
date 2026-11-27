//! Today page and report tabs. Numbers come from SQL counts inside the tenant transaction.
//! Where data is missing the text says "Not available", never a made up number.

use crate::app::App;
use crate::error::{Error, Result};
use crate::{ai, records};
use auth::Actor;
use shared::{NeedItem, ReportSet, ReportTable, StatTile, TodayData};
use sqlx::{PgConnection, Row};

pub const TABS: [&str; 7] = ["Leads", "Emails", "Campaigns", "Sales", "AI usage", "Activity", "Employees"];

fn tile(label: &str, v: impl ToString) -> StatTile {
    StatTile { label: label.into(), value: v.to_string() }
}

async fn count(conn: &mut PgConnection, sql: &str, tenant: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(sql).bind(tenant).fetch_one(&mut *conn).await?)
}

/// Percent with one decimal, or "Not available" when there is nothing to divide by.
pub fn percent(part: i64, whole: i64) -> String {
    if whole <= 0 {
        "Not available".into()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / whole as f64)
    }
}

pub async fn today(app: &App, actor: &Actor) -> Result<TodayData> {
    let mut tx = app.tx(actor).await?;
    let t = actor.tenant_id.as_str();
    let sent_today = count(&mut tx, "SELECT count(*) FROM emails WHERE tenant_id = $1::uuid AND direction = 'out' AND status = 'sent' AND sent_at >= date_trunc('day', now())", t).await?;
    let replies = count(&mut tx, "SELECT count(*) FROM emails WHERE tenant_id = $1::uuid AND direction = 'in' AND created_at >= now() - interval '7 days'", t).await?;
    let open_deals = count(&mut tx, "SELECT count(*) FROM deals WHERE tenant_id = $1::uuid AND stage IN ('open','proposal','negotiation')", t).await?;
    let waiting = count(&mut tx, "SELECT count(*) FROM emails WHERE tenant_id = $1::uuid AND status = 'waiting_approval'", t).await?;
    let ai = ai::status(&mut tx, t).await?;
    let mut tiles = vec![
        tile("Emails sent today", sent_today),
        tile("Replies, last 7 days", replies),
        tile("Open deals", open_deals),
        tile("Waiting for approval", waiting),
        tile("AI queue", match ai.state { "on_time" => format!("On time, {} waiting", ai.waiting), "late" => format!("Late, {} waiting", ai.waiting), _ => "Never checked".into() }),
    ];
    if sent_today == 0 {
        tiles[0].value = "0".into();
    }
    let mut needs = vec![];
    for r in sqlx::query("SELECT cv.id::text, ct.email, cv.status, cv.urgent FROM conversations cv JOIN contacts ct ON ct.id = cv.contact_id WHERE cv.tenant_id = $1::uuid AND cv.status IN ('open','handover') AND ($2::uuid IS NULL OR cv.assigned_to = $2::uuid) ORDER BY cv.urgent DESC, cv.last_message_at LIMIT 10")
        .bind(t).bind(records::scope(actor)).fetch_all(&mut *tx).await? {
        let status: String = r.get(2);
        needs.push(NeedItem { kind: if status == "handover" { "Handover".into() } else { "Reply".into() }, title: r.get::<String, _>(1), detail: if r.get::<bool, _>(3) { "Urgent".into() } else { "Waiting for an answer".into() }, when: String::new(), link: "conversations".into() });
    }
    for r in sqlx::query("SELECT kind, last_error FROM jobs WHERE tenant_id = $1::uuid AND status = 'dead' ORDER BY updated_at DESC LIMIT 5").bind(t).fetch_all(&mut *tx).await? {
        let kind: String = r.get(0);
        let err: String = r.get(1);
        needs.push(NeedItem { kind: "Job".into(), title: format!("A {kind} job failed for good"), detail: if err.is_empty() { "No error text was kept".into() } else { err }, when: String::new(), link: "today".into() });
    }
    let mut stages = vec![];
    for (k, l) in shared::STAGES {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts WHERE tenant_id = $1::uuid AND stage = $2 AND ($3::uuid IS NULL OR owner_id = $3::uuid)").bind(t).bind(k).bind(records::scope(actor)).fetch_one(&mut *tx).await?;
        stages.push(tile(l, n));
    }
    let runs = sqlx::query("SELECT source_name, status, found::text, saved::text FROM collector_runs WHERE tenant_id = $1::uuid ORDER BY created_at DESC LIMIT 5").bind(t).fetch_all(&mut *tx).await?
        .iter().map(|r| vec![r.get::<String, _>(0), r.get::<String, _>(1), r.get::<String, _>(2), r.get::<String, _>(3)]).collect();
    tx.commit().await?;
    Ok(TodayData { tiles, needs_you: needs, stages, runs })
}

async fn table(conn: &mut PgConnection, title: &str, note: &str, cols: &[&str], sql: &str, tenant: &str) -> Result<ReportTable> {
    let rows = sqlx::query(sql).bind(tenant).fetch_all(&mut *conn).await?;
    let ncols = cols.len();
    let rows = rows.iter().map(|r| (0..ncols).map(|i| r.try_get::<String, _>(i).unwrap_or_else(|_| "Not available".into())).collect()).collect();
    Ok(ReportTable { title: title.into(), note: note.into(), columns: cols.iter().map(|s| s.to_string()).collect(), rows })
}

pub async fn report(app: &App, actor: &Actor, tab: &str) -> Result<ReportSet> {
    crate::app::need(actor, "report.read")?;
    if !TABS.contains(&tab) {
        return Err(Error::NotFound);
    }
    let mut tx = app.tx(actor).await?;
    let t = actor.tenant_id.as_str();
    let (mut tiles, mut tables) = (vec![], vec![]);
    match tab {
        "Leads" => {
            let total = count(&mut tx, "SELECT count(*) FROM contacts WHERE tenant_id = $1::uuid", t).await?;
            let verified = count(&mut tx, "SELECT count(*) FROM contacts WHERE tenant_id = $1::uuid AND email_status = 'verified'", t).await?;
            tiles = vec![tile("People", total), tile("Verified emails", percent(verified, total))];
            tables.push(table(&mut tx, "People by stage", "", &["Stage", "People"], "SELECT stage, count(*)::text FROM contacts WHERE tenant_id = $1::uuid GROUP BY stage ORDER BY 2 DESC", t).await?);
        }
        "Emails" => {
            let sent = count(&mut tx, "SELECT count(*) FROM emails WHERE tenant_id = $1::uuid AND direction='out' AND status='sent'", t).await?;
            let replied = count(&mut tx, "SELECT count(DISTINCT contact_id) FROM emails WHERE tenant_id = $1::uuid AND direction='in'", t).await?;
            tiles = vec![tile("Sent", sent), tile("Reply rate", percent(replied, sent))];
            tables.push(table(&mut tx, "Emails by status", "", &["Status", "Emails"], "SELECT status, count(*)::text FROM emails WHERE tenant_id = $1::uuid AND direction='out' GROUP BY status ORDER BY 2 DESC", t).await?);
        }
        "Campaigns" => tables.push(table(&mut tx, "Campaigns", "", &["Name", "Status", "People"],
            "SELECT c.name, c.status, (SELECT count(*) FROM campaign_contacts cc WHERE cc.campaign_id = c.id)::text FROM campaigns c WHERE c.tenant_id = $1::uuid ORDER BY c.created_at DESC LIMIT 50", t).await?),
        "Sales" => {
            tiles = vec![tile("Won deals", count(&mut tx, "SELECT count(*) FROM deals WHERE tenant_id = $1::uuid AND stage='won'", t).await?)];
            tables.push(table(&mut tx, "Deals by stage", "", &["Stage", "Deals"], "SELECT stage, count(*)::text FROM deals WHERE tenant_id = $1::uuid GROUP BY stage", t).await?);
        }
        "AI usage" => {
            let quick_questions = count(&mut tx, "SELECT count(*) FROM quick_messages WHERE tenant_id = $1::uuid AND role = 'user'", t).await?;
            let ai_tasks = count(&mut tx, "SELECT count(*) FROM ai_tasks WHERE tenant_id = $1::uuid", t).await?;
            tiles = vec![tile("Quick questions", quick_questions), tile("AI tasks", ai_tasks)];
            tables.push(table(&mut tx, "Claude Desktop tool calls", "Counts come from the app log.", &["Tool", "Calls"], "SELECT tool, count(*)::text FROM mcp_tool_calls WHERE tenant_id = $1::uuid GROUP BY tool ORDER BY 2 DESC", t).await?);
            tables.push(table(&mut tx, "AI tasks by status", "Includes work waiting for Claude Desktop.", &["Task", "Status", "Count"], "SELECT kind, status, count(*)::text FROM ai_tasks WHERE tenant_id = $1::uuid GROUP BY kind, status ORDER BY kind, status", t).await?);
        }
        "Activity" => tables.push(table(&mut tx, "Latest activity", "", &["When", "Who", "What"], "SELECT to_char(at, 'YYYY-MM-DD HH24:MI'), actor, summary FROM activity_logs WHERE tenant_id = $1::uuid ORDER BY id DESC LIMIT 50", t).await?),
        _ => tables.push(table(&mut tx, "People owned by employee", "", &["Employee", "People"],
            "SELECT u.name, count(c.id)::text FROM users u LEFT JOIN contacts c ON c.owner_id = u.id WHERE u.tenant_id = $1::uuid GROUP BY u.name ORDER BY 2 DESC", t).await?),
    }
    tx.commit().await?;
    Ok(ReportSet { tabs: TABS.iter().map(|s| s.to_string()).collect(), tab: tab.into(), tiles, tables })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_handles_zero() {
        assert_eq!(percent(1, 0), "Not available");
        assert_eq!(percent(1, 4), "25.0%");
    }
}
