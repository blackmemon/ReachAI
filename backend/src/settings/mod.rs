//! Business rules kept as settings. Every setting has a default and a Reset.

use crate::activity;
use crate::app::need;
use crate::error::{Error, Result};
use auth::Actor;
use shared::settings_defs::{self, SettingDef, GROUPS, SETTINGS};
use shared::{SettingValue, SettingsPage};
use sqlx::{PgConnection, Row};
use std::collections::{BTreeMap, HashMap};

pub async fn all(conn: &mut PgConnection, tenant: &str) -> Result<HashMap<String, String>> {
    let mut m: HashMap<String, String> = SETTINGS.iter().map(|s| (s.key.to_string(), s.default.to_string())).collect();
    let rows = sqlx::query("SELECT key, value FROM settings WHERE tenant_id = $1::uuid").bind(tenant).fetch_all(&mut *conn).await?;
    for r in rows {
        m.insert(r.get::<String, _>(0), r.get::<String, _>(1));
    }
    Ok(m)
}

pub fn int(m: &HashMap<String, String>, key: &str) -> i64 {
    m.get(key).and_then(|v| v.trim().parse().ok()).or_else(|| settings_defs::find(key).and_then(|d| d.default.parse().ok())).unwrap_or(0)
}

pub fn flag(m: &HashMap<String, String>, key: &str) -> bool {
    m.get(key).map(|v| v == "true").unwrap_or_else(|| settings_defs::find(key).map(|d| d.default == "true").unwrap_or(false))
}

pub fn text<'a>(m: &'a HashMap<String, String>, key: &str) -> &'a str {
    m.get(key).map(|s| s.as_str()).unwrap_or("")
}

pub async fn set_raw(conn: &mut PgConnection, tenant: &str, user: Option<&str>, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (tenant_id, key, value, updated_by, updated_at) VALUES ($1::uuid, $2, $3, $4::uuid, now()) \
         ON CONFLICT (tenant_id, key) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(tenant)
    .bind(key)
    .bind(value)
    .bind(user)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn group_title(group: &str) -> Option<&'static str> {
    GROUPS.iter().find(|(g, _)| *g == group).map(|(_, t)| *t)
}

pub async fn page(conn: &mut PgConnection, actor: &Actor, group: &str) -> Result<SettingsPage> {
    need(actor, "record.read")?;
    let title = group_title(group).ok_or(Error::NotFound)?;
    let m = all(conn, &actor.tenant_id).await?;
    let items = SETTINGS
        .iter()
        .filter(|s| s.group == group)
        .map(|s| SettingValue {
            key: s.key.into(),
            label: s.label.into(),
            help: s.help.into(),
            kind: s.kind.into(),
            default: s.default.into(),
            value: m.get(s.key).cloned().unwrap_or_else(|| s.default.into()),
            choices: settings_defs::choices_for(s),
            editable: can_edit(actor, s),
        })
        .collect();
    Ok(SettingsPage { group: group.into(), title: title.into(), items })
}

fn can_edit(actor: &Actor, s: &SettingDef) -> bool {
    if s.admin_only { actor.can("settings.admin") } else { actor.can("settings.business") }
}

pub async fn save(conn: &mut PgConnection, actor: &Actor, group: &str, values: &BTreeMap<String, String>) -> Result<()> {
    group_title(group).ok_or(Error::NotFound)?;
    let current = all(conn, &actor.tenant_id).await?;
    let mut errors = BTreeMap::new();
    let mut changes = vec![];
    for (k, v) in values {
        let Some(def) = settings_defs::find(k).filter(|d| d.group == group) else {
            errors.insert(k.clone(), "Unknown setting.".to_string());
            continue;
        };
        if !can_edit(actor, def) {
            return Err(Error::Forbidden);
        }
        let v = v.trim().to_string();
        if let Err(e) = settings_defs::validate(def, &v) {
            errors.insert(k.clone(), e);
            continue;
        }
        if current.get(k.as_str()) != Some(&v) {
            changes.push((def, v));
        }
    }
    if !errors.is_empty() {
        return Err(Error::Fields(errors));
    }
    for (def, v) in changes {
        let old = current.get(def.key).cloned().unwrap_or_default();
        set_raw(conn, &actor.tenant_id, Some(&actor.user_id), def.key, &v).await?;
        // The old and the new value are written to the activity log.
        activity::log(
            conn, &actor.tenant_id, &actor.email, "settings.changed", "setting", def.key,
            &format!("{} changed", def.label), serde_json::json!({"old": old, "new": v}),
        )
        .await?;
    }
    Ok(())
}

pub async fn reset(conn: &mut PgConnection, actor: &Actor, group: &str) -> Result<()> {
    group_title(group).ok_or(Error::NotFound)?;
    for def in SETTINGS.iter().filter(|s| s.group == group) {
        if can_edit(actor, def) {
            sqlx::query("DELETE FROM settings WHERE tenant_id = $1::uuid AND key = $2").bind(&actor.tenant_id).bind(def.key).execute(&mut *conn).await?;
        }
    }
    activity::log(conn, &actor.tenant_id, &actor.email, "settings.reset", "setting", group, &format!("{group} settings reset to defaults"), serde_json::json!({})).await?;
    Ok(())
}
