//! Rate-limiter: 1 publish per 5 minutes per (page_id, metric_id).
//!
//! Per Atlassian Statuspage Public-Metric API quota policy the maximum
//! data-point ingestion rate is 1 sample per 5 minutes per metric. We
//! enforce that here client-side so we never burn through the quota.
//!
//! The limiter is monotonic-clock-driven (the caller injects the
//! current epoch ms; production wiring uses `SystemTime::now()` from
//! the worker). Concurrent publishes for the same metric serialize on
//! a per-metric mutex.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Canonical Statuspage rate limit window: 5 minutes.
pub const RATE_LIMIT_WINDOW_MS: u64 = 5 * 60 * 1_000;

/// Decision from the rate-limiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RateLimitDecision {
    /// Caller may publish now.
    Allow,
    /// Caller must wait `retry_after` before retrying. The duration is
    /// the gap to the next allowed publish slot for the metric. The
    /// `jitter` field encodes the 0..=`retry_after/8` jitter the
    /// caller SHOULD add to avoid thundering-herd retries.
    DenyBackoff {
        /// Earliest next-allowed delay.
        retry_after: Duration,
        /// Suggested upper bound on jitter (deterministic; caller
        /// picks a random value in `[Duration::ZERO, jitter]`).
        jitter: Duration,
    },
}

/// Per-(page_id, metric_id) rate limiter.
///
/// Thread-safe; the limiter is shared by reference (via `Arc`) between
/// the publish job and any auxiliary callers. The inner state is one
/// `HashMap` keyed by `(page_id, metric_id)` storing the last allowed
/// publish epoch in ms.
#[derive(Clone, Debug, Default)]
pub struct StatuspageRateLimiter {
    last_allowed: Arc<Mutex<HashMap<(String, String), u64>>>,
}

impl StatuspageRateLimiter {
    /// Construct an empty limiter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decide whether `(page_id, metric_id)` may publish at
    /// `now_epoch_ms`. On `Allow` the limiter records `now_epoch_ms`
    /// as the latest publish; on `DenyBackoff` no state is mutated.
    pub fn decide(&self, page_id: &str, metric_id: &str, now_epoch_ms: u64) -> RateLimitDecision {
        let key = (page_id.to_string(), metric_id.to_string());
        let mut guard = match self.last_allowed.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.get(&key).copied() {
            Some(last) if now_epoch_ms < last.saturating_add(RATE_LIMIT_WINDOW_MS) => {
                let elapsed = now_epoch_ms.saturating_sub(last);
                let retry_after_ms = RATE_LIMIT_WINDOW_MS.saturating_sub(elapsed);
                let jitter_ms = retry_after_ms / 8;
                RateLimitDecision::DenyBackoff {
                    retry_after: Duration::from_millis(retry_after_ms),
                    jitter: Duration::from_millis(jitter_ms),
                }
            }
            _ => {
                guard.insert(key, now_epoch_ms);
                RateLimitDecision::Allow
            }
        }
    }

    /// Inspect the last allowed publish ms for a metric (test helper).
    #[must_use]
    pub fn last_allowed_ms(&self, page_id: &str, metric_id: &str) -> Option<u64> {
        let key = (page_id.to_string(), metric_id.to_string());
        match self.last_allowed.lock() {
            Ok(g) => g.get(&key).copied(),
            Err(p) => p.into_inner().get(&key).copied(),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn first_call_allowed() {
        let l = StatuspageRateLimiter::new();
        assert_eq!(l.decide("p", "m", 1_000), RateLimitDecision::Allow);
    }

    #[test]
    fn second_call_within_window_denied_with_backoff() {
        let l = StatuspageRateLimiter::new();
        let _ = l.decide("p", "m", 0);
        let d = l.decide("p", "m", 60_000); // 1 min later
        let is_deny = matches!(d, RateLimitDecision::DenyBackoff { .. });
        assert!(is_deny);
        if let RateLimitDecision::DenyBackoff {
            retry_after,
            jitter,
        } = d
        {
            // 4 min remaining
            assert_eq!(retry_after, Duration::from_millis(4 * 60 * 1_000));
            // jitter = retry_after / 8
            assert_eq!(jitter, Duration::from_millis(30_000));
        }
    }

    #[test]
    fn call_after_window_allowed() {
        let l = StatuspageRateLimiter::new();
        let _ = l.decide("p", "m", 0);
        // exactly at the boundary (5 min)
        assert_eq!(
            l.decide("p", "m", RATE_LIMIT_WINDOW_MS),
            RateLimitDecision::Allow
        );
    }

    #[test]
    fn different_metric_keys_isolated() {
        let l = StatuspageRateLimiter::new();
        assert_eq!(l.decide("p", "m1", 1_000), RateLimitDecision::Allow);
        // different metric — should also be Allow at the same instant.
        assert_eq!(l.decide("p", "m2", 1_000), RateLimitDecision::Allow);
    }

    #[test]
    fn denied_decisions_do_not_advance_window() {
        let l = StatuspageRateLimiter::new();
        let _ = l.decide("p", "m", 0);
        // Multiple denies at increasing timestamps should not extend
        // the window — they all reference the original allowed slot.
        let _ = l.decide("p", "m", 60_000);
        let _ = l.decide("p", "m", 120_000);
        assert_eq!(l.last_allowed_ms("p", "m"), Some(0));
    }
}
