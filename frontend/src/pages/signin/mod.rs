use crate::api;
use crate::components::*;
use crate::theme::{self, sp};
use iced::widget::{column, container};
use iced::{Element, Length, Task};
use shared::{ApiError, CsrfToken, LoginRequest, SessionInfo};

#[derive(Default)]
pub struct State {
    pub email: String,
    pub password: String,
    csrf: String,
    error: Option<String>,
    busy: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Csrf(Result<CsrfToken, ApiError>),
    Email(String),
    Password(String),
    Submit,
    Done(Result<SessionInfo, ApiError>),
}

pub fn init() -> (State, Task<Msg>) {
    (State::default(), Task::perform(api::get::<CsrfToken>("/api/v1/auth/csrf"), Msg::Csrf))
}

impl State {
    pub fn ready(&self) -> bool {
        !self.busy && !self.email.trim().is_empty() && !self.password.is_empty() && !self.csrf.is_empty()
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Csrf(Ok(t)) => self.csrf = t.token,
            Msg::Csrf(Err(e)) => self.error = Some(e.message),
            Msg::Email(v) => self.email = v,
            Msg::Password(v) => self.password = v,
            Msg::Submit => {
                if !self.ready() {
                    if self.email.trim().is_empty() || self.password.is_empty() {
                        self.error = Some("Enter your work email and your password.".into());
                    }
                    return Task::none();
                }
                self.busy = true;
                self.error = None;
                let req = LoginRequest { email: self.email.trim().to_string(), password: self.password.clone() };
                let csrf = self.csrf.clone();
                return Task::perform(async move { api::send::<SessionInfo, _>("POST", "/api/v1/auth/login", &csrf, &req).await }, Msg::Done);
            }
            Msg::Done(Ok(_)) => {
                // A full page load gives the signed-in bundle.
                crate::browser::navigate_hard("/");
            }
            Msg::Done(Err(e)) => {
                self.busy = false;
                self.password.clear();
                self.error = Some(e.message);
                if e.code == "forbidden" {
                    // The pre-sign-in token expired: get a new one.
                    return Task::perform(api::get::<CsrfToken>("/api/v1/auth/csrf"), Msg::Csrf);
                }
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut form = column![card_title("Sign in")].spacing(sp::S4);
        if let Some(e) = &self.error {
            form = form.push(banner::<Msg>(Tone::Red, e));
        }
        form = form
            .push(field(
                "Work email",
                input("name@company.com", &self.email, Msg::Email, false).on_submit(Msg::Submit),
                None,
                "",
            ))
            .push(field(
                "Password",
                input("Your password", &self.password, Msg::Password, false).secure(true).on_submit(Msg::Submit),
                None,
                "",
            ))
            .push(button_wide(if self.busy { "Signing in" } else { "Sign in" }, theme::Kind::Primary, if self.ready() { Some(Msg::Submit) } else { None }))
            .push(muted("There is no public sign-up. Ask your admin for an invitation."));
        container(container(card(form).padding(sp::S5)).max_width(420.0).width(Length::Fill)).width(Length::Fill).center_x(Length::Fill).padding([sp::S6, 0.0]).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_both_fields_and_a_token() {
        let mut s = State::default();
        assert!(!s.ready());
        s.email = "a@b.com".into();
        s.password = "x".into();
        assert!(!s.ready(), "no CSRF token yet");
        s.csrf = "t".into();
        assert!(s.ready());
        s.busy = true;
        assert!(!s.ready(), "no double submit");
    }

    #[test]
    fn empty_submit_shows_a_message_and_sends_nothing() {
        let mut s = State::default();
        let _ = s.update(Msg::Submit);
        assert!(s.error.is_some() && !s.busy);
    }

    #[test]
    fn failed_login_clears_the_password_and_shows_the_server_message() {
        let mut s = State { email: "a@b.com".into(), password: "secret".into(), csrf: "t".into(), busy: true, ..Default::default() };
        let _ = s.update(Msg::Done(Err(ApiError::new("bad_request", "Email or password is incorrect."))));
        assert!(s.password.is_empty() && !s.busy);
        assert_eq!(s.error.as_deref(), Some("Email or password is incorrect."));
    }
}
