//! KV cross-region propagation-lag SLI — continuous SLI for
//! `SLO-REPLICATION-LAG-KV` (closes DEBT-011 R-PREP-REPL-P1-001).
//!
//! # What this module ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production probe writes `kv_probe:<region>:<ts_ms>` from each region
//! every 30 s and reads from all four regions, computing
//! `lag = read_observed_ts − written_ts` per (write_region, read_region) pair.
//! That wiring lives in the CF Worker probe glue.
//!
//! Here we ship:
//!
//! 1. [`KvPropagationProbe`] trait — fixed boundary between the synthetic probe
//!    worker and the rest of the system (DR-16 §1 Detect step + verifier
//!    `--domain=kv` consume this).
//! 2. [`InMemoryKvPropagationProbe`] — deterministic fixture
//!    (per-instance `Arc<Mutex<>>` F-001 closure).
//! 3. Canonical Prometheus metric name + label schema + SLO threshold constants
//!    so that `scripts/verify-replication-lag.py` (`PROM_METRIC["kv"]`) can rely
//!    on stable names. Renaming requires touching both sides.
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{write_region, read_region}` — no `tenant_id` / `key`. With 4 × 4 = 16
//! pairs max (12 inter-region pairs after excluding self), budget-safe.
//!
//! # Audit ordering
//!
//! The probe is a *read-only* observability source — no state mutation to wrap.
//! Probe failure → SEV-3 alert path is wired in the CF Worker glue per
//! `slo_catalog.md §4.25`.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for KV propagation lag.
///
/// LOAD-BEARING: must match `PROM_METRIC["kv"]` in
/// `scripts/verify-replication-lag.py` and the alerting rule in
/// `slo_catalog.md §4.25`.
pub const METRIC_KV_PROPAGATION_LAG_SECONDS: &str = "corelink_kv_propagation_lag_seconds";

/// Canonical probe cadence (30 s) per ticket P1-001 acceptance criterion §1.
pub const KV_PROBE_CADENCE_SECONDS: u64 = 30;

/// p99 typical lag ceiling enforced by `SLO-REPLICATION-LAG-KV` (§4.25).
pub const KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS: u64 = 60;

/// p99 pessimistic ceiling per `PAT-KV-TTL-001` (KV max TTL upper bound).
pub const KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS: u64 = 300;

/// Required fraction of region-pair samples that must be within the typical
/// ceiling (95% per `slo_catalog.md §4.25` "Target (typical)").
pub const KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION: f64 = 0.95;

/// One KV propagation-lag sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvPropagationSample {
    /// Region where the probe key was written.
    pub write_region: Region,
    /// Region from which the probe key was subsequently read.
    pub read_region: Region,
    /// Observed lag in seconds (`read_observed_ts − write_ts`).
    pub lag_seconds: f64,
    /// Timestamp (ms since epoch) at which the read observation was taken.
    pub probe_timestamp_ms: u64,
}

/// KV cross-region propagation-lag probe.
///
/// # Examples
///
/// ```
/// use corelink_region::kv_propagation::{
///     InMemoryKvPropagationProbe, KvPropagationProbe,
/// };
/// use corelink_region::Region;
///
/// let probe = InMemoryKvPropagationProbe::new();
/// probe.set_lag(Region::Wnam, Region::Enam, 12.0);
/// let s = probe
///     .probe(Region::Wnam, Region::Enam, 1_700_000_000_000)
///     .unwrap();
/// assert!((s.lag_seconds - 12.0).abs() < f64::EPSILON);
/// ```
pub trait KvPropagationProbe: std::fmt::Debug + Send + Sync {
    /// Probe the KV propagation lag between `write_region` and `read_region`.
    ///
    /// `read_region == write_region` is also valid (self-pair); the probe MAY
    /// return `0.0` for that case.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying KV read/write fails. Callers
    /// MUST emit SEV-3 on failure per `slo_catalog.md §4.25`.
    fn probe(
        &self,
        write_region: Region,
        read_region: Region,
        timestamp_ms: u64,
    ) -> Result<KvPropagationSample, String>;

    /// Canonical Prometheus metric name (load-bearing for the verifier).
    fn metric_name(&self) -> &'static str {
        METRIC_KV_PROPAGATION_LAG_SECONDS
    }
}

/// In-memory KV propagation probe — deterministic fixture for tests and
/// the verifier `--mode=inmemory`.
#[derive(Debug, Default)]
pub struct InMemoryKvPropagationProbe {
    lags: Mutex<Vec<(Region, Region, f64)>>,
}

impl InMemoryKvPropagationProbe {
    /// Construct an empty in-memory probe; default lag for any pair is 0 s.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fixed lag (seconds) for a (write, read) region pair.
    pub fn set_lag(&self, write_region: Region, read_region: Region, lag_seconds: f64) {
        let mut g = self.lags.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(w, r, _)| !(*w == write_region && *r == read_region));
        g.push((write_region, read_region, lag_seconds));
    }
}

impl KvPropagationProbe for InMemoryKvPropagationProbe {
    fn probe(
        &self,
        write_region: Region,
        read_region: Region,
        timestamp_ms: u64,
    ) -> Result<KvPropagationSample, String> {
        let lag = self
            .lags
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|(w, r, _)| *w == write_region && *r == read_region)
            .map(|(_, _, v)| *v)
            .unwrap_or(0.0);
        if !lag.is_finite() || lag < 0.0 {
            return Err(format!(
                "invalid configured lag {lag:?} for write={write_region:?} \
                 read={read_region:?}"
            ));
        }
        Ok(KvPropagationSample {
            write_region,
            read_region,
            lag_seconds: lag,
            probe_timestamp_ms: timestamp_ms,
        })
    }
}

/// Adversarial probe that always returns `Err` — verifies SEV-3 path.
#[derive(Debug)]
pub struct FailingKvPropagationProbe {
    message: String,
}

impl FailingKvPropagationProbe {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl KvPropagationProbe for FailingKvPropagationProbe {
    fn probe(
        &self,
        _write_region: Region,
        _read_region: Region,
        _timestamp_ms: u64,
    ) -> Result<KvPropagationSample, String> {
        Err(self.message.clone())
    }
}

/// Compute the fraction of `samples` whose `lag_seconds` is `<= ceiling_secs`.
///
/// Returns `1.0` for an empty sample set (vacuous truth; the verifier treats
/// an empty result as "no data" rather than as a violation — see
/// `scripts/verify-replication-lag.py` exit-code 2 contract).
#[must_use]
pub fn fraction_within_typical(samples: &[KvPropagationSample], ceiling_secs: f64) -> f64 {
    if samples.is_empty() {
        return 1.0;
    }
    let within = samples
        .iter()
        .filter(|s| s.lag_seconds <= ceiling_secs)
        .count();
    (within as f64) / (samples.len() as f64)
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
        // LOAD-BEARING for `scripts/verify-replication-lag.py::PROM_METRIC["kv"]`
        // and the alerting rule in `slo_catalog.md §4.25`.
        assert_eq!(
            METRIC_KV_PROPAGATION_LAG_SECONDS,
            "corelink_kv_propagation_lag_seconds"
        );
    }

    #[test]
    fn slo_constants_match_catalog() {
        // slo_catalog.md §4.25 — typical ceiling 60s, pessimistic ceiling 300s,
        // 95% sample fraction within typical.
        assert_eq!(KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS, 60);
        assert_eq!(KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS, 300);
        assert!((KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION - 0.95).abs() < f64::EPSILON);
        // ticket P1-001 acceptance criterion §1 — 30 s cadence
        assert_eq!(KV_PROBE_CADENCE_SECONDS, 30);
    }

    #[test]
    fn inmemory_probe_default_lag_zero() {
        let p = InMemoryKvPropagationProbe::new();
        let s = p
            .probe(Region::Wnam, Region::Enam, 1_700_000_000_000)
            .expect("probe");
        assert_eq!(s.lag_seconds, 0.0);
        assert_eq!(s.write_region, Region::Wnam);
        assert_eq!(s.read_region, Region::Enam);
        assert_eq!(s.probe_timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn inmemory_probe_set_lag_observable_inter_region() {
        // KV is global — every (write, read) pair is valid, including non-siblings.
        let p = InMemoryKvPropagationProbe::new();
        p.set_lag(Region::Weur, Region::Wnam, 45.0);
        let s = p.probe(Region::Weur, Region::Wnam, 0).expect("probe");
        assert!((s.lag_seconds - 45.0).abs() < f64::EPSILON);
    }

    #[test]
    fn inmemory_probe_set_lag_replaces_prior_value() {
        let p = InMemoryKvPropagationProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, 5.0);
        p.set_lag(Region::Wnam, Region::Enam, 90.0); // over typical ceiling
        let s = p.probe(Region::Wnam, Region::Enam, 0).expect("probe");
        assert!((s.lag_seconds - 90.0).abs() < f64::EPSILON);
        assert!(s.lag_seconds > (KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS as f64));
        assert!(s.lag_seconds < (KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS as f64));
    }

    #[test]
    fn inmemory_probe_rejects_negative_configured_lag() {
        let p = InMemoryKvPropagationProbe::new();
        p.set_lag(Region::Wnam, Region::Enam, -1.0);
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
    }

    #[test]
    fn failing_probe_returns_error() {
        let p = FailingKvPropagationProbe::new("simulated KV outage");
        let r = p.probe(Region::Wnam, Region::Enam, 0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("simulated"));
    }

    #[test]
    fn kv_probe_trait_object_safe() {
        // Required for production CF Worker glue to inject the real impl
        // via `Arc<dyn KvPropagationProbe>`.
        let probes: Vec<Box<dyn KvPropagationProbe>> = vec![
            Box::new(InMemoryKvPropagationProbe::new()),
            Box::new(FailingKvPropagationProbe::new("x")),
        ];
        assert_eq!(probes.len(), 2);
    }

    #[test]
    fn fraction_within_typical_empty_samples_is_vacuous() {
        // Empty sample set → 1.0 (matches verifier's "no data → inconclusive,
        // not a violation" exit-code 2 contract).
        let f = fraction_within_typical(&[], 60.0);
        assert!((f - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fraction_within_typical_all_under_ceiling() {
        let samples = vec![
            KvPropagationSample {
                write_region: Region::Wnam,
                read_region: Region::Enam,
                lag_seconds: 5.0,
                probe_timestamp_ms: 0,
            },
            KvPropagationSample {
                write_region: Region::Weur,
                read_region: Region::Sam,
                lag_seconds: 30.0,
                probe_timestamp_ms: 0,
            },
        ];
        assert!((fraction_within_typical(&samples, 60.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fraction_within_typical_partial() {
        // 3 out of 4 under 60s → 0.75 (below 0.95 SLO target).
        let samples = vec![
            KvPropagationSample {
                write_region: Region::Wnam,
                read_region: Region::Enam,
                lag_seconds: 10.0,
                probe_timestamp_ms: 0,
            },
            KvPropagationSample {
                write_region: Region::Wnam,
                read_region: Region::Weur,
                lag_seconds: 20.0,
                probe_timestamp_ms: 0,
            },
            KvPropagationSample {
                write_region: Region::Weur,
                read_region: Region::Sam,
                lag_seconds: 50.0,
                probe_timestamp_ms: 0,
            },
            KvPropagationSample {
                write_region: Region::Sam,
                read_region: Region::Wnam,
                lag_seconds: 120.0, // OVER typical, under pessimistic
                probe_timestamp_ms: 0,
            },
        ];
        let f = fraction_within_typical(&samples, 60.0);
        assert!((f - 0.75).abs() < f64::EPSILON);
        assert!(f < KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION);
    }
}
