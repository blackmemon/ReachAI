//! Tests through real HTTP against the built gateway binary.
//! Build first: `cargo build -p gateway`. The tests start the binary on a free port.
mod common;
use common::*;
use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Server {
    child: Child,
    port: u16,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn binary() -> String {
    let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    format!("{dir}/debug/blackmemon-leads")
}

async fn start() -> Server {
    ensure_schema();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let child = Command::new(binary())
        .env("DATABASE_URL", APP_URL)
        .env("LEADS_LISTEN", format!("127.0.0.1:{port}"))
        .env("LEADS_SECRET_KEY", "test-secret-key-0123456789")
        .env("LEADS_COOKIE_SECURE", "false")
        .env("LEADS_BASE_URL", format!("http://127.0.0.1:{port}"))
        .env("LEADS_DIST", "/nonexistent")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("run `cargo build -p gateway` first");
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Server { child, port };
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("server did not start");
}

#[derive(Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}
impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    fn cookies(&self) -> Vec<String> {
        self.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie")).map(|(_, v)| v.clone()).collect()
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Default)]
struct Client {
    jar: BTreeMap<String, String>,
}

impl Client {
    async fn call(&mut self, s: &Server, method: &str, path: &str, headers: &[(&str, &str)], body: Option<(&str, &str)>) -> Reply {
        let mut stream = TcpStream::connect(("127.0.0.1", s.port)).await.unwrap();
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n", s.port);
        if !self.jar.is_empty() {
            let c: Vec<String> = self.jar.iter().map(|(k, v)| format!("{k}={v}")).collect();
            req.push_str(&format!("Cookie: {}\r\n", c.join("; ")));
        }
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        let payload = body.map(|b| b.1.to_string()).unwrap_or_default();
        if let Some((ct, _)) = body {
            req.push_str(&format!("Content-Type: {ct}\r\n"));
        }
        if method != "GET" {
            req.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        req.push_str("\r\n");
        req.push_str(&payload);
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let mut lines = head.lines();
        let status: u16 = lines.next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
        let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(": ")).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let reply = Reply { status, headers, body: body.to_string() };
        for c in reply.cookies() {
            if let Some((kv, _)) = c.split_once(';') {
                if let Some((k, v)) = kv.split_once('=') {
                    if v.is_empty() { self.jar.remove(k); } else { self.jar.insert(k.into(), v.into()); }
                }
            }
        }
        reply
    }

    async fn login(&mut self, s: &Server, email: &str) -> String {
        let r = self.call(s, "GET", "/api/v1/auth/csrf", &[], None).await;
        let token = r.json()["token"].as_str().unwrap().to_string();
        let r = self.call(s, "POST", "/api/v1/auth/login", &[("X-CSRF-Token", &token)], Some(("application/json", &format!(r#"{{"email":"{email}","password":"{PASSWORD}"}}"#)))).await;
        assert_eq!(r.status, 200, "{}", r.body);
        r.json()["csrf"].as_str().unwrap().to_string()
    }
}

const JSON: &str = "application/json";

#[tokio::test]
async fn unauthenticated_calls_are_refused_and_headers_are_set() {
    let e = env().await;
    let _ = company(&e, "http1").await;
    let s = start().await;
    let mut c = Client::default();
    let r = c.call(&s, "GET", "/api/v1/session", &[], None).await;
    assert_eq!(r.status, 401);
    assert_eq!(r.json()["code"], "unauthorized");
    let csp = r.header("content-security-policy").unwrap();
    assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval'") && csp.contains("frame-ancestors 'none'"));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    for p in ["/api/v1/modules", "/api/v1/today", "/api/v1/records/people/query"] {
        let r = c.call(&s, if p.ends_with("query") { "POST" } else { "GET" }, p, &[], if p.ends_with("query") { Some((JSON, "{}")) } else { None }).await;
        assert_eq!(r.status, 401, "{p}");
    }
    assert_eq!(c.call(&s, "GET", "/health", &[], None).await.status, 200);
    assert_eq!(c.call(&s, "GET", "/ready", &[], None).await.status, 200);
    // Pages are one generated loader; the signed-in bundle is not given to visitors.
    let page = c.call(&s, "GET", "/people", &[], None).await;
    assert!(page.body.contains("/assets/public/boot.js") && !page.body.contains("<script>"));
    assert_eq!(c.call(&s, "GET", "/assets/app/boot.js", &[], None).await.status, 401);
    assert_eq!(c.call(&s, "GET", "/assets/public/..%2f..%2fetc%2fpasswd", &[], None).await.status, 404);
    assert_eq!(c.call(&s, "GET", "/assets/public/../../etc/passwd", &[], None).await.status, 404);
}

#[tokio::test]
async fn login_needs_csrf_cookies_are_safe_and_changes_need_the_session_token() {
    let e = env().await;
    let co = company(&e, "http2").await;
    let s = start().await;
    let mut c = Client::default();
    let body = format!(r#"{{"email":"{}","password":"{PASSWORD}"}}"#, co.admin.email);
    let r = c.call(&s, "POST", "/api/v1/auth/login", &[], Some((JSON, &body))).await;
    assert_eq!(r.status, 403, "login without a CSRF token");
    let csrf = c.login(&s, &co.admin.email).await;
    let r = c.call(&s, "GET", "/api/v1/session", &[], None).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["role"], "admin");
    // Cookie flags (plain http local run: no Secure, no __Host-).
    let r2 = Client::default().call(&s, "GET", "/api/v1/auth/csrf", &[], None).await;
    let cookie = &r2.cookies()[0];
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"), "{cookie}");
    // A change without the token is refused, with it works.
    let create = r#"{"values":{"first_name":"Hina","last_name":"Rao","email":"hina@acme.test"}}"#;
    let r = c.call(&s, "POST", "/api/v1/records/people", &[], Some((JSON, create))).await;
    assert_eq!(r.status, 403);
    let r = c.call(&s, "POST", "/api/v1/records/people", &[("X-CSRF-Token", "wrong")], Some((JSON, create))).await;
    assert_eq!(r.status, 403);
    let r = c.call(&s, "POST", "/api/v1/records/people", &[("X-CSRF-Token", &csrf), ("Origin", "http://evil.example")], Some((JSON, create))).await;
    assert_eq!(r.status, 403, "foreign origin");
    let r = c.call(&s, "POST", "/api/v1/records/people", &[("X-CSRF-Token", &csrf)], Some((JSON, create))).await;
    assert_eq!(r.status, 201, "{}", r.body);
    // Wrong content type is refused.
    let r = c.call(&s, "POST", "/api/v1/records/people", &[("X-CSRF-Token", &csrf)], Some(("text/plain", create))).await;
    assert_eq!(r.status, 400);
    // Logout ends the session.
    let r = c.call(&s, "POST", "/api/v1/auth/logout", &[("X-CSRF-Token", &csrf)], Some((JSON, "{}"))).await;
    assert_eq!(r.status, 200);
    let stale = r#"x"#;
    let _ = stale;
    assert_eq!(c.call(&s, "GET", "/api/v1/session", &[], None).await.status, 401);
}

#[tokio::test]
async fn other_company_gets_not_found_over_http_and_injection_is_harmless() {
    let e = env().await;
    let a = company(&e, "http3a").await;
    let b = company(&e, "http3b").await;
    let id = add_person(&e, &a.admin, "Secret", &format!("{}@acme.test", unique("s"))).await;
    let s = start().await;
    let mut cb = Client::default();
    let csrf = cb.login(&s, &b.admin.email).await;
    assert_eq!(cb.call(&s, "GET", &format!("/api/v1/records/people/{id}"), &[], None).await.status, 404);
    let r = cb.call(&s, "PUT", &format!("/api/v1/records/people/{id}"), &[("X-CSRF-Token", &csrf)], Some((JSON, r#"{"values":{"first_name":"X"},"row_version":1}"#))).await;
    assert_eq!(r.status, 404);
    // SQL injection in search, sort and filter never reaches SQL as text.
    for q in [
        r#"{"search":"' OR 1=1 --"}"#,
        r#"{"sort_field":"first_name; DROP TABLE contacts"}"#,
        r#"{"filters":[{"field":"email) OR (1=1","op":"eq","value":"x"}]}"#,
    ] {
        let r = cb.call(&s, "POST", "/api/v1/records/people/query", &[("X-CSRF-Token", &csrf)], Some((JSON, q))).await;
        assert!(r.status == 200 || r.status == 400, "{q} -> {}", r.status);
        if r.status == 200 {
            assert_eq!(r.json()["total"], 0);
        }
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts WHERE tenant_id = $1::uuid").bind(&a.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(n, 1, "table still there and untouched");
    // Unknown module and ids that are not ids.
    assert_eq!(cb.call(&s, "GET", "/api/v1/records/nope/00000000-0000-0000-0000-000000000000", &[], None).await.status, 404);
    assert_eq!(cb.call(&s, "GET", "/api/v1/records/people/not-an-id", &[], None).await.status, 404);
}

#[tokio::test]
async fn unsubscribe_link_checks_on_get_and_acts_on_post() {
    let e = env().await;
    let a = company(&e, "http4").await;
    let email = format!("{}@acme.test", unique("u"));
    let id = add_person(&e, &a.admin, "Una", &email).await;
    let s = start().await;
    let url = backend::outbox::unsubscribe_url(&e.app, &a.tenant, &id);
    let token = url.rsplit('/').next().unwrap().to_string();
    let mut c = Client::default();
    assert_eq!(c.call(&s, "GET", &format!("/api/v1/public/unsubscribe/{token}"), &[], None).await.status, 200);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM suppression_list WHERE tenant_id = $1::uuid").bind(&a.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(n, 0, "a link scanner opening the page must not unsubscribe anyone");
    assert_eq!(c.call(&s, "POST", &format!("/api/v1/public/unsubscribe/{token}"), &[], None).await.status, 200);
    assert_eq!(c.call(&s, "POST", &format!("/unsubscribe/{token}"), &[], None).await.status, 200, "one-click and repeat are fine");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM suppression_list WHERE tenant_id = $1::uuid AND email = $2").bind(&a.tenant).bind(&email).fetch_one(&e.owner).await.unwrap();
    assert_eq!(n, 1);
    // A forged or edited token is refused.
    let forged = token.replace(&id, "00000000-0000-0000-0000-000000000000");
    assert_eq!(c.call(&s, "POST", &format!("/api/v1/public/unsubscribe/{forged}"), &[], None).await.status, 404);
    assert_eq!(c.call(&s, "GET", "/api/v1/public/unsubscribe/garbage", &[], None).await.status, 404);
}

#[tokio::test]
async fn mcp_needs_a_token_lists_tools_and_records_a_heartbeat() {
    let e = env().await;
    let a = company(&e, "http5").await;
    let s = start().await;
    let mut c = Client::default();
    let r = c.call(&s, "POST", "/api/v1/mcp", &[("Authorization", "Bearer nope")], Some((JSON, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#))).await;
    assert_eq!(r.status, 401);
    let csrf = c.login(&s, &a.admin.email).await;
    let r = c.call(&s, "POST", "/api/v1/mcp-tokens", &[("X-CSRF-Token", &csrf)], Some((JSON, r#"{"label":"Claude Desktop"}"#))).await;
    assert_eq!(r.status, 201);
    let token = r.json()["token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {token}");
    let mut anon = Client::default();
    let r = anon.call(&s, "POST", "/api/v1/mcp", &[("Authorization", &bearer)], Some((JSON, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#))).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["result"]["tools"].as_array().unwrap().len(), 6);
    let r = anon.call(&s, "POST", "/api/v1/mcp", &[("Authorization", &bearer)], Some((JSON, r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_ai_tasks","arguments":{}}}"#))).await;
    assert_eq!(r.json()["result"]["isError"], false, "{}", r.body);
    let hb: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_heartbeats WHERE tenant_id = $1::uuid").bind(&a.tenant).fetch_one(&e.owner).await.unwrap();
    assert_eq!(hb, 1, "list_ai_tasks counts as a check");
    // The token is stored only as a hash.
    let plain: i64 = sqlx::query_scalar("SELECT count(*) FROM mcp_tokens WHERE token_hash = $1").bind(&token).fetch_one(&e.owner).await.unwrap();
    assert_eq!(plain, 0);
    // A tool that does not exist is an error result, not a crash.
    let r = anon.call(&s, "POST", "/api/v1/mcp", &[("Authorization", &bearer)], Some((JSON, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"delete_everything","arguments":{}}}"#))).await;
    assert_eq!(r.json()["result"]["isError"], true);
}

/// Every screen the app opens must answer for a signed-in admin, with real rows in the tables.
/// A page that returns an error here is a page the person sees broken.
#[tokio::test]
async fn every_screen_the_app_opens_answers_for_a_signed_in_admin() {
    let e = env().await;
    let co = company(&e, "http_screens").await;
    // A person, a company, and a job that failed for good: the Today page reads all of them.
    let a = co.admin.clone();
    let _ = add_person(&e, &a, "Sana", "sana@acme.test").await;
    sqlx::query("INSERT INTO jobs (tenant_id, kind, status, last_error) VALUES ($1::uuid, 'send_email', 'dead', 'Mailbox refused the login')")
        .bind(&co.tenant)
        .execute(&e.owner)
        .await
        .unwrap();
    let s = start().await;
    let mut c = Client::default();
    let csrf = c.login(&s, &co.admin.email).await;

    let today = c.call(&s, "GET", "/api/v1/today", &[], None).await;
    assert_eq!(today.status, 200, "{}", today.body);
    assert!(today.body.contains("failed for good"), "the failed job shows on Today: {}", today.body);

    let modules = c.call(&s, "GET", "/api/v1/modules", &[], None).await;
    assert_eq!(modules.status, 200);
    let keys: Vec<String> = modules.json().as_array().unwrap().iter().map(|m| m["key"].as_str().unwrap().to_string()).collect();
    assert!(keys.len() >= 20, "{keys:?}");
    for k in &keys {
        let r = c.call(&s, "GET", &format!("/api/v1/modules/{k}"), &[], None).await;
        assert_eq!(r.status, 200, "module info {k}: {}", r.body);
        let r = c.call(&s, "POST", &format!("/api/v1/records/{k}/query"), &[("X-CSRF-Token", &csrf)], Some((JSON, "{}"))).await;
        assert_eq!(r.status, 200, "list {k}: {}", r.body);
    }
    for tab in ["Leads", "Emails", "Campaigns", "Sales", "AI%20usage", "Activity", "Employees"] {
        let r = c.call(&s, "GET", &format!("/api/v1/reports/{tab}"), &[], None).await;
        assert_eq!(r.status, 200, "report {tab}: {}", r.body);
    }
    for g in ["general", "knowledge", "offers", "ai", "assignment", "webhooks"] {
        let r = c.call(&s, "GET", &format!("/api/v1/settings/{g}"), &[], None).await;
        assert_eq!(r.status, 200, "settings {g}: {}", r.body);
    }
    for p in ["/api/v1/quick", "/api/v1/notices", "/api/v1/ai/status", "/api/v1/session"] {
        let r = c.call(&s, "GET", p, &[], None).await;
        assert_eq!(r.status, 200, "{p}: {}", r.body);
    }
}
