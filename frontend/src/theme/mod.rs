//! Design tokens from the attached design (tokens.css): colours, type, spacing, sizes.
//! Screens never write their own numbers; they ask this module.

use iced::widget::{button, checkbox, container, pick_list, text_input};
use iced::{Background, Border, Color, Font, Shadow, Vector};

const fn rgb(hex: u32) -> Color {
    Color { r: ((hex >> 16) & 0xff) as f32 / 255.0, g: ((hex >> 8) & 0xff) as f32 / 255.0, b: (hex & 0xff) as f32 / 255.0, a: 1.0 }
}

pub mod c {
    use super::*;
    pub const PRIMARY: Color = rgb(0x36454F);
    pub const PRIMARY_DARK: Color = rgb(0x2A363E);
    pub const INK: Color = rgb(0x1F2A31);
    pub const MUTED: Color = rgb(0x56636B);
    pub const PAGE: Color = rgb(0xF5F7F8);
    pub const CARD: Color = rgb(0xFFFFFF);
    pub const SOFT: Color = rgb(0xE9EDF0);
    pub const BORDER: Color = rgb(0xD5DCE0);
    pub const ACCENT: Color = rgb(0x3D8FA8);
    pub const VIOLET: Color = rgb(0x5A4A8A);
    pub const GREEN: Color = rgb(0x147A3A);
    pub const GREEN_SOFT: Color = rgb(0xE6F4EA);
    pub const AMBER: Color = rgb(0x9A4A0A);
    pub const AMBER_SOFT: Color = rgb(0xFDF0DD);
    pub const RED: Color = rgb(0xB42318);
    pub const RED_SOFT: Color = rgb(0xFDEBE9);
    pub const BLUE_SOFT: Color = rgb(0xE6EEF4);
    pub const VIOLET_SOFT: Color = rgb(0xEFEBF6);
    pub const CYAN_SOFT: Color = rgb(0xDFF0F5);
    pub const SLATE_SOFT: Color = rgb(0xEBEEF0);
    pub const WHITE: Color = rgb(0xFFFFFF);
    pub const SCRIM: Color = Color { r: 0.122, g: 0.165, b: 0.192, a: 0.35 };
}

/// Pragati Narrow. Both weights are embedded in the build; nothing is loaded from outside.
pub const FONT_BYTES_REGULAR: &[u8] = include_bytes!("../../assets/fonts/PragatiNarrow-Regular.ttf");
pub const FONT_BYTES_BOLD: &[u8] = include_bytes!("../../assets/fonts/PragatiNarrow-Bold.ttf");
pub const FONT: Font = Font::with_name("Pragati Narrow");
pub const FONT_BOLD: Font = Font { weight: iced::font::Weight::Bold, ..Font::with_name("Pragati Narrow") };

pub mod size {
    pub const TEXT: f32 = 15.0;
    pub const SMALL: f32 = 13.0;
    pub const LABEL: f32 = 12.0;
    pub const HEADING: f32 = 16.0;
    pub const CARD_TITLE: f32 = 18.0;
    pub const PAGE_TITLE: f32 = 26.0;
    pub const BIG_NUMBER: f32 = 28.0;
    pub const ICON: f32 = 18.0;
    pub const ICON_SMALL: f32 = 14.0;
}

pub mod sp {
    pub const S1: f32 = 4.0;
    pub const S2: f32 = 8.0;
    pub const S3: f32 = 12.0;
    pub const S4: f32 = 16.0;
    pub const S5: f32 = 24.0;
    pub const S6: f32 = 32.0;
    pub const S7: f32 = 40.0;
}

pub mod dim {
    pub const SIDEBAR: f32 = 240.0;
    pub const SIDEBAR_COLLAPSED: f32 = 72.0;
    pub const TOPBAR: f32 = 56.0;
    pub const DRAWER: f32 = 560.0;
    pub const MODAL: f32 = 480.0;
    pub const CONTROL: f32 = 40.0;
    pub const ROW: f32 = 48.0;
    pub const MENU_ITEM: f32 = 32.0;
    pub const FORM_MAX: f32 = 960.0;
    pub const PAGE_MAX: f32 = 1848.0;
    pub const PUBLIC_MAX: f32 = 1200.0;
    /// Below this width the sidebar becomes a menu button (phone layout).
    pub const PHONE: f32 = 760.0;
    /// Below this page width (window minus sidebar), two panes side by side no longer fit.
    pub const SPLIT: f32 = 760.0;
    /// Below this window width the page title stacks above its buttons.
    pub const WIDE_HEADER: f32 = 1100.0;
    /// Below this window width the top bar drops its search box.
    pub const WIDE_TOPBAR: f32 = 980.0;
    pub const FOCUS: f32 = 3.0;
}

/// Column widths in tables, by kind of value.
pub fn column_width(kind: shared::FieldKind) -> f32 {
    use shared::FieldKind::*;
    match kind {
        Text => 130.0,
        LongText | MultiLine | EmailList | UrlList => 220.0,
        Email | Url => 200.0,
        Number | Integer | Bool => 80.0,
        Date => 120.0,
        DateTime => 140.0,
        Choice => 110.0,
    }
}

/// Whether the window is phone-sized. The root sets it on every resize so that small
/// building blocks (page headers, tile grids) can choose a layout without carrying `Ctx`.
static PHONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static WIDTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1280.0f32.to_bits());

pub fn set_width(width: f32) {
    PHONE.store(width < dim::PHONE, std::sync::atomic::Ordering::Relaxed);
    WIDTH.store(width.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The window width the layout sees (after the app scale).
pub fn viewport_width() -> f32 {
    f32::from_bits(WIDTH.load(std::sync::atomic::Ordering::Relaxed))
}

/// The app's root scale, like a root font size that follows the screen: 1 on ordinary laptop
/// windows, a little smaller on very narrow phones, and growing on large monitors so text and
/// controls stay comfortable instead of looking small and lost. `css_width` is the browser window.
pub fn root_scale(css_width: f32) -> f32 {
    if css_width < 600.0 {
        (css_width / 400.0).clamp(0.82, 1.0)
    } else if css_width < 1280.0 {
        1.0
    } else {
        (css_width / 1280.0).clamp(1.0, 1.3)
    }
}

/// A share of the window height, kept within sensible limits. For panels that need a height of
/// their own (a chat, a split inbox) so they follow the window instead of a fixed pixel size.
pub fn height_share(share: f32, min: f32, max: f32) -> f32 {
    (viewport_height() * share).clamp(min, max)
}

static HEIGHT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(800.0f32.to_bits());

pub fn set_height(height: f32) {
    HEIGHT.store(height.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The window height, so panels can take a share of it instead of a fixed number of pixels.
pub fn viewport_height() -> f32 {
    f32::from_bits(HEIGHT.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn is_phone() -> bool {
    PHONE.load(std::sync::atomic::Ordering::Relaxed)
}

pub mod radius {
    pub const FIELD: f32 = 8.0;
    pub const CARD: f32 = 16.0;
    pub const PILL: f32 = 999.0;
}

fn card_shadow() -> Shadow {
    Shadow { color: Color { a: 0.08, ..c::INK }, offset: Vector::new(0.0, 4.0), blur_radius: 16.0 }
}

pub fn page_bg(_: &iced::Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c::PAGE)), text_color: Some(c::INK), ..Default::default() }
}

pub fn card(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c::CARD)),
        border: Border { color: c::BORDER, width: 1.0, radius: radius::CARD.into() },
        shadow: card_shadow(),
        text_color: Some(c::INK),
        ..Default::default()
    }
}

pub fn flat(bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style { background: Some(Background::Color(bg)), text_color: Some(c::INK), ..Default::default() }
}

pub fn line_bottom(_: &iced::Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c::BORDER)), ..Default::default() }
}

pub fn drawer(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c::CARD)),
        border: Border { color: c::BORDER, width: 1.0, radius: 0.0.into() },
        shadow: Shadow { color: Color { a: 0.12, ..c::INK }, offset: Vector::new(-4.0, 0.0), blur_radius: 16.0 },
        text_color: Some(c::INK),
        ..Default::default()
    }
}

pub fn menu(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c::CARD)),
        border: Border { color: c::BORDER, width: 1.0, radius: radius::FIELD.into() },
        shadow: Shadow { color: Color { a: 0.12, ..c::INK }, offset: Vector::new(0.0, 4.0), blur_radius: 12.0 },
        text_color: Some(c::INK),
        ..Default::default()
    }
}

pub fn scrim(_: &iced::Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c::SCRIM)), ..Default::default() }
}

/// A pill: text on a soft fill. Meaning always comes with words (and an icon where it matters), never colour alone.
pub fn pill(fg: Color, bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: radius::PILL.into() },
        text_color: Some(fg),
        ..Default::default()
    }
}

pub fn panel(bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border { color: c::BORDER, width: 1.0, radius: radius::FIELD.into() },
        text_color: Some(c::INK),
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Accent,
    Secondary,
    Danger,
    Ghost,
    Menu,
    MenuActive,
}

fn focus_border(status: button::Status, base: Border) -> Border {
    if matches!(status, button::Status::Hovered) {
        base
    } else {
        base
    }
}

pub fn button_style(kind: Kind) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        let (bg, fg, border) = match kind {
            Kind::Primary => (if hovered { c::PRIMARY_DARK } else { c::PRIMARY }, c::WHITE, Color::TRANSPARENT),
            Kind::Accent => (if hovered { Color { r: 0.06, g: 0.41, b: 0.2, a: 1.0 } } else { c::GREEN }, c::WHITE, Color::TRANSPARENT),
            Kind::Secondary => (if hovered { c::SOFT } else { c::CARD }, c::INK, c::BORDER),
            Kind::Danger => (if hovered { c::RED_SOFT } else { c::CARD }, c::RED, c::RED),
            Kind::Ghost => (if hovered { c::SOFT } else { Color::TRANSPARENT }, c::PRIMARY, Color::TRANSPARENT),
            Kind::Menu => (if hovered { c::SOFT } else { Color::TRANSPARENT }, c::INK, Color::TRANSPARENT),
            Kind::MenuActive => (c::SOFT, c::PRIMARY, Color::TRANSPARENT),
        };
        let (bg, fg) = if disabled { (c::SLATE_SOFT, c::MUTED) } else { (bg, fg) };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: focus_border(status, Border { color: border, width: if border == Color::TRANSPARENT { 0.0 } else { 1.0 }, radius: radius::FIELD.into() }),
            shadow: Shadow::default(),
        }
    }
}

pub fn input_style(error: bool) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let focused = matches!(status, text_input::Status::Focused);
        let border = if error { c::RED } else if focused { c::PRIMARY } else { c::BORDER };
        text_input::Style {
            background: Background::Color(if matches!(status, text_input::Status::Disabled) { c::SLATE_SOFT } else { c::CARD }),
            border: Border { color: border, width: if focused { dim::FOCUS.min(2.0) } else { 1.0 }, radius: radius::FIELD.into() },
            icon: c::MUTED,
            placeholder: c::MUTED,
            value: c::INK,
            selection: Color { a: 0.25, ..c::PRIMARY },
        }
    }
}

pub fn pick_style(_: &iced::Theme, status: pick_list::Status) -> pick_list::Style {
    let open = matches!(status, pick_list::Status::Opened);
    pick_list::Style {
        text_color: c::INK,
        placeholder_color: c::MUTED,
        handle_color: c::MUTED,
        background: Background::Color(c::CARD),
        border: Border { color: if open { c::PRIMARY } else { c::BORDER }, width: 1.0, radius: radius::FIELD.into() },
    }
}

pub fn menu_style(_: &iced::Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
        background: Background::Color(c::CARD),
        border: Border { color: c::BORDER, width: 1.0, radius: radius::FIELD.into() },
        text_color: c::INK,
        selected_text_color: c::WHITE,
        selected_background: Background::Color(c::PRIMARY),
    }
}

pub fn check_style(_: &iced::Theme, status: checkbox::Status) -> checkbox::Style {
    let checked = matches!(status, checkbox::Status::Active { is_checked: true } | checkbox::Status::Hovered { is_checked: true });
    checkbox::Style {
        background: Background::Color(if checked { c::PRIMARY } else { c::CARD }),
        icon_color: c::WHITE,
        border: Border { color: if checked { c::PRIMARY } else { c::MUTED }, width: 1.5, radius: 4.0.into() },
        text_color: Some(c::INK),
    }
}

pub fn app_theme() -> iced::Theme {
    iced::Theme::custom(
        "blackmemon".to_string(),
        iced::theme::Palette { background: c::PAGE, text: c::INK, primary: c::PRIMARY, success: c::GREEN, danger: c::RED },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG contrast ratio of two colours.
    fn lum(c: Color) -> f32 {
        let f = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }
    fn contrast(a: Color, b: Color) -> f32 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_colours_meet_wcag_aa_on_their_backgrounds() {
        for (fg, bg, name) in [
            (c::INK, c::PAGE, "ink on page"),
            (c::INK, c::CARD, "ink on card"),
            (c::MUTED, c::CARD, "muted on card"),
            (c::MUTED, c::PAGE, "muted on page"),
            (c::WHITE, c::PRIMARY, "white on primary"),
            (c::PRIMARY, c::SOFT, "primary on soft"),
            (c::GREEN, c::GREEN_SOFT, "green on green soft"),
            (c::AMBER, c::AMBER_SOFT, "amber on amber soft"),
            (c::RED, c::RED_SOFT, "red on red soft"),
            (c::VIOLET, c::VIOLET_SOFT, "violet on violet soft"),
        ] {
            assert!(contrast(fg, bg) >= 4.5, "{name}: {:.2}", contrast(fg, bg));
        }
    }

    #[test]
    fn touch_targets_are_at_least_40() {
        assert!(dim::CONTROL >= 40.0 && dim::ROW >= 40.0);
    }
}
