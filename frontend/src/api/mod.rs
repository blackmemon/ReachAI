//! Calls to the server. The browser sends the session cookie by itself (same origin).
//! Changes carry the CSRF token from the session in a header.

use serde::de::DeserializeOwned;
use serde::Serialize;
use shared::ApiError;

pub struct Raw {
    pub status: u16,
    pub body: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
async fn transport(method: &str, path: &str, csrf: &str, content_type: Option<&str>, body: Option<Vec<u8>>) -> Result<Raw, String> {
    let url = format!("{}{}", crate::browser::origin(), path);
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut rb = reqwest::Client::new().request(m, url);
    if !csrf.is_empty() {
        rb = rb.header("X-CSRF-Token", csrf);
    }
    if let Some(ct) = content_type {
        rb = rb.header("Content-Type", ct);
    }
    if let Some(b) = body {
        rb = rb.body(b);
    }
    let resp = rb.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    Ok(Raw { status, body })
}

#[cfg(not(target_arch = "wasm32"))]
async fn transport(_: &str, _: &str, _: &str, _: Option<&str>, _: Option<Vec<u8>>) -> Result<Raw, String> {
    Err("The browser is needed for server calls.".into())
}

fn offline() -> ApiError {
    ApiError::new("offline", "The server could not be reached. Please check your connection and try again.")
}

pub fn parse_error(raw: &Raw) -> ApiError {
    serde_json::from_slice::<ApiError>(&raw.body).unwrap_or_else(|_| match raw.status {
        401 => ApiError::new("unauthorized", "Please sign in."),
        403 => ApiError::new("forbidden", "You do not have permission to do this."),
        404 => ApiError::new("not_found", "This was not found."),
        429 => ApiError::new("too_many", "Too many tries. Please wait a few minutes."),
        _ => ApiError::new("server", "Something went wrong on our side. Please try again."),
    })
}

pub fn decode<T: DeserializeOwned>(raw: &Raw) -> Result<T, ApiError> {
    if (200..300).contains(&raw.status) {
        serde_json::from_slice(&raw.body).map_err(|_| ApiError::new("server", "The answer from the server could not be read."))
    } else {
        Err(parse_error(raw))
    }
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let raw = transport("GET", path, "", None, None).await.map_err(|_| offline())?;
    decode(&raw)
}

pub async fn send<T: DeserializeOwned, B: Serialize>(method: &str, path: &str, csrf: &str, body: &B) -> Result<T, ApiError> {
    let bytes = serde_json::to_vec(body).map_err(|_| ApiError::new("bad_request", "The form could not be sent."))?;
    let raw = transport(method, path, csrf, Some("application/json"), Some(bytes)).await.map_err(|_| offline())?;
    decode(&raw)
}

pub async fn post<T: DeserializeOwned>(path: &str, csrf: &str) -> Result<T, ApiError> {
    send("POST", path, csrf, &serde_json::json!({})).await
}

pub async fn send_csv<T: DeserializeOwned>(path: &str, csrf: &str, csv: Vec<u8>) -> Result<T, ApiError> {
    let raw = transport("POST", path, csrf, Some("text/csv"), Some(csv)).await.map_err(|_| offline())?;
    decode(&raw)
}

/// For file downloads (CSV export and template).
pub async fn bytes(method: &str, path: &str, csrf: &str, body: Option<Vec<u8>>) -> Result<Vec<u8>, ApiError> {
    let ct = body.as_ref().map(|_| "application/json");
    let raw = transport(method, path, csrf, ct, body).await.map_err(|_| offline())?;
    if (200..300).contains(&raw.status) { Ok(raw.body) } else { Err(parse_error(&raw)) }
}

pub fn path_segment(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') { c.to_string() } else { format!("%{:02X}", c as u32 & 0xff) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_error_body_is_read() {
        let raw = Raw { status: 400, body: br#"{"code":"invalid","message":"Check the fields.","fields":{"email":"Not an email."}}"#.to_vec() };
        let e = decode::<serde_json::Value>(&raw).unwrap_err();
        assert_eq!(e.fields["email"], "Not an email.");
    }

    #[test]
    fn plain_statuses_get_plain_messages() {
        assert_eq!(parse_error(&Raw { status: 401, body: b"<html>".to_vec() }).code, "unauthorized");
        assert_eq!(parse_error(&Raw { status: 502, body: vec![] }).code, "server");
    }

    #[test]
    fn path_segments_are_escaped() {
        assert_eq!(path_segment("AI usage"), "AI%20usage");
        assert_eq!(path_segment("a/b"), "a%2Fb");
        assert_eq!(path_segment("id1.id2.sig-_~"), "id1.id2.sig-_~", "signed tokens keep their dots");
    }
}
