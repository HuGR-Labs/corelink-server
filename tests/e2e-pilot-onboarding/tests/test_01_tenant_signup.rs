//! Wave-23 pilot onboarding — Test 1: tenant signup.
//!
//! Exercises `POST /v1/signup` → Stripe-style checkout session → webhook
//! verification → tenant provisioning. Pins:
//!
//! - All four canonical audit events fire in canonical order.
//! - Audit chain is sound (each row's `prev_hash` ← predecessor's
//!   `row_hash`).
//! - Subscription transitions `None → CheckoutPending → Active` (audit
//!   emitted BEFORE every state flip; fail-CLOSED ordering).
//! - Duplicate signup is rejected with `SignupError::AlreadyProvisioned`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_pilot_onboarding::{
    canonical_pilot_tenant, AuditEventKind, PilotHarness, PilotHarnessError, SignupError,
    SubscriptionState, TenantLifecycleState,
};

#[test]
fn wave23_pilot_signup_full_journey() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-signup-happy");

    // Pre-signup: no state.
    assert!(h.lifecycle_state(&tenant).is_none());
    assert_eq!(h.subscription_state(&tenant), SubscriptionState::None);

    // Drive the full signup pipeline.
    let receipt = h.complete_signup(&tenant).unwrap();
    assert_eq!(receipt.tenant_id, tenant.tenant_id);
    assert_eq!(receipt.subscription_id, tenant.subscription_id);

    // Post-signup state.
    assert!(matches!(
        h.lifecycle_state(&tenant),
        Some(TenantLifecycleState::Active)
    ));
    assert_eq!(h.subscription_state(&tenant), SubscriptionState::Active);

    // Audit chain shape.
    let rows = h.audit_snapshot(&tenant);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].kind, AuditEventKind::SignupRequested);
    assert_eq!(rows[1].kind, AuditEventKind::CheckoutSessionCreated);
    assert_eq!(rows[2].kind, AuditEventKind::CheckoutWebhookVerified);
    assert_eq!(rows[3].kind, AuditEventKind::TenantProvisioned);

    // Chain integrity.
    let zero64 = "0".repeat(64);
    assert_eq!(rows[0].prev_hash_hex, zero64);
    for w in rows.windows(2) {
        assert_eq!(w[1].prev_hash_hex, w[0].row_hash_hex);
    }
}

#[test]
fn wave23_pilot_signup_duplicate_is_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-signup-dupe");
    h.complete_signup(&tenant).unwrap();
    let err = h.complete_signup(&tenant).unwrap_err();
    assert!(
        matches!(err, PilotHarnessError::Signup(SignupError::AlreadyProvisioned(_))),
        "got: {err:?}"
    );
    // Audit chain unchanged.
    assert_eq!(h.audit_snapshot(&tenant).len(), 4);
}
