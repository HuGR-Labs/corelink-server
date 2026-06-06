//! D1 read-replica lag probe — continuous SLI for `SLO-REPLICATION-LAG-D1`
//! (closes DEBT-011 R-PREP-REPL-P0-002).
//!
//! # What this module ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production probe queries the primary region's D1 (`SELECT MAX(created_at)`)
//! and each replica region's D1 with the same query; the **difference is the
//! observed lag** in seconds. That wiring lives in the CF Worker probe glue.
//!
//! Here we ship:
//!
//! 1. [`D1ReplicaLagProbe`] trait — fixed boundary that DR-16 §1 (Detect step)
//!    reads instead of the CF dashboard (acceptance criterion §3 of
//!    `R-PREP-REPL-P0-002`).
//! 2. [`InMemoryD1ReplicaLagProbe`] — deterministic fixture (per-instance
//!    `Arc<Mutex<>>` F-001 closure) used by integration tests AND the verifier
//!    script in `--mode=inmemory`.
//! 3. The canonical Prometheus metric name + label schema constants so the
//!    verifier (`scripts/verify-replication-lag.py`) can rely on stable names.
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{primary_region, replica_region}` — no `tenant_id`. With 4 × 4 = 16
//! pairs max, sibling-only path = 4 pairs at GA. Budget-safe.
//!
//! # Audit ordering
//!
//! The probe is a *read-only* observability source — there is no state mutation
//! to wrap with audit. The probe's failure → SEV-3 alert path is wired in the
//! CF Worker glue (read `last_synced_at` from D1; if probe itself errors, SEV-3
//! per `slo_catalog.md §4.24`).

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for D1 read-replica lag.
pub const METRIC_D1_REPLICA_LAG_SECONDS: &str = "corelink_d1_replica_lag_seconds";

/// Canonical probe cadence (30 s) per ticket acceptance criterion §1.
pub const D1_PROBE_CADENCE_SECONDS: u64 = 30;

/// p99 lag ceiling enforced by `SLO-REPLICATION-LAG-D1` (§4.24).
pub const D1_REPLICA_LAG_P99_CEILING_SECONDS: u64 = 60;

/// One probe sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct D1LagSample {
    /// Region whose D1 acts as the primary (write-owner).
    pub primary_region: Region,
    /// Region whose D1 acts as the read replica.
    pub replica_region: Region,
    /// Observed lag in seconds (`primary_max_ts − replica_max_ts`).
    pub lag_seconds: f64,
    /// Timestamp (ms since epoch) when the probe was taken.
    pub probe_timestamp_ms: u64,
}

/// D1 read-replica lag probe.
///
/// # Examples
///
/// ```
/// use corelink_region::replica_lag::{
///     D1ReplicaLagProbe, InMemoryD1ReplicaLagProbe,
/// };
/// use corelink_region::Region;
///
/// let probe = InMemoryD1ReplicaLagProbe::new();
/// probe.set_lag(Region::Wnam, Region::Enam, 2.5);
/// let s = probe.probe(Region::Wnam, Region::Enam, 1_700_000_000_000).unwrap();
/// assert!((s.lag_seconds - 2.5).abs() < f64::EPSILON);
/// ```
pub trait D1ReplicaLagProbe: std::fmt::Debug + Send + Sync {
    /// Probe the D1 read-replica lag between `primary` and `replica`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying probe query fails (network /
    /// auth / D1 SDK error). Callers MUST emit SEV-3 on failure
    /// (`slo_catalog.md §4.24`).
    fn probe(
        &self,
        primary: Region,
        replica: Region,
        timestamp_ms: u64,
    ) -> Result<D1LagSample, String>;

    /// Canonical Prometheus metric name (load-bearing for the verifier).
    fn metric_name(&self) -> &'static str {
        METRIC_D1_REPLICA_LAG_SECONDS
    }
}

/// In-memory D1 lag probe — deterministic fixture for tests and verifier
/// `--mode=inmemory`.
#[derive(Debug, Default)]
pub struct InMemoryD1ReplicaLagProbe {
    lags: Mutex<Vec<(Region, Region, f64)>>,
}

impl InMemoryD1ReplicaLagProbe {
    /// Construct an empty in-memory probe; default lag for any pair is 0 s.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fixed lag (seconds) for a primary→replica pair.
    pub fn set_lag(&self, primary: Region, replica: Region, lag_seconds: f64) {
        let mut g = self.lags.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(p, r, _)| !(*p == primary && *r == replica));
        g.push((primary, replica, lag_seconds));
    }
}

impl D1ReplicaLagProbe for InMemoryD1ReplicaLagProbe {
    fn probe(
        &self,
        primary: Region,
        replica: Region,
        timestamp_ms: u64,
    ) -> Result<D1LagSample, String> {
        let lag = self
            .lags
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|(p, r, _)| *p == primary && *r == replica)
            .map(|(_, _, v)| *v)
            .unwrap_or(0.0);
        if !lag.is_finite() || lag < 0.0 {
            return Err(format!(
                "invalid configured lag {lag:?} for primary={primary:?} \
                 replica={replica:?}"
            ));
        }
        Ok(D1LagSample {
            primary_region: primary,
            replica_region: replica,
            lag_seconds: lag,
            probe_timestamp_ms: timestamp_ms,
        })
    }
}

/// Adversarial probe that always returns `Err` — verifies SEV-3 path.
#[derive(Debug)]
pub struct FailingD1ReplicaLagProbe {
    message: String,
}

impl FailingD1ReplicaLagProbe {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl D1ReplicaLagProbe for FailingD1ReplicaLagProbe {
    fn probe(
        &self,
        _primary: Region,
        _replica: Region,
        _timestamp_ms: u64,
    ) -> Result<D1LagSample, String> {
        Err(self.message.clone())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[test]
    fn metric_name_is_canonical() {
        // Load-bearing for `scripts/verify-replication-lag.py` and Prometheus
        // alerting rules. Renaming requires updating both.
        assert_eq!(
            METRIC_D1_REPLICA_LAG_SECONDS,
            "corelink_d1_replica_lag_seconds"
        );
    }

    #[test]
    fn slo_constants_match_catalog() {
        // slo_catalog.md §4.24 — target p99 ≤ 60 s
        assert_eq!(D1_REPLICA_LAG_P99_CEILING_SECONDS, 60);
        // ticket P0-002 acceptance criterion §1 — 30 s cadence
        assert_eq!(D1_PROBE_CADENCE_SECONDS, 30);
    }

    #[test]
    fn inmemory_probe_default_lag_zero() {
        let p = InMemoryD1ReplicaLagProbe::new();
        let s = p
            .probe(Region::Wnam, Region::Enam, 1_700_000_000_000)
            .expect("probe");
        assert_eq!(s.lag_seconds, 0.0);
        assert_eq!(s.primary_region, Region::Wnam);
        assert_eq!(s.replica_region, Region::Enam);
        assert_eq!(s.probe_timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn inmemory_probe_set_lag_observable() {
        let p = InMemoryD1ReplicaLagProbe::new();
        p.set_lag(Region::Weur, Region::Sam, 12.5);
        let s = p.probe(Region::Weur, Region::Sam, 0).expect("probe");
        assert!((s.lag_seconds - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn inmemory_probe_set_lag_replaces_prior_value() {
        let p = InMemoryD1ReplicaLagProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, 5.0);
        p.set_lag(Region::Wnam, Region::Enam, 70.0); // over SLO ceiling
        let s = p.probe(Region::Wnam, Region::Enam, 0).expect("probe");
        assert!((s.lag_seconds - 70.0).abs() < f64::EPSILON);
        assert!(s.lag_seconds > (D1_REPLICA_LAG_P99_CEILING_SECONDS as f64));
    }

    #[test]
    fn inmemory_probe_rejects_negative_configured_lag() {
        let p = InMemoryD1ReplicaLagProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, -1.0);
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
    }

    #[test]
    fn failing_probe_returns_error() {
        let p = FailingD1ReplicaLagProbe::new("simulated D1 outage");
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("simulated"));
    }

    #[test]
    fn d1_probe_trait_object_safe() {
        // Verify the trait can be used behind dyn — required for production
        // CF Worker glue to inject the real impl via `Arc<dyn D1ReplicaLagProbe>`.
        let probes: Vec<Box<dyn D1ReplicaLagProbe>> = vec![
            Box::new(InMemoryD1ReplicaLagProbe::new()),
            Box::new(FailingD1ReplicaLagProbe::new("x")),
        ];
        assert_eq!(probes.len(), 2);
    }
}
