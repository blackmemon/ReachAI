//! blackmemon ReachAI gateway: the only HTTP entry point. Built on hyper, no web framework.
//! Commands: `serve` (default), `migrate`, `create-tenant`, `seed`.

mod api;
mod cli;
mod http;
mod server;
mod statics;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("serve") => server::run().await,
        Some(cmd) => cli::run(cmd, &args[1..]).await,
    };
    std::process::exit(code);
}
