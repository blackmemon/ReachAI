pub mod workspace;

use crate::api;
use crate::app::ctx::Ctx;
use crate::components::*;
use crate::pages::table;
use crate::theme::{sp, Kind};
use iced::widget::{column, row};
use iced::{Element, Task};
use shared::{ActionResult, ApiError};

pub const TABS: &[(&str, &str)] = &[
    ("ai-leads", "AI Leads"),
    ("competitors", "Competitors"),
    ("data-collections", "Data Collections"),
];

pub struct State {
    pub tab: String,
    pub collection_view: String,
    pub list: table::State,
    pub work: Option<workspace::State>,
    pub good: String,
    pub note: Option<(bool, String)>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(String),
    CollectionView(String),
    Table(table::Msg),
    Work(workspace::Msg),
    Good(String),
    Mark,
    Marked(Result<ActionResult, ApiError>),
}

fn module_for(tab: &str, collection_view: &str) -> &'static str {
    match tab {
        "ai-leads" => "candidates",
        "competitors" => "companies",
        "data-collections" if collection_view == "runs" => "research_runs",
        "data-collections" => "sources",
        _ => "candidates",
    }
}

/// The chat workspace belongs to the AI Leads and Competitors tabs, one mode each.
fn mode_for(tab: &str) -> Option<&'static str> {
    match tab {
        "ai-leads" => Some("leads"),
        "competitors" => Some("competitors"),
        _ => None,
    }
}

fn start_work(tab: &str) -> (Option<workspace::State>, Task<Msg>) {
    match mode_for(tab) {
        Some(mode) => {
            let (w, t) = workspace::init(mode);
            (Some(w), t.map(Msg::Work))
        }
        None => (None, Task::none()),
    }
}

pub fn init(tab: Option<String>) -> (State, Task<Msg>) {
    let tab = tab.filter(|t| TABS.iter().any(|x| x.0 == t)).unwrap_or_else(|| "ai-leads".into());
    let collection_view = "sources".to_string();
    let (list, t) = table::State::new(&tab, module_for(&tab, &collection_view), None);
    let (work, wt) = start_work(&tab);
    (State { tab, collection_view, list, work, good: String::new(), note: None }, Task::batch([t.map(Msg::Table), wt]))
}

impl State {
    pub fn update(&mut self, msg: Msg, ctx: &mut Ctx) -> Task<Msg> {
        match msg {
            Msg::Tab(t) => {
                ctx.go(&format!("/research/{t}"));
                self.collection_view = "sources".into();
                let (list, task) = table::State::new(&t, module_for(&t, &self.collection_view), None);
                self.list = list;
                let (work, wt) = start_work(&t);
                self.work = work;
                self.tab = t;
                return Task::batch([task.map(Msg::Table), wt]);
            }
            Msg::CollectionView(view) => {
                if matches!(view.as_str(), "sources" | "runs") && view != self.collection_view {
                    let (list, task) = table::State::new("data-collections", module_for("data-collections", &view), None);
                    self.collection_view = view;
                    self.list = list;
                    return task.map(Msg::Table);
                }
            }
            Msg::Table(m) => return self.list.update(m, ctx).map(Msg::Table),
            Msg::Work(m) => {
                let saved = matches!(m, workspace::Msg::Saved(Ok(_)));
                let mut tasks = vec![];
                if let Some(w) = self.work.as_mut() {
                    tasks.push(w.update(m, ctx).map(Msg::Work));
                }
                if saved {
                    // Results were sent to the review list or marked as competitors: show them there.
                    let (list, task) = table::State::new(&self.tab, module_for(&self.tab, &self.collection_view), None);
                    self.list = list;
                    tasks.push(task.map(Msg::Table));
                }
                return Task::batch(tasks);
            }
            Msg::Good(v) => self.good = v.chars().filter(|c| c.is_ascii_digit()).take(2).collect(),
            Msg::Mark => {
                let good: i64 = self.good.parse().unwrap_or(-1);
                if !(0..=10).contains(&good) {
                    self.note = Some((false, "Enter how many of the researched companies were good (0 to 10).".into()));
                    return Task::none();
                }
                let csrf = ctx.csrf();
                return Task::perform(
                    async move { api::send::<ActionResult, _>("POST", "/api/v1/research/sample", &csrf, &serde_json::json!({"good": good})).await },
                    Msg::Marked,
                );
            }
            Msg::Marked(Ok(a)) => self.note = Some((true, a.message)),
            Msg::Marked(Err(e)) => {
                self.note = Some((false, e.message.clone()));
                ctx.failed(&e);
            }
        }
        Task::none()
    }

    pub fn overlay<'a>(&'a self, ctx: &'a Ctx) -> Option<Element<'a, Msg>> {
        self.list.overlay(ctx).map(|e| e.map(Msg::Table))
    }

    pub fn view<'a>(&'a self, ctx: &'a Ctx) -> Element<'a, Msg> {
        let mut tabs = row![].spacing(sp::S1);
        for (key, title) in TABS {
            tabs = tabs.push(button_with(title, None, if *key == self.tab { Kind::MenuActive } else { Kind::Menu }, Some(Msg::Tab((*key).into()))));
        }
        let mut page = column![
            page_header("AI Leads", "Find business leads and research competitors by chatting with Claude CLI.", vec![]),
            tabs
        ]
        .spacing(sp::S4);

        if let Some(work) = &self.work {
            if ctx.can("ai.use") && ctx.can("record.write") {
                page = page.push(work.view(ctx).map(Msg::Work));
            } else {
                page = page.push(banner(Tone::Amber, "AI research requires AI use and record write permission."));
            }
        }

        if self.tab == "data-collections" {
            let subtabs = row![
                button_with("Sources", None, if self.collection_view == "sources" { Kind::MenuActive } else { Kind::Menu }, Some(Msg::CollectionView("sources".into()))),
                button_with("Collection runs", None, if self.collection_view == "runs" { Kind::MenuActive } else { Kind::Menu }, Some(Msg::CollectionView("runs".into()))),
            ]
            .spacing(sp::S1);
            page = page.push(subtabs);
        }
        if self.tab == "ai-leads" && ctx.can("record.write") {
            let mut c = column![
                card_title("Sample check"),
                muted("Mark how many of the first researched companies were good. Until 5 are marked, research stops at 10 companies and campaigns cannot start.")
            ]
            .spacing(sp::S2);
            if let Some((ok, message)) = &self.note {
                c = c.push(banner(if *ok { Tone::Green } else { Tone::Red }, message));
            }
            c = c.push(row![input("0 to 10", &self.good, Msg::Good, false).on_submit(Msg::Mark).width(120.0), button_with("Mark sample check", None, Kind::Secondary, Some(Msg::Mark))].spacing(sp::S2));
            page = page.push(card(c));
        }
        let list_title = match self.tab.as_str() {
            "ai-leads" => Some(("Leads waiting for review", "Businesses you sent from a conversation. Approve the good ones to add them as companies.")),
            "competitors" => Some(("Competitors in Companies", "Companies marked as competitors, from research or by hand.")),
            _ => None,
        };
        if let Some((title, hint)) = list_title {
            page = page.push(column![heading(title), muted(hint)].spacing(sp::S1));
        }
        page.push(self.list.view(ctx).map(Msg::Table)).into()
    }
}
