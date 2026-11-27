use super::{extra_actions, filter_visible, multi_select_filter, preset, Drawer, Msg, State};
use crate::app::ctx::Ctx;
use crate::components::*;
use crate::format;
use crate::icons::icon;
use crate::theme::{self, c, dim, sp, Kind};
use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, stack, text, text_editor, text_input, Row, Space};
use iced::{Alignment, Element, Length};
use shared::*;

const ANY: &str = "Any";

fn cell_text(kind: FieldKind, raw: &str, tz: i32) -> String {
    if raw.is_empty() {
        return "Not available".into();
    }
    match kind {
        FieldKind::DateTime => format::local_time(raw, tz),
        FieldKind::Bool => if raw == "true" { "Yes".into() } else { "No".into() },
        FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList => raw.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(", "),
        _ => raw.to_string(),
    }
}

pub fn view<'a>(s: &'a State, ctx: &'a Ctx) -> Element<'a, Msg> {
    let Some(info) = &s.info else {
        return match &s.error {
            Some(e) => banner(Tone::Red, e),
            None => body("Loading").into(),
        };
    };
    let (_, titles) = preset(&s.route_key);
    let (title, subtitle) = titles.map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((info.title.clone(), info.subtitle.clone()));
    if info.key == "emails" {
        return emails_page(s, ctx);
    }
    if info.key == "conversations" {
        return conversations_page(s, info, ctx);
    }

    let mut actions: Vec<Element<'a, Msg>> = vec![];
    if info.can_import && s.can_write(ctx) {
        actions.push(button_with("Template", Some("download"), Kind::Secondary, Some(Msg::Template)).into());
        actions.push(button_with("Import CSV", Some("upload"), Kind::Secondary, Some(Msg::Import)).into());
    }
    if info.can_export {
        actions.push(button_with("Export CSV", Some("download"), Kind::Secondary, Some(Msg::Export)).into());
    }
    if info.can_create && s.can_write(ctx) {
        let label = if s.route_key == "competitors" { "New competitor".to_string() } else { format!("New {}", info.singular) };
        actions.push(button_with(&label, Some("plus"), Kind::Primary, Some(Msg::New)).into());
    }

    let mut page = column![page_header(&title, &subtitle, actions)].spacing(sp::S4);
    if let Some(note) = &s.import_note {
        page = page.push(import_banner(note));
    }
    page = page.push(toolbar(s, info, ctx));
    if let Some(e) = &s.error {
        page = page.push(banner(Tone::Red, e));
    }
    page = page.push(table(s, info, ctx));
    page = page.push(pager(s));

    page.into()
}

/// The side drawer, drawn over the whole window (scrim, then the panel on the right).
pub fn overlay<'a>(s: &'a State, ctx: &'a Ctx) -> Option<Element<'a, Msg>> {
    let info = s.info.as_ref()?;
    if info.key == "conversations" {
        return None;
    }
    let d = s.drawer.as_ref()?;
    let scrim = button(Space::new(Length::Fill, Length::Fill)).on_press(Msg::Close).style(|_, _| button::Style { background: Some(iced::Background::Color(iced::Color { a: 0.35, ..c::INK })), ..Default::default() }).width(Length::Fill).height(Length::Fill);
    // A drawer takes up to its full width on a big screen, and at most half of a small one.
    let panel = container(drawer(s, info, d, ctx)).width(Length::Fill).max_width(if ctx.phone() { 10_000.0 } else { dim::DRAWER.min(ctx.width * 0.6) }).height(Length::Fill).style(theme::drawer);
    let side = row![Space::with_width(Length::Fill), panel];
    Some(stack![scrim, side].into())
}

fn row_value<'a>(result: &'a QueryResult, row: &'a shared::Row, name: &str) -> &'a str {
    result.columns.iter().position(|c| c.name == name).and_then(|i| row.cells.get(i)).map(String::as_str).unwrap_or("")
}

fn emails_page<'a>(s: &'a State, ctx: &'a Ctx) -> Element<'a, Msg> {
    let folders = [
        ("All mail", "all", "mail"),
        ("Inbox", "inbox", "inbox"),
        ("Sent", "sent", "send"),
        ("Queued", "queued", "clock"),
        ("Needs approval", "approval", "check"),
        ("Blocked", "blocked", "shield"),
    ];
    let folder_items = folders.into_iter().map(|(label_text, key, icon_name)| {
        let active = s.mail_folder == key;
        button(row![icon(icon_name, theme::size::ICON_SMALL + 2.0, if active { c::GREEN } else { c::MUTED }), text(label_text).size(theme::size::TEXT).color(if active { c::GREEN } else { c::INK })].spacing(sp::S2).align_y(Alignment::Center))
            .width(Length::Fill)
            .padding([sp::S2, sp::S3])
            .style(theme::button_style(if active { Kind::MenuActive } else { Kind::Menu }))
            .on_press(Msg::MailFolder(key.to_string()))
            .into()
    });
    let folder_items: Vec<Element<'a, Msg>> = folder_items.collect();
    let folder_nav: Element<'a, Msg> = if ctx.narrow() {
        Row::with_children(folder_items).spacing(sp::S1).wrap().into()
    } else {
        let items = folder_items.into_iter().fold(column![].spacing(sp::S1), |col, item| col.push(item));
        column![muted("FOLDERS"), items].spacing(sp::S2).into()
    };
    let sidebar = column![
        button_with("Compose mail", Some("plus"), Kind::Accent, Some(Msg::Compose)),
        muted("Messages are queued after the existing send checks."),
        divider(),
        folder_nav,
    ]
    .spacing(sp::S3);
    let sidebar = container(sidebar).padding(sp::S3).width(if ctx.narrow() { Length::Fill } else { Length::FillPortion(2) }).height(Length::Fill).style(theme::card);

    let search = text_input("Search emails...", &s.search)
        .on_input(Msg::Search)
        .on_submit(Msg::SubmitSearch)
        .padding([sp::S2, sp::S3])
        .size(theme::size::TEXT)
        .style(theme::input_style(false))
        .width(Length::Fill);
    let tabs = [("All", "all"), ("Read", "read"), ("Unread", "unread")]
    .into_iter()
    .map(|(label_text, key)| {
        button(text(label_text).size(theme::size::SMALL))
            .padding([sp::S1, sp::S3])
            .style(theme::button_style(if s.mail_read_filter == key { Kind::MenuActive } else { Kind::Menu }))
            .on_press(Msg::MailReadFilter(key.to_string()))
            .into()
    });
    let tabs: Vec<Element<'a, Msg>> = tabs.collect();

    let mut entries = column![].spacing(0);
    match &s.result {
        Some(result) if !result.rows.is_empty() => {
            for record in &result.rows {
                let from = row_value(result, record, "to_from");
                let subject = row_value(result, record, "subject");
                let preview = row_value(result, record, "preview");
                let status = row_value(result, record, "status");
                let unread = row_value(result, record, "read") == "No" && row_value(result, record, "direction") == "in";
                let when = row_value(result, record, "when");
                let selected = s.drawer.as_ref().and_then(|d| d.id.as_deref()) == Some(record.id.as_str());
                let date = if when.is_empty() { String::new() } else { format::local_time(when, ctx.tz) };
                let line = row![
                    if unread { strong(if from.is_empty() { "Unknown sender".into() } else { from.to_string() }) } else { body(if from.is_empty() { "Unknown sender".into() } else { from.to_string() }) },
                    Space::with_width(Length::Fill),
                    muted(date),
                ]
                .align_y(Alignment::Center);
                let subject_line = row![
                    body(if subject.is_empty() { "(no subject)".into() } else { subject.to_string() }),
                    Space::with_width(Length::Fill),
                    pill(&status.replace('_', " "), tone_of(status)),
                ]
                .align_y(Alignment::Center);
                let content = column![line, subject_line, muted(preview.to_string())].spacing(sp::S1);
                entries = entries.push(
                    button(container(content).padding([sp::S2, sp::S3]).width(Length::Fill))
                        .width(Length::Fill)
                        .style(theme::button_style(if selected { Kind::MenuActive } else { Kind::Menu }))
                        .on_press(Msg::Open(record.id.clone())),
                );
                entries = entries.push(divider());
            }
        }
        Some(_) => entries = entries.push(empty_state("No emails match this view.")),
        None if s.loading => entries = entries.push(body("Loading emails...")),
        None => entries = entries.push(empty_state("No emails yet.")),
    }
    let list = container(scrollable(entries).height(Length::Fill)).height(Length::Fill);
    let mut main = column![search, Row::with_children(tabs).spacing(sp::S1).wrap()].spacing(sp::S2);
    if let Some(error) = &s.error {
        main = main.push(banner(Tone::Red, error));
    }
    if s.compose_open {
        main = main.push(email_compose_panel(s));
    } else {
        main = main.push(muted(format!("{} emails", s.result.as_ref().map(|r| r.total).unwrap_or(0)))).push(list).push(pager(s));
    }
    main = main.padding(sp::S3).height(Length::Fill);
    let main = container(main).width(if ctx.narrow() { Length::Fill } else { Length::FillPortion(7) }).height(Length::Fill).style(theme::card);
    let split_height = Length::Fixed(theme::height_share(0.78, 420.0, 900.0));
    let content: Element<'a, Msg> = if ctx.narrow() { column![sidebar, main].spacing(sp::S3).height(split_height).into() } else { row![sidebar, main].spacing(sp::S3).height(split_height).into() };
    column![
        page_header("Email", "Review outbound mail and incoming replies.", vec![]),
        content,
    ]
    .spacing(sp::S4)
    .into()
}

fn email_compose_panel<'a>(s: &'a State) -> Element<'a, Msg> {
    let mut form = column![
        heading("Compose email"),
        text_field("To", "Existing contact email", &s.compose_to, Msg::ComposeTo, None, ""),
        text_field("Subject", "", &s.compose_subject, Msg::ComposeSubject, None, ""),
        field(
            "Message",
            text_editor(&s.compose_body).on_action(Msg::ComposeBody).height(Length::Fixed(240.0)).padding(sp::S3).size(theme::size::TEXT),
            None,
            "Only existing contacts can be selected. The email is checked and queued; it is not sent directly from this form.",
        ),
    ]
    .spacing(sp::S3);
    if let Some((ok, message)) = &s.compose_message {
        form = form.push(banner(if *ok { Tone::Green } else { Tone::Red }, message));
    }
    form = form.push(row![
        Space::with_width(Length::Fill),
        button_with("Cancel", None, Kind::Secondary, if s.compose_busy { None } else { Some(Msg::CancelCompose) }),
        button_with(if s.compose_busy { "Queueing..." } else { "Queue to send" }, Some("send"), Kind::Accent, if s.compose_busy { None } else { Some(Msg::SendComposed) }),
    ].spacing(sp::S2));
    card(form).into()
}

fn conversations_page<'a>(s: &'a State, info: &'a ModuleInfo, ctx: &'a Ctx) -> Element<'a, Msg> {
    let search = text_input("Search conversations...", &s.search)
        .on_input(Msg::Search)
        .on_submit(Msg::SubmitSearch)
        .padding([sp::S2, sp::S3])
        .size(theme::size::TEXT)
        .style(theme::input_style(false))
        .width(Length::Fill);
    let tabs = [("All", "all"), ("Group", "group"), ("Archive", "archive"), ("Unread", "unread")].into_iter().map(|(label_text, key)| {
        button(text(label_text).size(theme::size::SMALL))
            .padding([sp::S1, sp::S3])
            .style(theme::button_style(if s.conversation_filter == key { Kind::MenuActive } else { Kind::Menu }))
            .on_press(Msg::ConversationFilter(key.to_string()))
            .into()
    });
    let tabs: Vec<Element<'a, Msg>> = tabs.collect();
    let mut people = column![heading("Chats"), search, Row::with_children(tabs).spacing(sp::S1).wrap()].spacing(sp::S2);
    if let Some(error) = &s.error {
        people = people.push(banner(Tone::Red, error));
    }
    if let Some(result) = &s.result {
        if result.rows.is_empty() {
            people = people.push(empty_state(if s.loading { "Loading conversations..." } else { "No conversations match this view." }));
        } else {
            let mut rows = column![].spacing(sp::S1);
            for record in &result.rows {
                let contact = row_value(result, record, "contact");
                let preview = row_value(result, record, "preview");
                let status = row_value(result, record, "status");
                let urgent = row_value(result, record, "urgent") == "true";
                let unread = row_value(result, record, "unread") == "Yes";
                let group = row_value(result, record, "group") == "Yes";
                let when = row_value(result, record, "last_message_at");
                let active = s.drawer.as_ref().and_then(|d| d.id.as_deref()) == Some(record.id.as_str());
                let initials: String = contact.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
                let initials = if initials.is_empty() { "?".to_string() } else { initials };
                let avatar = container(text(initials).size(theme::size::SMALL).color(c::GREEN))
                    .width(Length::Fixed(34.0)).height(Length::Fixed(34.0)).center_x(Length::Fixed(34.0)).center_y(Length::Fixed(34.0)).style(theme::pill(c::GREEN, c::GREEN_SOFT));
                let date = if when.is_empty() { String::new() } else { format::local_time(when, ctx.tz) };
                let title_text = if unread { strong(if contact.is_empty() { "Unknown contact".into() } else { contact.to_string() }) } else { body(if contact.is_empty() { "Unknown contact".into() } else { contact.to_string() }) };
                let title = row![title_text, Space::with_width(Length::Fill), muted(date)];
                let entry = button(row![avatar, column![title, muted(row_value(result, record, "subject")), muted(preview.to_string())].spacing(sp::S1)].spacing(sp::S2).align_y(Alignment::Center))
                    .width(Length::Fill)
                    .padding(sp::S2)
                    .style(theme::button_style(if active { Kind::MenuActive } else { Kind::Menu }))
                    .on_press(Msg::Open(record.id.clone()));
                let badge = if unread { pill("Unread", Tone::Blue) } else if group { pill("Group", Tone::Green) } else if urgent { pill("Urgent", Tone::Amber) } else { pill(&status.replace('_', " "), tone_of(status)) };
                rows = rows.push(column![entry, badge].spacing(sp::S1)).push(divider());
            }
            people = people.push(scrollable(rows).height(Length::Fill));
        }
    } else {
        people = people.push(body("Loading conversations..."));
    }
    let people = container(people).padding(sp::S3).width(if ctx.narrow() { Length::Fill } else { Length::FillPortion(3) }).height(Length::Fill).style(theme::card);
    let detail = conversation_detail(s, info, ctx);
    let content: Element<'a, Msg> = if ctx.narrow() {
        if s.drawer.is_some() { detail } else { people.into() }
    } else {
        row![people, container(detail).width(Length::FillPortion(7))].spacing(sp::S3).height(Length::Fixed(theme::height_share(0.78, 420.0, 900.0))).into()
    };
    let actions = if ctx.can("conversation.reply") {
        vec![button_with("New group", Some("plus"), Kind::Accent, Some(Msg::NewGroup)).into()]
    } else {
        vec![]
    };
    column![page_header("Chats", "Conversations and replies from your contacts.", actions), content].spacing(sp::S4).into()
}

fn conversation_detail<'a>(s: &'a State, info: &'a ModuleInfo, ctx: &'a Ctx) -> Element<'a, Msg> {
    if s.group_compose_open {
        return group_compose_panel(s);
    }
    let Some(d) = &s.drawer else {
        return container(empty_state("Choose a conversation to read the thread and reply."))
            .width(Length::Fill).height(Length::Fill).style(theme::card).into();
    };
    if d.loading {
        return container(body("Loading conversation...")).padding(sp::S4).width(Length::Fill).height(Length::Fill).style(theme::card).into();
    }
    let contact = d.values.get("contact").map(String::as_str).unwrap_or("Contact");
    let subject = d.values.get("subject").map(String::as_str).unwrap_or("");
    let status = d.values.get("status").map(String::as_str).unwrap_or("");
    let back: Element<'a, Msg> = if ctx.narrow() { button_with("", Some("left"), Kind::Ghost, Some(Msg::Close)).into() } else { Space::with_width(Length::Fixed(0.0)).into() };
    let archived = d.values.get("archived").map(String::as_str) == Some("true");
    let header = row![
        back,
        container(text(contact).size(theme::size::HEADING).font(theme::FONT_BOLD)).width(Length::Fill),
        pill(&status.replace('_', " "), tone_of(status)),
    ]
    .spacing(sp::S2)
    .align_y(Alignment::Center);
    let mut actions = row![muted(subject), Space::with_width(Length::Fill)].spacing(sp::S2).align_y(Alignment::Center);
    if s.can_write(ctx) {
        actions = actions.push(button_with(if archived { "Unarchive" } else { "Archive" }, Some("archive"), Kind::Secondary, if d.saving { None } else { Some(Msg::Action("archive".into())) }));
        for (label_text, icon_name, endpoint) in extra_actions(&info.key, &d.values) {
            actions = actions.push(button_with(label_text, Some(icon_name), Kind::Secondary, if d.saving { None } else { Some(Msg::Action(endpoint.into())) }));
        }
    }
    let mut messages = column![].spacing(sp::S3).padding(sp::S3);
    if let Some(error) = &s.thread_error {
        messages = messages.push(banner(Tone::Red, error));
    } else if s.thread_loading {
        messages = messages.push(body("Loading messages..."));
    } else if s.thread.is_empty() {
        messages = messages.push(empty_state("No messages in this conversation yet."));
    }
    for message in &s.thread {
        let outgoing = message.direction == "out";
        let who = if outgoing { "You" } else if message.sender.is_empty() { contact } else { &message.sender };
        let at = if message.at.is_empty() { String::new() } else { format::local_time(&message.at, ctx.tz) };
        let bubble = container(column![muted(format!("{who} · {at}")), body(message.body.clone())].spacing(sp::S1))
            .padding(sp::S3)
            .max_width(620.0)
            .style(theme::panel(if outgoing { c::GREEN_SOFT } else { c::SOFT }));
        messages = messages.push(if outgoing { row![Space::with_width(Length::Fill), bubble] } else { row![bubble] });
    }
    let thread = scrollable(messages).height(Length::Fill);
    let mut panel = column![header, divider(), actions, thread].spacing(sp::S2).height(Length::Fill);
    if d.values.get("participants").is_some_and(|v| v.lines().count() > 1) {
        panel = panel.push(muted(format!("Participants: {}", d.values.get("participants").map(|v| v.lines().collect::<Vec<_>>().join(", ")).unwrap_or_default())));
    }
    panel = panel.push(label_controls(s, d, ctx));
    if let Some((ok, message)) = &d.message {
        panel = panel.push(banner(if *ok { Tone::Green } else { Tone::Red }, message));
    }
    if d.id.is_some() && s.can_write(ctx) {
        panel = panel.push(field(
            "Reply",
            text_editor(&d.reply).on_action(Msg::Reply).height(Length::Fixed(76.0)).padding(sp::S2).size(theme::size::TEXT),
            None,
            "Replies to groups are queued separately for each participant, with the same safety checks.",
        ));
        let can_send = !d.saving && !d.reply.text().trim().is_empty();
        panel = panel.push(row![
            Space::with_width(Length::Fill),
            button_with(if d.saving { "Sending..." } else { "Send reply" }, Some("send"), Kind::Accent, if can_send { Some(Msg::SendReply) } else { None }),
        ]);
    }
    container(panel.padding(sp::S3)).width(Length::Fill).height(Length::Fill).style(theme::card).into()
}

fn group_compose_panel<'a>(s: &'a State) -> Element<'a, Msg> {
    let mut form = column![
        heading("New group chat"),
        text_field("Subject", "", &s.group_subject, Msg::GroupSubject, None, ""),
        field(
            "Contact emails",
            text_editor(&s.group_participants).on_action(Msg::GroupParticipants).height(Length::Fixed(120.0)).padding(sp::S3).size(theme::size::TEXT),
            None,
            "Enter 2 to 20 existing contact email addresses, one per line.",
        ),
    ]
    .spacing(sp::S3);
    if let Some(error) = &s.group_error {
        form = form.push(banner(Tone::Red, error));
    }
    form = form.push(row![
        Space::with_width(Length::Fill),
        button_with("Cancel", None, Kind::Secondary, if s.group_busy { None } else { Some(Msg::CancelGroup) }),
        button_with(if s.group_busy { "Creating..." } else { "Create group" }, Some("plus"), Kind::Accent, if s.group_busy { None } else { Some(Msg::CreateGroup) }),
    ].spacing(sp::S2));
    container(card(form)).width(Length::Fill).height(Length::Fill).into()
}

fn label_controls<'a>(s: &'a State, d: &'a Drawer, ctx: &Ctx) -> Element<'a, Msg> {
    if d.id.is_none() {
        return Space::with_height(0).into();
    }
    let selected: Vec<&str> = d.values.get("labels").map(|value| value.lines().collect()).unwrap_or_default();
    let mut labels = column![muted("LABELS")].spacing(sp::S1);
    for name in &s.labels {
        if s.can_write(ctx) {
            let check = checkbox(name.clone(), selected.contains(&name.as_str())).on_toggle({
                let name = name.clone();
                move |_| Msg::ToggleLabel(name.clone())
            }).style(theme::check_style);
            let remove: Element<'a, Msg> = if ["Personal", "Work", "Important", "Updates"].iter().any(|default| default.eq_ignore_ascii_case(name)) {
                Space::with_width(Length::Fixed(0.0)).into()
            } else {
                button_with("", Some("close"), Kind::Ghost, Some(Msg::DeleteLabel(name.clone()))).into()
            };
            labels = labels.push(row![check, Space::with_width(Length::Fill), remove].align_y(Alignment::Center));
        } else if selected.contains(&name.as_str()) {
            labels = labels.push(body(name.clone()));
        }
    }
    if s.can_write(ctx) {
        let input = text_input("Create a label", &s.label_name)
            .on_input(Msg::LabelName)
            .on_submit(Msg::CreateLabel)
            .padding([sp::S2, sp::S3])
            .size(theme::size::TEXT)
            .style(theme::input_style(false))
            .width(Length::Fill);
        labels = labels.push(row![input, button_with("Add", Some("plus"), Kind::Secondary, Some(Msg::CreateLabel))].spacing(sp::S1));
    }
    container(labels).padding(sp::S2).style(theme::flat(c::PAGE)).into()
}

fn import_banner<'a>(note: &Result<ImportReport, String>) -> Element<'a, Msg> {
    match note {
        Err(m) => banner(Tone::Red, m),
        Ok(r) if r.errors.is_empty() => banner(Tone::Green, &format!("Import finished: {} added, {} updated, {} skipped as duplicates.", r.added, r.updated, r.skipped_duplicates)),
        Ok(r) => {
            let mut col = column![banner(Tone::Red, "Nothing was imported. Fix these rows and try again.")].spacing(sp::S1);
            for e in r.errors.iter().take(10) {
                col = col.push(muted(e.clone()));
            }
            if r.errors.len() > 10 {
                col = col.push(muted(format!("And {} more.", r.errors.len() - 10)));
            }
            col.into()
        }
    }
}

fn toolbar<'a>(s: &'a State, info: &'a ModuleInfo, ctx: &'a Ctx) -> Element<'a, Msg> {
    let phone = ctx.phone();
    let search_w = if phone { Length::Fill } else { Length::FillPortion(3) };
    let search = text_input("Search", &s.search).on_input(Msg::Search).on_submit(Msg::SubmitSearch).padding([10.0, sp::S3]).size(theme::size::TEXT).style(theme::input_style(false)).width(search_w);
    let narrowed = !s.search.is_empty() || !s.texts.values().all(|v| v.is_empty()) || s.query.filters.len() > preset(&s.route_key).0.len();
    let mut top = row![search, button_with("Search", Some("search"), Kind::Secondary, Some(Msg::SubmitSearch))].spacing(sp::S2).align_y(Alignment::Center);
    if narrowed {
        top = top.push(button_with("Clear filters", Some("close"), Kind::Ghost, Some(Msg::ClearFilters)));
    }
    let mut pickers: Vec<Element<'a, Msg>> = vec![];
    for f in info.fields.iter().filter(|f| filter_visible(&info.key, f)) {
        let w = Length::Fixed(if phone { 200.0 } else { 220.0 });
        let current = |default: &str| s.query.filters.iter().find(|x| x.field == f.name && x.op == FilterOp::Eq).map(|x| x.value.clone()).unwrap_or_else(|| default.to_string());
        let name = f.name.clone();
        if multi_select_filter(&info.key, &f.name) {
            let draft = s.multi_filter_drafts.get(&f.name).map(String::as_str).unwrap_or("");
            let name_input = name.clone();
            let input = text_input("Add value, press Enter", draft)
                .on_input(move |v| Msg::MultiFilterDraft(name_input.clone(), v))
                .on_submit(Msg::AddMultiFilter(name.clone()))
                .padding([10.0, sp::S3])
                .size(theme::size::TEXT)
                .style(theme::input_style(false))
                .width(Length::Fill);
            let values = s.multi_filter_values.get(&f.name).cloned().unwrap_or_default();
            let chips = values.into_iter().map(|value| {
                let remove = Msg::RemoveMultiFilter(f.name.clone(), value.clone());
                container(row![text(value).size(theme::size::SMALL), button_with("", Some("close"), Kind::Ghost, Some(remove))].spacing(sp::S1).align_y(Alignment::Center))
                    .padding([sp::S1, sp::S2])
                    .style(theme::panel(c::BLUE_SOFT))
                    .into()
            });
            let control: Element<'a, Msg> = column![
                row![input, button_with("", Some("plus"), Kind::Secondary, Some(Msg::AddMultiFilter(f.name.clone())))].spacing(sp::S1),
                Row::with_children(chips).spacing(sp::S1).wrap(),
                muted("Matches any selected value."),
            ]
            .spacing(sp::S1)
            .width(w)
            .into();
            pickers.push(column![label(f.label.clone()), control].spacing(sp::S1).into());
            continue;
        }
        let control: Element<'a, Msg> = match f.kind {
            FieldKind::Choice => {
                let mut opts = vec![ANY.to_string()];
                opts.extend(f.choices.iter().cloned());
                pick_list(opts, Some(current(ANY)), move |v: String| Msg::Filter(name.clone(), if v == ANY { String::new() } else { v })).text_size(theme::size::TEXT).padding([10.0, sp::S3]).style(theme::pick_style).menu_style(theme::menu_style).width(w).into()
            }
            FieldKind::Bool => {
                let shown = match current("").as_str() { "true" => "Yes", "false" => "No", _ => ANY }.to_string();
                pick_list(vec![ANY.to_string(), "Yes".to_string(), "No".to_string()], Some(shown), move |v: String| Msg::Filter(name.clone(), match v.as_str() { "Yes" => "true".into(), "No" => "false".into(), _ => String::new() })).text_size(theme::size::TEXT).padding([10.0, sp::S3]).style(theme::pick_style).menu_style(theme::menu_style).width(w).into()
            }
            FieldKind::Text | FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList | FieldKind::Email | FieldKind::Url | FieldKind::Number | FieldKind::Integer => {
                let value = s.texts.get(&f.name).cloned().or_else(|| s.query.filters.iter().find(|x| x.field == f.name).map(|x| x.value.clone())).unwrap_or_default();
                let placeholder = if matches!(f.kind, FieldKind::Number | FieldKind::Integer) { "Equals" } else { "Contains" };
                text_input(placeholder, &value).on_input(move |v| Msg::FilterText(name.clone(), v)).on_submit(Msg::SubmitSearch).padding([10.0, sp::S3]).size(theme::size::TEXT).style(theme::input_style(false)).width(w).into()
            }
            _ => continue,
        };
        pickers.push(column![label(f.label.clone()), control].spacing(sp::S1).into());
    }
    let mut col = column![top].spacing(sp::S3);
    if !pickers.is_empty() {
        col = col.push(Row::with_children(pickers).spacing(sp::S3).wrap());
    }
    col.into()
}

fn table<'a>(s: &'a State, info: &'a ModuleInfo, ctx: &'a Ctx) -> Element<'a, Msg> {
    let Some(res) = &s.result else {
        return card(body(if s.loading { "Loading" } else { "No data." })).into();
    };
    if res.rows.is_empty() {
        let narrowed = !s.query.search.is_empty() || s.query.filters.len() > preset(&s.route_key).0.len();
        return card(empty_state(if narrowed { "Nothing matches. Clear the search or a filter to see more." } else { &info.empty_text })).into();
    }
    let total: f32 = res.columns.iter().map(|col| theme::column_width(col.kind)).sum();
    // Wide screens: the columns share the whole card. Narrow screens: fixed widths and a sideways scroll.
    let room = ctx.width - ctx.sidebar - 2.0 * sp::S5 - 2.0;
    let fill = room >= total * 0.9;
    let width_of = move |kind: FieldKind| if fill { Length::FillPortion(theme::column_width(kind) as u16) } else { Length::Fixed(theme::column_width(kind)) };
    let mut head = row![].spacing(0).width(if fill { Length::Fill } else { Length::Shrink });
    for col in &res.columns {
        let w = width_of(col.kind);
        let arrow = if s.query.sort_field.as_deref() == Some(col.name.as_str()) { if s.query.sort_desc { " ↓" } else { " ↑" } } else { "" };
        let l = format!("{}{}", col.label, arrow);
        let cell: Element<'a, Msg> = if col.sortable {
            button(text(l).size(theme::size::SMALL).font(theme::FONT_BOLD).color(c::MUTED)).padding([sp::S2, sp::S3]).width(w).style(theme::button_style(Kind::Menu)).on_press(Msg::Sort(col.name.clone())).into()
        } else {
            container(text(l).size(theme::size::SMALL).font(theme::FONT_BOLD).color(c::MUTED)).padding([sp::S2, sp::S3]).width(w).into()
        };
        head = head.push(cell);
    }
    let mut rows = column![container(head).width(if fill { Length::Fill } else { Length::Shrink }).style(theme::flat(c::SOFT))].spacing(0).width(if fill { Length::Fill } else { Length::Shrink });
    for r in &res.rows {
        let mut line = row![].align_y(Alignment::Center).width(if fill { Length::Fill } else { Length::Shrink });
        for (i, col) in res.columns.iter().enumerate() {
            let raw = r.cells.get(i).map(String::as_str).unwrap_or("");
            let shown = cell_text(col.kind, raw, ctx.tz);
            let w = width_of(col.kind);
            let inner: Element<'a, Msg> = if col.kind == FieldKind::Choice && !raw.is_empty() { pill(&shown.replace('_', " "), tone_of(raw)) } else { text(shown).size(theme::size::TEXT).color(if raw.is_empty() { c::MUTED } else { c::INK }).into() };
            line = line.push(container(inner).padding([0.0, sp::S3]).width(w));
        }
        let b = button(line).padding([sp::S3, 0.0]).width(if fill { Length::Fill } else { Length::Shrink }).height(Length::Fixed(dim::ROW)).style(theme::button_style(Kind::Menu)).on_press(Msg::Open(r.id.clone()));
        rows = rows.push(b).push(if fill { divider() } else { divider_fixed(total) });
    }
    if fill {
        card(rows).padding(0).into()
    } else {
        card(scrollable(rows).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new()))).padding(0).into()
    }
}

fn pager<'a>(s: &'a State) -> Element<'a, Msg> {
    let Some(res) = &s.result else { return Space::with_height(0).into() };
    let from = if res.total == 0 { 0 } else { res.page as i64 * res.page_size as i64 + 1 };
    let to = ((res.page as i64 + 1) * res.page_size as i64).min(res.total);
    row![
        muted(format!("{from} to {to} of {}", res.total)),
        Space::with_width(Length::Fill),
        button_with("Previous", Some("left"), Kind::Secondary, if res.page > 0 { Some(Msg::Page(-1)) } else { None }),
        button_with("Next", Some("right"), Kind::Secondary, if res.page < s.last_page() { Some(Msg::Page(1)) } else { None }),
    ]
    .spacing(sp::S2)
    .align_y(Alignment::Center)
    .into()
}

fn drawer<'a>(s: &'a State, info: &'a ModuleInfo, d: &'a Drawer, ctx: &'a Ctx) -> Element<'a, Msg> {
    let title = if d.id.is_none() { format!("New {}", info.singular) } else { capitalise(&info.singular) };
    let head = row![heading(title), Space::with_width(Length::Fill), icon_button("close", Msg::Close)].align_y(Alignment::Center);
    if d.loading {
        return column![head, body("Loading")].spacing(sp::S4).padding(sp::S5).into();
    }
    let can_write = s.can_write(ctx);
    let mut form = column![].spacing(sp::S3).padding(iced::Padding { right: sp::S3, ..Default::default() });
    if let Some((ok, m)) = &d.message {
        form = form.push(banner(if *ok { Tone::Green } else { Tone::Red }, m));
    }
    for f in info.fields.iter().filter(|f| f.visible) {
        let value = d.values.get(&f.name).cloned().unwrap_or_default();
        let err = d.errors.get(&f.name).map(String::as_str);
        let name = f.name.clone();
        let editable = f.editable && can_write;
        let control: Element<'a, Msg> = if !editable {
            if d.id.is_none() { continue; }
            container(text(cell_text(f.kind, &value, ctx.tz)).size(theme::size::TEXT)).padding([sp::S2, sp::S3]).width(Length::Fill).style(theme::flat(c::PAGE)).into()
        } else {
            match f.kind {
                FieldKind::Bool => checkbox("", value == "true").on_toggle(move |b| Msg::Edit(name.clone(), b.to_string())).style(theme::check_style).into(),
                FieldKind::Choice => {
                    let mut opts = vec![String::new()];
                    opts.extend(f.choices.iter().cloned());
                    pick_list(opts, Some(value), move |v: String| Msg::Edit(name.clone(), v)).placeholder("Choose").text_size(theme::size::TEXT).padding([sp::S2, sp::S3]).style(theme::pick_style).menu_style(theme::menu_style).width(Length::Fill).into()
                }
                FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList => match d.editors.get(&f.name) {
                    Some(content) => {
                        let height = if f.kind == FieldKind::LongText { 140.0 } else { 80.0 };
                        text_editor(content).on_action(move |a| Msg::Editor(name.clone(), a)).height(Length::Fixed(height)).padding(sp::S3).size(theme::size::TEXT).into()
                    }
                    None => body(value).into(),
                },
                _ => input("", &value, move |v| Msg::Edit(name.clone(), v), err.is_some()).on_submit(Msg::Save).into(),
            }
        };
        form = form.push(field(&f.label, control, err, if f.required { "Required" } else { "" }));
    }
    if info.key == "emails" && d.id.is_some() {
        form = form.push(label_controls(s, d, ctx));
    }

    let mut foot = row![].spacing(sp::S2).align_y(Alignment::Center);
    if can_write {
        foot = foot.push(button_with(if d.saving { "Saving" } else { "Save" }, None, Kind::Primary, if d.saving { None } else { Some(Msg::Save) }));
    }
    foot = foot.push(secondary("Close", Msg::Close));
    let values = &d.values;
    let mut extras = column![].spacing(sp::S2);
    for (label_text, icon_name, endpoint) in extra_actions(&info.key, values) {
        extras = extras.push(button_with(label_text, Some(icon_name), Kind::Secondary, if d.saving { None } else { Some(Msg::Action(endpoint.to_string())) }));
    }
    if s.route_key == "competitors" && d.id.is_some() && can_write {
        extras = extras.push(button_with("Remove competitor", Some("close"), Kind::Secondary, if d.saving { None } else { Some(Msg::Action("remove-competitor".into())) }));
    }
    let mut parts = column![head, scrollable(form).height(Length::Fill)].spacing(sp::S4);
    parts = parts.push(extras);
    if info.key == "conversations" && d.id.is_some() && can_write {
        parts = parts.push(field("Your reply", text_editor(&d.reply).on_action(Msg::Reply).height(Length::Fixed(100.0)).padding(sp::S3).size(theme::size::TEXT), None, ""));
        parts = parts.push(button_with("Send reply", Some("send"), Kind::Primary, if d.saving { None } else { Some(Msg::SendReply) }));
    }
    if info.key == "emails" && values.get("status").map(String::as_str) == Some("waiting_approval") && can_write {
        parts = parts.push(text_field("Edit subject", "", &d.email_subject, Msg::EmailSubject, None, ""));
        parts = parts.push(field("Edit message", text_editor(&d.email_body).on_action(Msg::EmailBody).height(Length::Fixed(140.0)).padding(sp::S3).size(theme::size::TEXT), None, "The writing checks run when you save."));
        parts = parts.push(secondary("Save edited email", Msg::SaveEmail));
    }
    parts = parts.push(foot);
    parts.padding(sp::S5).into()
}

fn capitalise(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

#[allow(dead_code)]
fn _keep(_: text::Text<'static>) {}
