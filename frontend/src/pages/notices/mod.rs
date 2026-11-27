use crate::api;
use crate::components::*;
use crate::format;
use crate::theme::{self, sp, Kind};
use iced::widget::{column, row, Space};
use iced::{Element, Length, Task};
use shared::{ActionResult, ApiError, Notice};

#[derive(Default)]
pub struct State {
    pub items: Vec<Notice>,
    pub loaded: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Result<Vec<Notice>, ApiError>),
    Read(String),
    Marked(Result<ActionResult, ApiError>),
}

pub fn init() -> (State, Task<Msg>) {
    (State::default(), Task::perform(api::get::<Vec<Notice>>("/api/v1/notices"), Msg::Loaded))
}

impl State {
    pub fn update(&mut self, msg: Msg, ctx: &mut crate::app::ctx::Ctx) -> Task<Msg> {
        match msg {
            Msg::Loaded(Ok(v)) => {
                self.items = v;
                self.loaded = true;
            }
            Msg::Loaded(Err(e)) => ctx.failed(&e),
            Msg::Read(id) => {
                let csrf = ctx.csrf();
                let path = format!("/api/v1/notices/read?id={}", api::path_segment(&id));
                return Task::perform(async move { api::post::<ActionResult>(&path, &csrf).await }, Msg::Marked);
            }
            Msg::Marked(Ok(_)) => {
                ctx.refresh_session();
                return Task::perform(api::get::<Vec<Notice>>("/api/v1/notices"), Msg::Loaded);
            }
            Msg::Marked(Err(e)) => ctx.failed(&e),
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, ctx: &'a crate::app::ctx::Ctx) -> Element<'a, Msg> {
        let mut list = column![].spacing(sp::S2);
        if self.items.is_empty() {
            list = list.push(empty_state(if self.loaded { "No notices." } else { "Loading" }));
        }
        for n in &self.items {
            let mut r = row![pill(&n.kind, tone_of(&n.kind)), column![if n.unread { strong(n.title.clone()) } else { body(n.title.clone()) }, muted(n.body.clone())].spacing(2), Space::with_width(Length::Fill), muted(format::local_time(&n.when, ctx.tz))].spacing(sp::S3).align_y(iced::Alignment::Center);
            if n.unread {
                r = r.push(button_with("Mark as read", None, Kind::Secondary, Some(Msg::Read(n.id.clone()))));
            }
            list = list.push(r).push(divider());
        }
        column![page_header("Notices", "Things the app wants you to know.", vec![]), card(list)].spacing(sp::S4).into()
    }
}

#[allow(dead_code)]
fn _t() {
    let _ = theme::dim::ROW;
}
