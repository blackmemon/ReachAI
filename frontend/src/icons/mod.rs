//! Lucide icons (ISC licence), embedded as SVG files and drawn with Iced's svg widget.
//! Only the icons the app uses are included.

use iced::widget::svg;
use iced::{Color, Element, Length};

macro_rules! icons {
    ($($name:expr => $file:expr),* $(,)?) => {
        fn bytes(name: &str) -> &'static [u8] {
            match name {
                $($name => include_bytes!(concat!("../../assets/icons/", $file, ".svg")),)*
                _ => include_bytes!("../../assets/icons/info.svg"),
            }
        }
        pub const NAMES: &[&str] = &[$($name),*];
    };
}

icons! {
    "home" => "house", "check" => "square-check-big", "activity" => "activity", "zap" => "zap",
    "users" => "users", "building" => "building", "list" => "list", "sparkles" => "sparkles",
    "mail" => "mail", "file" => "file-text", "shield" => "shield-check", "phone" => "phone",
    "chat" => "message-square", "calendar" => "calendar", "coins" => "coins", "clock" => "clock",
    "target" => "target", "search" => "search", "send" => "send", "layers" => "layers",
    "filter" => "funnel", "chart" => "chart-column", "gear" => "settings", "help" => "circle-question-mark",
    "bell" => "bell", "user" => "user", "logout" => "log-out", "plus" => "plus", "download" => "download",
    "upload" => "upload", "close" => "x", "down" => "chevron-down", "up" => "chevron-up", "left" => "chevron-left",
    "right" => "chevron-right", "pause" => "pause", "play" => "play", "menu" => "menu", "edit" => "pencil",
    "refresh" => "refresh-cw", "info" => "info", "ok" => "circle-check", "error" => "circle-x",
    "asc" => "arrow-up", "desc" => "arrow-down", "inbox" => "inbox", "book" => "book-open",
}

pub fn icon<'a, M: 'a>(name: &str, size: f32, color: Color) -> Element<'a, M> {
    svg(svg::Handle::from_memory(bytes(name)))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_svg_data() {
        for n in NAMES {
            assert!(bytes(n).starts_with(b"<svg"), "{n}");
        }
    }

    #[test]
    fn every_sidebar_icon_exists() {
        for g in shared::nav::NAV {
            for i in g.items {
                assert!(NAMES.contains(&i.icon), "missing icon {}", i.icon);
            }
        }
    }
}
