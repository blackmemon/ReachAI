//! Roles, mass assignment, optimistic lock, CSV import.
mod common;
use backend::error::Error;
use common::*;
use shared::{Filter, FilterOp, QueryRequest, SaveRecord};

#[tokio::test]
async fn viewer_cannot_write_and_employee_sees_only_own_people() {
    let e = env().await;
    let c = company(&e, "rights").await;
    let viewer = user(&e, &c, "viewer").await;
    let r = backend::records::create(&e.app, &viewer, "people", &vals(&[("first_name", "V"), ("last_name", "T"), ("email", "v@acme.test")])).await;
    assert!(matches!(r, Err(Error::Forbidden)));

    let emp1 = user(&e, &c, "employee").await;
    let emp2 = user(&e, &c, "employee").await;
    let mine = add_person(&e, &emp1, "Mine", &format!("{}@acme.test", unique("m"))).await;
    add_person(&e, &emp2, "Theirs", &format!("{}@acme.test", unique("t"))).await;
    let list = backend::records::list(&e.app, &emp1, "people", &QueryRequest::default()).await.unwrap();
    assert_eq!(list.total, 1);
    assert_eq!(list.rows[0].id, mine);
    let admin_list = backend::records::list(&e.app, &c.admin, "people", &QueryRequest::default()).await.unwrap();
    assert_eq!(admin_list.total, 2);
}

#[tokio::test]
async fn mass_assignment_is_refused() {
    let e = env().await;
    let c = company(&e, "mass").await;
    for field in ["owner_id", "score", "tenant_id", "row_version", "stage"] {
        let r = backend::records::create(&e.app, &c.admin, "people", &vals(&[("first_name", "A"), ("last_name", "B"), ("email", &format!("{}@acme.test", unique("x"))), (field, "1")])).await;
        assert!(r.is_err(), "field {field} must not be accepted from the client");
    }
}

#[tokio::test]
async fn second_edit_with_old_version_conflicts() {
    let e = env().await;
    let c = company(&e, "lock").await;
    let id = add_person(&e, &c.admin, "Lock", &format!("{}@acme.test", unique("l"))).await;
    let (_, v) = backend::records::get(&e.app, &c.admin, "people", &id).await.unwrap();
    let first = backend::records::update(&e.app, &c.admin, "people", &id, &SaveRecord { values: vals(&[("job_title", "CEO")]), row_version: Some(v) }).await.unwrap();
    assert_eq!(first.row_version, v + 1);
    let second = backend::records::update(&e.app, &c.admin, "people", &id, &SaveRecord { values: vals(&[("job_title", "CTO")]), row_version: Some(v) }).await;
    assert!(matches!(second, Err(Error::Conflict(_))));
}

#[tokio::test]
async fn duplicate_email_is_refused_with_a_clear_message() {
    let e = env().await;
    let c = company(&e, "dup").await;
    let email = format!("{}@acme.test", unique("d"));
    add_person(&e, &c.admin, "One", &email).await;
    let r = backend::records::create(&e.app, &c.admin, "people", &vals(&[("first_name", "Two"), ("last_name", "T"), ("email", &email)])).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn people_and_company_multiple_values_round_trip_and_filter() {
    let e = env().await;
    let c = company(&e, "multi").await;
    let email = format!("{}@acme.test", unique("primary"));
    let alternate = format!("{}@acme.test", unique("alternate"));
    let person = backend::records::create(
        &e.app,
        &c.admin,
        "people",
        &vals(&[
            ("name", "Ada Lovelace"),
            ("job_title", "Engineer"),
            ("emails", &format!("{email}\n{alternate}")),
            ("phone_numbers", "+1 555 0100\n+1 555 0101"),
            ("links", "example.com\nhttps://linkedin.com/in/ada"),
            ("city", "London"),
            ("keywords", "math\ncomputing"),
        ]),
    )
    .await
    .unwrap()
    .id;
    let (record, _) = backend::records::get(&e.app, &c.admin, "people", &person).await.unwrap();
    assert_eq!(record.values["name"], "Ada Lovelace");
    assert_eq!(record.values["emails"], format!("{email}\n{alternate}"));
    let primary: Option<String> = sqlx::query_scalar("SELECT email FROM contacts WHERE id = $1::uuid")
        .bind(&person).fetch_one(&e.owner).await.unwrap();
    assert_eq!(primary.as_deref(), Some(email.as_str()));
    let mut query = QueryRequest::default();
    query.filters.push(Filter { field: "emails".into(), op: FilterOp::Contains, value: alternate });
    assert_eq!(backend::records::list(&e.app, &c.admin, "people", &query).await.unwrap().total, 1);

    let company = backend::records::create(
        &e.app,
        &c.admin,
        "companies",
        &vals(&[
            ("name", "Example Systems"),
            ("emails", "hello@example.com\nsales@example.com"),
            ("phone_numbers", "+1 555 0200\n+1 555 0201"),
            ("links", "https://example.com\nhttps://x.com/example"),
            ("employees", "42"),
            ("industries", "Software\nAnalytics"),
            ("keywords", "SaaS\nAI"),
            ("city", "London"),
        ]),
    )
    .await
    .unwrap()
    .id;
    let (record, _) = backend::records::get(&e.app, &c.admin, "companies", &company).await.unwrap();
    assert_eq!(record.values["employees"], "42");
    assert_eq!(record.values["industries"], "Software\nAnalytics");
    let mut query = QueryRequest::default();
    query.filters.push(Filter { field: "industries".into(), op: FilterOp::Contains, value: "Analytics".into() });
    query.filters.push(Filter { field: "employees".into(), op: FilterOp::Eq, value: "42".into() });
    assert_eq!(backend::records::list(&e.app, &c.admin, "companies", &query).await.unwrap().total, 1);
}

#[tokio::test]
async fn csv_import_all_or_nothing_and_no_duplicates() {
    let e = env().await;
    let c = company(&e, "csv").await;
    let good = format!("First name,Last name,Email\nAmna,Khan,{}@acme.test\nBilal,Shah,{}@acme.test\n", unique("a"), unique("b"));
    let r = backend::imports::run(&e.app, &c.admin, "people", good.as_bytes()).await.unwrap();
    assert_eq!((r.added, r.skipped_duplicates, r.errors.len()), (2, 0, 0));
    // Same file again: nothing new.
    let again = backend::imports::run(&e.app, &c.admin, "people", good.as_bytes()).await.unwrap();
    assert_eq!((again.added, again.skipped_duplicates), (0, 2));
    // One bad row: nothing is saved.
    let bad = format!("First name,Last name,Email\nOk,Row,{}@acme.test\nBad,Row,not-an-email\n", unique("ok"));
    let r = backend::imports::run(&e.app, &c.admin, "people", bad.as_bytes()).await.unwrap();
    assert_eq!(r.added, 0);
    assert!(!r.errors.is_empty());
    let all = backend::records::list(&e.app, &c.admin, "people", &QueryRequest::default()).await.unwrap();
    assert_eq!(all.total, 2);
    // Unknown column is refused.
    assert!(backend::imports::run(&e.app, &c.admin, "people", b"Password\nx\n").await.is_err());
}

#[tokio::test]
async fn csv_export_blocks_formula_injection() {
    let e = env().await;
    let c = company(&e, "formula").await;
    backend::records::create(&e.app, &c.admin, "people", &vals(&[("first_name", "=HYPERLINK(\"http://evil\")"), ("last_name", "T"), ("email", &format!("{}@acme.test", unique("f")))])).await.unwrap();
    let csv = String::from_utf8(backend::records::export(&e.app, &c.admin, "people", &QueryRequest::default()).await.unwrap()).unwrap();
    assert!(!csv.contains(",=HYPERLINK") && !csv.contains("\n=HYPERLINK"), "cell must be neutralised: {csv}");
}
