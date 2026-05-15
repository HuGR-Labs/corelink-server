//! R3-1 Adversarial — DPA NOT accepted → tier select returns
//! `TierError::DpaRequired`. Pins INV-ONBOARD-DPA-FIRST end-to-end.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_tier_selection::{TierError, TierKind};

use e2e_signup_flow::helpers::{
    make_test_tenant, setup_test_ledgers, tier_ctx_for, verify_audit_chain, ExpectedAuditEvent,
    ProvisionedTenant,
};

fn run() {
    let env = setup_test_ledgers();
    let tenant = make_test_tenant("dpa-missing");

    // Signup succeeds — DPA acceptance is a SEPARATE step.
    let resp = env.signup.provision(&tenant.signup_request).unwrap();
    let prov = ProvisionedTenant::from_response(resp).expect("provisioned");

    // Skip the DPA service call → tier ledger's DpaAcceptanceGate
    // returns `false` for `(tenant, v1.0.0)`.

    // Try Free tier — STILL must reject (WI §6.5: Free is NOT exempt).
    let err_free = env
        .tier
        .select_tier(
            &tier_ctx_for(&prov, env.now_ms),
            TierKind::Free,
            "u@example.com",
        )
        .unwrap_err();
    assert!(matches!(err_free, TierError::DpaRequired));

    // Try Starter — also rejected BEFORE Stripe is invoked.
    let err_starter = env
        .tier
        .select_tier(
            &tier_ctx_for(&prov, env.now_ms.saturating_add(1)),
            TierKind::Starter,
            "u@example.com",
        )
        .unwrap_err();
    assert!(matches!(err_starter, TierError::DpaRequired));
    // INV-ONBOARD-DPA-FIRST: zero Stripe sessions.
    assert!(env.stripe.sessions().is_empty());

    // Audit chain MUST carry the DPA-first violation marker.
    verify_audit_chain(
        &env,
        &[
            ExpectedAuditEvent::SignupStarted,
            ExpectedAuditEvent::SignupCompleted,
            ExpectedAuditEvent::TierAttempted,
            ExpectedAuditEvent::TierDpaFirstViolation,
        ],
    )
    .unwrap();
}

#[test]
fn r3_1_dpa_not_accepted_blocks_all_tiers() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|rt| rt.block_on(async { run() }))
        .unwrap();
}
