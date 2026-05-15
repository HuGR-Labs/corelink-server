//! R3-4 chaos drill — DNS failure with dual-resolver failover (post-GA
//! kind).
//!
//! Steady-state hypothesis: primary DNS NXDOMAIN 30 s; secondary resolver
//! serves traffic; no auth failure cascade.

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
fn chaos_dns_failure_dual_resolver_failover() {
    let drill = ChaosE2eDrill::DnsFailureDualResolverFailover;
    let exp = make_experiment(drill);

    assert_rollback_within_5min(&exp).unwrap();
    assert_eq!(exp.kind, ChaosKind::DnsFailure);

    let impact = exp.blast_radius_bps / 3;
    let (run, tele) = run_clean_staging(drill, "r3-4-dns-failover-001", impact);

    match run.outcome {
        ChaosOutcome::Passed { slo_impact_bps } => assert_eq!(slo_impact_bps, impact),
        other => panic!("expected Passed (failover successful), got {other:?}"),
    }
    let events = tele.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events, vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]);
    assert_eq!(tele.slo_violation_total(), 0);
}
