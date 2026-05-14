//! Exponential backoff retry policy for Stripe API calls.
//!
//! Pure-logic module: computes the next sleep duration given a retry
//! attempt index + the optional `Retry-After` header value. The
//! caller drives the actual sleep (sync via `std::thread::sleep` or
//! async via `tokio::time::sleep`).

/// Default max retries per spec.
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// Default base backoff in milliseconds: 250ms × 2^n capped at 8s.
pub const DEFAULT_BASE_MS: u64 = 250;

/// Default backoff cap in milliseconds.
pub const DEFAULT_CAP_MS: u64 = 8_000;

/// Exponential backoff retry policy.
///
/// Schedule (without `Retry-After` override):
/// - attempt 0 → 250ms
/// - attempt 1 → 500ms
/// - attempt 2 → 1000ms
/// - attempt 3 → 2000ms
/// - attempt 4 → 4000ms
/// - attempt 5+ → 8000ms (cap)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetryPolicy {
    /// Maximum retry attempts before giving up.
    pub max_retries: u32,
    /// Base backoff (ms) for the geometric sequence.
    pub base_ms: u64,
    /// Cap (ms) on the geometric sequence.
    pub cap_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_ms: DEFAULT_BASE_MS,
            cap_ms: DEFAULT_CAP_MS,
        }
    }
}

impl RetryPolicy {
    /// Construct a policy with explicit knobs.
    #[must_use]
    pub const fn new(max_retries: u32, base_ms: u64, cap_ms: u64) -> Self {
        Self {
            max_retries,
            base_ms,
            cap_ms,
        }
    }

    /// Compute the next sleep duration in milliseconds for `attempt`
    /// (0-indexed). If `retry_after_seconds` is set (from server
    /// header), it wins.
    ///
    /// Returns `None` if `attempt >= max_retries` (caller should give up).
    #[must_use]
    pub fn next_sleep_ms(&self, attempt: u32, retry_after_seconds: Option<u64>) -> Option<u64> {
        if attempt >= self.max_retries {
            return None;
        }
        if let Some(s) = retry_after_seconds {
            return Some(s.saturating_mul(1000));
        }
        // base * 2^attempt, capped.
        let shifted = self.base_ms.checked_shl(attempt).unwrap_or(self.cap_ms);
        Some(shifted.min(self.cap_ms))
    }

    /// Returns true if `http_status` is retryable (5xx or 429).
    #[must_use]
    pub const fn is_retryable_status(http_status: u16) -> bool {
        http_status == 429 || http_status >= 500
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
    fn backoff_schedule_geometric() {
        let p = RetryPolicy::default();
        assert_eq!(p.next_sleep_ms(0, None), Some(250));
        assert_eq!(p.next_sleep_ms(1, None), Some(500));
        assert_eq!(p.next_sleep_ms(2, None), Some(1_000));
        assert_eq!(p.next_sleep_ms(3, None), Some(2_000));
        assert_eq!(p.next_sleep_ms(4, None), Some(4_000));
    }

    #[test]
    fn backoff_capped_at_cap() {
        let p = RetryPolicy::new(20, 250, 8_000);
        assert_eq!(p.next_sleep_ms(6, None), Some(8_000));
        assert_eq!(p.next_sleep_ms(10, None), Some(8_000));
    }

    #[test]
    fn retry_after_header_wins() {
        let p = RetryPolicy::default();
        assert_eq!(p.next_sleep_ms(0, Some(3)), Some(3_000));
    }

    #[test]
    fn gives_up_at_max_retries() {
        let p = RetryPolicy::default();
        assert_eq!(p.next_sleep_ms(5, None), None);
        assert_eq!(p.next_sleep_ms(99, None), None);
    }

    #[test]
    fn retryable_status_classification() {
        assert!(RetryPolicy::is_retryable_status(429));
        assert!(RetryPolicy::is_retryable_status(500));
        assert!(RetryPolicy::is_retryable_status(503));
        assert!(!RetryPolicy::is_retryable_status(200));
        assert!(!RetryPolicy::is_retryable_status(400));
        assert!(!RetryPolicy::is_retryable_status(401));
        assert!(!RetryPolicy::is_retryable_status(404));
    }
}
