//! Business rules for blackmemon ReachAI. No HTTP here: the gateway calls these services.

pub mod accounts;
pub mod activity;
pub mod ai;
pub mod app;
pub mod assignment;
pub mod campaigns;
pub mod compliance;
pub mod conversations;
pub mod error;
pub mod imports;
pub mod jobs;
pub mod labels;
pub mod notices;
pub mod outbox;
pub mod quick;
pub mod records;
pub mod reports;
pub mod research;
pub mod scoring;
pub mod seed;
pub mod settings;

/// Order of stages. A reply moves a person forward, never back.
pub fn stage_rank(stage: &str) -> usize {
    shared::STAGES.iter().position(|(k, _)| *k == stage).unwrap_or(0)
}
