//! DR drill scheduler — semestral cadence (every 6 months) + CF Cron expression
//! `0 6 1 1,7 *` (Jan 1 + Jul 1 at 06:00 UTC).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::env_guard::DrillEnv;
use super::error::DrillError;
use super::run::{DrillRun, DrillStatus, SloImpact};
use super::Region;

/// CF Cron expression for semestral cadence: Jan 1 + Jul 1 at 06:00 UTC.
///
/// Format: `minute hour day-of-month month day-of-week`.
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::SEMESTRAL_CRON;
///
/// assert_eq!(SEMESTRAL_CRON, "0 6 1 1,7 *");
/// ```
pub const SEMESTRAL_CRON: &str = "0 6 1 1,7 *";

/// DR drill cadence options.
///
/// `#[non_exhaustive]` — callers MUST use a wildcard arm.
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::DrillCadence;
///
/// assert_eq!(DrillCadence::Semestral.months_between(), 6);
/// assert_eq!(DrillCadence::Annual.months_between(), 12);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DrillCadence {
    /// Every 6 months (canonical for cycle 1 CF region outage).
    Semestral,
    /// Annual (deferred cycles 2 + 3 at GA via waiver opt per WI §6.2).
    Annual,
}

impl DrillCadence {
    /// Returns the cadence interval in months.
    pub fn months_between(self) -> u32 {
        match self {
            DrillCadence::Semestral => 6,
            DrillCadence::Annual => 12,
        }
    }

    /// Canonical string label.
    pub fn as_str(self) -> &'static str {
        match self {
            DrillCadence::Semestral => "semestral",
            DrillCadence::Annual => "annual",
        }
    }
}

/// The three canonical DR drill cycles defined in WI §6.1.
///
/// Cycle 1 is executed in S-17 (mandatory ship gate per DoD §6).
/// Cycles 2 + 3 are deferred annual at GA via waiver opt (per WI §6.2 + ADR).
///
/// `#[non_exhaustive]` — callers MUST use a wildcard arm.
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::{DrillCadence, DrillCycle};
///
/// assert_eq!(DrillCycle::CfRegionOutage.cycle_number(), 1);
/// assert_eq!(DrillCycle::CfRegionOutage.canonical_cadence(), DrillCadence::Semestral);
/// assert_eq!(DrillCycle::D1PrimaryLoss.canonical_cadence(), DrillCadence::Annual);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DrillCycle {
    /// Cycle 1: simulate CF region outage → failover → SLO sustained.
    CfRegionOutage,
    /// Cycle 2: simulate D1 primary loss → restore from backup (deferred annual).
    D1PrimaryLoss,
    /// Cycle 3: simulate BYOK key compromise → crypto-erase → notify (deferred annual).
    ByokKeyCompromise,
}

impl DrillCycle {
    /// Cycle ordinal (1 / 2 / 3) — used for `cycle` Prometheus label.
    pub fn cycle_number(self) -> u8 {
        match self {
            DrillCycle::CfRegionOutage => 1,
            DrillCycle::D1PrimaryLoss => 2,
            DrillCycle::ByokKeyCompromise => 3,
        }
    }

    /// Canonical string label.
    pub fn as_str(self) -> &'static str {
        match self {
            DrillCycle::CfRegionOutage => "cf_region_outage",
            DrillCycle::D1PrimaryLoss => "d1_primary_loss",
            DrillCycle::ByokKeyCompromise => "byok_key_compromise",
        }
    }

    /// The canonical cadence for this cycle.
    ///
    /// Cycle 1 = `Semestral`. Cycles 2 + 3 = `Annual` (deferred at GA via waiver).
    pub fn canonical_cadence(self) -> DrillCadence {
        match self {
            DrillCycle::CfRegionOutage => DrillCadence::Semestral,
            DrillCycle::D1PrimaryLoss | DrillCycle::ByokKeyCompromise => DrillCadence::Annual,
        }
    }
}

/// A scheduled DR drill — the (cycle, cadence, cron) tuple the CF Worker
/// cron handler consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillSchedule {
    /// Which cycle.
    pub cycle: DrillCycle,
    /// Cadence for this schedule.
    pub cadence: DrillCadence,
    /// CF Cron expression (semestral → [`SEMESTRAL_CRON`]; annual → `0 6 1 1 *`).
    pub cron: String,
}

impl DrillSchedule {
    /// Build the canonical schedule for a cycle.
    pub fn canonical(cycle: DrillCycle) -> Self {
        let cadence = cycle.canonical_cadence();
        let cron = match cadence {
            DrillCadence::Semestral => SEMESTRAL_CRON.to_string(),
            DrillCadence::Annual => "0 6 1 1 *".to_string(),
        };
        DrillSchedule {
            cycle,
            cadence,
            cron,
        }
    }
}

/// DR drill scheduler trait — the abstraction the CF Worker cron handler
/// satisfies.
///
/// # Examples
///
/// ```
/// use corelink_ops::dr::drill::{
///     DrillCycle, DrillEnv, DrillScheduler, DrillStatus, InMemoryDrillScheduler, Region,
/// };
///
/// let sched = InMemoryDrillScheduler::new();
/// let run = sched
///     .schedule(
///         "dr-drill-cycle-1-2026-07-01".into(),
///         DrillCycle::CfRegionOutage,
///         DrillEnv::Staging,
///         1_751_356_800_000,
///         Region::Weur,
///     )
///     .unwrap();
/// assert_eq!(run.status, DrillStatus::Scheduled);
/// ```
pub trait DrillScheduler: std::fmt::Debug + Send + Sync {
    /// Schedule a new DR drill run. Returns the seeded [`DrillRun`] in
    /// [`DrillStatus::Scheduled`].
    ///
    /// Fails with [`DrillError::ProdEnvForbidden`] if `env == Production`
    /// (canonical staging-only enforcement boundary).
    fn schedule(
        &self,
        drill_id: String,
        cycle: DrillCycle,
        env: DrillEnv,
        started_at_ms: u64,
        simulated_region: Region,
    ) -> Result<DrillRun, DrillError>;

    /// List all currently-tracked drill runs.
    fn list(&self) -> Result<Vec<DrillRun>, DrillError>;

    /// Fetch a single drill run by id.
    fn get(&self, drill_id: &str) -> Result<Option<DrillRun>, DrillError>;

    /// Update the run's status (state-machine transition).
    fn update_status(&self, drill_id: &str, status: DrillStatus) -> Result<(), DrillError>;
}

/// In-memory `DrillScheduler` implementation for tests + the
/// trait-abstraction-defer skeleton.
#[derive(Debug, Default)]
pub struct InMemoryDrillScheduler {
    runs: Arc<Mutex<HashMap<String, DrillRun>>>,
}

impl InMemoryDrillScheduler {
    /// Construct an empty scheduler.
    pub fn new() -> Self {
        InMemoryDrillScheduler::default()
    }
}

impl DrillScheduler for InMemoryDrillScheduler {
    fn schedule(
        &self,
        drill_id: String,
        cycle: DrillCycle,
        env: DrillEnv,
        started_at_ms: u64,
        simulated_region: Region,
    ) -> Result<DrillRun, DrillError> {
        // Canonical staging-only enforcement boundary FIRST.
        super::env_guard::require_staging(env)?;

        let run = DrillRun {
            drill_id: drill_id.clone(),
            cycle,
            env,
            started_at_ms,
            simulated_region,
            failover_target: None,
            rto_seconds: 0,
            rpo_seconds: 0,
            slo_impact: SloImpact::zero(),
            completed_at_ms: None,
            status: DrillStatus::Scheduled,
        };

        let mut guard = self
            .runs
            .lock()
            .map_err(|e| DrillError::Internal(format!("scheduler lock poisoned: {e}")))?;
        guard.insert(drill_id, run.clone());
        Ok(run)
    }

    fn list(&self) -> Result<Vec<DrillRun>, DrillError> {
        let guard = self
            .runs
            .lock()
            .map_err(|e| DrillError::Internal(format!("scheduler lock poisoned: {e}")))?;
        let mut runs: Vec<DrillRun> = guard.values().cloned().collect();
        runs.sort_by_key(|r| r.started_at_ms);
        Ok(runs)
    }

    fn get(&self, drill_id: &str) -> Result<Option<DrillRun>, DrillError> {
        let guard = self
            .runs
            .lock()
            .map_err(|e| DrillError::Internal(format!("scheduler lock poisoned: {e}")))?;
        Ok(guard.get(drill_id).cloned())
    }

    fn update_status(&self, drill_id: &str, status: DrillStatus) -> Result<(), DrillError> {
        let mut guard = self
            .runs
            .lock()
            .map_err(|e| DrillError::Internal(format!("scheduler lock poisoned: {e}")))?;
        match guard.get_mut(drill_id) {
            Some(run) => {
                run.status = status;
                Ok(())
            }
            None => Err(DrillError::Internal(format!(
                "drill_id {drill_id} not found"
            ))),
        }
    }
}
