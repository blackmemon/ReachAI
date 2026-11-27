//! Static files: the generated loader page and the WebAssembly bundles.
//! `/assets/public/*` is open. `/assets/app/*` (the signed-in bundle) needs a valid session.
//! A pre-compressed `.br` file is sent when the browser accepts Brotli.

use crate::http::{self, Resp};
use std::path::{Path, PathBuf};

pub fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "css" => "text/css; charset=utf-8",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Reject anything that could leave the folder.
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\0') || rel.contains('\\') || rel.starts_with('/') {
        return None;
    }
    let mut out = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.starts_with('.') {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

pub async fn serve_file(root: &str, rel: &str, accept_encoding: &str, immutable: bool) -> Option<Resp> {
    let root = Path::new(root);
    let path = safe_join(root, rel)?;
    let wants_br = accept_encoding.split(',').any(|e| e.trim().starts_with("br"));
    let (data, br) = if wants_br {
        let mut p = path.clone().into_os_string();
        p.push(".br");
        match tokio::fs::read(&p).await {
            Ok(d) => (d, true),
            Err(_) => (tokio::fs::read(&path).await.ok()?, false),
        }
    } else {
        (tokio::fs::read(&path).await.ok()?, false)
    };
    let mut r = http::bytes_resp(200, content_type(rel), data);
    let h = r.headers_mut();
    if br {
        h.insert("content-encoding", hyper::header::HeaderValue::from_static("br"));
    }
    h.insert("vary", hyper::header::HeaderValue::from_static("accept-encoding"));
    h.insert("cache-control", hyper::header::HeaderValue::from_static(if immutable { "public, max-age=31536000, immutable" } else { "no-cache" }));
    Some(r)
}

/// The only HTML of the app. It is generated: it loads one boot script and nothing else.
pub fn loader_html(bundle: &str, title: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title><link rel=\"stylesheet\" href=\"/assets/loader.css\"></head><body><div id=\"boot\" role=\"status\">Loading blackmemon ReachAI</div><script type=\"module\" src=\"/assets/{bundle}/boot.js\"></script></body></html>"
    )
    .into_bytes()
}

pub const LOADER_CSS: &str = "html,body{margin:0;height:100%;background:#f5f6fb;color:#1a1d3a;font-family:system-ui,sans-serif}#boot{display:flex;align-items:center;justify-content:center;height:100%;font-size:16px}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_traversal_is_refused() {
        let root = Path::new("/srv/dist");
        for bad in ["../etc/passwd", "a/../b", "/abs", "a//b", ".hidden", "a/.env", "a\\b", ""] {
            assert!(safe_join(root, bad).is_none(), "{bad}");
        }
        assert_eq!(safe_join(root, "public/app.wasm").unwrap(), Path::new("/srv/dist/public/app.wasm"));
    }

    #[test]
    fn types_by_extension() {
        assert_eq!(content_type("a.wasm"), "application/wasm");
        assert_eq!(content_type("boot.js"), "text/javascript; charset=utf-8");
    }

    #[test]
    fn loader_has_no_inline_script() {
        let h = String::from_utf8(loader_html("app", "x")).unwrap();
        assert!(h.contains("src=\"/assets/app/boot.js\""));
        assert!(!h.contains("<script>"));
    }
}
