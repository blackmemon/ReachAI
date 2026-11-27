//! The application root: addresses, the session, the frame and the current page.

pub mod ctx;
pub mod route;

use crate::api;
use crate::browser;
use crate::components::{banner, body, Tone};
use crate::pages::*;
use crate::shell::{self, ShellMsg, ShellState};
use crate::theme;
use ctx::Ctx;
use iced::{Element, Size, Subscription, Task};
use route::Route;
use shared::*;

/// The public bundle only knows sign-in, legal pages and unsubscribe.
pub const PUBLIC_ONLY: bool = cfg!(feature = "public") && !cfg!(feature = "app");

pub enum Page {
    Loading,
    Failed(String),
    SignIn(signin::State),
    Legal(legal::Kind),
    Unsubscribe(unsubscribe::State),
    Today(today::State),
    Table(Box<table::State>),
    Quick(quick::State),
    Settings(Box<settings::State>),
    Reports(reports::State),
    Research(Box<research::State>),
    Notices(notices::State),
    Profile(profile::State),
    Help,
    NotFound,
}

pub struct App {
    pub ctx: Option<Ctx>,
    pub route: Route,
    pub page: Page,
    pub shell: ShellState,
    width: f32,
    scale: f32,
    toast: Option<(String, Tone)>,
    toast_id: u64,
    pending_search: Option<String>,
    last_path: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Session(Result<SessionInfo, ApiError>),
    Modules(Box<SessionInfo>, Result<Vec<ModuleInfo>, ApiError>),
    SessionAgain(Result<SessionInfo, ApiError>),
    Resized(Size),
    Escape,
    FocusNext,
    FocusPrevious,
    HistoryChanged,
    ToastOff(u64),
    Shell(ShellMsg),
    Go(String),
    Sent(Result<ActionResult, ApiError>),
    SignedOut,
    SignIn(signin::Msg),
    Unsubscribe(unsubscribe::Msg),
    Today(today::Msg),
    Table(table::Msg),
    Quick(quick::Msg),
    Settings(settings::Msg),
    Reports(reports::Msg),
    Research(research::Msg),
    Notices(notices::Msg),
    Profile(profile::Msg),
}

pub fn boot() -> (App, Task<Message>) {
    let route = route::parse(&browser::pathname());
    let mut app = App {
        ctx: None,
        route: route.clone(),
        page: Page::Loading,
        shell: ShellState::default(),
        width: 1280.0,
        scale: theme::root_scale(browser::inner_width()),
        toast: None,
        toast_id: 0,
        pending_search: None,
        last_path: browser::pathname(),
    };
    let size = iced::window::get_oldest()
        .and_then(iced::window::get_size)
        .map(Message::Resized);
    let first = if PUBLIC_ONLY {
        if route::is_public(&route) {
            app.open(route)
        } else {
            browser::navigate_hard("/sign-in");
            Task::none()
        }
    } else if matches!(route, Route::SignIn) {
        // Already running the signed-in bundle: a signed-in person does not need the form.
        Task::perform(api::get::<SessionInfo>("/api/v1/session"), Message::Session)
    } else {
        Task::perform(api::get::<SessionInfo>("/api/v1/session"), Message::Session)
    };
    (app, Task::batch([size, first]))
}

pub fn subscription(_: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::window::resize_events().map(|(_, s)| Message::Resized(s)),
        Subscription::run(browser::popstate).map(|_| Message::HistoryChanged),
        iced::keyboard::on_key_press(|key, modifiers| match key {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => Some(Message::Escape),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) if modifiers.shift() => {
                Some(Message::FocusPrevious)
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) => Some(Message::FocusNext),
            _ => None,
        }),
    ])
}

pub fn title(app: &App) -> String {
    let t = match &app.route {
        Route::Today => "Today".to_string(),
        Route::SignIn => "Sign in".into(),
        Route::Privacy => "Privacy notice".into(),
        Route::Terms => "Terms of use".into(),
        Route::Cookies => "Cookie notice".into(),
        Route::Unsubscribe(_) => "Unsubscribe".into(),
        Route::Table(k) => shared::nav::label_for(k).to_string(),
        Route::Quick(_) => "Quick".into(),
        Route::Settings(_) => "Settings".into(),
        Route::Reports(_) => "Reports".into(),
        Route::Research(_) => "AI Leads".into(),
        Route::Notices => "Notices".into(),
        Route::Profile => "My profile".into(),
        Route::Help => "Help".into(),
        Route::NotFound => "Page not found".into(),
    };
    format!("{t} · blackmemon ReachAI")
}

impl App {
    fn sidebar_for(width: f32, collapsed: bool) -> f32 {
        if width < theme::dim::PHONE {
            0.0
        } else if collapsed {
            theme::dim::SIDEBAR_COLLAPSED
        } else {
            theme::dim::SIDEBAR
        }
    }

    /// Start the page for a route. Signed-in pages need a session.
    fn open(&mut self, r: Route) -> Task<Message> {
        self.route = r.clone();
        self.shell.menu_open = false;
        self.shell.user_menu = false;
        let path = route::path(&r);
        if self.last_path != path {
            browser::push_path(&path);
            self.last_path = path;
        }
        browser::set_title(&title(self));
        match r {
            Route::SignIn => {
                let (s, t) = signin::init();
                self.page = Page::SignIn(s);
                t.map(Message::SignIn)
            }
            Route::Privacy => self.legal(legal::Kind::Privacy),
            Route::Terms => self.legal(legal::Kind::Terms),
            Route::Cookies => self.legal(legal::Kind::Cookies),
            Route::Unsubscribe(t) => {
                let (s, task) = unsubscribe::init(t);
                self.page = Page::Unsubscribe(s);
                task.map(Message::Unsubscribe)
            }
            _ if PUBLIC_ONLY => {
                browser::navigate_hard(&route::path(&r));
                Task::none()
            }
            Route::Today => {
                let (s, t) = today::init();
                self.page = Page::Today(s);
                t.map(Message::Today)
            }
            Route::Table(k) => {
                let module = route::table_module(&k).unwrap_or("people");
                let search = self.pending_search.take();
                let (s, t) = table::State::new(&k, module, search);
                self.page = Page::Table(Box::new(s));
                t.map(Message::Table)
            }
            Route::Quick(tab) => {
                let (s, t) = quick::init(tab);
                self.page = Page::Quick(s);
                t.map(Message::Quick)
            }
            Route::Settings(g) => {
                let (s, t) = settings::init(g);
                self.page = Page::Settings(Box::new(s));
                t.map(Message::Settings)
            }
            Route::Reports(tab) => {
                let (s, t) = reports::init(tab);
                self.page = Page::Reports(s);
                t.map(Message::Reports)
            }
            Route::Research(tab) => {
                let (s, t) = research::init(tab);
                self.page = Page::Research(Box::new(s));
                t.map(Message::Research)
            }
            Route::Notices => {
                let (s, t) = notices::init();
                self.page = Page::Notices(s);
                t.map(Message::Notices)
            }
            Route::Profile => {
                self.page = Page::Profile(profile::State::default());
                Task::none()
            }
            Route::Help => {
                self.page = Page::Help;
                Task::none()
            }
            Route::NotFound => {
                self.page = Page::NotFound;
                Task::none()
            }
        }
    }

    fn legal(&mut self, k: legal::Kind) -> Task<Message> {
        self.page = Page::Legal(k);
        Task::none()
    }

    fn navigate(&mut self, path: &str) -> Task<Message> {
        let r = route::parse(path);
        if self.ctx.is_some() && matches!(r, Route::SignIn) {
            return self.open(Route::Today);
        }
        self.open(r)
    }

    /// Things a page asked for through the context: a new address, a message, a sign-out, a fresh session.
    fn after(&mut self) -> Task<Message> {
        let mut tasks = vec![];
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.width = self.width;
            ctx.sidebar = Self::sidebar_for(self.width, self.shell.collapsed);
            if ctx.expired {
                ctx.expired = false;
                browser::navigate_hard("/sign-in");
            }
            if let Some(t) = ctx.toast.take() {
                self.toast_id += 1;
                let id = self.toast_id;
                self.toast = Some(t);
                tasks.push(Task::perform(browser::sleep(4500), move |_| {
                    Message::ToastOff(id)
                }));
            }
            if ctx.reload_session {
                ctx.reload_session = false;
                tasks.push(Task::perform(
                    api::get::<SessionInfo>("/api/v1/session"),
                    Message::SessionAgain,
                ));
            }
            if let Some(p) = ctx.go.take() {
                tasks.push(self.navigate(&p));
            }
        }
        Task::batch(tasks)
    }

    fn csrf(&self) -> String {
        self.ctx.as_ref().map(|c| c.csrf()).unwrap_or_default()
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        let t = self.update_inner(msg);
        Task::batch([t, self.after()])
    }

    fn update_inner(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Session(Ok(s)) => {
                Task::perform(api::get::<Vec<ModuleInfo>>("/api/v1/modules"), move |m| {
                    Message::Modules(Box::new(s.clone()), m)
                })
            }
            Message::Session(Err(e)) => {
                if e.code == "unauthorized" {
                    let r = self.route.clone();
                    if route::is_public(&r) && !matches!(r, Route::SignIn) {
                        return self.open(r);
                    }
                    browser::navigate_hard("/sign-in");
                    Task::none()
                } else {
                    self.page = Page::Failed(e.message);
                    Task::none()
                }
            }
            Message::Modules(s, Ok(m)) => {
                self.ctx = Some(Ctx::new(*s, m, browser::tz_offset_minutes(), self.width));
                if let Some(c) = self.ctx.as_mut() {
                    c.sidebar = Self::sidebar_for(self.width, self.shell.collapsed);
                }
                let r = if matches!(self.route, Route::SignIn) {
                    Route::Today
                } else {
                    self.route.clone()
                };
                self.open(r)
            }
            Message::Modules(_, Err(e)) => {
                self.page = Page::Failed(e.message);
                Task::none()
            }
            Message::SessionAgain(Ok(s)) => {
                if let Some(c) = self.ctx.as_mut() {
                    c.session = s;
                }
                Task::none()
            }
            Message::SessionAgain(Err(e)) => {
                if let Some(c) = self.ctx.as_mut() {
                    c.failed(&e);
                }
                Task::none()
            }
            Message::Resized(s) => {
                self.width = s.width;
                self.scale = theme::root_scale(browser::inner_width());
                theme::set_width(s.width);
                theme::set_height(s.height);
                if let Some(c) = self.ctx.as_mut() {
                    c.width = s.width;
                    c.sidebar = Self::sidebar_for(s.width, self.shell.collapsed);
                }
                Task::none()
            }
            Message::Escape => {
                self.shell.menu_open = false;
                self.shell.user_menu = false;
                self.shell.confirm_pause = false;
                if let Page::Table(t) = &mut self.page {
                    t.drawer = None;
                }
                Task::none()
            }
            Message::FocusNext => iced::widget::focus_next(),
            Message::FocusPrevious => iced::widget::focus_previous(),
            Message::HistoryChanged => {
                let now = browser::pathname();
                if now != self.last_path {
                    let route = route::parse(&now);
                    self.last_path = now;
                    if route != self.route {
                        return self.open(route);
                    }
                }
                Task::none()
            }
            Message::ToastOff(id) => {
                if id == self.toast_id {
                    self.toast = None;
                }
                Task::none()
            }
            Message::Go(p) => self.navigate(&p),
            Message::Shell(m) => self.shell_msg(m),
            Message::Sent(r) => {
                if let Some(c) = self.ctx.as_mut() {
                    match r {
                        Ok(a) => {
                            c.say(&a.message);
                            c.refresh_session();
                        }
                        Err(e) => c.failed(&e),
                    }
                }
                Task::none()
            }
            Message::SignedOut => {
                browser::navigate_hard("/sign-in");
                Task::none()
            }
            Message::SignIn(m) => match &mut self.page {
                Page::SignIn(s) => s.update(m).map(Message::SignIn),
                _ => Task::none(),
            },
            Message::Unsubscribe(m) => match &mut self.page {
                Page::Unsubscribe(s) => s.update(m).map(Message::Unsubscribe),
                _ => Task::none(),
            },
            Message::Today(m) => {
                if let (Page::Today(s), Some(c)) = (&mut self.page, self.ctx.as_mut()) {
                    s.update(m, c);
                }
                Task::none()
            }
            Message::Table(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Table(s), Some(c)) => s.update(m, c).map(Message::Table),
                _ => Task::none(),
            },
            Message::Quick(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Quick(s), Some(c)) => s.update(m, c).map(Message::Quick),
                _ => Task::none(),
            },
            Message::Settings(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Settings(s), Some(c)) => s.update(m, c).map(Message::Settings),
                _ => Task::none(),
            },
            Message::Reports(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Reports(s), Some(c)) => s.update(m, c).map(Message::Reports),
                _ => Task::none(),
            },
            Message::Research(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Research(s), Some(c)) => s.update(m, c).map(Message::Research),
                _ => Task::none(),
            },
            Message::Notices(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Notices(s), Some(c)) => s.update(m, c).map(Message::Notices),
                _ => Task::none(),
            },
            Message::Profile(m) => match (&mut self.page, self.ctx.as_mut()) {
                (Page::Profile(s), Some(c)) => s.update(m, c).map(Message::Profile),
                _ => Task::none(),
            },
        }
    }

    fn shell_msg(&mut self, m: ShellMsg) -> Task<Message> {
        match m {
            ShellMsg::Go(p) => return self.navigate(&p),
            ShellMsg::ToggleCollapse => {
                self.shell.collapsed = !self.shell.collapsed;
                if let Some(c) = self.ctx.as_mut() {
                    c.sidebar = Self::sidebar_for(self.width, self.shell.collapsed);
                }
            }
            ShellMsg::ToggleMenu => self.shell.menu_open = !self.shell.menu_open,
            ShellMsg::ToggleUser => self.shell.user_menu = !self.shell.user_menu,
            ShellMsg::AskPause => self.shell.confirm_pause = true,
            ShellMsg::CancelPause => self.shell.confirm_pause = false,
            ShellMsg::ConfirmPause => {
                self.shell.confirm_pause = false;
                let csrf = self.csrf();
                return Task::perform(
                    async move { api::post::<ActionResult>("/api/v1/sending/pause", &csrf).await },
                    Message::Sent,
                );
            }
            ShellMsg::Resume => {
                let csrf = self.csrf();
                return Task::perform(
                    async move { api::post::<ActionResult>("/api/v1/sending/resume", &csrf).await },
                    Message::Sent,
                );
            }
            ShellMsg::Search(s) => self.shell.search = s,
            ShellMsg::SubmitSearch => {
                let s = self.shell.search.trim().to_string();
                if !s.is_empty() {
                    self.pending_search = Some(s);
                    self.shell.search.clear();
                    return self.open(Route::Table("people".into()));
                }
            }
            ShellMsg::SignOut => {
                let csrf = self.csrf();
                return Task::perform(
                    async move { api::post::<ActionResult>("/api/v1/auth/logout", &csrf).await },
                    |_| Message::SignedOut,
                );
            }
            ShellMsg::CloseOverlays => {
                self.shell.user_menu = false;
                self.shell.menu_open = false;
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        if route::is_public(&self.route) {
            let inner: Element<'_, Message> = match &self.page {
                Page::SignIn(s) => s.view().map(Message::SignIn),
                Page::Legal(k) => legal::view(*k),
                Page::Unsubscribe(s) => s.view().map(Message::Unsubscribe),
                _ => body("Loading").into(),
            };
            return shell::public_frame(inner, |p| Message::Go(p.to_string()));
        }
        let Some(ctx) = &self.ctx else {
            let msg: Element<'_, Message> = match &self.page {
                Page::Failed(m) => banner(Tone::Red, m),
                _ => body("Loading").into(),
            };
            return iced::widget::container(msg)
                .padding(theme::sp::S5)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .style(theme::page_bg)
                .into();
        };
        let page: Element<'_, Message> = match &self.page {
            Page::Today(s) => s.view().map(Message::Today),
            Page::Table(s) => s.view(ctx).map(Message::Table),
            Page::Quick(s) => s.view().map(Message::Quick),
            Page::Settings(s) => s.view(ctx).map(Message::Settings),
            Page::Reports(s) => s.view().map(Message::Reports),
            Page::Research(s) => s.view(ctx).map(Message::Research),
            Page::Notices(s) => s.view(ctx).map(Message::Notices),
            Page::Profile(s) => s.view(ctx).map(Message::Profile),
            Page::Help => help::view(),
            Page::Failed(m) => banner(Tone::Red, m),
            Page::NotFound => crate::components::empty_state("This page does not exist."),
            _ => body("Loading").into(),
        };
        let toast = self.toast.as_ref().map(|(m, t)| (m.as_str(), *t));
        let overlay: Option<Element<'_, Message>> = match &self.page {
            Page::Table(s) => s.overlay(ctx).map(|e| e.map(Message::Table)),
            Page::Settings(s) => s.overlay(ctx).map(|e| e.map(Message::Settings)),
            Page::Research(s) => s.overlay(ctx).map(|e| e.map(Message::Research)),
            _ => None,
        };
        shell::app_frame(
            ctx,
            &self.shell,
            route::nav_key(&self.route),
            page,
            toast,
            overlay,
            Message::Shell,
        )
    }
}

pub fn theme(_: &App) -> iced::Theme {
    theme::app_theme()
}

pub fn run() -> iced::Result {
    iced::application(title, App::update, App::view)
        .subscription(subscription)
        .theme(theme)
        .font(theme::FONT_BYTES_REGULAR)
        .font(theme::FONT_BYTES_BOLD)
        .default_font(theme::FONT)
        .scale_factor(|app: &App| app.scale as f64)
        .window_size(Size::new(1280.0, 800.0))
        .run_with(boot)
}
