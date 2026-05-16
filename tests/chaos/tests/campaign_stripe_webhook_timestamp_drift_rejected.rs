//! Scenario 6: Stripe webhook timestamp drift.
//!
//! Hypothesis: a webhook whose sender clock is skewed beyond the 300s
//! replay window is rejected, an audit event is emitted, and SEV-2
//! fires — even if the HMAC signature itself is otherwise valid.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignWebhookVerifier, WebhookOutcome,
};

#[test]
fn clock_drift_outside_window_rejects_webhook() {
    let mut v = CampaignWebhookVerifier::new();

    // Baseline — same clock, signed payload, accepted.
    assert_eq!(
        v.verify(true, 1_000_000, 1_000_000),
        WebhookOutcome::Accepted
    );

    // Inject sender-clock skew of 10 minutes (600s > 300s window).
    assert_eq!(
        v.verify(true, 1_000_000, 1_000_600),
        WebhookOutcome::OutsideReplayWindow,
        "drift > 300s must reject even with valid signature"
    );

    // (2) Audit event emitted.
    assert_audit_emitted_once(
        v.audit_events(),
        "corelink.billing.webhook.replay_window",
    )
    .unwrap();

    // (3) SEV-2 alert fired.
    assert_alert_fired(v.sev2_alerts(), "stripe_webhook_clock_skew").unwrap();
}

#[test]
fn invalid_signature_rejected_separately_from_clock_drift() {
    let mut v = CampaignWebhookVerifier::new();

    // Signed-not-ok always rejects regardless of drift.
    assert_eq!(
        v.verify(false, 1_000_000, 1_000_000),
        WebhookOutcome::SignatureMismatch
    );
    assert!(v.sev2_alerts().is_empty(), "signature-mismatch path is its own audit");
}
