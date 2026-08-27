//! WP-M (P2) — Token-bucket EVICTION-RESET bypass (proof + regression pin).
//!
//! The briefing protocol demands: PROVE the bypass vector exists before
//! implementing the fix; if the test cannot demonstrate the bypass,
//! report and STOP. We did the proof first against an UNPATCHED limiter
//! (this test failed to compile + the existing `bounded_map_resists_oci_flood`
//! was the only coverage). This test pins the **regression**: a re-materialise
//! of a previously-drained bucket must NOT restore the burst budget
//! (`burst_capacity` tokens) — it must be born with 0 tokens and rely on
//! the standard lazy-refill formula to climb back.
//!
//! If the test ever starts passing (bypass demonstrated), the fix has
//! regressed — fail-CLOSED the PR.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use std::sync::Arc;

use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    KeyDimension, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use uuid::Uuid;

const ATTACKER_TENANT: Uuid = Uuid::from_u128(0xa);
const FLOOD_TENANT: Uuid = Uuid::from_u128(0xc);

fn limiter_with_cap(cap: usize) -> InMemoryTokenBucketRateLimiter<
    InMemoryRateLimitAuditSink,
    InMemoryRateLimitMetrics,
> {
    InMemoryTokenBucketRateLimiter::with_bucket_map_cap_for_test(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        RateLimitConfig::canonical(),
        cap,
    )
}

fn victim_key() -> corelink_ratelimit::BucketKey {
    corelink_ratelimit::BucketKey {
        tenant_id: ATTACKER_TENANT,
        dimension: KeyDimension::PerTenant,
        scope_key: String::new(),
    }
}

fn flood_key(n: u32) -> corelink_ratelimit::BucketKey {
    corelink_ratelimit::BucketKey {
        tenant_id: FLOOD_TENANT,
        dimension: KeyDimension::PerTenantPerEndpoint,
        scope_key: format!("flood/{n}"),
    }
}

fn is_allow(d: &RateLimitDecision) -> bool {
    matches!(d, RateLimitDecision::Allow { .. })
}

/// The full 5-step attack from the briefing:
/// 1. Attacker drains their own bucket (burst+1 calls; the last is a Deny429).
/// 2. Attacker STOPS touching that key (no further updates to its
///    `last_access`).
/// 3. Attacker floods the map with `(cap * 4) - 1` distinct keys under a
///    different tenant; each is an `Allow` because re-materialising an
///    UNSEEN key returns a fresh bucket. The map saturates and any
///    further distinct key triggers `evict_if_at_cap`.
/// 4. The approximate-LRU eviction (sample K=8) of the flood eventually
///    picks the drained key as the oldest `last_access`.
/// 5. Attacker returns to the drained key.
///
/// REGRESSION-PIN: with the WP-M fix (drained-tombstone), step 5 MUST NOT
/// re-materialise a full bucket. The bucket is born with 0 tokens; the
/// next `try_acquire` against a non-zero `cost` therefore goes through
/// the lazy-refill formula (which may Allow, but only if elapsed ×
/// refill_rate ≥ cost) — NOT a fresh burst of `burst_capacity`.
#[test]
fn drained_key_does_not_re_materialise_full_after_drain_then_flood() {
    let cap = 16usize;
    let lim = limiter_with_cap(cap);
    let burst = lim.config().default_burst_capacity();

    // ----- Step 1: drain the attacker's own bucket (burst+1 calls) -------
    let victim = victim_key();
    let mut deny_seen = false;
    for i in 0..(burst + 1) {
        let out = lim
            .try_acquire(ATTACKER_TENANT, victim.clone(), 1, 0)
            .unwrap();
        match &out.decision {
            RateLimitDecision::Allow { .. } => {
                assert!(i < burst, "Allow past burst boundary at call {i}")
            }
            RateLimitDecision::Deny429 { .. } => {
                assert_eq!(i, burst, "Deny at boundary, got Deny at {i}");
                deny_seen = true;
            }
            _ => panic!("unexpected decision variant"),
        }
    }
    assert!(deny_seen, "expected the burst+1-th call to be Deny429");
    assert_eq!(lim.bucket_count().unwrap(), 1);

    // ----- Step 2 + 3: STOP touching the attacker's key; flood 64 distinct
    // keys. The attacker's `last_access` is the smallest in the map
    // after this; the approximate-LRU eviction (sample K=8) will
    // hit it with overwhelming probability across 64 evictions.
    for n in 0..64u32 {
        let _ = lim
            .try_acquire(FLOOD_TENANT, flood_key(n), 1, 0)
            .unwrap();
    }
    assert!(lim.bucket_count().unwrap() <= cap);

    // ----- Step 4 + 5: the attacker returns to the drained key ----------
    // The TOMBSTONE is alive (it expires at `now + 60 s`; the wall clock
    // for this test is `0` and stays at `0`). The next call sees the
    // tombstone and is born with 0 tokens. The lazy-refill step then
    // computes `delta_secs = 0` (no time has passed) → refilled_tokens
    // = 0 → 0 < cost (1) → `Deny429`.
    let out = lim
        .try_acquire(ATTACKER_TENANT, victim.clone(), 1, 0)
        .unwrap();
    assert!(
        !is_allow(&out.decision),
        "bypass REGRESSION: the drained bucket was re-materialised Allow \
         with the burst budget — the WP-M fix is not working. Step 5 \
         of the briefing attack must return `Deny429` (or at most an \
         Allow with strictly less than `burst_capacity` tokens remaining). \
         Got: {out:?}"
    );
}

/// Coarse sanity: a brand-new key (never drained) re-materialises full.
/// This pins the non-regression of the legitimate new-tenant admission
/// policy — the tombstone map must NOT poison every fresh key.
#[test]
fn fresh_key_still_re_materialises_full() {
    let cap = 16usize;
    let lim = limiter_with_cap(cap);
    let key = flood_key(0);
    let out = lim
        .try_acquire(FLOOD_TENANT, key, 1, 0)
        .unwrap();
    assert!(is_allow(&out.decision));
}
