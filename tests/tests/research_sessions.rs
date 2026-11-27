//! AI research conversations: a fake Claude CLI that prints real stream-json events stands in for
//! the real one, so the whole flow (run, live events, checks, duplicates, follow-up, stop,
//! one run at a time, export, save, privacy between people) runs against a real PostgreSQL.
mod common;
use backend::error::Error;
use backend::research::session;
use common::*;
use shared::{ResearchExport, ResearchSend};
use std::os::unix::fs::PermissionsExt;
use std::sync::Once;
use std::time::Duration;

static FAKE: Once = Once::new();
/// The app allows two research runs at once for the whole process; these tests take turns.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// One fake CLI for every test in this file. It answers by what the prompt asks for.
fn fake_cli() {
    FAKE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("fake-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        let script = r#"#!/usr/bin/env python3
import sys, json, time
prompt = sys.argv[-1]
def out(o):
    print(json.dumps(o), flush=True)
out({"type": "system", "subtype": "init"})
out({"type": "assistant", "message": {"content": [{"type": "text", "text": "Looking.\nFound: Luigi's"}]}})
out({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "WebSearch", "input": {"query": "pizza"}}]}})
out({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "WebFetch", "input": {"url": "https://luigis.test"}}]}})
if "STUCK" in prompt.split("Latest request from the user:")[-1]:
    time.sleep(60)
def lead(name, site, email="info@luigis.test", phone="+1 212 555 0101"):
    return {"name": name, "category": "Pizza", "website": site, "address": "1 Main St", "location": "New York, NY", "phone": phone, "email": email,
            "socials": ["https://instagram.com/x", "nope"], "source_url": "https://luigis.test", "verification": "verified", "notes": "",
            "services": "Dine-in", "pricing": "$$", "reviews": "4.5 on Google", "presence": "Site", "offerings": "Pasta"}
results = [lead("Luigi's", "https://luigis.test"), lead("Luigis Pizza NYC", "http://www.luigis.test/menu"),
           lead("Bella", "", "bad email", "12"), lead("", "https://noname.test")]
out({"type": "result", "subtype": "success", "is_error": False, "structured_output": {"answer": "Here you go.", "results": results}})
"#;
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Set once, before any test in this file starts a run.
        std::env::set_var("CLAUDE_CLI", &path);
    });
}

async fn wait_done(e: &Env, a: &auth::Actor, run: &str) -> shared::ResearchRunInfo {
    let mut after = 0;
    for _ in 0..200 {
        let p = session::poll(&e.app, a, run, after).await.unwrap();
        if let Some(last) = p.events.last() {
            after = last.id;
        }
        if p.run.status != "running" {
            return p.run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("run did not finish");
}

fn ask(mode: &str, sid: &str, text: &str) -> ResearchSend {
    ResearchSend { session_id: sid.into(), mode: mode.into(), text: text.into() }
}

#[tokio::test]
async fn a_run_saves_checked_results_without_duplicates_and_follow_ups_keep_the_session() {
    let _turn = TURN.lock().await;
    fake_cli();
    let e = env().await;
    let c = company(&e, "research").await;
    let started = session::send(&e.app, &c.admin, &ask("leads", "", "Find pizza leads in New York")).await.unwrap();
    let run = wait_done(&e, &c.admin, &started.run_id).await;
    assert_eq!(run.status, "done", "{run:?}");
    // 4 raw results: one duplicate of the first (same site), one without a name, so 2 are kept.
    assert_eq!((run.found, run.duplicates), (2, 1));

    let view = session::get(&e.app, &c.admin, &started.session_id).await.unwrap();
    let names: Vec<&str> = view.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Luigi's", "Bella"]);
    let bella = &view.results[1];
    assert_eq!((bella.email.as_str(), bella.phone.as_str()), ("", ""), "invalid email and phone are dropped, not guessed");
    assert_eq!(view.results[0].socials, "https://instagram.com/x", "an invalid profile link is dropped");
    assert!(view.events.iter().any(|ev| ev.kind == "site" && ev.body.contains("luigis.test")), "pages read are reported");
    assert!(view.events.iter().any(|ev| ev.kind == "lead"), "businesses found are reported");
    assert_eq!(view.messages.len(), 2);
    assert!(view.messages[1].body.contains("Added 2 businesses"));

    // A follow-up stays in the same conversation and finds nothing new (all duplicates).
    let again = session::send(&e.app, &c.admin, &ask("leads", &started.session_id, "Find more")).await.unwrap();
    assert_eq!(again.session_id, started.session_id);
    let run2 = wait_done(&e, &c.admin, &again.run_id).await;
    assert_eq!(run2.found, 0);
    let view = session::get(&e.app, &c.admin, &started.session_id).await.unwrap();
    assert_eq!((view.results.len(), view.messages.len()), (2, 4));
    assert_eq!(session::list(&e.app, &c.admin, "leads").await.unwrap().len(), 1);
    assert!(session::list(&e.app, &c.admin, "competitors").await.unwrap().is_empty(), "each mode keeps its own conversations");
}

#[tokio::test]
async fn only_one_run_at_a_time_and_a_run_can_be_stopped() {
    let _turn = TURN.lock().await;
    fake_cli();
    let e = env().await;
    let c = company(&e, "stop").await;
    let started = session::send(&e.app, &c.admin, &ask("leads", "", "STUCK search")).await.unwrap();
    let busy = session::send(&e.app, &c.admin, &ask("leads", &started.session_id, "second")).await;
    assert!(matches!(busy, Err(Error::Conflict(_))), "a second message while running is refused");
    session::cancel(&e.app, &c.admin, &started.run_id).await.unwrap();
    let run = wait_done(&e, &c.admin, &started.run_id).await;
    assert_eq!(run.status, "cancelled");
    // The conversation is free again.
    let next = session::send(&e.app, &c.admin, &ask("leads", &started.session_id, "Find pizza")).await.unwrap();
    assert_eq!(wait_done(&e, &c.admin, &next.run_id).await.status, "done");
}

#[tokio::test]
async fn results_export_to_csv_and_excel_and_are_saved_for_review() {
    let _turn = TURN.lock().await;
    fake_cli();
    let e = env().await;
    let c = company(&e, "export").await;
    let started = session::send(&e.app, &c.admin, &ask("competitors", "", "Find competitors of Luigi's")).await.unwrap();
    wait_done(&e, &c.admin, &started.run_id).await;
    let view = session::get(&e.app, &c.admin, &started.session_id).await.unwrap();
    assert_eq!(view.results[0].pricing, "$$", "competitor details are kept");

    let (csv, mime, name) = session::export(&e.app, &c.admin, &started.session_id, &ResearchExport { format: "csv".into(), ids: vec![] }).await.unwrap();
    let text = String::from_utf8(csv).unwrap();
    assert!(mime.starts_with("text/csv") && name == "competitors.csv");
    assert!(text.contains("Pricing") && text.contains("Luigi's"));
    let (xlsx, _, name) = session::export(&e.app, &c.admin, &started.session_id, &ResearchExport { format: "xlsx".into(), ids: vec![view.results[0].id.clone()] }).await.unwrap();
    assert!(name.ends_with(".xlsx") && xlsx.starts_with(b"PK"));
    let bad = session::export(&e.app, &c.admin, &started.session_id, &ResearchExport { format: "pdf".into(), ids: vec![] }).await;
    assert!(matches!(bad, Err(Error::BadRequest(_))));

    // Mark as competitors in Companies; doing it twice adds nothing new.
    let ids: Vec<String> = view.results.iter().map(|r| r.id.clone()).collect();
    let first = session::save(&e.app, &c.admin, &started.session_id, &ids).await.unwrap();
    assert!(first.starts_with("2 marked"), "{first}");
    let second = session::save(&e.app, &c.admin, &started.session_id, &ids).await.unwrap();
    assert!(second.starts_with("0 marked") && second.contains("2 already existed"), "{second}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM companies WHERE tenant_id = $1::uuid AND is_competitor").bind(&c.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn conversations_are_private_to_their_owner_and_to_their_company() {
    let _turn = TURN.lock().await;
    fake_cli();
    let e = env().await;
    let c = company(&e, "privacy").await;
    let other_company = company(&e, "privacy-other").await;
    let colleague = user(&e, &c, "employee").await;
    let started = session::send(&e.app, &c.admin, &ask("leads", "", "Find pizza")).await.unwrap();
    wait_done(&e, &c.admin, &started.run_id).await;
    for who in [&colleague, &other_company.admin] {
        assert!(matches!(session::get(&e.app, who, &started.session_id).await, Err(Error::NotFound)));
        assert!(matches!(session::poll(&e.app, who, &started.run_id, 0).await, Err(Error::NotFound)));
        assert!(matches!(session::export(&e.app, who, &started.session_id, &ResearchExport { format: "csv".into(), ids: vec![] }).await, Err(Error::NotFound)));
        assert!(matches!(session::send(&e.app, who, &ask("leads", &started.session_id, "hi")).await, Err(Error::NotFound)));
    }
    let viewer = user(&e, &c, "viewer").await;
    assert!(matches!(session::send(&e.app, &viewer, &ask("leads", "", "x")).await, Err(Error::Forbidden)));
    assert!(matches!(session::send(&e.app, &c.admin, &ask("leads", "", "   ")).await, Err(Error::BadRequest(_))));
    assert!(matches!(session::send(&e.app, &c.admin, &ask("nonsense", "", "x")).await, Err(Error::BadRequest(_))));
}
