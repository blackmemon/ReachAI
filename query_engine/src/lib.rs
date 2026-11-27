//! Universal Data Query Engine. One engine for search, filters, sorting, paging, exports,
//! and validated create/update of records for every module. Modules give configuration
//! (`registry`); they never write their own query code. All SQL is parameterised: user input
//! is only ever a bind value, and field and operator names are checked against the registry.

pub mod registry;

use registry::{field, owner_col, Field, Module, Write};
use shared::{Column, Filter, FilterOp, FieldKind, QueryRequest, QueryResult, Record, Row, SaveResult};
use sqlx::{PgConnection, Row as _};
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum QueryError {
    BadRequest(String),
    /// Field name to message.
    Fields(BTreeMap<String, String>),
    NotFound,
    /// The record changed since it was loaded.
    Conflict,
    Duplicate(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for QueryError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(d) = &e {
            if d.code().as_deref() == Some("23505") {
                return QueryError::Duplicate(d.constraint().unwrap_or("duplicate").to_string());
            }
        }
        QueryError::Db(e)
    }
}

pub const MAX_PAGE_SIZE: u32 = 100;
pub const EXPORT_LIMIT: i64 = 50_000;

fn cell_expr(f: &Field) -> String {
    match f.kind {
        FieldKind::DateTime => format!(
            "COALESCE(to_char(({}) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), '')",
            f.expr
        ),
        FieldKind::Date => format!("COALESCE(to_char({}, 'YYYY-MM-DD'), '')", f.expr),
        _ => format!("COALESCE(({})::text, '')", f.expr),
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn parse_when(v: &str) -> Option<String> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
    let v = v.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(v) {
        return Some(d.with_timezone(&Utc).to_rfc3339());
    }
    if let Ok(d) = NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M") {
        return Some(Utc.from_utc_datetime(&d).to_rfc3339());
    }
    if let Ok(d) = NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?).to_rfc3339());
    }
    None
}

fn parse_date_only(v: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok().map(|d| d.to_string())
}

pub struct Built {
    pub select: String,
    pub count: String,
    pub binds: Vec<String>,
    pub count_binds: usize,
}

fn filter_clause(f: &Field, flt: &Filter, binds: &mut Vec<String>) -> Result<String, QueryError> {
    let bad = |m: &str| QueryError::BadRequest(format!("{}: {}", f.label, m));
    let text_like = matches!(
        f.kind,
        FieldKind::Text | FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList | FieldKind::Email | FieldKind::Url | FieldKind::Choice
    );
    match flt.op {
        FilterOp::IsEmpty => return Ok(format!("(({})::text IS NULL OR ({})::text = '')", f.expr, f.expr)),
        FilterOp::NotEmpty => return Ok(format!("(({})::text IS NOT NULL AND ({})::text <> '')", f.expr, f.expr)),
        _ => {}
    }
    let value = flt.value.trim();
    if matches!(f.name, "job_title" | "company" | "city" | "keywords") && value.contains('\n') {
        let values: Vec<&str> = value.lines().map(str::trim).filter(|v| !v.is_empty()).collect();
        if values.is_empty() || !matches!(flt.op, FilterOp::Eq | FilterOp::Contains) {
            return Err(bad("choose one or more values"));
        }
        let mut clauses = Vec::with_capacity(values.len());
        for value in values {
            if flt.op == FilterOp::Contains {
                binds.push(format!("%{}%", escape_like(value)));
                clauses.push(format!("(({})::text ILIKE ${})", f.expr, binds.len()));
            } else {
                binds.push(value.to_string());
                clauses.push(format!("(lower(({})::text) = lower(${}::text))", f.expr, binds.len()));
            }
        }
        return Ok(format!("({})", clauses.join(" OR ")));
    }
    match (f.kind, flt.op) {
        (k, FilterOp::Eq | FilterOp::Ne) if matches!(k, FieldKind::Bool) => {
            let b = match value.to_lowercase().as_str() {
                "true" | "yes" => "true",
                "false" | "no" => "false",
                _ => return Err(bad("use yes or no")),
            };
            binds.push(b.to_string());
            let n = binds.len();
            let op = if flt.op == FilterOp::Eq { "=" } else { "<>" };
            Ok(format!("(({}) {} ${}::boolean)", f.expr, op, n))
        }
        (k, op) if text_like && matches!(op, FilterOp::Eq | FilterOp::Ne | FilterOp::Contains) => {
            let _ = k;
            match op {
                FilterOp::Contains => {
                    binds.push(format!("%{}%", escape_like(value)));
                    Ok(format!("(({})::text ILIKE ${})", f.expr, binds.len()))
                }
                _ => {
                    binds.push(value.to_string());
                    let sym = if op == FilterOp::Eq { "=" } else { "<>" };
                    Ok(format!("(lower(({})::text) {} lower(${}::text))", f.expr, sym, binds.len()))
                }
            }
        }
        (FieldKind::Number | FieldKind::Integer, op) if matches!(op, FilterOp::Eq | FilterOp::Ne | FilterOp::Gt | FilterOp::Lt) => {
            let valid = if f.kind == FieldKind::Integer { value.parse::<i64>().is_ok() } else { value.parse::<f64>().is_ok() };
            if !valid {
                return Err(bad(if f.kind == FieldKind::Integer { "use a whole number" } else { "use a number" }));
            }
            binds.push(value.to_string());
            let sym = match op { FilterOp::Eq => "=", FilterOp::Ne => "<>", FilterOp::Gt => ">", _ => "<" };
            Ok(format!("(({})::numeric {} ${}::numeric)", f.expr, sym, binds.len()))
        }
        (FieldKind::Date | FieldKind::DateTime, op) if matches!(op, FilterOp::Eq | FilterOp::Gt | FilterOp::Lt) => {
            let v = if f.kind == FieldKind::Date {
                parse_date_only(value).ok_or_else(|| bad("use a date like 2026-12-31"))?
            } else {
                parse_when(value).ok_or_else(|| bad("use a date like 2026-12-31"))?
            };
            binds.push(v);
            let sym = match op { FilterOp::Eq => "=", FilterOp::Gt => ">", _ => "<" };
            if f.kind == FieldKind::Date {
                Ok(format!("(({}) {} ${}::date)", f.expr, sym, binds.len()))
            } else if op == FilterOp::Eq {
                Ok(format!("((({})::date) = (${}::timestamptz)::date)", f.expr, binds.len()))
            } else {
                Ok(format!("(({}) {} ${}::timestamptz)", f.expr, sym, binds.len()))
            }
        }
        _ => Err(bad("this filter is not available for this field")),
    }
}

/// Build the SQL for a list request. Pure: used by tests and by `list`.
pub fn build_list(m: &Module, tenant: &str, scope: Option<&str>, req: &QueryRequest, paged: bool) -> Result<Built, QueryError> {
    let mut binds: Vec<String> = vec![tenant.to_string()];
    let mut wheres = vec!["m.tenant_id = $1::uuid".to_string()];
    if let (Some(user), Some(col)) = (scope, owner_col(m)) {
        binds.push(user.to_string());
        wheres.push(format!("{} = ${}::uuid", col, binds.len()));
    }

    let term = req.search.trim();
    if !term.is_empty() {
        let searchable: Vec<&Field> = m.fields.iter().filter(|f| f.search).collect();
        if !searchable.is_empty() {
            binds.push(format!("%{}%", escape_like(term)));
            let n = binds.len();
            let parts: Vec<String> = searchable.iter().map(|f| format!("({})::text ILIKE ${}", f.expr, n)).collect();
            wheres.push(format!("({})", parts.join(" OR ")));
        }
    }
    if req.filters.len() > 10 {
        return Err(QueryError::BadRequest("Too many filters.".into()));
    }
    for flt in &req.filters {
        let f = field(m, &flt.field).ok_or_else(|| QueryError::BadRequest(format!("Unknown field: {}", flt.field)))?;
        if !(f.filter || f.list) {
            return Err(QueryError::BadRequest(format!("You cannot filter by {}.", f.label)));
        }
        wheres.push(filter_clause(f, flt, &mut binds)?);
    }
    let where_sql = wheres.join(" AND ");
    let count_binds = binds.len();
    let count = format!("SELECT count(*) FROM {} WHERE {}", m.from, where_sql);

    let order = match &req.sort_field {
        Some(name) => {
            let f = field(m, name).ok_or_else(|| QueryError::BadRequest(format!("Unknown field: {name}")))?;
            if !(f.sort || f.list) {
                return Err(QueryError::BadRequest(format!("You cannot sort by {}.", f.label)));
            }
            format!("{} {} NULLS LAST, m.id", f.expr, if req.sort_desc { "DESC" } else { "ASC" })
        }
        None => format!("{}, m.id", m.default_sort),
    };
    let cells: Vec<String> = m.fields.iter().filter(|f| f.list).map(cell_expr).collect();
    let mut select = format!("SELECT m.id::text, {} FROM {} WHERE {} ORDER BY {}", cells.join(", "), m.from, where_sql, order);
    if paged {
        let size = req.page_size.clamp(1, MAX_PAGE_SIZE);
        select.push_str(&format!(" LIMIT {} OFFSET {}", size, (req.page as u64) * (size as u64)));
    } else {
        select.push_str(&format!(" LIMIT {}", EXPORT_LIMIT));
    }
    Ok(Built { select, count, binds, count_binds })
}

fn display_cell(f: &Field, raw: String) -> String {
    match f.kind {
        FieldKind::Bool => match raw.as_str() {
            "true" => "Yes".into(),
            "false" => "No".into(),
            _ => "Not available".into(),
        },
        _ if raw.is_empty() => "Not available".into(),
        _ => raw,
    }
}

pub async fn list(conn: &mut PgConnection, m: &Module, tenant: &str, scope: Option<&str>, req: &QueryRequest) -> Result<QueryResult, QueryError> {
    let b = build_list(m, tenant, scope, req, true)?;
    let mut q = sqlx::query(&b.select);
    for v in &b.binds {
        q = q.bind(v);
    }
    let rows = q.fetch_all(&mut *conn).await?;
    let mut c = sqlx::query_scalar::<_, i64>(&b.count);
    for v in b.binds.iter().take(b.count_binds) {
        c = c.bind(v);
    }
    let total = c.fetch_one(&mut *conn).await?;

    let list_fields: Vec<&Field> = m.fields.iter().filter(|f| f.list).collect();
    let out = rows
        .iter()
        .map(|r| Row {
            id: r.get::<String, _>(0),
            cells: list_fields.iter().enumerate().map(|(i, f)| display_cell(f, r.get::<String, _>(i + 1))).collect(),
        })
        .collect();
    Ok(QueryResult {
        columns: list_fields
            .iter()
            .map(|f| Column { name: f.name.into(), label: f.label.into(), sortable: f.sort || f.list, kind: f.kind })
            .collect(),
        rows: out,
        total,
        page: req.page,
        page_size: req.page_size.clamp(1, MAX_PAGE_SIZE),
    })
}

pub async fn get(conn: &mut PgConnection, m: &Module, tenant: &str, scope: Option<&str>, id: &str) -> Result<(Record, i64), QueryError> {
    let uid = uuid_ok(id)?;
    let cells: Vec<String> = m.fields.iter().map(cell_expr).collect();
    let version = if m.versioned { "m.row_version" } else { "0::bigint" };
    let scope_sql = match (scope, owner_col(m)) {
        (Some(_), Some(col)) => format!(" AND {col} = $3::uuid"),
        _ => String::new(),
    };
    let sql = format!(
        "SELECT {}, {} FROM {} WHERE m.tenant_id = $1::uuid AND m.id = $2::uuid{}",
        cells.join(", "),
        version,
        m.from,
        scope_sql
    );
    let mut q = sqlx::query(&sql).bind(tenant).bind(&uid);
    if scope_sql.len() > 0 {
        q = q.bind(scope.unwrap_or_default());
    }
    let row = q.fetch_optional(&mut *conn).await?.ok_or(QueryError::NotFound)?;
    let mut values = BTreeMap::new();
    for (i, f) in m.fields.iter().enumerate() {
        values.insert(f.name.to_string(), row.get::<String, _>(i));
    }
    let rv: i64 = row.get(m.fields.len());
    Ok((Record { id: uid, values }, rv))
}

fn uuid_ok(id: &str) -> Result<String, QueryError> {
    let ok = id.len() == 36
        && id.chars().enumerate().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() });
    if ok { Ok(id.to_lowercase()) } else { Err(QueryError::NotFound) }
}

fn valid_email(v: &str) -> bool {
    if v.len() > 254 || v.contains(char::is_whitespace) {
        return false;
    }
    match v.split_once('@') {
        Some((l, d)) => !l.is_empty() && !d.contains('@') && d.contains('.') && !d.starts_with('.') && !d.ends_with('.'),
        None => false,
    }
}

fn valid_url(v: &str) -> Result<String, String> {
    let lower = v.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if v.contains(char::is_whitespace) { Err("Enter a web address without spaces.".into()) } else { Ok(v.to_string()) }
    } else if v.contains('.') && !v.contains(char::is_whitespace) && !lower.contains(':') {
        Ok(format!("https://{v}"))
    } else {
        Err("Enter a web address like https://example.com.".into())
    }
}

/// Check and normalise submitted values. Returns (field, value) pairs; `None` means NULL.
pub fn validate(m: &Module, values: &BTreeMap<String, String>, creating: bool) -> Result<Vec<(&'static Field, Option<String>)>, BTreeMap<String, String>> {
    let mut errors = BTreeMap::new();
    let mut out = Vec::new();
    for name in values.keys() {
        match field(m, name) {
            Some(f) if f.editable() => {}
            Some(f) => {
                errors.insert(name.clone(), format!("{} cannot be changed here.", f.label));
            }
            None => {
                errors.insert(name.clone(), "Unknown field.".to_string());
            }
        }
    }
    for f in m.fields.iter().filter(|f| f.editable()) {
        let raw = match values.get(f.name) {
            Some(v) => v.trim().to_string(),
            None => {
                let legacy_person_name = m.key == "people" && f.name == "name" && (values.contains_key("first_name") || values.contains_key("last_name"));
                if creating && f.required && !legacy_person_name {
                    errors.insert(f.name.into(), format!("{} is required.", f.label));
                }
                continue;
            }
        };
        if raw.is_empty() {
            if f.required {
                errors.insert(f.name.into(), format!("{} is required.", f.label));
            } else if matches!(f.kind, FieldKind::Text | FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList | FieldKind::Email | FieldKind::Url)
                && !f.nullable
                && matches!(f.write, Write::Col(_))
            {
                out.push((f, Some(String::new())));
            } else if !f.nullable && matches!(f.kind, FieldKind::Choice | FieldKind::Bool | FieldKind::Number | FieldKind::Integer | FieldKind::Date | FieldKind::DateTime) && matches!(f.write, Write::Col(_)) {
                // A blank box in a form means "leave it": a new record gets the default and an
                // existing record keeps its value. The column cannot hold NULL.
            } else {
                out.push((f, None));
            }
            continue;
        }
        let checked: Result<String, String> = match f.kind {
            FieldKind::Text => if raw.chars().count() > 500 { Err("This is too long.".into()) } else { Ok(raw) },
            FieldKind::LongText => if raw.chars().count() > 4000 { Err("This is too long.".into()) } else { Ok(raw) },
            FieldKind::MultiLine => {
                if raw.chars().count() > 4000 {
                    Err("This is too long.".into())
                } else {
                    Ok(raw.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n"))
                }
            }
            FieldKind::EmailList => {
                if raw.chars().count() > 4000 {
                    Err("This is too long.".into())
                } else {
                    let emails: Vec<String> = raw.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_lowercase).collect();
                    if emails.iter().all(|email| valid_email(email)) {
                        Ok(emails.join("\n"))
                    } else {
                        Err("Enter one valid email address per line.".into())
                    }
                }
            }
            FieldKind::UrlList => {
                if raw.chars().count() > 4000 {
                    Err("This is too long.".into())
                } else {
                    raw.lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(valid_url)
                        .collect::<Result<Vec<_>, _>>()
                        .map(|links| links.join("\n"))
                }
            }
            FieldKind::Email => if valid_email(&raw) { Ok(raw.to_lowercase()) } else { Err("Enter an email like name@company.com.".into()) },
            FieldKind::Url => valid_url(&raw),
            FieldKind::Number => raw.parse::<f64>().map(|_| raw.clone()).map_err(|_| "Enter a number.".to_string()),
            FieldKind::Integer => raw.parse::<i64>().map(|_| raw.clone()).map_err(|_| "Enter a whole number.".to_string()),
            FieldKind::Bool => match raw.as_str() {
                "true" | "false" => Ok(raw),
                _ => Err("Use yes or no.".into()),
            },
            FieldKind::Date => parse_date_only(&raw).ok_or_else(|| "Enter a date like 2026-12-31.".to_string()),
            FieldKind::DateTime => parse_when(&raw).ok_or_else(|| "Enter a date and time.".to_string()),
            FieldKind::Choice => if f.choices.contains(&raw.as_str()) { Ok(raw) } else { Err("Pick one of the listed choices.".into()) },
        };
        match checked {
            Ok(v) => out.push((f, Some(v))),
            Err(e) => {
                errors.insert(f.name.into(), e);
            }
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

fn cast(kind: FieldKind, n: usize) -> String {
    let t = match kind {
        FieldKind::Number => "numeric",
        FieldKind::Integer => "integer",
        FieldKind::Bool => "boolean",
        FieldKind::Date => "date",
        FieldKind::DateTime => "timestamptz",
        _ => "text",
    };
    format!("${n}::{t}")
}

async fn check_refs(conn: &mut PgConnection, tenant: &str, items: &[(&'static Field, Option<String>)]) -> Result<(), QueryError> {
    let mut errors = BTreeMap::new();
    for (f, v) in items {
        if let (Write::Ref { table, by, .. }, Some(v)) = (f.write, v) {
            let sql = format!("SELECT 1 FROM {table} WHERE tenant_id = $1::uuid AND lower({by}) = lower($2::text) LIMIT 1");
            let found = sqlx::query(&sql).bind(tenant).bind(v).fetch_optional(&mut *conn).await?;
            if found.is_none() {
                errors.insert(f.name.to_string(), format!("Nothing found for \"{}\". Check the spelling.", v));
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(QueryError::Fields(errors)) }
}

fn write_expr(f: &Field, n: usize) -> (String, String) {
    match f.write {
        Write::Col(c) => (c.to_string(), cast(f.kind, n)),
        Write::Ref { col, table, by } => (
            col.to_string(),
            format!("(SELECT id FROM {table} WHERE tenant_id = $1::uuid AND lower({by}) = lower(${n}::text) LIMIT 1)"),
        ),
        Write::No => unreachable!("only editable fields are written"),
    }
}

pub async fn create(conn: &mut PgConnection, m: &Module, tenant: &str, values: &BTreeMap<String, String>) -> Result<SaveResult, QueryError> {
    if !m.can_create {
        return Err(QueryError::BadRequest("This list does not take new records.".into()));
    }
    let items = validate(m, values, true).map_err(QueryError::Fields)?;
    check_refs(conn, tenant, &items).await?;
    let mut cols = vec!["tenant_id".to_string()];
    let mut exprs = vec!["$1::uuid".to_string()];
    let mut binds: Vec<Option<String>> = vec![];
    for (f, v) in &items {
        let n = binds.len() + 2;
        let (c, e) = write_expr(f, n);
        cols.push(c);
        exprs.push(e);
        binds.push(v.clone());
    }
    let ver = if m.versioned { ", row_version" } else { "" };
    let sql = format!("INSERT INTO {} ({}) VALUES ({}) RETURNING id::text{}", m.table, cols.join(", "), exprs.join(", "), ver);
    let mut q = sqlx::query(&sql).bind(tenant);
    for b in &binds {
        q = q.bind(b.clone());
    }
    let row = q.fetch_one(&mut *conn).await?;
    Ok(SaveResult { id: row.get::<String, _>(0), row_version: if m.versioned { row.get::<i64, _>(1) } else { 0 } })
}

pub async fn update(
    conn: &mut PgConnection,
    m: &Module,
    tenant: &str,
    scope: Option<&str>,
    id: &str,
    values: &BTreeMap<String, String>,
    row_version: Option<i64>,
) -> Result<SaveResult, QueryError> {
    let uid = uuid_ok(id)?;
    if !m.versioned {
        return Err(QueryError::BadRequest("This list is read only.".into()));
    }
    let rv = row_version.ok_or_else(|| QueryError::BadRequest("row_version is required to change a record.".into()))?;
    let items = validate(m, values, false).map_err(QueryError::Fields)?;
    if items.is_empty() {
        return Err(QueryError::BadRequest("Nothing to change.".into()));
    }
    check_refs(conn, tenant, &items).await?;
    let mut sets = vec![];
    let mut binds: Vec<Option<String>> = vec![];
    for (f, v) in &items {
        let n = binds.len() + 4;
        let (c, e) = write_expr(f, n);
        sets.push(format!("{c} = {e}"));
        binds.push(v.clone());
    }
    let scope_sql = match (scope, owner_col(m)) {
        (Some(_), Some(col)) => format!(" AND {} = ${}::uuid", col, binds.len() + 4),
        _ => String::new(),
    };
    let sql = format!(
        "UPDATE {} AS m SET {}, updated_at = now(), row_version = m.row_version + 1 WHERE m.id = $2::uuid AND m.tenant_id = $1::uuid AND m.row_version = $3{} RETURNING m.row_version",
        m.table,
        sets.join(", "),
        scope_sql
    );
    let mut q = sqlx::query(&sql).bind(tenant).bind(&uid).bind(rv);
    for b in &binds {
        q = q.bind(b.clone());
    }
    if scope_sql.len() > 0 {
        q = q.bind(scope.unwrap_or_default());
    }
    match q.fetch_optional(&mut *conn).await? {
        Some(r) => Ok(SaveResult { id: uid, row_version: r.get::<i64, _>(0) }),
        None => {
            // A record outside the user's scope is reported as not found, never as a conflict.
            let scope_check = match (scope, owner_col(m)) {
                (Some(_), Some(col)) => format!(" AND {} = $3::uuid", col.replace("m.", "")),
                _ => String::new(),
            };
            let ex_sql = format!("SELECT 1 FROM {} WHERE id = $1::uuid AND tenant_id = $2::uuid{}", m.table, scope_check);
            let mut ex = sqlx::query(&ex_sql)
                .bind(&uid)
                .bind(tenant);
            if scope_check.len() > 0 {
                ex = ex.bind(scope.unwrap_or_default());
            }
            let exists = ex.fetch_optional(&mut *conn).await?;
            Err(if exists.is_some() { QueryError::Conflict } else { QueryError::NotFound })
        }
    }
}

/// Spreadsheet programs run text that starts with these characters as a formula.
pub fn csv_safe(s: &str) -> String {
    match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') => format!("'{s}"),
        _ => s.to_string(),
    }
}

pub async fn export_csv(conn: &mut PgConnection, m: &Module, tenant: &str, scope: Option<&str>, req: &QueryRequest) -> Result<Vec<u8>, QueryError> {
    let b = build_list(m, tenant, scope, req, false)?;
    let mut q = sqlx::query(&b.select);
    for v in &b.binds {
        q = q.bind(v);
    }
    let rows = q.fetch_all(&mut *conn).await?;
    let list_fields: Vec<&Field> = m.fields.iter().filter(|f| f.list).collect();
    let mut w = csv::Writer::from_writer(vec![]);
    w.write_record(list_fields.iter().map(|f| f.label)).map_err(|e| QueryError::BadRequest(e.to_string()))?;
    for r in &rows {
        let rec: Vec<String> = list_fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let raw: String = r.get(i + 1);
                csv_safe(&match f.kind {
                    FieldKind::Bool => if raw == "true" { "Yes".into() } else { "No".into() },
                    _ => raw,
                })
            })
            .collect();
        w.write_record(&rec).map_err(|e| QueryError::BadRequest(e.to_string()))?;
    }
    w.into_inner().map_err(|e| QueryError::BadRequest(e.to_string()))
}

/// Template CSV built from the editable columns of a module.
pub fn csv_template(m: &Module) -> Vec<u8> {
    let mut w = csv::Writer::from_writer(vec![]);
    let _ = w.write_record(m.fields.iter().filter(|f| f.editable() && f.visible).map(|f| f.label));
    w.into_inner().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use registry::module;

    const T: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn a_form_with_blank_boxes_never_writes_null_into_a_column_that_cannot_hold_it() {
        // The browser form sends every field. Blank means "leave it", not NULL.
        let m = module("people").unwrap();
        let mut v = BTreeMap::new();
        for f in m.fields.iter().filter(|f| f.editable()) {
            v.insert(f.name.to_string(), String::new());
        }
        v.insert("name".into(), "Zainab".into());
        v.insert("first_name".into(), "Zainab".into());
        let out = validate(m, &v, true).expect("valid");
        for (f, val) in &out {
            if !f.nullable && !matches!(f.kind, FieldKind::Text | FieldKind::LongText | FieldKind::MultiLine | FieldKind::EmailList | FieldKind::UrlList | FieldKind::Email | FieldKind::Url) {
                panic!("{} would be written as {:?}", f.name, val);
            }
        }
        assert!(out.iter().any(|(f, v)| f.name == "first_name" && v.as_deref() == Some("Zainab")));
    }

    fn req() -> QueryRequest {
        QueryRequest::default()
    }

    #[test]
    fn tenant_filter_is_always_first() {
        let m = module("people").unwrap();
        let b = build_list(m, T, None, &req(), true).unwrap();
        assert!(b.select.contains("m.tenant_id = $1::uuid"));
        assert_eq!(b.binds[0], T);
    }

    #[test]
    fn user_text_is_a_bind_never_in_the_sql() {
        let m = module("people").unwrap();
        let mut r = req();
        r.search = "x'; DROP TABLE contacts; --".into();
        r.filters.push(Filter { field: "city".into(), op: FilterOp::Eq, value: "' OR 1=1 --".into() });
        let b = build_list(m, T, None, &r, true).unwrap();
        assert!(!b.select.contains("DROP TABLE"));
        assert!(!b.select.contains("OR 1=1"));
        assert!(b.binds.iter().any(|v| v.contains("DROP TABLE")));
    }

    #[test]
    fn people_text_columns_can_be_filtered() {
        let m = module("people").unwrap();
        for (field, op, value) in [("company", FilterOp::Eq, "Acme"), ("city", FilterOp::Eq, "Karachi")] {
            let mut r = req();
            r.filters.push(Filter { field: field.into(), op, value: value.into() });
            assert!(build_list(m, T, None, &r, true).is_ok(), "{field}");
        }
    }

    #[test]
    fn unknown_or_forbidden_fields_are_refused() {
        let m = module("people").unwrap();
        let mut r = req();
        r.sort_field = Some("password_hash".into());
        assert!(matches!(build_list(m, T, None, &r, true), Err(QueryError::BadRequest(_))));
        let mut r = req();
        r.filters.push(Filter { field: "research_note".into(), op: FilterOp::Eq, value: "x".into() });
        assert!(matches!(build_list(m, T, None, &r, true), Err(QueryError::BadRequest(_))));
        let mut r = req();
        r.sort_field = Some("phone".into());
        assert!(build_list(m, T, None, &r, true).is_err());
    }

    #[test]
    fn page_size_is_capped() {
        let m = module("people").unwrap();
        let mut r = req();
        r.page_size = 100_000;
        r.page = 2;
        let b = build_list(m, T, None, &r, true).unwrap();
        assert!(b.select.ends_with("LIMIT 100 OFFSET 200"));
    }

    #[test]
    fn employee_count_filter_checks_its_value() {
        let m = module("companies").unwrap();
        let mut r = req();
        r.filters.push(Filter { field: "employees".into(), op: FilterOp::Gt, value: "abc".into() });
        assert!(build_list(m, T, None, &r, true).is_err());
        r.filters[0].value = "42.5".into();
        assert!(build_list(m, T, None, &r, true).is_err());
        let mut r = req();
        r.filters.push(Filter { field: "employees".into(), op: FilterOp::Gt, value: "60".into() });
        assert!(build_list(m, T, None, &r, true).is_ok());
    }

    #[test]
    fn employee_count_requires_a_whole_number() {
        let m = module("companies").unwrap();
        let values = BTreeMap::from([("name".to_string(), "Acme".to_string()), ("employees".to_string(), "42.5".to_string())]);
        assert!(validate(m, &values, true).unwrap_err().contains_key("employees"));
        let values = BTreeMap::from([("name".to_string(), "Acme".to_string()), ("employees".to_string(), "42".to_string())]);
        assert!(validate(m, &values, true).is_ok());
    }

    #[test]
    fn multi_select_filters_match_any_selected_value() {
        let m = module("people").unwrap();
        let mut r = req();
        r.filters.push(Filter { field: "job_title".into(), op: FilterOp::Eq, value: "Founder\nCEO".into() });
        let built = build_list(m, T, None, &r, true).unwrap();
        assert!(built.select.contains("(lower((m.job_title)::text) = lower($2::text)) OR (lower((m.job_title)::text) = lower($3::text))"));
        assert_eq!(built.binds, [T, "Founder", "CEO"]);

        r.filters[0] = Filter { field: "keywords".into(), op: FilterOp::Contains, value: "analytics\nretail".into() };
        r.filters.push(Filter { field: "company".into(), op: FilterOp::Eq, value: "Acme\nNorthwind".into() });
        let built = build_list(m, T, None, &r, true).unwrap();
        assert!(built.select.contains("((m.keywords)::text ILIKE $2) OR ((m.keywords)::text ILIKE $3)"));
        assert!(built.select.contains(" OR ") && built.select.contains(" AND "));
        assert_eq!(built.binds, [T, "%analytics%", "%retail%", "Acme", "Northwind"]);
        let mut r = req();
        r.filters.push(Filter { field: "full_name".into(), op: FilterOp::Gt, value: "a".into() });
        assert!(build_list(m, T, None, &r, true).is_err());
    }

    #[test]
    fn employee_scope_adds_an_owner_filter_as_a_bind() {
        let m = module("people").unwrap();
        let u = "22222222-2222-2222-2222-222222222222";
        let b = build_list(m, T, Some(u), &req(), true).unwrap();
        assert!(b.select.contains("m.owner_id = $2::uuid"));
        assert_eq!(b.binds[1], u);
        // A module with no owner column ignores the scope.
        let c = build_list(module("templates").unwrap(), T, Some(u), &req(), true).unwrap();
        assert!(!c.select.contains("owner"));
    }

    #[test]
    fn like_wildcards_in_search_are_escaped() {
        assert_eq!(escape_like("50%_off"), "50\\%\\_off");
    }

    #[test]
    fn validation_rules() {
        let m = module("people").unwrap();
        let mut v = BTreeMap::new();
        v.insert("first_name".to_string(), "Ali".to_string());
        v.insert("email".to_string(), "ALI@Example.com ".to_string());
        v.insert("stage".to_string(), "interested".to_string());
        let ok = validate(m, &v, true).unwrap();
        assert!(ok.iter().any(|(f, val)| f.name == "email" && val.as_deref() == Some("ali@example.com")));

        let mut bad = BTreeMap::new();
        bad.insert("email".to_string(), "not-an-email".to_string());
        bad.insert("stage".to_string(), "nope".to_string());
        bad.insert("score".to_string(), "99".to_string());
        bad.insert("ghost".to_string(), "1".to_string());
        let e = validate(m, &bad, true).unwrap_err();
        assert!(e.contains_key("name"));
        assert!(e.contains_key("email"));
        assert!(e.contains_key("stage"));
        assert!(e.contains_key("score"), "computed fields cannot be set");
        assert!(e.contains_key("ghost"));
    }

    #[test]
    fn empty_email_becomes_null_and_url_gets_scheme() {
        let m = module("people").unwrap();
        let mut v = BTreeMap::new();
        v.insert("first_name".to_string(), "Ali".to_string());
        v.insert("email".to_string(), "".to_string());
        v.insert("source_url".to_string(), "example.com/page".to_string());
        let ok = validate(m, &v, true).unwrap();
        assert!(ok.iter().any(|(f, val)| f.name == "email" && val.is_none()));
        assert!(ok.iter().any(|(f, val)| f.name == "source_url" && val.as_deref() == Some("https://example.com/page")));
    }

    #[test]
    fn multi_value_fields_normalize_each_line_and_reject_invalid_entries() {
        let m = module("people").unwrap();
        let values = BTreeMap::from([
            ("name".to_string(), "Ali Khan".to_string()),
            ("emails".to_string(), " Ali@Example.com \n\nali2@example.com ".to_string()),
            ("links".to_string(), "example.com\nhttps://linkedin.com/in/ali".to_string()),
        ]);
        let ok = validate(m, &values, true).unwrap();
        assert!(ok.iter().any(|(f, v)| f.name == "emails" && v.as_deref() == Some("ali@example.com\nali2@example.com")));
        assert!(ok.iter().any(|(f, v)| f.name == "links" && v.as_deref() == Some("https://example.com\nhttps://linkedin.com/in/ali")));

        let invalid = BTreeMap::from([
            ("name".to_string(), "Ali Khan".to_string()),
            ("emails".to_string(), "ali@example.com\nnot-an-email".to_string()),
        ]);
        assert!(validate(m, &invalid, true).unwrap_err().contains_key("emails"));
    }

    #[test]
    fn dates_are_normalised_to_utc() {
        assert_eq!(parse_when("2026-12-01").unwrap(), "2026-12-01T00:00:00+00:00");
        assert_eq!(parse_when("2026-12-01 14:30").unwrap(), "2026-12-01T14:30:00+00:00");
        assert_eq!(parse_when("2026-12-01T19:30:00+05:00").unwrap(), "2026-12-01T14:30:00+00:00");
        assert!(parse_when("tomorrow").is_none());
    }

    #[test]
    fn csv_cells_are_formula_safe() {
        assert_eq!(csv_safe("=1+1"), "'=1+1");
        assert_eq!(csv_safe("@cmd"), "'@cmd");
        assert_eq!(csv_safe("-5"), "'-5");
        assert_eq!(csv_safe("Ali"), "Ali");
    }

    #[test]
    fn template_has_editable_columns_only() {
        let t = String::from_utf8(csv_template(module("people").unwrap())).unwrap();
        assert!(t.starts_with("Name,Job title,Company,Emails (one per line),"));
        assert!(!t.contains("First name"));
        assert!(!t.contains("Score"));
    }

    #[test]
    fn read_only_modules_cannot_be_written() {
        let m = module("activity").unwrap();
        assert!(!m.can_create);
        assert!(m.fields.iter().all(|f| !f.editable()));
    }
}
