//! Shared application context: the pool, config, the mail transport and the domain checker.
//! Outside services are enums with a test double, so tests never touch the network.

use auth::Actor;
use configs::Config;
use database::{Pool, Tx};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingMail {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub message_id: String,
    pub list_unsubscribe: String,
}

/// Mail transport. `Recording` is the test double: it keeps mail in memory and sends nothing.
/// `Live` (real Gmail SMTP) is not built yet and fails with a clear message.
#[derive(Clone)]
pub enum Transport {
    Recording(Arc<Mutex<Vec<OutgoingMail>>>),
    Live,
}

impl Transport {
    pub fn recording() -> Self {
        Transport::Recording(Arc::new(Mutex::new(vec![])))
    }

    pub async fn send(&self, mail: &OutgoingMail) -> Result<(), String> {
        match self {
            Transport::Recording(v) => {
                v.lock()
                    .map_err(|_| "mail log is locked".to_string())?
                    .push(mail.clone());
                Ok(())
            }
            Transport::Live => {
                Err("Live Gmail SMTP sending is not built yet. Use the recording transport.".into())
            }
        }
    }

    pub fn sent(&self) -> Vec<OutgoingMail> {
        match self {
            Transport::Recording(v) => v.lock().map(|g| g.clone()).unwrap_or_default(),
            Transport::Live => vec![],
        }
    }
}

/// Checks that an email domain exists. `Fake` lists the domains that resolve (for tests).
#[derive(Clone)]
pub enum DomainCheck {
    Dns,
    Fake(Vec<String>),
}

impl DomainCheck {
    pub async fn resolves(&self, domain: &str) -> bool {
        match self {
            DomainCheck::Fake(list) => list.iter().any(|d| d.eq_ignore_ascii_case(domain)),
            DomainCheck::Dns => tokio::net::lookup_host((domain, 25))
                .await
                .map(|mut i| i.next().is_some())
                .unwrap_or(false),
        }
    }
}

#[derive(Clone)]
pub struct App {
    pub pool: Pool,
    pub cfg: Config,
    pub transport: Transport,
    pub domains: DomainCheck,
}

impl App {
    pub fn new(pool: Pool, cfg: Config) -> Self {
        let transport = if cfg.smtp_live {
            Transport::Live
        } else {
            Transport::recording()
        };
        Self {
            pool,
            cfg,
            transport,
            domains: DomainCheck::Dns,
        }
    }

    /// A transaction for the actor's tenant. The tenant comes from the session only.
    pub async fn tx(&self, actor: &Actor) -> crate::error::Result<Tx> {
        let mut tx = database::scoped(&self.pool, &actor.tenant_id).await?;
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(&actor.user_id)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    pub async fn tx_for_tenant(&self, tenant_id: &str) -> crate::error::Result<Tx> {
        Ok(database::scoped(&self.pool, tenant_id).await?)
    }
}

pub fn need(actor: &Actor, permission: &str) -> crate::error::Result<()> {
    if actor.can(permission) {
        Ok(())
    } else {
        Err(crate::error::Error::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_transport_keeps_mail_and_live_refuses() {
        let t = Transport::recording();
        let m = OutgoingMail {
            from: "a@x.com".into(),
            to: "b@y.com".into(),
            subject: "s".into(),
            body: "b".into(),
            message_id: "1".into(),
            list_unsubscribe: "".into(),
        };
        t.send(&m).await.unwrap();
        assert_eq!(t.sent().len(), 1);
        assert!(Transport::Live.send(&m).await.is_err());
    }

    #[tokio::test]
    async fn fake_domain_check() {
        let d = DomainCheck::Fake(vec!["good.pk".into()]);
        assert!(d.resolves("GOOD.pk").await);
        assert!(!d.resolves("bad.pk").await);
    }
}
