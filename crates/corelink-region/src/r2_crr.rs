//! R2 platform CRR indirect lag SLI — continuous SLI for
//! `SLO-REPLICATION-LAG-R2` (CRR extension; closes DEBT-011
//! R-PREP-REPL-P1-004).
//!
//! # What this module ships
//!
//! Cloudflare R2 cross-region replication is a managed feature without a
//! direct lag metric. Per the audit (`2026-05-15-replication-audit.md §3.2 +
//! GAP-R2`), we build an **indirect** SLI by writing a synthetic object
//! `r2_probe/<region>/<ts_ms>.bin` every 5 min to each region; a
//! sibling-region probe-worker confirms presence and emits
//! `corelink_r2_crr_lag_seconds{primary_region, replica_region}` histogram.
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the CF Worker probe wiring (R2 PUT primary → R2 HEAD replica retry loop) is
//! deferred. Here we ship:
//!
//! 1. [`R2CrrProbe`] trait — fixed boundary between the probe-worker and the
//!    metrics emit / verifier (`--domain=r2_crr`).
//! 2. [`InMemoryR2CrrProbe`] — deterministic fixture (per-instance
//!    `Arc<Mutex<>>` F-001 closure).
//! 3. Canonical metric name + cadence + 24 h ceiling constants matching
//!    `slo_catalog.md §4.23` "Target (CRR)".
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{primary_region, replica_region}` — no `tenant_id` / `object_key`.
//! With 4 × 4 = 16 pairs max (4 sibling pairs at GA). Budget-safe.
//!
//! # Audit ordering
//!
//! The probe writes a synthetic object (not customer data); the *write itself*
//! is the probe action. CRR lag is measured by polling the replica bucket.
//! Probe failure → SEV-3 alert path per `slo_catalog.md §4.23`.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for R2 CRR lag.
///
/// LOAD-BEARING: must match `PROM_METRIC["r2_crr"]` in
/// `scripts/verify-replication-lag.py`.
pub const METRIC_R2_CRR_LAG_SECONDS: &str = "corelink_r2_crr_lag_seconds";

/// Synthetic probe cadence (5 min = 300 s) per ticket P1-004
/// acceptance criterion §1.
pub const R2_CRR_PROBE_CADENCE_SECONDS: u64 = 300;

/// p99 lag ceiling enforced by `SLO-REPLICATION-LAG-R2` "Target (CRR)"
/// (`slo_catalog.md §4.23`). Matches `SLO-BACKUP-VERIFICATION` "R2 (RPO 24h)".
pub const R2_CRR_LAG_P99_CEILING_SECONDS: u64 = 24 * 3600;

/// "Object missing after 24h" hard incident threshold — anything beyond this
/// MUST emit `SEV-2` and create a paging incident regardless of burn-rate.
/// Matches §4.23 burn alert table.
pub const R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS: u64 = 24 * 3600;

/// One R2 CRR lag sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct R2CrrSample {
    /// Region where the synthetic object was written (primary).
    pub primary_region: Region,
    /// Region in which the object's presence was confirmed (replica).
    pub replica_region: Region,
    /// Observed lag in seconds (`replica_observed_ts − primary_write_ts`).
    pub lag_seconds: f64,
    /// `true` iff the synthetic object was confirmed present in the replica
    /// bucket; `false` iff still missing at probe time (lag = "elapsed since
    /// write").
    pub object_present: bool,
    /// Timestamp (ms since epoch) when the sample was taken.
    pub probe_timestamp_ms: u64,
}

impl R2CrrSample {
    /// `true` iff this sample is within the 24 h ceiling AND the object was
    /// confirmed present in the replica bucket.
    #[must_use]
    pub fn within_ceiling(&self) -> bool {
        self.object_present && self.lag_seconds <= (R2_CRR_LAG_P99_CEILING_SECONDS as f64)
    }

    /// `true` iff this sample crosses the "object missing after 24h"
    /// incident threshold — caller MUST emit SEV-2 + page.
    #[must_use]
    pub fn is_missing_incident(&self) -> bool {
        !self.object_present
            && self.lag_seconds >= (R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS as f64)
    }
}

/// R2 CRR indirect-lag probe.
///
/// # Examples
///
/// ```
/// use corelink_region::r2_crr::{InMemoryR2CrrProbe, R2CrrProbe};
/// use corelink_region::Region;
///
/// let probe = InMemoryR2CrrProbe::new();
/// probe.set_lag(Region::Wnam, Region::Enam, 600.0, true); // 10 min, present
/// let s = probe.probe(Region::Wnam, Region::Enam, 0).unwrap();
/// assert!(s.within_ceiling());
/// assert!(!s.is_missing_incident());
/// ```
pub trait R2CrrProbe: std::fmt::Debug + Send + Sync {
    /// Probe the R2 CRR lag from `primary` to `replica`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the probe-worker cannot reach either bucket
    /// (network / auth / R2 SDK error). Callers MUST emit SEV-3 on failure
    /// per `slo_catalog.md §4.23` burn alert table.
    fn probe(
        &self,
        primary: Region,
        replica: Region,
        timestamp_ms: u64,
    ) -> Result<R2CrrSample, String>;

    /// Canonical Prometheus metric name (load-bearing for the verifier).
    fn metric_name(&self) -> &'static str {
        METRIC_R2_CRR_LAG_SECONDS
    }
}

/// In-memory R2 CRR probe — deterministic fixture for tests.
#[derive(Debug, Default)]
pub struct InMemoryR2CrrProbe {
    /// `(primary, replica) → (lag_seconds, object_present)`.
    samples: Mutex<Vec<(Region, Region, f64, bool)>>,
}

impl InMemoryR2CrrProbe {
    /// Construct an empty probe; default is `(0.0 s, present=true)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fixed lag + presence flag for a `(primary, replica)` pair.
    pub fn set_lag(
        &self,
        primary: Region,
        replica: Region,
        lag_seconds: f64,
        object_present: bool,
    ) {
        let mut g = self.samples.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(p, r, _, _)| !(*p == primary && *r == replica));
        g.push((primary, replica, lag_seconds, object_present));
    }
}

impl R2CrrProbe for InMemoryR2CrrProbe {
    fn probe(
        &self,
        primary: Region,
        replica: Region,
        timestamp_ms: u64,
    ) -> Result<R2CrrSample, String> {
        let (lag, present) = self
            .samples
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|(p, r, _, _)| *p == primary && *r == replica)
            .map(|(_, _, lag, present)| (*lag, *present))
            .unwrap_or((0.0, true));
        if !lag.is_finite() || lag < 0.0 {
            return Err(format!(
                "invalid configured lag {lag:?} for primary={primary:?} \
                 replica={replica:?}"
            ));
        }
        Ok(R2CrrSample {
            primary_region: primary,
            replica_region: replica,
            lag_seconds: lag,
            object_present: present,
            probe_timestamp_ms: timestamp_ms,
        })
    }
}

/// Adversarial probe that always returns `Err`.
#[derive(Debug)]
pub struct FailingR2CrrProbe {
    message: String,
}

impl FailingR2CrrProbe {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl R2CrrProbe for FailingR2CrrProbe {
    fn probe(
        &self,
        _primary: Region,
        _replica: Region,
        _timestamp_ms: u64,
    ) -> Result<R2CrrSample, String> {
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
        assert_eq!(METRIC_R2_CRR_LAG_SECONDS, "corelink_r2_crr_lag_seconds");
    }

    #[test]
    fn slo_constants_match_catalog() {
        // slo_catalog.md §4.23 "Target (CRR)" — 24h ceiling.
        assert_eq!(R2_CRR_LAG_P99_CEILING_SECONDS, 24 * 3600);
        assert_eq!(R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS, 24 * 3600);
        // ticket P1-004 acceptance criterion §1 — 5 min cadence.
        assert_eq!(R2_CRR_PROBE_CADENCE_SECONDS, 300);
    }

    #[test]
    fn within_ceiling_present_under_24h() {
        let s = R2CrrSample {
            primary_region: Region::Wnam,
            replica_region: Region::Enam,
            lag_seconds: 600.0,
            object_present: true,
            probe_timestamp_ms: 0,
        };
        assert!(s.within_ceiling());
        assert!(!s.is_missing_incident());
    }

    #[test]
    fn within_ceiling_boundary_exact_24h() {
        let s = R2CrrSample {
            primary_region: Region::Wnam,
            replica_region: Region::Enam,
            lag_seconds: (24 * 3600) as f64,
            object_present: true,
            probe_timestamp_ms: 0,
        };
        assert!(s.within_ceiling(), "exactly 24h is within ceiling (<=)");
    }

    #[test]
    fn within_ceiling_false_when_object_missing() {
        let s = R2CrrSample {
            primary_region: Region::Wnam,
            replica_region: Region::Enam,
            lag_seconds: 60.0,
            object_present: false,
            probe_timestamp_ms: 0,
        };
        assert!(!s.within_ceiling(), "missing object never within ceiling");
    }

    #[test]
    fn is_missing_incident_at_24h_threshold() {
        let s = R2CrrSample {
            primary_region: Region::Wnam,
            replica_region: Region::Enam,
            lag_seconds: (24 * 3600) as f64,
            object_present: false,
            probe_timestamp_ms: 0,
        };
        assert!(s.is_missing_incident());
    }

    #[test]
    fn is_missing_incident_false_under_threshold() {
        // Missing object under 24h is still concerning but not yet a paging
        // incident (caught by burn-rate alert instead).
        let s = R2CrrSample {
            primary_region: Region::Wnam,
            replica_region: Region::Enam,
            lag_seconds: 100.0,
            object_present: false,
            probe_timestamp_ms: 0,
        };
        assert!(!s.is_missing_incident());
    }

    #[test]
    fn inmemory_probe_default_zero_lag_present() {
        let p = InMemoryR2CrrProbe::new();
        let s = p
            .probe(Region::Wnam, Region::Enam, 1_700_000_000_000)
            .expect("probe");
        assert_eq!(s.lag_seconds, 0.0);
        assert!(s.object_present);
        assert!(s.within_ceiling());
    }

    #[test]
    fn inmemory_probe_set_lag_observable() {
        let p = InMemoryR2CrrProbe::new();
        p.set_lag(Region::Weur, Region::Sam, 3600.0, true); // 1h
        let s = p.probe(Region::Weur, Region::Sam, 0).expect("probe");
        assert!((s.lag_seconds - 3600.0).abs() < f64::EPSILON);
        assert!(s.within_ceiling());
    }

    #[test]
    fn inmemory_probe_set_missing_observable() {
        let p = InMemoryR2CrrProbe::new();
        // Simulate 25h missing object — must trigger incident.
        p.set_lag(Region::Wnam, Region::Enam, (25 * 3600) as f64, false);
        let s = p.probe(Region::Wnam, Region::Enam, 0).expect("probe");
        assert!(!s.within_ceiling());
        assert!(s.is_missing_incident());
    }

    #[test]
    fn inmemory_probe_set_lag_replaces_prior_value() {
        let p = InMemoryR2CrrProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, 100.0, true);
        p.set_lag(Region::Wnam, Region::Enam, 200.0, true);
        let s = p.probe(Region::Wnam, Region::Enam, 0).expect("probe");
        assert!((s.lag_seconds - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn inmemory_probe_rejects_negative_configured_lag() {
        let p = InMemoryR2CrrProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, -1.0, true);
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
    }

    #[test]
    fn failing_probe_returns_error() {
        let p = FailingR2CrrProbe::new("simulated R2 outage");
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("simulated"));
    }

    #[test]
    fn r2_crr_probe_trait_object_safe() {
        let probes: Vec<Box<dyn R2CrrProbe>> = vec![
            Box::new(InMemoryR2CrrProbe::new()),
            Box::new(FailingR2CrrProbe::new("x")),
        ];
        assert_eq!(probes.len(), 2);
    }
}
