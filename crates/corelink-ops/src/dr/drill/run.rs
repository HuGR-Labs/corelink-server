//! DR drill run records: status state machine + RTO/RPO measurements + SLO impact.

use serde::{Deserialize, Serialize};

use super::env_guard::DrillEnv;
use super::schedule::DrillCycle;
use super::Region;

/// DR drill execution status state machine.
///
/// State machine: `Scheduled` → `InProgress` → (`Completed` | `Failed` | `Aborted`).
///
/// `#[non_exhaustive]` — callers MUST use a wildcard arm.
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::DrillStatus;
///
/// assert_eq!(DrillStatus::Scheduled.as_str(), "scheduled");
/// assert!(DrillStatus::Completed.is_terminal());
/// assert!(!DrillStatus::InProgress.is_terminal());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DrillStatus {
    /// Drill scheduled (cron triggered; not yet executing).
    Scheduled,
    /// Drill currently executing chaos injection / failover.
    InProgress,
    /// Drill completed successfully; RTO + RPO within SLO.
    Completed,
    /// Drill executed but did not meet SLO targets.
    Failed,
    /// Drill aborted (operator intervention or safety guard fired).
    Aborted,
}

impl DrillStatus {
    /// Canonical string for the D1 `status` column + Prometheus `outcome` label.
    pub fn as_str(self) -> &'static str {
        match self {
            DrillStatus::Scheduled => "scheduled",
            DrillStatus::InProgress => "in_progress",
            DrillStatus::Completed => "completed",
            DrillStatus::Failed => "failed",
            DrillStatus::Aborted => "aborted",
        }
    }

    /// Returns `true` if this is a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DrillStatus::Completed | DrillStatus::Failed | DrillStatus::Aborted
        )
    }

    /// Returns `true` if the drill outcome counts as success against the DoD.
    pub fn is_success(self) -> bool {
        matches!(self, DrillStatus::Completed)
    }
}

impl std::fmt::Display for DrillStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Measured Recovery Time + Recovery Point objectives for a single drill.
///
/// - **RTO** = elapsed wall-clock time from outage onset to traffic served
///   by sibling region (failover complete).
/// - **RPO** = data loss window in seconds (replication lag at outage onset).
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::RtoRpoMeasurement;
///
/// let m = RtoRpoMeasurement { rto_seconds: 120, rpo_seconds: 30 };
/// assert!(m.within_slo());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtoRpoMeasurement {
    /// Recovery time in seconds.
    pub rto_seconds: u64,
    /// Recovery point (data loss window) in seconds.
    pub rpo_seconds: u64,
}

impl RtoRpoMeasurement {
    /// Returns `true` if both RTO ≤ 1800s and RPO ≤ 60s.
    pub fn within_slo(self) -> bool {
        self.rto_seconds <= super::outage::RTO_CEIL_SECONDS
            && self.rpo_seconds <= super::outage::RPO_CEIL_SECONDS
    }
}

/// SLO impact measured during the drill window.
///
/// Captured against the multi-burn-rate alerts wired from S-09
/// (`corelink-slo`). Numerical impact lets the SRE Lead grade the drill
/// against error-budget consumption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SloImpact {
    /// Error budget consumed during the drill (percentage 0.0–100.0).
    pub error_budget_consumed_pct: f64,
    /// Number of 5xx responses observed during the drill window.
    pub error_count: u64,
    /// p99 latency observed during the drill window (milliseconds).
    pub latency_p99_ms: u64,
}

impl SloImpact {
    /// Construct a zero-impact baseline (used in `Scheduled` state).
    pub fn zero() -> Self {
        SloImpact {
            error_budget_consumed_pct: 0.0,
            error_count: 0,
            latency_p99_ms: 0,
        }
    }
}

/// A single DR drill execution record.
///
/// Mirrors the D1 `dr_drill_runs` table 1:1. The crate ships this struct;
/// the production CF Worker handler is responsible for persisting it to D1
/// (one row per `drill_id`) and archiving the JSON form to R2 with 7-year
/// retention (per Quality Standard 14.s17.7).
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::{DrillCycle, DrillEnv, DrillRun, DrillStatus, Region, SloImpact};
///
/// let run = DrillRun {
///     drill_id: "dr-drill-cycle-1-2026-07-01".into(),
///     cycle: DrillCycle::CfRegionOutage,
///     env: DrillEnv::Staging,
///     started_at_ms: 1_751_356_800_000,
///     simulated_region: Region::Weur,
///     failover_target: Some(Region::Sam),
///     rto_seconds: 0,
///     rpo_seconds: 0,
///     slo_impact: SloImpact::zero(),
///     completed_at_ms: None,
///     status: DrillStatus::Scheduled,
/// };
/// assert_eq!(run.status.as_str(), "scheduled");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillRun {
    /// Stable drill identifier (e.g. `dr-drill-cycle-1-2026-07-01`).
    pub drill_id: String,
    /// Which cycle is being executed.
    pub cycle: DrillCycle,
    /// Environment the drill executed in (staging-only at GA).
    pub env: DrillEnv,
    /// Wall-clock start time (ms since epoch).
    pub started_at_ms: u64,
    /// Region targeted by the synthetic outage.
    pub simulated_region: Region,
    /// Sibling region traffic was failed over to (`None` if drill aborted
    /// before failover engaged).
    pub failover_target: Option<Region>,
    /// Measured RTO in seconds (0 until drill completes).
    pub rto_seconds: u64,
    /// Measured RPO in seconds (0 until drill completes).
    pub rpo_seconds: u64,
    /// SLO impact captured during the drill window.
    pub slo_impact: SloImpact,
    /// Wall-clock end time (ms since epoch); `None` until terminal state.
    pub completed_at_ms: Option<u64>,
    /// Terminal drill status.
    pub status: DrillStatus,
}
