//! Token-bucket state machine and the canonical lazy-refill formula.
//!
//! ## Token-bucket math (lazy refill canonical; WI-S08-001 §6.1.3)
//!
//! ```text
//! delta_secs           = max(0.0, (now_ms - last_refill_at_ms) / 1000.0)
//! refilled_tokens      = min(burst_capacity,
//!                            available_tokens + delta_secs × refill_rate_per_sec)
//! if refilled_tokens >= cost {
//!     available_tokens = refilled_tokens − cost
//!     last_refill_at_ms = now_ms
//!     Allow
//! } else if refill_rate_per_sec <= EPSILON {
//!     // Canceled tenant guard (Lote 10.8bis P1-1 division-by-zero):
//!     // saturating Retry-After to canonical 7 days = "effectively never".
//!     retry_after_secs = canceled_tenant_secs
//!     available_tokens = refilled_tokens
//!     last_refill_at_ms = now_ms
//!     Deny429
//! } else {
//!     // RFC 6585 §4 + RFC 9331: minimum wait until the bucket has
//!     // enough tokens to admit `cost`. We always charge the floor so
//!     // a hot retry loop client cannot busy-spin on `Retry-After: 0`.
//!     needed           = cost - refilled_tokens
//!     wait_secs        = ceil(needed / refill_rate_per_sec)
//!     retry_after_secs = clamp(wait_secs, floor_secs, hard_ceiling_secs)
//!     // tokens NOT decremented on the deny arm — fairness invariant.
//!     available_tokens  = refilled_tokens
//!     last_refill_at_ms = now_ms
//!     Deny429
//! }
//! ```
//!
//! ## Why f64 for `available_tokens`
//!
//! The lazy-refill product `delta_secs × refill_rate_per_sec` is a
//! floating-point computation. Truncating to `u32` per call would
//! systematically lose sub-1-token refills on small refill rates over
//! long quiet windows, leading to over-throttling. WI §1 cripto-driven
//! invariant 5 documents the rationale: f64 ≥ 9 decimal digits is
//! sufficient for 10000 RPS × 1ms granularity.
//!
//! ## Monotonic clock clamp (WI §1 invariant 5 + R-004)
//!
//! `now_ms` is clamped to `last_refill_at_ms` on every call. If the
//! Worker clock skews backward, `delta_secs` is 0 — tokens never
//! decrement spuriously, and `available_tokens` never goes negative.
//! Pinned by `prop_token_bucket_never_negative` +
//! `prop_clock_skew_safety` + `prop_refill_monotonic_in_time`.
//!
//! ## Fairness invariant (deny path)
//!
//! Deny arm does NOT decrement `available_tokens`. A heavily-burst
//! tenant who hits 429 doesn't get charged for the rejected request;
//! the next call benefits from the same `refilled_tokens`. Pinned by
//! `prop_deny_path_does_not_decrement_tokens`.

use crate::config::RateLimitConfig;

/// Token-bucket state machine.
///
/// Production wiring stores this struct as the in-memory representation
/// of the per-tenant DO singleton's token bucket; the durable mirror
/// columns line up byte-for-byte with the SQL fields in
/// `migrations/d1/0010_ratelimit_buckets.sql`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TokenBucketState {
    /// Currently available tokens (clamp `[0.0, burst_capacity as f64]`).
    pub available_tokens: f64,
    /// Bucket capacity (token ceiling; >= 1).
    pub burst_capacity: u32,
    /// Plan refill rate (tokens / second; >= 0.0).
    pub refill_rate_per_sec: f64,
    /// Last refill watermark (Unix ms; monotonic).
    pub last_refill_at_ms: u64,
}

impl TokenBucketState {
    /// Build a fresh full bucket — `available_tokens = burst_capacity`.
    /// Mirrors the canonical "new tenant lands with full burst budget"
    /// admission policy.
    #[must_use]
    pub fn new_full(
        burst_capacity: u32,
        refill_rate_per_sec: u32,
        created_at_ms: u64,
    ) -> Self {
        Self {
            available_tokens: f64::from(burst_capacity),
            burst_capacity,
            refill_rate_per_sec: f64::from(refill_rate_per_sec),
            last_refill_at_ms: created_at_ms,
        }
    }

    /// Build a bucket from explicit values (for cold-start reload from
    /// the durable D1 mirror). Clamps `available_tokens` to
    /// `[0.0, burst_capacity as f64]` defensively.
    #[must_use]
    pub fn from_persisted(
        available_tokens: f64,
        burst_capacity: u32,
        refill_rate_per_sec: f64,
        last_refill_at_ms: u64,
    ) -> Self {
        let cap = f64::from(burst_capacity);
        let clamped = if available_tokens.is_nan() || available_tokens < 0.0 {
            0.0
        } else if available_tokens > cap {
            cap
        } else {
            available_tokens
        };
        let rate = if refill_rate_per_sec.is_nan() || refill_rate_per_sec < 0.0 {
            0.0
        } else {
            refill_rate_per_sec
        };
        Self {
            available_tokens: clamped,
            burst_capacity,
            refill_rate_per_sec: rate,
            last_refill_at_ms,
        }
    }

    /// Apply the lazy refill step: clamp `now_ms` to monotonic
    /// `last_refill_at_ms`, compute `refilled_tokens`, and return the
    /// new value WITHOUT mutating self.
    ///
    /// Pure function — the orchestrator decides whether to commit the
    /// new value based on the cost check.
    #[must_use]
    pub fn refilled(&self, now_ms: u64) -> f64 {
        let clamped_now = now_ms.max(self.last_refill_at_ms);
        let delta_ms = clamped_now.saturating_sub(self.last_refill_at_ms);
        let delta_secs = (delta_ms as f64) / 1000.0;
        let added = delta_secs * self.refill_rate_per_sec;
        let projected = self.available_tokens + added;
        let cap = f64::from(self.burst_capacity);
        if projected.is_nan() {
            // Defensive — propagating NaN would corrupt downstream
            // comparisons; fall back to capacity (assumes max-capacity
            // worst-case for the caller; conservative on the deny side
            // because it admits the next request).
            cap
        } else if projected > cap {
            cap
        } else if projected < 0.0 {
            // Cannot happen given non-negative inputs, but defend
            // anyway.
            0.0
        } else {
            projected
        }
    }

    /// Whether this bucket is canceled (refill_rate effectively 0).
    /// Mirrors WI §6.1.3 Lote 10.8bis P1-1 division-by-zero guard
    /// trigger.
    #[must_use]
    pub fn is_canceled(&self) -> bool {
        self.refill_rate_per_sec <= f64::EPSILON
    }
}

/// Per-call decision rendered by the lazy-refill arithmetic.
///
/// `PartialEq` is intentionally NOT derived because the `Allow` arm
/// carries `available_tokens_after: f64` (NaN-aware semantics). Tests
/// use `matches!` + per-field equality where needed.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum BucketDecision {
    /// `cost` consumed from the bucket; updated state ready to commit.
    Allow {
        /// Updated bucket state (caller commits to durable mirror).
        next_state: TokenBucketState,
        /// Available tokens after the consume, integer-rounded for
        /// metric / audit emit (the wire-level `next_state` carries
        /// the f64 precision).
        available_tokens_after: u64,
    },
    /// 429 + Retry-After arm (RFC 6585 §4 + RFC 9331).
    Deny429 {
        /// Updated bucket state — `available_tokens` reflects the
        /// post-refill value but the deny arm does NOT decrement
        /// (fairness invariant).
        next_state: TokenBucketState,
        /// Retry-After value (seconds; clamped to
        /// `[floor, hard_ceiling_secs]` for live tenants and to
        /// `canceled_tenant_secs` when the bucket is canceled).
        retry_after_secs: u64,
        /// Available tokens after the refill, integer-rounded.
        available_tokens_after: u64,
    },
}

/// Compute the per-request bucket decision via the lazy-refill
/// canonical formula.
///
/// `cost` is the request weight (typically 1; bandwidth-weighted
/// requests sometimes >1 per WI §1). `now_ms` is the wall-clock
/// instant the request was admitted at the Tower layer (caller should
/// pass `worker::Date::now()` or equivalent).
///
/// # Panics
///
/// Never. The function is total — every input branch returns a typed
/// decision (no panics, no `unwrap`).
#[must_use]
pub fn try_acquire(
    state: TokenBucketState,
    cost: u32,
    now_ms: u64,
    config: &RateLimitConfig,
) -> BucketDecision {
    // Cost == 0 is idempotent: no refill, no consume, just echo state.
    if cost == 0 {
        return BucketDecision::Allow {
            next_state: state,
            available_tokens_after: state.available_tokens.floor() as u64,
        };
    }

    let refilled = state.refilled(now_ms);
    let cost_f = f64::from(cost);
    // Always advance last_refill_at_ms to clamped(now). Mirrors the WI
    // §6.1.3 spec — both Allow and Deny update the watermark so the
    // next call's delta computes correctly.
    let next_now = now_ms.max(state.last_refill_at_ms);

    if refilled >= cost_f {
        let after = refilled - cost_f;
        let next_state = TokenBucketState {
            available_tokens: after,
            burst_capacity: state.burst_capacity,
            refill_rate_per_sec: state.refill_rate_per_sec,
            last_refill_at_ms: next_now,
        };
        BucketDecision::Allow {
            next_state,
            available_tokens_after: after.floor() as u64,
        }
    } else {
        // Deny arm — NEVER decrement `available_tokens` (fairness
        // invariant). Update state to reflect the refill so the next
        // call observes the latest watermark.
        let next_state = TokenBucketState {
            available_tokens: refilled,
            burst_capacity: state.burst_capacity,
            refill_rate_per_sec: state.refill_rate_per_sec,
            last_refill_at_ms: next_now,
        };

        let retry_after_secs = if state.is_canceled() {
            // Canonical canceled-tenant retry (WI §6.1.3 Lote 10.8bis P1-1).
            config.retry_after_canceled_tenant_secs()
        } else {
            // RFC 6585 §4 + RFC 9331 — minimum wait for `cost - refilled`
            // tokens to refill.
            let needed = cost_f - refilled;
            let wait_secs_f = (needed / state.refill_rate_per_sec).ceil();
            // Defensive: clamp to live-tenant floor + hard ceiling.
            let wait_u = if wait_secs_f.is_nan() || wait_secs_f < 0.0 {
                config.retry_after_floor_secs()
            } else if wait_secs_f > config.retry_after_hard_ceiling_secs() as f64
            {
                config.retry_after_hard_ceiling_secs()
            } else {
                wait_secs_f as u64
            };
            wait_u
                .max(config.retry_after_floor_secs())
                .min(config.retry_after_hard_ceiling_secs())
        };

        BucketDecision::Deny429 {
            next_state,
            retry_after_secs,
            available_tokens_after: refilled.floor() as u64,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn cfg() -> RateLimitConfig {
        RateLimitConfig::canonical()
    }

    #[test]
    fn fresh_bucket_starts_full() {
        let s = TokenBucketState::new_full(100, 10, 1000);
        assert_eq!(s.available_tokens, 100.0);
        assert_eq!(s.burst_capacity, 100);
        assert_eq!(s.refill_rate_per_sec, 10.0);
        assert_eq!(s.last_refill_at_ms, 1000);
    }

    #[test]
    fn from_persisted_clamps_above_capacity() {
        let s = TokenBucketState::from_persisted(150.0, 100, 10.0, 1000);
        assert_eq!(s.available_tokens, 100.0);
    }

    #[test]
    fn from_persisted_clamps_below_zero() {
        let s = TokenBucketState::from_persisted(-5.0, 100, 10.0, 1000);
        assert_eq!(s.available_tokens, 0.0);
    }

    #[test]
    fn from_persisted_clamps_nan() {
        let s = TokenBucketState::from_persisted(f64::NAN, 100, 10.0, 1000);
        assert_eq!(s.available_tokens, 0.0);
    }

    #[test]
    fn from_persisted_clamps_negative_rate() {
        let s = TokenBucketState::from_persisted(50.0, 100, -5.0, 1000);
        assert_eq!(s.refill_rate_per_sec, 0.0);
    }

    #[test]
    fn refill_clamps_at_capacity() {
        // Bucket at 50/100; rate 10/s; 100s elapsed → 1000 added → clamp to 100.
        let s = TokenBucketState {
            available_tokens: 50.0,
            burst_capacity: 100,
            refill_rate_per_sec: 10.0,
            last_refill_at_ms: 1000,
        };
        let r = s.refilled(101_000);
        assert_eq!(r, 100.0);
    }

    #[test]
    fn refill_zero_delta_yields_unchanged() {
        let s = TokenBucketState::new_full(100, 10, 1000);
        let r = s.refilled(1000);
        assert_eq!(r, 100.0);
    }

    #[test]
    fn refill_clamps_clock_skew_backward() {
        // last_refill = 5000; now = 4000 (skewed backward). Should
        // clamp to last_refill — delta = 0.
        let s = TokenBucketState {
            available_tokens: 50.0,
            burst_capacity: 100,
            refill_rate_per_sec: 10.0,
            last_refill_at_ms: 5000,
        };
        let r = s.refilled(4000);
        assert_eq!(r, 50.0);
    }

    #[test]
    fn try_acquire_cost_zero_is_idempotent() {
        let s = TokenBucketState::new_full(100, 10, 1000);
        let d = try_acquire(s, 0, 2000, &cfg());
        match d {
            BucketDecision::Allow { next_state, .. } => {
                assert_eq!(next_state.available_tokens, 100.0);
                // last_refill_at_ms NOT advanced (cost 0 path is pure echo).
                assert_eq!(next_state.last_refill_at_ms, 1000);
            }
            BucketDecision::Deny429 { .. } => panic!("cost 0 must allow"),
        }
    }

    #[test]
    fn try_acquire_under_capacity_allows() {
        let s = TokenBucketState::new_full(100, 10, 1000);
        let d = try_acquire(s, 1, 1000, &cfg());
        match d {
            BucketDecision::Allow {
                next_state,
                available_tokens_after,
            } => {
                assert_eq!(next_state.available_tokens, 99.0);
                assert_eq!(available_tokens_after, 99);
                assert_eq!(next_state.last_refill_at_ms, 1000);
            }
            _ => panic!("expected allow"),
        }
    }

    #[test]
    fn try_acquire_at_exact_capacity_allows_then_denies() {
        let s = TokenBucketState::new_full(2, 1, 1000);
        // First call: tokens=2, cost=1 → allow; tokens=1.
        let d1 = try_acquire(s, 1, 1000, &cfg());
        let next = match d1 {
            BucketDecision::Allow { next_state, .. } => next_state,
            _ => panic!("expected allow"),
        };
        // Second call same instant: tokens=1, cost=1 → allow; tokens=0.
        let d2 = try_acquire(next, 1, 1000, &cfg());
        let next = match d2 {
            BucketDecision::Allow { next_state, .. } => next_state,
            _ => panic!("expected allow"),
        };
        // Third call: tokens=0, cost=1 → deny.
        let d3 = try_acquire(next, 1, 1000, &cfg());
        match d3 {
            BucketDecision::Deny429 { .. } => {}
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn try_acquire_deny_does_not_decrement_tokens() {
        let s = TokenBucketState::from_persisted(0.0, 100, 1.0, 1000);
        // 0 tokens; cost 5; rate 1/s; same instant → deny.
        let d = try_acquire(s, 5, 1000, &cfg());
        match d {
            BucketDecision::Deny429 {
                next_state,
                available_tokens_after,
                ..
            } => {
                // Fairness invariant: deny path does NOT decrement.
                assert_eq!(next_state.available_tokens, 0.0);
                assert_eq!(available_tokens_after, 0);
            }
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn try_acquire_deny_retry_after_at_least_floor() {
        // 0 tokens; cost 1; rate 1/s; same instant → wait_secs = 1.0
        // → retry_after = 1 (== floor).
        let s = TokenBucketState::from_persisted(0.0, 100, 1.0, 1000);
        let d = try_acquire(s, 1, 1000, &cfg());
        match d {
            BucketDecision::Deny429 {
                retry_after_secs, ..
            } => {
                assert!(retry_after_secs >= 1);
            }
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn try_acquire_deny_retry_after_clamped_to_hard_ceiling() {
        // 0 tokens; cost 100_000; rate 1/s → wait_secs = 100_000;
        // hard ceiling 86_400 → clamp.
        let s = TokenBucketState::from_persisted(0.0, 1_000_000, 1.0, 1000);
        let d = try_acquire(s, 100_000, 1000, &cfg());
        match d {
            BucketDecision::Deny429 {
                retry_after_secs, ..
            } => {
                assert_eq!(retry_after_secs, 86_400);
            }
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn try_acquire_canceled_tenant_returns_canceled_retry() {
        // refill_rate=0 → canceled tenant guard fires.
        let s = TokenBucketState::from_persisted(0.0, 100, 0.0, 1000);
        let d = try_acquire(s, 1, 1000, &cfg());
        match d {
            BucketDecision::Deny429 {
                retry_after_secs, ..
            } => {
                assert_eq!(retry_after_secs, 7 * 86_400);
            }
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn try_acquire_clock_skew_backward_safe() {
        // last_refill 5000; now 4000 (skewed). Tokens NEVER decrement
        // spuriously (delta=0 via clamp).
        let s = TokenBucketState {
            available_tokens: 10.0,
            burst_capacity: 100,
            refill_rate_per_sec: 1.0,
            last_refill_at_ms: 5000,
        };
        let d = try_acquire(s, 1, 4000, &cfg());
        match d {
            BucketDecision::Allow { next_state, .. } => {
                assert_eq!(next_state.available_tokens, 9.0);
                // Watermark advances to clamped now (= last_refill=5000).
                assert_eq!(next_state.last_refill_at_ms, 5000);
            }
            _ => panic!("expected allow (10 tokens >> 1 cost)"),
        }
    }

    #[test]
    fn try_acquire_lazy_refill_correct_over_time() {
        // 0 tokens; rate 10/s; 5s elapsed → 50 tokens refilled.
        let s = TokenBucketState::from_persisted(0.0, 100, 10.0, 1000);
        let d = try_acquire(s, 25, 6000, &cfg());
        match d {
            BucketDecision::Allow { next_state, .. } => {
                // 0 + (5000ms × 10/s / 1000) = 50; consume 25; left=25.
                assert!((next_state.available_tokens - 25.0).abs() < 1e-9);
            }
            _ => panic!("expected allow"),
        }
    }

    #[test]
    fn is_canceled_detects_zero_rate() {
        let s = TokenBucketState::from_persisted(50.0, 100, 0.0, 1000);
        assert!(s.is_canceled());
    }

    #[test]
    fn is_canceled_does_not_trip_for_small_positive_rate() {
        // 0.001 RPS is small but legitimate (a heavily throttled
        // canary tenant).
        let s = TokenBucketState::from_persisted(50.0, 100, 0.001, 1000);
        assert!(!s.is_canceled());
    }

    #[test]
    fn refill_handles_capacity_zero_clamp_at_zero() {
        // A burst capacity of 0 is rejected at config layer; if it
        // somehow leaks through, refill must not return negative.
        let s = TokenBucketState {
            available_tokens: 0.0,
            burst_capacity: 0,
            refill_rate_per_sec: 10.0,
            last_refill_at_ms: 1000,
        };
        let r = s.refilled(2000);
        assert_eq!(r, 0.0);
    }
}
