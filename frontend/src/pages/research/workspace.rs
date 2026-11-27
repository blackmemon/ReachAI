//! The AI research workspace: a chat that starts Claude CLI research runs, shows their live
//! progress, and keeps the results in a table you can filter, sort, select, open and export.
//! AI Leads and Competitors share this screen; each keeps its own conversations.

use crate::api;
use crate::app::ctx::Ctx;
use crate::browser;
use crate::components::*;
use crate::icons::icon;
use crate::theme::{self, c, sp, Kind};
use iced::widget::scrollable::{self, RelativeOffset};
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input, Row, Space};
use iced::{Alignment, Element, Length, Task};
use shared::*;
use std::collections::HashSet;

const ANY: &str = "Any status";
const PAGE: usize = 25;
const POLL_MS: i32 = 1000;
const MAX_POLL_FAILS: u8 = 5;

pub struct State {
    pub mode: &'static str,
    sessions: Vec<ResearchSession>,
    current: Option<String>,
    messages: Vec<ResearchMessage>,
    results: Vec<ResearchResult>,
    run: Option<ResearchRunInfo>,
    events: Vec<ResearchEvent>,
    after: i64,
    poll_fails: u8,
    input: String,
    sending: bool,
    loading: bool,
    show_sessions: bool,
    search: String,
    verification: String,
    only_email: bool,
    sort: Option<(&'static str, bool)>,
    page: usize,
    selected: HashSet<String>,
    open: Option<String>,
    note: Option<(bool, String)>,
    busy_action: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Sessions(Result<Vec<ResearchSession>, ApiError>),
    ToggleSessions,
    New,
    Open(String),
    Loaded(String, Result<ResearchSessionView, ApiError>),
    Input(String),
    Prompt(String),
    Send,
    Started(Result<ResearchStarted, ApiError>),
    PollNow(String),
    Polled(String, Result<ResearchPoll, ApiError>),
    Stop,
    Stopped(Result<ActionResult, ApiError>),
    Search(String),
    Verification(String),
    OnlyEmail(bool),
    Sort(&'static str),
    Page(i32),
    Toggle(String),
    ToggleAll,
    Detail(Option<String>),
    OpenUrl(String),
    Export(&'static str),
    Exported(Result<Vec<u8>, ApiError>, String, &'static str),
    Save,
    Saved(Result<ActionResult, ApiError>),
}

async fn fetch<T: serde::de::DeserializeOwned>(path: String) -> Result<T, ApiError> {
    api::get::<T>(&path).await
}

fn chat_id() -> scrollable::Id {
    scrollable::Id::new("research-chat")
}

fn to_end() -> Task<Msg> {
    scrollable::snap_to(chat_id(), RelativeOffset { x: 0.0, y: 1.0 })
}

pub fn init(mode: &'static str) -> (State, Task<Msg>) {
    let state = State {
        mode,
        sessions: vec![],
        current: None,
        messages: vec![],
        results: vec![],
        run: None,
        events: vec![],
        after: 0,
        poll_fails: 0,
        input: String::new(),
        sending: false,
        loading: false,
        show_sessions: false,
        search: String::new(),
        verification: ANY.into(),
        only_email: false,
        sort: None,
        page: 0,
        selected: HashSet::new(),
        open: None,
        note: None,
        busy_action: false,
    };
    let task = load_sessions(mode);
    (state, task)
}

fn load_sessions(mode: &'static str) -> Task<Msg> {
    Task::perform(fetch::<Vec<ResearchSession>>(format!("/api/v1/research/sessions?mode={mode}")), Msg::Sessions)
}

fn running(run: &Option<ResearchRunInfo>) -> bool {
    run.as_ref().is_some_and(|r| r.status == "running")
}

impl State {
    fn clear_session(&mut self) {
        self.current = None;
        self.messages.clear();
        self.results.clear();
        self.run = None;
        self.events.clear();
        self.after = 0;
        self.selected.clear();
        self.open = None;
        self.page = 0;
        self.note = None;
    }

    fn poll_later(&self, run_id: String) -> Task<Msg> {
        Task::perform(browser::sleep(POLL_MS), move |_| Msg::PollNow(run_id.clone()))
    }

    pub fn update(&mut self, msg: Msg, ctx: &mut Ctx) -> Task<Msg> {
        match msg {
            Msg::Sessions(Ok(list)) => self.sessions = list,
            Msg::Sessions(Err(e)) => ctx.failed(&e),
            Msg::ToggleSessions => self.show_sessions = !self.show_sessions,
            Msg::New => {
                if !running(&self.run) {
                    self.clear_session();
                }
                self.show_sessions = false;
            }
            Msg::Open(id) => {
                if self.current.as_deref() == Some(id.as_str()) {
                    self.show_sessions = false;
                    return Task::none();
                }
                self.clear_session();
                self.current = Some(id.clone());
                self.loading = true;
                self.show_sessions = false;
                let sid = id.clone();
                return Task::perform(fetch::<ResearchSessionView>(format!("/api/v1/research/sessions/{id}")), move |r| Msg::Loaded(sid.clone(), r));
            }
            Msg::Loaded(id, r) => {
                if self.current.as_deref() != Some(id.as_str()) {
                    return Task::none();
                }
                self.loading = false;
                match r {
                    Ok(v) => {
                        self.messages = v.messages;
                        self.results = v.results;
                        self.after = v.events.last().map(|e| e.id).unwrap_or(0);
                        self.events = v.events;
                        self.run = v.run;
                        let known: HashSet<&str> = self.results.iter().map(|r| r.id.as_str()).collect();
                        self.selected.retain(|id| known.contains(id.as_str()));
                        if self.open.as_deref().is_some_and(|id| !known.contains(id)) {
                            self.open = None;
                        }
                        if let Some(run) = self.run.as_ref().filter(|r| r.status == "running") {
                            let rid = run.id.clone();
                            return Task::batch([to_end(), self.poll_later(rid)]);
                        }
                        return to_end();
                    }
                    Err(e) => {
                        self.current = None;
                        ctx.failed(&e);
                    }
                }
            }
            Msg::Input(v) => self.input = v,
            Msg::Prompt(v) => {
                self.input = v;
                return self.update(Msg::Send, ctx);
            }
            Msg::Send => {
                let text = self.input.trim().to_string();
                if text.is_empty() || self.sending || running(&self.run) {
                    return Task::none();
                }
                self.sending = true;
                self.note = None;
                self.messages.push(ResearchMessage { role: "user".into(), body: text.clone(), at: String::new() });
                self.input.clear();
                let body = ResearchSend { session_id: self.current.clone().unwrap_or_default(), mode: self.mode.into(), text };
                let csrf = ctx.csrf();
                return Task::batch([
                    to_end(),
                    Task::perform(async move { api::send::<ResearchStarted, _>("POST", "/api/v1/research/messages", &csrf, &body).await }, Msg::Started),
                ]);
            }
            Msg::Started(Ok(s)) => {
                self.sending = false;
                self.current = Some(s.session_id.clone());
                self.events.clear();
                self.after = 0;
                self.poll_fails = 0;
                self.run = Some(ResearchRunInfo { id: s.run_id.clone(), status: "running".into(), step: "Understanding your request".into(), error: String::new(), found: 0, duplicates: 0 });
                return Task::batch([load_sessions(self.mode), self.poll_later(s.run_id)]);
            }
            Msg::Started(Err(e)) => {
                self.sending = false;
                // Take the unsent message back so it is not lost.
                if let Some(m) = self.messages.pop() {
                    if m.role == "user" {
                        self.input = m.body;
                    }
                }
                self.note = Some((false, e.message.clone()));
                ctx.failed(&e);
            }
            Msg::PollNow(rid) => {
                if self.run.as_ref().map(|r| r.id.as_str()) != Some(rid.as_str()) {
                    return Task::none();
                }
                let path = format!("/api/v1/research/runs/{rid}?after={}", self.after);
                return Task::perform(fetch::<ResearchPoll>(path), move |r| Msg::Polled(rid.clone(), r));
            }
            Msg::Polled(rid, r) => {
                if self.run.as_ref().map(|r| r.id.as_str()) != Some(rid.as_str()) {
                    return Task::none();
                }
                match r {
                    Ok(p) => {
                        self.poll_fails = 0;
                        if let Some(last) = p.events.last() {
                            self.after = last.id;
                        }
                        self.events.extend(p.events);
                        let finished = p.run.status != "running";
                        self.run = Some(p.run);
                        if !finished {
                            return Task::batch([to_end(), self.poll_later(rid)]);
                        }
                        // The run ended: fetch the answer, the results and the conversation list.
                        let Some(sid) = self.current.clone() else { return Task::none() };
                        let sid2 = sid.clone();
                        return Task::batch([
                            load_sessions(self.mode),
                            Task::perform(fetch::<ResearchSessionView>(format!("/api/v1/research/sessions/{sid}")), move |r| Msg::Loaded(sid2.clone(), r)),
                        ]);
                    }
                    Err(e) => {
                        if e.code == "unauthorized" {
                            ctx.failed(&e);
                            return Task::none();
                        }
                        self.poll_fails += 1;
                        if self.poll_fails >= MAX_POLL_FAILS {
                            self.note = Some((false, "Lost contact with the server while the search ran. Reopen the conversation to see where it got to.".into()));
                            return Task::none();
                        }
                        return self.poll_later(rid);
                    }
                }
            }
            Msg::Stop => {
                if let Some(run) = self.run.as_ref().filter(|r| r.status == "running") {
                    let csrf = ctx.csrf();
                    let path = format!("/api/v1/research/runs/{}/cancel", run.id);
                    return Task::perform(async move { api::send::<ActionResult, _>("POST", &path, &csrf, &serde_json::json!({})).await }, Msg::Stopped);
                }
            }
            Msg::Stopped(Ok(_)) => {}
            Msg::Stopped(Err(e)) => ctx.failed(&e),
            Msg::Search(v) => {
                self.search = v;
                self.page = 0;
            }
            Msg::Verification(v) => {
                self.verification = v;
                self.page = 0;
            }
            Msg::OnlyEmail(b) => {
                self.only_email = b;
                self.page = 0;
            }
            Msg::Sort(col) => {
                self.sort = match self.sort {
                    Some((c, false)) if c == col => Some((col, true)),
                    Some((c, true)) if c == col => None,
                    _ => Some((col, false)),
                };
            }
            Msg::Page(d) => {
                let last = self.visible().len().saturating_sub(1) / PAGE;
                self.page = (self.page as i64 + d as i64).clamp(0, last as i64) as usize;
            }
            Msg::Toggle(id) => {
                if !self.selected.remove(&id) {
                    self.selected.insert(id);
                }
            }
            Msg::ToggleAll => {
                let ids: Vec<String> = self.visible().iter().map(|r| r.id.clone()).collect();
                if ids.iter().all(|id| self.selected.contains(id)) {
                    for id in ids {
                        self.selected.remove(&id);
                    }
                } else {
                    self.selected.extend(ids);
                }
            }
            Msg::Detail(id) => self.open = id,
            Msg::OpenUrl(url) => browser::open_url(&url),
            Msg::Export(format) => {
                let Some(sid) = self.current.clone() else { return Task::none() };
                let ids: Vec<String> = self.selected.iter().cloned().collect();
                let csrf = ctx.csrf();
                let path = format!("/api/v1/research/sessions/{sid}/export");
                let body = serde_json::to_vec(&ResearchExport { format: format.into(), ids }).unwrap_or_default();
                let name = if self.mode == "competitors" { "competitors" } else { "leads" };
                let file = format!("{name}.{format}");
                return Task::perform(async move { api::bytes("POST", &path, &csrf, Some(body)).await }, move |r| Msg::Exported(r, file.clone(), format));
            }
            Msg::Exported(Ok(bytes), name, format) => {
                let mime = if format == "csv" { "text/csv" } else { "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" };
                browser::download(&name, mime, &bytes);
            }
            Msg::Exported(Err(e), _, _) => ctx.failed(&e),
            Msg::Save => {
                let Some(sid) = self.current.clone() else { return Task::none() };
                if self.selected.is_empty() || self.busy_action {
                    return Task::none();
                }
                self.busy_action = true;
                let ids: Vec<String> = self.selected.iter().cloned().collect();
                let csrf = ctx.csrf();
                let path = format!("/api/v1/research/sessions/{sid}/save");
                return Task::perform(async move { api::send::<ActionResult, _>("POST", &path, &csrf, &ResearchIds { ids }).await }, Msg::Saved);
            }
            Msg::Saved(r) => {
                self.busy_action = false;
                match r {
                    Ok(a) => {
                        self.note = Some((true, a.message));
                        self.selected.clear();
                        if let Some(sid) = self.current.clone() {
                            let sid2 = sid.clone();
                            return Task::perform(fetch::<ResearchSessionView>(format!("/api/v1/research/sessions/{sid}")), move |r| Msg::Loaded(sid2.clone(), r));
                        }
                    }
                    Err(e) => {
                        self.note = Some((false, e.message.clone()));
                        ctx.failed(&e);
                    }
                }
            }
        }
        Task::none()
    }

    /// Results after the search box, the status filter and the sort.
    fn visible(&self) -> Vec<&ResearchResult> {
        let needle = self.search.trim().to_lowercase();
        let mut rows: Vec<&ResearchResult> = self
            .results
            .iter()
            .filter(|r| self.verification == ANY || r.verification == self.verification.to_lowercase())
            .filter(|r| !self.only_email || !r.email.is_empty())
            .filter(|r| {
                needle.is_empty()
                    || [&r.name, &r.category, &r.location, &r.address, &r.website, &r.email, &r.phone, &r.notes, &r.services, &r.pricing, &r.reviews]
                        .iter()
                        .any(|f| f.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((col, desc)) = self.sort {
            let key = |r: &ResearchResult| -> String {
                match col {
                    "name" => r.name.to_lowercase(),
                    "category" => r.category.to_lowercase(),
                    "location" => r.location.to_lowercase(),
                    "phone" => r.phone.clone(),
                    "email" => r.email.to_lowercase(),
                    "pricing" => r.pricing.to_lowercase(),
                    "reviews" => r.reviews.to_lowercase(),
                    "status" => match r.verification.as_str() {
                        "verified" => "0".into(),
                        "partial" => "1".into(),
                        _ => "2".into(),
                    },
                    _ => String::new(),
                }
            };
            rows.sort_by(|a, b| key(a).cmp(&key(b)));
            if desc {
                rows.reverse();
            }
        }
        rows
    }

    // ---------- views ----------

    pub fn view<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let main = column![self.chat_card(ctx), self.results_card(ctx)].spacing(sp::S4).width(Length::Fill);
        if ctx.narrow() {
            let n = self.sessions.len();
            let toggle = button_with(&format!("Conversations ({n})"), Some(if self.show_sessions { "up" } else { "down" }), Kind::Secondary, Some(Msg::ToggleSessions));
            let mut col = column![toggle].spacing(sp::S3);
            if self.show_sessions {
                col = col.push(self.sessions_card());
            }
            return col.push(main).into();
        }
        row![container(self.sessions_card()).width(Length::FillPortion(2)), container(main).width(Length::FillPortion(7))].spacing(sp::S4).into()
    }

    fn sessions_card(&self) -> Element<'_, Msg> {
        let mut list = column![button_wide("New conversation", Kind::Accent, if running(&self.run) { None } else { Some(Msg::New) })].spacing(sp::S2);
        if self.sessions.is_empty() {
            list = list.push(muted("Your conversations will be listed here."));
        }
        for s in &self.sessions {
            let active = self.current.as_deref() == Some(s.id.as_str());
            let title = if s.title.is_empty() { "Conversation".to_string() } else { s.title.clone() };
            let label = column![text(title).size(theme::size::SMALL).color(c::INK), muted(format!("{} · {} results", s.updated_at, s.results))].spacing(2.0);
            list = list.push(
                button(label)
                    .width(Length::Fill)
                    .padding([sp::S2, sp::S3])
                    .style(theme::button_style(if active { Kind::MenuActive } else { Kind::Menu }))
                    .on_press(Msg::Open(s.id.clone())),
            );
        }
        card(column![card_title("Conversations"), list].spacing(sp::S3)).into()
    }

    fn chat_card<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let competitors = self.mode == "competitors";
        let bubble_max = ((ctx.width - ctx.sidebar) * 0.55).max(240.0);
        let mut convo = column![].spacing(sp::S3).padding(sp::S3).width(Length::Fill);
        if self.messages.is_empty() && !self.loading {
            let (title, hint, prompts): (&str, &str, [&str; 3]) = if competitors {
                (
                    "Research competitors",
                    "Name a restaurant or an industry and a place. Claude searches public pages and compares the competitors it finds.",
                    ["Find competitors of this restaurant", "Research the top competitors of restaurant businesses in New York", "Compare pizza restaurants in Chicago"],
                )
            } else {
                (
                    "Find business leads",
                    "Describe who you want to reach in plain words. Claude searches public pages and collects contact details.",
                    ["Find restaurant leads in New York", "Find potential restaurant customers in California", "Find independent cafes in Austin"],
                )
            };
            let chips: Vec<Element<'_, Msg>> = prompts.into_iter().map(|p| button_with(p, None, Kind::Secondary, Some(Msg::Prompt(p.into()))).into()).collect();
            convo = convo.push(column![heading(title), muted(hint), Row::with_children(chips).spacing(sp::S2).wrap()].spacing(sp::S3));
        }
        if self.loading {
            convo = convo.push(muted("Opening the conversation..."));
        }
        for m in &self.messages {
            let mine = m.role == "user";
            let bubble = container(body(m.body.clone())).padding([sp::S2, sp::S3]).max_width(bubble_max).style(theme::panel(if mine { c::GREEN_SOFT } else { c::PAGE }));
            convo = convo.push(if mine { row![Space::with_width(Length::Fill), bubble] } else { row![bubble] });
        }
        if running(&self.run) {
            convo = convo.push(self.progress());
        } else if let Some(run) = self.run.as_ref().filter(|r| r.status == "failed") {
            convo = convo.push(banner(Tone::Red, &run.error));
        }
        // The conversation takes a share of the window height, within sensible limits.
        let height = (theme::viewport_height() * 0.42).clamp(260.0, 560.0);
        let log = scrollable::Scrollable::new(convo).id(chat_id()).height(Length::Fixed(height)).width(Length::Fill);

        let busy = self.sending || running(&self.run);
        let placeholder = if self.current.is_some() { "Ask a follow-up or refine the search..." } else if competitors { "Find competitors of..." } else { "Find restaurant leads in..." };
        let field = text_input(placeholder, &self.input)
            .on_input(Msg::Input)
            .on_submit(Msg::Send)
            .padding([sp::S2, sp::S3])
            .size(theme::size::TEXT)
            .style(theme::input_style(false))
            .width(Length::Fill);
        let send = button_with("Search", Some("search"), Kind::Accent, if busy || self.input.trim().is_empty() { None } else { Some(Msg::Send) });
        let mut panel = column![log].spacing(sp::S2);
        if let Some((ok, message)) = &self.note {
            panel = panel.push(banner(if *ok { Tone::Green } else { Tone::Red }, message));
        }
        if self.current.is_some() && !busy && !self.results.is_empty() {
            let chips: [&str; 3] = if competitors {
                ["Find more competitors", "Compare their pricing", "Which ones have the best reviews?"]
            } else {
                ["Find more leads", "Only ones with a public email", "Search a nearby area"]
            };
            let chips: Vec<Element<'_, Msg>> = chips.into_iter().map(|p| button_with(p, None, Kind::Ghost, Some(Msg::Prompt(p.into()))).into()).collect();
            panel = panel.push(Row::with_children(chips).spacing(sp::S2).wrap());
        }
        panel = panel.push(row![field, send].spacing(sp::S2).align_y(Alignment::Center));
        panel = panel.push(muted("Claude uses public web pages only. Details it cannot confirm are left empty and marked, never guessed."));
        card(panel).padding(sp::S3).into()
    }

    /// Live progress of a running search: the step, pages read, businesses spotted, and recent activity.
    fn progress(&self) -> Element<'_, Msg> {
        let pages = self.events.iter().filter(|e| e.kind == "site").count();
        let spotted = self.events.iter().filter(|e| e.kind == "lead").count();
        let step = self.run.as_ref().map(|r| r.step.clone()).unwrap_or_default();
        let head = row![
            icon("sparkles", theme::size::ICON, c::GREEN),
            column![strong(step), muted(format!("{pages} {} analyzed · {spotted} {} spotted", if pages == 1 { "page" } else { "pages" }, if spotted == 1 { "business" } else { "businesses" }))].spacing(2.0).width(Length::Fill),
            button_with("Stop", Some("close"), Kind::Secondary, Some(Msg::Stop)),
        ]
        .spacing(sp::S3)
        .align_y(Alignment::Center);
        let mut log = column![].spacing(sp::S1);
        let start = self.events.len().saturating_sub(7);
        for e in &self.events[start..] {
            let (name, color) = match e.kind.as_str() {
                "site" => ("book", c::PRIMARY),
                "lead" => ("target", c::GREEN),
                "error" => ("error", c::RED),
                "step" => ("search", c::MUTED),
                _ => ("info", c::MUTED),
            };
            log = log.push(row![icon(name, theme::size::ICON_SMALL, color), text(e.body.clone()).size(theme::size::SMALL).color(c::MUTED)].spacing(sp::S2).align_y(Alignment::Center));
        }
        container(column![head, log].spacing(sp::S3)).padding(sp::S3).width(Length::Fill).style(theme::panel(c::CYAN_SOFT)).into()
    }

    fn results_card<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let competitors = self.mode == "competitors";
        if self.current.is_none() || self.results.is_empty() {
            let msg = if running(&self.run) { "Results will appear here when the search finishes." } else { "Results from your searches are listed here, ready to filter, open and export." };
            return card(column![card_title(if competitors { "Competitors found" } else { "Leads found" }), empty_state(msg)].spacing(sp::S3)).into();
        }
        let rows = self.visible();
        let phone = ctx.phone();
        let verifications = vec![ANY.to_string(), "Verified".into(), "Partial".into(), "Unverified".into()];
        let search = text_input("Search results", &self.search)
            .on_input(Msg::Search)
            .padding([sp::S2, sp::S3])
            .size(theme::size::TEXT)
            .style(theme::input_style(false))
            .width(Length::FillPortion(3));
        let status = pick_list(verifications, Some(self.verification.clone()), Msg::Verification)
            .text_size(theme::size::TEXT)
            .padding([10.0, sp::S3])
            .style(theme::pick_style)
            .menu_style(theme::menu_style)
            .width(Length::FillPortion(2));
        let mut filters = row![search, status].spacing(sp::S2).align_y(Alignment::Center);
        if !competitors {
            filters = filters.push(checkbox("Has email", self.only_email).on_toggle(Msg::OnlyEmail).style(theme::check_style).text_size(theme::size::SMALL));
        }

        let n = self.selected.len();
        let all_picked = !rows.is_empty() && rows.iter().all(|r| self.selected.contains(&r.id));
        let scope = if n == 0 { "all".to_string() } else { format!("{n} selected") };
        let can_write = ctx.can("record.write");
        let save_label = if competitors { "Mark as competitors" } else { "Send to AI Leads review" };
        let actions: Vec<Element<'_, Msg>> = vec![
            button_with(&format!("CSV ({scope})"), Some("download"), Kind::Secondary, Some(Msg::Export("csv"))).into(),
            button_with(&format!("Excel ({scope})"), Some("download"), Kind::Secondary, Some(Msg::Export("xlsx"))).into(),
            button_with(save_label, Some("plus"), Kind::Primary, if can_write && n > 0 && !self.busy_action { Some(Msg::Save) } else { None }).into(),
        ];
        let toolbar = Row::with_children(actions).spacing(sp::S2).wrap();

        // Column set: wide windows show the useful details; a phone shows name, place and status.
        type Col = (&'static str, &'static str, u16);
        let cols: Vec<Col> = if phone {
            vec![("name", "Name", 5), ("status", "Status", 3)]
        } else if competitors {
            vec![("name", "Name", 3), ("category", "Category", 2), ("location", "Location", 2), ("pricing", "Pricing", 2), ("reviews", "Reviews", 3), ("status", "Status", 2)]
        } else {
            vec![("name", "Name", 3), ("category", "Category", 2), ("location", "Location", 2), ("phone", "Phone", 2), ("email", "Email", 3), ("status", "Status", 2)]
        };
        let mut head = row![checkbox("", all_picked).on_toggle(|_| Msg::ToggleAll).style(theme::check_style)].spacing(sp::S2).align_y(Alignment::Center);
        for (key, label_text, w) in &cols {
            let arrow = match self.sort {
                Some((k, d)) if k == *key => if d { " ↓" } else { " ↑" },
                _ => "",
            };
            head = head.push(
                button(text(format!("{label_text}{arrow}")).size(theme::size::SMALL).font(theme::FONT_BOLD).color(c::MUTED))
                    .padding([sp::S2, sp::S1])
                    .width(Length::FillPortion(*w))
                    .style(theme::button_style(Kind::Menu))
                    .on_press(Msg::Sort(*key)),
            );
        }
        if !phone {
            head = head.push(Space::with_width(Length::Fixed(theme::dim::CONTROL)));
        }
        let mut table = column![container(head).padding([0.0, sp::S2]).style(theme::flat(c::SOFT))].spacing(0);
        let first = self.page.min(rows.len().saturating_sub(1) / PAGE) * PAGE;
        for r in rows.iter().skip(first).take(PAGE) {
            let picked = self.selected.contains(&r.id);
            let id = r.id.clone();
            let mut line = row![checkbox("", picked).on_toggle(move |_| Msg::Toggle(id.clone())).style(theme::check_style)].spacing(sp::S2).align_y(Alignment::Center);
            for (key, _, w) in &cols {
                let cell: Element<'_, Msg> = match *key {
                    "name" => {
                        let sub = if phone { r.location.clone() } else { String::new() };
                        let mut lines = column![text(r.name.clone()).size(theme::size::TEXT).color(c::INK)].spacing(2.0);
                        if !sub.is_empty() {
                            lines = lines.push(muted(sub));
                        }
                        button(lines).padding([sp::S2, sp::S1]).width(Length::Fill).style(theme::button_style(Kind::Menu)).on_press(Msg::Detail(Some(r.id.clone()))).into()
                    }
                    "status" => pill(&r.verification, tone_of(if r.verification == "partial" { "waiting" } else { &r.verification })),
                    "category" => cell_text(&r.category),
                    "location" => cell_text(&r.location),
                    "phone" => cell_text(&r.phone),
                    "email" => cell_text(&r.email),
                    "pricing" => cell_text(&r.pricing),
                    _ => cell_text(&r.reviews),
                };
                line = line.push(container(cell).width(Length::FillPortion(*w)));
            }
            if !phone {
                let site: Element<'_, Msg> = if r.website.is_empty() {
                    Space::with_width(Length::Fixed(theme::dim::CONTROL)).into()
                } else {
                    button(container(icon("right", theme::size::ICON, c::PRIMARY)).center_x(Length::Fixed(theme::dim::CONTROL - 8.0)).center_y(Length::Fixed(theme::dim::CONTROL - 8.0)))
                        .padding(0)
                        .style(theme::button_style(Kind::Menu))
                        .on_press(Msg::OpenUrl(r.website.clone()))
                        .into()
                };
                line = line.push(site);
            }
            table = table.push(container(line).padding([sp::S1, sp::S2]).width(Length::Fill)).push(divider());
        }

        let last = rows.len().saturating_sub(1) / PAGE;
        let page_no = self.page.min(last);
        let to = (first + PAGE).min(rows.len());
        let pager = row![
            muted(if rows.is_empty() { "No results match.".to_string() } else { format!("{} to {} of {}", first + 1, to, rows.len()) }),
            Space::with_width(Length::Fill),
            button_with("Previous", Some("left"), Kind::Secondary, if page_no > 0 { Some(Msg::Page(-1)) } else { None }),
            button_with("Next", Some("right"), Kind::Secondary, if page_no < last { Some(Msg::Page(1)) } else { None }),
        ]
        .spacing(sp::S2)
        .align_y(Alignment::Center);

        let mut out = column![card_title(format!("{} ({})", if competitors { "Competitors found" } else { "Leads found" }, self.results.len())), filters, toolbar, container(table).style(theme::card)].spacing(sp::S3);
        if rows.is_empty() {
            out = out.push(empty_state("No results match these filters."));
        }
        out = out.push(pager);
        if let Some(r) = self.open.as_ref().and_then(|id| self.results.iter().find(|r| &r.id == id)) {
            out = out.push(self.detail(r, competitors));
        }
        card(out).padding(sp::S3).into()
    }

    fn detail<'a>(&'a self, r: &'a ResearchResult, competitors: bool) -> Element<'a, Msg> {
        let line = |name: &'a str, value: &'a str| -> Element<'a, Msg> {
            let shown = if value.trim().is_empty() { "Not found".to_string() } else { value.to_string() };
            row![container(muted(name)).width(Length::FillPortion(2)), container(text(shown).size(theme::size::TEXT).color(if value.trim().is_empty() { c::MUTED } else { c::INK })).width(Length::FillPortion(7))]
                .spacing(sp::S3)
                .into()
        };
        let mut col = column![
            row![
                column![heading(r.name.clone()), muted(r.category.clone())].spacing(2.0).width(Length::Fill),
                pill(&r.verification, tone_of(if r.verification == "partial" { "waiting" } else { &r.verification })),
                button_with("Close", Some("close"), Kind::Secondary, Some(Msg::Detail(None))),
            ]
            .spacing(sp::S3)
            .align_y(Alignment::Center),
            line("Website", &r.website),
            line("Address", &r.address),
            line("Location", &r.location),
            line("Phone", &r.phone),
            line("Email", &r.email),
            line("Social profiles", &r.socials),
            line("Source page", &r.source_url),
        ]
        .spacing(sp::S2);
        if competitors {
            col = col.push(line("Services", &r.services)).push(line("Pricing", &r.pricing)).push(line("Customer reviews", &r.reviews)).push(line("Online presence", &r.presence)).push(line("Offerings", &r.offerings));
        }
        col = col.push(line("Verification notes", &r.notes));
        let mut links: Vec<Element<'_, Msg>> = vec![];
        if !r.website.is_empty() {
            links.push(button_with("Open website", Some("right"), Kind::Primary, Some(Msg::OpenUrl(r.website.clone()))).into());
        }
        if !r.source_url.is_empty() {
            links.push(button_with("Open source page", Some("right"), Kind::Secondary, Some(Msg::OpenUrl(r.source_url.clone()))).into());
        }
        for s in r.socials.lines().filter(|s| !s.is_empty()).take(4) {
            links.push(button_with(&social_label(s), Some("right"), Kind::Secondary, Some(Msg::OpenUrl(s.to_string()))).into());
        }
        if !r.saved_as.is_empty() {
            col = col.push(muted(if r.saved_as == "competitor" { "Marked as a competitor in Companies." } else { "Sent to AI Leads for review." }));
        }
        container(column![col, Row::with_children(links).spacing(sp::S2).wrap()].spacing(sp::S3)).padding(sp::S3).width(Length::Fill).style(theme::panel(c::PAGE)).into()
    }
}

fn cell_text<'a>(value: &str) -> Element<'a, Msg> {
    if value.is_empty() {
        muted("Not found").into()
    } else {
        text(value.to_string()).size(theme::size::SMALL).color(c::INK).into()
    }
}

fn social_label(url: &str) -> String {
    let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).trim_start_matches("www.");
    let name = host.split('.').next().unwrap_or(host);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => format!("Open {}{}", first.to_uppercase(), chars.as_str()),
        None => "Open profile".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(name: &str, email: &str, verification: &str) -> ResearchResult {
        ResearchResult { id: name.into(), name: name.into(), email: email.into(), verification: verification.into(), ..Default::default() }
    }

    fn state() -> State {
        let (mut s, _) = init("leads");
        s.results = vec![result("Cafe Zed", "", "partial"), result("Bistro Amy", "a@b.co", "verified"), result("Diner Max", "m@d.co", "unverified")];
        s
    }

    #[test]
    fn filters_narrow_the_results() {
        let mut s = state();
        s.verification = "Verified".into();
        assert_eq!(s.visible().len(), 1);
        s.verification = ANY.into();
        s.only_email = true;
        assert_eq!(s.visible().len(), 2);
        s.only_email = false;
        s.search = "zed".into();
        assert_eq!(s.visible()[0].name, "Cafe Zed");
    }

    #[test]
    fn sorting_goes_up_then_down() {
        let mut s = state();
        s.sort = Some(("name", false));
        assert_eq!(s.visible()[0].name, "Bistro Amy");
        s.sort = Some(("name", true));
        assert_eq!(s.visible()[0].name, "Diner Max");
        s.sort = Some(("status", false));
        assert_eq!(s.visible()[0].verification, "verified");
    }

    #[test]
    fn social_links_get_a_readable_label() {
        assert_eq!(social_label("https://www.instagram.com/cafe"), "Open Instagram");
        assert_eq!(social_label("https://facebook.com/x"), "Open Facebook");
    }
}
