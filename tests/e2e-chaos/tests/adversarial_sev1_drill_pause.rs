//! R3-4 adversarial — **INV-S17-SEV1-DRILL-PAUSE** pinning.
//!
//! Spec contract §S-17:
//! > Runbook drills, chaos experiments, and DR drills are auto-deferred
//! > when a global SEV-1 incident is active. Enforced by `incident_active`
//! > flag check in scheduler `should_run()`; emits
//! > `corelink.ops.drill_deferred` audit event.
//!
//! At the runner level, the SEV-1 guard maps to
//! `SafeModeSnapshot::prod_sev1_active = true` which trips the
//! `prod_sev1_active` safe-mode trigger → `Aborted` outcome with no state
//! capture. The audit chain is exactly `[Aborted]` (single event; the
//! charter "audit-emit-BEFORE-mutation" inverse: no Started event ever
//! fires because the runner aborts before reaching the Started emit
//! point).
//!
//! We assert this against the GA-canonical `lat-r2-get` experiment to
//! pin the cross-WI invariant on a real catalog entry.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_chaos_scheduler::{
    run_experiment, ChaosAuditEvent, ChaosOutcome, ChaosRunId, ChaosTarget, SafeModeSnapshot,
};
use e2e_chaos::{
    assert_canonical_audit_sequence, make_experiment, ChaosE2eDrill, ChaosTestTelemetry,
};

#[test]
fn adversarial_sev1_active_auto_defers_chaos_drill() {
    let exp = make_experiment(ChaosE2eDrill::LatencyInjectionP99Bounded);
    let telemetry = ChaosTestTelemetry::new(100);

    let sev1_snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: true, // global SEV-1 in flight
        staging_error_rate_bps: 100,
    };
    let run = run_experiment(
        &exp,
        ChaosRunId::new("r3-4-sev1-defer-001"),
        sev1_snap,
        &telemetry,
    );

    match run.outcome {
        ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_sev1_active"),
        other => panic!("INV-S17-SEV1-DRILL-PAUSE violated: expected Aborted, got {other:?}"),
    }

    // Audit chain: exactly [Aborted]; no Started (drill never reaches runner body).
    let events = telemetry.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events, vec![ChaosAuditEvent::Aborted]);

    // Aborted runs do NOT increment the SLO violation counter (the breach
    // path is reserved for runs that completed but exceeded blast radius).
    assert_eq!(telemetry.slo_violation_total(), 0);
}

#[test]
fn adversarial_sev1_clears_drill_resumes() {
    // After SEV-1 clears (prod_sev1_active=false), the same experiment is
    // free to run normally (Passed outcome with the audit lifecycle pair).
    let exp = make_experiment(ChaosE2eDrill::LatencyInjectionP99Bounded);
    let telemetry = ChaosTestTelemetry::new(100);

    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: false,
        staging_error_rate_bps: 100,
    };
    let run = run_experiment(
        &exp,
        ChaosRunId::new("r3-4-sev1-resume-001"),
        snap,
        &telemetry,
    );
    assert!(matches!(run.outcome, ChaosOutcome::Passed { .. }));
    let events = telemetry.audit_events();
    assert_eq!(
        events,
        vec![ChaosAuditEvent::Started, ChaosAuditEvent::Completed]
    );
}
