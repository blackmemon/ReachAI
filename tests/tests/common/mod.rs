#![allow(dead_code)]
//! Shared helpers. The test database is rebuilt once per test file, then every test
//! makes its own companies so tests can run side by side.

use auth::Actor;
use backend::app::{App, DomainCheck, Transport};
use configs::Config;
use std::collections::BTreeMap;
use std::sync::Once;

pub const OWNER_URL: &str = "postgres://leads:leads@127.0.0.1:5433/leads_test";
pub const APP_URL: &str = "postgres://leads_app:leads_app@127.0.0.1:5433/leads_test";
pub const PASSWORD: &str = "correct horse battery 1";

static SCHEMA: Once = Once::new();

pub fn ensure_schema() {
    SCHEMA.call_once(|| {
        std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let pool = database::connect(OWNER_URL, 1).await.expect("owner connect");
                sqlx::query("DROP SCHEMA public CASCADE").execute(&pool).await.unwrap();
                sqlx::query("CREATE SCHEMA public").execute(&pool).await.unwrap();
                pool.close().await;
                database::migrate(OWNER_URL).await.expect("migrate");
            });
        })
        .join()
        .unwrap();
    });
}

pub fn config() -> Config {
    Config {
        database_url: APP_URL.into(),
        listen: "127.0.0.1:0".into(),
        secret_key: "test-secret-key-0123456789".into(),
        cookie_secure: false,
        base_url: "http://127.0.0.1:8000".into(),
        dist_dir: "dist".into(),
        session_hard_seconds: 24 * 3600,
        session_idle_seconds: 20 * 60,
        smtp_live: false,
    }
}

pub struct Env {
    pub app: App,
    pub owner: database::Pool,
}

pub async fn env() -> Env {
    ensure_schema();
    let pool = database::connect(APP_URL, 5).await.expect("app connect");
    let owner = database::connect(OWNER_URL, 2).await.expect("owner connect");
    let mut app = App::new(pool, config());
    app.transport = Transport::recording();
    app.domains = DomainCheck::Fake(vec!["acme.test".into(), "other.test".into()]);
    Env { app, owner }
}

pub fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

pub struct Company {
    pub tenant: String,
    pub admin: Actor,
}

pub async fn company(e: &Env, name: &str) -> Company {
    let email = format!("{}@{}.test", unique("admin"), name);
    let (tenant, uid) = backend::accounts::create_tenant(&e.owner, name, &email, "Admin", PASSWORD).await.unwrap();
    Company { tenant: tenant.clone(), admin: Actor { user_id: uid, tenant_id: tenant, role: "admin".into(), name: "Admin".into(), email } }
}

pub async fn user(e: &Env, c: &Company, role: &str) -> Actor {
    let email = format!("{}@x.test", unique(role));
    let id = backend::accounts::create_user(&e.app, &c.admin, &email, role, role, PASSWORD).await.unwrap();
    Actor { user_id: id, tenant_id: c.tenant.clone(), role: role.into(), name: role.into(), email }
}

pub fn vals(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

pub async fn add_person(e: &Env, a: &Actor, first: &str, email: &str) -> String {
    backend::records::create(&e.app, a, "people", &vals(&[("first_name", first), ("last_name", "Test"), ("email", email)])).await.unwrap().id
}
