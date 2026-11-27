//! Mandatory: one company can never see or change another company's data.
mod common;
use common::*;
use shared::{QueryRequest, SaveRecord};

#[tokio::test]
async fn company_b_cannot_read_company_a() {
    let e = env().await;
    let a = company(&e, "alpha").await;
    let b = company(&e, "beta").await;
    let id = add_person(&e, &a.admin, "Aisha", &format!("{}@acme.test", unique("a"))).await;

    let list = backend::records::list(&e.app, &b.admin, "people", &QueryRequest::default()).await.unwrap();
    assert_eq!(list.total, 0, "company B must see no people of company A");
    assert!(backend::records::get(&e.app, &b.admin, "people", &id).await.is_err());
    let upd = backend::records::update(&e.app, &b.admin, "people", &id, &SaveRecord { values: vals(&[("first_name", "Hacked")]), row_version: Some(1) }).await;
    assert!(upd.is_err(), "B must not update A's record");
    let csv = backend::records::export(&e.app, &b.admin, "people", &QueryRequest::default()).await.unwrap();
    assert!(!String::from_utf8_lossy(&csv).contains("Aisha"), "export must not leak A's rows");
    let (rec, _) = backend::records::get(&e.app, &a.admin, "people", &id).await.unwrap();
    assert_eq!(rec.values["first_name"], "Aisha", "A's record is unchanged");
}

#[tokio::test]
async fn database_refuses_rows_without_a_tenant_set() {
    let e = env().await;
    let a = company(&e, "gamma").await;
    add_person(&e, &a.admin, "Zed", &format!("{}@acme.test", unique("z"))).await;
    // Runtime role, no tenant set on the connection: row level security hides everything.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts").fetch_one(&e.app.pool).await.unwrap();
    assert_eq!(n, 0);
    // Writing a row for another tenant from a scoped connection is refused too.
    let b = company(&e, "delta").await;
    let mut tx = e.app.tx(&a.admin).await.unwrap();
    let r = sqlx::query("INSERT INTO contacts (tenant_id, first_name, last_name, email) VALUES ($1::uuid, 'X', 'Y', $2)")
        .bind(&b.tenant).bind(format!("{}@x.test", unique("w"))).execute(&mut *tx).await;
    assert!(r.is_err(), "RLS must refuse a row for another tenant");
}

#[tokio::test]
async fn activity_log_is_append_only_and_chain_holds() {
    let e = env().await;
    let a = company(&e, "epsilon").await;
    add_person(&e, &a.admin, "Una", &format!("{}@acme.test", unique("u"))).await;
    add_person(&e, &a.admin, "Dos", &format!("{}@acme.test", unique("d"))).await;
    let mut tx = e.app.tx(&a.admin).await.unwrap();
    assert!(backend::activity::verify_chain(&mut tx, &a.tenant).await.unwrap());
    let upd = sqlx::query("UPDATE activity_logs SET summary = 'changed' WHERE tenant_id = $1::uuid").bind(&a.tenant).execute(&mut *tx).await;
    assert!(upd.is_err(), "the runtime role may not update the activity log");
}
