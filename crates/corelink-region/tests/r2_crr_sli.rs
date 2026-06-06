//! R2 platform CRR indirect-lag SLI integration tests
//! (closes DEBT-011 R-PREP-REPL-P1-004).
//!
//! Covers:
//! 1. Metric name + cadence + 24 h ceiling match `slo_catalog.md §4.23 Target
//!    (CRR)` and the verifier's `PROM_METRIC["r2_crr"]` key.
//! 2. `R2CrrSample::within_ceiling()` requires BOTH `object_present == true`
//!    AND `lag_seconds <= 24h`. Missing object is never within ceiling
//!    regardless of elapsed time.
//! 3. `R2CrrSample::is_missing_incident()` fires only when object is missing
//!    AND elapsed ≥ 24 h (the SEV-2 paging threshold per §4.23 burn table).
//! 4. InMemory probe round-trips configured lag + presence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests use direct assertions"
)]

use corelink_region::r2_crr::{
    FailingR2CrrProbe, InMemoryR2CrrProbe, R2CrrProbe, R2CrrSample, METRIC_R2_CRR_LAG_SECONDS,
    R2_CRR_LAG_P99_CEILING_SECONDS, R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS,
    R2_CRR_PROBE_CADENCE_SECONDS,
};
use corelink_region::Region;

#[test]
fn metric_name_matches_verifier() {
    // LOAD-BEARING for `scripts/verify-replication-lag.py::PROM_METRIC["r2_crr"]`.
    assert_eq!(METRIC_R2_CRR_LAG_SECONDS, "corelink_r2_crr_lag_seconds");
}

#[test]
fn slo_constants_match_catalog_4_23() {
    assert_eq!(R2_CRR_LAG_P99_CEILING_SECONDS, 24 * 3600);
    assert_eq!(R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS, 24 * 3600);
    assert_eq!(
        R2_CRR_PROBE_CADENCE_SECONDS, 300,
        "5 min cadence per P1-004 §1"
    );
}

#[test]
fn missing_object_never_within_ceiling_even_at_zero_lag() {
    let s = R2CrrSample {
        primary_region: Region::Wnam,
        replica_region: Region::Enam,
        lag_seconds: 0.0,
        object_present: false,
        probe_timestamp_ms: 0,
    };
    assert!(
        !s.within_ceiling(),
        "missing object is never within ceiling"
    );
}

#[test]
fn present_object_within_24h_passes() {
    // The dominant nominal path — synthetic probe object replicated cleanly.
    let s = R2CrrSample {
        primary_region: Region::Wnam,
        replica_region: Region::Enam,
        lag_seconds: (12 * 3600) as f64,
        object_present: true,
        probe_timestamp_ms: 0,
    };
    assert!(s.within_ceiling());
    assert!(!s.is_missing_incident());
}

#[test]
fn missing_object_24h_threshold_triggers_incident() {
    let s = R2CrrSample {
        primary_region: Region::Wnam,
        replica_region: Region::Enam,
        lag_seconds: (24 * 3600) as f64,
        object_present: false,
        probe_timestamp_ms: 0,
    };
    assert!(
        s.is_missing_incident(),
        "missing + 24h crosses incident threshold"
    );
}

#[test]
fn missing_object_just_under_24h_not_incident() {
    let s = R2CrrSample {
        primary_region: Region::Wnam,
        replica_region: Region::Enam,
        lag_seconds: (24 * 3600 - 1) as f64,
        object_present: false,
        probe_timestamp_ms: 0,
    };
    assert!(
        !s.is_missing_incident(),
        "still under threshold — caught by burn-rate alert instead"
    );
}

#[test]
fn inmemory_probe_round_trip_sibling_pairs() {
    let p = InMemoryR2CrrProbe::new();
    // Inject deterministic lag for the two sibling pairs (the verifier
    // probes sibling pairs only for `r2_crr`).
    p.set_lag(Region::Wnam, Region::Enam, 100.0, true);
    p.set_lag(Region::Enam, Region::Wnam, 200.0, true);
    p.set_lag(Region::Weur, Region::Sam, 300.0, true);
    p.set_lag(Region::Sam, Region::Weur, 400.0, true);

    let samples: Vec<_> = [
        (Region::Wnam, Region::Enam),
        (Region::Enam, Region::Wnam),
        (Region::Weur, Region::Sam),
        (Region::Sam, Region::Weur),
    ]
    .iter()
    .map(|(p_, r_)| p.probe(*p_, *r_, 0).expect("probe"))
    .collect();

    assert_eq!(samples.len(), 4);
    assert!(samples.iter().all(R2CrrSample::within_ceiling));
    assert!(samples.iter().all(|s| s.object_present));
}

#[test]
fn inmemory_probe_simulates_missing_24h_incident() {
    let p = InMemoryR2CrrProbe::new();
    p.set_lag(Region::Wnam, Region::Enam, (25 * 3600) as f64, false);
    let s = p.probe(Region::Wnam, Region::Enam, 0).expect("probe");
    assert!(s.is_missing_incident(), "25h + missing → paging incident");
    assert!(!s.within_ceiling());
}

#[test]
fn failing_probe_returns_err_for_sev3_path() {
    let p = FailingR2CrrProbe::new("simulated R2 dashboard outage");
    let r = p.probe(Region::Wnam, Region::Enam, 0);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("simulated"));
}
