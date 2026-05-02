//! Size-proportional reservation TTL formula (Lote 10.7bis R5 P0-2).
//!
//! ## Why size-proportional
//!
//! Naive 60s reservation TTL would expire mid-upload for large
//! multipart blobs (160 GiB at 50 MiB/s ≈ 1h). The reservation would
//! auto-release before the write commits → quota leak (the blob bytes
//! were already in flight to R2 but `bytes_used` no longer reflects
//! the pending reservation). Size-proportional TTL scales the
//! reservation window with `request_bytes` so any legitimate upload
//! has a wide-enough margin.
//!
//! ## Canonical formula
//!
//! ```text
//! ttl_ms = clamp(
//!     max(60_000, request_bytes / 1_MiB_per_s_in_bytes_per_ms × 2),
//!     60_000,                    // floor (small uploads)
//!     7 * 86_400_000             // cap (7d ceiling per Lote 10.7bis R5)
//! )
//! ```
//!
//! Where:
//! - `1 MiB/s = 1_048_576 bytes / 1000 ms ≈ 1048.576 bytes/ms` (we
//!   round to 1024 bytes/ms for integer math; see
//!   [`RESERVATION_THROUGHPUT_BYTES_PER_MS`]).
//! - The `× 2` multiplier provides 2× headroom for slow networks +
//!   retries (per WI-S07-003 §6.1.5).
//! - The cap of 7d covers even pathological upload windows; an upload
//!   stalled > 7d is operationally a write that crashed (and the
//!   reservation SHOULD auto-release).
//!
//! ## Test boundaries (pinned by `prop_ttl_size_proportional_reservation`)
//!
//! - `request_bytes = 1` → returns `MIN_RESERVATION_TTL_MS` (60s
//!   floor; the formula yields `~2 ms` which is dwarfed by the floor).
//! - `request_bytes = 1 MiB` → `1_MiB / 1024 bytes/ms × 2 = 2048 ms`;
//!   below floor → returns 60s.
//! - `request_bytes = 100 MiB` → `100 × 1024 ms × 2 = 204_800 ms` ≈
//!   3.4 min; above floor → returns 204_800.
//! - `request_bytes = 1 GiB` → `1024 × 1024 ms × 2 = 2_097_152 ms` ≈
//!   35 min.
//! - `request_bytes = 160 GiB` → 160 × 1024 × 1024 × 2 ms ≈ 5.8d;
//!   below cap → returns the formula value.
//! - `request_bytes = 1 TiB` → would yield ≈ 37d; clamped to 7d cap.

/// Reservation TTL floor — small uploads always get at least this
/// much window (mirrors WI-S07-003 §6.1.5 "floor 60s for tiny
/// requests").
pub const MIN_RESERVATION_TTL_MS: u64 = 60_000;

/// Reservation TTL ceiling — 7 days. An upload stalled this long is
/// operationally a crashed write; the reservation SHOULD auto-release.
pub const MAX_RESERVATION_TTL_MS: u64 = 7 * 86_400_000;

/// Headroom multiplier — `× 2` per WI-S07-003 §6.1.5 (slow-network +
/// retry coverage).
pub const RESERVATION_PROPORTIONAL_MULTIPLIER: u64 = 2;

/// Throughput baseline — bytes per ms equivalent to ≈ 1 MiB/s
/// (1_048_576 bytes / 1000 ms ≈ 1048 bytes/ms; we round DOWN to 1024
/// bytes/ms so a tighter denominator yields a slightly LARGER TTL,
/// giving the reservation a small extra safety margin).
pub const RESERVATION_THROUGHPUT_BYTES_PER_MS: u64 = 1024;

/// Compute the canonical size-proportional reservation TTL (ms).
///
/// Formula:
/// `clamp(request_bytes / 1024 × 2, MIN, MAX)`
/// where `MIN = 60_000` ms and `MAX = 7 × 86_400_000` ms.
///
/// Saturating arithmetic; never panics on overflow.
#[must_use]
pub const fn reservation_ttl_ms(request_bytes: u64) -> u64 {
    let proportional = (request_bytes / RESERVATION_THROUGHPUT_BYTES_PER_MS)
        .saturating_mul(RESERVATION_PROPORTIONAL_MULTIPLIER);
    if proportional < MIN_RESERVATION_TTL_MS {
        return MIN_RESERVATION_TTL_MS;
    }
    if proportional > MAX_RESERVATION_TTL_MS {
        return MAX_RESERVATION_TTL_MS;
    }
    proportional
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
    fn canonical_constants_pinned() {
        assert_eq!(MIN_RESERVATION_TTL_MS, 60_000);
        assert_eq!(MAX_RESERVATION_TTL_MS, 7 * 86_400_000);
        assert_eq!(RESERVATION_PROPORTIONAL_MULTIPLIER, 2);
        assert_eq!(RESERVATION_THROUGHPUT_BYTES_PER_MS, 1024);
    }

    #[test]
    fn one_byte_returns_floor() {
        assert_eq!(reservation_ttl_ms(1), MIN_RESERVATION_TTL_MS);
    }

    #[test]
    fn zero_bytes_returns_floor() {
        // Defensive: a zero-byte request should still get the floor
        // (no useful upload window otherwise).
        assert_eq!(reservation_ttl_ms(0), MIN_RESERVATION_TTL_MS);
    }

    #[test]
    fn one_mib_returns_floor() {
        // 1 MiB / 1024 = 1024 ms; × 2 = 2048 ms; below floor 60_000 ms.
        assert_eq!(reservation_ttl_ms(1024 * 1024), MIN_RESERVATION_TTL_MS);
    }

    #[test]
    fn one_hundred_mib_above_floor() {
        // 100 MiB / 1024 = 102_400 ms; × 2 = 204_800 ms ≈ 3.4 min.
        assert_eq!(reservation_ttl_ms(100 * 1024 * 1024), 204_800);
    }

    #[test]
    fn one_gib_about_35_min() {
        // 1 GiB / 1024 = 1_048_576 ms; × 2 = 2_097_152 ms ≈ 34.95 min.
        assert_eq!(reservation_ttl_ms(1024 * 1024 * 1024), 2_097_152);
    }

    #[test]
    fn one_hundred_sixty_gib_below_cap() {
        // 160 GiB / 1024 = 167_772_160 ms; × 2 = 335_544_320 ms ≈ 3.88d.
        // Below 7d cap.
        let v = reservation_ttl_ms(160 * 1024 * 1024 * 1024);
        assert_eq!(v, 335_544_320);
        assert!(v < MAX_RESERVATION_TTL_MS);
    }

    #[test]
    fn one_tib_clamped_to_cap() {
        // 1 TiB / 1024 = 1_073_741_824 ms; × 2 = 2_147_483_648 ms ≈
        // 24.85d. Above 7d cap → clamped.
        let v = reservation_ttl_ms(1024_u64 * 1024 * 1024 * 1024);
        assert_eq!(v, MAX_RESERVATION_TTL_MS);
    }

    #[test]
    fn pathological_overflow_clamped_to_cap() {
        // u64::MAX request_bytes — saturating math returns u64::MAX
        // pre-clamp; clamp to MAX_RESERVATION_TTL_MS.
        let v = reservation_ttl_ms(u64::MAX);
        assert_eq!(v, MAX_RESERVATION_TTL_MS);
    }

    #[test]
    fn ttl_monotone_in_request_bytes_in_proportional_band() {
        // Pick two values both above the floor and below the cap.
        let a = reservation_ttl_ms(50 * 1024 * 1024);
        let b = reservation_ttl_ms(200 * 1024 * 1024);
        assert!(b >= a, "monotone violated: 50MiB={a} vs 200MiB={b}");
    }
}
