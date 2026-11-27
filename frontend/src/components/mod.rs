//! Shared building blocks: text styles, buttons, cards, pills, form fields, banners.

use crate::icons::icon;
use crate::theme::{self, c, dim, radius, size, sp, Kind};
use iced::widget::{button, column, container, row, text, text_input, Column, Container, Row, Text};
use iced::{Alignment, Element, Length};

pub fn body<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::TEXT).color(c::INK)
}
pub fn muted<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::SMALL).color(c::MUTED)
}
pub fn strong<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::TEXT).color(c::INK).font(theme::FONT_BOLD)
}
pub fn heading<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::HEADING).color(c::INK).font(theme::FONT_BOLD)
}
pub fn card_title<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::CARD_TITLE).color(c::INK)
}
pub fn page_title<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into()).size(size::PAGE_TITLE).color(c::INK)
}
pub fn label<'a>(s: impl Into<String>) -> Text<'a> {
    text(s.into().to_uppercase()).size(size::LABEL).color(c::MUTED)
}

pub fn button_with<'a, M: Clone + 'a>(label: &str, icon_name: Option<&str>, kind: Kind, msg: Option<M>) -> button::Button<'a, M> {
    let fg = match kind {
        Kind::Primary | Kind::Accent => c::WHITE,
        Kind::Danger => c::RED,
        Kind::Ghost | Kind::MenuActive => c::PRIMARY,
        _ => c::INK,
    };
    let mut r: Row<'a, M> = row![].spacing(sp::S2).align_y(Alignment::Center);
    if let Some(i) = icon_name {
        r = r.push(icon(i, size::ICON_SMALL + 2.0, if msg.is_some() { fg } else { c::MUTED }));
    }
    if !label.is_empty() {
        r = r.push(text(label.to_string()).size(size::TEXT));
    }
    let b = button(container(r).height(Length::Fill).center_y(Length::Fill))
        .padding([sp::S2, sp::S4])
        .height(Length::Fixed(dim::CONTROL))
        .style(theme::button_style(kind));
    match msg {
        Some(m) => b.on_press(m),
        None => b,
    }
}

/// A button that fills the width of its column, label in the middle.
pub fn button_wide<'a, M: Clone + 'a>(label: &str, kind: Kind, msg: Option<M>) -> button::Button<'a, M> {
    let fg = if matches!(kind, Kind::Primary | Kind::Accent) { c::WHITE } else { c::INK };
    let _ = fg;
    let b = button(container(text(label.to_string()).size(size::TEXT)).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill))
        .padding([sp::S2, sp::S4])
        .width(Length::Fill)
        .height(Length::Fixed(dim::CONTROL))
        .style(theme::button_style(kind));
    match msg {
        Some(m) => b.on_press(m),
        None => b,
    }
}

pub fn primary<'a, M: Clone + 'a>(label: &str, msg: M) -> Element<'a, M> {
    button_with(label, None, Kind::Primary, Some(msg)).into()
}
pub fn secondary<'a, M: Clone + 'a>(label: &str, msg: M) -> Element<'a, M> {
    button_with(label, None, Kind::Secondary, Some(msg)).into()
}

pub fn icon_button<'a, M: Clone + 'a>(name: &str, msg: M) -> Element<'a, M> {
    button(container(icon(name, size::ICON, c::INK)).center_x(Length::Fixed(dim::CONTROL - 8.0)).center_y(Length::Fixed(dim::CONTROL - 8.0)))
        .padding(0)
        .style(theme::button_style(Kind::Menu))
        .on_press(msg)
        .into()
}

pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content).padding(sp::S4).width(Length::Fill).style(theme::card)
}

pub fn card_titled<'a, M: 'a>(title: &str, subtitle: &str, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let mut head: Column<'a, M> = column![card_title(title.to_string())].spacing(0);
    if !subtitle.is_empty() {
        head = head.push(muted(subtitle.to_string()));
    }
    card(column![head, content.into()].spacing(sp::S3)).into()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Green,
    Amber,
    Red,
    Blue,
    Violet,
    Cyan,
    Slate,
}

impl Tone {
    pub fn colors(self) -> (iced::Color, iced::Color) {
        match self {
            Tone::Green => (c::GREEN, c::GREEN_SOFT),
            Tone::Amber => (c::AMBER, c::AMBER_SOFT),
            Tone::Red => (c::RED, c::RED_SOFT),
            Tone::Blue => (c::PRIMARY, c::BLUE_SOFT),
            Tone::Violet => (c::VIOLET, c::VIOLET_SOFT),
            Tone::Cyan => (c::INK, c::CYAN_SOFT),
            Tone::Slate => (c::INK, c::SLATE_SOFT),
        }
    }
}

/// Words that mean good, waiting or bad, for status pills in tables.
pub fn tone_of(value: &str) -> Tone {
    match value.to_lowercase().as_str() {
        "sent" | "verified" | "active" | "done" | "won" | "running" | "saved" | "interested" | "ready_to_meet" | "on time" => Tone::Green,
        "queued" | "waiting" | "waiting_approval" | "paused" | "needs_review" | "open" | "draft" | "risky" | "handover" | "unknown" | "to_write" | "contacted" => Tone::Amber,
        "blocked" | "failed" | "invalid" | "dead" | "error" | "rejected" | "lost" | "late" | "stopped" | "closed_out" => Tone::Red,
        "ai" | "claude" | "employee" => Tone::Violet,
        "replied" | "new" | "received" | "proposal" | "negotiation" => Tone::Blue,
        _ => Tone::Slate,
    }
}

pub fn pill<'a, M: 'a>(label: &str, tone: Tone) -> Element<'a, M> {
    let (fg, bg) = tone.colors();
    container(text(label.to_string()).size(size::SMALL).color(fg)).padding([2.0, sp::S3]).style(theme::pill(fg, bg)).into()
}

pub fn stat_tile<'a, M: 'a>(label: &str, value: &str) -> Element<'a, M> {
    container(column![muted(label.to_string()), text(value.to_string()).size(size::BIG_NUMBER).color(c::INK)].spacing(sp::S1))
        .padding(sp::S4)
        .width(Length::Fill)
        .style(theme::panel(c::CYAN_SOFT))
        .into()
}

pub fn banner<'a, M: 'a>(tone: Tone, message: &str) -> Element<'a, M> {
    let (fg, bg) = tone.colors();
    let name = match tone {
        Tone::Green => "ok",
        Tone::Red => "error",
        _ => "info",
    };
    container(row![icon(name, size::ICON, fg), text(message.to_string()).size(size::TEXT).color(fg)].spacing(sp::S2).align_y(Alignment::Center))
        .padding(sp::S3)
        .width(Length::Fill)
        .style(theme::panel(bg))
        .into()
}

pub fn field<'a, M: 'a>(label_text: &str, input: impl Into<Element<'a, M>>, error: Option<&str>, help: &str) -> Element<'a, M> {
    let mut col: Column<'a, M> = column![text(label_text.to_string()).size(size::SMALL).color(c::INK).font(theme::FONT_BOLD), input.into()].spacing(sp::S1);
    if let Some(e) = error {
        col = col.push(row![icon("error", size::ICON_SMALL, c::RED), text(e.to_string()).size(size::SMALL).color(c::RED)].spacing(sp::S1).align_y(Alignment::Center));
    } else if !help.is_empty() {
        col = col.push(muted(help.to_string()));
    }
    col.into()
}

pub fn input<'a, M: Clone + 'a>(placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a, error: bool) -> text_input::TextInput<'a, M> {
    text_input(placeholder, value).on_input(on_input).padding(sp::S3).size(size::TEXT).font(theme::FONT).style(theme::input_style(error))
}

pub fn text_field<'a, M: Clone + 'a>(label_text: &str, placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a, error: Option<&str>, help: &str) -> Element<'a, M> {
    field(label_text, input(placeholder, value, on_input, error.is_some()), error, help)
}

pub fn page_header<'a, M: 'a>(title: &str, subtitle: &str, actions: Vec<Element<'a, M>>) -> Element<'a, M> {
    let titles = column![page_title(title.to_string()), muted(subtitle.to_string())].spacing(sp::S1).width(Length::Fill);
    if actions.is_empty() {
        return titles.into();
    }
    if theme::viewport_width() < theme::dim::WIDE_HEADER {
        // Narrow window: the title first, then the buttons, wrapping onto more lines.
        let acts = Row::with_children(actions).spacing(sp::S2).wrap();
        return column![titles, acts].spacing(sp::S3).into();
    }
    let mut r: Row<'a, M> = row![titles].spacing(sp::S2).align_y(Alignment::Center);
    for a in actions {
        r = r.push(a);
    }
    r.into()
}

/// Tiles in rows of `columns` (two on a phone). The last row is padded so tiles keep one width.
pub fn tile_grid<'a, M: 'a>(tiles: Vec<Element<'a, M>>, columns: usize) -> Element<'a, M> {
    let cols = if theme::is_phone() { 2 } else { columns.max(1) };
    let mut rows = column![].spacing(sp::S3);
    let mut it = tiles.into_iter().peekable();
    while it.peek().is_some() {
        let mut r = row![].spacing(sp::S3);
        let mut n = 0;
        for t in it.by_ref().take(cols) {
            r = r.push(t);
            n += 1;
        }
        while n < cols {
            r = r.push(iced::widget::Space::with_width(Length::Fill));
            n += 1;
        }
        rows = rows.push(r);
    }
    rows.into()
}

pub fn empty_state<'a, M: 'a>(message: &str) -> Element<'a, M> {
    container(column![icon("inbox", 28.0, c::MUTED), muted(message.to_string())].spacing(sp::S2).align_x(Alignment::Center))
        .padding(sp::S6)
        .width(Length::Fill)
        .center_x(Length::Fill)
        .into()
}

/// A thin line of a fixed width. Use inside a horizontal scroll area, where "fill" has no meaning.
pub fn divider_fixed<'a, M: 'a>(width: f32) -> Element<'a, M> {
    container(iced::widget::Space::new(Length::Fixed(width), Length::Fixed(1.0))).style(theme::line_bottom).into()
}

pub fn divider<'a, M: 'a>() -> Element<'a, M> {
    container(iced::widget::Space::new(Length::Fill, Length::Fixed(1.0))).style(theme::line_bottom).into()
}

pub fn pill_radius_is_round() -> bool {
    radius::PILL > 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_words_get_sensible_tones() {
        assert_eq!(tone_of("sent"), Tone::Green);
        assert_eq!(tone_of("blocked"), Tone::Red);
        assert_eq!(tone_of("waiting_approval"), Tone::Amber);
        assert_eq!(tone_of("something else"), Tone::Slate);
    }
}
