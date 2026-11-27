//! One page for every list in the app (people, companies, emails, and so on).
//! It is driven by the module description from the server: columns, filters, fields, rules.

mod view;

use crate::api;
use crate::app::ctx::Ctx;
use crate::browser;
use iced::widget::text_editor;
use iced::Task;
use serde::Deserialize;
use shared::*;
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize, Debug, Clone)]
pub struct RecordResponse {
    pub record: Record,
    pub row_version: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GroupCreatedResponse {
    pub id: String,
    pub message: String,
}

pub struct Drawer {
    pub id: Option<String>,
    pub values: BTreeMap<String, String>,
    pub original: BTreeMap<String, String>,
    pub row_version: i64,
    pub errors: BTreeMap<String, String>,
    pub message: Option<(bool, String)>,
    pub loading: bool,
    pub saving: bool,
    pub editors: HashMap<String, text_editor::Content>,
    pub reply: text_editor::Content,
    pub email_subject: String,
    pub email_body: text_editor::Content,
}

impl Drawer {
    fn new(id: Option<String>) -> Self {
        Self {
            id, values: BTreeMap::new(), original: BTreeMap::new(), row_version: 0, errors: BTreeMap::new(), message: None,
            loading: false, saving: false, editors: HashMap::new(), reply: text_editor::Content::new(), email_subject: String::new(), email_body: text_editor::Content::new(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.values != self.original
    }
    fn load_values(&mut self, values: BTreeMap<String, String>, info: &ModuleInfo) {
        self.editors.clear();
        for f in &info.fields {
            if f.visible && matches!(f.kind, FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList) && f.editable {
                self.editors.insert(f.name.clone(), text_editor::Content::with_text(values.get(&f.name).map(String::as_str).unwrap_or("")));
            }
        }
        self.email_subject = values.get("subject").cloned().unwrap_or_default();
        self.email_body = text_editor::Content::with_text(values.get("body").map(String::as_str).unwrap_or(""));
        self.original = values.clone();
        self.values = values;
    }
}

pub struct State {
    pub key: String,
    pub route_key: String,
    pub info: Option<ModuleInfo>,
    pub query: QueryRequest,
    pub search: String,
    /// Text typed into the "contains" filter boxes, applied on Enter or Apply.
    pub texts: HashMap<String, String>,
    pub multi_filter_drafts: HashMap<String, String>,
    pub multi_filter_values: HashMap<String, Vec<String>>,
    pub mail_folder: String,
    pub mail_read_filter: String,
    pub conversation_filter: String,
    pub labels: Vec<String>,
    pub label_name: String,
    pub labels_busy: bool,
    pub compose_open: bool,
    pub compose_to: String,
    pub compose_subject: String,
    pub compose_body: text_editor::Content,
    pub compose_busy: bool,
    pub compose_message: Option<(bool, String)>,
    pub group_compose_open: bool,
    pub group_subject: String,
    pub group_participants: text_editor::Content,
    pub group_busy: bool,
    pub group_error: Option<String>,
    pub result: Option<QueryResult>,
    pub thread: Vec<ConversationMessage>,
    pub thread_error: Option<String>,
    pub thread_loading: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub drawer: Option<Drawer>,
    pending_conversation_action: Option<String>,
    pub import_note: Option<Result<ImportReport, String>>,
    req: u64,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Info(Result<ModuleInfo, ApiError>),
    Loaded(u64, Result<QueryResult, ApiError>),
    Search(String),
    SubmitSearch,
    Filter(String, String),
    FilterText(String, String),
    MultiFilterDraft(String, String),
    AddMultiFilter(String),
    RemoveMultiFilter(String, String),
    MailFolder(String),
    MailReadFilter(String),
    ConversationFilter(String),
    LabelsLoaded(String, Result<Vec<String>, ApiError>),
    LabelName(String),
    CreateLabel,
    LabelCreated(String, Result<Vec<String>, ApiError>),
    DeleteLabel(String),
    LabelDeleted(String, String, Result<Vec<String>, ApiError>),
    ToggleLabel(String),
    LabelsSaved(Result<Vec<String>, ApiError>),
    Compose,
    ComposeTo(String),
    ComposeSubject(String),
    ComposeBody(text_editor::Action),
    SendComposed,
    CancelCompose,
    Composed(Result<ActionResult, ApiError>),
    NewGroup,
    CancelGroup,
    GroupSubject(String),
    GroupParticipants(text_editor::Action),
    CreateGroup,
    GroupCreated(Result<GroupCreatedResponse, ApiError>),
    ThreadLoaded(String, Result<Vec<ConversationMessage>, ApiError>),
    ClearFilters,
    Sort(String),
    Page(i64),
    Refresh,
    Open(String),
    New,
    Close,
    RecordLoaded(Result<RecordResponse, ApiError>),
    Edit(String, String),
    Editor(String, text_editor::Action),
    Save,
    Saved(Result<SaveResult, ApiError>),
    Export,
    Template,
    File(Result<Vec<u8>, ApiError>, String),
    Import,
    Picked(Option<Vec<u8>>),
    Imported(Result<ImportReport, ApiError>),
    Action(String),
    ActionDone(Result<ActionResult, ApiError>),
    Reply(text_editor::Action),
    SendReply,
    EmailSubject(String),
    EmailBody(text_editor::Action),
    SaveEmail,
    EmailChecked(Result<DraftCheckResult, ApiError>),
}

/// Pages that show an existing list with a ready-made filter.
pub fn preset(route_key: &str) -> (Vec<Filter>, Option<(&'static str, &'static str)>) {
    match route_key {
        "enrichment" => (
            vec![Filter { field: "email_status".into(), op: FilterOp::Ne, value: "verified".into() }],
            Some(("Enrichment", "People whose email is not verified yet. Finish the missing data here.")),
        ),
        "schedule" => (
            vec![Filter { field: "status".into(), op: FilterOp::Eq, value: "queued".into() }],
            Some(("Schedule", "Emails that wait for their send time. They go out only inside the send window.")),
        ),
        "domain_health" => (vec![], Some(("Domain health", "Mailboxes, daily limits and how many were sent today."))),
        "inbox" => (vec![], Some(("Mailbox", "Mail sent and received."))),
        "competitors" => (
            vec![Filter { field: "is_competitor".into(), op: FilterOp::Eq, value: "true".into() }],
            Some(("Competitors", "Companies marked as competitors.")),
        ),
        _ => (vec![], None),
    }
}

pub fn extra_actions(key: &str, v: &BTreeMap<String, String>) -> Vec<(&'static str, &'static str, &'static str)> {
    let st = v.get("status").map(String::as_str).unwrap_or("");
    match key {
        "conversations" => match st {
            "open" => vec![("Take over", "user", "take-over"), ("Add 10 minutes", "clock", "extend")],
            "employee" | "handover" => vec![("Hand back to AI", "zap", "hand-back")],
            _ => vec![],
        },
        "campaigns" => match st {
            "draft" => vec![("Start campaign", "play", "start")],
            "paused" => vec![("Resume", "play", "resume")],
            "running" => vec![("Pause campaign", "pause", "pause")],
            _ => vec![],
        },
        "emails" if st == "waiting_approval" => vec![("Approve and send", "ok", "approve")],
        "candidates" if st == "needs_review" => vec![("Save as company", "ok", "approve"), ("Reject", "close", "reject")],
        _ => vec![],
    }
}

pub fn filter_visible(module: &str, field: &FieldInfo) -> bool {
    field.filterable
        && field.in_list
        && !(matches!(module, "people" | "companies") && matches!(field.name.as_str(), "email" | "emails" | "phone" | "phone_numbers" | "links" | "employees"))
}

pub fn multi_select_filter(module: &str, field: &str) -> bool {
    match module {
        "people" => matches!(field, "job_title" | "company" | "city" | "keywords"),
        "companies" => matches!(field, "city" | "keywords"),
        _ => false,
    }
}

impl State {
    pub fn new(route_key: &str, key: &str, search: Option<String>) -> (Self, Task<Msg>) {
        let (filters, _) = preset(route_key);
        let mut query = QueryRequest::default();
        query.filters = filters;
        if let Some(s) = &search {
            query.search = s.clone();
        }
        let st = State {
            key: key.to_string(), route_key: route_key.to_string(), info: None, query, search: search.unwrap_or_default(),
            texts: HashMap::new(), multi_filter_drafts: HashMap::new(), multi_filter_values: HashMap::new(),
            mail_folder: if route_key == "schedule" { "queued".into() } else { "all".into() },
            mail_read_filter: "all".into(), conversation_filter: "all".into(), labels: vec![], label_name: String::new(), labels_busy: false,
            compose_open: false, compose_to: String::new(), compose_subject: String::new(), compose_body: text_editor::Content::new(),
            compose_busy: false, compose_message: None, group_compose_open: false, group_subject: String::new(),
            group_participants: text_editor::Content::new(), group_busy: false, group_error: None,
            result: None, thread: vec![], thread_error: None, thread_loading: false,
            loading: true, error: None, drawer: None, pending_conversation_action: None, import_note: None, req: 0,
        };
        let path = format!("/api/v1/modules/{}", api::path_segment(key));
        (st, Task::perform(async move { api::get::<ModuleInfo>(&path).await }, Msg::Info))
    }

    fn load(&mut self, ctx: &Ctx) -> Task<Msg> {
        self.req += 1;
        self.loading = true;
        let id = self.req;
        let path = format!("/api/v1/records/{}/query", api::path_segment(&self.key));
        let csrf = ctx.csrf();
        let q = self.query.clone();
        Task::perform(async move { (id, api::send::<QueryResult, _>("POST", &path, &csrf, &q).await) }, |(i, r)| Msg::Loaded(i, r))
    }

    pub fn can_write(&self, ctx: &Ctx) -> bool {
        self.info.as_ref().map(|i| ctx.can(&i.write_permission)).unwrap_or(false)
    }

    pub fn update(&mut self, msg: Msg, ctx: &mut Ctx) -> Task<Msg> {
        match msg {
            Msg::Info(Ok(i)) => {
                self.info = Some(i);
                let mut tasks = vec![self.load(ctx)];
                if let Some(kind) = self.label_kind() {
                    let path = format!("/api/v1/labels/{kind}");
                    tasks.push(Task::perform(async move { api::get::<Vec<String>>(&path).await }, move |r| Msg::LabelsLoaded(kind.to_string(), r)));
                }
                return Task::batch(tasks);
            }
            Msg::Info(Err(e)) => {
                self.loading = false;
                self.error = Some(e.message.clone());
                ctx.failed(&e);
            }
            Msg::Loaded(id, r) => {
                if id != self.req {
                    return Task::none();
                }
                self.loading = false;
                match r {
                    Ok(res) => {
                        self.error = None;
                        self.result = Some(res);
                    }
                    Err(e) => {
                        self.error = Some(e.message.clone());
                        ctx.failed(&e);
                    }
                }
            }
            Msg::Search(s) => self.search = s,
            Msg::FilterText(field, value) => {
                self.texts.insert(field, value);
            }
            Msg::MultiFilterDraft(field, value) => {
                self.multi_filter_drafts.insert(field, value);
            }
            Msg::AddMultiFilter(field) => {
                let value = self.multi_filter_drafts.remove(&field).unwrap_or_default().trim().to_string();
                if value.is_empty() {
                    return Task::none();
                }
                let selected = self.multi_filter_values.entry(field.clone()).or_default();
                if selected.iter().any(|v| v.eq_ignore_ascii_case(&value)) {
                    return Task::none();
                }
                selected.push(value);
                self.apply_multi_filter(&field);
                return self.load(ctx);
            }
            Msg::RemoveMultiFilter(field, value) => {
                if let Some(selected) = self.multi_filter_values.get_mut(&field) {
                    selected.retain(|v| v != &value);
                    if selected.is_empty() {
                        self.multi_filter_values.remove(&field);
                    }
                }
                self.apply_multi_filter(&field);
                return self.load(ctx);
            }
            Msg::MailFolder(folder) => {
                self.set_mail_folder(&folder);
                return self.load(ctx);
            }
            Msg::MailReadFilter(filter) => {
                self.set_mail_read_filter(&filter);
                return self.load(ctx);
            }
            Msg::ConversationFilter(filter) => {
                self.set_conversation_filter(&filter);
                return self.load(ctx);
            }
            Msg::LabelsLoaded(kind, result) => {
                if self.label_kind() == Some(kind.as_str()) {
                    match result {
                        Ok(labels) => self.labels = labels,
                        Err(e) => {
                            self.error = Some(e.message.clone());
                            ctx.failed(&e);
                        }
                    }
                }
            }
            Msg::LabelName(name) => self.label_name = name,
            Msg::CreateLabel => {
                if self.labels_busy {
                    return Task::none();
                }
                let Some(kind) = self.label_kind() else { return Task::none() };
                let name = self.label_name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }
                let path = format!("/api/v1/labels/{kind}");
                let csrf = ctx.csrf();
                self.labels_busy = true;
                return Task::perform(
                    async move { api::send::<Vec<String>, _>("POST", &path, &csrf, &serde_json::json!({"name": name})).await },
                    move |r| Msg::LabelCreated(kind.to_string(), r),
                );
            }
            Msg::LabelCreated(kind, result) => {
                self.labels_busy = false;
                if self.label_kind() == Some(kind.as_str()) {
                    match result {
                        Ok(labels) => {
                            self.labels = labels;
                            self.label_name.clear();
                            ctx.say("Label saved.");
                        }
                        Err(e) => ctx.failed(&e),
                    }
                }
            }
            Msg::DeleteLabel(name) => {
                if self.labels_busy {
                    return Task::none();
                }
                let Some(kind) = self.label_kind() else { return Task::none() };
                let path = format!("/api/v1/labels/{kind}");
                let csrf = ctx.csrf();
                self.labels_busy = true;
                let request_name = name.clone();
                return Task::perform(
                    async move { api::send::<Vec<String>, _>("DELETE", &path, &csrf, &serde_json::json!({"name": request_name})).await },
                    move |r| Msg::LabelDeleted(kind.to_string(), name.clone(), r),
                );
            }
            Msg::LabelDeleted(kind, deleted, result) => {
                self.labels_busy = false;
                if self.label_kind() == Some(kind.as_str()) {
                    match result {
                        Ok(labels) => {
                            self.labels = labels;
                            if let Some(d) = self.drawer.as_mut() {
                                let remaining: Vec<&str> = d.values.get("labels").map(|v| v.lines().filter(|label| !label.eq_ignore_ascii_case(&deleted)).collect()).unwrap_or_default();
                                let value = remaining.join("\n");
                                d.values.insert("labels".into(), value.clone());
                                d.original.insert("labels".into(), value);
                            }
                            ctx.say("Label deleted.");
                        }
                        Err(e) => ctx.failed(&e),
                    }
                }
            }
            Msg::ToggleLabel(name) => {
                if self.labels_busy {
                    return Task::none();
                }
                let (Some(kind), Some(id), Some(d)) = (self.label_kind(), self.drawer.as_ref().and_then(|d| d.id.clone()), self.drawer.as_ref()) else {
                    return Task::none();
                };
                let mut selected: Vec<String> = d.values.get("labels").map(|v| v.lines().map(str::to_string).collect()).unwrap_or_default();
                if selected.iter().any(|label| label == &name) {
                    selected.retain(|label| label != &name);
                } else {
                    selected.push(name);
                }
                let path = format!("/api/v1/labels/{kind}/records/{id}");
                let csrf = ctx.csrf();
                self.labels_busy = true;
                return Task::perform(
                    async move { api::send::<Vec<String>, _>("POST", &path, &csrf, &serde_json::json!({"labels": selected})).await },
                    Msg::LabelsSaved,
                );
            }
            Msg::LabelsSaved(Ok(selected)) => {
                self.labels_busy = false;
                if let Some(d) = self.drawer.as_mut() {
                    let value = selected.join("\n");
                    d.values.insert("labels".into(), value.clone());
                    d.original.insert("labels".into(), value);
                }
            }
            Msg::LabelsSaved(Err(e)) => {
                self.labels_busy = false;
                ctx.failed(&e);
            }
            Msg::Compose => {
                self.compose_open = true;
                self.compose_message = None;
            }
            Msg::ComposeTo(value) => self.compose_to = value,
            Msg::ComposeSubject(value) => self.compose_subject = value,
            Msg::ComposeBody(action) => self.compose_body.perform(action),
            Msg::CancelCompose => {
                self.compose_open = false;
                self.compose_message = None;
            }
            Msg::SendComposed => {
                let body = self.compose_body.text().trim().to_string();
                if self.compose_to.trim().is_empty() || self.compose_subject.trim().is_empty() || body.is_empty() {
                    self.compose_message = Some((false, "Enter a recipient, subject, and message.".into()));
                    return Task::none();
                }
                self.compose_busy = true;
                let csrf = ctx.csrf();
                let request = serde_json::json!({"to": self.compose_to, "subject": self.compose_subject, "body": body});
                return Task::perform(async move { api::send::<ActionResult, _>("POST", "/api/v1/emails/compose", &csrf, &request).await }, Msg::Composed);
            }
            Msg::Composed(Ok(result)) => {
                self.compose_open = false;
                self.compose_busy = false;
                self.compose_to.clear();
                self.compose_subject.clear();
                self.compose_body = text_editor::Content::new();
                ctx.say(&result.message);
                return self.load(ctx);
            }
            Msg::Composed(Err(e)) => {
                self.compose_busy = false;
                self.compose_message = Some((false, e.message.clone()));
                ctx.failed(&e);
            }
            Msg::NewGroup => {
                self.group_compose_open = true;
                self.group_error = None;
            }
            Msg::CancelGroup => {
                self.group_compose_open = false;
                self.group_error = None;
            }
            Msg::GroupSubject(value) => self.group_subject = value,
            Msg::GroupParticipants(action) => self.group_participants.perform(action),
            Msg::CreateGroup => {
                let participant_emails: Vec<String> = self.group_participants.text().lines().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect();
                if self.group_subject.trim().is_empty() || participant_emails.len() < 2 {
                    self.group_error = Some("Enter a subject and at least two contact email addresses, one per line.".into());
                    return Task::none();
                }
                self.group_busy = true;
                let csrf = ctx.csrf();
                let request = serde_json::json!({"subject": self.group_subject, "participant_emails": participant_emails});
                return Task::perform(async move { api::send::<GroupCreatedResponse, _>("POST", "/api/v1/conversations/groups", &csrf, &request).await }, Msg::GroupCreated);
            }
            Msg::GroupCreated(Ok(result)) => {
                self.group_busy = false;
                self.group_compose_open = false;
                self.group_subject.clear();
                self.group_participants = text_editor::Content::new();
                ctx.say(&result.message);
                let open = self.update(Msg::Open(result.id), ctx);
                return Task::batch([open, self.load(ctx)]);
            }
            Msg::GroupCreated(Err(e)) => {
                self.group_busy = false;
                self.group_error = Some(e.message.clone());
                ctx.failed(&e);
            }
            Msg::ThreadLoaded(id, result) => {
                if self.drawer.as_ref().and_then(|d| d.id.as_deref()) == Some(id.as_str()) {
                    self.thread_loading = false;
                    match result {
                        Ok(messages) => {
                            self.thread = messages;
                            self.thread_error = None;
                            if self.key == "conversations" {
                                return self.load(ctx);
                            }
                        }
                        Err(e) => {
                            self.thread.clear();
                            self.thread_error = Some(e.message.clone());
                            ctx.failed(&e);
                        }
                    }
                }
            }
            Msg::ClearFilters => {
                let (filters, _) = preset(&self.route_key);
                self.query.filters = filters;
                self.query.search.clear();
                self.search.clear();
                self.texts.clear();
                self.multi_filter_drafts.clear();
                self.multi_filter_values.clear();
                self.query.page = 0;
                return self.load(ctx);
            }
            Msg::SubmitSearch => {
                let texts: Vec<(String, String)> = self.texts.iter().map(|(k, v)| (k.clone(), v.trim().to_string())).collect();
                for (field, value) in texts {
                    self.query.filters.retain(|f| f.field != field);
                    if !value.is_empty() {
                        let op = if self.info.as_ref().and_then(|i| i.fields.iter().find(|f| f.name == field)).is_some_and(|f| matches!(f.kind, FieldKind::Number | FieldKind::Integer)) {
                            FilterOp::Eq
                        } else {
                            FilterOp::Contains
                        };
                        self.query.filters.push(Filter { field, op, value });
                    }
                }
                self.query.search = self.search.trim().to_string();
                self.query.page = 0;
                return self.load(ctx);
            }
            Msg::Filter(field, value) => {
                self.query.filters.retain(|f| f.field != field);
                if !value.is_empty() {
                    self.query.filters.push(Filter { field, op: FilterOp::Eq, value });
                }
                self.query.page = 0;
                return self.load(ctx);
            }
            Msg::Sort(field) => {
                if self.query.sort_field.as_deref() == Some(field.as_str()) {
                    self.query.sort_desc = !self.query.sort_desc;
                } else {
                    self.query.sort_field = Some(field);
                    self.query.sort_desc = false;
                }
                self.query.page = 0;
                return self.load(ctx);
            }
            Msg::Page(delta) => {
                let last = self.last_page();
                self.query.page = (self.query.page as i64 + delta).clamp(0, last as i64) as u32;
                return self.load(ctx);
            }
            Msg::Refresh => return self.load(ctx),
            Msg::New => {
                self.drawer = Some(Drawer::new(None));
                if let (Some(d), Some(i)) = (self.drawer.as_mut(), self.info.as_ref()) {
                    let mut v = BTreeMap::new();
                    for f in i.fields.iter().filter(|f| f.editable && f.visible) {
                        v.insert(f.name.clone(), String::new());
                    }
                    d.load_values(v, i);
                }
            }
            Msg::Open(id) => {
                let mut d = Drawer::new(Some(id.clone()));
                d.loading = true;
                self.drawer = Some(d);
                self.thread.clear();
                self.thread_error = None;
                self.thread_loading = self.key == "conversations";
                let path = format!("/api/v1/records/{}/{}", api::path_segment(&self.key), api::path_segment(&id));
                let record = Task::perform(async move { api::get::<RecordResponse>(&path).await }, Msg::RecordLoaded);
                if self.key == "conversations" {
                    let path = format!("/api/v1/conversations/{}/messages", api::path_segment(&id));
                    let thread = Task::perform(async move { api::get::<Vec<ConversationMessage>>(&path).await }, move |r| Msg::ThreadLoaded(id.clone(), r));
                    return Task::batch([record, thread]);
                }
                return record;
            }
            Msg::Close => {
                self.drawer = None;
                self.thread.clear();
                self.thread_error = None;
                self.thread_loading = false;
            }
            Msg::RecordLoaded(Ok(r)) => {
                if let (Some(d), Some(i)) = (self.drawer.as_mut(), self.info.as_ref()) {
                    if d.id.as_deref() == Some(r.record.id.as_str()) {
                        d.row_version = r.row_version;
                        d.loading = false;
                        d.load_values(r.record.values, i);
                        if self.key == "emails" {
                            return self.load(ctx);
                        }
                    }
                }
            }
            Msg::RecordLoaded(Err(e)) => {
                self.drawer = None;
                self.thread_loading = false;
                ctx.failed(&e);
            }
            Msg::Edit(field, value) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.errors.remove(&field);
                    d.values.insert(field, value);
                }
            }
            Msg::Editor(field, action) => {
                if let Some(d) = self.drawer.as_mut() {
                    if let Some(c) = d.editors.get_mut(&field) {
                        c.perform(action);
                        let t = c.text();
                        d.values.insert(field.clone(), t.trim_end_matches('\n').to_string());
                        d.errors.remove(&field);
                    }
                }
            }
            Msg::Save => return self.save(ctx),
            Msg::Saved(Ok(_)) => {
                self.drawer = None;
                ctx.say("Saved.");
                return self.load(ctx);
            }
            Msg::Saved(Err(e)) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.saving = false;
                    d.errors = e.fields.clone();
                    d.message = Some((false, if e.code == "conflict" { "Someone else changed this record. Close it and open it again to see the latest version.".into() } else { e.message.clone() }));
                }
                if e.code == "unauthorized" {
                    ctx.failed(&e);
                }
            }
            Msg::Export => {
                let path = format!("/api/v1/records/{}/export", api::path_segment(&self.key));
                let csrf = ctx.csrf();
                let body = serde_json::to_vec(&self.query).unwrap_or_default();
                let name = format!("{}.csv", self.key);
                return Task::perform(async move { api::bytes("POST", &path, &csrf, Some(body)).await }, move |r| Msg::File(r, name.clone()));
            }
            Msg::Template => {
                let path = format!("/api/v1/records/{}/template", api::path_segment(&self.key));
                let name = format!("{}-template.csv", self.key);
                return Task::perform(async move { api::bytes("GET", &path, "", None).await }, move |r| Msg::File(r, name.clone()));
            }
            Msg::File(Ok(bytes), name) => browser::download(&name, "text/csv", &bytes),
            Msg::File(Err(e), _) => ctx.failed(&e),
            Msg::Import => return Task::perform(browser::pick_csv(), Msg::Picked),
            Msg::Picked(None) => {}
            Msg::Picked(Some(bytes)) => {
                let path = format!("/api/v1/records/{}/import", api::path_segment(&self.key));
                let csrf = ctx.csrf();
                return Task::perform(async move { api::send_csv::<ImportReport>(&path, &csrf, bytes).await }, Msg::Imported);
            }
            Msg::Imported(Ok(r)) => {
                let ok = r.errors.is_empty();
                self.import_note = Some(Ok(r));
                if ok {
                    ctx.say("Import finished.");
                    return self.load(ctx);
                }
            }
            Msg::Imported(Err(e)) => {
                self.import_note = Some(Err(e.message.clone()));
                ctx.failed(&e);
            }
            Msg::Action(endpoint) => {
                if let Some(d) = self.drawer.as_mut() {
                    if let Some(id) = d.id.clone() {
                        d.saving = true;
                        if self.key == "conversations" {
                            self.pending_conversation_action = Some(endpoint.clone());
                        }
                        let path = format!("/api/v1/{}/{}/{}", api::path_segment(&self.key), api::path_segment(&id), endpoint);
                        let csrf = ctx.csrf();
                        return Task::perform(async move { api::post::<ActionResult>(&path, &csrf).await }, Msg::ActionDone);
                    }
                }
            }
            Msg::ActionDone(Ok(r)) => {
                if self.key == "conversations" {
                    if r.ok {
                        ctx.say(&r.message);
                        if let Some(d) = self.drawer.as_mut() {
                            d.message = None;
                        }
                        if self.pending_conversation_action.as_deref() == Some("reply") {
                            if let Some(d) = self.drawer.as_mut() {
                                d.reply = text_editor::Content::new();
                            }
                        }
                    } else if let Some(d) = self.drawer.as_mut() {
                        d.message = Some((false, r.message.clone()));
                    }
                    let refresh_record = self.pending_conversation_action.as_deref() == Some("archive");
                    self.pending_conversation_action = None;
                    let list = self.load(ctx);
                    if let Some(id) = self.drawer.as_ref().and_then(|d| d.id.clone()) {
                        if let Some(d) = self.drawer.as_mut() {
                            d.saving = false;
                        }
                        let path = format!("/api/v1/conversations/{}/messages", api::path_segment(&id));
                        let thread_id = id.clone();
                        let thread = Task::perform(async move { api::get::<Vec<ConversationMessage>>(&path).await }, move |r| Msg::ThreadLoaded(thread_id.clone(), r));
                        let mut tasks = vec![list, thread];
                        if refresh_record {
                            let path = format!("/api/v1/records/conversations/{}", api::path_segment(&id));
                            let record = Task::perform(async move { api::get::<RecordResponse>(&path).await }, Msg::RecordLoaded);
                            tasks.push(record);
                        }
                        return Task::batch(tasks);
                    }
                    return list;
                }
                self.drawer = None;
                ctx.say(&r.message);
                return self.load(ctx);
            }
            Msg::ActionDone(Err(e)) => {
                self.pending_conversation_action = None;
                if let Some(d) = self.drawer.as_mut() {
                    d.saving = false;
                    d.message = Some((false, e.message.clone()));
                }
                ctx.failed(&e);
            }
            Msg::Reply(a) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.reply.perform(a);
                }
            }
            Msg::SendReply => {
                if let Some(d) = self.drawer.as_mut() {
                    let text = d.reply.text();
                    if text.trim().is_empty() {
                        d.message = Some((false, "Write the reply first.".into()));
                        return Task::none();
                    }
                    if let Some(id) = d.id.clone() {
                        d.saving = true;
                        if self.key == "conversations" {
                            self.pending_conversation_action = Some("reply".into());
                        }
                        let path = format!("/api/v1/conversations/{}/reply", api::path_segment(&id));
                        let csrf = ctx.csrf();
                        return Task::perform(async move { api::send::<ActionResult, _>("POST", &path, &csrf, &serde_json::json!({"body": text})).await }, |r| Msg::ActionDone(r));
                    }
                }
            }
            Msg::EmailSubject(s) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.email_subject = s;
                }
            }
            Msg::EmailBody(a) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.email_body.perform(a);
                }
            }
            Msg::SaveEmail => {
                if let Some(d) = self.drawer.as_mut() {
                    if let Some(id) = d.id.clone() {
                        d.saving = true;
                        let path = format!("/api/v1/emails/{}/edit", api::path_segment(&id));
                        let csrf = ctx.csrf();
                        let body = serde_json::json!({"subject": d.email_subject, "body": d.email_body.text().trim_end_matches('\n')});
                        return Task::perform(async move { api::send::<DraftCheckResult, _>("POST", &path, &csrf, &body).await }, Msg::EmailChecked);
                    }
                }
            }
            Msg::EmailChecked(Ok(r)) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.saving = false;
                    if r.ok {
                        self.drawer = None;
                        ctx.say("Email saved. It passed the writing checks.");
                        return self.load(ctx);
                    }
                    d.message = Some((false, format!("Not saved. {}", r.problems.join(" "))));
                }
            }
            Msg::EmailChecked(Err(e)) => {
                if let Some(d) = self.drawer.as_mut() {
                    d.saving = false;
                    d.message = Some((false, e.message.clone()));
                }
                ctx.failed(&e);
            }
        }
        Task::none()
    }

    fn save(&mut self, ctx: &mut Ctx) -> Task<Msg> {
        let (Some(d), Some(info)) = (self.drawer.as_mut(), self.info.as_ref()) else { return Task::none() };
        if d.saving {
            return Task::none();
        }

        // Only editable fields are sent. The server checks them again.
        let mut values = BTreeMap::new();
        for f in info.fields.iter().filter(|f| f.editable && f.visible) {
            if let Some(v) = d.values.get(&f.name) {
                values.insert(f.name.clone(), v.clone());
            }
        }
        if self.route_key == "competitors" && d.id.is_none() {
            values.insert("is_competitor".into(), "true".into());
        }
        let missing: BTreeMap<String, String> = info.fields.iter().filter(|f| f.editable && f.visible && f.required && values.get(&f.name).map(|v| v.trim().is_empty()).unwrap_or(true)).map(|f| (f.name.clone(), format!("{} is required.", f.label))).collect();
        if !missing.is_empty() {
            d.errors = missing;
            d.message = Some((false, "Please fill in the required fields.".into()));
            return Task::none();
        }
        d.saving = true;
        d.message = None;
        let csrf = ctx.csrf();
        let key = api::path_segment(&self.key);
        match d.id.clone() {
            None => {
                let body = SaveRecord { values, row_version: None };
                Task::perform(async move { api::send::<SaveResult, _>("POST", &format!("/api/v1/records/{key}"), &csrf, &body).await }, Msg::Saved)
            }
            Some(id) => {
                let body = SaveRecord { values, row_version: Some(d.row_version) };
                Task::perform(async move { api::send::<SaveResult, _>("PUT", &format!("/api/v1/records/{key}/{}", api::path_segment(&id)), &csrf, &body).await }, Msg::Saved)
            }
        }
    }

    fn apply_multi_filter(&mut self, field: &str) {
        self.query.filters.retain(|f| f.field != field);
        if let Some(values) = self.multi_filter_values.get(field).filter(|values| !values.is_empty()) {
            let op = if field == "keywords" { FilterOp::Contains } else { FilterOp::Eq };
            self.query.filters.push(Filter { field: field.to_string(), op, value: values.join("\n") });
        }
        self.query.page = 0;
    }

    fn set_mail_folder(&mut self, folder: &str) {
        self.mail_folder = folder.to_string();
        self.rebuild_email_filters();
    }

    fn set_mail_read_filter(&mut self, filter: &str) {
        self.mail_read_filter = filter.to_string();
        self.rebuild_email_filters();
    }

    fn rebuild_email_filters(&mut self) {
        self.query.filters = if self.mail_folder == "queued" { preset(&self.route_key).0 } else { vec![] };
        match self.mail_folder.as_str() {
            "inbox" => self.query.filters.push(Filter { field: "direction".into(), op: FilterOp::Eq, value: "in".into() }),
            "sent" => {
                self.query.filters.push(Filter { field: "direction".into(), op: FilterOp::Eq, value: "out".into() });
                self.query.filters.push(Filter { field: "status".into(), op: FilterOp::Eq, value: "sent".into() });
            }
            "queued" if self.route_key != "schedule" => self.query.filters.push(Filter { field: "status".into(), op: FilterOp::Eq, value: "queued".into() }),
            "approval" => self.query.filters.push(Filter { field: "status".into(), op: FilterOp::Eq, value: "waiting_approval".into() }),
            "blocked" => self.query.filters.push(Filter { field: "status".into(), op: FilterOp::Eq, value: "blocked".into() }),
            _ => {}
        }
        match self.mail_read_filter.as_str() {
            "read" | "unread" => {
                if !self.query.filters.iter().any(|f| f.field == "direction") {
                    self.query.filters.push(Filter { field: "direction".into(), op: FilterOp::Eq, value: "in".into() });
                }
                self.query.filters.push(Filter { field: "read".into(), op: FilterOp::Eq, value: (self.mail_read_filter == "read").to_string() });
            }
            _ => {}
        }
        self.query.page = 0;
    }

    fn set_conversation_filter(&mut self, filter: &str) {
        self.conversation_filter = filter.to_string();
        self.query.filters.retain(|f| !matches!(f.field.as_str(), "group" | "archived" | "unread"));
        match filter {
            "group" => self.query.filters.push(Filter { field: "group".into(), op: FilterOp::Eq, value: "true".into() }),
            "archive" => self.query.filters.push(Filter { field: "archived".into(), op: FilterOp::Eq, value: "true".into() }),
            "unread" => self.query.filters.push(Filter { field: "unread".into(), op: FilterOp::Eq, value: "true".into() }),
            _ => {}
        }
        self.query.page = 0;
    }

    fn label_kind(&self) -> Option<&'static str> {
        match self.key.as_str() {
            "emails" => Some("email"),
            "conversations" => Some("conversation"),
            _ => None,
        }
    }

    pub fn last_page(&self) -> u32 {
        match &self.result {
            Some(r) if r.page_size > 0 => ((r.total.max(1) - 1) / r.page_size as i64) as u32,
            _ => 0,
        }
    }

    pub fn view<'a>(&'a self, ctx: &'a Ctx) -> iced::Element<'a, Msg> {
        view::view(self, ctx)
    }

    pub fn overlay<'a>(&'a self, ctx: &'a Ctx) -> Option<iced::Element<'a, Msg>> {
        view::overlay(self, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ModuleInfo {
        ModuleInfo {
            key: "people".into(), title: "People".into(), subtitle: "".into(), singular: "contact".into(),
            fields: vec![
                FieldInfo { name: "first_name".into(), label: "First name".into(), kind: FieldKind::Text, choices: vec![], in_list: true, visible: true, editable: true, required: true, filterable: false, sortable: true },
                FieldInfo { name: "score".into(), label: "Score".into(), kind: FieldKind::Number, choices: vec![], in_list: true, visible: true, editable: false, required: false, filterable: false, sortable: true },
            ],
            can_create: true, can_export: true, can_import: true, empty_text: "".into(), write_permission: "record.write".into(),
        }
    }

    fn ctx() -> Ctx {
        let s = SessionInfo { user_id: "u".into(), name: "A B".into(), email: "a@b.com".into(), role: "admin".into(), tenant_name: "T".into(), permissions: vec!["record.write".into()], csrf: "c".into(), hard_expires_in: 1, idle_expires_in: 1, sending_paused: false, unread_notices: 0 };
        Ctx::new(s, vec![], 0, 1200.0)
    }

    fn state() -> State {
        let (mut s, _) = State::new("people", "people", None);
        s.info = Some(info());
        s
    }

    #[test]
    fn saving_without_a_required_field_stops_before_the_server() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::New, &mut c);
        let _ = s.update(Msg::Save, &mut c);
        let d = s.drawer.as_ref().unwrap();
        assert!(!d.saving, "nothing was sent");
        assert_eq!(d.errors["first_name"], "First name is required.");
    }

    #[test]
    fn computed_fields_are_not_sent() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::New, &mut c);
        let _ = s.update(Msg::Edit("first_name".into(), "Sana".into()), &mut c);
        s.drawer.as_mut().unwrap().values.insert("score".into(), "99".into());
        let _ = s.update(Msg::Save, &mut c);
        assert!(s.drawer.as_ref().unwrap().saving, "a valid form is sent");
    }

    #[test]
    fn editing_clears_that_fields_error() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::New, &mut c);
        let _ = s.update(Msg::Save, &mut c);
        let _ = s.update(Msg::Edit("first_name".into(), "S".into()), &mut c);
        assert!(!s.drawer.as_ref().unwrap().errors.contains_key("first_name"));
        assert!(s.drawer.as_ref().unwrap().dirty());
    }

    #[test]
    fn old_answers_are_ignored() {
        let mut s = state();
        let mut c = ctx();
        s.req = 5;
        let stale = QueryResult { columns: vec![], rows: vec![], total: 9, page: 0, page_size: 25 };
        let _ = s.update(Msg::Loaded(4, Ok(stale)), &mut c);
        assert!(s.result.is_none());
    }

    #[test]
    fn paging_stays_inside_the_list() {
        let mut s = state();
        let mut c = ctx();
        s.result = Some(QueryResult { columns: vec![], rows: vec![], total: 60, page: 0, page_size: 25 });
        assert_eq!(s.last_page(), 2);
        let _ = s.update(Msg::Page(5), &mut c);
        assert_eq!(s.query.page, 2);
        let _ = s.update(Msg::Page(-9), &mut c);
        assert_eq!(s.query.page, 0);
    }

    #[test]
    fn sort_toggles_direction_on_the_same_column() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::Sort("score".into()), &mut c);
        assert!(!s.query.sort_desc);
        let _ = s.update(Msg::Sort("score".into()), &mut c);
        assert!(s.query.sort_desc);
        let _ = s.update(Msg::Sort("first_name".into()), &mut c);
        assert!(!s.query.sort_desc && s.query.sort_field.as_deref() == Some("first_name"));
    }

    #[test]
    fn expired_session_is_flagged() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::Info(Err(ApiError::new("unauthorized", "Please sign in."))), &mut c);
        assert!(c.expired);
    }

    #[test]
    fn presets_filter_the_shared_lists() {
        let (f, t) = preset("enrichment");
        assert_eq!(f.len(), 1);
        assert_eq!(t.unwrap().0, "Enrichment");
        assert!(preset("people").0.is_empty());
        let (f, t) = preset("competitors");
        assert_eq!(f[0].field, "is_competitor");
        assert_eq!(t.unwrap().0, "Competitors");
    }

    #[test]
    fn crm_filter_controls_hide_contact_channels_and_employee_count() {
        let hidden = |name: &str| FieldInfo {
            name: name.into(), label: name.into(), kind: FieldKind::Text, choices: vec![], in_list: true,
            visible: true, editable: false, required: false, filterable: true, sortable: false,
        };
        for name in ["email", "emails", "phone", "phone_numbers", "links", "employees"] {
            assert!(!filter_visible("people", &hidden(name)));
            assert!(!filter_visible("companies", &hidden(name)));
        }
        assert!(filter_visible("people", &hidden("job_title")));
    }

    #[test]
    fn multi_select_filters_are_limited_to_requested_crm_fields() {
        assert!(multi_select_filter("people", "job_title"));
        assert!(multi_select_filter("people", "company"));
        assert!(multi_select_filter("people", "city"));
        assert!(multi_select_filter("people", "keywords"));
        assert!(multi_select_filter("companies", "city"));
        assert!(multi_select_filter("companies", "keywords"));
        assert!(!multi_select_filter("companies", "employees"));
        assert!(!multi_select_filter("emails", "company"));
    }

    #[test]
    fn multi_filter_values_are_added_removed_and_combined_with_any_semantics() {
        let mut s = state();
        let mut c = ctx();
        let _ = s.update(Msg::MultiFilterDraft("job_title".into(), "Founder".into()), &mut c);
        let _ = s.update(Msg::AddMultiFilter("job_title".into()), &mut c);
        let _ = s.update(Msg::MultiFilterDraft("job_title".into(), "CEO".into()), &mut c);
        let _ = s.update(Msg::AddMultiFilter("job_title".into()), &mut c);
        assert_eq!(s.query.filters.iter().find(|f| f.field == "job_title").unwrap().value, "Founder\nCEO");
        let _ = s.update(Msg::RemoveMultiFilter("job_title".into(), "Founder".into()), &mut c);
        assert_eq!(s.query.filters.iter().find(|f| f.field == "job_title").unwrap().value, "CEO");
    }

    #[test]
    fn mail_folders_keep_their_filter_meaning() {
        let (mut s, _) = State::new("inbox", "emails", None);
        s.set_mail_folder("sent");
        assert_eq!(s.mail_folder, "sent");
        assert!(s.query.filters.iter().any(|f| f.field == "direction" && f.value == "out"));
        assert!(s.query.filters.iter().any(|f| f.field == "status" && f.value == "sent"));
        s.set_mail_folder("all");
        assert!(s.query.filters.is_empty());
    }

    #[test]
    fn read_filters_are_per_user_incoming_email_filters() {
        let (mut s, _) = State::new("inbox", "emails", None);
        s.set_mail_read_filter("unread");
        assert!(s.query.filters.iter().any(|f| f.field == "direction" && f.value == "in"));
        assert!(s.query.filters.iter().any(|f| f.field == "read" && f.value == "false"));
        s.set_mail_read_filter("read");
        assert!(s.query.filters.iter().any(|f| f.field == "read" && f.value == "true"));
        s.set_mail_read_filter("all");
        assert!(s.query.filters.is_empty());
    }

    #[test]
    fn chat_filters_select_group_archive_and_unread_state() {
        let (mut s, _) = State::new("conversations", "conversations", None);
        for (tab, field) in [("group", "group"), ("archive", "archived"), ("unread", "unread")] {
            s.set_conversation_filter(tab);
            assert_eq!(s.query.filters.len(), 1);
            assert_eq!(s.query.filters[0].field, field);
            assert_eq!(s.query.filters[0].value, "true");
        }
        s.set_conversation_filter("all");
        assert!(s.query.filters.is_empty());
    }

    #[test]
    fn actions_depend_on_status() {
        let mut v = BTreeMap::new();
        v.insert("status".to_string(), "draft".to_string());
        assert_eq!(extra_actions("campaigns", &v)[0].2, "start");
        v.insert("status".to_string(), "running".to_string());
        assert_eq!(extra_actions("campaigns", &v)[0].2, "pause");
        v.insert("status".to_string(), "sent".to_string());
        assert!(extra_actions("emails", &v).is_empty());
    }
}
