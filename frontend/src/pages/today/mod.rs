use crate::api;
use crate::components::*;
use crate::theme::{self, sp};
use iced::widget::{button, column, row, Space};
use iced::{Element, Length, Task};
use shared::{ApiError, TodayData};

#[derive(Default)]
pub struct State {
    pub data: Option<TodayData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Result<TodayData, ApiError>),
    Open(String),
}

pub fn init() -> (State, Task<Msg>) {
    (State::default(), Task::perform(api::get::<TodayData>("/api/v1/today"), Msg::Loaded))
}

impl State {
    /// Returns a path to open, if the person picked an item.
    pub fn update(&mut self, msg: Msg, ctx: &mut crate::app::ctx::Ctx) {
        match msg {
            Msg::Loaded(Ok(d)) => self.data = Some(d),
            Msg::Loaded(Err(e)) => {
                self.error = Some(e.message.clone());
                ctx.failed(&e);
            }
            Msg::Open(p) => ctx.go(&p),
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut page = column![page_header("Today", "What needs you now and how outreach is going.", vec![])].spacing(sp::S4);
        if let Some(e) = &self.error {
            page = page.push(banner(Tone::Red, e));
        }
        let Some(d) = &self.data else { return page.push(body("Loading")).into() };
        page = page.push(tile_grid(d.tiles.iter().map(|t| stat_tile(&t.label, &t.value)).collect(), 5));
        let mut needs = column![].spacing(sp::S2);
        if d.needs_you.is_empty() {
            needs = needs.push(empty_state("Nothing needs you right now."));
        }
        for n in &d.needs_you {
            let b = button(row![pill(&n.kind, tone_of(&n.kind)), column![strong(n.title.clone()), muted(n.detail.clone())].spacing(2), Space::with_width(Length::Fill), muted(n.when.clone())].spacing(sp::S3).align_y(iced::Alignment::Center))
                .width(Length::Fill)
                .padding([sp::S2, sp::S3])
                .style(theme::button_style(theme::Kind::Menu))
                .on_press(Msg::Open(n.link.clone()));
            needs = needs.push(b);
        }
        page = page.push(card_titled("Needs you", "Open an item to act on it.", needs));
        let stages = tile_grid(d.stages.iter().map(|t| stat_tile(&t.label, &t.value)).collect(), 4);
        page = page.push(card_titled("Pipeline", "People by stage.", stages));
        if !d.runs.is_empty() {
            let mut runs = column![].spacing(sp::S1);
            for r in &d.runs {
                runs = runs.push(body(r.join("  ·  ")));
            }
            page = page.push(card_titled("Recent research runs", "", runs));
        }
        page.into()
    }
}
