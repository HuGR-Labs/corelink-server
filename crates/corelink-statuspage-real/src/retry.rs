//! Synchronous retry policy for Statuspage Public-Metric POSTs.
//!
//! Statuspage Public-Metric API returns:
//! - `2xx` on success (we accept any 2xx; the API typically returns
//!   `201 Created` on a fresh data-point ingestion).
//! - `401` / `403` on auth failure (fatal — surface as a distinct
//!   audit outcome).
//! - `429` on quota exhaustion (transient — back off + retry).
//! - `5xx` on transient infra failure (back off + retry).
//! - Other `4xx` on permanent reject (give up).

use std::time::Duration;

/// Decision returned by the retry policy for a single attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryDecision {
    /// Attempt succeeded — exit the loop.
    Success,
    /// Caller should retry after `delay`.
    Retry {
        /// Delay before next attempt.
        delay: Duration,
    },
    /// Caller must give up — auth failed (401 / 403).
    GiveUpAuth,
    /// Caller must give up — permanent reject (other 4xx) or retries
    /// exhausted on a transient class.
    GiveUp,
}

/// Retry policy parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of retries (NOT counting the initial attempt).
    pub max_retries: u32,
    /// Initial back-off delay; doubled per retry.
    pub initial_backoff: Duration,
    /// Cap on back-off delay.
    pub max_backoff: Duration,
}

impl RetryPolicy {
    /// Canonical wave-16 policy: 3 retries, 250ms initial, 8s cap.
    #[must_use]
    pub const fn wave16_default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(8),
        }
    }

    /// Compute exponential back-off for a 0-based attempt index.
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
    #[must_use]
    pub fn decide(&self, status: Option<u16>, attempt: u32) -> RetryDecision {
        match status {
            Some(s) if (200..300).contains(&s) => RetryDecision::Success,
            Some(s) if s == 401 || s == 403 => RetryDecision::GiveUpAuth,
            Some(s) if s == 429 || (500..600).contains(&s) => {
                if attempt < self.max_retries {
                    RetryDecision::Retry {
                        delay: self.backoff_for(attempt),
                    }
                } else {
                    RetryDecision::GiveUp
                }
            }
            Some(_) => RetryDecision::GiveUp,
            None => {
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
        Self::wave16_default()
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
        let p = RetryPolicy::wave16_default();
        assert_eq!(p.decide(Some(200), 0), RetryDecision::Success);
        assert_eq!(p.decide(Some(201), 0), RetryDecision::Success);
        assert_eq!(p.decide(Some(204), 2), RetryDecision::Success);
    }

    #[test]
    fn auth_failure_is_distinct_giveup() {
        let p = RetryPolicy::wave16_default();
        assert_eq!(p.decide(Some(401), 0), RetryDecision::GiveUpAuth);
        assert_eq!(p.decide(Some(403), 0), RetryDecision::GiveUpAuth);
    }

    #[test]
    fn retry_on_429_until_exhausted() {
        let p = RetryPolicy::wave16_default();
        assert!(matches!(p.decide(Some(429), 0), RetryDecision::Retry { .. }));
        assert!(matches!(p.decide(Some(429), 1), RetryDecision::Retry { .. }));
        assert!(matches!(p.decide(Some(429), 2), RetryDecision::Retry { .. }));
        assert_eq!(p.decide(Some(429), 3), RetryDecision::GiveUp);
    }

    #[test]
    fn retry_on_5xx_until_exhausted() {
        let p = RetryPolicy::wave16_default();
        assert!(matches!(p.decide(Some(503), 0), RetryDecision::Retry { .. }));
        assert_eq!(p.decide(Some(503), 3), RetryDecision::GiveUp);
    }

    #[test]
    fn give_up_on_other_4xx() {
        let p = RetryPolicy::wave16_default();
        assert_eq!(p.decide(Some(404), 0), RetryDecision::GiveUp);
        assert_eq!(p.decide(Some(422), 0), RetryDecision::GiveUp);
    }

    #[test]
    fn transport_error_retries() {
        let p = RetryPolicy::wave16_default();
        assert!(matches!(p.decide(None, 0), RetryDecision::Retry { .. }));
        assert_eq!(p.decide(None, 3), RetryDecision::GiveUp);
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let p = RetryPolicy::wave16_default();
        assert_eq!(p.backoff_for(0), Duration::from_millis(250));
        assert_eq!(p.backoff_for(1), Duration::from_millis(500));
        assert_eq!(p.backoff_for(2), Duration::from_millis(1000));
        assert_eq!(p.backoff_for(30), Duration::from_secs(8));
    }
}
