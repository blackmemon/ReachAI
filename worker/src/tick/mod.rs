use backend::app::App;
use sqlx::Row;

#[derive(Debug, Default, PartialEq)]
pub struct TickReport {
    pub tenants: usize,
    pub planned: usize,
    pub sent: usize,
    pub timers: usize,
    pub errors: usize,
}

/// One pass over every active company. Errors for one company never stop the others.
pub async fn tick(app: &App) -> TickReport {
    let mut rep = TickReport::default();
    let tenants: Vec<String> =
        match sqlx::query("SELECT id::text FROM tenants WHERE status = 'active'")
            .fetch_all(&app.pool)
            .await
        {
            Ok(r) => r.iter().map(|x| x.get(0)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "cannot list companies");
                rep.errors += 1;
                return rep;
            }
        };
    for t in &tenants {
        rep.tenants += 1;
        match backend::campaigns::plan_due(app, t).await {
            Ok(n) => rep.planned += n,
            Err(e) => {
                tracing::error!(tenant = %t, error = ?e.body().message, "planner failed");
                rep.errors += 1;
            }
        }
        // Send until nothing is due. The mailbox limit and send window stop the loop by themselves.
        for _ in 0..50 {
            match backend::outbox::send_next(app, t).await {
                Ok(true) => rep.sent += 1,
                Ok(false) => break,
                Err(e) => {
                    tracing::error!(tenant = %t, error = ?e.body().message, "sender failed");
                    rep.errors += 1;
                    break;
                }
            }
        }
        match backend::conversations::timers_due(app, t).await {
            Ok(n) => rep.timers += n,
            Err(_) => rep.errors += 1,
        }
        if ai_late_notice(app, t).await.is_err() {
            rep.errors += 1;
        }
    }
    let _ = sqlx::query("DELETE FROM login_attempts WHERE at < now() - interval '7 days'")
        .execute(&app.pool)
        .await;
    rep
}

/// Tell the team once an hour when work waits for Claude Desktop and it has not checked in time.
async fn ai_late_notice(app: &App, tenant: &str) -> backend::error::Result<()> {
    let mut tx = app.tx_for_tenant(tenant).await?;
    let st = backend::ai::status(&mut tx, tenant).await?;
    if st.waiting > 0 && st.state != "on_time" {
        let recent: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM notices WHERE tenant_id = $1::uuid AND kind = 'ai_late' AND created_at > now() - interval '1 hour')").bind(tenant).fetch_one(&mut *tx).await?;
        if !recent {
            let body = format!(
                "{} task(s) wait for Claude Desktop. It has not checked the queue on time.",
                st.waiting
            );
            backend::notices::push(
                &mut tx,
                tenant,
                None,
                "ai_late",
                "Claude Desktop is late",
                &body,
                "quick",
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
