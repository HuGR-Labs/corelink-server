//! Integration tests for the Neon read-replica lag SLI (DEBT-011 P2-001).
//!
//! Mirrors the pattern of `d1_replica_lag_sli.rs` + `r2_crr_sli.rs`: cross-
//! crate import surface + deterministic in-memory probe + boundary-anchored
//! assertions on the canonical constants the verifier (`scripts/verify-
//! replication-lag.py --domain=neon`) consumes.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::assertions_on_constants
)]

use corelink_region::neon_replica_lag::{
    FailingNeonReplicaLagProbe, InMemoryNeonReplicaLagProbe, NeonReplicaLagProbe,
    NeonReplicaLagSample, METRIC_NEON_REPLICA_LAG_SECONDS, NEON_PROBE_CADENCE_SECONDS,
    NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS, NEON_SLO_IS_INFORMATIONAL,
};
use corelink_region::Region;

/// Verifier compatibility — the Python verifier's `PROM_METRIC["neon"]` MUST
/// match this canonical Rust constant, otherwise a rename on either side
/// silently breaks the daily cron.
#[test]
fn metric_name_matches_verifier_canonical_key() {
    assert_eq!(
        METRIC_NEON_REPLICA_LAG_SECONDS,
        "corelink_neon_replica_lag_seconds",
        "verifier key mismatch — both Rust and scripts/verify-replication-lag.py must rename together"
    );
}

#[test]
fn slo_is_informational_at_ga() {
    // Per ticket P2-001 §2 + slo_catalog §4.27: soft / informational, no
    // alerting at GA. Toggling this flag requires an ADR.
    assert!(NEON_SLO_IS_INFORMATIONAL);
    assert_eq!(NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS, 5);
    assert_eq!(NEON_PROBE_CADENCE_SECONDS, 60);
}

#[test]
fn deterministic_probe_round_trip() {
    // Acceptance §1: probe writes to primary, reads from each replica.
    // The in-memory probe round-trips a deterministic per-(primary, replica)
    // lag with strict equality (no fuzzing).
    let probe = InMemoryNeonReplicaLagProbe::new();
    probe.set_lag(Region::Enam, Region::Weur, 0.8);
    probe.set_lag(Region::Enam, Region::Sam, 1.5);
    probe.set_lag(Region::Enam, Region::Wnam, 2.3);
    for (replica, expected) in [
        (Region::Weur, 0.8_f64),
        (Region::Sam, 1.5),
        (Region::Wnam, 2.3),
    ] {
        let s = probe
            .probe(Region::Enam, replica, 1_700_000_000_000)
            .expect("probe ok");
        assert_eq!(s.primary_region, Region::Enam);
        assert_eq!(s.replica_region, replica);
        assert!((s.lag_seconds - expected).abs() < 1e-9);
        assert!(s.within_soft_ceiling());
    }
}

#[test]
fn soft_ceiling_breach_is_observable_but_not_blocking() {
    // 12 s > 5 s soft ceiling — the SLI MUST record the sample (so a
    // dashboard panel surfaces it) but MUST NOT propagate an error (no
    // alerting at GA per ticket P2-001).
    let probe = InMemoryNeonReplicaLagProbe::new();
    probe.set_lag(Region::Enam, Region::Sam, 12.0);
    let s = probe.probe(Region::Enam, Region::Sam, 0).expect("ok");
    assert!(!s.within_soft_ceiling());
    // The trait's `probe()` returns Ok — the breach is observable via the
    // sample's `within_soft_ceiling()` check, not via an Err result.
}

#[test]
fn failing_probe_propagates_error_for_sev3_path() {
    // Acceptance §1 errors path: backend failure → caller emits SEV-3
    // informational. The trait must return Err so the cron can ladder.
    let p = FailingNeonReplicaLagProbe::new("neon platform 5xx");
    let r = p.probe(Region::Enam, Region::Weur, 0);
    assert!(r.is_err());
    let msg = r.unwrap_err();
    assert!(msg.contains("neon"), "actionable error contains 'neon': {msg}");
}

#[test]
fn sample_serde_roundtrip() {
    // The verifier and the audit chain serialize samples — ensure the
    // canonical JSON shape is stable.
    let s = NeonReplicaLagSample {
        primary_region: Region::Enam,
        replica_region: Region::Weur,
        lag_seconds: 0.42,
        sample_timestamp_ms: 1_700_000_000_000,
    };
    let j = serde_json::to_string(&s).expect("ser");
    let back: NeonReplicaLagSample = serde_json::from_str(&j).expect("de");
    assert_eq!(back, s);
}
