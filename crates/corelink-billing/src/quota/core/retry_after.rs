//! PROVISIONAL Retry-After computation (Lote 10.7-tris cycle 4 +
//! cycle 6 framing; transitional per ADR-0020 FROZEN).
//!
//! ## PROVISIONAL framing
//!
//! Per WI-S07-003 narrative §1 + spec_contract §1 boundary, S-07 ships
//! a **transitional** 429 + Retry-After response when the 100% deny arm
//! fires. The canonical production-grade `Retry-After: days-until-month-reset`
//! semantic is owned by S-08 CAP-QUOTA-001 (rate-limit DO with monthly
//! billing-cycle alignment).
//!
//! This module ships the PROVISIONAL formula used in the transitional
//! window:
//!
//! ```text
//! retry_after_secs = max(
//!     RETRY_AFTER_FLOOR_SECS,
//!     min(RETRY_AFTER_HARD_CEILING_SECS, base_secs × over_quota_ratio_pct)
//! )
//! ```
//!
//! Where:
//! - `over_quota_ratio_pct = max(1.0, bytes_used / bytes_quota)` — the
//!   over-quota multiplier; a tenant exactly at 100% returns 1.0 (the
//!   floor is the relevant arm); a tenant at 200% returns 2.0 (the
//!   ratio scales the wait time linearly so heavy over-quota tenants
//!   are throttled longer in the transitional window).
//! - `base_secs = floor` — we anchor the computation at the floor so a
//!   tenant exactly at 100% returns the floor (60s default).
//!
//! ## Why scale linearly with over-quota ratio
//!
//! - At 100% the floor (60s) gives the eviction trigger window to
//!   reduce `bytes_used`; client retries succeed shortly after.
//! - At 200% (extreme over-quota; misconfigured tenant) we double the
//!   wait so the operator has time to investigate.
//! - The hard ceiling (1 day) prevents the PROVISIONAL stub from
//!   shipping a `Retry-After: 30d` value that would surprise customers
//!   when S-08 lands the canonical formula.

/// PROVISIONAL hard ceiling for the Retry-After value (seconds).
/// 1 day caps the transitional response — the canonical S-08
/// `days-until-month-reset` formula returns much larger values for
/// tenants who hit quota mid-month, but that semantic is OUT of S-07
/// scope per ADR-0020 FROZEN.
pub const RETRY_AFTER_HARD_CEILING_SECS: u64 = 86_400;

/// Compute the PROVISIONAL Retry-After value (seconds) for a tenant
/// whose `bytes_used / bytes_quota` ratio crosses the deny boundary.
///
/// `floor_secs` anchors the floor (default 60s per spec contract);
/// `bytes_used` and `bytes_quota` come from `tenant_storage_state`.
///
/// Defensive invariants:
/// - `bytes_quota == 0` → returns `floor_secs` (misconfigured tenant;
///   defer to operator).
/// - `bytes_used <= bytes_quota` → returns `floor_secs` (caller hit
///   the deny boundary on `would_use`, not on current `bytes_used`;
///   the floor is the canonical answer in this band).
/// - `bytes_used > bytes_quota` → linearly scaled by the over-quota
///   ratio, clamped to `RETRY_AFTER_HARD_CEILING_SECS`.
#[must_use]
pub fn provisional_retry_after_secs(floor_secs: u64, bytes_used: u64, bytes_quota: u64) -> u64 {
    if floor_secs == 0 {
        // Defensive — caller should never pass 0; returning 60s preserves
        // a sensible client behaviour even on misconfiguration.
        return RETRY_AFTER_HARD_CEILING_SECS.min(60);
    }
    if bytes_quota == 0 {
        return floor_secs;
    }
    if bytes_used <= bytes_quota {
        return floor_secs;
    }
    let used_f = bytes_used as f64;
    let quota_f = bytes_quota as f64;
    let ratio = used_f / quota_f;
    let scaled_f = (floor_secs as f64) * ratio;
    // Floor + clamp to hard ceiling.
    let scaled = scaled_f.ceil() as u64;
    let clamped = scaled.min(RETRY_AFTER_HARD_CEILING_SECS);
    clamped.max(floor_secs)
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
    fn hard_ceiling_pinned_to_one_day() {
        assert_eq!(RETRY_AFTER_HARD_CEILING_SECS, 86_400);
    }

    #[test]
    fn at_exactly_quota_returns_floor() {
        assert_eq!(provisional_retry_after_secs(60, 100, 100), 60);
    }

    #[test]
    fn below_quota_returns_floor() {
        // Should not normally fire (caller hit deny on `would_use`),
        // but the function MUST return floor in this band rather than
        // panicking or scaling negative.
        assert_eq!(provisional_retry_after_secs(60, 50, 100), 60);
    }

    #[test]
    fn double_quota_doubles_floor() {
        // bytes_used = 200, bytes_quota = 100 → ratio 2.0 → 60 × 2 = 120.
        assert_eq!(provisional_retry_after_secs(60, 200, 100), 120);
    }

    #[test]
    fn ten_x_quota_returns_ten_x_floor_within_ceiling() {
        // 60 × 10 = 600 < 86_400.
        assert_eq!(provisional_retry_after_secs(60, 1000, 100), 600);
    }

    #[test]
    fn extreme_over_quota_clamped_to_hard_ceiling() {
        // 60 × 10000 = 600_000 > 86_400 → clamp to 86_400.
        let v = provisional_retry_after_secs(60, 1_000_000, 100);
        assert_eq!(v, RETRY_AFTER_HARD_CEILING_SECS);
    }

    #[test]
    fn zero_quota_returns_floor() {
        // Misconfigured tenant — fall back to floor.
        assert_eq!(provisional_retry_after_secs(60, 100, 0), 60);
    }

    #[test]
    fn larger_floor_respected_at_boundary() {
        // Custom floor 300s; tenant exactly at quota → 300s.
        assert_eq!(provisional_retry_after_secs(300, 100, 100), 300);
    }

    #[test]
    fn larger_floor_scaled_linearly() {
        // Custom floor 120s; double quota → 240s.
        assert_eq!(provisional_retry_after_secs(120, 200, 100), 240);
    }

    #[test]
    fn zero_floor_defensive_returns_sensible_minimum() {
        // Caller passed 0 (programmer error). Function MUST NOT panic
        // or return 0 (which would create a hot-spin retry loop).
        let v = provisional_retry_after_secs(0, 100, 100);
        assert!(v > 0);
        assert!(v <= 60);
    }

    #[test]
    fn monotone_in_over_quota_ratio() {
        let a = provisional_retry_after_secs(60, 110, 100);
        let b = provisional_retry_after_secs(60, 200, 100);
        let c = provisional_retry_after_secs(60, 500, 100);
        assert!(a <= b);
        assert!(b <= c);
        assert!(c <= RETRY_AFTER_HARD_CEILING_SECS);
    }

    #[test]
    fn always_at_least_floor() {
        // Property: result >= floor for every valid input.
        for used in [0_u64, 50, 100, 150, 1000, u64::MAX] {
            for quota in [0_u64, 1, 100, u64::MAX] {
                let v = provisional_retry_after_secs(60, used, quota);
                assert!(
                    v >= 60,
                    "result {v} < floor 60 (used={used}, quota={quota})"
                );
            }
        }
    }
}
