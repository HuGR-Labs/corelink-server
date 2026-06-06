//! Core types for the chaos scheduler (WI-S17-001).
//!
//! All public enums are `#[non_exhaustive]` so the canonical taxonomy can grow
//! (e.g., when KV-cold-start or supply-chain partition experiments are added
//! post-GA) without breaking the API contract.
//!
//! Invariants encoded here:
//!
//! - [`ChaosTarget::Production`] **must never** be the runtime env: the
//!   [`crate::runner::run_experiment`] helper rejects it at the first line
//!   (`CHAOS_PROD_HIT_ATTEMPTED`).
//! - [`ChaosKind`] mirrors the 8-experiment canonical taxonomy in
//!   `specs/04_sprints/S17/work_items/WI-S17-001-...md §6.1`.

use core::fmt;

// ---------------------------------------------------------------------------
// Experiment + run identifiers
// ---------------------------------------------------------------------------

/// Identifier of a chaos experiment in the canonical catalog.
///
/// Stable string keys (kebab-case) matching the catalog entries documented
/// in `specs/_runbooks/RB-CHAOS-CATALOG.md`. Tests use these to look up
/// catalog entries deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChaosExperimentId(pub(crate) String);

impl ChaosExperimentId {
    /// Construct a new experiment id from a borrowed string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the raw catalog key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChaosExperimentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier of a single chaos run (deterministic-seed source).
///
/// The PRNG used in [`crate::runner`] is seeded by the BLAKE-style 64-bit
/// digest of this value combined with the experiment id; replays of the same
/// (run_id, experiment_id) pair produce bit-identical outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChaosRunId(pub(crate) String);

impl ChaosRunId {
    /// Construct a new run id.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the raw run id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChaosRunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Taxonomy enums (#[non_exhaustive])
// ---------------------------------------------------------------------------

/// Canonical kind of chaos experiment.
///
/// Matches `specs/04_sprints/S17/work_items/WI-S17-001-...md §1`:
/// 4 families × 8 baseline experiments (latency / failure / resource
/// exhaustion / network partition) + 4 optional advanced experiments
/// (CPU pressure, memory pressure, disk-fill, DNS / auth-provider /
/// KV-cold-start) reserved for the post-GA expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChaosKind {
    /// Latency injection: add deterministic delay to a target dependency.
    LatencyInjection,
    /// Failure injection: synthetic 5xx / timeout / connection-drop.
    FailureInjection,
    /// Resource exhaustion: storage / quota near limit.
    ResourceExhaustion,
    /// Network partition: edge-to-origin or cross-region split.
    NetworkPartition,
    /// CPU pressure: stress N cores for `duration_seconds` (post-GA opt).
    CpuPressure,
    /// Memory pressure: allocate N MiB transient (post-GA opt).
    MemoryPressure,
    /// DNS failure: NXDOMAIN inject on target hostname (post-GA opt).
    DnsFailure,
    /// Auth provider unavailable (Clerk outage) — staged synthetic.
    AuthProviderUnavailable,
}

impl ChaosKind {
    /// Human-readable label (kebab-case, stable identifier).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LatencyInjection => "latency",
            Self::FailureInjection => "failure",
            Self::ResourceExhaustion => "resource_exhaustion",
            Self::NetworkPartition => "network_partition",
            Self::CpuPressure => "cpu_pressure",
            Self::MemoryPressure => "memory_pressure",
            Self::DnsFailure => "dns_failure",
            Self::AuthProviderUnavailable => "auth_provider_unavailable",
        }
    }
}

/// Target environment for a chaos run.
///
/// At GA only [`ChaosTarget::Staging`] is permissible; the runner enforces
/// this at the first line, before any state-capture work begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChaosTarget {
    /// Staging environment (the only legal target at GA).
    Staging,
    /// Production environment — runner aborts immediately with
    /// `CHAOS_PROD_HIT_ATTEMPTED` audit event.
    Production,
}

impl ChaosTarget {
    /// True iff this target is the only env where chaos is permitted at GA.
    #[must_use]
    pub fn is_chaos_safe(self) -> bool {
        matches!(self, Self::Staging)
    }

    /// Stable label for audit-event payloads.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

/// Outcome of a single chaos run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChaosOutcome {
    /// Steady-state hypothesis held; SLO impact within blast-radius bound.
    Passed {
        /// Deterministic SLO-impact magnitude (basis points; 100 = 1%).
        slo_impact_bps: u32,
    },
    /// Steady-state hypothesis breached during the run — auto-rollback fired.
    SteadyStateBreached {
        /// Reason label propagated to the audit event.
        reason: &'static str,
    },
    /// Safe-mode auto-abort fired *before* the experiment executed.
    Aborted {
        /// Trigger code; one of:
        /// `prod_target_violation`, `prod_sev1_active`,
        /// `staging_error_rate_high`.
        trigger: &'static str,
    },
}

impl ChaosOutcome {
    /// Stable label for métricas (Prometheus `outcome` label).
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed { .. } => "passed",
            Self::SteadyStateBreached { .. } => "steady_state_breached",
            Self::Aborted { .. } => "aborted",
        }
    }
}

// ---------------------------------------------------------------------------
// Experiment definition + run record
// ---------------------------------------------------------------------------

/// Definition of a chaos experiment as stored in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaosExperiment {
    /// Catalog key (kebab-case).
    pub id: ChaosExperimentId,
    /// Canonical FM-id covered (see `failure_modes.md`).
    pub fm_id: &'static str,
    /// Kind of chaos.
    pub kind: ChaosKind,
    /// Target dependency label (e.g., `r2`, `d1`, `neon`, `kv`, `do`).
    pub target: &'static str,
    /// Blast-radius bound: max allowed SLO impact (basis points). If the
    /// run exceeds this, auto-rollback fires.
    pub blast_radius_bps: u32,
    /// Maximum rollback time (seconds). Constant — per spec §6.1 ≤ 300s.
    pub rollback_seconds_max: u32,
    /// Pre-condition: services required healthy before chaos starts.
    pub preconditions: &'static [&'static str],
    /// Steady-state hypothesis (SLO that must NOT break during the run).
    pub steady_state_hypothesis: &'static str,
}

impl ChaosExperiment {
    /// True iff this experiment is in the GA-mandatory 8-set.
    #[must_use]
    pub fn is_ga_mandatory(&self) -> bool {
        matches!(
            self.kind,
            ChaosKind::LatencyInjection
                | ChaosKind::FailureInjection
                | ChaosKind::ResourceExhaustion
                | ChaosKind::NetworkPartition
        )
    }
}

/// Record of a single chaos run (used for audit + 7y archive manifest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaosRun {
    /// Unique run id (also PRNG seed source).
    pub run_id: ChaosRunId,
    /// Catalog key of the experiment.
    pub experiment_id: ChaosExperimentId,
    /// Target env (must be [`ChaosTarget::Staging`] at GA).
    pub target: ChaosTarget,
    /// Final outcome.
    pub outcome: ChaosOutcome,
    /// Deterministic 64-bit PRNG seed derived from `run_id` + `experiment_id`.
    pub seed: u64,
}

/// Audit-event categories emitted by the runner.
///
/// `corelink.chaos.run.started` / `.completed` / `.aborted` are the three
/// canonical event names; `Telemetry` adapters map them to the audit chain
/// (R2 `evidence-chaos/<run_id>.json` + 7y retention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChaosAuditEvent {
    /// `corelink.chaos.run.started` — fired right after safe-mode checks pass.
    Started,
    /// `corelink.chaos.run.completed` — fired with `Passed` *or* breached.
    Completed,
    /// `corelink.chaos.run.aborted` — fired for safe-mode triggered aborts.
    Aborted,
}

impl ChaosAuditEvent {
    /// Canonical event name (snake.dot.case per audit-event taxonomy).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Started => "corelink.chaos.run.started",
            Self::Completed => "corelink.chaos.run.completed",
            Self::Aborted => "corelink.chaos.run.aborted",
        }
    }
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
    fn experiment_id_round_trips() {
        let id = ChaosExperimentId::new("lat-r2-get");
        assert_eq!(id.as_str(), "lat-r2-get");
        assert_eq!(id.to_string(), "lat-r2-get");
    }

    #[test]
    fn run_id_round_trips() {
        let id = ChaosRunId::new("2026-05-14T06:00:00Z-run-42");
        assert_eq!(id.as_str(), "2026-05-14T06:00:00Z-run-42");
    }

    #[test]
    fn chaos_target_safety() {
        assert!(ChaosTarget::Staging.is_chaos_safe());
        assert!(!ChaosTarget::Production.is_chaos_safe());
        assert_eq!(ChaosTarget::Staging.label(), "staging");
        assert_eq!(ChaosTarget::Production.label(), "production");
    }

    #[test]
    fn chaos_kind_labels_unique() {
        let labels = [
            ChaosKind::LatencyInjection.label(),
            ChaosKind::FailureInjection.label(),
            ChaosKind::ResourceExhaustion.label(),
            ChaosKind::NetworkPartition.label(),
            ChaosKind::CpuPressure.label(),
            ChaosKind::MemoryPressure.label(),
            ChaosKind::DnsFailure.label(),
            ChaosKind::AuthProviderUnavailable.label(),
        ];
        let mut sorted: Vec<&&str> = labels.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 8, "8 distinct kind labels expected");
    }

    #[test]
    fn audit_event_names_canonical() {
        assert_eq!(
            ChaosAuditEvent::Started.name(),
            "corelink.chaos.run.started"
        );
        assert_eq!(
            ChaosAuditEvent::Completed.name(),
            "corelink.chaos.run.completed"
        );
        assert_eq!(
            ChaosAuditEvent::Aborted.name(),
            "corelink.chaos.run.aborted"
        );
    }

    #[test]
    fn outcome_labels_stable() {
        assert_eq!(ChaosOutcome::Passed { slo_impact_bps: 0 }.label(), "passed");
        assert_eq!(
            ChaosOutcome::SteadyStateBreached { reason: "x" }.label(),
            "steady_state_breached"
        );
        assert_eq!(
            ChaosOutcome::Aborted {
                trigger: "prod_target_violation"
            }
            .label(),
            "aborted"
        );
    }
}
