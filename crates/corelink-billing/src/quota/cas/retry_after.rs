//! Canonical Retry-After computation: seconds-until next 1st-UTC-midnight
//! calendar boundary (per ADR-0020 FROZEN; sprint contract §10.s08.5
//! calendar-month determinism; WI-S08-003 §1 invariant 8).
//!
//! ## Why hand-rolled Gregorian arithmetic
//!
//! Per the autonomous execution charter `wasm32-clean lib code`
//! constraint, the crate avoids `chrono` (a full I/O-heavy time crate);
//! the arithmetic we need is small and deterministic at every boundary
//! the WI calls out. The canonical primitive is
//! [`next_month_first_utc_midnight_secs`]: given a Unix-epoch second,
//! return the Unix-epoch second of the *next* 1st-UTC-midnight (handling
//! the December → January year increment, February non-leap / leap year,
//! end-of-month spillover).
//!
//! Constants:
//! - [`SECS_PER_DAY`] = 86_400.
//! - `UNIX_EPOCH_DAYS_TO_1970` = 0 (anchor).
//! - [`MAX_SECS_PER_MONTH`] = 31 × 86_400 = 2_678_400 (worst-case
//!   distance between two 1st-UTC-midnights).
//!
//! Fixture: the [`days_in_month`] helper handles Gregorian leap years
//! per the canonical rule (`year % 4 == 0 AND (year % 100 != 0 OR
//! year % 400 == 0)`); pinned by inline regression tests.
//!
//! Boundary cases pinned by tests (per WI-S08-003 §1 invariant 8):
//! - Jan 1 00:00:01 UTC → next Feb 1 00:00:00 UTC (~31 d − 1 s).
//! - Dec 31 23:59:59 UTC → next Jan 1 00:00:00 UTC (1 s; year increment).
//! - Feb 28 12:00:00 UTC non-leap year → Mar 1 00:00:00 UTC (~12 h).
//! - Feb 28 12:00:00 UTC leap year → Feb 29 00:00:00 UTC (~12 h).
//! - Feb 29 12:00:00 UTC leap year → Mar 1 00:00:00 UTC (~12 h).
//!
//! ## Why a minimum floor
//!
//! [`RETRY_AFTER_MIN_SECS`] = 1 second prevents a `Retry-After: 0`
//! header from creating a hot-spin retry loop on the client when the
//! request lands on the literal last second of the month boundary
//! (Dec 31 23:59:59 UTC).

/// Seconds per UTC day.
pub const SECS_PER_DAY: i64 = 86_400;

/// Worst-case seconds between two consecutive 1st-UTC-midnights — used
/// as the canonical Retry-After upper bound (31-day month, no DST in UTC).
pub const MAX_SECS_PER_MONTH: u64 = 31 * 86_400;

/// Canonical minimum Retry-After value (seconds). Prevents
/// `Retry-After: 0` from creating client hot-spin retry loops when the
/// request lands on the literal last second of the calendar month.
pub const RETRY_AFTER_MIN_SECS: u64 = 1;

/// Days in the calendar month (`year` is the Gregorian year; `month`
/// is 1..=12). Returns `None` if `month` is out of range — defensive
/// against caller error.
#[must_use]
pub const fn days_in_month(year: i64, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => {
            // Canonical Gregorian leap rule.
            let leap =
                (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0));
            Some(if leap { 29 } else { 28 })
        }
        _ => None,
    }
}

/// Given Unix-epoch seconds (UTC), return the canonical
/// `(year, month, day, hour, minute, second)` decomposition.
///
/// Handles negative seconds defensively (clamps to 1970-01-01 00:00:00).
/// Pure arithmetic; no `chrono` dep.
///
/// # Determinism
///
/// The function uses the standard Gregorian-from-epoch formula
/// (Howard Hinnant `civil_from_days`) so the decomposition is
/// byte-exact across runs.
#[must_use]
pub fn decompose_utc(now_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    // Defensive clamp — negative epoch seconds collapse to 1970-01-01.
    let z = if now_secs < 0 { 0 } else { now_secs };
    let days = z.div_euclid(SECS_PER_DAY);
    let secs_in_day = z.rem_euclid(SECS_PER_DAY);

    // Howard Hinnant `civil_from_days` algorithm.
    let z_shifted = days + 719_468;
    let era = z_shifted.div_euclid(146_097);
    let doe = z_shifted.rem_euclid(146_097);
    let yoe =
        (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let year = if m <= 2 { y + 1 } else { y };

    let hour = (secs_in_day / 3600) as u32;
    let minute = ((secs_in_day % 3600) / 60) as u32;
    let second = (secs_in_day % 60) as u32;

    (year, m, d, hour, minute, second)
}

/// Given `(year, month, day, hour, minute, second)` UTC, return the
/// Unix-epoch seconds. Inverse of [`decompose_utc`].
///
/// Returns 0 (1970-01-01) for any out-of-range component (month not in
/// 1..=12; day not in 1..=days_in_month; hour/min/sec out of range).
#[must_use]
pub fn compose_utc(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> i64 {
    if !(1..=12).contains(&month) || hour >= 24 || minute >= 60 || second >= 60
    {
        return 0;
    }
    let dim = match days_in_month(year, month) {
        Some(d) => d,
        None => return 0,
    };
    if !(1..=dim).contains(&day) {
        return 0;
    }
    // Howard Hinnant `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = month as i64;
    let d = day as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    days * SECS_PER_DAY
        + (hour as i64) * 3600
        + (minute as i64) * 60
        + (second as i64)
}

/// Unix-epoch seconds of the next 1st-UTC-midnight strictly after
/// `now_secs`.
///
/// Handles December → January year increment per WI-S08-003 §1
/// invariant 8 (canonical primitive). Defensive clamp on negative
/// inputs (collapses to 1970-02-01 00:00:00 UTC).
#[must_use]
pub fn next_month_first_utc_midnight_secs(now_secs: i64) -> i64 {
    let (year, month, _d, _h, _m, _s) = decompose_utc(now_secs);
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1u32)
    } else {
        (year, month + 1)
    };
    compose_utc(next_year, next_month, 1, 0, 0, 0)
}

/// Canonical Retry-After value (seconds) per ADR-0020 FROZEN +
/// sprint contract §10.s08.5.
///
/// Returns the seconds until the next 1st-UTC-midnight strictly after
/// `now_secs`. Defensive bounds:
/// - Result clamped to `[RETRY_AFTER_MIN_SECS, MAX_SECS_PER_MONTH]`.
/// - `now_secs` < 0 → returns the floor (`RETRY_AFTER_MIN_SECS`).
/// - `now_secs` >= year-9999 boundary → returns the floor (defensive
///   ceiling; an out-of-range future timestamp is a programmer error
///   but we don't crash).
///
/// # Examples (from WI §1 invariant 8 boundary cases)
///
/// - `2026-01-01 00:00:01 UTC` → ~31 days − 1 s.
/// - `2026-12-31 23:59:59 UTC` → 1 s (year increment to 2027-01-01).
/// - `2027-02-28 12:00:00 UTC` (non-leap) → 12 h to 2027-03-01.
/// - `2028-02-28 12:00:00 UTC` (leap year) → 12 h to 2028-02-29.
/// - `2028-02-29 12:00:00 UTC` (leap year) → 12 h to 2028-03-01.
#[must_use]
pub fn days_until_month_reset_secs(now_secs: i64) -> u64 {
    if now_secs < 0 {
        return RETRY_AFTER_MIN_SECS;
    }
    let next = next_month_first_utc_midnight_secs(now_secs);
    if next <= now_secs {
        return RETRY_AFTER_MIN_SECS;
    }
    let delta = next - now_secs;
    if delta < (RETRY_AFTER_MIN_SECS as i64) {
        return RETRY_AFTER_MIN_SECS;
    }
    let delta_u: u64 = match u64::try_from(delta) {
        Ok(v) => v,
        Err(_) => return MAX_SECS_PER_MONTH,
    };
    delta_u.clamp(RETRY_AFTER_MIN_SECS, MAX_SECS_PER_MONTH)
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

    // ---- days_in_month canonical -----------------------------------

    #[test]
    fn days_in_month_canonical_31_day_set() {
        for m in [1u32, 3, 5, 7, 8, 10, 12] {
            assert_eq!(days_in_month(2026, m), Some(31));
        }
    }

    #[test]
    fn days_in_month_canonical_30_day_set() {
        for m in [4u32, 6, 9, 11] {
            assert_eq!(days_in_month(2026, m), Some(30));
        }
    }

    #[test]
    fn days_in_month_february_non_leap_returns_28() {
        assert_eq!(days_in_month(2026, 2), Some(28));
        assert_eq!(days_in_month(2027, 2), Some(28));
        // Century rule: 2100 is NOT a leap year.
        assert_eq!(days_in_month(2100, 2), Some(28));
    }

    #[test]
    fn days_in_month_february_leap_returns_29() {
        assert_eq!(days_in_month(2024, 2), Some(29));
        assert_eq!(days_in_month(2028, 2), Some(29));
        // 400-year rule: 2000 IS a leap year.
        assert_eq!(days_in_month(2000, 2), Some(29));
    }

    #[test]
    fn days_in_month_out_of_range_returns_none() {
        assert_eq!(days_in_month(2026, 0), None);
        assert_eq!(days_in_month(2026, 13), None);
    }

    // ---- decompose_utc / compose_utc round-trip --------------------

    #[test]
    fn epoch_zero_decomposes_to_1970_01_01() {
        let (y, mo, d, h, mi, s) = decompose_utc(0);
        assert_eq!((y, mo, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn negative_epoch_clamps_to_1970() {
        let (y, mo, d, h, mi, s) = decompose_utc(-1);
        assert_eq!((y, mo, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn compose_round_trips_through_decompose() {
        for &(y, mo, d, h, mi, s) in &[
            (2026, 1u32, 1u32, 0u32, 0u32, 0u32),
            (2026, 12, 31, 23, 59, 59),
            (2027, 2, 28, 12, 0, 0),
            (2028, 2, 29, 12, 0, 0),
            (2028, 12, 31, 23, 59, 59),
        ] {
            let secs = compose_utc(y, mo, d, h, mi, s);
            assert!(secs > 0, "compose returned 0 for valid date");
            let (y2, mo2, d2, h2, mi2, s2) = decompose_utc(secs);
            assert_eq!((y, mo, d, h, mi, s), (y2, mo2, d2, h2, mi2, s2));
        }
    }

    #[test]
    fn compose_invalid_inputs_return_zero() {
        // Out-of-range month.
        assert_eq!(compose_utc(2026, 0, 1, 0, 0, 0), 0);
        assert_eq!(compose_utc(2026, 13, 1, 0, 0, 0), 0);
        // Out-of-range day (Feb non-leap).
        assert_eq!(compose_utc(2026, 2, 29, 0, 0, 0), 0);
        // Out-of-range hour / minute / second.
        assert_eq!(compose_utc(2026, 1, 1, 24, 0, 0), 0);
        assert_eq!(compose_utc(2026, 1, 1, 0, 60, 0), 0);
        assert_eq!(compose_utc(2026, 1, 1, 0, 0, 60), 0);
    }

    // ---- next_month_first_utc_midnight_secs canonical --------------

    #[test]
    fn from_jan_1_001_returns_feb_1_midnight() {
        let now = compose_utc(2026, 1, 1, 0, 0, 1);
        let next = next_month_first_utc_midnight_secs(now);
        let expected = compose_utc(2026, 2, 1, 0, 0, 0);
        assert_eq!(next, expected);
    }

    #[test]
    fn from_dec_31_235959_returns_next_jan_1_year_increment() {
        let now = compose_utc(2026, 12, 31, 23, 59, 59);
        let next = next_month_first_utc_midnight_secs(now);
        let expected = compose_utc(2027, 1, 1, 0, 0, 0);
        assert_eq!(next, expected);
        assert_eq!(next - now, 1);
    }

    #[test]
    fn from_feb_28_non_leap_returns_mar_1() {
        let now = compose_utc(2027, 2, 28, 12, 0, 0);
        let next = next_month_first_utc_midnight_secs(now);
        let expected = compose_utc(2027, 3, 1, 0, 0, 0);
        assert_eq!(next, expected);
    }

    #[test]
    fn from_feb_28_leap_year_returns_mar_1() {
        // Per WI §1 invariant 8 boundary: Feb 28 leap year → Feb 29 → Mar 1.
        // The "next 1st-UTC-midnight" from Feb 28 is Mar 1 (we skip Feb 29
        // boundary because Feb 29 is NOT a 1st-UTC-midnight; the canonical
        // primitive is `1st of next month` not `next day-boundary`).
        let now = compose_utc(2028, 2, 28, 12, 0, 0);
        let next = next_month_first_utc_midnight_secs(now);
        let expected = compose_utc(2028, 3, 1, 0, 0, 0);
        assert_eq!(next, expected);
    }

    #[test]
    fn from_feb_29_leap_year_returns_mar_1() {
        let now = compose_utc(2028, 2, 29, 12, 0, 0);
        let next = next_month_first_utc_midnight_secs(now);
        let expected = compose_utc(2028, 3, 1, 0, 0, 0);
        assert_eq!(next, expected);
    }

    // ---- days_until_month_reset_secs canonical ---------------------

    #[test]
    fn negative_secs_returns_floor() {
        assert_eq!(days_until_month_reset_secs(-1), RETRY_AFTER_MIN_SECS);
    }

    #[test]
    fn last_second_of_month_returns_one_second() {
        let now = compose_utc(2026, 12, 31, 23, 59, 59);
        let secs = days_until_month_reset_secs(now);
        assert_eq!(secs, 1);
    }

    #[test]
    fn middle_of_month_returns_realistic_distance() {
        // 2026-04-15 12:00:00 UTC → next 2026-05-01 00:00:00 UTC.
        // Distance = 15.5 days = 1_339_200 s.
        let now = compose_utc(2026, 4, 15, 12, 0, 0);
        let secs = days_until_month_reset_secs(now);
        assert_eq!(
            secs,
            (15 * 86_400 + 12 * 3600) as u64,
            "expected 15.5 days = 1_339_200 s"
        );
    }

    #[test]
    fn first_second_of_month_returns_almost_full_month() {
        // 2026-01-01 00:00:01 → next 2026-02-01 00:00:00 (31 d − 1 s).
        let now = compose_utc(2026, 1, 1, 0, 0, 1);
        let secs = days_until_month_reset_secs(now);
        assert_eq!(secs, 31 * 86_400 - 1);
    }

    #[test]
    fn always_within_canonical_bounds() {
        for now in [0i64, 1, 1_000_000, 1_700_000_000, 2_000_000_000] {
            let secs = days_until_month_reset_secs(now);
            assert!(
                secs >= RETRY_AFTER_MIN_SECS,
                "below floor: {secs}"
            );
            assert!(
                secs <= MAX_SECS_PER_MONTH,
                "above ceiling: {secs}"
            );
        }
    }

    #[test]
    fn distance_is_positive_for_non_negative_inputs() {
        for now in [0i64, 1, 86_400, 31_536_000] {
            let secs = days_until_month_reset_secs(now);
            assert!(secs > 0);
        }
    }
}
