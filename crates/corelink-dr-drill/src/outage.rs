//! CF region outage simulator — marks region `synthetic_outage`, routes
//! read/write traffic to sibling region, measures RTO + RPO against
//! SLO ceilings (RTO ≤ 1800s + RPO ≤ 60s per WI §6 DoD).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::env_guard::DrillEnv;
use crate::error::DrillError;
use crate::run::{RtoRpoMeasurement, SloImpact};
use crate::{Region, ResidencyGraph};

/// RTO ceiling: failover MUST complete within 30 minutes (1800 seconds).
///
/// Per WI §6 DoD + SLO catalog `SLO-RTO-REGION-FAILOVER`.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::RTO_CEIL_SECONDS;
///
/// assert_eq!(RTO_CEIL_SECONDS, 1800);
/// ```
pub const RTO_CEIL_SECONDS: u64 = 1800;

/// RPO ceiling: data loss window ≤ 60s (replication lag p99).
///
/// Per WI §6 DoD + `SLO-RPO-REGION` + replication lag SLO from WI-S14-003.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::RPO_CEIL_SECONDS;
///
/// assert_eq!(RPO_CEIL_SECONDS, 60);
/// ```
pub const RPO_CEIL_SECONDS: u64 = 60;

/// Result of executing a CF region outage simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutageOutcome {
    /// Region that was marked `synthetic_outage`.
    pub simulated_region: Region,
    /// Sibling region traffic was failed over to.
    pub failover_target: Region,
    /// Measured RTO + RPO.
    pub measurement: RtoRpoMeasurement,
    /// SLO impact captured during the drill window.
    pub slo_impact: SloImpact,
    /// `true` if both RTO and RPO were within SLO ceilings.
    pub within_slo: bool,
}

/// CF region outage simulator trait.
///
/// Implementations mark the target region as `synthetic_outage`, route
/// read/write traffic to the sibling region (per
/// [`ResidencyGraph`](corelink_failover_router::ResidencyGraph)), and
/// measure RTO / RPO against the canonical SLO ceilings.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::{
///     CfRegionOutageSimulator, DrillEnv, InMemoryCfRegionOutageSimulator, Region,
/// };
///
/// let sim = InMemoryCfRegionOutageSimulator::new();
/// let outcome = sim
///     .simulate(DrillEnv::Staging, Region::Weur, 120, 30)
///     .unwrap();
/// assert_eq!(outcome.failover_target, Region::Sam);
/// assert!(outcome.within_slo);
/// ```
pub trait CfRegionOutageSimulator: std::fmt::Debug + Send + Sync {
    /// Run a synthetic CF region outage simulation.
    ///
    /// - `env` MUST be `Staging` or `Test` (else `ProdEnvForbidden`).
    /// - `simulated_region` MUST have a sibling in [`ResidencyGraph`]
    ///   (else `NoSiblingAvailable`).
    /// - `synthetic_rto_seconds` + `synthetic_rpo_seconds` are the values
    ///   the harness injected (the production handler computes these from
    ///   real clock + replication lag).
    fn simulate(
        &self,
        env: DrillEnv,
        simulated_region: Region,
        synthetic_rto_seconds: u64,
        synthetic_rpo_seconds: u64,
    ) -> Result<OutageOutcome, DrillError>;

    /// List regions currently marked `synthetic_outage`.
    fn synthetic_outages(&self) -> Result<Vec<Region>, DrillError>;

    /// Clear all synthetic outage markings (drill teardown).
    fn clear(&self) -> Result<(), DrillError>;
}

/// In-memory `CfRegionOutageSimulator` for tests + the trait-abstraction-defer
/// skeleton.
#[derive(Debug, Default)]
pub struct InMemoryCfRegionOutageSimulator {
    graph: ResidencyGraph,
    outages: Arc<Mutex<HashSet<Region>>>,
}

impl InMemoryCfRegionOutageSimulator {
    /// Construct a new simulator with the canonical [`ResidencyGraph`].
    pub fn new() -> Self {
        InMemoryCfRegionOutageSimulator::default()
    }
}

impl CfRegionOutageSimulator for InMemoryCfRegionOutageSimulator {
    fn simulate(
        &self,
        env: DrillEnv,
        simulated_region: Region,
        synthetic_rto_seconds: u64,
        synthetic_rpo_seconds: u64,
    ) -> Result<OutageOutcome, DrillError> {
        // Canonical staging-only enforcement boundary FIRST (mirrors WI §1 TS guard).
        crate::env_guard::require_staging(env)?;

        let failover_target = self
            .graph
            .sibling(simulated_region)
            .ok_or(DrillError::NoSiblingAvailable {
                region: simulated_region,
            })?;

        // Mark synthetic_outage.
        {
            let mut guard = self.outages.lock().map_err(|e| {
                DrillError::Internal(format!("outages lock poisoned: {e}"))
            })?;
            guard.insert(simulated_region);
        }

        let measurement = RtoRpoMeasurement {
            rto_seconds: synthetic_rto_seconds,
            rpo_seconds: synthetic_rpo_seconds,
        };

        // Synthetic SLO impact: linear consumption proportional to RTO/RPO.
        // The production handler reads real S-09 multi-burn-rate alerts;
        // here we ship a deterministic shim so tests can assert ordering.
        let error_budget_consumed_pct =
            slo_budget_pct(measurement.rto_seconds, measurement.rpo_seconds);
        let slo_impact = SloImpact {
            error_budget_consumed_pct,
            error_count: synthetic_rto_seconds.saturating_mul(10),
            latency_p99_ms: 200u64.saturating_add(synthetic_rto_seconds),
        };

        let within_slo = measurement.within_slo();

        // If outside SLO, return SloViolation — caller decides Failed vs Aborted.
        if !within_slo {
            let (slo, measured, ceiling) = if measurement.rto_seconds > RTO_CEIL_SECONDS {
                ("rto", measurement.rto_seconds, RTO_CEIL_SECONDS)
            } else {
                ("rpo", measurement.rpo_seconds, RPO_CEIL_SECONDS)
            };
            return Err(DrillError::SloViolation {
                slo,
                measured_seconds: measured,
                ceiling_seconds: ceiling,
            });
        }

        Ok(OutageOutcome {
            simulated_region,
            failover_target,
            measurement,
            slo_impact,
            within_slo,
        })
    }

    fn synthetic_outages(&self) -> Result<Vec<Region>, DrillError> {
        let guard = self
            .outages
            .lock()
            .map_err(|e| DrillError::Internal(format!("outages lock poisoned: {e}")))?;
        let mut out: Vec<Region> = guard.iter().copied().collect();
        out.sort_by_key(|r| r.as_str());
        Ok(out)
    }

    fn clear(&self) -> Result<(), DrillError> {
        let mut guard = self
            .outages
            .lock()
            .map_err(|e| DrillError::Internal(format!("outages lock poisoned: {e}")))?;
        guard.clear();
        Ok(())
    }
}

/// Deterministic SLO budget consumption percentage for a given RTO/RPO pair.
///
/// Formula (test shim only — production reads real S-09 metrics):
/// `min(100, (rto/RTO_CEIL + rpo/RPO_CEIL) * 50)`.
fn slo_budget_pct(rto_seconds: u64, rpo_seconds: u64) -> f64 {
    let rto_frac = rto_seconds as f64 / RTO_CEIL_SECONDS as f64;
    let rpo_frac = rpo_seconds as f64 / RPO_CEIL_SECONDS as f64;
    let raw = (rto_frac + rpo_frac) * 50.0;
    if raw > 100.0 {
        100.0
    } else {
        raw
    }
}
