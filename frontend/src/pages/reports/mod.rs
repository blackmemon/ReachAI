use crate::api;
use crate::components::*;
use crate::theme::{self, c, sp, Kind};
use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length, Task};
use shared::{ApiError, ReportSet};

pub struct State {
    pub tab: String,
    pub data: Option<ReportSet>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Result<ReportSet, ApiError>),
    Tab(String),
}

pub fn init(tab: Option<String>) -> (State, Task<Msg>) {
    let tab = tab.unwrap_or_else(|| "Leads".into());
    let t = load(&tab);
    (State { tab, data: None, error: None }, t)
}

fn load(tab: &str) -> Task<Msg> {
    let path = format!("/api/v1/reports/{}", api::path_segment(tab));
    Task::perform(async move { api::get::<ReportSet>(&path).await }, Msg::Loaded)
}

impl State {
    pub fn update(&mut self, msg: Msg, ctx: &mut crate::app::ctx::Ctx) -> Task<Msg> {
        match msg {
            Msg::Loaded(Ok(d)) => {
                self.error = None;
                self.data = Some(d)
            }
            Msg::Loaded(Err(e)) => {
                self.error = Some(e.message.clone());
                ctx.failed(&e)
            }
            Msg::Tab(t) => {
                self.tab = t.clone();
                self.data = None;
                ctx.go(&format!("/reports/{}", t.replace(' ', "%20")));
                return load(&t);
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut page = column![page_header("Reports", "Numbers from your own data. Test emails are never counted.", vec![])].spacing(sp::S4);
        if let Some(d) = &self.data {
            let mut tabs = row![].spacing(sp::S1);
            for t in &d.tabs {
                tabs = tabs.push(button_with(t, None, if *t == self.tab { Kind::MenuActive } else { Kind::Menu }, Some(Msg::Tab(t.clone()))));
            }
            page = page.push(scrollable(tabs).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new())));
            page = page.push(tile_grid(d.tiles.iter().map(|t| stat_tile(&t.label, &t.value)).collect(), 4));
            for t in &d.tables {
                let col_w = Length::Fixed(((theme::viewport_width() - 120.0) / t.columns.len().max(1) as f32).clamp(110.0, 260.0));
                let mut head = row![].spacing(sp::S2);
                for h in &t.columns {
                    head = head.push(container(text(h.clone()).size(theme::size::SMALL).font(theme::FONT_BOLD).color(c::MUTED)).width(col_w));
                }
                let mut rows = column![head].spacing(sp::S1);
                if t.rows.is_empty() {
                    rows = rows.push(muted("No data yet."));
                }
                for r in &t.rows {
                    let mut line = row![].spacing(sp::S2);
                    for v in r {
                        line = line.push(container(body(v.clone())).width(col_w));
                    }
                    rows = rows.push(line);
                }
                let inner = column![scrollable(rows).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new())), muted(t.note.clone())].spacing(sp::S2);
                page = page.push(card_titled(&t.title, "", inner));
            }
        } else if let Some(e) = &self.error {
            page = page.push(banner(Tone::Red, e));
        } else {
            page = page.push(body("Loading"));
        }
        page.into()
    }
}
