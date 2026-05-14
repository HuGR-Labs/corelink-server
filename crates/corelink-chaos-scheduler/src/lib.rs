//! CoreLink chaos scheduler (WI-S17-001).
//!
//! Foundation crate for the S-17 Ops Maturity sprint. Provides:
//!
//! - **8 canonical chaos experiments** (latency / failure / resource
//!   exhaustion / network partition) — see [`catalog::canonical_catalog`].
//! - **Deterministic-seed reproducibility** — same `run_id` × `experiment_id`
//!   produces bit-identical [`types::ChaosRun`] (FNV-1a 64-bit; no host /
//!   arch / clock dependency). See [`runner::derive_seed`].
//! - **Safe-mode auto-abort** — HARD env check at code level
//!   (`CHAOS_TARGET == "staging"`); prod SEV-1 guard; staging error-rate
//!   guard (> 50%). See [`runner::SafeModeSnapshot`].
//! - **Audit event lifecycle** — `corelink.chaos.run.started` / `.completed`
//!   / `.aborted` (see [`types::ChaosAuditEvent`]).
//! - **Scheduler trait** — pluggable orchestration; in-memory impl for tests.
//!
//! # Invariants
//!
//! - **PAT-DEGRADE-001**: chaos test reflects partial degradation; SLO impact
//!   measured.
//! - **CTRL-PRIV-001**: zero PII in chaos reports (caller responsibility; the
//!   types here carry no PII).
//! - **No `tokio` in `src/`** — pure synchronous state machine; async is the
//!   caller's concern (worker handler, CI driver, tests).
//! - **No `unsafe`** — `forbid(unsafe_code)`.
//! - **All public enums `#[non_exhaustive]`** — taxonomy can grow post-GA
//!   without breaking changes.
//!
//! # Quick start
//!
//! ```ignore
//! use corelink_chaos_scheduler::{
//!     catalog::canonical_catalog,
//!     runner::{run_experiment, SafeModeSnapshot, Telemetry},
//!     types::{ChaosAuditEvent, ChaosRun, ChaosRunId, ChaosTarget},
//! };
//!
//! struct NullTelemetry;
//! impl Telemetry for NullTelemetry {
//!     fn emit_audit(&self, _: &ChaosRun, _: ChaosAuditEvent) {}
//!     fn capture_state_pre(&self, _: &ChaosRun) -> u64 { 0 }
//!     fn capture_state_post(&self, _: &ChaosRun) -> u64 { 0 }
//!     fn measure_slo_impact(&self, _: &ChaosRun, _: u64, _: u64) -> u32 { 0 }
//! }
//!
//! let catalog = canonical_catalog();
//! let exp = catalog.first().unwrap();
//! let snap = SafeModeSnapshot {
//!     target: ChaosTarget::Staging,
//!     prod_sev1_active: false,
//!     staging_error_rate_bps: 0,
//! };
//! let _run = run_experiment(exp, ChaosRunId::new("weekly-001"), snap, &NullTelemetry);
//! ```

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod catalog;
pub mod runner;
pub mod types;

// Public re-exports for ergonomic import.
pub use catalog::{canonical_catalog, distinct_fm_count, lookup};
pub use runner::{derive_seed, run_experiment, SafeModeSnapshot, Telemetry};
pub use types::{
    ChaosAuditEvent, ChaosExperiment, ChaosExperimentId, ChaosKind, ChaosOutcome, ChaosRun,
    ChaosRunId, ChaosTarget,
};

// ---------------------------------------------------------------------------
// ChaosScheduler trait + InMemory impl
// ---------------------------------------------------------------------------

use std::collections::VecDeque;
use std::sync::Mutex;

/// Pluggable scheduler abstraction.
///
/// In production this is wired to the CF Cron Trigger (Mondays 06:00 UTC,
/// per `wrangler.toml`). In CI / tests, [`InMemoryScheduler`] is used to
/// drive deterministic replays.
pub trait ChaosScheduler {
    /// Enqueue an experiment id for execution at the next tick.
    fn enqueue(&self, experiment_id: ChaosExperimentId);
    /// Dequeue the next experiment id (returns `None` if queue is empty).
    fn next(&self) -> Option<ChaosExperimentId>;
    /// True iff the scheduler has no pending work.
    fn is_idle(&self) -> bool;
}

/// In-memory FIFO scheduler. Used in tests and as the canonical reference
/// implementation. Not intended for production (worker handler implements
/// the trait against KV).
#[derive(Debug, Default)]
pub struct InMemoryScheduler {
    queue: Mutex<VecDeque<ChaosExperimentId>>,
}

impl InMemoryScheduler {
    /// Construct an empty scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of pending experiments.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }
}

impl ChaosScheduler for InMemoryScheduler {
    fn enqueue(&self, experiment_id: ChaosExperimentId) {
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(experiment_id);
        }
    }

    fn next(&self) -> Option<ChaosExperimentId> {
        self.queue.lock().ok().and_then(|mut q| q.pop_front())
    }

    fn is_idle(&self) -> bool {
        self.queue.lock().map(|q| q.is_empty()).unwrap_or(true)
    }
}

/// Weekly rotation: yield catalog entries round-robin from the canonical
/// catalog so 8 successive weekly cron fires cover all 8 experiments.
#[must_use]
pub fn weekly_rotation(week_index: u32) -> ChaosExperimentId {
    let catalog = canonical_catalog();
    let len = catalog.len() as u32;
    let idx = if len == 0 {
        0
    } else {
        (week_index % len) as usize
    };
    catalog
        .get(idx)
        .map(|e| e.id.clone())
        .unwrap_or_else(|| ChaosExperimentId::new("noop"))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_starts_idle() {
        let s = InMemoryScheduler::new();
        assert!(s.is_idle());
        assert_eq!(s.pending(), 0);
        assert!(s.next().is_none());
    }

    #[test]
    fn scheduler_fifo_order() {
        let s = InMemoryScheduler::new();
        s.enqueue(ChaosExperimentId::new("a"));
        s.enqueue(ChaosExperimentId::new("b"));
        s.enqueue(ChaosExperimentId::new("c"));
        assert_eq!(s.pending(), 3);
        assert_eq!(s.next().map(|e| e.as_str().to_owned()), Some("a".into()));
        assert_eq!(s.next().map(|e| e.as_str().to_owned()), Some("b".into()));
        assert_eq!(s.next().map(|e| e.as_str().to_owned()), Some("c".into()));
        assert!(s.is_idle());
    }

    #[test]
    fn scheduler_idle_after_drain() {
        let s = InMemoryScheduler::new();
        s.enqueue(ChaosExperimentId::new("x"));
        let _ = s.next();
        assert!(s.is_idle());
    }

    #[test]
    fn weekly_rotation_covers_all_eight() {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for week in 0..8 {
            seen.insert(weekly_rotation(week).as_str().to_owned());
        }
        assert_eq!(seen.len(), 8, "8 weeks => 8 distinct experiments");
    }

    #[test]
    fn weekly_rotation_wraps() {
        // Week 0 == Week 8 (mod 8).
        assert_eq!(weekly_rotation(0), weekly_rotation(8));
    }

    #[test]
    fn public_re_exports_compile() {
        // Smoke test that re-exports resolve.
        let _ = ChaosTarget::Staging.is_chaos_safe();
        let _ = canonical_catalog();
    }
}
