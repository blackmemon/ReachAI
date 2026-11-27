//! Background work: plan emails, send them one by one inside the limits, run reply timers,
//! warn when Claude Desktop is late, and clean old login attempts.
//! Every step is safe to run twice, so two workers can run side by side.

pub mod tick;
