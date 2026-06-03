//! R3-4 chaos drill — sustained CPU pressure (post-GA kind).
//!
//! Steady-state hypothesis: sustained CPU pressure 60 s; P99 worker
//! latency ≤ baseline + 250 ms; no OOMK.

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
fn chaos_cpu_pressure_sustained() {
    let drill = ChaosE2eDrill::CpuPressureSustained;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::CpuPressure);

    // Steady-state hypothesis holds at impact = blast_radius - 1 (just inside).
    let impact = exp.blast_radius_bps.saturating_sub(1);
    let (run, tele) = run_clean_staging(drill, "r3-4-cpu-pressure-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed, got {other:?}"),
    }
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(
        events,
        vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]
    );
    assert_eq!(tele.slo_violation_total(), 0);

    // Adversarial: 1 bps over blast radius → SteadyStateBreached + counter increment.
    let breach_impact = exp.blast_radius_bps + 1;
    let (breach_run, breach_tele) =
        run_clean_staging(drill, "r3-4-cpu-pressure-002-breach", breach_impact);
    match breach_run.outcome {
        ChaosOutcome::SteadyStateBreached { reason } => {
            assert_eq!(reason, "blast_radius_exceeded");
        }
        other => panic!("expected SteadyStateBreached, got {other:?}"),
    }
    assert_eq!(breach_tele.slo_violation_total(), 1);
}
