//! Settings read from the environment. Secrets never live in source code.

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub listen: String,
    /// Key for pgcrypto (mailbox passwords) and for signing tokens.
    pub secret_key: String,
    /// Secure cookie flag. Always true in production; false only for plain-http local runs.
    pub cookie_secure: bool,
    /// Public address. The unsubscribe link in every email starts with this.
    pub base_url: String,
    pub dist_dir: String,
    pub session_hard_seconds: i64,
    pub session_idle_seconds: i64,
    /// Live sending. Off by default: the test double records mail and sends nothing.
    pub smtp_live: bool,
}

fn get(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let secret_key = get("LEADS_SECRET_KEY", "");
        if secret_key.len() < 16 {
            return Err("LEADS_SECRET_KEY must be set and at least 16 characters long".into());
        }
        Ok(Self {
            database_url: get("DATABASE_URL", "postgres://leads_app:leads_app@127.0.0.1:5433/leads_dev"),
            listen: get("LEADS_LISTEN", "127.0.0.1:8000"),
            secret_key,
            cookie_secure: get("LEADS_COOKIE_SECURE", "true") != "false",
            base_url: get("LEADS_BASE_URL", "http://127.0.0.1:8000"),
            dist_dir: get("LEADS_DIST", "dist"),
            // Fixed default: a login ends 24 hours after it began.
            session_hard_seconds: 24 * 3600,
            // 15 to 20 minutes allowed by the standard; 20 chosen.
            session_idle_seconds: 20 * 60,
            smtp_live: get("LEADS_SMTP_LIVE", "false") == "true",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_secret_is_refused() {
        env::set_var("LEADS_SECRET_KEY", "short");
        assert!(Config::from_env().is_err());
    }
}
