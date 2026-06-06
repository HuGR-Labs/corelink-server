//! Integration tests for the hot-blob replication-coverage SLI
//! (DEBT-011 P2-002).
//!
//! Mirrors the pattern of `sli_emit.rs`: cross-crate import surface +
//! deterministic in-memory SLI + audit-fail-CLOSED orthogonality assertion
//! (the SLI is best-effort; aggregation outcome is canonical).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use corelink_replica_worker::coverage::{
    compute_coverage_ratio, CoverageObservation, FailingHotBlobCoverageSli, HotBlobCoverageSli,
    InMemoryHotBlobCoverageSli, HOT_BLOB_COVERAGE_DEADLINE_SECONDS, HOT_BLOB_COVERAGE_TARGET_RATIO,
    METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO,
};
use corelink_replica_worker::Region;

#[test]
fn metric_name_is_load_bearing_canonical() {
    // LOAD-BEARING: dashboard panels + alerting rules in slo_catalog.md §4.28
    // anchor on this exact name.
    assert_eq!(
        METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO,
        "corelink_hot_blob_replication_coverage_ratio"
    );
}

#[test]
fn slo_constants_match_acceptance_criterion() {
    // Ticket P2-002 §2: target ≥ 0.90; deadline = 24h.
    assert!((HOT_BLOB_COVERAGE_TARGET_RATIO - 0.90).abs() < f64::EPSILON);
    assert_eq!(HOT_BLOB_COVERAGE_DEADLINE_SECONDS, 24 * 3600);
}

#[test]
fn ratio_helper_is_pure_and_deterministic() {
    // Three boundaries the alerting rule depends on:
    //   - empty window → 1.0 (vacuous truth; never pages)
    //   - exact 90% → meets target
    //   - 89% → breach
    //   - inversion → clamped 1.0 (defensive)
    assert_eq!(compute_coverage_ratio(0, 0), 1.0);
    assert!((compute_coverage_ratio(9, 10) - 0.9).abs() < f64::EPSILON);
    assert!((compute_coverage_ratio(89, 100) - 0.89).abs() < f64::EPSILON);
    assert_eq!(compute_coverage_ratio(11, 10), 1.0);
}

#[test]
fn inmemory_sli_records_per_region_observations() {
    // Acceptance §1: gauge per region; cardinality bound is 4 séries.
    let sli = InMemoryHotBlobCoverageSli::new();
    for (r, num, den) in [
        (Region::Wnam, 95_u64, 100_u64),
        (Region::Enam, 99, 100),
        (Region::Weur, 50, 100), // breach
        (Region::Sam, 100, 100),
    ] {
        sli.emit_coverage(r, num, den, 0).expect("emit ok");
    }
    let obs = sli.observations();
    assert_eq!(obs.len(), 4);
    assert!(obs[0].meets_target(), "wnam 95% meets target");
    assert!(obs[1].meets_target(), "enam 99% meets target");
    assert!(!obs[2].meets_target(), "weur 50% breaches target");
    assert!(obs[3].meets_target(), "sam 100% meets target");
}

#[test]
fn failing_sli_does_not_propagate_to_state_caller() {
    // Audit fail-CLOSED ordering reaffirmed: the SLI is best-effort; if it
    // fails, callers MUST NOT propagate as a state-mutation failure (see
    // module-level audit ordering comment in coverage.rs).
    //
    // Here we assert the trait-level Err surface so the caller has a clean
    // discriminator: it propagates to its audit-sink detail line and
    // continues — never blocks aggregation completion.
    let sli = FailingHotBlobCoverageSli::new("prometheus push failed");
    let r = sli.emit_coverage(Region::Wnam, 100, 100, 0);
    assert!(r.is_err());
    // Caller pattern (asserted at audit-emit ordering layer):
    //   if let Err(e) = sli.emit_coverage(...) { audit.detail("coverage_emit_failed: ..."); }
    //   // continue — aggregation outcome is unchanged.
}

#[test]
fn observation_struct_carries_full_provenance() {
    // The observation must carry numerator + denominator + computed ratio so
    // dashboard panels can both show "95/100 = 0.95" and post-hoc audits can
    // reconstruct the window without re-querying the source data.
    let sli = InMemoryHotBlobCoverageSli::new();
    sli.emit_coverage(Region::Wnam, 95, 100, 1_700_000_000_000)
        .expect("ok");
    let obs = sli.observations();
    let row: &CoverageObservation = &obs[0];
    assert_eq!(row.region, Region::Wnam);
    assert_eq!(row.classified_hot, 100);
    assert_eq!(row.replicated_within_24h, 95);
    assert!((row.ratio - 0.95).abs() < f64::EPSILON);
    assert_eq!(row.timestamp_ms, 1_700_000_000_000);
}

#[test]
fn coverage_sli_is_object_safe() {
    // Trait-abstraction-defer contract: the production CF Worker glue must be
    // able to inject any impl via `&dyn HotBlobCoverageSli`.
    let slis: Vec<Box<dyn HotBlobCoverageSli>> = vec![
        Box::new(InMemoryHotBlobCoverageSli::new()),
        Box::new(FailingHotBlobCoverageSli::new("x")),
    ];
    for sli in &slis {
        // metric_name() is the default impl on the trait — both must agree.
        assert_eq!(
            sli.metric_name(),
            METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO
        );
    }
    assert_eq!(slis.len(), 2);
}
