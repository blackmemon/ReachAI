use crate::api;
use crate::components::*;
use crate::theme::sp;
use iced::widget::column;
use iced::{Element, Task};
use shared::{ActionResult, ApiError};

#[derive(Default)]
pub struct State {
    pub current: String,
    pub new: String,
    pub again: String,
    pub note: Option<(bool, String)>,
    pub busy: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Current(String),
    New(String),
    Again(String),
    Save,
    Done(Result<ActionResult, ApiError>),
}

impl State {
    pub fn update(&mut self, msg: Msg, ctx: &mut crate::app::ctx::Ctx) -> Task<Msg> {
        match msg {
            Msg::Current(v) => self.current = v,
            Msg::New(v) => self.new = v,
            Msg::Again(v) => self.again = v,
            Msg::Save => {
                if self.new.chars().count() < 12 {
                    self.note = Some((false, "The new password needs at least 12 characters.".into()));
                } else if self.new != self.again {
                    self.note = Some((false, "The two new passwords are not the same.".into()));
                } else if !self.busy {
                    self.busy = true;
                    let csrf = ctx.csrf();
                    let body = serde_json::json!({"current": self.current, "new": self.new});
                    return Task::perform(async move { api::send::<ActionResult, _>("POST", "/api/v1/auth/password", &csrf, &body).await }, Msg::Done);
                }
            }
            Msg::Done(r) => {
                self.busy = false;
                match r {
                    Ok(a) => {
                        self.note = Some((true, a.message));
                        self.current.clear();
                        self.new.clear();
                        self.again.clear();
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

    pub fn view<'a>(&'a self, ctx: &'a crate::app::ctx::Ctx) -> Element<'a, Msg> {
        let s = &ctx.session;
        let mut pw = column![card_title("Change password")].spacing(sp::S3);
        if let Some((ok, m)) = &self.note {
            pw = pw.push(banner(if *ok { Tone::Green } else { Tone::Red }, m));
        }
        pw = pw
            .push(field("Current password", input("", &self.current, Msg::Current, false).secure(true).on_submit(Msg::Save), None, ""))
            .push(field("New password", input("", &self.new, Msg::New, false).secure(true).on_submit(Msg::Save), None, "At least 12 characters."))
            .push(field("New password again", input("", &self.again, Msg::Again, false).secure(true).on_submit(Msg::Save), None, ""))
            .push(button_with(if self.busy { "Saving" } else { "Change password" }, None, crate::theme::Kind::Primary, if self.busy { None } else { Some(Msg::Save) }));
        let about = column![card_title("You"), body(format!("{}  ·  {}", s.name, s.email)), muted(format!("Role: {}  ·  Company: {}", s.role, s.tenant_name)), muted(format!("Times are shown in your device time zone (UTC{:+}:{:02}).", ctx.tz / 60, (ctx.tz % 60).abs()))].spacing(sp::S2);
        column![page_header("My profile", "", vec![]), card(about), card(pw)].spacing(sp::S4).max_width(crate::theme::dim::FORM_MAX).into()
    }
}
