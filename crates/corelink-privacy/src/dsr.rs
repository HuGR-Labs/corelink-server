//! DSR rights orchestrator — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-dsr` at the module
//! root, and exposes the DSR Statuspage 24h scheduler at
//! `dsr::statuspage`. The actual implementations live in
//! `crates/corelink-dsr/` and
//! `crates/corelink-dsr-statuspage-scheduler/` (Stage 1 Stream B
//! sub-step B.5 Option-A aggregator pattern).

pub use corelink_dsr::*;

/// DSR 24h-rolling Statuspage publish scheduler (06:00 UTC daily
/// cron). Composes `corelink-privacy-erasure-worker::aggregate_24h_window`
/// + `corelink-statuspage-real::bridge_to_report` +
///   `StatuspageBackend::publish_dsr_metric` into a deterministic
///   orchestration. Idempotency dedupe via `(date, metric_id)`.
pub mod statuspage {
    pub use corelink_dsr_statuspage_scheduler::*;
}
