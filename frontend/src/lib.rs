//! blackmemon ReachAI frontend: Iced compiled to WebAssembly. All screens are drawn by Iced.
//! Two bundles come from this crate: `public` (sign-in, legal, unsubscribe) and `app` (signed in).

pub mod api;
pub mod browser;
pub mod components;
pub mod format;
pub mod icons;
pub mod theme;
pub mod app;
pub mod pages;
pub mod shell;

/// Browser entry. The loader calls `init()` after the module is loaded.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = app::run();
}
