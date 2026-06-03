//! R3-4 chaos drill — memory pressure with OOMK avoidance (post-GA kind).
//!
//! Steady-state hypothesis: memory pressure +N MiB; eviction policy fires
//! before OOM kill; no worker restarts.

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
fn chaos_memory_pressure_oomk_avoided() {
    let drill = ChaosE2eDrill::MemoryPressureOomkAvoided;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::MemoryPressure);

    let impact = exp.blast_radius_bps / 4;
    let (run, tele) = run_clean_staging(drill, "r3-4-mem-pressure-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed (OOMK avoided), got {other:?}"),
    }

    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(
        events,
        vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]
    );
    assert_eq!(tele.slo_violation_total(), 0);
}
