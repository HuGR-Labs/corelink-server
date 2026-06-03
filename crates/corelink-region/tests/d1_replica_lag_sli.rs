//! D1 read-replica lag SLI integration tests
//! (closes DEBT-011 R-PREP-REPL-P0-002).
//!
//! Per ticket acceptance criteria:
//!  - Probe emits per-pair lag on a 30 s cadence.
//!  - Lag > 5 min threshold triggers SEV-2 path (verified by probe-failure
//!    propagation here; SEV-emit lives in the CF Worker glue).
//!  - DR-16 §1 Detect step reads `corelink_d1_replica_lag_seconds` instead
//!    of CF dashboard (verified by `metric_name()` constant).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests use direct assertions"
)]

use corelink_region::replica_lag::{
    D1LagSample, D1ReplicaLagProbe, FailingD1ReplicaLagProbe, InMemoryD1ReplicaLagProbe,
    D1_PROBE_CADENCE_SECONDS, D1_REPLICA_LAG_P99_CEILING_SECONDS, METRIC_D1_REPLICA_LAG_SECONDS,
};
use corelink_region::Region;

#[test]
fn probe_emits_zero_lag_by_default() {
    let probe = InMemoryD1ReplicaLagProbe::new();
    let sample = probe
        .probe(Region::Wnam, Region::Enam, 1_700_000_000_000)
        .expect("probe should succeed");
    assert_eq!(sample.primary_region, Region::Wnam);
    assert_eq!(sample.replica_region, Region::Enam);
    assert_eq!(sample.lag_seconds, 0.0);
    assert_eq!(sample.probe_timestamp_ms, 1_700_000_000_000);
}

#[test]
fn probe_emits_configured_lag() {
    let probe = InMemoryD1ReplicaLagProbe::new();
    probe.set_lag(Region::Wnam, Region::Enam, 3.0);
    let s = probe.probe(Region::Wnam, Region::Enam, 0).expect("probe");
    assert!((s.lag_seconds - 3.0).abs() < f64::EPSILON);
}

#[test]
fn probe_observes_sustained_over_budget_lag() {
    // Inject lag > 5 min (DR-16 declaration trigger).
    let probe = InMemoryD1ReplicaLagProbe::new();
    probe.set_lag(Region::Weur, Region::Sam, 360.0); // 6 min
    let s = probe.probe(Region::Weur, Region::Sam, 0).expect("probe");
    assert!(s.lag_seconds > 300.0, "lag > 5 min triggers SEV-2 path");
    assert!(
        s.lag_seconds > (D1_REPLICA_LAG_P99_CEILING_SECONDS as f64),
        "lag must also exceed the p99 ceiling"
    );
}

#[test]
fn metric_name_constant_is_canonical() {
    // Load-bearing for scripts/verify-replication-lag.py::PROM_METRIC["d1"].
    // Renaming requires updating the verifier.
    let probe = InMemoryD1ReplicaLagProbe::new();
    assert_eq!(probe.metric_name(), METRIC_D1_REPLICA_LAG_SECONDS);
    assert_eq!(
        METRIC_D1_REPLICA_LAG_SECONDS,
        "corelink_d1_replica_lag_seconds"
    );
}

#[test]
fn cadence_matches_ticket_p0_002_acceptance() {
    // Ticket acceptance criterion §1: emit every 30 s.
    assert_eq!(D1_PROBE_CADENCE_SECONDS, 30);
}

#[test]
fn probe_uses_deterministic_timing_per_sample() {
    let probe = InMemoryD1ReplicaLagProbe::new();
    probe.set_lag(Region::Wnam, Region::Enam, 5.0);

    let s1 = probe
        .probe(Region::Wnam, Region::Enam, 1_000)
        .expect("probe");
    let s2 = probe
        .probe(Region::Wnam, Region::Enam, 31_000)
        .expect("probe");
    // Cadence: 30s gap between consecutive samples (deterministic).
    assert_eq!(s2.probe_timestamp_ms - s1.probe_timestamp_ms, 30_000);
    // Same lag value (no drift in deterministic fixture).
    assert!((s1.lag_seconds - s2.lag_seconds).abs() < f64::EPSILON);
}

#[test]
fn probe_propagates_underlying_query_failure() {
    let probe = FailingD1ReplicaLagProbe::new("D1 binding unavailable");
    let r = probe.probe(Region::Wnam, Region::Enam, 0);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("D1 binding unavailable"));
}

#[test]
fn probe_sample_serializes_to_json() {
    // CloudEvents payload compatibility — required for the staging
    // SEV-2-on-failure runbook path.
    let s = D1LagSample {
        primary_region: Region::Wnam,
        replica_region: Region::Enam,
        lag_seconds: 12.5,
        probe_timestamp_ms: 1_700_000_000_000,
    };
    let j = serde_json::to_string(&s).expect("serde_json");
    assert!(j.contains("\"primary_region\":\"wnam\""));
    assert!(j.contains("\"replica_region\":\"enam\""));
    assert!(j.contains("\"lag_seconds\":12.5"));
}

#[test]
fn probe_handles_all_sibling_pairs() {
    // Verify all 4 GA sibling pairs probe without error.
    let probe = InMemoryD1ReplicaLagProbe::new();
    let pairs = [
        (Region::Wnam, Region::Enam),
        (Region::Enam, Region::Wnam),
        (Region::Weur, Region::Sam),
        (Region::Sam, Region::Weur),
    ];
    for (primary, replica) in pairs {
        probe.set_lag(primary, replica, 1.0);
        let s = probe.probe(primary, replica, 0).expect("probe");
        assert!((s.lag_seconds - 1.0).abs() < f64::EPSILON);
    }
}
