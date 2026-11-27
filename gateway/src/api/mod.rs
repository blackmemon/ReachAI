//! The JSON API under /api/v1. Every handler checks the session, then the CSRF token on
//! changes, then calls one backend service. Nothing here holds business rules.

use crate::http::{self, Resp};
use auth::Actor;
use backend::accounts::Resolved;
use backend::app::App;
use backend::error::{Error, Result};
use bytes::Bytes;
use hyper::{HeaderMap, Method};
use serde::de::DeserializeOwned;
use shared::*;

pub struct Req<'a> {
    pub app: &'a App,
    pub method: Method,
    pub segs: Vec<&'a str>,
    pub query: &'a str,
    pub headers: &'a HeaderMap,
    pub body: Bytes,
    pub ip: String,
}

fn parse<T: DeserializeOwned>(r: &Req) -> Result<T> {
    serde_json::from_slice(&r.body).map_err(|_| {
        Error::BadRequest(
            "The request could not be read. Please check the values and try again.".into(),
        )
    })
}

fn header<'a>(r: &'a Req, name: &str) -> &'a str {
    r.headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn secure(r: &Req) -> bool {
    r.app.cfg.cookie_secure
}

fn session_token<'a>(r: &'a Req) -> Option<&'a str> {
    auth::cookie_value(header(r, "cookie"), auth::session_cookie_name(secure(r)))
        .filter(|t| !t.is_empty())
}

async fn session(r: &Req<'_>) -> Result<Resolved> {
    let tok = session_token(r).ok_or(Error::Unauthorized)?;
    backend::accounts::resolve(r.app, tok)
        .await?
        .ok_or(Error::Unauthorized)
}

/// Changes need the session's CSRF token in a header, and a same-site Origin when sent.
fn check_csrf(r: &Req, s: &Resolved) -> Result<()> {
    if !auth::constant_eq(header(r, "x-csrf-token").as_bytes(), s.csrf.as_bytes()) {
        return Err(Error::Forbidden);
    }
    origin_ok(r)
}

fn origin_ok(r: &Req) -> Result<()> {
    let o = header(r, "origin");
    if o.is_empty() {
        return Ok(());
    }
    let host = |u: &str| {
        u.trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string()
    };
    if host(o) == host(&r.app.cfg.base_url) || host(o) == header(r, "host") {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}

fn ok(message: &str) -> Resp {
    http::json(
        200,
        &ActionResult {
            ok: true,
            message: message.into(),
        },
    )
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn id_ok(s: &str) -> Result<&str> {
    if is_uuid(s) {
        Ok(s)
    } else {
        Err(Error::NotFound)
    }
}

pub async fn handle(r: Req<'_>) -> Result<Resp> {
    let segs = r.segs.clone();
    let m = r.method.clone();
    let write = m != Method::GET && m != Method::HEAD;
    match (segs.as_slice(), &m) {
        // ---- before sign-in ----
        (["auth", "csrf"], &Method::GET) => {
            let t = auth::make_pre_login_csrf(&r.app.cfg.secret_key);
            let mut resp = http::json(200, &CsrfToken { token: t.clone() });
            http::add_cookie(
                &mut resp,
                &auth::set_cookie(auth::pre_csrf_cookie_name(secure(&r)), &t, secure(&r), 3600),
            );
            Ok(resp)
        }
        (["auth", "login"], &Method::POST) => {
            origin_ok(&r)?;
            let cookie =
                auth::cookie_value(header(&r, "cookie"), auth::pre_csrf_cookie_name(secure(&r)))
                    .unwrap_or("");
            if !auth::verify_pre_login_csrf(
                &r.app.cfg.secret_key,
                cookie,
                header(&r, "x-csrf-token"),
            ) {
                return Err(Error::Forbidden);
            }
            let req: LoginRequest = parse(&r)?;
            let tok = backend::accounts::login(r.app, &req.email, &req.password, &r.ip).await?;
            let s = backend::accounts::resolve(r.app, &tok)
                .await?
                .ok_or(Error::Unauthorized)?;
            let info = backend::accounts::session_info(r.app, &s).await?;
            let mut resp = http::json(200, &info);
            http::add_cookie(
                &mut resp,
                &auth::set_cookie(
                    auth::session_cookie_name(secure(&r)),
                    &tok,
                    secure(&r),
                    r.app.cfg.session_hard_seconds,
                ),
            );
            http::add_cookie(
                &mut resp,
                &auth::clear_cookie(auth::pre_csrf_cookie_name(secure(&r)), secure(&r)),
            );
            Ok(resp)
        }
        (["public", "unsubscribe", token], &Method::GET) => {
            // GET only checks the link, so mail scanners that open links do not unsubscribe anyone.
            backend::outbox::parse_unsubscribe_token(r.app, token).ok_or(Error::NotFound)?;
            Ok(ok("This link is valid."))
        }
        (["public", "unsubscribe", token], &Method::POST) => {
            backend::outbox::unsubscribe(r.app, token).await?;
            Ok(ok("You are unsubscribed. We will not email you again."))
        }
        (["mcp"], &Method::POST) => mcp(&r).await,
        _ => authed(r, write).await,
    }
}

async fn mcp(r: &Req<'_>) -> Result<Resp> {
    let tok = header(r, "authorization")
        .strip_prefix("Bearer ")
        .unwrap_or("");
    let actor = backend::ai::actor_for_token(r.app, tok)
        .await?
        .ok_or(Error::Unauthorized)?;
    let msg: serde_json::Value = parse(r)?;
    Ok(match backend::ai::handle_rpc(r.app, &actor, &msg).await {
        Some(v) => http::json(200, &v),
        None => http::bytes_resp(202, "application/json", vec![]),
    })
}

async fn authed(r: Req<'_>, write: bool) -> Result<Resp> {
    let s = session(&r).await?;
    if write {
        check_csrf(&r, &s)?;
    }
    let a = &s.actor;
    let app = r.app;
    let segs = r.segs.clone();
    let m = r.method.clone();
    match (segs.as_slice(), &m) {
        (["session"], &Method::GET) => Ok(http::json(
            200,
            &backend::accounts::session_info(app, &s).await?,
        )),
        (["auth", "logout"], &Method::POST) => {
            if let Some(t) = session_token(&r) {
                backend::accounts::logout(app, t).await?;
            }
            let mut resp = ok("Signed out.");
            http::add_cookie(
                &mut resp,
                &auth::clear_cookie(auth::session_cookie_name(secure(&r)), secure(&r)),
            );
            Ok(resp)
        }
        (["auth", "password"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct P {
                current: String,
                new: String,
            }
            let p: P = parse(&r)?;
            backend::accounts::change_password(
                app,
                a,
                &p.current,
                &p.new,
                session_token(&r).unwrap_or(""),
            )
            .await?;
            Ok(ok("Your password was changed. Other sign-ins were ended."))
        }
        (["modules"], &Method::GET) => Ok(http::json(200, &backend::records::infos())),
        (["modules", key], &Method::GET) => Ok(http::json(200, &backend::records::info(key)?)),

        (["records", key, "query"], &Method::POST) => Ok(http::json(
            200,
            &backend::records::list(app, a, key, &parse(&r)?).await?,
        )),
        (["records", key, "export"], &Method::POST) => {
            let csv = backend::records::export(app, a, key, &parse(&r)?).await?;
            let mut resp = http::bytes_resp(200, "text/csv; charset=utf-8", csv);
            resp.headers_mut().insert(
                "content-disposition",
                hyper::header::HeaderValue::from_str(&format!(
                    "attachment; filename=\"{key}.csv\""
                ))
                .unwrap(),
            );
            Ok(resp)
        }
        (["records", key, "template"], &Method::GET) => {
            let mut resp = http::bytes_resp(
                200,
                "text/csv; charset=utf-8",
                backend::imports::template(key)?,
            );
            resp.headers_mut().insert(
                "content-disposition",
                hyper::header::HeaderValue::from_str(&format!(
                    "attachment; filename=\"{key}-template.csv\""
                ))
                .unwrap(),
            );
            Ok(resp)
        }
        (["records", key, "import"], &Method::POST) => Ok(http::json(
            200,
            &backend::imports::run(app, a, key, &r.body).await?,
        )),
        (["records", key], &Method::POST) => {
            let sr: SaveRecord = parse(&r)?;
            Ok(http::json(
                201,
                &backend::records::create(app, a, key, &sr.values).await?,
            ))
        }
        (["records", key, id], &Method::GET) => {
            let (rec, v) = backend::records::get(app, a, key, id_ok(id)?).await?;
            Ok(http::json(
                200,
                &serde_json::json!({"record": rec, "row_version": v}),
            ))
        }
        (["records", key, id], &Method::PUT) => Ok(http::json(
            200,
            &backend::records::update(app, a, key, id_ok(id)?, &parse(&r)?).await?,
        )),
        (["companies", id, "remove-competitor"], &Method::POST) => {
            backend::records::remove_competitor(app, a, id_ok(id)?)
                .await
                .map(|_| ok("Company removed from competitors."))
        }

        (["today"], &Method::GET) => Ok(http::json(200, &backend::reports::today(app, a).await?)),
        (["reports", tab], &Method::GET) => Ok(http::json(
            200,
            &backend::reports::report(app, a, &tab.replace("%20", " ")).await?,
        )),
        (["quick"], &Method::GET) => Ok(http::json(200, &backend::quick::history(app, a).await?)),
        (["quick"], &Method::POST) => {
            let q: QuickAsk = parse(&r)?;
            Ok(http::json(
                200,
                &serde_json::json!({"answer": backend::quick::ask(app, a, &q.text).await?}),
            ))
        }
        (["labels", kind], &Method::GET) => {
            Ok(http::json(200, &backend::labels::list(app, a, kind).await?))
        }
        (["labels", kind], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                name: String,
            }
            let b: B = parse(&r)?;
            Ok(http::json(
                200,
                &backend::labels::create(app, a, kind, &b.name).await?,
            ))
        }
        (["labels", kind], &Method::DELETE) => {
            #[derive(serde::Deserialize)]
            struct B {
                name: String,
            }
            let b: B = parse(&r)?;
            Ok(http::json(
                200,
                &backend::labels::delete(app, a, kind, &b.name).await?,
            ))
        }
        (["labels", kind, "records", id], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                labels: Vec<String>,
            }
            let b: B = parse(&r)?;
            Ok(http::json(
                200,
                &backend::labels::replace(app, a, kind, id_ok(id)?, &b.labels).await?,
            ))
        }

        (["settings", group], &Method::GET) => {
            let mut tx = app.tx(a).await?;
            let p = backend::settings::page(&mut tx, a, group).await?;
            tx.commit().await?;
            Ok(http::json(200, &p))
        }
        (["settings", group], &Method::PUT) => {
            let sv: SaveSettings = parse(&r)?;
            let mut tx = app.tx(a).await?;
            backend::settings::save(&mut tx, a, group, &sv.values).await?;
            tx.commit().await?;
            Ok(ok("Settings saved."))
        }
        (["settings", group, "reset"], &Method::POST) => {
            let mut tx = app.tx(a).await?;
            backend::settings::reset(&mut tx, a, group).await?;
            tx.commit().await?;
            Ok(ok("Settings reset to the defaults."))
        }
        (["notices"], &Method::GET) => {
            let mut tx = app.tx(a).await?;
            let kind = http::query_param(r.query, "kind");
            let n = backend::notices::list(&mut tx, a, kind).await?;
            tx.commit().await?;
            Ok(http::json(200, &n))
        }
        (["notices", "read"], &Method::POST) => {
            let id = http::query_param(r.query, "id");
            let mut tx = app.tx(a).await?;
            backend::notices::mark_read(&mut tx, a, id).await?;
            tx.commit().await?;
            Ok(ok("Done."))
        }

        (["conversations", id, action], &Method::POST) => {
            let id = id_ok(id)?;
            match *action {
                "take-over" => backend::conversations::take_over(app, a, id)
                    .await
                    .map(|_| ok("You took over this conversation.")),
                "hand-back" => backend::conversations::hand_back(app, a, id)
                    .await
                    .map(|_| ok("Handed back to the queue.")),
                "extend" => backend::conversations::extend_timer(app, a, id)
                    .await
                    .map(|x| {
                        ok(if x {
                            "Ten more minutes added."
                        } else {
                            "The timer was already extended."
                        })
                    }),
                "archive" => backend::conversations::toggle_archive(app, a, id)
                    .await
                    .map(|archived| {
                        ok(if archived {
                            "Conversation archived."
                        } else {
                            "Conversation unarchived."
                        })
                    }),
                "reply" => {
                    #[derive(serde::Deserialize)]
                    struct B {
                        body: String,
                    }
                    let b: B = parse(&r)?;
                    let hard = backend::conversations::employee_reply(app, a, id, &b.body).await?;
                    Ok(if hard.is_empty() {
                        ok("Reply queued.")
                    } else {
                        http::json(
                            200,
                            &ActionResult {
                                ok: false,
                                message: hard.join(" "),
                            },
                        )
                    })
                }
                _ => Err(Error::NotFound),
            }
        }
        (["conversations", id, "messages"], &Method::GET) => Ok(http::json(
            200,
            &backend::conversations::messages(app, a, id_ok(id)?).await?,
        )),
        (["conversations", "groups"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                subject: String,
                participant_emails: Vec<String>,
            }
            let b: B = parse(&r)?;
            let id =
                backend::conversations::create_group(app, a, &b.subject, &b.participant_emails)
                    .await?;
            Ok(http::json(
                201,
                &serde_json::json!({"id": id, "message": "Group conversation created."}),
            ))
        }
        (["campaigns", id, action], &Method::POST) => {
            let id = id_ok(id)?;
            match *action {
                "start" => backend::campaigns::start(app, a, id)
                    .await
                    .map(|n| ok(&format!("Campaign started with {n} people."))),
                "pause" => backend::campaigns::set_paused(app, a, id, true)
                    .await
                    .map(|_| ok("Campaign paused.")),
                "resume" => backend::campaigns::set_paused(app, a, id, false)
                    .await
                    .map(|_| ok("Campaign resumed.")),
                _ => Err(Error::NotFound),
            }
        }
        (["emails", "compose"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                to: String,
                subject: String,
                body: String,
            }
            let b: B = parse(&r)?;
            let id = backend::outbox::compose(app, a, &b.to, &b.subject, &b.body).await?;
            Ok(http::json(
                201,
                &serde_json::json!({"id": id, "message": "Email passed the checks and was queued."}),
            ))
        }
        (["emails", id, "approve"], &Method::POST) => backend::outbox::approve(app, a, id_ok(id)?)
            .await
            .map(|_| ok("Approved. It will be sent in the next send window.")),
        (["emails", id, "edit"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                subject: String,
                body: String,
            }
            let b: B = parse(&r)?;
            let problems = backend::outbox::edit(app, a, id_ok(id)?, &b.subject, &b.body).await?;
            Ok(http::json(
                200,
                &DraftCheckResult {
                    ok: problems.is_empty(),
                    problems,
                },
            ))
        }
        (["sending", which], &Method::POST) => {
            backend::campaigns::set_pause_all(app, a, *which == "pause")
                .await
                .map(|_| {
                    ok(if *which == "pause" {
                        "All sending is paused."
                    } else {
                        "Sending resumed."
                    })
                })
        }
        (["draft-check"], &Method::POST) => {
            let d: DraftCheckRequest = parse(&r)?;
            let mut tx = app.tx(a).await?;
            let st = backend::settings::all(&mut tx, &a.tenant_id).await?;
            tx.commit().await?;
            let problems = backend::compliance::draft::check(
                &d.subject,
                &d.body,
                d.step,
                &backend::compliance::draft::Rules {
                    ban_dashes: backend::settings::flag(&st, "ban_dashes"),
                },
            );
            Ok(http::json(
                200,
                &DraftCheckResult {
                    ok: problems.is_empty(),
                    problems,
                },
            ))
        }
        (["candidates", id, action], &Method::POST) => {
            let id = id_ok(id)?;
            match *action {
                "approve" => backend::research::approve(app, a, id)
                    .await
                    .map(|_| ok("Saved as a company.")),
                "reject" => backend::research::reject(app, a, id)
                    .await
                    .map(|_| ok("Rejected.")),
                _ => Err(Error::NotFound),
            }
        }
        (["research", "sample"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                good: i64,
            }
            let b: B = parse(&r)?;
            backend::research::mark_sample(app, a, b.good)
                .await
                .map(|_| ok("Sample check marked done."))
        }
        (["research", "sessions"], &Method::GET) => {
            let mode = http::query_param(r.query, "mode").unwrap_or("");
            Ok(http::json(200, &backend::research::session::list(app, a, mode).await?))
        }
        (["research", "sessions", id], &Method::GET) => {
            let id = id_ok(id)?;
            Ok(http::json(200, &backend::research::session::get(app, a, id).await?))
        }
        (["research", "messages"], &Method::POST) => {
            let b: ResearchSend = parse(&r)?;
            Ok(http::json(200, &backend::research::session::send(app, a, &b).await?))
        }
        (["research", "runs", id], &Method::GET) => {
            let id = id_ok(id)?;
            let after: i64 = http::query_param(r.query, "after").and_then(|v| v.parse().ok()).unwrap_or(0);
            Ok(http::json(200, &backend::research::session::poll(app, a, id, after).await?))
        }
        (["research", "runs", id, "cancel"], &Method::POST) => {
            let id = id_ok(id)?;
            backend::research::session::cancel(app, a, id).await.map(|_| ok("Stopped."))
        }
        (["research", "sessions", id, "export"], &Method::POST) => {
            let id = id_ok(id)?;
            let b: ResearchExport = parse(&r)?;
            let (bytes, content_type, name) = backend::research::session::export(app, a, id, &b).await?;
            let mut resp = http::bytes_resp(200, content_type, bytes);
            resp.headers_mut().insert(
                "content-disposition",
                hyper::header::HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).unwrap(),
            );
            Ok(resp)
        }
        (["research", "sessions", id, "save"], &Method::POST) => {
            let id = id_ok(id)?;
            let b: ResearchIds = parse(&r)?;
            backend::research::session::save(app, a, id, &b.ids).await.map(|m| ok(&m))
        }
        (["mcp-tokens"], &Method::POST) => {
            #[derive(serde::Deserialize)]
            struct B {
                label: String,
            }
            let b: B = parse(&r)?;
            let t = backend::ai::create_token(app, a, &b.label).await?;
            Ok(http::json(
                201,
                &serde_json::json!({"token": t, "note": "Copy it now. It is shown only once."}),
            ))
        }
        (["ai", "status"], &Method::GET) => {
            let mut tx = app.tx(a).await?;
            let s = backend::ai::status(&mut tx, &a.tenant_id).await?;
            tx.commit().await?;
            Ok(http::json(
                200,
                &serde_json::json!({"state": s.state, "last_check_seconds": s.last_check_seconds, "expected_minutes": s.expected_minutes, "waiting": s.waiting, "oldest_wait_seconds": s.oldest_wait_seconds}),
            ))
        }
        _ => Err(Error::NotFound),
    }
}

#[allow(dead_code)]
fn _actor_is_used(_: &Actor) {}
