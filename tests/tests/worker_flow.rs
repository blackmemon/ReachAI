//! The worker's tick must plan and send through the same rules, and must be safe to run twice.
mod common;
use common::*;

#[tokio::test]
async fn tick_sends_once_even_when_run_twice() {
    let e = env().await;
    let c = company(&e, "tick").await;
    let mut tx = e.app.tx(&c.admin).await.unwrap();
    for (k, v) in [("send_from_hour", "0"), ("send_to_hour", "24"), ("sample_check_done", "true")] {
        backend::settings::set_raw(&mut tx, &c.tenant, Some(&c.admin.user_id), k, v).await.unwrap();
    }
    tx.commit().await.unwrap();
    // Demo data gives templates, a sequence, an audience, a mailbox and a campaign.
    backend::seed::run(&e.owner, Some(&c.tenant)).await.unwrap();
    let campaign: String = sqlx::query_scalar("SELECT id::text FROM campaigns WHERE tenant_id = $1::uuid").bind(&c.tenant).fetch_one(&e.owner).await.unwrap();
    // The seed people have verified emails on .example domains: allow them in the fake domain check.
    let mut app = e.app.clone();
    app.domains = backend::app::DomainCheck::Fake(vec!["sindhfabrics.example".into(), "lahorelogistics.example".into(), "indusdental.example".into(), "punjabagro.example".into(), "margallasoft.example".into(), "cliftonretail.example".into(), "ravisteel.example".into(), "harbourfreight.example".into()]);
    sqlx::query("UPDATE contacts SET research_note = 'Runs a busy local business.' WHERE tenant_id = $1::uuid").bind(&c.tenant).execute(&e.owner).await.unwrap();
    sqlx::query("UPDATE mailboxes SET daily_limit = 30 WHERE tenant_id = $1::uuid").bind(&c.tenant).execute(&e.owner).await.unwrap();
    let started = backend::campaigns::start(&app, &c.admin, &campaign).await.unwrap();
    assert_eq!(started, 6, "the 6 verified seed people join (2 are unverified)");
    let first = worker::tick::tick(&app).await;
    assert_eq!(first.errors, 0);
    let after_first = app.transport.sent().len();
    let second = worker::tick::tick(&app).await;
    assert_eq!(second.errors, 0);
    assert_eq!(app.transport.sent().len(), after_first, "a second tick must not send the same emails again");
    let out: i64 = sqlx::query_scalar("SELECT count(*) FROM emails WHERE tenant_id = $1::uuid AND direction = 'out' AND status = 'sent'").bind(&c.tenant).fetch_one(&e.owner).await.unwrap();
    assert!(out >= 1 && out <= 6, "sent {out}");
}
