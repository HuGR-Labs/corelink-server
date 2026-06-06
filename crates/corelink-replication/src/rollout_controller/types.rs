//! Core types for the progressive rollout controller
//! (WI-S13-005, PAT-PROGRESSIVE-ROLLOUT-001).
//!
//! All public enums are `#[non_exhaustive]` per Lote 10.6bis discipline
//! so additive growth lands without breaking downstream `match` sites.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The four progressive rollout stages: 1% → 10% → 50% → 100%.
///
/// Stage sequence is strictly enforced; skipping a stage returns HTTP
/// 403 + audit `corelink.admin.rollout.bypass_attempted`
/// (INV-ROLLOUT-NO-STAGE-SKIP).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RolloutStage {
    /// 1% traffic — lowest blast radius; surfaces obvious bugs.
    /// Minimum dwell: 15 min before gate evaluation may advance.
    Stage1Pct,
    /// 10% traffic — validates at 10× scale; surfaces ratio-based bugs.
    /// Minimum dwell: 30 min.
    Stage10Pct,
    /// 50% traffic — validates at majority scale; surfaces capacity bugs.
    /// Minimum dwell: 60 min.
    Stage50Pct,
    /// 100% traffic — terminal; full traffic.
    /// Minimum dwell: 0 (no advance possible; rollout completed).
    Stage100Pct,
}

impl RolloutStage {
    /// Returns the next stage in progression, or `None` if terminal
    /// (`Stage100Pct`).
    ///
    /// Stage advance is gated on [`GateMetrics`] satisfying all
    /// criteria + minimum dwell time [`Self::dwell_minutes_min`].
    #[must_use]
    pub fn next(self) -> Option<RolloutStage> {
        match self {
            Self::Stage1Pct => Some(Self::Stage10Pct),
            Self::Stage10Pct => Some(Self::Stage50Pct),
            Self::Stage50Pct => Some(Self::Stage100Pct),
            Self::Stage100Pct => None,
        }
    }

    /// Canonical traffic percentage for this stage.
    #[must_use]
    pub fn traffic_pct(self) -> u8 {
        match self {
            Self::Stage1Pct => 1,
            Self::Stage10Pct => 10,
            Self::Stage50Pct => 50,
            Self::Stage100Pct => 100,
        }
    }

    /// Minimum dwell time (minutes) at this stage before gate
    /// evaluation may advance the rollout. Enforces hi-fidelity
    /// metric collection (lower stage = less data = longer dwell needed).
    #[must_use]
    pub fn dwell_minutes_min(self) -> u32 {
        match self {
            Self::Stage1Pct => 15,
            Self::Stage10Pct => 30,
            Self::Stage50Pct => 60,
            Self::Stage100Pct => 0,
        }
    }

    /// Canonical D1 CHECK constraint string for this stage.
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Stage1Pct => "stage_1pct",
            Self::Stage10Pct => "stage_10pct",
            Self::Stage50Pct => "stage_50pct",
            Self::Stage100Pct => "stage_100pct",
        }
    }
}

/// Status of a rollout session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum RolloutStatus {
    /// Rollout queued; not yet started (pre-start state).
    Pending,
    /// Rollout actively progressing through stages.
    Active,
    /// Rollout reached `Stage100Pct` successfully.
    Completed,
    /// Auto-rollback triggered (error rate / SLO burn / p99 latency).
    AutoRolledBack,
    /// Admin manually aborted via dual-approval gate (WI-S13-002).
    ManuallyAborted,
    /// Monthly rollback budget exceeded (>30%); deploys frozen.
    BudgetFrozen,
}

impl RolloutStatus {
    /// Canonical D1 CHECK constraint string for this status.
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::AutoRolledBack => "auto_rolled_back",
            Self::ManuallyAborted => "manually_aborted",
            Self::BudgetFrozen => "budget_frozen",
        }
    }
}

/// Auto-rollback trigger cause (any of the 3 fires rollback).
///
/// Per spec §9.3: 3 independent triggers cover orthogonal failure modes.
/// Sustained 5-min threshold filters transient variance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutoRollbackTrigger {
    /// Error rate > baseline + 3σ, sustained 5 min.
    /// Rolling 7d baseline; covers reliability degradation.
    ErrorRateExceedsBaseline3Sigma,
    /// SLO burn-rate > 14.4 (1h window, per Google SRE Workbook Ch 16).
    /// Depletes 1 month error budget in ~70 min if sustained.
    SloBurnRateExceeds14_4,
    /// p99 latency > baseline + 50%, sustained 5 min.
    /// Rolling 7d baseline; covers latency degradation without errors.
    P99LatencyExceedsBaseline50Pct,
}

impl AutoRollbackTrigger {
    /// Canonical Prometheus label value for metric
    /// `corelink_admin_rollout_auto_rollback_total{trigger}`.
    #[must_use]
    pub fn as_metric_label(self) -> &'static str {
        match self {
            Self::ErrorRateExceedsBaseline3Sigma => "error_rate",
            Self::SloBurnRateExceeds14_4 => "slo_burn",
            Self::P99LatencyExceedsBaseline50Pct => "p99_latency",
        }
    }
}

/// Deploy artifact to be rolled out. Must carry a Cosign signature
/// and Rekor log index (INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-PROVENANCE-IN-REKOR
/// from S-12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployArtifact {
    /// SHA-256 digest of the deploy artifact (32 bytes, hex-encoded).
    pub sha256_hex: String,
    /// URL of the Cosign signature (OCI reference or detached sig URL).
    /// Required; `None` → `RolloutError::UnsignedDeploy`.
    pub cosign_signature_url: Option<String>,
    /// Rekor transparency log inclusion index.
    /// Required; `None` → `RolloutError::UnsignedDeploy`.
    pub rekor_log_index: Option<i64>,
    /// Human-readable deploy artifact description.
    pub description: String,
}

impl DeployArtifact {
    /// Returns `true` if the artifact carries a Cosign signature and
    /// Rekor log index (INV-SUPPLY-SIGNED-DEPLOY).
    #[must_use]
    pub fn is_cosign_signed(&self) -> bool {
        self.cosign_signature_url.is_some() && self.rekor_log_index.is_some()
    }
}

/// Admin actor initiating or approving a rollout operation.
/// Carries MFA freshness attestation (CTRL-AUTH-010).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminActor {
    /// Admin user identifier (D1 BLOB(16) / UUID).
    pub user_id: Uuid,
    /// Human-readable actor email for audit records.
    pub email: String,
    /// Unix timestamp (ms) of last successful MFA verification.
    /// Must be ≤ 30 min before op for MFA freshness gate
    /// (INV-ADMIN-MFA-FRESHNESS).
    pub mfa_verified_at_ms: u64,
}

/// Handle referencing a specific rollout session in progress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RolloutHandle {
    /// Unique identifier for this rollout session (UUIDv7, monotonic).
    pub handle_id: Uuid,
    /// Current stage as of last probe.
    pub current_stage: RolloutStage,
    /// Current status as of last probe.
    pub status: RolloutStatus,
    /// Unix timestamp (ms) when rollout was started.
    pub started_at_ms: u64,
    /// Unix timestamp (ms) when current stage was entered.
    pub stage_entered_at_ms: u64,
    /// Auto-rollback trigger, if `status == AutoRolledBack`.
    pub rollback_trigger: Option<AutoRollbackTrigger>,
    /// Unix timestamp (ms) of rollback, if applicable.
    pub rollback_at_ms: Option<u64>,
    /// Deploy artifact SHA-256 (hex).
    pub artifact_sha256_hex: String,
}

/// Gate metrics snapshot evaluated at each probe interval (60s).
/// Used for advance/hold/rollback decision by the state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateMetrics {
    /// Current error rate (fraction, 0.0–1.0).
    pub error_rate: f64,
    /// 7-day rolling baseline error rate.
    pub error_rate_baseline: f64,
    /// Standard deviation of error rate over rolling baseline window.
    pub error_rate_sigma: f64,
    /// SLO burn-rate over 1h window (Google SRE Workbook Ch 16).
    pub slo_burn_rate_1h: f64,
    /// Current p99 latency (ms).
    pub p99_latency_ms: f64,
    /// 7-day rolling baseline p99 latency (ms).
    pub p99_baseline_ms: f64,
    /// Elapsed time at current stage (seconds).
    pub stage_elapsed_secs: u64,
}

impl GateMetrics {
    /// Returns `true` if error rate exceeds baseline + 3σ.
    #[must_use]
    pub fn error_rate_trigger(&self) -> bool {
        self.error_rate > self.error_rate_baseline + 3.0 * self.error_rate_sigma
    }

    /// Returns `true` if SLO burn-rate > 14.4 (1h window).
    #[must_use]
    pub fn slo_burn_trigger(&self) -> bool {
        self.slo_burn_rate_1h > 14.4
    }

    /// Returns `true` if p99 latency > baseline + 50%.
    #[must_use]
    pub fn p99_latency_trigger(&self) -> bool {
        self.p99_baseline_ms > 0.0 && self.p99_latency_ms > self.p99_baseline_ms * 1.5
    }

    /// Returns the first auto-rollback trigger that fires, if any.
    #[must_use]
    pub fn any_rollback_trigger(&self) -> Option<AutoRollbackTrigger> {
        if self.error_rate_trigger() {
            Some(AutoRollbackTrigger::ErrorRateExceedsBaseline3Sigma)
        } else if self.slo_burn_trigger() {
            Some(AutoRollbackTrigger::SloBurnRateExceeds14_4)
        } else if self.p99_latency_trigger() {
            Some(AutoRollbackTrigger::P99LatencyExceedsBaseline50Pct)
        } else {
            None
        }
    }

    /// Returns `true` if dwell time ≥ minimum for current stage.
    #[must_use]
    pub fn dwell_satisfied(&self, stage: RolloutStage) -> bool {
        let min_secs = u64::from(stage.dwell_minutes_min()) * 60;
        self.stage_elapsed_secs >= min_secs
    }

    /// Returns `true` if all gate criteria pass (no triggers, dwell OK).
    #[must_use]
    pub fn gate_passes(&self, stage: RolloutStage) -> bool {
        self.any_rollback_trigger().is_none() && self.dwell_satisfied(stage)
    }
}

/// Decision produced by `probe_and_advance`.
#[derive(Debug, Clone)]
pub struct RolloutDecision {
    /// Stage at time of probe.
    pub current_stage: RolloutStage,
    /// Recommended next action.
    pub next_action: NextAction,
    /// Gate metrics snapshot at time of decision.
    pub gate_metrics: GateMetrics,
}

/// Next action recommended by the rollout controller.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum NextAction {
    /// Advance to the given stage (gate criteria met + dwell satisfied).
    Advance(RolloutStage),
    /// Hold at current stage; reason string for observability.
    Hold(String),
    /// Trigger auto-rollback with given trigger cause.
    AutoRollback(AutoRollbackTrigger),
    /// Rollout complete (Stage100Pct reached and stable).
    Complete,
}
