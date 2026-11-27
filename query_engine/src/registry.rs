//! Module registry: one place that says which fields exist, which are searchable,
//! filterable, sortable or editable. Every request is checked against it, so a user
//! can never name a column that is not listed here (no user SQL).

use shared::FieldKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Write {
    No,
    /// Write to this column of the main table.
    Col(&'static str),
    /// Write an id found by matching a text value in another table of the same tenant.
    Ref { col: &'static str, table: &'static str, by: &'static str },
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    /// Read expression. The main table alias is `m`.
    pub expr: &'static str,
    pub write: Write,
    pub choices: &'static [&'static str],
    pub list: bool,
    pub visible: bool,
    pub required: bool,
    pub filter: bool,
    pub sort: bool,
    pub search: bool,
    /// Empty text is stored as NULL.
    pub nullable: bool,
}

pub const fn fld(name: &'static str, label: &'static str, kind: FieldKind, expr: &'static str) -> Field {
    Field {
        name, label, kind, expr, write: Write::No, choices: &[],
        list: false, visible: true, required: false, filter: false, sort: false, search: false, nullable: false,
    }
}

impl Field {
    pub const fn col(mut self, c: &'static str) -> Self { self.write = Write::Col(c); self }
    pub const fn reference(mut self, col: &'static str, table: &'static str, by: &'static str) -> Self {
        self.write = Write::Ref { col, table, by };
        self
    }
    pub const fn list(mut self) -> Self { self.list = true; self }
    pub const fn hidden(mut self) -> Self { self.visible = false; self }
    pub const fn req(mut self) -> Self { self.required = true; self }
    pub const fn filt(mut self) -> Self { self.filter = true; self }
    pub const fn sorted(mut self) -> Self { self.sort = true; self }
    pub const fn srch(mut self) -> Self { self.search = true; self }
    pub const fn nul(mut self) -> Self { self.nullable = true; self }
    pub const fn pick(mut self, c: &'static [&'static str]) -> Self { self.choices = c; self }
    pub fn editable(&self) -> bool { !matches!(self.write, Write::No) }
}

pub struct Module {
    pub key: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub singular: &'static str,
    pub table: &'static str,
    /// FROM clause. The main table must have alias `m`.
    pub from: &'static str,
    pub fields: &'static [Field],
    pub default_sort: &'static str,
    pub can_create: bool,
    pub can_import: bool,
    pub can_export: bool,
    pub empty_text: &'static str,
    pub write_permission: &'static str,
    /// Table has an `updated_at` and `row_version` column.
    pub versioned: bool,
}

use FieldKind::*;

const STAGES: &[&str] = &["new", "to_write", "contacted", "replied", "interested", "ready_to_meet", "won", "closed_out"];
const EMAIL_STATUS: &[&str] = &["unknown", "verified", "invalid", "risky"];

static PEOPLE: &[Field] = &[
    fld("name", "Name", Text, "m.display_name").col("display_name").req().list().sorted().srch().filt(),
    fld("job_title", "Job title", Text, "m.job_title").col("job_title").list().srch().filt(),
    fld("company", "Company", Text, "co.name").reference("company_id", "companies", "name").list().sorted().srch().filt(),
    fld("emails", "Emails (one per line)", EmailList, "m.emails").col("emails").list().srch().filt(),
    fld("phone_numbers", "Phone numbers (one per line)", MultiLine, "m.phone_numbers").col("phone_numbers").list().filt().srch(),
    fld("links", "Links (one URL per line)", UrlList, "m.links").col("links").list().filt().srch(),
    fld("city", "Location", Text, "m.city").col("city").list().filt().sorted().srch(),
    fld("keywords", "Keywords (one per line)", MultiLine, "m.keywords").col("keywords").list().filt().srch(),
    fld("full_name", "Full name", Text, "m.full_name").hidden(),
    fld("first_name", "First name", Text, "m.first_name").col("first_name").hidden(),
    fld("last_name", "Last name", Text, "m.last_name").col("last_name").hidden(),
    fld("email", "Work email", Email, "m.email").col("email").nul().hidden(),
    fld("stage", "Stage", Choice, "m.stage").col("stage").pick(STAGES).hidden(),
    fld("score", "Score", Number, "m.score").hidden(),
    fld("email_status", "Email status", Choice, "m.email_status").col("email_status").pick(EMAIL_STATUS).filt().hidden(),
    fld("phone", "Phone", Text, "m.phone").col("phone").hidden(),
    fld("country", "Country", Text, "m.country").col("country").hidden(),
    fld("source", "Source", Text, "m.source").col("source").hidden(),
    fld("source_url", "Source link", Url, "m.source_url").col("source_url").hidden(),
    fld("research_note", "Research note", LongText, "m.research_note").col("research_note").hidden(),
    fld("do_not_contact", "Do not contact", Bool, "m.do_not_contact").col("do_not_contact").hidden(),
    fld("created_at", "Added", DateTime, "m.created_at").hidden(),
];

static COMPANIES: &[Field] = &[
    fld("name", "Company", Text, "m.name").col("name").req().list().sorted().srch().filt(),
    fld("emails", "Emails (one per line)", EmailList, "m.emails").col("emails").list().srch().filt(),
    fld("phone_numbers", "Phone numbers (one per line)", MultiLine, "m.phone_numbers").col("phone_numbers").list().filt().srch(),
    fld("links", "Links (website first, then one per line)", UrlList, "m.links").col("links").list().filt().srch(),
    fld("employees", "Number of employees", Integer, "m.employees").col("employees").nul().list().filt().sorted(),
    fld("industries", "Industries (one per line)", MultiLine, "m.industries").col("industries").list().filt().srch(),
    fld("keywords", "Keywords (one per line)", MultiLine, "m.keywords").col("keywords").list().filt().srch(),
    fld("city", "Location", Text, "m.city").col("city").list().filt().sorted().srch(),
    fld("website", "Website", Url, "m.website").col("website").hidden(),
    fld("industry", "Industry", Text, "m.industry").col("industry").hidden(),
    fld("size", "Size", Text, "m.size").col("size").hidden(),
    fld("phone", "Phone", Text, "m.phone").col("phone").hidden(),
    fld("people", "People", Number, "(SELECT count(*) FROM contacts c WHERE c.company_id = m.id)").hidden(),
    fld("source", "Source", Text, "m.source").col("source").hidden(),
    fld("is_competitor", "Competitor", Bool, "m.is_competitor").col("is_competitor").hidden().filt(),
    fld("notes", "Notes", LongText, "m.notes").col("notes").hidden(),
    fld("created_at", "Added", DateTime, "m.created_at").hidden(),
];

static LISTS: &[Field] = &[
    fld("name", "List", Text, "m.name").col("name").req().list().sorted().srch(),
    fld("description", "Description", LongText, "m.description").col("description").list().srch(),
    fld("members", "People", Number, "(SELECT count(*) FROM list_members lm WHERE lm.list_id = m.id)").list().sorted(),
    fld("created_at", "Created", DateTime, "m.created_at").list().sorted(),
];

static TEMPLATES: &[Field] = &[
    fld("name", "Template", Text, "m.name").col("name").req().list().sorted().srch(),
    fld("kind", "Use", Choice, "m.kind").col("kind").pick(&["email1", "email2", "email3", "reply"]).list().filt(),
    fld("subject", "Subject", Text, "m.subject").col("subject").list().srch(),
    fld("body", "Body", LongText, "m.body").col("body").srch(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["active", "archived"]).list().filt(),
    fld("updated_at", "Changed", DateTime, "m.updated_at").list().sorted(),
];

static MAILBOXES: &[Field] = &[
    fld("email", "Mailbox", Email, "m.email").col("email").req().list().sorted().srch(),
    fld("display_name", "Sender name", Text, "m.display_name").col("display_name").list(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["active", "paused", "error"]).list().filt(),
    fld("daily_limit", "Daily limit", Number, "m.daily_limit").col("daily_limit").list().sorted(),
    fld("sent_today", "Sent today", Number, "CASE WHEN m.sent_day = current_date THEN m.sent_today ELSE 0 END").list(),
    fld("last_checked", "Last inbox check", DateTime, "m.last_checked").list().sorted(),
];

static SEQUENCES: &[Field] = &[
    fld("name", "Sequence", Text, "m.name").col("name").req().list().sorted().srch(),
    fld("description", "Description", LongText, "m.description").col("description").list(),
    fld("steps", "Steps", Number, "(SELECT count(*) FROM sequence_steps s WHERE s.sequence_id = m.id)").list(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["active", "archived"]).list().filt(),
    fld("updated_at", "Changed", DateTime, "m.updated_at").list().sorted(),
];

static AUDIENCES: &[Field] = &[
    fld("name", "Audience", Text, "m.name").col("name").req().list().sorted().srch(),
    fld("min_score", "Lowest score", Number, "m.min_score").col("min_score").list().sorted(),
    fld("industry", "Industry", Text, "m.industry").col("industry").list(),
    fld("city", "City", Text, "m.city").col("city").list(),
    fld("stage", "Stage", Choice, "m.stage").col("stage").pick(STAGES).list(),
    fld("size", "People now", Number, "(SELECT count(*) FROM contacts c LEFT JOIN companies co ON co.id = c.company_id WHERE c.tenant_id = m.tenant_id AND c.score >= m.min_score AND c.stage = m.stage AND (m.industry = '' OR co.industry ILIKE m.industry) AND (m.city = '' OR c.city ILIKE m.city))").list(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["active", "archived"]).filt(),
];

static CAMPAIGNS: &[Field] = &[
    fld("name", "Campaign", Text, "m.name").col("name").req().list().sorted().srch(),
    fld("status", "Status", Choice, "m.status").pick(&["draft", "running", "paused", "done"]).list().filt().sorted(),
    fld("sequence", "Sequence", Text, "sq.name").reference("sequence_id", "sequences", "name").list().filt(),
    fld("audience", "Audience", Text, "au.name").reference("audience_id", "audiences", "name").list().filt(),
    fld("mailbox", "Mailbox", Text, "mb.email").reference("mailbox_id", "mailboxes", "email").list(),
    fld("subject_variants", "Subject variants (one per line)", LongText, "m.subject_variants").col("subject_variants"),
    fld("contacts", "People", Number, "(SELECT count(*) FROM campaign_contacts cc WHERE cc.campaign_id = m.id)").list(),
    fld("sent", "Sent", Number, "(SELECT count(*) FROM emails e WHERE e.campaign_id = m.id AND e.direction = 'out' AND e.status = 'sent')").list(),
    fld("replies", "Replies", Number, "(SELECT count(DISTINCT e.contact_id) FROM emails e JOIN campaign_contacts cc ON cc.contact_id = e.contact_id AND cc.campaign_id = m.id WHERE e.direction = 'in')").list(),
    fld("started_at", "Started", DateTime, "m.started_at").list().sorted(),
];

static EMAILS: &[Field] = &[
    fld("subject", "Subject", Text, "m.subject").list().srch(),
    fld("to_from", "To / from", Text, "CASE WHEN m.direction = 'out' THEN m.recipient ELSE m.sender END").list().srch(),
    fld("preview", "Preview", Text, "left(m.body, 240)").list(),
    fld("direction", "Direction", Choice, "m.direction").pick(&["out", "in"]).list().filt(),
    fld("status", "Status", Choice, "m.status").pick(&["queued", "blocked", "sent", "failed", "received", "waiting_approval"]).list().filt().sorted(),
    fld("blocked_reason", "Blocked because", Text, "m.blocked_reason").list(),
    fld("step_no", "Step", Number, "m.step_no").list().filt(),
    fld("body", "Message", LongText, "m.body").srch(),
    fld("written_by", "Written by", Text, "m.written_by").filt(),
    fld("is_test", "Test", Bool, "m.is_test").filt(),
    fld("read", "Read", Bool, "m.direction = 'in' AND EXISTS (SELECT 1 FROM email_user_state eus WHERE eus.tenant_id = m.tenant_id AND eus.email_id = m.id AND eus.user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)").list().hidden().filt(),
    fld("labels", "Labels", MultiLine, "COALESCE((SELECT string_agg(l.name, E'\\n' ORDER BY lower(l.name), l.name) FROM email_labels el JOIN labels l ON l.id = el.label_id AND l.tenant_id = el.tenant_id WHERE el.tenant_id = m.tenant_id AND el.email_id = m.id), '')").list().hidden(),
    fld("when", "When", DateTime, "COALESCE(m.sent_at, m.scheduled_at, m.created_at)").list().sorted(),
];

static CONVERSATIONS: &[Field] = &[
    fld("contact", "Person or group", Text, "CASE WHEN (SELECT count(*) FROM conversation_participants cp WHERE cp.tenant_id = m.tenant_id AND cp.conversation_id = m.id) > 1 THEN m.subject ELSE ct.full_name END").list().srch().sorted(),
    fld("participants", "Participants", MultiLine, "COALESCE((SELECT string_agg(c.full_name, E'\\n' ORDER BY lower(c.full_name)) FROM conversation_participants cp JOIN contacts c ON c.id = cp.contact_id AND c.tenant_id = cp.tenant_id WHERE cp.tenant_id = m.tenant_id AND cp.conversation_id = m.id), '')").list().hidden(),
    fld("group", "Group", Bool, "(SELECT count(*) FROM conversation_participants cp WHERE cp.tenant_id = m.tenant_id AND cp.conversation_id = m.id) > 1").list().hidden().filt(),
    fld("archived", "Archived", Bool, "COALESCE((SELECT cus.archived FROM conversation_user_state cus WHERE cus.tenant_id = m.tenant_id AND cus.conversation_id = m.id AND cus.user_id = NULLIF(current_setting('app.user_id', true), '')::uuid), false)").list().hidden().filt(),
    fld("unread", "Unread", Bool, "m.last_message_at > COALESCE((SELECT cus.last_read_at FROM conversation_user_state cus WHERE cus.tenant_id = m.tenant_id AND cus.conversation_id = m.id AND cus.user_id = NULLIF(current_setting('app.user_id', true), '')::uuid), 'epoch'::timestamptz)").list().hidden().filt(),
    fld("subject", "Subject", Text, "m.subject").list().srch(),
    fld("preview", "Last message", Text, "COALESCE((SELECT left(e.body, 180) FROM emails e WHERE e.conversation_id = m.id ORDER BY e.created_at DESC LIMIT 1), '')").list(),
    fld("labels", "Labels", MultiLine, "COALESCE((SELECT string_agg(l.name, E'\\n' ORDER BY lower(l.name), l.name) FROM conversation_labels cl JOIN labels l ON l.id = cl.label_id AND l.tenant_id = cl.tenant_id WHERE cl.tenant_id = m.tenant_id AND cl.conversation_id = m.id), '')").list().hidden(),
    fld("label", "Reply type", Choice, "m.label").pick(&["interested", "question", "price", "not_now", "unsubscribe", "out_of_office", "bounce", "other"]).col("label").list().filt(),
    fld("status", "Who handles it", Choice, "m.status").pick(&["open", "handover", "employee", "ai", "closed"]).list().filt().sorted(),
    fld("urgent", "Urgent", Bool, "m.urgent").col("urgent").list().filt(),
    fld("timer_due_at", "AI may reply after", DateTime, "m.timer_due_at").list().sorted(),
    fld("last_message_at", "Last message", DateTime, "m.last_message_at").list().sorted(),
];

static TASKS: &[Field] = &[
    fld("title", "Task", Text, "m.title").col("title").req().list().srch().sorted(),
    fld("kind", "Type", Choice, "m.kind").col("kind").pick(&["call", "email", "linkedin", "meeting", "other"]).list().filt(),
    fld("contact_email", "Person (work email)", Email, "ct.email").reference("contact_id", "contacts", "email").filt(),
    fld("contact", "Person", Text, "ct.full_name").list().srch(),
    fld("due_at", "Due", DateTime, "m.due_at").col("due_at").nul().list().sorted().filt(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["open", "done"]).list().filt().sorted(),
    fld("overdue", "Overdue", Bool, "(m.status = 'open' AND m.due_at < now())").filt(),
];

static FOLLOW_UPS: &[Field] = &[
    fld("note", "Follow-up", Text, "m.note").col("note").req().list().srch(),
    fld("contact_email", "Person (work email)", Email, "ct.email").reference("contact_id", "contacts", "email"),
    fld("contact", "Person", Text, "ct.full_name").list().srch(),
    fld("due_at", "Due", DateTime, "m.due_at").col("due_at").nul().list().sorted().filt(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["open", "done"]).list().filt(),
];

static DEALS: &[Field] = &[
    fld("title", "Deal", Text, "m.title").col("title").req().list().srch().sorted(),
    fld("contact_email", "Person (work email)", Email, "ct.email").reference("contact_id", "contacts", "email"),
    fld("contact", "Person", Text, "ct.full_name").list().srch(),
    fld("stage", "Stage", Choice, "m.stage").col("stage").pick(&["open", "proposal", "negotiation", "won", "lost"]).list().filt().sorted(),
    fld("value", "Value", Number, "m.value").col("value").nul().list().sorted().filt(),
    fld("currency", "Currency", Text, "m.currency").col("currency").list(),
    fld("close_date", "Close date", Date, "m.close_date").col("close_date").nul().list().sorted(),
    fld("next_step", "Next step", Text, "m.next_step").col("next_step").list(),
];

static MEETINGS: &[Field] = &[
    fld("title", "Meeting", Text, "m.title").col("title").req().list().srch(),
    fld("contact_email", "Person (work email)", Email, "ct.email").reference("contact_id", "contacts", "email"),
    fld("contact", "Person", Text, "ct.full_name").list().srch(),
    fld("starts_at", "Starts", DateTime, "m.starts_at").col("starts_at").nul().list().sorted().filt(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["planned", "held", "cancelled", "no_show"]).list().filt(),
    fld("host", "Host", Text, "m.host").col("host").list(),
    fld("link", "Meeting link", Url, "m.link").col("link"),
];

static CALLS: &[Field] = &[
    fld("contact_email", "Person (work email)", Email, "ct.email").reference("contact_id", "contacts", "email").req(),
    fld("contact", "Person", Text, "ct.full_name").list().srch(),
    fld("direction", "Direction", Choice, "m.direction").col("direction").pick(&["out", "in"]).list().filt(),
    fld("outcome", "Outcome", Choice, "m.outcome").col("outcome").pick(&["no_answer", "spoke", "voicemail", "wrong_number", "booked"]).list().filt(),
    fld("notes", "Notes", LongText, "m.notes").col("notes").list().srch(),
    fld("called_at", "When", DateTime, "m.called_at").col("called_at").list().sorted(),
];

static GOALS: &[Field] = &[
    fld("name", "Goal", Text, "m.name").col("name").req().list().srch(),
    fld("metric", "Measure", Choice, "m.metric").col("metric").pick(&["contacts", "emails", "replies", "meetings", "deals_won"]).list(),
    fld("target", "Target", Number, "m.target").col("target").req().list().sorted(),
    fld("period_start", "From", Date, "m.period_start").col("period_start").nul().list(),
    fld("period_end", "To", Date, "m.period_end").col("period_end").nul().list(),
    fld("actual", "So far", Number, "CASE m.metric \
        WHEN 'contacts' THEN (SELECT count(*) FROM contacts c WHERE c.tenant_id = m.tenant_id AND c.created_at::date BETWEEN COALESCE(m.period_start, '1970-01-01') AND COALESCE(m.period_end, '2999-01-01')) \
        WHEN 'emails' THEN (SELECT count(*) FROM emails e WHERE e.tenant_id = m.tenant_id AND e.direction = 'out' AND e.status = 'sent' AND e.is_test = false AND e.sent_at::date BETWEEN COALESCE(m.period_start, '1970-01-01') AND COALESCE(m.period_end, '2999-01-01')) \
        WHEN 'replies' THEN (SELECT count(*) FROM emails e WHERE e.tenant_id = m.tenant_id AND e.direction = 'in' AND e.is_test = false AND e.created_at::date BETWEEN COALESCE(m.period_start, '1970-01-01') AND COALESCE(m.period_end, '2999-01-01')) \
        WHEN 'meetings' THEN (SELECT count(*) FROM meetings x WHERE x.tenant_id = m.tenant_id AND x.status <> 'cancelled' AND x.starts_at::date BETWEEN COALESCE(m.period_start, '1970-01-01') AND COALESCE(m.period_end, '2999-01-01')) \
        ELSE (SELECT count(*) FROM deals d WHERE d.tenant_id = m.tenant_id AND d.stage = 'won' AND d.updated_at::date BETWEEN COALESCE(m.period_start, '1970-01-01') AND COALESCE(m.period_end, '2999-01-01')) END").list(),
];

static SOURCES: &[Field] = &[
    fld("name", "Source", Text, "m.name").col("name").req().list().srch(),
    fld("kind", "Kind", Choice, "m.kind").col("kind").pick(&["osm", "overture", "csv", "website_list"]).list().filt(),
    fld("url", "Link", Url, "m.url").col("url").list(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["open", "protected", "blocked"]).list().filt(),
    fld("paused_until", "Paused until", DateTime, "m.paused_until").list(),
    fld("notes", "Notes", LongText, "m.notes").col("notes"),
];

static RUNS: &[Field] = &[
    fld("source_name", "Source", Text, "m.source_name").list().srch(),
    fld("kind", "Kind", Text, "m.kind").list().filt(),
    fld("area", "Area", Text, "m.area").list(),
    fld("status", "Status", Choice, "m.status").pick(&["queued", "running", "done", "failed"]).list().filt().sorted(),
    fld("found", "Found", Number, "m.found").list(),
    fld("saved", "Saved", Number, "m.saved").list(),
    fld("duplicates", "Duplicates", Number, "m.duplicates").list(),
    fld("error", "Problem", Text, "m.error").list(),
    fld("created_at", "When", DateTime, "m.created_at").list().sorted(),
];

static CANDIDATES: &[Field] = &[
    fld("name", "Company", Text, "m.name").list().srch().sorted(),
    fld("website", "Website", Url, "m.website").list(),
    fld("industry", "Industry", Text, "m.industry").list().filt(),
    fld("city", "City", Text, "m.city").list().filt(),
    fld("licence", "Licence", Text, "m.licence").list(),
    fld("status", "Status", Choice, "m.status").pick(&["needs_review", "saved", "rejected"]).list().filt(),
    fld("source_url", "Source link", Url, "m.source_url"),
    fld("created_at", "Found", DateTime, "m.created_at").list().sorted(),
];

static ACTIVITY: &[Field] = &[
    fld("at", "When", DateTime, "m.at").list().sorted().filt(),
    fld("actor", "Who", Text, "m.actor").list().filt().srch(),
    fld("kind", "What", Text, "m.kind").list().filt().srch(),
    fld("summary", "Details", Text, "m.summary").list().srch(),
    fld("subject_type", "About", Text, "m.subject_type").list().filt(),
];

static AI_TASKS: &[Field] = &[
    fld("title", "AI task", Text, "m.title").list().srch(),
    fld("kind", "Kind", Text, "m.kind").list().filt(),
    fld("status", "Status", Choice, "m.status").pick(&["waiting", "claimed", "done", "failed"]).list().filt().sorted(),
    fld("result", "Result", LongText, "m.result").list(),
    fld("created_at", "Asked", DateTime, "m.created_at").list().sorted(),
];

static MCP_CALLS: &[Field] = &[
    fld("tool", "Tool", Text, "m.tool").list().filt().srch(),
    fld("ok", "Worked", Bool, "m.ok").list().filt(),
    fld("summary", "What happened", Text, "m.summary").list().srch(),
    fld("created_at", "When", DateTime, "m.created_at").list().sorted(),
];

static USERS: &[Field] = &[
    fld("name", "Name", Text, "m.name").list().srch().sorted(),
    fld("email", "Email", Email, "m.email").list().srch(),
    fld("role", "Role", Choice, "m.role").pick(&["admin", "manager", "employee", "viewer"]).list().filt(),
    fld("status", "Status", Choice, "m.status").pick(&["active", "suspended"]).list().filt(),
    fld("last_sign_in", "Last sign-in", DateTime, "m.last_sign_in").list().sorted(),
];

static WEBHOOKS: &[Field] = &[
    fld("url", "Address", Url, "m.url").col("url").req().list(),
    fld("events", "Events (one per line)", LongText, "m.events").col("events").list(),
    fld("status", "Status", Choice, "m.status").col("status").pick(&["active", "paused"]).list().filt(),
];

macro_rules! module {
    ($key:expr, $title:expr, $sub:expr, $sing:expr, $table:expr, $from:expr, $fields:expr, $sort:expr,
     create=$c:expr, import=$i:expr, export=$e:expr, empty=$empty:expr, perm=$perm:expr, versioned=$v:expr) => {
        Module {
            key: $key, title: $title, subtitle: $sub, singular: $sing, table: $table, from: $from, fields: $fields,
            default_sort: $sort, can_create: $c, can_import: $i, can_export: $e, empty_text: $empty,
            write_permission: $perm, versioned: $v,
        }
    };
}

pub static MODULES: &[Module] = &[
    module!("people", "People", "Everyone you may email. Missing values show Not available.", "contact", "contacts",
        "contacts m LEFT JOIN companies co ON co.id = m.company_id", PEOPLE, "m.created_at DESC",
        create = true, import = true, export = true, empty = "No people yet. Add one, import a CSV file, or run research.", perm = "record.write", versioned = true),
    module!("companies", "Companies", "Companies found by research or added by a person.", "company", "companies",
        "companies m", COMPANIES, "m.created_at DESC",
        create = true, import = true, export = true, empty = "No companies yet.", perm = "record.write", versioned = true),
    module!("lists", "Lists", "Saved groups of people. Saving a person only adds them to a list.", "list", "lists",
        "lists m", LISTS, "m.created_at DESC",
        create = true, import = false, export = true, empty = "No lists yet.", perm = "record.write", versioned = true),
    module!("templates", "Templates", "Email text kept for reuse. Every draft goes through the writing checks.", "template", "templates",
        "templates m", TEMPLATES, "m.updated_at DESC",
        create = true, import = false, export = true, empty = "No templates yet.", perm = "record.write", versioned = true),
    module!("mailboxes", "Mailboxes", "Gmail mailboxes that send and receive. Start at 10 emails a day, up to 30.", "mailbox", "mailboxes",
        "mailboxes m", MAILBOXES, "m.email",
        create = true, import = false, export = false, empty = "No mailbox yet.", perm = "settings.admin", versioned = true),
    module!("sequences", "Sequences", "Three emails: day 0, day 3, day 7. A reply, bounce or unsubscribe stops it.", "sequence", "sequences",
        "sequences m", SEQUENCES, "m.updated_at DESC",
        create = true, import = false, export = false, empty = "No sequences yet.", perm = "campaign.run", versioned = true),
    module!("audiences", "Audiences", "Saved rules that pick people for a campaign.", "audience", "audiences",
        "audiences m", AUDIENCES, "m.name",
        create = true, import = false, export = true, empty = "No audiences yet.", perm = "campaign.run", versioned = true),
    module!("campaigns", "Campaigns", "A sequence sent to an audience from one mailbox.", "campaign", "campaigns",
        "campaigns m LEFT JOIN sequences sq ON sq.id = m.sequence_id LEFT JOIN audiences au ON au.id = m.audience_id LEFT JOIN mailboxes mb ON mb.id = m.mailbox_id",
        CAMPAIGNS, "m.created_at DESC",
        create = true, import = false, export = true, empty = "No campaigns yet.", perm = "campaign.run", versioned = true),
    module!("emails", "Emails", "Sent, queued and blocked email. A blocked email shows the reason.", "email", "emails",
        "emails m", EMAILS, "COALESCE(m.sent_at, m.scheduled_at, m.created_at) DESC",
        create = false, import = false, export = true, empty = "No email yet.", perm = "conversation.reply", versioned = true),
    module!("conversations", "Conversations", "Replies from people. An employee answers first.", "conversation", "conversations",
        "conversations m JOIN contacts ct ON ct.id = m.contact_id", CONVERSATIONS, "m.last_message_at DESC",
        create = false, import = false, export = false, empty = "No conversations yet.", perm = "conversation.reply", versioned = true),
    module!("tasks", "Tasks", "Calls, emails and other work for people.", "task", "tasks",
        "tasks m LEFT JOIN contacts ct ON ct.id = m.contact_id", TASKS, "m.status ASC, m.due_at ASC NULLS LAST",
        create = true, import = false, export = true, empty = "No tasks.", perm = "record.write", versioned = true),
    module!("follow_ups", "Follow-ups", "People to come back to.", "follow-up", "follow_ups",
        "follow_ups m LEFT JOIN contacts ct ON ct.id = m.contact_id", FOLLOW_UPS, "m.due_at ASC NULLS LAST",
        create = true, import = false, export = false, empty = "No follow-ups.", perm = "record.write", versioned = true),
    module!("deals", "Deals", "Value, stage and next step. Values are typed by a person, never made up.", "deal", "deals",
        "deals m LEFT JOIN contacts ct ON ct.id = m.contact_id", DEALS, "m.created_at DESC",
        create = true, import = false, export = true, empty = "No deals yet.", perm = "record.write", versioned = true),
    module!("meetings", "Meetings", "Free 30-minute AI assessment calls on Google Meet.", "meeting", "meetings",
        "meetings m LEFT JOIN contacts ct ON ct.id = m.contact_id", MEETINGS, "m.starts_at DESC NULLS LAST",
        create = true, import = false, export = false, empty = "No meetings yet.", perm = "record.write", versioned = true),
    module!("calls", "Calls", "A log of calls made by hand. The app does not place or record calls.", "call", "calls",
        "calls m LEFT JOIN contacts ct ON ct.id = m.contact_id", CALLS, "m.called_at DESC",
        create = true, import = false, export = true, empty = "No calls logged.", perm = "record.write", versioned = true),
    module!("goals", "Goals", "Targets for a period, counted from real records.", "goal", "goals",
        "goals m", GOALS, "m.created_at DESC",
        create = true, import = false, export = false, empty = "No goals yet.", perm = "campaign.run", versioned = true),
    module!("sources", "Sources", "Approved public sources. A protected source pauses for 30 days.", "source", "sources",
        "sources m", SOURCES, "m.name",
        create = true, import = false, export = false, empty = "No sources yet.", perm = "settings.admin", versioned = true),
    module!("research_runs", "Research runs", "Latest collector runs from approved public sources.", "run", "collector_runs",
        "collector_runs m", RUNS, "m.created_at DESC",
        create = false, import = false, export = false, empty = "No runs yet.", perm = "campaign.run", versioned = true),
    module!("candidates", "Candidates", "Found by research. They need review before they become people.", "candidate", "research_candidates",
        "research_candidates m", CANDIDATES, "m.created_at DESC",
        create = false, import = false, export = true, empty = "No candidates.", perm = "record.write", versioned = false),
    module!("activity", "Activity", "Everything done in the app. The log is append-only.", "entry", "activity_logs",
        "activity_logs m", ACTIVITY, "m.at DESC",
        create = false, import = false, export = true, empty = "No activity yet.", perm = "settings.admin", versioned = false),
    module!("ai_tasks", "AI queue", "Tasks that wait for Claude Desktop.", "AI task", "ai_tasks",
        "ai_tasks m", AI_TASKS, "m.created_at DESC",
        create = false, import = false, export = false, empty = "Nothing waits for Claude Desktop.", perm = "ai.use", versioned = true),
    module!("mcp_calls", "Agent history", "Every tool call made through the MCP server.", "call", "mcp_tool_calls",
        "mcp_tool_calls m", MCP_CALLS, "m.created_at DESC",
        create = false, import = false, export = false, empty = "No tool calls yet.", perm = "ai.use", versioned = true),
    module!("users", "Users", "People who can sign in.", "user", "users",
        "users m", USERS, "m.name",
        create = false, import = false, export = false, empty = "No users.", perm = "user.manage", versioned = false),
    module!("webhooks", "Webhooks", "Signed calls to other systems when something happens.", "webhook", "webhooks",
        "webhooks m", WEBHOOKS, "m.created_at DESC",
        create = true, import = false, export = false, empty = "No webhooks.", perm = "settings.admin", versioned = true),
];

/// Column that holds the owner of a record. An employee sees only records they own.
pub fn owner_col(m: &Module) -> Option<&'static str> {
    match m.key {
        "people" => Some("m.owner_id"),
        "tasks" | "conversations" => Some("m.assigned_to"),
        _ => None,
    }
}

pub fn module(key: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.key == key)
}

pub fn field<'a>(m: &'a Module, name: &str) -> Option<&'a Field> {
    m.fields.iter().find(|f| f.name == name)
}

pub fn info(m: &Module) -> shared::ModuleInfo {
    shared::ModuleInfo {
        key: m.key.into(),
        title: m.title.into(),
        subtitle: m.subtitle.into(),
        singular: m.singular.into(),
        fields: m
            .fields
            .iter()
            .map(|f| shared::FieldInfo {
                name: f.name.into(),
                label: f.label.into(),
                kind: f.kind,
                choices: f.choices.iter().map(|s| s.to_string()).collect(),
                in_list: f.list,
                visible: f.visible,
                editable: f.editable(),
                required: f.required,
                filterable: f.filter || f.list,
                sortable: f.sort || f.list,
            })
            .collect(),
        can_create: m.can_create,
        can_export: m.can_export,
        can_import: m.can_import,
        empty_text: m.empty_text.into(),
        write_permission: m.write_permission.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_list;
    use shared::{Filter, FilterOp, QueryRequest};

    #[test]
    fn module_keys_and_field_names_are_unique() {
        let mut keys: Vec<_> = MODULES.iter().map(|m| m.key).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(n, keys.len());
        for m in MODULES {
            let mut names: Vec<_> = m.fields.iter().map(|f| f.name).collect();
            let n = names.len();
            names.sort();
            names.dedup();
            assert_eq!(n, names.len(), "duplicate field in {}", m.key);
            assert!(m.fields.iter().any(|f| f.list), "{} has no list column", m.key);
        }
    }

    #[test]
    fn choice_fields_have_choices() {
        for m in MODULES {
            for f in m.fields {
                if f.kind == Choice {
                    assert!(!f.choices.is_empty(), "{}.{}", m.key, f.name);
                }
            }
        }
    }

    #[test]
    fn creatable_modules_have_required_editable_field() {
        for m in MODULES.iter().filter(|m| m.can_create) {
            assert!(m.fields.iter().any(|f| f.required && f.editable()), "{}", m.key);
        }
    }

    #[test]
    fn people_and_company_screens_expose_only_the_requested_fields() {
        let visible = |key| module(key).unwrap().fields.iter().filter(|f| f.visible).map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(visible("people"), ["name", "job_title", "company", "emails", "phone_numbers", "links", "city", "keywords"]);
        assert_eq!(visible("companies"), ["name", "emails", "phone_numbers", "links", "employees", "industries", "keywords", "city"]);
    }

    #[test]
    fn email_and_chat_state_filters_use_per_user_tables() {
        let email_read = field(module("emails").unwrap(), "read").unwrap();
        assert!(email_read.filter && email_read.list && !email_read.visible);
        assert!(email_read.expr.contains("email_user_state"));

        for name in ["group", "archived", "unread"] {
            let f = field(module("conversations").unwrap(), name).unwrap();
            assert!(f.filter && f.list && !f.visible, "{name}");
        }
        assert!(field(module("conversations").unwrap(), "unread").unwrap().expr.contains("conversation_user_state"));
    }

    #[test]
    fn competitor_marker_is_internal_but_can_filter_the_competitor_view() {
        let f = field(module("companies").unwrap(), "is_competitor").unwrap();
        assert!(f.filter && !f.visible && !f.list);
        let query = QueryRequest {
            filters: vec![Filter { field: "is_competitor".into(), op: FilterOp::Eq, value: "true".into() }],
            ..Default::default()
        };
        assert!(build_list(module("companies").unwrap(), "tenant", None, &query, true).is_ok());
    }
}
