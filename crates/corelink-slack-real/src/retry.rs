//! Synchronous retry policy for Slack webhook POSTs.
//!
//! Slack webhooks return:
//! - `2xx` on success (we parse the body — `ok` literal or `{"ts": ...}`).
//! - `429` when rate-limited (the `Retry-After` header indicates how
//!   long to back off).
//! - `5xx` on transient infra failure.
//! - `4xx` (other than 429) on permanent error (malformed payload,
//!   webhook URL revoked) — never retry.
//!
//! This module is sync-only: no tokio, no async. The real HTTP client
//! sleeps the calling thread between retries using
//! [`std::thread::sleep`].

use std::time::Duration;

/// Decision for the next iteration of the retry loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryDecision {
    /// Caller succeeded — exit the loop.
    Success,
    /// Caller should retry after `delay`.
    Retry {
        /// Delay before next attempt.
        delay: Duration,
    },
    /// Caller must give up — the failure is permanent.
    GiveUp,
}

/// Retry policy parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of retries (NOT counting the initial attempt).
    /// Total attempts = `max_retries + 1`.
    pub max_retries: u32,
    /// Initial backoff delay; doubled on each subsequent retry.
    pub initial_backoff: Duration,
    /// Cap on backoff delay (prevents 30-minute waits on storms).
    pub max_backoff: Duration,
}

impl RetryPolicy {
    /// Canonical R2-4 policy: 3 retries, 250ms initial, 8s cap.
    #[must_use]
    pub const fn r2_4_default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(8),
        }
    }

    /// Compute the back-off for a given attempt index (0-based).
    #[must_use]
    pub fn backoff_for(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt).unwrap_or(u32::MAX);
        let ms = self
            .initial_backoff
            .as_millis()
            .saturating_mul(u128::from(factor));
        let cap = self.max_backoff.as_millis();
        let clamped = ms.min(cap);
        Duration::from_millis(u64::try_from(clamped).unwrap_or(u64::MAX))
    }

    /// Decide what to do after seeing an HTTP status (or a transport
    /// error indicated by `status == None`).
    ///
    /// - `2xx` → [`RetryDecision::Success`].
    /// - `429` or `5xx` or transport error → [`RetryDecision::Retry`]
    ///   while `attempt < max_retries`, else [`RetryDecision::GiveUp`].
    /// - Other `4xx` → [`RetryDecision::GiveUp`] immediately.
    #[must_use]
    pub fn decide(&self, status: Option<u16>, attempt: u32) -> RetryDecision {
        match status {
            Some(s) if (200..300).contains(&s) => RetryDecision::Success,
            Some(s) if s == 429 || (500..600).contains(&s) => {
                if attempt < self.max_retries {
                    RetryDecision::Retry {
                        delay: self.backoff_for(attempt),
                    }
                } else {
                    RetryDecision::GiveUp
                }
            }
            Some(_) => RetryDecision::GiveUp, // 4xx other than 429
            None => {
                // Transport error (DNS, TCP, TLS) — treat as transient.
                if attempt < self.max_retries {
                    RetryDecision::Retry {
                        delay: self.backoff_for(attempt),
                    }
                } else {
                    RetryDecision::GiveUp
                }
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::r2_4_default()
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
    fn success_on_2xx() {
        let p = RetryPolicy::r2_4_default();
        assert_eq!(p.decide(Some(200), 0), RetryDecision::Success);
        assert_eq!(p.decide(Some(204), 2), RetryDecision::Success);
    }

    #[test]
    fn retry_on_5xx_until_exhausted() {
        let p = RetryPolicy::r2_4_default();
        // 4 total attempts: 0, 1, 2, 3 (last → GiveUp because attempt
        // == max_retries).
        assert!(matches!(p.decide(Some(503), 0), RetryDecision::Retry { .. }));
        assert!(matches!(p.decide(Some(503), 1), RetryDecision::Retry { .. }));
        assert!(matches!(p.decide(Some(503), 2), RetryDecision::Retry { .. }));
        assert_eq!(p.decide(Some(503), 3), RetryDecision::GiveUp);
    }

    #[test]
    fn retry_on_429() {
        let p = RetryPolicy::r2_4_default();
        assert!(matches!(p.decide(Some(429), 0), RetryDecision::Retry { .. }));
    }

    #[test]
    fn give_up_on_4xx() {
        let p = RetryPolicy::r2_4_default();
        assert_eq!(p.decide(Some(400), 0), RetryDecision::GiveUp);
        assert_eq!(p.decide(Some(403), 0), RetryDecision::GiveUp);
        assert_eq!(p.decide(Some(404), 0), RetryDecision::GiveUp);
    }

    #[test]
    fn transport_error_retries() {
        let p = RetryPolicy::r2_4_default();
        assert!(matches!(p.decide(None, 0), RetryDecision::Retry { .. }));
        assert_eq!(p.decide(None, 3), RetryDecision::GiveUp);
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let p = RetryPolicy::r2_4_default();
        assert_eq!(p.backoff_for(0), Duration::from_millis(250));
        assert_eq!(p.backoff_for(1), Duration::from_millis(500));
        assert_eq!(p.backoff_for(2), Duration::from_millis(1000));
        assert_eq!(p.backoff_for(3), Duration::from_millis(2000));
        // Eventually clamped at max_backoff
        assert_eq!(p.backoff_for(30), Duration::from_secs(8));
    }
}
