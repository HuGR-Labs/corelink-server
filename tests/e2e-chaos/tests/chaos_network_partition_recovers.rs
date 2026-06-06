//! R3-4 chaos drill — `net-cross-region` (NetworkPartition).
//!
//! Steady-state hypothesis: cross-region 30 s partition; reads served from
//! local replica; no data loss; replication catches up ≤ 60 s post-heal.
//!
//! Audit lifecycle: `corelink.chaos.run.started` → `.completed` (Passed).
//! SLO violation counter: NOT incremented (steady-state held).

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
fn chaos_network_partition_recovers() {
    let drill = ChaosE2eDrill::NetworkPartition;
    let exp = make_experiment(drill);

    // Charter: rollback ≤ 5 min.
    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::NetworkPartition);
    assert!(exp.rollback_seconds_max <= 60, "partition heal is fast");

    // Steady-state: configured impact within blast radius.
    let impact = exp.blast_radius_bps / 2;
    let (run, tele) = run_clean_staging(drill, "r3-4-net-partition-001", impact);

    // (1) Steady-state holds → Passed.
    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("steady-state breached unexpectedly: {other:?}"),
    }

    // (2) Audit lifecycle canonical.
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(
        events,
        vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]
    );

    // (3) SLO violation counter NOT incremented.
    assert_eq!(tele.slo_violation_total(), 0);
}
