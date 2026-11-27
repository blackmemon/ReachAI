mod common;
use backend::error::Error;
use common::*;

#[tokio::test]
async fn login_session_logout_and_generic_errors() {
    let e = env().await;
    let c = company(&e, "sess").await;
    let tok = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.0.0.1").await.unwrap();
    let r = backend::accounts::resolve(&e.app, &tok).await.unwrap().expect("session");
    assert_eq!(r.actor.email, c.admin.email);
    backend::accounts::logout(&e.app, &tok).await.unwrap();
    assert!(backend::accounts::resolve(&e.app, &tok).await.unwrap().is_none());

    let wrong = backend::accounts::login(&e.app, &c.admin.email, "wrong password 123", "10.0.0.2").await.unwrap_err().body().message;
    let unknown = backend::accounts::login(&e.app, "nobody@nowhere.test", "wrong password 123", "10.0.0.3").await.unwrap_err().body().message;
    assert_eq!(wrong, unknown, "no hint whether the account exists");
}

#[tokio::test]
async fn too_many_failures_lock_the_account() {
    let e = env().await;
    let c = company(&e, "lockout").await;
    for _ in 0..5 {
        let _ = backend::accounts::login(&e.app, &c.admin.email, "bad password 12345", "10.1.1.1").await;
    }
    let r = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.1.1.2").await;
    assert!(matches!(r, Err(Error::TooMany)));
}

#[tokio::test]
async fn idle_and_hard_limits_end_the_session() {
    let e = env().await;
    let c = company(&e, "limits").await;
    let tok = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.2.2.2").await.unwrap();
    sqlx::query("UPDATE sessions SET last_seen = now() - interval '21 minutes' WHERE id_hash = $1").bind(auth::token_hash(&tok)).execute(&e.owner).await.unwrap();
    assert!(backend::accounts::resolve(&e.app, &tok).await.unwrap().is_none(), "20 minutes idle ends the session");
    let tok2 = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.2.2.3").await.unwrap();
    sqlx::query("UPDATE sessions SET hard_expires_at = now() - interval '1 minute' WHERE id_hash = $1").bind(auth::token_hash(&tok2)).execute(&e.owner).await.unwrap();
    assert!(backend::accounts::resolve(&e.app, &tok2).await.unwrap().is_none(), "24 hours is a hard limit");
}

#[tokio::test]
async fn password_change_revokes_other_sessions() {
    let e = env().await;
    let c = company(&e, "pw").await;
    let keep = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.3.3.3").await.unwrap();
    let other = backend::accounts::login(&e.app, &c.admin.email, PASSWORD, "10.3.3.4").await.unwrap();
    backend::accounts::change_password(&e.app, &c.admin, PASSWORD, "another strong pass 22", &keep).await.unwrap();
    assert!(backend::accounts::resolve(&e.app, &keep).await.unwrap().is_some());
    assert!(backend::accounts::resolve(&e.app, &other).await.unwrap().is_none());
}
