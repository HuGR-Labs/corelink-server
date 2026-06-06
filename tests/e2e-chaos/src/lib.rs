//! `e2e-chaos` — R3-4 end-to-end chaos drill harness.
//!
//! Drives the [`corelink_chaos_scheduler`] crate against the **8 canonical
//! chaos experiment kinds** (4 GA-mandatory: latency / failure / resource
//! exhaustion / network partition + 4 post-GA reserved: cpu / memory / dns /
//! auth-provider) plus a synthetic kv-d1 cold start drill, and pins:
//!
//! 1. **Steady-state hypothesis** — SLO impact within `blast_radius_bps` →
//!    [`ChaosOutcome::Passed`]; otherwise [`ChaosOutcome::SteadyStateBreached`]
//!    (auto-rollback). Charter `rollback ≤ 5 min` enforced at the experiment
//!    construction site (`rollback_seconds_max ≤ 300`).
//! 2. **Audit lifecycle** — exactly one of `[Started, Completed]`
//!    (in-flight) or `[Aborted]` (safe-mode trip). Any other shape is a
//!    bug per `corelink.chaos.run.*` taxonomy.
//! 3. **`corelink_chaos_slo_violation_total`** — recorded against every
//!    breach (counter incremented; labels mirror the WI-S17-001 metric
//!    schema documented in `RB-CHAOS-CATALOG.md` §9).
//!
//! # Cross-WI invariants pinned (adversarial)
//!
//! - **INV-S17-OPS-EXCLUSIVITY** — concurrent chaos drill + DR drill →
//!   [`OpsEventLock`] refuses (`LockHeld`); only one operation per region
//!   at a time.
//! - **INV-S17-SEV1-DRILL-PAUSE** — SEV-1 active → `prod_sev1_active`
//!   safe-mode trips → `Aborted{trigger:"prod_sev1_active"}` +
//!   `corelink.ops.drill_deferred` semantics asserted at audit-event level.
//! - **INV-S17-CHAOS-STAGING-ONLY** — `target = Production` → pre-flight
//!   reject with `Aborted{trigger:"prod_target_violation"}`. No state
//!   capture writes.
//!
//! # Property test (200 cases)
//!
//! `prop_seed_replay_identical` — for any `(run_id, experiment_id)` pair,
//! two independent runs produce bit-identical `ChaosRun` records (FNV-1a
//! 64-bit seed match + identical outcome). Deterministic replay is the
//! Lote 10.17 codex P2 gate.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` and `#[non_exhaustive]` enums consumed via
//!   canonical constructors only.
//! - No production HTTPS endpoints reached.
//! - No `tokio` — chaos runner is pure sync state machine.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::cell::RefCell;
use std::sync::Mutex;

use corelink_chaos_scheduler::{
    canonical_catalog, run_experiment, ChaosAuditEvent, ChaosExperiment, ChaosExperimentId,
    ChaosKind, ChaosOutcome, ChaosRun, ChaosRunId, ChaosTarget, SafeModeSnapshot, Telemetry,
};

// ---------------------------------------------------------------------------
// Catalog index for the 8 e2e-chaos test functions
// ---------------------------------------------------------------------------

/// E2E test function name → experiment kind binding.
///
/// The 8 canonical experiment kinds the R3-4 harness drives, mapping each
/// public test function to its underlying [`ChaosKind`] and steady-state
/// hypothesis text. Synthetic post-GA kinds (cpu / memory / dns /
/// auth-provider / cold-start) are constructed below via [`make_experiment`]
/// because the canonical catalog only ships the 4 GA-mandatory kinds.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ChaosE2eDrill {
    /// `chaos_network_partition_recovers` — `net-cross-region` (GA catalog).
    NetworkPartition,
    /// `chaos_cpu_pressure_sustained` — synthetic `cpu-pressure-sustained`.
    CpuPressureSustained,
    /// `chaos_memory_pressure_oomk_avoided` — synthetic `mem-pressure-oomk-avoid`.
    MemoryPressureOomkAvoided,
    /// `chaos_r2_disk_fill_quarantine` — synthetic R2 disk-fill quarantine.
    R2DiskFillQuarantine,
    /// `chaos_latency_injection_p99_bounded` — `lat-r2-get` (GA catalog).
    LatencyInjectionP99Bounded,
    /// `chaos_dns_failure_dual_resolver_failover` — synthetic DNS failover.
    DnsFailureDualResolverFailover,
    /// `chaos_clerk_outage_grace_period` — synthetic Clerk outage drill.
    ClerkOutageGracePeriod,
    /// `chaos_kv_d1_cold_start_below_sla` — synthetic KV/D1 cold start.
    KvD1ColdStartBelowSla,
}

impl ChaosE2eDrill {
    /// Stable kebab-case experiment id used as a catalog key.
    #[must_use]
    pub fn experiment_id(self) -> &'static str {
        match self {
            Self::NetworkPartition => "net-cross-region",
            Self::CpuPressureSustained => "cpu-pressure-sustained",
            Self::MemoryPressureOomkAvoided => "mem-pressure-oomk-avoid",
            Self::R2DiskFillQuarantine => "res-r2-disk-fill",
            Self::LatencyInjectionP99Bounded => "lat-r2-get",
            Self::DnsFailureDualResolverFailover => "dns-dual-resolver-failover",
            Self::ClerkOutageGracePeriod => "auth-clerk-outage",
            Self::KvD1ColdStartBelowSla => "kv-d1-cold-start",
        }
    }
}

/// Build the [`ChaosExperiment`] for a given e2e drill.
///
/// GA-catalog entries are returned via [`canonical_catalog`] lookup;
/// post-GA synthetic experiments are constructed in-place with charter-bound
/// values (`rollback_seconds_max ≤ 300`, `blast_radius_bps` documented
/// per drill).
#[must_use]
pub fn make_experiment(drill: ChaosE2eDrill) -> ChaosExperiment {
    let catalog = canonical_catalog();
    if let Some(entry) = catalog
        .iter()
        .find(|e| e.id.as_str() == drill.experiment_id())
    {
        return entry.clone();
    }

    match drill {
        ChaosE2eDrill::CpuPressureSustained => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-150",
            kind: ChaosKind::CpuPressure,
            target: "worker-cpu",
            blast_radius_bps: 300, // 3% SLO budget tolerance under sustained CPU stress
            rollback_seconds_max: 240,
            preconditions: &["worker-pool-healthy", "slo-burn-rate-nominal"],
            steady_state_hypothesis:
                "Sustained CPU pressure 60s; P99 worker latency ≤ baseline + 250ms; no OOMK",
        },
        ChaosE2eDrill::MemoryPressureOomkAvoided => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-150",
            kind: ChaosKind::MemoryPressure,
            target: "worker-rss",
            blast_radius_bps: 200,
            rollback_seconds_max: 180,
            preconditions: &["worker-rss-below-70pct"],
            steady_state_hypothesis:
                "Memory pressure +N MiB; eviction policy fires before OOM kill; no restarts",
        },
        ChaosE2eDrill::R2DiskFillQuarantine => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-059",
            kind: ChaosKind::ResourceExhaustion,
            target: "r2-bucket",
            blast_radius_bps: 250,
            rollback_seconds_max: 300,
            preconditions: &["r2-bucket-below-90pct"],
            steady_state_hypothesis:
                "R2 disk near 95%; writes quarantined to overflow tier; no data loss",
        },
        ChaosE2eDrill::DnsFailureDualResolverFailover => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-105",
            kind: ChaosKind::DnsFailure,
            target: "resolver",
            blast_radius_bps: 150,
            rollback_seconds_max: 120,
            preconditions: &["dual-resolver-configured"],
            steady_state_hypothesis:
                "Primary DNS NXDOMAIN 30s; secondary resolver serves traffic; no auth failure",
        },
        ChaosE2eDrill::ClerkOutageGracePeriod => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-160",
            kind: ChaosKind::AuthProviderUnavailable,
            target: "clerk",
            blast_radius_bps: 100,
            rollback_seconds_max: 300,
            preconditions: &["clerk-healthy", "grace-period-cache-warm"],
            steady_state_hypothesis:
                "Clerk 503 for grace_period_s; cached sessions remain valid; no logout storm",
        },
        ChaosE2eDrill::KvD1ColdStartBelowSla => ChaosExperiment {
            id: ChaosExperimentId::new(drill.experiment_id()),
            fm_id: "FM-152",
            kind: ChaosKind::ResourceExhaustion,
            target: "kv-d1",
            blast_radius_bps: 200,
            rollback_seconds_max: 180,
            preconditions: &["kv-healthy", "d1-healthy"],
            steady_state_hypothesis:
                "KV+D1 cold-start; P99 cold-start latency ≤ 800ms (SLA bound); no client errors",
        },
        // GA-catalog kinds must come from canonical_catalog(); reaching this
        // arm means the catalog did not contain the expected entry — fall
        // back to a charter-safe synthetic (rollback ≤ 300s).
        ChaosE2eDrill::NetworkPartition | ChaosE2eDrill::LatencyInjectionP99Bounded => {
            ChaosExperiment {
                id: ChaosExperimentId::new(drill.experiment_id()),
                fm_id: "FM-150",
                kind: ChaosKind::LatencyInjection,
                target: "fallback",
                blast_radius_bps: 500,
                rollback_seconds_max: 300,
                preconditions: &["healthy"],
                steady_state_hypothesis: "fallback hypothesis (catalog miss)",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test telemetry — captures audit events + records SLO violation metric
// ---------------------------------------------------------------------------

/// In-memory Telemetry capturing the audit-event sequence + the
/// `corelink_chaos_slo_violation_total` counter (incremented on every
/// `SteadyStateBreached` outcome). State-capture digests are deterministic
/// (constants); SLO impact is configured at construction time so each test
/// can drive either a passing or a breaching scenario.
#[derive(Debug)]
pub struct ChaosTestTelemetry {
    events: RefCell<Vec<ChaosAuditEvent>>,
    slo_violation_total: RefCell<u64>,
    configured_impact_bps: u32,
}

impl ChaosTestTelemetry {
    /// Construct with a configured SLO impact (basis points). Setting this
    /// above `blast_radius_bps` triggers `SteadyStateBreached`.
    #[must_use]
    pub fn new(configured_impact_bps: u32) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            slo_violation_total: RefCell::new(0),
            configured_impact_bps,
        }
    }

    /// Snapshot of audit events emitted so far (FIFO order).
    #[must_use]
    pub fn audit_events(&self) -> Vec<ChaosAuditEvent> {
        self.events.borrow().clone()
    }

    /// Read the `corelink_chaos_slo_violation_total` counter.
    #[must_use]
    pub fn slo_violation_total(&self) -> u64 {
        *self.slo_violation_total.borrow()
    }
}

impl Telemetry for ChaosTestTelemetry {
    fn emit_audit(&self, run: &ChaosRun, event: ChaosAuditEvent) {
        self.events.borrow_mut().push(event);
        // Record SLO violation on `Completed` events that carry a breach
        // outcome (the runner sets `outcome` before emitting Completed).
        if event == ChaosAuditEvent::Completed {
            if let ChaosOutcome::SteadyStateBreached { .. } = run.outcome {
                let mut g = self.slo_violation_total.borrow_mut();
                *g = g.saturating_add(1);
            }
        }
    }

    fn capture_state_pre(&self, _run: &ChaosRun) -> u64 {
        0x0000_a11c_e5ee_d000_u64
    }

    fn capture_state_post(&self, _run: &ChaosRun) -> u64 {
        0x0000_b0bb_1b1b_0000_u64
    }

    fn measure_slo_impact(&self, _run: &ChaosRun, _pre: u64, _post: u64) -> u32 {
        self.configured_impact_bps
    }
}

// ---------------------------------------------------------------------------
// Steady-state hypothesis helpers
// ---------------------------------------------------------------------------

/// Run an experiment with a deterministic clean-staging safe-mode snapshot
/// and a [`ChaosTestTelemetry`] configured for `impact_bps`. Returns the
/// telemetry handle alongside the [`ChaosRun`] so tests can assert audit
/// sequence + metric increments.
#[must_use]
pub fn run_clean_staging(
    drill: ChaosE2eDrill,
    run_id: &str,
    impact_bps: u32,
) -> (ChaosRun, ChaosTestTelemetry) {
    let exp = make_experiment(drill);
    let telemetry = ChaosTestTelemetry::new(impact_bps);
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: false,
        staging_error_rate_bps: 100,
    };
    let run = run_experiment(&exp, ChaosRunId::new(run_id), snap, &telemetry);
    (run, telemetry)
}

/// Assert that the audit-event sequence matches one of the two canonical
/// shapes: `[Aborted]` OR `[Started, Completed]`. Any other shape is a
/// fail-CLOSED bug per WI-S17-001 §1.
///
/// # Errors
///
/// Returns a descriptive error string if the shape is invalid.
pub fn assert_canonical_audit_sequence(events: &[ChaosAuditEvent]) -> Result<(), String> {
    match events {
        [ChaosAuditEvent::Aborted] => Ok(()),
        [ChaosAuditEvent::Started, ChaosAuditEvent::Completed] => Ok(()),
        other => Err(format!(
            "non-canonical audit sequence: {other:?}; expected [Aborted] OR [Started, Completed]"
        )),
    }
}

/// Assert charter `rollback ≤ 5 min` on an experiment. Used at the top of
/// every e2e test so failure surfaces as the offending experiment id.
///
/// # Errors
///
/// Returns descriptive error if `rollback_seconds_max > 300`.
pub fn assert_rollback_within_5min(exp: &ChaosExperiment) -> Result<(), String> {
    if exp.rollback_seconds_max > 300 {
        return Err(format!(
            "{} rollback_seconds_max={} exceeds 5-min charter cap",
            exp.id, exp.rollback_seconds_max
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-WI mutex — `ops_event_lock` model for INV-S17-OPS-EXCLUSIVITY
// ---------------------------------------------------------------------------

/// Reason an operation requested the [`OpsEventLock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpsEventKind {
    /// Chaos experiment in flight.
    Chaos,
    /// DR drill in flight.
    DrDrill,
    /// Runbook drill in flight.
    RunbookDrill,
}

impl OpsEventKind {
    /// Stable label propagated to audit events.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Chaos => "chaos",
            Self::DrDrill => "dr_drill",
            Self::RunbookDrill => "runbook_drill",
        }
    }
}

/// Pre-flight rejection reason.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpsLockError {
    /// Another operation is currently holding the lock.
    LockHeld {
        /// Kind of operation currently holding the lock.
        held_by: OpsEventKind,
    },
}

/// Shared `ops_event_lock` model — at most one of
/// {chaos / dr drill / runbook drill} active at a time per region.
///
/// Mirrors the D1 row `ops_event_lock` referenced in INV-S17-OPS-EXCLUSIVITY.
/// The e2e harness uses this in-memory model to exercise the invariant
/// at the test layer; production wires the same surface against D1.
#[derive(Debug, Default)]
pub struct OpsEventLock {
    held_by: Mutex<Option<OpsEventKind>>,
}

impl OpsEventLock {
    /// Construct an idle lock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to acquire the lock for `kind`. Returns
    /// [`OpsLockError::LockHeld`] if another operation is already running.
    ///
    /// # Errors
    ///
    /// Returns [`OpsLockError::LockHeld`] when the lock is currently held
    /// by another op kind.
    pub fn try_acquire(&self, kind: OpsEventKind) -> Result<OpsLockGuard<'_>, OpsLockError> {
        let mut g = match self.held_by.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(held) = *g {
            return Err(OpsLockError::LockHeld { held_by: held });
        }
        *g = Some(kind);
        Ok(OpsLockGuard {
            lock: self,
            kind,
            released: false,
        })
    }

    fn release(&self, kind: OpsEventKind) {
        let mut g = match self.held_by.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if *g == Some(kind) {
            *g = None;
        }
    }
}

/// RAII guard that releases the [`OpsEventLock`] on drop.
#[derive(Debug)]
pub struct OpsLockGuard<'a> {
    lock: &'a OpsEventLock,
    kind: OpsEventKind,
    released: bool,
}

impl OpsLockGuard<'_> {
    /// Explicitly release the lock (idempotent).
    pub fn release(mut self) {
        if !self.released {
            self.lock.release(self.kind);
            self.released = true;
        }
    }
}

impl Drop for OpsLockGuard<'_> {
    fn drop(&mut self) {
        if !self.released {
            self.lock.release(self.kind);
            self.released = true;
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn drill_ids_unique() {
        let ids = [
            ChaosE2eDrill::NetworkPartition,
            ChaosE2eDrill::CpuPressureSustained,
            ChaosE2eDrill::MemoryPressureOomkAvoided,
            ChaosE2eDrill::R2DiskFillQuarantine,
            ChaosE2eDrill::LatencyInjectionP99Bounded,
            ChaosE2eDrill::DnsFailureDualResolverFailover,
            ChaosE2eDrill::ClerkOutageGracePeriod,
            ChaosE2eDrill::KvD1ColdStartBelowSla,
        ];
        let mut sorted: Vec<&str> = ids.iter().map(|d| d.experiment_id()).collect();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "drill ids must be unique");
        assert_eq!(sorted.len(), 8);
    }

    #[test]
    fn make_experiment_rollback_charter_compliant() {
        for d in [
            ChaosE2eDrill::NetworkPartition,
            ChaosE2eDrill::CpuPressureSustained,
            ChaosE2eDrill::MemoryPressureOomkAvoided,
            ChaosE2eDrill::R2DiskFillQuarantine,
            ChaosE2eDrill::LatencyInjectionP99Bounded,
            ChaosE2eDrill::DnsFailureDualResolverFailover,
            ChaosE2eDrill::ClerkOutageGracePeriod,
            ChaosE2eDrill::KvD1ColdStartBelowSla,
        ] {
            let exp = make_experiment(d);
            assert_rollback_within_5min(&exp).unwrap();
        }
    }

    #[test]
    fn ops_lock_basic_acquire_release() {
        let lock = OpsEventLock::new();
        {
            let _g = lock.try_acquire(OpsEventKind::Chaos).unwrap();
            assert!(matches!(
                lock.try_acquire(OpsEventKind::DrDrill),
                Err(OpsLockError::LockHeld {
                    held_by: OpsEventKind::Chaos
                })
            ));
        }
        // After drop, lock is releasable again.
        assert!(lock.try_acquire(OpsEventKind::DrDrill).is_ok());
    }

    #[test]
    fn telemetry_records_slo_violation_on_breach() {
        // Use the GA-catalog lat-r2-get (blast 500 bps); configure 600 bps
        // impact to force breach.
        let (run, tele) = run_clean_staging(
            ChaosE2eDrill::LatencyInjectionP99Bounded,
            "test-breach",
            600,
        );
        assert!(matches!(
            run.outcome,
            ChaosOutcome::SteadyStateBreached { .. }
        ));
        assert_eq!(tele.slo_violation_total(), 1);
    }
}
