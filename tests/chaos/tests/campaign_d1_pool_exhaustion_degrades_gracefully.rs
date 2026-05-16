//! Scenario 2: D1 connection pool exhaustion.
//!
//! Hypothesis: when every slot is taken by a slow neighbouring tenant,
//! new acquires return 503 with a `Retry-After` hint rather than
//! blocking the worker indefinitely. Partial-transaction visibility is
//! impossible because the acquire never resolves to a handle.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignD1Pool, D1AcquireOutcome,
};

#[test]
fn pool_exhaustion_returns_503_with_retry_after() {
    let mut pool = CampaignD1Pool::new(4);

    // Baseline — pool serves up to capacity.
    for _ in 0..4 {
        assert_eq!(pool.acquire(), D1AcquireOutcome::Acquired);
    }

    // Inject saturation: every slot taken.
    pool.inject_saturation();

    // (1) Fail CLOSED — 503 + Retry-After.
    match pool.acquire() {
        D1AcquireOutcome::Pool503 { retry_after_secs } => {
            assert!(
                retry_after_secs > 0,
                "Retry-After must be a positive hint"
            );
        }
        other => panic!("expected Pool503, got {other:?}"),
    }

    // (2) Audit event emitted.
    assert_audit_emitted_once(pool.audit_events(), "corelink.d1.pool.exhausted").unwrap();

    // (3) SEV-2 alert fired.
    assert_alert_fired(pool.sev2_alerts(), "d1_pool_saturated").unwrap();
}
