//! Neon read-replica lag SLI — CoreLink-side soft SLI for
//! `SLO-REPLICATION-LAG-NEON` (closes DEBT-011 R-PREP-REPL-P2-001).
//!
//! # What this module ships
//!
//! Neon's platform SLA is sufficient for GA, but post-GA we want our own lag
//! signal so we can catch silent Neon-platform regressions before they bite a
//! customer-facing SLO. Per the audit (`2026-05-15-replication-audit.md §3.6 +
//! GAP-R6`), we build a CoreLink-side probe that writes `neon_probe(ts)` to the
//! primary every 60 s and reads from each EU replica, emitting
//! `corelink_neon_replica_lag_seconds{replica}` as an informational gauge.
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the CF Worker probe wiring (Neon serverless driver INSERT primary → SELECT
//! cross-region replica) is deferred. Here we ship:
//!
//! 1. [`NeonReplicaLagProbe`] trait — fixed boundary between the probe-worker
//!    and the metrics emit / verifier (`--domain=neon`).
//! 2. [`InMemoryNeonReplicaLagProbe`] — deterministic fixture (per-instance
//!    `Mutex<>` F-001 closure).
//! 3. Canonical metric name + cadence + 5 s soft ceiling constants matching
//!    `slo_catalog.md §4.27` (informational; not alerting at GA).
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{primary_region, replica_region}` — no `tenant_id` / `query_id`.
//! With 1 primary (`enam`) × 3 EU/SAM replicas = 3 séries (or 4 region × 4
//! region = 16 worst case if Neon expands). Budget-safe.
//!
//! # SLO classification — SOFT / informational
//!
//! `SLO-REPLICATION-LAG-NEON` is **soft / informational** post-GA:
//!
//! - 30-d baseline observation before tightening.
//! - **No alerting** at GA; only dashboard trend.
//! - Reviewed quarterly with Neon platform team if regression observed.
//!
//! This contrasts with `SLO-REPLICATION-LAG-{R2,D1,KV,DO}` which **do** alert.
//!
//! # Audit ordering
//!
//! The probe is a *read-only* observability source — no state mutation to wrap.
//! Probe failure does NOT block writes; it emits SEV-3 informational via the
//! audit sink so post-mortems can correlate.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for Neon replica lag.
///
/// LOAD-BEARING: must match `PROM_METRIC["neon"]` in
/// `scripts/verify-replication-lag.py`.
pub const METRIC_NEON_REPLICA_LAG_SECONDS: &str = "corelink_neon_replica_lag_seconds";

/// Probe cadence in seconds (60 s per ticket P2-001 acceptance criterion §1).
pub const NEON_PROBE_CADENCE_SECONDS: u64 = 60;

/// Soft p99 lag ceiling enforced by `SLO-REPLICATION-LAG-NEON` (informational
/// only — not paging at GA). Matches `slo_catalog.md §4.27 "Target (soft)"`.
pub const NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS: u64 = 5;

/// `true` — this SLO is informational only (no alerting at GA).
///
/// Asserted in tests so toggling this flag without an ADR is caught at CI.
pub const NEON_SLO_IS_INFORMATIONAL: bool = true;

/// One Neon replica-lag sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeonReplicaLagSample {
    /// Region where the probe write was issued (primary).
    pub primary_region: Region,
    /// Region the replica read was issued from.
    pub replica_region: Region,
    /// Observed lag in seconds (`replica_observed_ts − primary_write_ts`).
    pub lag_seconds: f64,
    /// Timestamp (ms since epoch) when the sample was taken.
    pub sample_timestamp_ms: u64,
}

impl NeonReplicaLagSample {
    /// `true` iff this sample is within the soft 5 s ceiling. Used for
    /// dashboard surfacing — never blocks writes.
    #[must_use]
    pub fn within_soft_ceiling(&self) -> bool {
        self.lag_seconds <= (NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS as f64)
    }
}

/// Neon read-replica lag probe.
///
/// # Examples
///
/// ```
/// use corelink_region::neon_replica_lag::{
///     InMemoryNeonReplicaLagProbe, NeonReplicaLagProbe,
/// };
/// use corelink_region::Region;
///
/// let probe = InMemoryNeonReplicaLagProbe::new();
/// probe.set_lag(Region::Enam, Region::Weur, 0.8);
/// let s = probe
///     .probe(Region::Enam, Region::Weur, 1_700_000_000_000)
///     .unwrap();
/// assert!((s.lag_seconds - 0.8).abs() < f64::EPSILON);
/// assert!(s.within_soft_ceiling());
/// ```
pub trait NeonReplicaLagProbe: std::fmt::Debug + Send + Sync {
    /// Probe Neon replica lag for `(primary_region, replica_region)`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying Neon query fails. Callers
    /// MUST emit SEV-3 informational on failure (no paging at GA).
    fn probe(
        &self,
        primary_region: Region,
        replica_region: Region,
        timestamp_ms: u64,
    ) -> Result<NeonReplicaLagSample, String>;

    /// Canonical Prometheus metric name (load-bearing for the verifier).
    fn metric_name(&self) -> &'static str {
        METRIC_NEON_REPLICA_LAG_SECONDS
    }
}

/// In-memory Neon replica-lag probe — deterministic fixture for tests.
#[derive(Debug, Default)]
pub struct InMemoryNeonReplicaLagProbe {
    lags: Mutex<Vec<(Region, Region, f64)>>,
}

impl InMemoryNeonReplicaLagProbe {
    /// Construct an empty probe; default lag for any pair is 0 s.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fixed lag (seconds) for a `(primary, replica)` pair.
    pub fn set_lag(&self, primary: Region, replica: Region, lag_seconds: f64) {
        let mut g = self.lags.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(p, r, _)| !(*p == primary && *r == replica));
        g.push((primary, replica, lag_seconds));
    }
}

impl NeonReplicaLagProbe for InMemoryNeonReplicaLagProbe {
    fn probe(
        &self,
        primary_region: Region,
        replica_region: Region,
        timestamp_ms: u64,
    ) -> Result<NeonReplicaLagSample, String> {
        let lag = self
            .lags
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|(p, r, _)| *p == primary_region && *r == replica_region)
            .map(|(_, _, v)| *v)
            .unwrap_or(0.0);
        if !lag.is_finite() || lag < 0.0 {
            return Err(format!(
                "invalid configured lag {lag:?} for primary={primary_region:?} replica={replica_region:?}"
            ));
        }
        Ok(NeonReplicaLagSample {
            primary_region,
            replica_region,
            lag_seconds: lag,
            sample_timestamp_ms: timestamp_ms,
        })
    }
}

/// Adversarial probe that always returns `Err`.
#[derive(Debug)]
pub struct FailingNeonReplicaLagProbe {
    message: String,
}

impl FailingNeonReplicaLagProbe {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl NeonReplicaLagProbe for FailingNeonReplicaLagProbe {
    fn probe(
        &self,
        _primary_region: Region,
        _replica_region: Region,
        _timestamp_ms: u64,
    ) -> Result<NeonReplicaLagSample, String> {
        Err(self.message.clone())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::assertions_on_constants
)]
mod tests {
    use super::*;

    #[test]
    fn metric_name_is_canonical() {
        assert_eq!(
            METRIC_NEON_REPLICA_LAG_SECONDS,
            "corelink_neon_replica_lag_seconds"
        );
    }

    #[test]
    fn slo_constants_match_catalog() {
        // slo_catalog.md §4.27 — soft 5s ceiling, 60s cadence, informational.
        assert_eq!(NEON_PROBE_CADENCE_SECONDS, 60);
        assert_eq!(NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS, 5);
        assert!(NEON_SLO_IS_INFORMATIONAL);
    }

    #[test]
    fn within_soft_ceiling_boundary() {
        // Exactly at ceiling = within (`<=` semantics).
        let s = NeonReplicaLagSample {
            primary_region: Region::Enam,
            replica_region: Region::Weur,
            lag_seconds: 5.0,
            sample_timestamp_ms: 0,
        };
        assert!(s.within_soft_ceiling());
        let s_over = NeonReplicaLagSample {
            primary_region: Region::Enam,
            replica_region: Region::Weur,
            lag_seconds: 5.001,
            sample_timestamp_ms: 0,
        };
        assert!(!s_over.within_soft_ceiling());
    }

    #[test]
    fn inmemory_probe_default_lag_zero() {
        let p = InMemoryNeonReplicaLagProbe::new();
        let s = p
            .probe(Region::Enam, Region::Weur, 1_700_000_000_000)
            .expect("probe");
        assert_eq!(s.lag_seconds, 0.0);
        assert!(s.within_soft_ceiling());
    }

    #[test]
    fn inmemory_probe_set_lag_observable() {
        let p = InMemoryNeonReplicaLagProbe::new();
        p.set_lag(Region::Enam, Region::Weur, 3.5);
        let s = p
            .probe(Region::Enam, Region::Weur, 1_700_000_000_000)
            .expect("probe");
        assert!((s.lag_seconds - 3.5).abs() < f64::EPSILON);
        assert!(s.within_soft_ceiling());
    }

    #[test]
    fn inmemory_probe_set_lag_breach_observable() {
        let p = InMemoryNeonReplicaLagProbe::new();
        p.set_lag(Region::Enam, Region::Sam, 12.0);
        let s = p
            .probe(Region::Enam, Region::Sam, 0)
            .expect("probe");
        assert!((s.lag_seconds - 12.0).abs() < f64::EPSILON);
        assert!(!s.within_soft_ceiling(), "12s > 5s soft ceiling");
    }

    #[test]
    fn inmemory_probe_rejects_negative_configured_lag() {
        let p = InMemoryNeonReplicaLagProbe::new();
        p.set_lag(Region::Enam, Region::Weur, -1.0);
        let r = p.probe(Region::Enam, Region::Weur, 0);
        assert!(r.is_err());
    }

    #[test]
    fn inmemory_probe_set_lag_overwrites_prior() {
        // Idempotency contract: re-setting a (primary, replica) pair replaces
        // the prior value rather than appending — ensures deterministic tests.
        let p = InMemoryNeonReplicaLagProbe::new();
        p.set_lag(Region::Enam, Region::Weur, 1.0);
        p.set_lag(Region::Enam, Region::Weur, 2.0);
        let s = p
            .probe(Region::Enam, Region::Weur, 0)
            .expect("probe");
        assert!((s.lag_seconds - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn failing_probe_returns_error() {
        let p = FailingNeonReplicaLagProbe::new("neon outage");
        let r = p.probe(Region::Enam, Region::Weur, 0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("neon"));
    }

    #[test]
    fn neon_probe_trait_object_safe() {
        let probes: Vec<Box<dyn NeonReplicaLagProbe>> = vec![
            Box::new(InMemoryNeonReplicaLagProbe::new()),
            Box::new(FailingNeonReplicaLagProbe::new("x")),
        ];
        assert_eq!(probes.len(), 2);
    }
}
