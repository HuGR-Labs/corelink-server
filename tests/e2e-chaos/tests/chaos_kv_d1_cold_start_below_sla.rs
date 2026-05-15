//! R3-4 chaos drill — KV + D1 cold start latency below SLA (post-GA
//! ResourceExhaustion kind targeting kv-d1).
//!
//! Steady-state hypothesis: KV + D1 cold-start; P99 cold-start latency
//! ≤ 800 ms (SLA bound); no client errors.

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
fn chaos_kv_d1_cold_start_below_sla() {
    let drill = ChaosE2eDrill::KvD1ColdStartBelowSla;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::ResourceExhaustion);
    assert_eq!(exp.target, "kv-d1");

    let impact = exp.blast_radius_bps / 2;
    let (run, tele) = run_clean_staging(drill, "r3-4-kv-d1-cold-start-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed (cold-start under SLA), got {other:?}"),
    }
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events, vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]);
    assert_eq!(tele.slo_violation_total(), 0);
}
