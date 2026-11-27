//! What every signed-in page may read, and the few things a page may ask the shell to do.

use crate::components::Tone;
use shared::{ApiError, ModuleInfo, SessionInfo};

pub struct Ctx {
    pub session: SessionInfo,
    pub modules: Vec<ModuleInfo>,
    /// Minutes the device clock is ahead of UTC.
    pub tz: i32,
    pub width: f32,
    /// Width taken by the sidebar (0 on phones).
    pub sidebar: f32,
    pub(crate) toast: Option<(String, Tone)>,
    pub(crate) go: Option<String>,
    pub(crate) expired: bool,
    pub(crate) reload_session: bool,
}

impl Ctx {
    pub fn new(session: SessionInfo, modules: Vec<ModuleInfo>, tz: i32, width: f32) -> Self {
        Self { session, modules, tz, width, sidebar: 0.0, toast: None, go: None, expired: false, reload_session: false }
    }
    pub fn csrf(&self) -> String {
        self.session.csrf.clone()
    }
    pub fn can(&self, permission: &str) -> bool {
        self.session.permissions.iter().any(|p| p == permission)
    }
    pub fn say(&mut self, message: &str) {
        self.toast = Some((message.to_string(), Tone::Green));
    }
    pub fn warn(&mut self, message: &str) {
        self.toast = Some((message.to_string(), Tone::Red));
    }
    pub fn go(&mut self, path: &str) {
        self.go = Some(path.to_string());
    }
    pub fn refresh_session(&mut self) {
        self.reload_session = true;
    }
    pub fn phone(&self) -> bool {
        self.width < crate::theme::dim::PHONE
    }
    /// The page itself (the window minus the sidebar) is too narrow for side-by-side panes.
    pub fn narrow(&self) -> bool {
        self.width - self.sidebar < crate::theme::dim::SPLIT
    }
    /// A failed call: an ended session sends the person to sign in, anything else shows a message.
    pub fn failed(&mut self, e: &ApiError) {
        if e.code == "unauthorized" {
            self.expired = true;
        } else {
            self.warn(&e.message);
        }
    }
    pub fn module(&self, key: &str) -> Option<&ModuleInfo> {
        self.modules.iter().find(|m| m.key == key)
    }
}
