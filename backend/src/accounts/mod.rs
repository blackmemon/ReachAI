//! Sign-in, sessions, password change, user and company creation.

use crate::activity;
use crate::app::{need, App};
use crate::error::{Error, Result};
use auth::Actor;
use shared::SessionInfo;
use sqlx::Row;

const MAX_FAILS_PER_ACCOUNT: i64 = 5;
const MAX_FAILS_PER_IP: i64 = 30;
const LOCK_MINUTES: i64 = 15;

pub struct Resolved {
    pub actor: Actor,
    pub csrf: String,
    pub hard_expires_in: i64,
    pub idle_expires_in: i64,
    pub tenant_name: String,
}

async fn recent_fails(app: &App, key: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM login_attempts WHERE key = $1 AND ok = false AND at > now() - make_interval(mins => $2::int)",
    )
    .bind(key)
    .bind(LOCK_MINUTES as i32)
    .fetch_one(&app.pool)
    .await?;
    Ok(n)
}

async fn record(app: &App, key: &str, ok: bool) -> Result<()> {
    sqlx::query("INSERT INTO login_attempts (key, ok) VALUES ($1, $2)")
        .bind(key)
        .bind(ok)
        .execute(&app.pool)
        .await?;
    Ok(())
}

/// Returns the session token. The error for every failure is the same text.
pub async fn login(app: &App, email: &str, password: &str, ip: &str) -> Result<String> {
    let email = email.trim().to_lowercase();
    let acct_key = format!("acct:{email}");
    let ip_key = format!("ip:{ip}");
    if recent_fails(app, &acct_key).await? >= MAX_FAILS_PER_ACCOUNT
        || recent_fails(app, &ip_key).await? >= MAX_FAILS_PER_IP
    {
        return Err(Error::TooMany);
    }
    let row = sqlx::query(
        "SELECT u.id::text, u.tenant_id::text, c.password_hash, u.status, t.status FROM users u \
         JOIN user_credentials c ON c.user_id = u.id JOIN tenants t ON t.id = u.tenant_id WHERE lower(u.email) = $1",
    )
    .bind(&email)
    .fetch_optional(&app.pool)
    .await?;
    let fail = || Error::BadRequest("Email or password is incorrect.".into());
    let Some(row) = row else {
        auth::dummy_verify(password);
        record(app, &acct_key, false).await?;
        record(app, &ip_key, false).await?;
        return Err(fail());
    };
    let (uid, tid, hash): (String, String, String) = (row.get(0), row.get(1), row.get(2));
    let active = row.get::<String, _>(3) == "active" && row.get::<String, _>(4) == "active";
    if !auth::verify_password(&hash, password) || !active {
        record(app, &acct_key, false).await?;
        record(app, &ip_key, false).await?;
        return Err(fail());
    }
    record(app, &acct_key, true).await?;
    let token = auth::new_token();
    sqlx::query(
        "INSERT INTO sessions (id_hash, user_id, tenant_id, csrf, hard_expires_at) \
         VALUES ($1, $2::uuid, $3::uuid, $4, now() + make_interval(secs => $5::int))",
    )
    .bind(auth::token_hash(&token))
    .bind(&uid)
    .bind(&tid)
    .bind(auth::new_token())
    .bind(app.cfg.session_hard_seconds as i32)
    .execute(&app.pool)
    .await?;
    sqlx::query("UPDATE users SET last_sign_in = now() WHERE id = $1::uuid")
        .bind(&uid)
        .execute(&app.pool)
        .await?;
    let mut tx = app.tx_for_tenant(&tid).await?;
    activity::log(
        &mut tx,
        &tid,
        &email,
        "auth.sign_in",
        "user",
        &uid,
        "Signed in",
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(token)
}

pub async fn resolve(app: &App, token: &str) -> Result<Option<Resolved>> {
    let row = sqlx::query(
        "SELECT s.user_id::text, s.tenant_id::text, s.csrf, u.email, u.name, u.role, t.name, \
                GREATEST(extract(epoch FROM s.hard_expires_at - now()), 0)::bigint, \
                extract(epoch FROM now() - s.last_seen)::bigint \
         FROM sessions s JOIN users u ON u.id = s.user_id JOIN tenants t ON t.id = s.tenant_id \
         WHERE s.id_hash = $1 AND s.revoked_at IS NULL AND s.hard_expires_at > now() \
           AND u.status = 'active' AND t.status = 'active'",
    )
    .bind(auth::token_hash(token))
    .fetch_optional(&app.pool)
    .await?;
    let Some(r) = row else { return Ok(None) };
    let idle_used: i64 = r.get(8);
    if idle_used > app.cfg.session_idle_seconds {
        // Idle too long: the session is ended on the server.
        sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id_hash = $1")
            .bind(auth::token_hash(token))
            .execute(&app.pool)
            .await?;
        return Ok(None);
    }
    if idle_used > 20 {
        sqlx::query("UPDATE sessions SET last_seen = now() WHERE id_hash = $1")
            .bind(auth::token_hash(token))
            .execute(&app.pool)
            .await?;
    }
    Ok(Some(Resolved {
        actor: Actor {
            user_id: r.get(0),
            tenant_id: r.get(1),
            email: r.get(3),
            name: r.get(4),
            role: r.get(5),
        },
        csrf: r.get(2),
        hard_expires_in: r.get(7),
        idle_expires_in: app.cfg.session_idle_seconds
            - idle_used.max(0).min(app.cfg.session_idle_seconds),
        tenant_name: r.get(6),
    }))
}

pub async fn logout(app: &App, token: &str) -> Result<()> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id_hash = $1")
        .bind(auth::token_hash(token))
        .execute(&app.pool)
        .await?;
    Ok(())
}

pub async fn session_info(app: &App, r: &Resolved) -> Result<SessionInfo> {
    let mut tx = app.tx(&r.actor).await?;
    let settings = crate::settings::all(&mut tx, &r.actor.tenant_id).await?;
    let unread: i64 = sqlx::query_scalar("SELECT count(*) FROM notices WHERE tenant_id = $1::uuid AND read_at IS NULL AND (user_id IS NULL OR user_id = $2::uuid)")
        .bind(&r.actor.tenant_id)
        .bind(&r.actor.user_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(SessionInfo {
        user_id: r.actor.user_id.clone(),
        name: r.actor.name.clone(),
        email: r.actor.email.clone(),
        role: r.actor.role.clone(),
        tenant_name: r.tenant_name.clone(),
        permissions: shared::permissions::for_role(&r.actor.role)
            .into_iter()
            .map(String::from)
            .collect(),
        csrf: r.csrf.clone(),
        hard_expires_in: r.hard_expires_in,
        idle_expires_in: r.idle_expires_in,
        sending_paused: crate::settings::flag(&settings, "sending_paused"),
        unread_notices: unread,
    })
}

pub async fn change_password(
    app: &App,
    actor: &Actor,
    current: &str,
    new: &str,
    keep_token: &str,
) -> Result<()> {
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM user_credentials WHERE user_id = $1::uuid")
            .bind(&actor.user_id)
            .fetch_one(&app.pool)
            .await?;
    if !auth::verify_password(&hash, current) {
        return Err(Error::BadRequest(
            "Your current password is not correct.".into(),
        ));
    }
    if let Some(p) = auth::password_problem(new) {
        return Err(Error::BadRequest(p.into()));
    }
    let h = auth::hash_password(new).map_err(Error::Internal)?;
    sqlx::query("UPDATE user_credentials SET password_hash = $2, changed_at = now() WHERE user_id = $1::uuid").bind(&actor.user_id).bind(&h).execute(&app.pool).await?;
    // Every other session of this user ends at once.
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1::uuid AND id_hash <> $2 AND revoked_at IS NULL")
        .bind(&actor.user_id)
        .bind(auth::token_hash(keep_token))
        .execute(&app.pool)
        .await?;
    let mut tx = app.tx(actor).await?;
    activity::log(
        &mut tx,
        &actor.tenant_id,
        &actor.email,
        "auth.password_changed",
        "user",
        &actor.user_id,
        "Password changed",
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn create_user(
    app: &App,
    actor: &Actor,
    email: &str,
    name: &str,
    role: &str,
    password: &str,
) -> Result<String> {
    need(actor, "user.manage")?;
    if !shared::permissions::ROLES.contains(&role) {
        return Err(Error::BadRequest("Pick a role from the list.".into()));
    }
    if let Some(p) = auth::password_problem(password) {
        return Err(Error::BadRequest(p.into()));
    }
    let h = auth::hash_password(password).map_err(Error::Internal)?;
    let email = email.trim().to_lowercase();
    if !email.contains('@') || name.trim().is_empty() {
        return Err(Error::BadRequest("Enter a name and a valid email.".into()));
    }
    let id: String = sqlx::query_scalar("INSERT INTO users (tenant_id, email, name, role) VALUES ($1::uuid, $2, $3, $4) RETURNING id::text")
        .bind(&actor.tenant_id)
        .bind(&email)
        .bind(name.trim())
        .bind(role)
        .fetch_one(&app.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => Error::Conflict("A user with this email already exists.".into()),
            _ => Error::from(e),
        })?;
    sqlx::query("INSERT INTO user_credentials (user_id, password_hash) VALUES ($1::uuid, $2)")
        .bind(&id)
        .bind(&h)
        .execute(&app.pool)
        .await?;
    let mut tx = app.tx(actor).await?;
    activity::log(
        &mut tx,
        &actor.tenant_id,
        &actor.email,
        "user.created",
        "user",
        &id,
        &format!("User {email} added as {role}"),
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Creates a company and its first user. The first user of a company is its admin.
pub async fn create_tenant(
    owner_pool: &database::Pool,
    company: &str,
    admin_email: &str,
    admin_name: &str,
    password: &str,
) -> Result<(String, String)> {
    if let Some(p) = auth::password_problem(password) {
        return Err(Error::BadRequest(p.into()));
    }
    let h = auth::hash_password(password).map_err(Error::Internal)?;
    let mut tx = owner_pool.begin().await?;
    let tid: String =
        sqlx::query_scalar("INSERT INTO tenants (name) VALUES ($1) RETURNING id::text")
            .bind(company)
            .fetch_one(&mut *tx)
            .await?;
    let uid: String = sqlx::query_scalar("INSERT INTO users (tenant_id, email, name, role) VALUES ($1::uuid, $2, $3, 'admin') RETURNING id::text")
        .bind(&tid)
        .bind(admin_email.trim().to_lowercase())
        .bind(admin_name)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO user_credentials (user_id, password_hash) VALUES ($1::uuid, $2)")
        .bind(&uid)
        .bind(&h)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((tid, uid))
}
