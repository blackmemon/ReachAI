//! Types shared by the frontend (Iced WASM) and the backend.
//! No database, UI or browser types live here (src/project-structure.md section 7).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod nav;
pub mod permissions;
pub mod settings_defs;

/// Error body returned by every failing API call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    /// Field name to message, for form errors.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

impl ApiError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CsrfToken {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: String,
    pub tenant_name: String,
    pub permissions: Vec<String>,
    pub csrf: String,
    /// Seconds until the 24-hour hard limit.
    pub hard_expires_in: i64,
    /// Seconds until the idle timeout.
    pub idle_expires_in: i64,
    pub sending_paused: bool,
    pub unread_notices: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    LongText,
    MultiLine,
    EmailList,
    UrlList,
    Number,
    Integer,
    Bool,
    Date,
    DateTime,
    Email,
    Url,
    Choice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldInfo {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    /// Shown in the record drawer.
    pub visible: bool,
    /// Allowed values when kind is Choice.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Shown as a table column.
    pub in_list: bool,
    pub editable: bool,
    pub required: bool,
    pub filterable: bool,
    pub sortable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleInfo {
    pub key: String,
    pub title: String,
    pub subtitle: String,
    pub singular: String,
    pub fields: Vec<FieldInfo>,
    pub can_create: bool,
    pub can_export: bool,
    pub can_import: bool,
    /// Short note shown when the table is empty.
    pub empty_text: String,
    /// Permission needed to create or change a record.
    pub write_permission: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Ne,
    Contains,
    Gt,
    Lt,
    IsEmpty,
    NotEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryRequest {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sort_field: Option<String>,
    #[serde(default)]
    pub sort_desc: bool,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page_size() -> u32 {
    25
}

impl Default for QueryRequest {
    fn default() -> Self {
        Self {
            search: String::new(),
            filters: vec![],
            sort_field: None,
            sort_desc: false,
            page: 0,
            page_size: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Row {
    pub id: String,
    /// Display text, one per column in `QueryResult::columns`.
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Column {
    pub name: String,
    pub label: String,
    pub sortable: bool,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

/// One record: field name to display value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Record {
    pub id: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveRecord {
    pub values: BTreeMap<String, String>,
    /// Required when changing a record (optimistic lock).
    #[serde(default)]
    pub row_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveResult {
    pub id: String,
    pub row_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatTile {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NeedItem {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub when: String,
    /// Route key to open.
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TodayData {
    pub tiles: Vec<StatTile>,
    pub needs_you: Vec<NeedItem>,
    pub stages: Vec<StatTile>,
    pub runs: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportTable {
    pub title: String,
    pub note: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportSet {
    pub tabs: Vec<String>,
    pub tab: String,
    pub tiles: Vec<StatTile>,
    pub tables: Vec<ReportTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuickMessage {
    pub role: String,
    pub body: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationMessage {
    pub direction: String,
    pub sender: String,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuickAsk {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingValue {
    pub key: String,
    pub label: String,
    pub help: String,
    pub kind: String,
    pub default: String,
    pub value: String,
    pub choices: Vec<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsPage {
    pub group: String,
    pub title: String,
    pub items: Vec<SettingValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveSettings {
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notice {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub when: String,
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportReport {
    pub added: u32,
    pub updated: u32,
    pub skipped_duplicates: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftCheckRequest {
    pub subject: String,
    pub body: String,
    pub step: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftCheckResult {
    pub ok: bool,
    pub problems: Vec<String>,
}

/// The stages a contact moves through (Today page, "Contacts by stage").
pub const STAGES: [(&str, &str); 8] = [
    ("new", "New"),
    ("to_write", "To write"),
    ("contacted", "Contacted"),
    ("replied", "Replied"),
    ("interested", "Interested"),
    ("ready_to_meet", "Ready to meet"),
    ("won", "Won"),
    ("closed_out", "Closed out"),
];

pub fn stage_label(key: &str) -> &str {
    STAGES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or("Not available")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_labels_known_and_unknown() {
        assert_eq!(stage_label("ready_to_meet"), "Ready to meet");
        assert_eq!(stage_label("nope"), "Not available");
    }

    #[test]
    fn query_request_defaults_from_empty_json() {
        let q: QueryRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(q.page_size, 25);
        assert_eq!(q.page, 0);
    }

    #[test]
    fn login_request_rejects_unknown_fields() {
        let r: Result<LoginRequest, _> =
            serde_json::from_str(r#"{"email":"a","password":"b","x":1}"#);
        assert!(r.is_err());
    }
}

// ---- AI research workspace (chat sessions, live progress, structured results) ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchSession {
    pub id: String,
    pub mode: String,
    pub title: String,
    pub updated_at: String,
    pub results: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchMessage {
    pub role: String,
    pub body: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResearchResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub website: String,
    pub address: String,
    pub location: String,
    pub phone: String,
    pub email: String,
    pub socials: String,
    pub source_url: String,
    /// "verified", "partial" or "unverified".
    pub verification: String,
    pub notes: String,
    pub services: String,
    pub pricing: String,
    pub reviews: String,
    pub presence: String,
    pub offerings: String,
    /// What was done with it: "", "candidate" (sent to AI Leads review) or "competitor".
    pub saved_as: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchEvent {
    pub id: i64,
    /// "step", "site", "lead", "note" or "error".
    pub kind: String,
    pub body: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchRunInfo {
    pub id: String,
    /// "running", "done", "failed" or "cancelled".
    pub status: String,
    pub step: String,
    pub error: String,
    pub found: i64,
    pub duplicates: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchSessionView {
    pub session: ResearchSession,
    pub messages: Vec<ResearchMessage>,
    pub results: Vec<ResearchResult>,
    /// The latest run, and its events, so a reopened chat can show the work.
    pub run: Option<ResearchRunInfo>,
    pub events: Vec<ResearchEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchPoll {
    pub run: ResearchRunInfo,
    pub events: Vec<ResearchEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchSend {
    /// Empty to start a new conversation in `mode`.
    #[serde(default)]
    pub session_id: String,
    pub mode: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchStarted {
    pub session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchExport {
    /// "csv" or "xlsx".
    pub format: String,
    /// Empty means every result of the conversation.
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchIds {
    pub ids: Vec<String>,
}
