use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cfg = match configs::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration problem: {e}");
            std::process::exit(2);
        }
    };
    // The worker reads all companies, so it may use its own database role.
    let url =
        std::env::var("LEADS_WORKER_DATABASE_URL").unwrap_or_else(|_| cfg.database_url.clone());
    let pool = match database::connect(&url, 5).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot reach the database: {e}");
            std::process::exit(2);
        }
    };
    let app = backend::app::App::new(pool, cfg);
    tracing::info!("worker started");
    let mut stop = Box::pin(tokio::signal::ctrl_c());
    loop {
        let r = worker::tick::tick(&app).await;
        if r.planned + r.sent + r.timers + r.errors > 0 {
            tracing::info!(?r, "tick");
        }
        tokio::select! {
            _ = &mut stop => break,
            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
        }
    }
}
