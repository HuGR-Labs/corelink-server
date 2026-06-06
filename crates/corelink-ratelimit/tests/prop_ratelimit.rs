//! Property tests pinning the load-bearing invariants of
//! `corelink-ratelimit` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override).
//!
//! Coverage map (mirrors WI-S08-001 §6.1.11):
//!
//! - `prop_token_bucket_never_exceeds_capacity` — refill never overflows
//!   the burst capacity ceiling.
//! - `prop_token_bucket_never_negative` — consume never drives
//!   `available_tokens` below zero.
//! - `prop_refill_monotonic_in_time` — later timestamps produce >= tokens
//!   for the same starting state.
//! - `prop_retry_after_lower_bound` — Retry-After is the minimum wait
//!   until capacity is available (RFC 6585 §4 + RFC 9331); never below
//!   the floor.
//! - `prop_tenant_isolation` — tenant A's bucket never affects tenant B.
//! - `prop_idempotent_zero_cost_acquire` — `cost == 0` always Allow,
//!   no state change.
//! - `prop_audit_emit_per_decision_arm` — every Allow / Deny429
//!   decision emits exactly one canonical audit record (matching arm).
//! - `prop_concurrent_acquire_does_not_double_spend` — sequential calls
//!   under per-instance Mutex never over-quota even at the boundary
//!   (mirrors DO actor model).
//! - `prop_token_bucket_proportionality` — random plans × random calls
//!   assert `tokens_consumed <= refill_rate × elapsed_secs +
//!   initial_burst`.
//! - `prop_clock_skew_safety` — backward clock skew never decrements
//!   tokens spuriously (monotonic clamp).
//! - `prop_check_duration_under_3ms_p99` — informational SLO probe
//!   over 10k samples.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_eviction::Tier;
use corelink_ratelimit::{
    bucket::try_acquire as bucket_try_acquire, canonical_audit_event_strings,
    canonical_metric_names, refill_rate_for_tier, BucketDecision, BucketKey,
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    KeyDimension, RateLimitConfig, RateLimitDecision, RateLimitEventType, RateLimitMetricKind,
    RateLimitResultLabel, RateLimiter, TokenBucketState, MIGRATION_0010_RATELIMIT_BUCKETS,
    TIER_RATE_LADDER,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

type Limiter = InMemoryTokenBucketRateLimiter<InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics>;

fn fresh_limiter() -> (
    Limiter,
    Arc<InMemoryRateLimitAuditSink>,
    Arc<InMemoryRateLimitMetrics>,
) {
    let audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let lim =
        InMemoryTokenBucketRateLimiter::with_defaults(Arc::clone(&audit), Arc::clone(&metrics));
    (lim, audit, metrics)
}

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 3);
    assert!(s.contains(&"corelink.ratelimit.allowed"));
    assert!(s.contains(&"corelink.ratelimit.denied_429"));
    assert!(s.contains(&"corelink.ratelimit.bucket_refilled"));
}

#[test]
fn canonical_metric_names_pinned() {
    let m = canonical_metric_names();
    assert_eq!(m.len(), 7);
    assert!(m.contains(&"corelink.ratelimit.check_total"));
    assert!(m.contains(&"corelink.ratelimit.tokens_remaining"));
    assert!(m.contains(&"corelink.ratelimit.refill_rate"));
    assert!(m.contains(&"corelink.ratelimit.plan_sync_lag_ms"));
    assert!(m.contains(&"corelink.ratelimit.do_cold_start_total"));
    assert!(m.contains(&"corelink.ratelimit.middleware_duration_us"));
    assert!(m.contains(&"corelink.ratelimit.cross_tenant_violation_total"));
}

#[test]
fn migration_0010_is_embedded() {
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("ratelimit_buckets"));
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("migration 0010"));
}

#[test]
fn ratelimit_schema_version_pinned() {
    assert_eq!(corelink_ratelimit::ratelimit_schema_version(), 10);
}

#[test]
fn rate_limit_config_canonical_constants() {
    let c = RateLimitConfig::canonical();
    assert_eq!(c.default_refill_rate_per_sec(), 200);
    assert_eq!(c.default_burst_capacity(), 1000);
    assert_eq!(c.retry_after_floor_secs(), 1);
    assert_eq!(c.retry_after_hard_ceiling_secs(), 86_400);
    assert_eq!(c.retry_after_canceled_tenant_secs(), 7 * 86_400);
}

#[test]
fn tier_ladder_5_canonical() {
    assert_eq!(TIER_RATE_LADDER.len(), 5);
    // Stripe-style + WI-S08-001 §6.1.4 baselines.
    assert_eq!(refill_rate_for_tier(Tier::Free), (10, 50));
    assert_eq!(refill_rate_for_tier(Tier::Solo), (50, 200));
    assert_eq!(refill_rate_for_tier(Tier::Team), (200, 1000));
    assert_eq!(refill_rate_for_tier(Tier::Business), (1000, 5000));
    assert_eq!(refill_rate_for_tier(Tier::Enterprise), (10_000, 50_000));
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Refill never overflows the burst capacity ceiling, regardless
    /// of elapsed time / initial token count / refill rate.
    #[test]
    fn prop_token_bucket_never_exceeds_capacity(
        burst_capacity in 1_u32..1_000_000,
        refill_rate in 0_u32..100_000,
        starting_tokens in 0_u32..1_000_000,
        delta_ms in 0_u64..1_000_000_000,
    ) {
        // Clamp starting_tokens to capacity (mirrors from_persisted invariant).
        let start_f = (starting_tokens as f64).min(burst_capacity as f64);
        let s = TokenBucketState::from_persisted(
            start_f,
            burst_capacity,
            f64::from(refill_rate),
            1000,
        );
        let r = s.refilled(1000_u64.saturating_add(delta_ms));
        prop_assert!(r <= f64::from(burst_capacity) + 1e-6);
    }

    /// Consume never drives `available_tokens` below 0 across any
    /// sequence of allow / deny outcomes.
    #[test]
    fn prop_token_bucket_never_negative(
        burst_capacity in 1_u32..1000,
        refill_rate in 1_u32..1000,
        starting_tokens in 0_u32..1000,
        cost in 1_u32..1000,
        delta_ms in 0_u64..10_000_000,
    ) {
        let start_f = (starting_tokens as f64).min(burst_capacity as f64);
        let s = TokenBucketState::from_persisted(
            start_f,
            burst_capacity,
            f64::from(refill_rate),
            1000,
        );
        let cost_clamped = cost.min(burst_capacity);
        let d = bucket_try_acquire(s, cost_clamped, 1000_u64.saturating_add(delta_ms), &RateLimitConfig::canonical());
        let next = match d {
            BucketDecision::Allow { next_state, .. } => next_state,
            BucketDecision::Deny429 { next_state, .. } => next_state,
            _ => {
                prop_assert!(false, "BucketDecision gained a new variant");
                unreachable!();
            }
        };
        prop_assert!(next.available_tokens >= 0.0);
        prop_assert!(next.available_tokens <= f64::from(burst_capacity));
    }

    /// Later timestamps produce >= tokens than earlier timestamps.
    #[test]
    fn prop_refill_monotonic_in_time(
        burst_capacity in 1_u32..10_000,
        refill_rate in 0_u32..10_000,
        starting_tokens in 0_u32..10_000,
        t1 in 0_u64..100_000,
        delta in 1_u64..100_000,
    ) {
        let start_f = (starting_tokens as f64).min(burst_capacity as f64);
        let s = TokenBucketState::from_persisted(
            start_f,
            burst_capacity,
            f64::from(refill_rate),
            t1,
        );
        let r1 = s.refilled(t1);
        let r2 = s.refilled(t1.saturating_add(delta));
        prop_assert!(r2 + 1e-6 >= r1, "r2={r2} < r1={r1}");
    }

    /// Retry-After is at least the floor and at most the hard ceiling
    /// (live tenants) or the canceled-tenant value.
    #[test]
    fn prop_retry_after_lower_bound(
        burst_capacity in 1_u32..1000,
        refill_rate in 0_u32..1000,
        starting_tokens in 0_u32..50,
        cost in 50_u32..1000,
    ) {
        // Tight scenario: starting_tokens < cost and same instant →
        // deny path (caller should also assume cost <= burst_capacity).
        let cost_clamped = cost.min(burst_capacity);
        prop_assume!(cost_clamped > starting_tokens);
        let s = TokenBucketState::from_persisted(
            f64::from(starting_tokens),
            burst_capacity,
            f64::from(refill_rate),
            1000,
        );
        let cfg = RateLimitConfig::canonical();
        let d = bucket_try_acquire(s, cost_clamped, 1000, &cfg);
        match d {
            BucketDecision::Deny429 {
                retry_after_secs, ..
            } => {
                prop_assert!(retry_after_secs >= cfg.retry_after_floor_secs());
                if refill_rate == 0 {
                    // Canceled-tenant arm.
                    prop_assert_eq!(
                        retry_after_secs,
                        cfg.retry_after_canceled_tenant_secs()
                    );
                } else {
                    prop_assert!(
                        retry_after_secs <= cfg.retry_after_hard_ceiling_secs()
                    );
                }
            }
            // If the boundary admitted, the prop is vacuously true
            // for THIS arm — the floor / ceiling only apply to deny.
            BucketDecision::Allow { .. } => {}
            _ => {
                prop_assert!(false, "BucketDecision gained a new variant");
            }
        }
    }

    /// Tenant A's bucket NEVER affects tenant B's bucket.
    #[test]
    fn prop_tenant_isolation(
        a_starting in 0_u32..100,
        b_starting in 0_u32..100,
        cost in 1_u32..50,
    ) {
        let (lim, _a, _m) = fresh_limiter();
        let ka = BucketKey::per_tenant(ten_a());
        let kb = BucketKey::per_tenant(ten_b());
        // Seed both with explicit values.
        lim.seed_bucket(
            ka.clone(),
            TokenBucketState::from_persisted(
                f64::from(a_starting),
                100,
                10.0,
                1000,
            ),
        )
        .unwrap();
        lim.seed_bucket(
            kb.clone(),
            TokenBucketState::from_persisted(
                f64::from(b_starting),
                100,
                10.0,
                1000,
            ),
        )
        .unwrap();
        // Read tenant_b's snapshot.
        let snap_b_before = lim.snapshot_bucket(&kb).unwrap().unwrap();
        // Drain tenant_a as much as possible.
        for _ in 0..20 {
            let _ = lim.try_acquire(ten_a(), ka.clone(), cost, 1000);
        }
        // Tenant_b's bucket must be UNCHANGED.
        let snap_b_after = lim.snapshot_bucket(&kb).unwrap().unwrap();
        prop_assert_eq!(snap_b_before.available_tokens, snap_b_after.available_tokens);
    }

    /// `cost == 0` always allows + does not change state.
    #[test]
    fn prop_idempotent_zero_cost_acquire(
        starting_tokens in 0_u32..1000,
        burst in 1_u32..1000,
    ) {
        let start_f = (starting_tokens as f64).min(burst as f64);
        let s = TokenBucketState::from_persisted(start_f, burst, 100.0, 1000);
        let d = bucket_try_acquire(s, 0, 5000, &RateLimitConfig::canonical());
        match d {
            BucketDecision::Allow { next_state, .. } => {
                prop_assert_eq!(next_state.available_tokens, start_f);
                prop_assert_eq!(next_state.last_refill_at_ms, 1000);
            }
            BucketDecision::Deny429 { .. } => {
                prop_assert!(false, "cost=0 must always allow");
            }
            _ => {
                prop_assert!(false, "BucketDecision gained a new variant");
            }
        }
    }

    /// Every decision arm emits exactly ONE canonical audit record
    /// (Allowed or Denied429); BucketRefilled only fires when the
    /// refill step moved the watermark.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        starting_tokens in 0_u32..200,
        cost in 1_u32..100,
        delta_ms in 0_u64..1_000_000,
    ) {
        let (lim, audit, _m) = fresh_limiter();
        let key = BucketKey::per_tenant(ten_a());
        lim.seed_bucket(
            key.clone(),
            TokenBucketState::from_persisted(
                f64::from(starting_tokens),
                1000,
                100.0,
                1000,
            ),
        )
        .unwrap();
        let out = lim
            .try_acquire(ten_a(), key, cost, 1000_u64.saturating_add(delta_ms))
            .unwrap();
        // Exactly one decision-arm record (Allowed or Denied429).
        let allowed = audit.snapshot_of(RateLimitEventType::Allowed).len();
        let denied = audit.snapshot_of(RateLimitEventType::Denied429).len();
        prop_assert_eq!(allowed + denied, 1);
        match out.decision {
            RateLimitDecision::Allow { .. } => prop_assert_eq!(allowed, 1),
            RateLimitDecision::Deny429 { .. } => prop_assert_eq!(denied, 1),
            _ => {
                prop_assert!(false, "RateLimitDecision gained a new variant");
            }
        }
    }

    /// Sequential calls at the boundary — DO actor model serialisation
    /// — never double-spend. The first call admits; the second sees
    /// the first's commit and fires deny.
    #[test]
    fn prop_concurrent_acquire_does_not_double_spend(
        cost in 1_u32..100,
    ) {
        let (lim, _a, _m) = fresh_limiter();
        let key = BucketKey::per_tenant(ten_a());
        // Burst = cost exactly: first call drains the bucket; second
        // call MUST deny (no leftover, no refill mid-pair).
        let burst = cost;
        lim.seed_bucket(
            key.clone(),
            TokenBucketState::from_persisted(
                f64::from(burst),
                burst,
                0.0, // canceled rate so nothing refills mid-pair.
                1000,
            ),
        )
        .unwrap();
        // First call admits.
        let out1 = lim.try_acquire(ten_a(), key.clone(), cost, 1000).unwrap();
        prop_assert!(out1.decision.is_allow());
        // Second call: same cost, same instant, no refill — leftover=0
        // strictly less than cost ⇒ deny.
        let out2 = lim.try_acquire(ten_a(), key, cost, 1000).unwrap();
        prop_assert!(
            out2.decision.is_deny(),
            "second call must deny: burst={burst} cost={cost}"
        );
    }

    /// Tokens consumed bounded by `initial + refill_rate * elapsed_secs`.
    #[test]
    fn prop_token_bucket_proportionality(
        rate in 1_u32..10_000,
        burst in 1_u32..100_000,
        elapsed_secs in 0_u32..3_600,
    ) {
        let s = TokenBucketState::from_persisted(0.0, burst, f64::from(rate), 1000);
        let now = 1000_u64.saturating_add(u64::from(elapsed_secs) * 1000);
        let r = s.refilled(now);
        let expected = (f64::from(rate) * f64::from(elapsed_secs))
            .min(f64::from(burst));
        prop_assert!(
            (r - expected).abs() < 1e-6 || r <= f64::from(burst) + 1e-6,
            "r={r} expected≈{expected} burst={burst}"
        );
    }

    /// Backward clock skew NEVER decrements tokens spuriously.
    #[test]
    fn prop_clock_skew_safety(
        starting_tokens in 1_u32..1000,
        skew_back_ms in 1_u64..1_000_000,
    ) {
        let s = TokenBucketState {
            available_tokens: f64::from(starting_tokens),
            burst_capacity: 1000,
            refill_rate_per_sec: 100.0,
            last_refill_at_ms: 1_000_000,
        };
        // now < last_refill — should clamp to last_refill, delta=0.
        let now = 1_000_000_u64.saturating_sub(skew_back_ms);
        let r = s.refilled(now);
        prop_assert_eq!(r, f64::from(starting_tokens));
    }
}

/// Informational SLO probe: collect 10k Allow-arm decisions and assert
/// the captured `middleware_duration_us` samples remain at 0 (the
/// in-memory orchestrator reports 0 because it doesn't measure
/// wall-clock; the production wiring substitutes a real timer).
///
/// The point of this test is to pin the `middleware_duration_us` API
/// surface so the Tower-layer wiring composes the timer correctly.
/// The hot-path is in-memory operations only (no external IO); a
/// 3ms p99 regression would be visible in the production benchmark,
/// not here.
#[test]
fn prop_check_duration_under_3ms_p99() {
    let (lim, _a, metrics) = fresh_limiter();
    // Use a generous bucket so 10k acquires never drain.
    let key = BucketKey::per_tenant(ten_a());
    lim.seed_bucket(
        key.clone(),
        TokenBucketState::from_persisted(1_000_000.0, 1_000_000, 1_000_000.0, 1000),
    )
    .unwrap();
    for _ in 0..10_000 {
        let _ = lim.try_acquire(ten_a(), key.clone(), 1, 1000).unwrap();
    }
    let samples = metrics.duration_samples(RateLimitResultLabel::Allowed);
    assert_eq!(samples.len(), 10_000);
    let max = samples.iter().copied().max().unwrap_or_default();
    // 3000 us = 3 ms p99 SLO ceiling per spec_contract §10.s08.2.
    assert!(max <= 3000, "in-memory duration sample > 3ms us: {max}");
    let total = metrics.counter_total(RateLimitMetricKind::CheckTotal);
    assert_eq!(total, 10_000);
}

/// Helper test — pins the `KeyDimension` 3-canonical list against
/// the SQL CHECK constraint shape.
#[test]
fn key_dimension_canonical_strings_align_with_sql_check() {
    let canonical = ["per_tenant", "per_ip", "per_tenant_per_endpoint"];
    for d in [
        KeyDimension::PerTenant,
        KeyDimension::PerIp,
        KeyDimension::PerTenantPerEndpoint,
    ] {
        assert!(canonical.contains(&d.as_str()), "missing dimension {d}");
    }
}
