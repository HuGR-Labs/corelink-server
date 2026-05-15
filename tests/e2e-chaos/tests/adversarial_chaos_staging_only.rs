//! R3-4 adversarial — **INV-S17-CHAOS-STAGING-ONLY** pinning.
//!
//! Spec contract §S-17:
//! > Chaos experiments must NEVER target production environment. Enforced
//! > by `DrillEnv::require_staging()` (corelink-chaos-scheduler) AND
//! > `[env.prod]` wrangler config omitting `[triggers]`. Audit
//! > fail-CLOSED with `Aborted{reason="prod_target"}` if attempted.
//!
//! Runner-level enforcement is the first line of `run_experiment`:
//! `target != Staging` → `Aborted{trigger:"prod_target_violation"}` +
//! single `corelink.chaos.run.aborted` audit event. NO state capture
//! writes (no `capture_state_pre` / `capture_state_post` /
//! `measure_slo_impact` calls).

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
fn adversarial_prod_target_rejected_pre_flight() {
    let exp = make_experiment(ChaosE2eDrill::LatencyInjectionP99Bounded);
    let telemetry = ChaosTestTelemetry::new(100);

    let prod_snap = SafeModeSnapshot {
        target: ChaosTarget::Production, // hard rule violation
        prod_sev1_active: false,
        staging_error_rate_bps: 0,
    };

    let run = run_experiment(
        &exp,
        ChaosRunId::new("r3-4-staging-only-001"),
        prod_snap,
        &telemetry,
    );

    match run.outcome {
        ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_target_violation"),
        other => panic!("INV-S17-CHAOS-STAGING-ONLY violated: {other:?}"),
    }
    assert_eq!(run.target, ChaosTarget::Production);

    // Audit chain: exactly [Aborted].
    let events = telemetry.audit_events();
    assert_canonical_audit_sequence(&events).unwrap();
    assert_eq!(events, vec![ChaosAuditEvent::Aborted]);

    // SLO counter NOT incremented (run aborted before completion).
    assert_eq!(telemetry.slo_violation_total(), 0);
}

#[test]
fn adversarial_prod_target_highest_priority_over_sev1() {
    // If both prod-target AND prod-sev1 are set, the prod_target trigger
    // wins (it is the highest-priority safe-mode trip). This locks the
    // documented runner order so reordering would be a regression.
    let exp = make_experiment(ChaosE2eDrill::NetworkPartition);
    let telemetry = ChaosTestTelemetry::new(0);
    let dual_snap = SafeModeSnapshot {
        target: ChaosTarget::Production,
        prod_sev1_active: true,
        staging_error_rate_bps: 9_999,
    };
    let run = run_experiment(
        &exp,
        ChaosRunId::new("r3-4-staging-only-002"),
        dual_snap,
        &telemetry,
    );
    match run.outcome {
        ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_target_violation"),
        other => panic!("expected prod_target_violation priority, got {other:?}"),
    }
}
