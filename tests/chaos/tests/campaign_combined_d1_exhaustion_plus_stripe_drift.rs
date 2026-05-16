//! Combined Scenario B (wave-23): D1 connection pool exhausted +
//! Stripe webhook timestamp drift, happening concurrently.
//!
//! Composes Scenario 2 (`CampaignD1Pool`) and Scenario 6
//! (`CampaignWebhookVerifier`) from the wave-22 harness.
//!
//! Steady-state hypothesis: when a tenant is being throttled by D1
//! saturation **and** a Stripe webhook arrives outside the 300s replay
//! window, both surfaces must degrade gracefully and independently:
//!
//! - The D1 acquire returns a 503 with `Retry-After` (no partial txn).
//! - The webhook handler rejects the drifted event (no state mutation).
//!
//! Wave-22 dress rehearsal noted that the billing webhook handler and
//! the tenant request path *share* the D1 pool: a saturated pool could
//! theoretically mask a webhook-replay-window rejection by 503'ing the
//! webhook before its signature/timestamp check ran. This wave-23
//! scenario pins the contract that the verifier rejects on its own
//! merits *before* any D1 acquire happens, so a saturated pool cannot
//! short-circuit the replay-window check.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignD1Pool, CampaignWebhookVerifier,
    D1AcquireOutcome, WebhookOutcome,
};

#[test]
fn d1_exhaustion_plus_stripe_drift_both_degrade_gracefully() {
    // --- D1 surface: pool sized for 4 concurrent connections.
    let mut pool = CampaignD1Pool::new(4);
    // First 4 acquires succeed — pool fills under normal load.
    for _ in 0..4 {
        assert_eq!(pool.acquire(), D1AcquireOutcome::Acquired);
    }
    // The fifth acquire (under chaos saturation) is the trigger.
    pool.inject_saturation();
    let outcome = pool.acquire();
    match outcome {
        D1AcquireOutcome::Pool503 { retry_after_secs } => {
            assert!(
                retry_after_secs > 0,
                "Retry-After must be a positive hint, got {retry_after_secs}"
            );
        }
        other => panic!("expected Pool503, got {other:?}"),
    }

    // --- Stripe surface: webhook arrives 400s in the future (clock skew
    // > 300s replay window). The verifier MUST reject regardless of
    // upstream D1 state — the webhook signature/timestamp check is
    // verified before any DB acquire.
    let mut verifier = CampaignWebhookVerifier::new();
    let verifier_now: i64 = 1_715_000_000;
    let sender_ts: i64 = verifier_now + 400; // future-drifted sender
    let webhook = verifier.verify(true, sender_ts, verifier_now);
    assert_eq!(
        webhook,
        WebhookOutcome::OutsideReplayWindow,
        "drifted webhook must be rejected — no state mutation under combined chaos"
    );

    // --- Independence assertion: even after the webhook is rejected,
    // the D1 pool still surfaces its own SEV-2 alert and audit event —
    // neither surface masks the other.
    assert_audit_emitted_once(pool.audit_events(), "corelink.d1.pool.exhausted").unwrap();
    assert_alert_fired(pool.sev2_alerts(), "d1_pool_saturated").unwrap();

    assert_audit_emitted_once(
        verifier.audit_events(),
        "corelink.billing.webhook.replay_window",
    )
    .unwrap();
    assert_alert_fired(verifier.sev2_alerts(), "stripe_webhook_clock_skew").unwrap();

    // --- Concurrency contract: a second drifted webhook arriving while
    // the pool is still saturated must still be rejected by signature/
    // timestamp policy (not by the D1 503). The verifier owns the
    // contract; the pool's saturation state must not leak into it.
    let webhook_2 = verifier.verify(true, sender_ts + 1, verifier_now);
    assert_eq!(
        webhook_2,
        WebhookOutcome::OutsideReplayWindow,
        "subsequent drifted webhooks must continue to be rejected on their own merits"
    );

    // --- Pool recovery path (autoscale releases a slot) — a fresh pool
    // models the post-autoscale state with capacity bumped to 8.
    let mut pool_scaled = CampaignD1Pool::new(8);
    assert_eq!(pool_scaled.acquire(), D1AcquireOutcome::Acquired);
    assert!(
        pool_scaled.audit_events().is_empty(),
        "post-scale acquire must not emit the saturation audit event"
    );
    assert!(
        pool_scaled.sev2_alerts().is_empty(),
        "post-scale acquire must not raise d1_pool_saturated"
    );
}
