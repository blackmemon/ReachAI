//! The outreach flow with the recording transport: gate, sample check, send once,
//! unsubscribe, pause-all, hand over. Nothing here talks to a real mail server.
mod common;
use backend::app::{App};
use backend::error::Error;
use common::*;
use sqlx::Row;

const BODY1: &str = "Hi {first_name},\n\n{personal_line} {company} in {city} probably keeps a lot of numbers in spreadsheets, and many teams there lose an hour each week copying them into reports. A short script can often prepare those reports before anyone arrives in the morning. Which report takes your team the longest to build today?\n\n{sender_name}";

struct Flow {
    c: Company,
    campaign: String,
    contact: String,
    email: String,
}

async fn setup(e: &Env, sample_done: bool) -> Flow {
    let c = company(e, "flow").await;
    let mut tx = e.app.tx(&c.admin).await.unwrap();
    for (k, v) in [("send_from_hour", "0"), ("send_to_hour", "24"), ("sample_check_done", if sample_done { "true" } else { "false" })] {
        backend::settings::set_raw(&mut tx, &c.tenant, Some(&c.admin.user_id), k, v).await.unwrap();
    }
    tx.commit().await.unwrap();
    let t = &c.tenant;
    let mb: String = sqlx::query_scalar("INSERT INTO mailboxes (tenant_id, email, display_name) VALUES ($1::uuid, $2, 'Sender') RETURNING id::text").bind(t).bind(format!("{}@acme.test", unique("mb"))).fetch_one(&e.owner).await.unwrap();
    let tpl: String = sqlx::query_scalar("INSERT INTO templates (tenant_id, name, kind, subject, body) VALUES ($1::uuid, 'E1', 'email1', 'Weekly reports at Acme', $2) RETURNING id::text").bind(t).bind(BODY1).fetch_one(&e.owner).await.unwrap();
    let seq: String = sqlx::query_scalar("INSERT INTO sequences (tenant_id, name) VALUES ($1::uuid, 'S') RETURNING id::text").bind(t).fetch_one(&e.owner).await.unwrap();
    sqlx::query("INSERT INTO sequence_steps (sequence_id, tenant_id, step_no, day_offset, template_id) VALUES ($1::uuid, $2::uuid, 1, 0, $3::uuid)").bind(&seq).bind(t).bind(&tpl).execute(&e.owner).await.unwrap();
    let aud: String = sqlx::query_scalar("INSERT INTO audiences (tenant_id, name, min_score) VALUES ($1::uuid, 'A', 0) RETURNING id::text").bind(t).fetch_one(&e.owner).await.unwrap();
    let campaign: String = sqlx::query_scalar("INSERT INTO campaigns (tenant_id, name, sequence_id, audience_id, mailbox_id) VALUES ($1::uuid, 'C', $2::uuid, $3::uuid, $4::uuid) RETURNING id::text").bind(t).bind(&seq).bind(&aud).bind(&mb).fetch_one(&e.owner).await.unwrap();
    let email = format!("{}@acme.test", unique("lead"));
    let contact = add_person(e, &c.admin, "Sana", &email).await;
    sqlx::query("UPDATE contacts SET email_status = 'verified', city = 'Karachi', research_note = 'Runs an online fabric shop.' WHERE id = $1::uuid").bind(&contact).execute(&e.owner).await.unwrap();
    sqlx::query("INSERT INTO companies (tenant_id, name, domain) VALUES ($1::uuid, 'Acme Fabrics', 'acme.test')").bind(t).execute(&e.owner).await.unwrap();
    sqlx::query("UPDATE contacts SET company_id = (SELECT id FROM companies WHERE tenant_id = $1::uuid LIMIT 1) WHERE id = $2::uuid").bind(t).bind(&contact).execute(&e.owner).await.unwrap();
    Flow { c, campaign, contact, email }
}

async fn email_status(e: &Env, f: &Flow) -> Vec<String> {
    sqlx::query("SELECT status FROM emails WHERE contact_id = $1::uuid AND direction = 'out' ORDER BY created_at").bind(&f.contact).fetch_all(&e.owner).await.unwrap().iter().map(|r| r.get(0)).collect()
}

fn sent(app: &App) -> usize {
    app.transport.sent().len()
}

#[tokio::test]
async fn campaign_cannot_start_before_the_sample_check() {
    let e = env().await;
    let f = setup(&e, false).await;
    let r = backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await;
    assert!(matches!(r, Err(Error::Conflict(_))));
}

#[tokio::test]
async fn email_is_planned_checked_and_sent_exactly_once() {
    let e = env().await;
    let f = setup(&e, true).await;
    assert_eq!(backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await.unwrap(), 1);
    assert_eq!(backend::campaigns::plan_due(&e.app, &f.c.tenant).await.unwrap(), 1, "{:?}", email_status(&e, &f).await);
    assert_eq!(email_status(&e, &f).await, vec!["queued"], "auto send is on by default");
    let before = sent(&e.app);
    assert!(backend::outbox::send_next(&e.app, &f.c.tenant).await.unwrap());
    assert_eq!(sent(&e.app), before + 1);
    let mail = e.app.transport.sent().into_iter().find(|m| m.to == f.email).expect("mail to the lead");
    assert!(mail.body.contains("unsubscribe") || mail.body.contains("Unsubscribe"), "footer has an unsubscribe line");
    assert!(!mail.body.contains('{'), "no placeholder is left");
    // Running the planner and the sender again must not send a second copy.
    backend::campaigns::plan_due(&e.app, &f.c.tenant).await.unwrap();
    let _ = backend::outbox::send_next(&e.app, &f.c.tenant).await.unwrap();
    assert_eq!(sent(&e.app), before + 1);
    assert_eq!(email_status(&e, &f).await, vec!["sent"]);
    let stage: String = sqlx::query_scalar("SELECT stage FROM contacts WHERE id = $1::uuid").bind(&f.contact).fetch_one(&e.owner).await.unwrap();
    assert_eq!(stage, "contacted");
}

#[tokio::test]
async fn pause_all_stops_sending_and_resume_sends() {
    let e = env().await;
    let f = setup(&e, true).await;
    backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await.unwrap();
    backend::campaigns::plan_due(&e.app, &f.c.tenant).await.unwrap();
    backend::campaigns::set_pause_all(&e.app, &f.c.admin, true).await.unwrap();
    let before = sent(&e.app);
    let _ = backend::outbox::send_next(&e.app, &f.c.tenant).await.unwrap();
    assert_eq!(sent(&e.app), before, "nothing leaves while paused");
    assert_eq!(email_status(&e, &f).await, vec!["queued"], "the email waits, it is not lost");
    backend::campaigns::set_pause_all(&e.app, &f.c.admin, false).await.unwrap();
    // A held email is retried after its postponed time (5 minutes); move that time up for the test.
    sqlx::query("UPDATE emails SET scheduled_at = now() WHERE contact_id = $1::uuid AND status = 'queued'").bind(&f.contact).execute(&e.owner).await.unwrap();
    assert!(backend::outbox::send_next(&e.app, &f.c.tenant).await.unwrap());
    assert_eq!(sent(&e.app), before + 1);
}

#[tokio::test]
async fn unsubscribe_reply_stops_everything_and_is_never_overridden() {
    let e = env().await;
    let f = setup(&e, true).await;
    backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await.unwrap();
    backend::campaigns::plan_due(&e.app, &f.c.tenant).await.unwrap();
    let mut tx = e.app.tx_for_tenant(&f.c.tenant).await.unwrap();
    let r = backend::conversations::receive(&mut tx, &f.c.tenant, &f.email, "Re: Quick question", "Please unsubscribe me from this list.", Some("<m1@x>")).await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(r, backend::conversations::Received::Saved { .. }));
    let before = sent(&e.app);
    let _ = backend::outbox::send_next(&e.app, &f.c.tenant).await.unwrap();
    assert_eq!(sent(&e.app), before, "an unsubscribed person gets no more email");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM suppression_list WHERE tenant_id = $1::uuid AND lower(email) = lower($2)").bind(&f.c.tenant).bind(&f.email).fetch_one(&e.owner).await.unwrap();
    assert_eq!(n, 1);
    // The same message twice is saved once.
    let mut tx = e.app.tx_for_tenant(&f.c.tenant).await.unwrap();
    let again = backend::conversations::receive(&mut tx, &f.c.tenant, &f.email, "Re: Quick question", "Please unsubscribe me from this list.", Some("<m1@x>")).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(again, backend::conversations::Received::Duplicate);
}

#[tokio::test]
async fn price_question_goes_to_a_person_not_to_the_ai() {
    let e = env().await;
    let f = setup(&e, true).await;
    backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await.unwrap();
    let mut tx = e.app.tx_for_tenant(&f.c.tenant).await.unwrap();
    backend::conversations::receive(&mut tx, &f.c.tenant, &f.email, "Re: Quick question", "How much would this cost for us?", Some("<m2@x>")).await.unwrap();
    tx.commit().await.unwrap();
    let st: String = sqlx::query_scalar("SELECT status FROM conversations WHERE contact_id = $1::uuid").bind(&f.contact).fetch_one(&e.owner).await.unwrap();
    assert_eq!(st, "handover");
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_tasks WHERE tenant_id = $1::uuid AND kind = 'write_reply'").bind(&f.c.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(tasks, 0);
}

#[tokio::test]
async fn interested_reply_waits_for_the_employee_then_asks_the_ai() {
    let e = env().await;
    let f = setup(&e, true).await;
    backend::campaigns::start(&e.app, &f.c.admin, &f.campaign).await.unwrap();
    let mut tx = e.app.tx_for_tenant(&f.c.tenant).await.unwrap();
    backend::conversations::receive(&mut tx, &f.c.tenant, &f.email, "Re: Quick question", "Sounds good, tell me more.", Some("<m3@x>")).await.unwrap();
    tx.commit().await.unwrap();
    // Inside the 15 minute window the AI is not asked.
    assert_eq!(backend::conversations::timers_due(&e.app, &f.c.tenant).await.unwrap(), 0);
    sqlx::query("UPDATE conversations SET timer_due_at = now() - interval '1 minute' WHERE contact_id = $1::uuid").bind(&f.contact).execute(&e.owner).await.unwrap();
    assert_eq!(backend::conversations::timers_due(&e.app, &f.c.tenant).await.unwrap(), 1);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_tasks WHERE tenant_id = $1::uuid AND kind = 'write_reply' AND status = 'waiting'").bind(&f.c.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(tasks, 1);
}
