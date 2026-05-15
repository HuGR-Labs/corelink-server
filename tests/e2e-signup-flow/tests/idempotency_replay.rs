//! R3-1 Idempotency — replay signup with the same `Idempotency-Key`
//! returns the SAME `signup_id` + zero side-effects (no second tenant
//! row, no second audit `started` event for the replay path).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_signup::{AtomicSignupStore, SignupAuditEventType, SignupOutcome};

use e2e_signup_flow::helpers::{make_test_tenant, setup_test_ledgers};

#[test]
fn r3_1_signup_idempotency_replay_returns_duplicate() {
    let env = setup_test_ledgers();
    let tenant = make_test_tenant("idem-replay");

    // First call.
    let r1 = env.signup.provision(&tenant.signup_request).unwrap();
    let original_signup_id = match r1.outcome {
        SignupOutcome::Provisioned { signup_id, .. } => signup_id,
        other => panic!("expected Provisioned, got {other:?}"),
    };

    // Replay with the SAME idempotency key.
    let r2 = env.signup.provision(&tenant.signup_request).unwrap();
    match r2.outcome {
        SignupOutcome::Duplicate { signup_id, .. } => {
            assert_eq!(
                signup_id, original_signup_id,
                "INV-S19-1 idempotency violated"
            );
        }
        other => panic!("expected Duplicate on replay, got {other:?}"),
    }

    // Exactly ONE atomic D1 commit happened (replay skipped the tx).
    assert_eq!(env.signup_store.committed_tenant_count().unwrap(), 1);

    // Audit chain: exactly ONE `Started` event (replay does NOT
    // re-emit; the original `started+completed` chain stands).
    let started = env
        .signup_audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == SignupAuditEventType::Started)
        .count();
    assert_eq!(started, 1, "replay must not re-emit signup.started");
}
