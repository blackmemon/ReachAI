use crate::{api, http, statics};
use backend::app::App;
use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

const MAX_BODY: usize = 3 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run() -> i32 {
    let cfg = match configs::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration problem: {e}");
            return 2;
        }
    };
    let pool = match database::connect(&cfg.database_url, 20).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot reach the database: {e}");
            return 2;
        }
    };
    let app = Arc::new(App::new(pool, cfg.clone()));
    let listener = match tokio::net::TcpListener::bind(&cfg.listen).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Cannot listen on {}: {e}", cfg.listen);
            return 2;
        }
    };
    tracing::info!(listen = %cfg.listen, "blackmemon ReachAI is running");
    let trust_proxy = std::env::var("LEADS_TRUST_PROXY")
        .map(|v| v == "true")
        .unwrap_or(false);
    let mut shutdown = Box::pin(tokio::signal::ctrl_c());
    loop {
        tokio::select! {
            _ = &mut shutdown => { tracing::info!("stopping"); break; }
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let app = app.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req| {
                        let app = app.clone();
                        let peer = peer.ip().to_string();
                        async move { Ok::<_, Infallible>(serve(app, req, peer, trust_proxy).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().keep_alive(true).serve_connection(io, svc).await;
                });
            }
        }
    }
    0
}

async fn serve(
    app: Arc<App>,
    req: Request<Incoming>,
    peer: String,
    trust_proxy: bool,
) -> http::Resp {
    let https = app.cfg.cookie_secure;
    let timeout = if req.uri().path() == "/api/v1/research/chat" {
        Duration::from_secs(110)
    } else {
        REQUEST_TIMEOUT
    };
    let mut resp = match tokio::time::timeout(timeout, route(&app, req, &peer, trust_proxy)).await {
        Ok(r) => r,
        Err(_) => http::text(504, "The request took too long."),
    };
    http::secure_headers(&mut resp, https);
    resp
}

async fn has_session(app: &App, headers: &hyper::HeaderMap) -> bool {
    let name = auth::session_cookie_name(app.cfg.cookie_secure);
    let cookie = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match auth::cookie_value(cookie, name) {
        Some(t) if !t.is_empty() => matches!(backend::accounts::resolve(app, t).await, Ok(Some(_))),
        _ => false,
    }
}

async fn route(
    app: &Arc<App>,
    req: Request<Incoming>,
    peer: &str,
    trust_proxy: bool,
) -> http::Resp {
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or("").to_string();
    let accept_enc = parts
        .headers
        .get("accept-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if path == "/health" {
        return http::text(200, "ok");
    }
    if path == "/ready" {
        let ready = match app.pool.acquire().await {
            Ok(mut c) => database::ping(&mut c).await.is_ok(),
            Err(_) => false,
        };
        return if ready {
            http::text(200, "ready")
        } else {
            http::text(503, "database not reachable")
        };
    }

    // Static files.
    if let Some(rest) = path.strip_prefix("/assets/") {
        if rest == "loader.css" {
            let mut r = http::bytes_resp(
                200,
                "text/css; charset=utf-8",
                statics::LOADER_CSS.as_bytes().to_vec(),
            );
            r.headers_mut().insert(
                "cache-control",
                hyper::header::HeaderValue::from_static("public, max-age=3600"),
            );
            return r;
        }
        if rest.starts_with("app/") {
            if !has_session(app, &parts.headers).await {
                return http::text(401, "Please sign in.");
            }
        } else if !rest.starts_with("public/") {
            return http::text(404, "Not found.");
        }
        let immutable = rest
            .rsplit('/')
            .next()
            .map(|f| f.contains('-') && !f.starts_with("boot"))
            .unwrap_or(false);
        return statics::serve_file(&app.cfg.dist_dir, rest, &accept_enc, immutable)
            .await
            .unwrap_or_else(|| http::text(404, "Not found."));
    }

    // One-click unsubscribe from the mail client (List-Unsubscribe-Post). The signed link is the proof.
    if let Some(token) = path.strip_prefix("/unsubscribe/") {
        if parts.method == Method::POST {
            return match backend::outbox::unsubscribe(app, token).await {
                Ok(_) => http::text(200, "You are unsubscribed."),
                Err(e) => http::error(&e),
            };
        }
    }

    if let Some(api_path) = path.strip_prefix("/api/v1/") {
        let body_bytes = match Limited::new(body, MAX_BODY).collect().await {
            Ok(b) => b.to_bytes(),
            Err(_) => {
                return http::error(&backend::error::Error::BadRequest(
                    "The request is too large.".into(),
                ))
            }
        };
        let ct = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let is_csv_import = api_path.ends_with("/import");
        if parts.method != Method::GET && !body_bytes.is_empty() {
            let ok = if is_csv_import {
                ct.starts_with("text/csv")
            } else {
                ct.starts_with("application/json")
            };
            if !ok {
                return http::error(&backend::error::Error::BadRequest(
                    "Unsupported content type.".into(),
                ));
            }
        }
        let segs: Vec<&str> = api_path.split('/').filter(|s| !s.is_empty()).collect();
        let r = api::Req {
            app,
            method: parts.method.clone(),
            segs,
            query: &query,
            headers: &parts.headers,
            body: Bytes::from(body_bytes),
            ip: http::client_ip(&parts.headers, peer, trust_proxy),
        };
        return match api::handle(r).await {
            Ok(r) => r,
            Err(e) => http::error(&e),
        };
    }

    // Every other page is one generated loader. Signed-in people get the app bundle.
    if parts.method == Method::GET || parts.method == Method::HEAD {
        let public_only = ["/privacy", "/terms", "/cookies", "/sign-in"].contains(&path.as_str())
            || path.starts_with("/unsubscribe/");
        let bundle = if !public_only && has_session(app, &parts.headers).await {
            "app"
        } else {
            "public"
        };
        return http::bytes_resp(
            200,
            "text/html; charset=utf-8",
            statics::loader_html(bundle, "blackmemon ReachAI"),
        );
    }
    http::text(404, "Not found.")
}
