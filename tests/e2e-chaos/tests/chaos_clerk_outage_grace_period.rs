//! R3-4 chaos drill — Clerk auth provider outage during grace period
//! (post-GA AuthProviderUnavailable kind).
//!
//! Steady-state hypothesis: Clerk 503 for grace_period_s; cached sessions
//! remain valid; no logout storm or auth-invalid cascade.

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
fn chaos_clerk_outage_grace_period() {
    let drill = ChaosE2eDrill::ClerkOutageGracePeriod;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::AuthProviderUnavailable);
    assert_eq!(exp.target, "clerk");

    let impact = exp.blast_radius_bps / 4;
    let (run, tele) = run_clean_staging(drill, "r3-4-clerk-outage-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed (cached sessions held), got {other:?}"),
    }
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events, vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]);
    assert_eq!(tele.slo_violation_total(), 0);
}
