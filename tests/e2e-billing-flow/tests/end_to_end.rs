//! R3-5 end-to-end billing flow — five canonical scenarios that
//! exercise every cross-system gate in the signup → subscribe →
//! cancel → refund → DSR pipeline.
//!
//! ## Scenarios
//!
//! 1. `happy_path_starter_signup_to_refund` — signup →
//!    tier_select(Starter) → checkout webhook → invoice paid →
//!    cancel → refund.
//! 2. `idempotent_webhook_replay_dedups_to_single_activation` — a
//!    Stripe webhook delivered 3× hits the tier ledger but only the
//!    first delivery flips the subscription state.
//! 3. `cancel_while_active_preserves_access_until_period_end` —
//!    cancel mid-cycle; `has_access_at` returns `true` until
//!    `period_end_ms` and `false` afterwards.
//! 4. `dsr_erasure_blocked_while_subscription_active_then_allowed_after_refund`
//!    — DSR Erasure rejected while subscription is `Active` (canonical
//!    harness audit emitted); after cancel + refund, the SAME DSR
//!    Erasure request lands at the DSR endpoint with the canonical
//!    `RequestAccepted` arm.
//! 5. `refund_webhook_with_tampered_hmac_is_rejected` — a webhook
//!    whose payload was tampered AFTER signing yields
//!    `StripeError::SignatureRejected`; the canonical
//!    `corelink.billing_stripe.signature_rejected` audit row fires.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_billing_stripe::{
    StripeAdapterDecision, StripeAuditEventType, StripeError, WebhookEventKind,
};
use corelink_dsr::{DsrDecision, DsrRequestKind};
use corelink_signup::{AtomicSignupStore, SignupOutcome};
use corelink_tier_selection::{SubscriptionActivationReceipt, TierSelectionReceipt};

use e2e_billing_flow::{
    audit_chain_contains, build_signed_webhook, make_test_tenant, BillingHarness,
    BillingHarnessError, HarnessGateEvent, SubscriptionLifecycleState, FIXED_NOW_MS,
    FIXED_NOW_SECONDS,
};

// One canonical billing period = 30 days, used by cancel-mid-cycle
// scenario so `period_end_ms` is unambiguous.
const ONE_DAY_MS: u64 = 86_400_000;
const PERIOD_LENGTH_MS: u64 = 30 * ONE_DAY_MS;

// -------------------------------------------------------------------
// Scenario 1 — happy path
// -------------------------------------------------------------------

#[test]
fn r3_5_happy_path_starter_signup_to_refund() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("happy-flow");

    // --- (1) Signup + DPA accept ---
    let resp = harness.signup_and_accept_dpa(&tenant).unwrap();
    assert!(matches!(
        resp.outcome,
        SignupOutcome::Provisioned { .. }
    ));

    // --- (2) Tier select Starter (Stripe Checkout path) ---
    let receipt = harness.select_starter_tier(&tenant.name).unwrap();
    let session_id = match receipt {
        TierSelectionReceipt::CheckoutRedirect { session_id, .. } => session_id,
        other => panic!("expected CheckoutRedirect, got {other:?}"),
    };
    assert!(!session_id.is_empty());
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::PendingActivation
    ));
    // The Stripe fake should have recorded exactly one session.
    assert_eq!(harness.stripe_fake().sessions().len(), 1);

    // --- (3) Deliver checkout.session.completed webhook ---
    let activation = harness
        .deliver_checkout_completed(&tenant.name, "evt_happy_checkout_completed")
        .unwrap();
    assert!(matches!(
        activation,
        SubscriptionActivationReceipt::Activated { .. }
    ));
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::Active
    ));

    // --- (4) Deliver invoice.paid webhook through the canonical
    // signature-verify path. ---
    let payload = br#"{"id":"evt_happy_invoice_paid","type":"invoice.paid"}"#;
    let signed = build_signed_webhook(
        payload,
        "evt_happy_invoice_paid",
        WebhookEventKind::InvoicePaid,
        FIXED_NOW_SECONDS,
    )
    .unwrap();
    let decision = harness.deliver_signed_webhook(&signed).unwrap();
    assert!(matches!(
        decision,
        StripeAdapterDecision::WebhookProcessed { .. }
    ));

    // --- (5) Cancel mid-cycle ---
    let period_end_ms = FIXED_NOW_MS + PERIOD_LENGTH_MS;
    harness
        .cancel_subscription(&tenant.name, period_end_ms)
        .unwrap();
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { .. }
    ));

    // --- (6) Refund webhook lands ---
    let refund_payload =
        br#"{"id":"evt_happy_refund","type":"charge.refunded","data":{"refunded":true}}"#;
    let refund = build_signed_webhook(
        refund_payload,
        "evt_happy_refund",
        WebhookEventKind::SubscriptionUpdated,
        FIXED_NOW_SECONDS,
    )
    .unwrap();
    let refund_decision = harness.process_refund(&tenant.name, &refund).unwrap();
    assert!(matches!(
        refund_decision,
        StripeAdapterDecision::WebhookProcessed { .. }
    ));
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::Refunded
    ));

    // --- (7) Cross-system audit ordering ---
    // Tier audit chain: must contain TierSelectAttempted +
    // StripeSubscriptionActivated.
    let tier_events: Vec<_> = harness
        .tier_audit()
        .snapshot()
        .into_iter()
        .map(|r| r.event_type)
        .collect();
    assert!(tier_events
        .iter()
        .any(|e| matches!(
            e,
            corelink_tier_selection::TierSelectionAuditEventType::TierSelectAttempted
        )));
    assert!(tier_events.iter().any(|e| matches!(
        e,
        corelink_tier_selection::TierSelectionAuditEventType::StripeSubscriptionActivated
    )));

    // Webhook audit chain: each delivery records WebhookReceived +
    // SignatureVerified BEFORE the dispatch.
    let webhook_events: Vec<_> = harness
        .webhook_audit()
        .snapshot()
        .into_iter()
        .map(|r| r.event_type)
        .collect();
    let received = webhook_events
        .iter()
        .filter(|e| matches!(e, StripeAuditEventType::WebhookReceived))
        .count();
    let verified = webhook_events
        .iter()
        .filter(|e| matches!(e, StripeAuditEventType::SignatureVerified))
        .count();
    // Exactly two signed webhooks delivered (invoice.paid + refund).
    assert_eq!(received, 2);
    assert_eq!(verified, 2);

    // Harness-local gate log: cancel + refund both audited.
    let gates = harness.gate_log_snapshot();
    assert!(audit_chain_contains(&gates, |e| matches!(
        e,
        HarnessGateEvent::CancelScheduledAtPeriodEnd { .. }
    )));
    assert!(audit_chain_contains(&gates, |e| matches!(
        e,
        HarnessGateEvent::RefundProcessed { .. }
    )));
}

// -------------------------------------------------------------------
// Scenario 2 — idempotent webhook replay
// -------------------------------------------------------------------

#[test]
fn r3_5_idempotent_webhook_replay_dedups_to_single_activation() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("idem-replay");
    harness.signup_and_accept_dpa(&tenant).unwrap();
    let _ = harness.select_starter_tier(&tenant.name).unwrap();

    // First delivery — canonical activation.
    let first = harness
        .deliver_checkout_completed(&tenant.name, "evt_replay_canonical")
        .unwrap();
    assert!(matches!(
        first,
        SubscriptionActivationReceipt::Activated { .. }
    ));

    // Replay 1.
    let r1 = harness
        .replay_checkout_completed(&tenant.name, "evt_replay_canonical")
        .unwrap();
    // Replay 2.
    let r2 = harness
        .replay_checkout_completed(&tenant.name, "evt_replay_canonical")
        .unwrap();
    assert!(matches!(
        r1,
        SubscriptionActivationReceipt::DuplicateIgnored { .. }
    ));
    assert!(matches!(
        r2,
        SubscriptionActivationReceipt::DuplicateIgnored { .. }
    ));

    // Subscription state untouched by the replays.
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::Active
    ));

    // Tier audit chain: exactly ONE StripeSubscriptionActivated +
    // TWO StripeWebhookDuplicate rows.
    let tier_events: Vec<_> = harness
        .tier_audit()
        .snapshot()
        .into_iter()
        .map(|r| r.event_type)
        .collect();
    let activated = tier_events
        .iter()
        .filter(|e| matches!(
            e,
            corelink_tier_selection::TierSelectionAuditEventType::StripeSubscriptionActivated
        ))
        .count();
    let duplicates = tier_events
        .iter()
        .filter(|e| matches!(
            e,
            corelink_tier_selection::TierSelectionAuditEventType::StripeWebhookDuplicate
        ))
        .count();
    assert_eq!(activated, 1, "replay must not re-fire activation audit");
    assert_eq!(duplicates, 2, "each replay records a duplicate audit row");

    // Exactly one signup committed.
    assert_eq!(harness.signup_store().committed_tenant_count().unwrap(), 1);
}

// -------------------------------------------------------------------
// Scenario 3 — cancel-while-active preserves access until period_end
// -------------------------------------------------------------------

#[test]
fn r3_5_cancel_while_active_preserves_access_until_period_end() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("cancel-mid-cycle");
    harness.signup_and_accept_dpa(&tenant).unwrap();
    let _ = harness.select_starter_tier(&tenant.name).unwrap();
    let _ = harness
        .deliver_checkout_completed(&tenant.name, "evt_cancel_canonical")
        .unwrap();

    let period_end_ms = FIXED_NOW_MS + PERIOD_LENGTH_MS;
    harness
        .cancel_subscription(&tenant.name, period_end_ms)
        .unwrap();

    // Just-cancelled: still active mid-period.
    assert!(harness.has_access_at(&tenant.name, FIXED_NOW_MS));
    // Halfway through the period: still active.
    assert!(harness.has_access_at(&tenant.name, FIXED_NOW_MS + (PERIOD_LENGTH_MS / 2)));
    // One millisecond before the end: still active.
    assert!(harness.has_access_at(&tenant.name, period_end_ms - 1));
    // AT the period end: access flips off.
    assert!(!harness.has_access_at(&tenant.name, period_end_ms));
    // After period end: definitely off.
    assert!(!harness.has_access_at(&tenant.name, period_end_ms + ONE_DAY_MS));

    // Harness gate log captured the cancel scheduling.
    let gates = harness.gate_log_snapshot();
    let cancel_event_period_end = gates.iter().find_map(|e| match e {
        HarnessGateEvent::CancelScheduledAtPeriodEnd { period_end_ms, .. } => {
            Some(*period_end_ms)
        }
        _ => None,
    });
    assert_eq!(cancel_event_period_end, Some(period_end_ms));
}

// -------------------------------------------------------------------
// Scenario 4 — DSR erasure gated by subscription state
// -------------------------------------------------------------------

#[test]
fn r3_5_dsr_erasure_blocked_while_subscription_active_then_allowed_after_refund() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("dsr-while-subscribed");
    harness.signup_and_accept_dpa(&tenant).unwrap();
    let _ = harness.select_starter_tier(&tenant.name).unwrap();
    let _ = harness
        .deliver_checkout_completed(&tenant.name, "evt_dsr_active_canonical")
        .unwrap();

    // (a) DSR Erasure while Active: blocked at the harness gate.
    let err = harness
        .request_dsr_erasure(&tenant.name, "first-attempt", Some("ok"))
        .unwrap_err();
    let blocked_state = match err {
        BillingHarnessError::DsrBlockedSubscriptionActive { state } => state,
        other => panic!("expected DsrBlockedSubscriptionActive, got {other:?}"),
    };
    assert!(matches!(blocked_state, SubscriptionLifecycleState::Active));

    // Harness gate log recorded the rejection.
    let gates = harness.gate_log_snapshot();
    assert!(audit_chain_contains(&gates, |e| matches!(
        e,
        HarnessGateEvent::DsrErasureBlockedSubscriptionActive { .. }
    )));
    // DSR endpoint NEVER saw the request — its audit chain is empty.
    assert!(harness.dsr_audit().is_empty());

    // (b) DSR Erasure while CancelScheduled (pre-refund): still
    // blocked. The contract requires the refund to settle first.
    let period_end_ms = FIXED_NOW_MS + PERIOD_LENGTH_MS;
    harness
        .cancel_subscription(&tenant.name, period_end_ms)
        .unwrap();
    let err = harness
        .request_dsr_erasure(&tenant.name, "second-attempt", Some("ok"))
        .unwrap_err();
    assert!(matches!(
        err,
        BillingHarnessError::DsrBlockedSubscriptionActive {
            state: SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { .. }
        }
    ));

    // (c) Refund webhook lands → DSR now allowed.
    let refund_payload = br#"{"id":"evt_dsr_refund","type":"charge.refunded"}"#;
    let refund = build_signed_webhook(
        refund_payload,
        "evt_dsr_refund",
        WebhookEventKind::SubscriptionUpdated,
        FIXED_NOW_SECONDS,
    )
    .unwrap();
    let _ = harness.process_refund(&tenant.name, &refund).unwrap();
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::Refunded
    ));

    let decision = harness
        .request_dsr_erasure(&tenant.name, "post-refund", Some("ok"))
        .unwrap();
    assert!(decision.is_accepted(), "got: {decision:?}");

    // DSR audit chain now non-empty + contains the canonical
    // request_received row for the Erasure kind.
    let dsr_records = harness.dsr_audit().snapshot();
    assert!(!dsr_records.is_empty());
    assert!(dsr_records
        .iter()
        .any(|r| matches!(r.request_kind, DsrRequestKind::Erasure)));

    // (d) Without MFA token: canonical MfaRequired arm.
    let mfa = harness
        .request_dsr_erasure(&tenant.name, "post-refund-no-mfa", None)
        .unwrap();
    assert!(matches!(mfa, DsrDecision::MfaRequired { .. }));
}

// -------------------------------------------------------------------
// Scenario 5 — refund webhook with tampered HMAC is rejected
// -------------------------------------------------------------------

#[test]
fn r3_5_refund_webhook_with_tampered_hmac_is_rejected() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("tampered-refund");
    harness.signup_and_accept_dpa(&tenant).unwrap();
    let _ = harness.select_starter_tier(&tenant.name).unwrap();
    let _ = harness
        .deliver_checkout_completed(&tenant.name, "evt_tamper_canonical")
        .unwrap();
    harness
        .cancel_subscription(&tenant.name, FIXED_NOW_MS + PERIOD_LENGTH_MS)
        .unwrap();

    // Sign a refund payload; then tamper with the payload AFTER
    // signing so the HMAC no longer matches.
    let original = br#"{"id":"evt_tamper_refund","type":"charge.refunded","amount":100}"#;
    let mut signed = build_signed_webhook(
        original,
        "evt_tamper_refund",
        WebhookEventKind::SubscriptionUpdated,
        FIXED_NOW_SECONDS,
    )
    .unwrap();
    // Tamper: append a byte so the recomputed HMAC diverges.
    signed.payload.extend_from_slice(b"x");

    // The webhook handler must fail-CLOSED with SignatureRejected.
    let err = harness.deliver_signed_webhook(&signed).unwrap_err();
    let stripe_err = match err {
        BillingHarnessError::Stripe(e) => e,
        other => panic!("expected Stripe error, got {other:?}"),
    };
    assert!(
        matches!(stripe_err, StripeError::SignatureRejected(_)),
        "got: {stripe_err:?}"
    );

    // Canonical audit ordering: WebhookReceived BEFORE
    // SignatureRejected.
    let webhook_events: Vec<_> = harness
        .webhook_audit()
        .snapshot()
        .into_iter()
        .map(|r| r.event_type)
        .collect();
    let received_pos = webhook_events
        .iter()
        .position(|e| matches!(e, StripeAuditEventType::WebhookReceived));
    let rejected_pos = webhook_events
        .iter()
        .position(|e| matches!(e, StripeAuditEventType::SignatureRejected));
    assert!(received_pos.is_some());
    assert!(rejected_pos.is_some());
    assert!(received_pos.unwrap() < rejected_pos.unwrap(), "WebhookReceived MUST audit-emit BEFORE SignatureRejected");

    // The subscription state must NOT advance to Refunded — the
    // tampered webhook never landed.
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::CancelScheduledAtPeriodEnd { .. }
    ));
}

// -------------------------------------------------------------------
// Scenario 6 (bonus) — refund attempt without prior cancel is
// invariant-rejected at the harness boundary.
// -------------------------------------------------------------------

#[test]
fn r3_5_refund_without_cancel_is_invariant_rejected() {
    let harness = BillingHarness::setup();
    let tenant = make_test_tenant("refund-no-cancel");
    harness.signup_and_accept_dpa(&tenant).unwrap();
    let _ = harness.select_starter_tier(&tenant.name).unwrap();
    let _ = harness
        .deliver_checkout_completed(&tenant.name, "evt_inv_canonical")
        .unwrap();

    let refund_payload = br#"{"id":"evt_inv_refund","type":"charge.refunded"}"#;
    let refund = build_signed_webhook(
        refund_payload,
        "evt_inv_refund",
        WebhookEventKind::SubscriptionUpdated,
        FIXED_NOW_SECONDS,
    )
    .unwrap();
    let err = harness.process_refund(&tenant.name, &refund).unwrap_err();
    assert!(matches!(err, BillingHarnessError::Invariant(_)));
    // Subscription unchanged.
    assert!(matches!(
        harness.lifecycle_state(&tenant.name),
        SubscriptionLifecycleState::Active
    ));
}
