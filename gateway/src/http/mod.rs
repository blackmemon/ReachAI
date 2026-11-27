//! Small helpers on top of hyper: JSON replies, error mapping, security headers, cookies.

use backend::error::Error;
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::{Response, StatusCode};
use serde::Serialize;

pub type Resp = Response<Full<Bytes>>;

pub fn bytes_resp(status: u16, content_type: &str, body: Vec<u8>) -> Resp {
    let mut r = Response::new(Full::new(Bytes::from(body)));
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    r.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    r
}

pub fn json<T: Serialize>(status: u16, v: &T) -> Resp {
    match serde_json::to_vec(v) {
        Ok(b) => bytes_resp(status, "application/json", b),
        Err(_) => bytes_resp(500, "application/json", br#"{"code":"internal","message":"Something went wrong on our side.","fields":{}}"#.to_vec()),
    }
}

pub fn error(e: &Error) -> Resp {
    if let Error::Internal(m) = e {
        // The detail goes to the server log only.
        tracing::error!(detail = %m, "internal error");
    }
    let mut r = json(e.status(), &e.body());
    if matches!(e, Error::TooMany) {
        r.headers_mut().insert("retry-after", HeaderValue::from_static("900"));
    }
    r
}

pub fn text(status: u16, msg: &str) -> Resp {
    bytes_resp(status, "text/plain; charset=utf-8", msg.as_bytes().to_vec())
}

pub fn add_cookie(r: &mut Resp, cookie: &str) {
    if let Ok(v) = HeaderValue::from_str(cookie) {
        r.headers_mut().append("set-cookie", v);
    }
}

/// Headers on every response. The CSP allows only our own files and WebAssembly.
pub fn secure_headers(r: &mut Resp, https: bool) {
    let h = r.headers_mut();
    let set = |h: &mut hyper::HeaderMap, k: &'static str, v: &'static str| {
        h.insert(k, HeaderValue::from_static(v));
    };
    set(h, "content-security-policy", "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-hashes' 'sha256-+n38dPls+ai5Ru+1a0PvppyRFKnHpRVnC61FUYtjzhQ='; img-src 'self' data:; font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'");
    set(h, "x-content-type-options", "nosniff");
    set(h, "referrer-policy", "no-referrer");
    set(h, "x-frame-options", "DENY");
    set(h, "permissions-policy", "camera=(), microphone=(), geolocation=(), payment=()");
    set(h, "cross-origin-opener-policy", "same-origin");
    if https {
        set(h, "strict-transport-security", "max-age=31536000; includeSubDomains");
    }
    if !h.contains_key("cache-control") {
        set(h, "cache-control", "no-store");
    }
}

/// First hop of X-Forwarded-For, only when the deployer says a trusted proxy sets it.
pub fn client_ip(headers: &hyper::HeaderMap, peer: &str, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                let f = first.trim();
                if !f.is_empty() && f.len() < 64 {
                    return f.to_string();
                }
            }
        }
    }
    peer.to_string()
}

pub fn query_param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').filter_map(|p| p.split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_reads_values() {
        assert_eq!(query_param("a=1&kind=bounce", "kind"), Some("bounce"));
        assert_eq!(query_param("a=1", "kind"), None);
    }

    #[test]
    fn forwarded_ip_only_when_trusted() {
        let mut h = hyper::HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4, 5.6.7.8"));
        assert_eq!(client_ip(&h, "9.9.9.9", true), "1.2.3.4");
        assert_eq!(client_ip(&h, "9.9.9.9", false), "9.9.9.9");
    }

    #[test]
    fn headers_include_csp_and_nosniff() {
        let mut r = text(200, "x");
        secure_headers(&mut r, true);
        assert!(r.headers()["content-security-policy"].to_str().unwrap().contains("frame-ancestors 'none'"));
        assert_eq!(r.headers()["x-content-type-options"], "nosniff");
        assert!(r.headers().contains_key("strict-transport-security"));
    }
}
