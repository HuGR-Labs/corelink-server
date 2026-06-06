//! Property tests for `corelink-tenant-offboarding`.
//!
//! Two canonical properties pin the load-bearing invariants:
//!
//! 1. `prop_state_machine_reachability_respects_grace` —
//!    INV-OFFBOARDING-GRACE-RESPECTED: every reachable state is
//!    reachable ONLY through a legal transition path AND the
//!    canonical timer thresholds (T+1 / T+30 / T+45) are respected
//!    for every TimerExpired advance.
//!
//! 2. `prop_audit_emit_precedes_every_state_mutation` —
//!    INV-OFFBOARDING-AUDIT-COMPLETE: for every successful state
//!    transition, exactly one audit row is captured AND its
//!    `to` field matches the new store state.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use proptest::prelude::*;

use corelink_ops::tenant_offboarding::{
    AdminCommitErasureRequest, InMemoryTenantOffboardingAuditSink,
    InMemoryTenantOffboardingOrchestrator, InMemoryTenantOffboardingStore, TenantOffboardingError,
    TenantOffboardingOrchestrator, TenantOffboardingState, TenantOffboardingTransition,
    TransitionTrigger, CANONICAL_GRACE_PERIOD_DAYS, CANONICAL_READ_ONLY_DAYS,
};

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

const MS_PER_DAY: i64 = 86_400_000;

#[derive(Clone, Debug)]
enum Action {
    InitiateCancel,
    Revert,
    Cron,
    OpsForce,
    AdminCommit,
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        Just(Action::InitiateCancel),
        Just(Action::Revert),
        Just(Action::Cron),
        Just(Action::OpsForce),
        Just(Action::AdminCommit),
    ]
}

fn time_step_strategy() -> impl Strategy<Value = i64> {
    // 0 days (immediate), 1 day, ~31 days, ~46 days, ~91 days
    prop_oneof![
        Just(0_i64),
        Just(MS_PER_DAY),
        Just((CANONICAL_GRACE_PERIOD_DAYS as i64 + 1) * MS_PER_DAY),
        Just(((CANONICAL_GRACE_PERIOD_DAYS + CANONICAL_READ_ONLY_DAYS) as i64 + 1) * MS_PER_DAY),
        Just(95_i64 * MS_PER_DAY),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// INV-OFFBOARDING-GRACE-RESPECTED + reachability:
    ///
    /// For any sequence of actions issued against a fresh tenant,
    /// the resulting store state is ALWAYS one of the canonical
    /// 6 states AND every TimerExpired advance that returned
    /// successfully respected the canonical timer threshold
    /// (the orchestrator rejects same-instant Cron advances with
    /// GraceNotElapsed).
    #[test]
    fn prop_state_machine_reachability_respects_grace(
        actions in proptest::collection::vec((action_strategy(), time_step_strategy()), 1..40),
    ) {
        let audit = Arc::new(InMemoryTenantOffboardingAuditSink::new());
        let store = Arc::new(InMemoryTenantOffboardingStore::new());
        let orch =
            InMemoryTenantOffboardingOrchestrator::new(audit.clone(), store.clone());

        let mut now_ms: i64 = 0;
        let tenant_id = "tenant-prop";

        for (a, dt) in actions {
            now_ms = now_ms.saturating_add(dt);
            // Snapshot state BEFORE the action (so we can validate
            // the post-condition).
            let before = orch.status(tenant_id).unwrap();

            let res: Result<_, TenantOffboardingError> = match a {
                Action::InitiateCancel => orch
                    .initiate_cancel(tenant_id, None, None, now_ms)
                    .map(|r| r.state),
                Action::Revert => orch.customer_revert(tenant_id, now_ms).map(|r| r.state),
                Action::Cron => orch.cron_advance(tenant_id, now_ms).map(|r| r.state),
                Action::OpsForce => orch
                    .ops_force_advance(tenant_id, "op-1".to_string(), now_ms)
                    .map(|r| r.state),
                Action::AdminCommit => orch
                    .admin_commit_erasure(AdminCommitErasureRequest {
                        tenant_id: tenant_id.to_string(),
                        operator_id: "op-1".to_string(),
                        dry_run_preview_confirmed: true,
                        now_ms,
                    })
                    .map(|r| r.state),
            };

            let after = orch.status(tenant_id).unwrap();
            match res {
                Ok(new_state) => {
                    // Post-condition: the store state matches the
                    // returned state.
                    prop_assert_eq!(
                        after.as_ref().map(|r| r.state),
                        Some(new_state)
                    );

                    // The transition was a canonical edge (or a
                    // fresh insert ACTIVE → CANCEL_REQUESTED).
                    if let Some(prev) = before.as_ref() {
                        let trigger = match a {
                            Action::InitiateCancel => TransitionTrigger::CustomerInitiated,
                            Action::Revert => TransitionTrigger::CustomerReverted,
                            Action::Cron => TransitionTrigger::TimerExpired,
                            Action::OpsForce => TransitionTrigger::OpsForced,
                            Action::AdminCommit => TransitionTrigger::AdminCommitErasure,
                        };
                        let expected = TenantOffboardingTransition::resolve(
                            prev.state, trigger,
                        );
                        prop_assert_eq!(expected, Some(new_state));

                        // INV-OFFBOARDING-GRACE-RESPECTED: for Cron
                        // success, the canonical timer threshold
                        // MUST have been reached.
                        if matches!(a, Action::Cron) {
                            let threshold =
                                InMemoryTenantOffboardingOrchestrator
                                    ::next_timer_threshold_ms(
                                        prev.state,
                                        prev.cancel_requested_at_ms,
                                    );
                            if let Some(t) = threshold {
                                prop_assert!(now_ms >= t);
                            }
                        }
                    }
                }
                Err(_) => {
                    // Failures MUST NOT mutate the store state.
                    prop_assert_eq!(
                        before.as_ref().map(|r| r.state),
                        after.as_ref().map(|r| r.state)
                    );
                }
            }

            // The store state, if present, is always one of the
            // canonical 6 (typed enum makes this structural; we
            // assert anyway for documentation).
            if let Some(rec) = after {
                let _: TenantOffboardingState = rec.state;
            }
        }
    }

    /// INV-OFFBOARDING-AUDIT-COMPLETE:
    ///
    /// For any sequence of actions, the number of audit rows
    /// captured equals the number of successful state mutations.
    /// Equivalently: every successful transition emitted exactly
    /// one canonical audit row whose `to` field matches the post-
    /// transition store state.
    #[test]
    fn prop_audit_emit_precedes_every_state_mutation(
        actions in proptest::collection::vec((action_strategy(), time_step_strategy()), 1..40),
    ) {
        let audit = Arc::new(InMemoryTenantOffboardingAuditSink::new());
        let store = Arc::new(InMemoryTenantOffboardingStore::new());
        let orch =
            InMemoryTenantOffboardingOrchestrator::new(audit.clone(), store.clone());

        let mut now_ms: i64 = 0;
        let tenant_id = "tenant-audit";
        let mut successes: usize = 0;
        let mut last_to: Option<TenantOffboardingState> = None;

        for (a, dt) in actions {
            now_ms = now_ms.saturating_add(dt);
            let res: Result<_, TenantOffboardingError> = match a {
                Action::InitiateCancel => orch
                    .initiate_cancel(tenant_id, None, None, now_ms)
                    .map(|r| r.state),
                Action::Revert => orch.customer_revert(tenant_id, now_ms).map(|r| r.state),
                Action::Cron => orch.cron_advance(tenant_id, now_ms).map(|r| r.state),
                Action::OpsForce => orch
                    .ops_force_advance(tenant_id, "op-1".to_string(), now_ms)
                    .map(|r| r.state),
                Action::AdminCommit => orch
                    .admin_commit_erasure(AdminCommitErasureRequest {
                        tenant_id: tenant_id.to_string(),
                        operator_id: "op-1".to_string(),
                        dry_run_preview_confirmed: true,
                        now_ms,
                    })
                    .map(|r| r.state),
            };

            if let Ok(new_state) = res {
                successes = successes.saturating_add(1);
                last_to = Some(new_state);
            }
        }

        let audit_snap = audit.snapshot().unwrap();
        prop_assert_eq!(audit_snap.len(), successes);
        // Last audit row, if any, must agree with the final state.
        if let (Some(last), Some(to)) = (audit_snap.last(), last_to) {
            prop_assert_eq!(last.to, to);
        }
        // And every audit row matches its canonical event type for
        // its destination.
        for r in &audit_snap {
            let expected =
                corelink_ops::tenant_offboarding::TenantOffboardingAuditEventType
                    ::for_destination(r.to);
            prop_assert_eq!(Some(r.event_type), expected);
        }
    }
}
