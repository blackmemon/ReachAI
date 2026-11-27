use crate::api;
use crate::app::ctx::Ctx;
use crate::components::*;
use crate::pages::table;
use crate::theme::{self, sp, Kind};
use iced::widget::{checkbox, column, pick_list, row, scrollable, text_editor};
use iced::{Element, Task};
use shared::*;
use std::collections::{BTreeMap, HashMap};

/// Groups shown as tabs. "users" and "webhooks" are lists, the rest are forms.
pub fn groups() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = shared::settings_defs::GROUPS.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
    v.insert(v.len() - 1, ("users".into(), "Users".into()));
    v
}

pub struct State {
    pub group: String,
    pub page: Option<SettingsPage>,
    pub values: BTreeMap<String, String>,
    pub original: BTreeMap<String, String>,
    pub editors: HashMap<String, text_editor::Content>,
    pub list: Option<table::State>,
    pub note: Option<(bool, String)>,
    pub token_label: String,
    pub token: Option<String>,
    pub ai: Option<serde_json::Value>,
    pub busy: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Group(String),
    Loaded(Result<SettingsPage, ApiError>),
    Edit(String, String),
    Editor(String, text_editor::Action),
    Save,
    Reset,
    Done(Result<ActionResult, ApiError>),
    Table(table::Msg),
    TokenLabel(String),
    MakeToken,
    Token(Result<serde_json::Value, ApiError>),
    AiStatus(Result<serde_json::Value, ApiError>),
}

pub fn init(group: Option<String>) -> (State, Task<Msg>) {
    let group = group.unwrap_or_else(|| "general".into());
    let mut s = State { group: group.clone(), page: None, values: BTreeMap::new(), original: BTreeMap::new(), editors: HashMap::new(), list: None, note: None, token_label: "Claude Desktop".into(), token: None, ai: None, busy: false };
    let t = s.open(&group);
    (s, t)
}

impl State {
    fn open(&mut self, group: &str) -> Task<Msg> {
        self.group = group.to_string();
        self.page = None;
        self.list = None;
        self.note = None;
        self.token = None;
        match group {
            "users" | "webhooks" => {
                let (st, t) = table::State::new(group, group, None);
                self.list = Some(st);
                t.map(Msg::Table)
            }
            _ => {
                let path = format!("/api/v1/settings/{}", api::path_segment(group));
                let load = Task::perform(async move { api::get::<SettingsPage>(&path).await }, Msg::Loaded);
                if group == "ai" {
                    Task::batch([load, Task::perform(api::get::<serde_json::Value>("/api/v1/ai/status"), Msg::AiStatus)])
                } else {
                    load
                }
            }
        }
    }

    pub fn update(&mut self, msg: Msg, ctx: &mut Ctx) -> Task<Msg> {
        match msg {
            Msg::Group(g) => {
                ctx.go(&format!("/settings/{g}"));
                return self.open(&g);
            }
            Msg::Loaded(Ok(p)) => {
                self.values = p.items.iter().map(|i| (i.key.clone(), i.value.clone())).collect();
                self.original = self.values.clone();
                self.editors = p.items.iter().filter(|i| i.kind == "longtext").map(|i| (i.key.clone(), text_editor::Content::with_text(&i.value))).collect();
                self.page = Some(p);
            }
            Msg::Loaded(Err(e)) => ctx.failed(&e),
            Msg::Edit(k, v) => {
                self.values.insert(k, v);
            }
            Msg::Editor(k, a) => {
                if let Some(c) = self.editors.get_mut(&k) {
                    c.perform(a);
                    self.values.insert(k, c.text().trim_end_matches('\n').to_string());
                }
            }
            Msg::Save => {
                self.busy = true;
                let csrf = ctx.csrf();
                let body = SaveSettings { values: self.values.clone() };
                let path = format!("/api/v1/settings/{}", api::path_segment(&self.group));
                return Task::perform(async move { api::send::<ActionResult, _>("PUT", &path, &csrf, &body).await }, Msg::Done);
            }
            Msg::Reset => {
                self.busy = true;
                let csrf = ctx.csrf();
                let path = format!("/api/v1/settings/{}/reset", api::path_segment(&self.group));
                return Task::perform(async move { api::post::<ActionResult>(&path, &csrf).await }, Msg::Done);
            }
            Msg::Done(r) => {
                self.busy = false;
                match r {
                    Ok(a) => {
                        self.note = Some((true, a.message));
                        let g = self.group.clone();
                        let path = format!("/api/v1/settings/{}", api::path_segment(&g));
                        return Task::perform(async move { api::get::<SettingsPage>(&path).await }, Msg::Loaded);
                    }
                    Err(e) => {
                        self.note = Some((false, e.message.clone()));
                        ctx.failed(&e);
                    }
                }
            }
            Msg::Table(m) => {
                if let Some(t) = self.list.as_mut() {
                    return t.update(m, ctx).map(Msg::Table);
                }
            }
            Msg::TokenLabel(v) => self.token_label = v,
            Msg::MakeToken => {
                let csrf = ctx.csrf();
                let body = serde_json::json!({"label": self.token_label.trim()});
                return Task::perform(async move { api::send::<serde_json::Value, _>("POST", "/api/v1/mcp-tokens", &csrf, &body).await }, Msg::Token);
            }
            Msg::Token(Ok(v)) => self.token = v["token"].as_str().map(String::from),
            Msg::Token(Err(e)) => ctx.failed(&e),
            Msg::AiStatus(Ok(v)) => self.ai = Some(v),
            Msg::AiStatus(Err(_)) => {}
        }
        Task::none()
    }

    pub fn dirty(&self) -> bool {
        self.values != self.original
    }

    pub fn view<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let mut tabs = row![].spacing(sp::S1);
        for (k, l) in groups() {
            tabs = tabs.push(button_with(&l, None, if k == self.group { Kind::MenuActive } else { Kind::Menu }, Some(Msg::Group(k.clone()))));
        }
        let mut page = column![page_header("Settings", "Every rule has a default. You can reset a group.", vec![]), scrollable(tabs).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new()))].spacing(sp::S4);
        if let Some(t) = &self.list {
            return page.push(t.view(ctx).map(Msg::Table)).into();
        }
        if let Some((ok, m)) = &self.note {
            page = page.push(banner(if *ok { Tone::Green } else { Tone::Red }, m));
        }
        let Some(p) = &self.page else { return page.push(body("Loading")).into() };
        let admin = ctx.can("settings.admin");
        let mut form = column![card_title(p.title.clone())].spacing(sp::S3);
        for i in &p.items {
            let v = self.values.get(&i.key).cloned().unwrap_or_default();
            let key = i.key.clone();
            let can = i.editable && admin;
            let help = if can { i.help.clone() } else { format!("{} Only an admin can change this.", i.help) };
            let control: Element<'a, Msg> = match i.kind.as_str() {
                "bool" => {
                    let c = checkbox(if v == "true" { "On" } else { "Off" }, v == "true").style(theme::check_style);
                    (if can { c.on_toggle(move |b| Msg::Edit(key.clone(), b.to_string())) } else { c }).into()
                }
                "choice" => {
                    let pl = pick_list(i.choices.clone(), Some(v), move |x: String| Msg::Edit(key.clone(), x)).text_size(theme::size::TEXT).padding([sp::S2, sp::S3]).style(theme::pick_style).menu_style(theme::menu_style);
                    pl.into()
                }
                "longtext" => match self.editors.get(&i.key) {
                    Some(c) => {
                        let e = text_editor(c).height(iced::Length::Fixed(120.0)).padding(sp::S3).size(theme::size::TEXT);
                        (if can { e.on_action(move |a| Msg::Editor(key.clone(), a)) } else { e }).into()
                    }
                    None => body(v).into(),
                },
                _ => {
                    let inp = input("", &v, move |x| Msg::Edit(key.clone(), x), false);
                    inp.on_submit(Msg::Save).into()
                }
            };
            form = form.push(field(&i.label, control, None, &help));
        }
        if admin {
            form = form.push(row![button_with(if self.busy { "Saving" } else { "Save" }, None, Kind::Primary, if self.busy || !self.dirty() { None } else { Some(Msg::Save) }), secondary("Reset to defaults", Msg::Reset)].spacing(sp::S2));
        }
        page = page.push(card(form));
        if self.group == "ai" {
            page = page.push(self.ai_card(ctx));
        }
        page.max_width(theme::dim::FORM_MAX).into()
    }

    pub fn overlay<'a>(&'a self, ctx: &'a Ctx) -> Option<Element<'a, Msg>> {
        self.list.as_ref()?.overlay(ctx).map(|e| e.map(Msg::Table))
    }

    fn ai_card<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let mut col = column![card_title("Claude Desktop connection")].spacing(sp::S3);
        if let Some(a) = &self.ai {
            let state = a["state"].as_str().unwrap_or("never");
            let tone = match state { "on_time" => Tone::Green, "late" => Tone::Amber, _ => Tone::Red };
            col = col.push(pill(&state.replace('_', " "), tone));
            col = col.push(muted(format!("Tasks waiting: {}", a["waiting"].as_i64().unwrap_or(0))));
        }
        col = col.push(muted("Claude Desktop does the AI work through this app's MCP server. A token gives it your rights. It is shown once."));
        if ctx.can("ai.use") {
            col = col.push(text_field("Token label", "Claude Desktop", &self.token_label, Msg::TokenLabel, None, ""));
            col = col.push(button_with("Create token", Some("plus"), Kind::Secondary, Some(Msg::MakeToken)));
        }
        if let Some(t) = &self.token {
            col = col.push(banner(Tone::Amber, "Copy this token now. It will not be shown again."));
            col = col.push(card(iced::widget::text(t.clone()).size(theme::size::SMALL).font(iced::Font::MONOSPACE)));
        }
        card(col).into()
    }
}
