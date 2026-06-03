//! R3-4 chaos drill — `lat-r2-get` (LatencyInjection; GA catalog).
//!
//! Steady-state hypothesis: P99 GET latency ≤ baseline + 500 ms; error
//! rate unchanged.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_chaos_scheduler::{ChaosAuditEvent, ChaosKind, ChaosOutcome};
use e2e_chaos::{
    assert_canonical_audit_sequence, assert_rollback_within_5min, make_experiment,
    run_clean_staging, ChaosE2eDrill,
};

#[test]
fn chaos_latency_injection_p99_bounded() {
    let drill = ChaosE2eDrill::LatencyInjectionP99Bounded;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::LatencyInjection);
    assert_eq!(exp.id.as_str(), "lat-r2-get");
    assert_eq!(exp.blast_radius_bps, 500);

    // P99 bounded: impact stays under 500 bps blast radius.
    let impact = 300;
    let (run, tele) = run_clean_staging(drill, "r3-4-lat-r2-get-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, 300),
        other => panic!("expected Passed (P99 bounded), got {other:?}"),
    }
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(
        events,
        vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]
    );
    assert_eq!(tele.slo_violation_total(), 0);

    // Boundary: impact exactly at blast_radius_bps still Passes.
    let (run_boundary, tele_boundary) =
        run_clean_staging(drill, "r3-4-lat-r2-get-boundary", exp.blast_radius_bps);
    assert!(matches!(run_boundary.outcome, ChaosOutcome::Passed { .. }));
    assert_eq!(tele_boundary.slo_violation_total(), 0);
}
