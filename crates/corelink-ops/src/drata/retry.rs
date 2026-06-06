//! Synchronous retry policy. Mirrors the canonical R2-4 Slack policy
//! (3 retries, 250ms initial, 8s cap) — identical contract, audited and
//! property-tested upstream.

use std::time::Duration;

/// Retry decision returned by [`RetryPolicy::decide`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryDecision {
    /// 2xx — exit retry loop with success.
    Success,
    /// Sleep `delay` then retry.
    Retry {
        /// Backoff delay before next attempt.
        delay: Duration,
    },
    /// Non-retryable failure — propagate to caller.
    GiveUp,
}

/// Retry policy parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum retries excluding the initial attempt.
    pub max_retries: u32,
    /// First backoff; doubled on each retry.
    pub initial_backoff: Duration,
    /// Backoff ceiling.
    pub max_backoff: Duration,
}

impl RetryPolicy {
    /// Canonical R5-prep Drata-sync policy: 3 retries, 250ms initial,
    /// 8s cap. Matches the R2-4 Slack retry envelope.
    #[must_use]
    pub const fn r5p_default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(8),
        }
    }

    /// Compute the backoff for a 0-based attempt index.
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

    /// Decide the next action given an HTTP status (or transport
    /// failure → `None`) and the current attempt index.
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
        Self::r5p_default()
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
        let p = RetryPolicy::r5p_default();
        assert_eq!(p.decide(Some(200), 0), RetryDecision::Success);
        assert_eq!(p.decide(Some(201), 1), RetryDecision::Success);
    }

    #[test]
    fn retry_then_give_up_on_5xx() {
        let p = RetryPolicy::r5p_default();
        assert!(matches!(
            p.decide(Some(503), 0),
            RetryDecision::Retry { .. }
        ));
        assert_eq!(p.decide(Some(503), 3), RetryDecision::GiveUp);
    }

    #[test]
    fn give_up_on_4xx() {
        let p = RetryPolicy::r5p_default();
        assert_eq!(p.decide(Some(400), 0), RetryDecision::GiveUp);
        assert_eq!(p.decide(Some(401), 0), RetryDecision::GiveUp);
        assert_eq!(p.decide(Some(404), 0), RetryDecision::GiveUp);
    }

    #[test]
    fn backoff_is_capped() {
        let p = RetryPolicy::r5p_default();
        assert_eq!(p.backoff_for(0), Duration::from_millis(250));
        assert_eq!(p.backoff_for(30), Duration::from_secs(8));
    }
}
