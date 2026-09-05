//! Canonical DSR response deadlines and calendar arithmetic.
//!
//! This module owns the legal SLA constants and the pure calendar-month
//! implementation used by the public [`super::sla_for`] helper. Keeping the
//! date arithmetic together makes its boundary and mutation tests travel with
//! the responsibility they protect.

use super::DsrJurisdiction;

/// LGPD Art. 19 canonical SLA: 15 days for the controller response.
pub const SLA_LGPD_DAYS: u32 = 15;

/// GDPR Art. 12(3) reference window: "one calendar month" ≈ 30 days.
/// RETAINED as a documentation / sizing reference only — the canonical
/// GDPR deadline in [`super::sla_for`] is a true CALENDAR-month add (see
/// [`add_one_calendar_month_ms`]), NOT this flat day count. Computing
/// `30 * 86_400_000` overshoots the legal "one month" deadline for any
/// month shorter than 30 days (a Jan-31 submission is legally due
/// Feb-28/29, not Mar-2). The law allows extension to 3 months for
/// complex requests (Privacy Officer review path).
pub const SLA_GDPR_DAYS: u32 = 30;

/// CCPA §1798.130 canonical SLA: 45 days.
pub const SLA_CCPA_DAYS: u32 = 45;

/// JWT receipt expiration window (anti-replay cap). Per WI-S11-001 §1:
/// receipts older than 90 days fail verification. The customer's
/// proof-of-submission token is short-lived; the canonical durable
/// audit trail (R2 audit Object Lock 7y) is the long-term forensic
/// record.
pub const RECEIPT_EXPIRY_DAYS: u32 = 90;

/// Compute the canonical SLA deadline (Unix epoch ms) for a given
/// (jurisdiction, submitted_at) pair. The trait-level deadline is the
/// un-extended canonical SLA per jurisdiction; production wiring at
/// WI-S11-008 may extend this for "complex" GDPR Art. 12.3 cases (cap
/// 3 months = 90 days; the extension is decided by the Privacy
/// Officer review path).
#[must_use]
pub const fn sla_for(jurisdiction: DsrJurisdiction, submitted_at_ms: u64) -> u64 {
    match jurisdiction {
        // GDPR Art. 12(3) requires "one calendar MONTH", not 30 days.
        // A flat `30 * 86_400_000` silently overshoots the legal
        // deadline for any month shorter than 30 days — e.g. a Jan-31
        // submission is legally due Feb-28 (Feb-29 in a leap year), but
        // the day-based math lands on Mar-2 = up to 2 days LATE during
        // the real breach window. Use a true calendar-month add.
        DsrJurisdiction::Gdpr => add_one_calendar_month_ms(submitted_at_ms),
        // LGPD (15d) and CCPA (45d) are genuinely DAY-based per statute.
        DsrJurisdiction::Lgpd => {
            submitted_at_ms.saturating_add((SLA_LGPD_DAYS as u64) * MS_PER_DAY)
        }
        DsrJurisdiction::Ccpa => {
            submitted_at_ms.saturating_add((SLA_CCPA_DAYS as u64) * MS_PER_DAY)
        }
    }
}

/// Milliseconds per 24h day (UTC; no leap-second modelling — DSR SLA
/// granularity is days, not seconds).
const MS_PER_DAY: u64 = 86_400_000;

/// Add **one calendar month** to a Unix-epoch-ms instant, clamping the
/// day-of-month to the target month's last valid day (so Jan-31 →
/// Feb-28, or Feb-29 in a leap year — never spilling into March), and
/// preserving the time-of-day component. Pure const integer civil-
/// calendar arithmetic (Howard Hinnant's algorithm); no external date
/// crate (corelink-dsr must stay wasm32-clean with a minimal dep graph).
#[must_use]
const fn add_one_calendar_month_ms(epoch_ms: u64) -> u64 {
    let days = (epoch_ms / MS_PER_DAY) as i64;
    let time_of_day_ms = epoch_ms % MS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    // Advance one calendar month (Dec → next Jan).
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    // Clamp the day to the target month's last valid day.
    let last = last_day_of_month(ny, nm);
    let nd = if day > last { last } else { day };
    let out_days = days_from_civil(ny, nm, nd);
    (out_days as u64)
        .saturating_mul(MS_PER_DAY)
        .saturating_add(time_of_day_ms)
}

/// Whether `y` is a Gregorian leap year.
#[must_use]
const fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Last (highest) valid day-of-month for `(y, m)` (`m` in 1..=12).
#[must_use]
const fn last_day_of_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        // February non-leap (28) intentionally shares the fail-safe default: a
        // separate `2 => 28` arm is an EQUIVALENT mutant (cargo-mutants
        // "delete arm" can't be killed when the arm and the fallback return the
        // same value). `m` is always 1..=12 from callers; 28 is the fail-safe too.
        _ => 28,
    }
}

/// Civil date `(year, month, day)` from a count of days since the Unix
/// epoch (1970-01-01). Howard Hinnant's `civil_from_days`.
#[must_use]
const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d)
}

/// Count of days since the Unix epoch for a civil date `(y, m, d)`.
/// Howard Hinnant's `days_from_civil` (inverse of [`civil_from_days`]).
#[must_use]
const fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
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
    fn sla_for_lgpd_15d() {
        let submitted = 1_000_000_000_000_u64;
        let deadline = sla_for(DsrJurisdiction::Lgpd, submitted);
        // 15 days × 86_400_000 ms/day = 1_296_000_000 ms.
        assert_eq!(deadline, submitted + 1_296_000_000);
    }

    #[test]
    fn sla_for_gdpr_is_calendar_month_not_30_days() {
        // GDPR Art. 12(3) = "one calendar MONTH". A Jan-31 submission is
        // legally due Feb-28 (non-leap), NOT Mar-2 (= +30 days). The old
        // `30 * 86_400_000` math overshot the legal deadline by 2 days.
        //
        // Epoch-day anchors (days since 1970-01-01, × 86_400_000 ms):
        //   2023-01-31 = day 19_388 ; 2023-02-28 = day 19_416 (Δ 28d)
        //   30 days later would be   day 19_418 = 2023-03-02 (2d LATE)
        let jan31_2023 = 19_388_u64 * 86_400_000;
        let feb28_2023 = 19_416_u64 * 86_400_000;
        let deadline = sla_for(DsrJurisdiction::Gdpr, jan31_2023);
        assert_eq!(
            deadline, feb28_2023,
            "Jan-31 GDPR must land Feb-28, not Mar-2"
        );
        assert!(deadline < jan31_2023 + 30 * 86_400_000);

        let jan31_2024 = 19_753_u64 * 86_400_000;
        let feb29_2024 = 19_782_u64 * 86_400_000;
        assert_eq!(sla_for(DsrJurisdiction::Gdpr, jan31_2024), feb29_2024);

        let with_tod = jan31_2023 + 13 * 3_600_000 + 17 * 60_000;
        assert_eq!(
            sla_for(DsrJurisdiction::Gdpr, with_tod),
            feb28_2023 + 13 * 3_600_000 + 17 * 60_000
        );

        let mar15_2023 = 19_431_u64 * 86_400_000;
        let apr15_2023 = 19_462_u64 * 86_400_000;
        assert_eq!(sla_for(DsrJurisdiction::Gdpr, mar15_2023), apr15_2023);
    }

    #[test]
    fn sla_for_ccpa_45d() {
        let submitted = 1_000_000_000_000_u64;
        let deadline = sla_for(DsrJurisdiction::Ccpa, submitted);
        assert_eq!(deadline, submitted + 45 * 86_400_000);
    }

    #[test]
    fn last_day_of_month_exact_per_month() {
        for m in [1u32, 3, 5, 7, 8, 10, 12] {
            assert_eq!(last_day_of_month(2023, m), 31, "month {m}");
        }
        for m in [4u32, 6, 9, 11] {
            assert_eq!(last_day_of_month(2023, m), 30, "month {m}");
        }
        assert_eq!(last_day_of_month(2023, 2), 28);
        assert_eq!(last_day_of_month(2024, 2), 29);
        assert_eq!(last_day_of_month(2100, 2), 28);
        assert_eq!(last_day_of_month(2000, 2), 29);
    }

    #[test]
    fn civil_days_known_anchors_and_roundtrip() {
        let anchors: &[(i64, (i64, u32, u32))] = &[
            (0, (1970, 1, 1)),
            (10_957, (2000, 1, 1)),
            (19_416, (2023, 2, 28)),
            (19_782, (2024, 2, 29)),
        ];
        for &(days, ymd) in anchors {
            assert_eq!(civil_from_days(days), ymd, "civil_from_days({days})");
            let (y, m, d) = ymd;
            assert_eq!(days_from_civil(y, m, d), days, "days_from_civil{ymd:?}");
        }
        for &(lo, hi) in &[(-20_000_i64, 20_000_i64), (-740_000_i64, -700_000_i64)] {
            let mut z = lo;
            while z <= hi {
                let (y, m, d) = civil_from_days(z);
                assert_eq!(days_from_civil(y, m, d), z, "roundtrip day {z}");
                z += 1;
            }
        }
    }

    #[test]
    fn sla_for_saturating_at_overflow_boundary() {
        let submitted = u64::MAX - 100;
        let deadline = sla_for(DsrJurisdiction::Lgpd, submitted);
        assert_eq!(deadline, u64::MAX);
    }
}
