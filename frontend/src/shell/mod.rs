//! The frames around pages: the public frame and the signed-in frame (sidebar and top bar).

use crate::app::ctx::Ctx;
use crate::components::*;
use crate::icons::icon;
use crate::theme::{self, c, dim, size, sp, Kind};
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use shared::nav::NAV;

const LOGO: &[u8] = include_bytes!("../../assets/logo.svg");

/// The logo wordmark (black plate, white letters, 4:1), 160 x 40.
pub fn logo<'a, M: 'a>(height: f32) -> Element<'a, M> {
    iced::widget::svg(iced::widget::svg::Handle::from_memory(LOGO))
        .width(Length::Fixed(height * 4.0))
        .height(Length::Fixed(height))
        .into()
}

pub fn brand<'a, M: 'a>() -> Element<'a, M> {
    row![logo(36.0), text("ReachAI").size(size::HEADING).color(c::PRIMARY).font(theme::FONT_BOLD)]
        .spacing(sp::S2)
        .align_y(Alignment::Center)
        .into()
}

/// Public pages: brand on top, content centred, legal links at the bottom.
pub fn public_frame<'a, M: Clone + 'a>(content: Element<'a, M>, go: impl Fn(&str) -> M + 'a) -> Element<'a, M> {
    let top = container(row![brand(), Space::with_width(Length::Fill), button(text("Sign in").size(size::TEXT)).padding([sp::S2, sp::S3]).style(theme::button_style(Kind::Ghost)).on_press(go("/sign-in"))].align_y(Alignment::Center))
        .padding([sp::S3, sp::S5])
        .width(Length::Fill)
        .style(theme::flat(c::CARD));
    let link = |label: &str, path: &str| button(text(label.to_string()).size(size::SMALL)).padding([sp::S1, sp::S2]).style(theme::button_style(Kind::Ghost)).on_press(go(path));
    let foot = container(row![muted("blackmemon ReachAI"), Space::with_width(Length::Fill), link("Privacy", "/privacy"), link("Terms", "/terms"), link("Cookies", "/cookies")].spacing(sp::S2).align_y(Alignment::Center))
        .padding([sp::S3, sp::S5])
        .width(Length::Fill);
    let body = scrollable(container(container(content).max_width(dim::PUBLIC_MAX).width(Length::Fill)).padding(sp::S5).width(Length::Fill).center_x(Length::Fill)).height(Length::Fill);
    container(column![top, body, foot]).width(Length::Fill).height(Length::Fill).style(theme::page_bg).into()
}

pub struct ShellState {
    pub collapsed: bool,
    pub menu_open: bool,
    pub user_menu: bool,
    pub confirm_pause: bool,
    pub search: String,
}

impl Default for ShellState {
    fn default() -> Self {
        Self { collapsed: false, menu_open: false, user_menu: false, confirm_pause: false, search: String::new() }
    }
}

#[derive(Debug, Clone)]
pub enum ShellMsg {
    Go(String),
    ToggleCollapse,
    ToggleMenu,
    ToggleUser,
    AskPause,
    CancelPause,
    ConfirmPause,
    Resume,
    Search(String),
    SubmitSearch,
    SignOut,
    CloseOverlays,
}

fn sidebar<'a>(active: &str, collapsed: bool, phone: bool) -> Element<'a, ShellMsg> {
    let mut col = column![].spacing(sp::S1).padding([sp::S3, sp::S2]);
    for g in NAV {
        if !collapsed {
            col = col.push(container(label(g.label)).padding([sp::S2, sp::S3]));
        }
        for i in g.items {
            let on = i.key == active;
            let fg = if on { c::PRIMARY } else { c::INK };
            let mut r = row![icon(i.icon, size::ICON, fg)].spacing(sp::S3).align_y(Alignment::Center);
            if !collapsed {
                r = r.push(text(i.label).size(size::TEXT).color(fg).font(if on { theme::FONT_BOLD } else { theme::FONT }));
            }
            let b = button(r)
                .width(Length::Fill)
                .padding([sp::S2, sp::S3])
                .height(Length::Fixed(if phone { dim::CONTROL } else { dim::MENU_ITEM + 6.0 }))
                .style(theme::button_style(if on { Kind::MenuActive } else { Kind::Menu }))
                .on_press(ShellMsg::Go(format!("/{}", if i.key == "today" { "" } else { i.key })));
            col = col.push(b);
        }
    }
    let w = if collapsed { dim::SIDEBAR_COLLAPSED } else { dim::SIDEBAR };
    container(scrollable(col).direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(4).scroller_width(4))).height(Length::Fill)).width(Length::Fixed(w)).height(Length::Fill).style(|t: &iced::Theme| {
        let mut s = theme::flat(c::CARD)(t);
        s.border = iced::Border { color: c::BORDER, width: 1.0, radius: 0.0.into() };
        s
    }).into()
}

fn topbar<'a>(ctx: &'a Ctx, st: &'a ShellState) -> Element<'a, ShellMsg> {
    let phone = ctx.phone();
    let mut r = row![].spacing(sp::S3).align_y(Alignment::Center);
    r = r.push(icon_button("menu", if phone { ShellMsg::ToggleMenu } else { ShellMsg::ToggleCollapse }));
    if !phone {
        r = r.push(brand());
    }
    r = r.push(Space::with_width(Length::FillPortion(6)));
    if ctx.session.sending_paused {
        r = r.push(pill("Sending paused", Tone::Amber));
        if ctx.can("sending.pause") {
            r = r.push(button_with("Resume", Some("play"), Kind::Secondary, Some(ShellMsg::Resume)));
        }
    } else {
        r = r.push(pill("Sending on", Tone::Green));
        if ctx.can("sending.pause") {
            r = r.push(button_with(if phone { "" } else { "Pause all" }, Some("pause"), Kind::Secondary, Some(ShellMsg::AskPause)));
        }
    }
    if !phone {
        if ctx.width >= dim::WIDE_TOPBAR {
            r = r.push(container(input("Search people", &st.search, ShellMsg::Search, false).on_submit(ShellMsg::SubmitSearch)).width(Length::FillPortion(2)));
        }
        r = r.push(button_with("Ask Quick", Some("zap"), Kind::Secondary, Some(ShellMsg::Go("/quick".into()))));
    }
    let bell = {
        let mut b = row![icon("bell", size::ICON, c::INK)].spacing(sp::S1).align_y(Alignment::Center);
        if ctx.session.unread_notices > 0 {
            b = b.push(pill(&ctx.session.unread_notices.to_string(), Tone::Blue));
        }
        button(b).padding([sp::S2, sp::S3]).height(Length::Fixed(dim::CONTROL)).style(theme::button_style(Kind::Menu)).on_press(ShellMsg::Go("/notices".into()))
    };
    r = r.push(bell);
    let initials: String = ctx.session.name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
    r = r.push(
        button(row![container(text(initials).size(size::SMALL).color(c::WHITE)).width(Length::Fixed(28.0)).height(Length::Fixed(28.0)).center_x(Length::Fixed(28.0)).center_y(Length::Fixed(28.0)).style(theme::pill(c::WHITE, c::PRIMARY)), icon("down", size::ICON_SMALL, c::MUTED)].spacing(sp::S1).align_y(Alignment::Center))
            .padding([sp::S1, sp::S2])
            .height(Length::Fixed(dim::CONTROL))
            .style(theme::button_style(Kind::Menu))
            .on_press(ShellMsg::ToggleUser),
    );
    container(r).padding([0.0, sp::S4]).height(Length::Fixed(dim::TOPBAR)).width(Length::Fill).center_y(Length::Fixed(dim::TOPBAR)).style(|t: &iced::Theme| {
        let mut s = theme::flat(c::CARD)(t);
        s.border = iced::Border { color: c::BORDER, width: 1.0, radius: 0.0.into() };
        s
    }).into()
}

fn user_menu<'a>(ctx: &'a Ctx) -> Element<'a, ShellMsg> {
    let item = |label: &str, ic: &str, m: ShellMsg| button(row![icon(ic, size::ICON, c::INK), text(label.to_string()).size(size::TEXT)].spacing(sp::S2).align_y(Alignment::Center)).width(Length::Fill).padding([sp::S2, sp::S3]).style(theme::button_style(Kind::Menu)).on_press(m);
    container(
        column![
            container(column![strong(ctx.session.name.clone()), muted(format!("{} · {}", ctx.session.email, ctx.session.role))].spacing(0)).padding([sp::S2, sp::S3]),
            divider(),
            item("My profile", "user", ShellMsg::Go("/profile".into())),
            item("Notices", "bell", ShellMsg::Go("/notices".into())),
            item("Sign out", "logout", ShellMsg::SignOut),
        ]
        .spacing(sp::S1),
    )
    .padding(sp::S2)
    .width(Length::Fill)
    .max_width(260.0)
    .style(theme::menu)
    .into()
}

fn confirm_pause<'a>() -> Element<'a, ShellMsg> {
    let modal = container(
        column![
            card_title("Pause all sending?"),
            muted("This stops every mailbox at once. Emails already sent are not recalled."),
            row![Space::with_width(Length::Fill), secondary("Cancel", ShellMsg::CancelPause), button_with("Pause all", Some("pause"), Kind::Primary, Some(ShellMsg::ConfirmPause))].spacing(sp::S2),
        ]
        .spacing(sp::S3),
    )
    .padding(sp::S5)
    .width(Length::Fill)
    .max_width(dim::MODAL)
    .style(theme::card);
    container(modal).padding(sp::S3).center_x(Length::Fill).center_y(Length::Fill).width(Length::Fill).height(Length::Fill).style(theme::scrim).into()
}

/// The signed-in frame. `page` is the current page, already mapped into `ShellMsg`-free messages by the caller.
pub fn app_frame<'a, M: Clone + 'a>(
    ctx: &'a Ctx,
    st: &'a ShellState,
    active: &str,
    page: Element<'a, M>,
    toast: Option<(&'a str, Tone)>,
    overlay: Option<Element<'a, M>>,
    wrap: impl Fn(ShellMsg) -> M + Copy + 'a,
) -> Element<'a, M> {
    let phone = ctx.phone();
    let side: Option<Element<'a, M>> = if phone { None } else { Some(sidebar(active, st.collapsed, false).map(wrap)) };
    let main = container(scrollable(container(container(page).max_width(dim::PAGE_MAX).width(Length::Fill)).padding(if phone { sp::S3 } else { sp::S5 }).width(Length::Fill)).direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(6).scroller_width(6))).height(Length::Fill)).width(Length::Fill).height(Length::Fill);
    let body: Element<'a, M> = match side {
        Some(s) => row![s, main].into(),
        None => main.into(),
    };
    let base: Element<'a, M> = container(column![topbar(ctx, st).map(wrap), body]).width(Length::Fill).height(Length::Fill).style(theme::page_bg).into();

    let mut layers: Vec<Element<'a, M>> = vec![base];
    if phone && st.menu_open {
        layers.push(container(iced::widget::opaque(sidebar(active, false, true).map(wrap))).height(Length::Fill).padding(iced::Padding { top: dim::TOPBAR, ..Default::default() }).into());
    }
    if let Some(o) = overlay {
        layers.push(o);
    }
    if st.user_menu {
        let dismiss = iced::widget::mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(wrap(ShellMsg::CloseOverlays));
        layers.push(dismiss.into());
        layers.push(container(user_menu(ctx).map(wrap)).width(Length::Fill).align_x(iced::alignment::Horizontal::Right).padding(iced::Padding { top: dim::TOPBAR, right: sp::S4, ..Default::default() }).into());
    }
    if let Some((msg, tone)) = toast {
        layers.push(container(container(banner(tone, msg)).width(Length::Fill).max_width(dim::MODAL)).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).align_y(iced::alignment::Vertical::Bottom).padding(sp::S5).into());
    }
    if st.confirm_pause {
        layers.push(confirm_pause().map(wrap));
    }
    iced::widget::stack(layers).width(Length::Fill).height(Length::Fill).into()
}
