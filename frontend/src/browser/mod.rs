//! The few browser facts Iced does not give us: the address, the time zone, downloads, a timer.
//! Native builds (used for unit tests) get harmless stand-ins.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    thread_local! {
        static POPSTATE_CALLBACK: RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>>> = const { RefCell::new(None) };
    }

    pub fn pathname() -> String {
        web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_else(|| "/".into())
    }
    pub fn origin() -> String {
        web_sys::window().and_then(|w| w.location().origin().ok()).unwrap_or_default()
    }
    /// The browser window's width in CSS pixels (before the app's own scale is applied).
    pub fn inner_width() -> f32 {
        web_sys::window().and_then(|w| w.inner_width().ok()).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(1280.0)
    }
    pub fn push_path(path: &str) {
        if let Some(w) = web_sys::window() {
            if let Ok(h) = w.history() {
                let _ = h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
            }
        }
    }
    pub fn replace_path(path: &str) {
        if let Some(w) = web_sys::window() {
            if let Ok(h) = w.history() {
                let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
            }
        }
    }
    pub fn set_title(t: &str) {
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            d.set_title(t);
        }
    }
    pub fn popstate() -> impl iced::futures::Stream<Item = ()> {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        if let Some(w) = web_sys::window() {
            let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
                let _ = sender.unbounded_send(());
            }) as Box<dyn FnMut(_)>);
            if w.add_event_listener_with_callback("popstate", callback.as_ref().unchecked_ref()).is_ok() {
                POPSTATE_CALLBACK.with(|slot| *slot.borrow_mut() = Some(callback));
            }
        }
        receiver
    }
    /// Minutes the device clock is ahead of UTC.
    pub fn tz_offset_minutes() -> i32 {
        -(js_sys::Date::new_0().get_timezone_offset() as i32)
    }
    pub fn navigate_hard(path: &str) {
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href(path);
        }
    }
    pub async fn sleep(ms: i32) {
        let p = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
            }
        });
        let _ = JsFuture::from(p).await;
    }
    /// Ask for a CSV file with the browser's file chooser.
    pub async fn pick_csv() -> Option<Vec<u8>> {
        let f = rfd::AsyncFileDialog::new().add_filter("CSV file", &["csv"]).pick_file().await?;
        Some(f.read().await)
    }
    /// Open a public web address in a new tab. Only http and https are opened, without sending
    /// this app's address along.
    pub fn open_url(url: &str) {
        let lower = url.trim().to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            return;
        }
        if let Some(w) = web_sys::window() {
            let _ = w.open_with_url_and_target_and_features(url.trim(), "_blank", "noopener,noreferrer");
        }
    }
    pub fn download(name: &str, mime: &str, bytes: &[u8]) {
        let arr = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&arr);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts) else { return };
        let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            if let Ok(a) = d.create_element("a") {
                if let Ok(a) = a.dyn_into::<web_sys::HtmlAnchorElement>() {
                    a.set_href(&url);
                    a.set_download(name);
                    a.click();
                }
            }
        }
        let _ = web_sys::Url::revoke_object_url(&url);
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    pub fn pathname() -> String {
        "/".into()
    }
    pub fn origin() -> String {
        String::new()
    }
    pub fn inner_width() -> f32 {
        1280.0
    }
    pub fn push_path(_: &str) {}
    pub fn replace_path(_: &str) {}
    pub fn set_title(_: &str) {}
    pub fn popstate() -> impl iced::futures::Stream<Item = ()> {
        iced::futures::stream::pending()
    }
    pub fn tz_offset_minutes() -> i32 {
        0
    }
    pub fn navigate_hard(_: &str) {}
    pub async fn sleep(_: i32) {}
    pub async fn pick_csv() -> Option<Vec<u8>> {
        None
    }
    pub fn open_url(_: &str) {}
    pub fn download(_: &str, _: &str, _: &[u8]) {}
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
