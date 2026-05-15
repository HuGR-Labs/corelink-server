//! R3-4 chaos drill — R2 disk-fill near quota with quarantine (synthetic
//! ResourceExhaustion targeting r2).
//!
//! Steady-state hypothesis: R2 disk near 95 %; writes quarantined to
//! overflow tier; no data loss.

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
fn chaos_r2_disk_fill_quarantine() {
    let drill = ChaosE2eDrill::R2DiskFillQuarantine;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::ResourceExhaustion);
    assert_eq!(exp.target, "r2-bucket");

    let impact = exp.blast_radius_bps / 2;
    let (run, tele) = run_clean_staging(drill, "r3-4-r2-disk-fill-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed (quarantine), got {other:?}"),
    }

    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], ChaosAuditEvent::Started);
    assert_eq!(events[1], ChaosAuditEvent::Completed);
    assert_eq!(tele.slo_violation_total(), 0);
}
