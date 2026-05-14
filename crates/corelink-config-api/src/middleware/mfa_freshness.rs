//! MFA freshness guard (CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS).
//!
//! Every config write path (update + rollback) MUST call
//! [`check_mfa_freshness`] before executing any mutation. If the MFA
//! timestamp is more than 30 minutes old the operation is rejected with
//! [`ApiError::MfaStale`].
//!
//! # Clock-skew tolerance
//!
//! A 60-second forward-skew grace is applied (`mfa_ts_ms` may be up to 60s
//! in the future relative to `now_ms`) to tolerate NTP drift between the
//! authn service and the admin plane handler.

use crate::error::ApiError;

/// MFA freshness window: 30 minutes in milliseconds.
pub const MFA_FRESHNESS_WINDOW_MS: u64 = 30 * 60 * 1000;

/// Forward-skew grace for NTP drift: 60 seconds.
pub const MFA_FORWARD_SKEW_GRACE_MS: u64 = 60 * 1000;

/// Check that the admin's MFA session is fresh (≤ 30 min old).
///
/// # Arguments
///
/// - `mfa_ts_ms`: MFA completion timestamp (Unix ms) from the session token.
/// - `now_ms`: Current wall-clock time (Unix ms) from the handler context.
///
/// # Errors
///
/// Returns [`ApiError::MfaStale`] if the MFA timestamp is older than 30 min.
/// Returns [`ApiError::MfaStale`] if the MFA timestamp is implausibly far in
/// the future (more than the forward-skew grace).
pub fn check_mfa_freshness(mfa_ts_ms: u64, now_ms: u64) -> Result<(), ApiError> {
    // Reject implausibly future MFA timestamps (replay / clock skew attack).
    if mfa_ts_ms > now_ms + MFA_FORWARD_SKEW_GRACE_MS {
        return Err(ApiError::MfaStale {
            mfa_ts_ms,
            now_ms,
            window_ms: MFA_FRESHNESS_WINDOW_MS,
        });
    }

    let age_ms = now_ms.saturating_sub(mfa_ts_ms);
    if age_ms > MFA_FRESHNESS_WINDOW_MS {
        return Err(ApiError::MfaStale {
            mfa_ts_ms,
            now_ms,
            window_ms: MFA_FRESHNESS_WINDOW_MS,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn fresh_mfa_passes() {
        let now = 10_000_000u64;
        let mfa_ts = now - 5 * 60 * 1000; // 5 minutes ago
        assert!(check_mfa_freshness(mfa_ts, now).is_ok());
    }

    #[test]
    fn stale_mfa_exactly_30min_plus_1ms_fails() {
        let now = 10_000_000u64;
        let mfa_ts = now - MFA_FRESHNESS_WINDOW_MS - 1;
        assert!(check_mfa_freshness(mfa_ts, now).is_err());
    }

    #[test]
    fn mfa_exactly_30min_passes() {
        let now = 10_000_000u64;
        let mfa_ts = now - MFA_FRESHNESS_WINDOW_MS;
        assert!(check_mfa_freshness(mfa_ts, now).is_ok());
    }

    #[test]
    fn future_mfa_within_grace_passes() {
        let now = 10_000_000u64;
        let mfa_ts = now + 30_000; // 30s in future (within 60s grace)
        assert!(check_mfa_freshness(mfa_ts, now).is_ok());
    }

    #[test]
    fn future_mfa_beyond_grace_fails() {
        let now = 10_000_000u64;
        let mfa_ts = now + MFA_FORWARD_SKEW_GRACE_MS + 1;
        assert!(check_mfa_freshness(mfa_ts, now).is_err());
    }
}
