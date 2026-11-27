use crate::api;
use crate::components::*;
use crate::theme::sp;
use iced::widget::{column, container};
use iced::{Element, Length, Task};
use shared::{ActionResult, ApiError};

#[derive(Debug, PartialEq)]
pub enum Phase {
    Checking,
    Ready,
    Invalid,
    Working,
    Done(String),
    Failed(String),
}

pub struct State {
    token: String,
    pub phase: Phase,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Checked(Result<ActionResult, ApiError>),
    Confirm,
    Done(Result<ActionResult, ApiError>),
}

pub fn init(token: String) -> (State, Task<Msg>) {
    let path = format!("/api/v1/public/unsubscribe/{}", api::path_segment(&token));
    (State { token, phase: Phase::Checking }, Task::perform(async move { api::get::<ActionResult>(&path).await }, Msg::Checked))
}

impl State {
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Checked(Ok(_)) => self.phase = Phase::Ready,
            Msg::Checked(Err(_)) => self.phase = Phase::Invalid,
            Msg::Confirm => {
                self.phase = Phase::Working;
                let path = format!("/api/v1/public/unsubscribe/{}", api::path_segment(&self.token));
                return Task::perform(async move { api::send::<ActionResult, _>("POST", &path, "", &serde_json::json!({})).await }, Msg::Done);
            }
            Msg::Done(Ok(r)) => self.phase = Phase::Done(r.message),
            Msg::Done(Err(e)) => self.phase = Phase::Failed(e.message),
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let inner: Element<'_, Msg> = match &self.phase {
            Phase::Checking | Phase::Working => body("Please wait").into(),
            Phase::Ready => column![
                card_title("Unsubscribe"),
                body("Press the button to stop all email from us to this address. You will not hear from us again."),
                button_with("Unsubscribe", None, crate::theme::Kind::Primary, Some(Msg::Confirm)),
            ]
            .spacing(sp::S3)
            .into(),
            Phase::Invalid => column![card_title("This link is not valid"), body("The link may be incomplete. Please use the link in the email, or reply to the email and ask us to stop.")].spacing(sp::S3).into(),
            Phase::Done(m) => column![card_title("Done"), banner(Tone::Green, m)].spacing(sp::S3).into(),
            Phase::Failed(m) => column![card_title("Something went wrong"), banner(Tone::Red, m), secondary("Try again", Msg::Confirm)].spacing(sp::S3).into(),
        };
        container(container(card(inner).padding(sp::S5)).max_width(480.0).width(Length::Fill)).width(Length::Fill).center_x(Length::Fill).padding([sp::S6, 0.0]).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_check_and_confirm_flow() {
        let (mut s, _) = init("a.b.c".into());
        assert_eq!(s.phase, Phase::Checking);
        let _ = s.update(Msg::Checked(Err(ApiError::new("not_found", "x"))));
        assert_eq!(s.phase, Phase::Invalid);
        let _ = s.update(Msg::Checked(Ok(ActionResult { ok: true, message: "ok".into() })));
        assert_eq!(s.phase, Phase::Ready);
        let _ = s.update(Msg::Confirm);
        assert_eq!(s.phase, Phase::Working);
        let _ = s.update(Msg::Done(Ok(ActionResult { ok: true, message: "You are unsubscribed.".into() })));
        assert_eq!(s.phase, Phase::Done("You are unsubscribed.".into()));
    }
}
