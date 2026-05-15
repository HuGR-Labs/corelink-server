//! R3-1 Adversarial — Stripe webhook signature invalid → 400 + no
//! state mutation. Pins the canonical HMAC-SHA256 + replay-window
//! verification path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_tier_selection::{
    compute_stripe_signature, verify_stripe_signature, StripeCheckoutSessionCompletedEvent,
    StripeCustomerId, SubscriptionState, TenantId as TierTenantId, TierError, TierKind,
};

use e2e_signup_flow::helpers::{
    dpa_ctx_for, dpa_request_for, make_test_tenant, setup_test_ledgers, tier_ctx_for,
    ProvisionedTenant,
};
use e2e_signup_flow::TEST_WEBHOOK_SECRET;

#[test]
fn r3_1_webhook_signature_mismatch_rejected_no_mutation() {
    let env = setup_test_ledgers();
    let tenant = make_test_tenant("wh-bad-sig");
    let resp = env.signup.provision(&tenant.signup_request).unwrap();
    let prov = ProvisionedTenant::from_response(resp).expect("provisioned");

    // DPA acceptance + tier select Starter → Checkout pending.
    let _ = env
        .dpa
        .accept(
            &dpa_ctx_for(&prov, "sig-wh-bad"),
            dpa_request_for(&env, corelink_dpa_acceptance::LocaleBcp47::EnUs),
        )
        .unwrap();
    env.dpa_gate
        .accept(TierTenantId::new(prov.tenant_id.as_str()), "1.0.0");
    let _ = env
        .tier
        .select_tier(
            &tier_ctx_for(&prov, env.now_ms),
            TierKind::Starter,
            "u@example.com",
        )
        .unwrap();

    let payload = br#"{"id":"evt_x","type":"checkout.session.completed"}"#;
    let ts = 1_700_000_000_u64;
    // Bad signature: HMAC computed under a DIFFERENT secret →
    // verification rejects.
    let wrong_sig = compute_stripe_signature(b"whsec_wrong", ts, payload);
    let bad_header = format!("t={ts},v1={wrong_sig}");
    let err =
        verify_stripe_signature(TEST_WEBHOOK_SECRET, &bad_header, payload, ts * 1000).unwrap_err();
    assert!(matches!(err, TierError::InvalidSignature(_)));

    // Critically: we did NOT call `on_checkout_completed`; the
    // signature failure short-circuited at the boundary. State is
    // still `PendingCheckout` — no spurious activation.
    let row = env.tier.row(&TierTenantId::new(prov.tenant_id.as_str()));
    let state = row.map(|r| r.subscription_state).unwrap_or(SubscriptionState::Inactive);
    assert_eq!(state, SubscriptionState::PendingCheckout);

    // Sanity: a correctly-signed payload would verify.
    let good_sig = compute_stripe_signature(TEST_WEBHOOK_SECRET, ts, payload);
    let good_header = format!("t={ts},v1={good_sig}");
    verify_stripe_signature(TEST_WEBHOOK_SECRET, &good_header, payload, ts * 1000).unwrap();

    // And ALSO: replay-out-of-window rejected even with valid sig.
    let stale_now = (ts + 10 * 60) * 1000; // 10 min later
    let err =
        verify_stripe_signature(TEST_WEBHOOK_SECRET, &good_header, payload, stale_now).unwrap_err();
    assert!(matches!(err, TierError::InvalidSignature(_)));

    // The webhook activation path is reachable only with a valid sig.
    let event = StripeCheckoutSessionCompletedEvent::new(
        "evt_e2e_valid",
        "cs_fake_00000001",
        TierTenantId::new(prov.tenant_id.as_str()),
        TierKind::Starter,
        StripeCustomerId::new(format!("cus_fake_{}", prov.tenant_id)),
        env.now_ms.saturating_add(1_000),
    );
    let _ = env.tier.on_checkout_completed(&event).unwrap();
    let row = env.tier.row(&TierTenantId::new(prov.tenant_id.as_str()));
    assert_eq!(
        row.unwrap().subscription_state,
        SubscriptionState::Active
    );
}
