use crate::api;
use crate::components::*;
use crate::theme::{self, c, sp, Kind};
use crate::icons::icon;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Row, Space};
use iced::{Alignment, Element, Length, Task};
use shared::{ApiError, QuickAsk, QuickMessage, ReportSet};

pub struct State {
    pub tab: String,
    pub messages: Vec<QuickMessage>,
    pub usage: Option<ReportSet>,
    pub input: String,
    pub busy: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(String),
    History(Result<Vec<QuickMessage>, ApiError>),
    Usage(Result<ReportSet, ApiError>),
    Input(String),
    Prompt(String),
    Send,
    Answered(Result<serde_json::Value, ApiError>),
}

pub const TABS: &[(&str, &str)] = &[("chat", "AI Chat"), ("usage", "AI Usage"), ("history", "History")];

pub fn init(tab: Option<String>) -> (State, Task<Msg>) {
    let tab = tab.filter(|t| TABS.iter().any(|(key, _)| *key == t)).unwrap_or_else(|| "chat".into());
    let state = State { tab: tab.clone(), messages: vec![], usage: None, input: String::new(), busy: false };
    let task = if tab == "usage" {
        Task::perform(api::get::<ReportSet>("/api/v1/reports/AI%20usage"), Msg::Usage)
    } else {
        Task::perform(api::get::<Vec<QuickMessage>>("/api/v1/quick"), Msg::History)
    };
    (state, task)
}

impl State {
    pub fn update(&mut self, msg: Msg, ctx: &mut crate::app::ctx::Ctx) -> Task<Msg> {
        match msg {
            Msg::Tab(tab) if TABS.iter().any(|(key, _)| *key == tab) => {
                self.tab = tab.clone();
                ctx.go(&format!("/quick/{tab}"));
                return if tab == "usage" {
                    Task::perform(api::get::<ReportSet>("/api/v1/reports/AI%20usage"), Msg::Usage)
                } else {
                    Task::perform(api::get::<Vec<QuickMessage>>("/api/v1/quick"), Msg::History)
                };
            }
            Msg::Tab(_) => {}
            Msg::History(Ok(h)) => self.messages = h,
            Msg::History(Err(e)) => ctx.failed(&e),
            Msg::Usage(Ok(report)) => self.usage = Some(report),
            Msg::Usage(Err(e)) => ctx.failed(&e),
            Msg::Input(v) => self.input = v,
            Msg::Prompt(v) => {
                self.input = v;
                return self.update(Msg::Send, ctx);
            }
            Msg::Send => {
                let t = self.input.trim().to_string();
                if t.is_empty() || self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.messages.push(QuickMessage { role: "user".into(), body: t.clone(), at: String::new() });
                self.input.clear();
                let csrf = ctx.csrf();
                return Task::perform(async move { api::send::<serde_json::Value, _>("POST", "/api/v1/quick", &csrf, &QuickAsk { text: t }).await }, Msg::Answered);
            }
            Msg::Answered(r) => {
                self.busy = false;
                match r {
                    Ok(v) => self.messages.push(QuickMessage { role: "quick".into(), body: v["answer"].as_str().unwrap_or("").to_string(), at: String::new() }),
                    Err(e) => ctx.failed(&e),
                }
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut tabs = row![].spacing(sp::S1);
        for (key, title) in TABS {
            tabs = tabs.push(button_with(title, None, if *key == self.tab { Kind::MenuActive } else { Kind::Menu }, Some(Msg::Tab((*key).into()))));
        }
        let header = column![page_header("Quick", "Ask about your records, review AI usage, or return to saved history.", vec![]), tabs].spacing(sp::S3);
        match self.tab.as_str() {
            "usage" => return column![header, self.usage_view()].spacing(sp::S4).into(),
            "history" => return column![header, self.history_view()].spacing(sp::S4).into(),
            _ => {}
        }

        let mut chat = column![].spacing(sp::S3).padding(sp::S4);
        if self.messages.is_empty() {
            let prompts = [
                "Summarize recent outreach",
                "Who needs a follow-up?",
                "Which replies need attention?",
                "Review this week's activity",
            ]
            .into_iter()
            .map(|prompt| {
                button(container(text(prompt).size(theme::size::SMALL).color(c::INK)).width(Length::Fill))
                    .width(Length::Fill)
                    .padding([sp::S2, sp::S3])
                    .style(theme::button_style(Kind::Secondary))
                    .on_press(Msg::Prompt(prompt.to_string()))
                    .into()
            });
            let prompts: Vec<Element<'_, Msg>> = prompts.collect();
            let welcome = column![
                container(icon("sparkles", 24.0, c::GREEN)).padding(sp::S2).style(theme::pill(c::GREEN, c::GREEN_SOFT)),
                heading("What would you like to know?"),
                muted("Quick answers using the information in this dashboard."),
                iced::widget::Row::with_children(prompts).spacing(sp::S2).wrap(),
            ]
            .spacing(sp::S3)
            .align_x(Alignment::Center)
            .max_width(560.0);
            // Not `Fill` in height: this sits inside a scrollable, where Fill means unbounded.
            chat = chat.push(container(welcome).width(Length::Fill).padding(sp::S5).center_x(Length::Fill));
        }
        for m in &self.messages {
            let mine = m.role == "user";
            let bubble = container(body(m.body.clone())).padding([sp::S2, sp::S3]).max_width((theme::viewport_width() * 0.6).max(220.0)).style(theme::panel(if mine { c::GREEN_SOFT } else { c::PAGE }));
            chat = chat.push(if mine { row![iced::widget::Space::with_width(Length::Fill), bubble] } else { row![bubble] });
        }
        if self.busy {
            chat = chat.push(row![icon("sparkles", theme::size::ICON_SMALL, c::GREEN), muted("Quick is thinking...")].spacing(sp::S2).align_y(Alignment::Center));
        }
        let input = text_input("Ask about your contacts, outreach, or follow-ups...", &self.input)
            .on_input(Msg::Input)
            .on_submit(Msg::Send)
            .padding([sp::S2, sp::S3])
            .size(theme::size::TEXT)
            .style(theme::input_style(false))
            .width(Length::Fill);
        let send = button(container(icon("send", theme::size::ICON_SMALL, c::WHITE)).center_x(Length::Fixed(36.0)).center_y(Length::Fixed(36.0)))
            .padding(sp::S1)
            .height(Length::Fixed(40.0))
            .style(theme::button_style(Kind::Accent));
        let send = if self.busy || self.input.trim().is_empty() { send } else { send.on_press(Msg::Send) };
        let composer = row![input, send].spacing(sp::S2).align_y(Alignment::Center);
        let footer = row![muted("Quick uses dashboard data. Bigger requests go through the Claude Desktop queue."), Space::with_width(Length::Fill)].padding([sp::S1, sp::S2]);
        let panel = column![scrollable(chat).height(Length::Fill), divider(), composer, footer].spacing(sp::S2).padding(sp::S3);
        column![header, container(panel).width(Length::Fill).height(Length::Fixed(theme::height_share(0.75, 380.0, 900.0))).style(theme::card)].spacing(sp::S4).into()
    }

    fn history_view(&self) -> Element<'_, Msg> {
        let mut entries = column![].spacing(sp::S2);
        if self.messages.is_empty() {
            entries = entries.push(empty_state("No Quick history yet."));
        }
        for message in &self.messages {
            entries = entries.push(card_titled(
                if message.role == "user" { "You" } else { "Quick" },
                &message.at,
                body(message.body.clone()),
            ));
        }
        container(scrollable(entries).height(Length::Fixed(theme::height_share(0.75, 380.0, 900.0)))).padding(sp::S3).style(theme::card).into()
    }

    fn usage_view(&self) -> Element<'_, Msg> {
        let Some(report) = &self.usage else {
            return container(body("Loading AI usage...")).padding(sp::S4).style(theme::card).into();
        };
        let mut page = column![tile_grid(report.tiles.iter().map(|t| stat_tile(&t.label, &t.value)).collect(), 4)].spacing(sp::S3);
        for table in &report.tables {
            let mut rows = column![].spacing(sp::S1);
            // Columns share the window width, but never get too narrow to read.
            let col_w = Length::Fixed(((theme::viewport_width() - 120.0) / table.columns.len().max(1) as f32).clamp(110.0, 260.0));
            let mut header = Row::new().spacing(sp::S2);
            for name in &table.columns {
                header = header.push(container(body(name.clone()).size(theme::size::SMALL).font(theme::FONT_BOLD)).width(col_w));
            }
            rows = rows.push(header);
            if table.rows.is_empty() {
                rows = rows.push(muted("No AI usage recorded yet."));
            }
            for values in &table.rows {
                let mut row = Row::new().spacing(sp::S2);
                for value in values {
                    row = row.push(container(body(value.clone())).width(col_w));
                }
                rows = rows.push(row);
            }
            page = page.push(card_titled(&table.title, &table.note, scrollable(rows).direction(iced::widget::scrollable::Direction::Horizontal(iced::widget::scrollable::Scrollbar::new()))));
        }
        container(page).padding(sp::S3).style(theme::card).into()
    }
}
