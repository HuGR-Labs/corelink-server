//! 3-region follow-the-sun rotation taxonomy.

/// Canonical 3-region follow-the-sun rotation taxonomy per WI-S20-006
/// §2.1. The `#[non_exhaustive]` marker reserves additive growth for
/// post-GA Q1 APAC sub-splits (e.g. ANZ vs JP-KR) per ADR-0034.
///
/// Shift windows are UTC-anchored 8-hour spans:
///
/// | Region   | Start UTC | End UTC | Local anchor                  |
/// |----------|-----------|---------|-------------------------------|
/// | Americas |     16:00 |   00:00 | UTC-8 .. UTC-5 (PT/CT/ET)     |
/// | EMEA     |     00:00 |   08:00 | UTC+0 .. UTC+3 (GMT/CET/EET)  |
/// | APAC     |     08:00 |   16:00 | UTC+8 .. UTC+11 (HKT/JST/AEST)|
///
/// Handoffs occur at region boundaries (00:00 / 08:00 / 16:00 UTC).
/// 24/7 coverage is guaranteed via the closed-loop 3 × 8h cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Region {
    /// Americas shift — 16:00 UTC → 00:00 UTC (UTC-8 .. UTC-5 local).
    Americas,
    /// EMEA shift — 00:00 UTC → 08:00 UTC (UTC+0 .. UTC+3 local).
    Emea,
    /// APAC shift — 08:00 UTC → 16:00 UTC (UTC+8 .. UTC+11 local).
    Apac,
}

impl Region {
    /// Canonical wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Americas => "americas",
            Self::Emea => "emea",
            Self::Apac => "apac",
        }
    }

    /// Shift start hour in UTC (0..=23).
    #[must_use]
    pub const fn shift_start_utc_hour(self) -> u8 {
        match self {
            Self::Americas => 16,
            Self::Emea => 0,
            Self::Apac => 8,
        }
    }

    /// Shift end hour in UTC (the start hour of the next region; wraps
    /// across midnight for Americas which ends at 00:00 UTC = next-day).
    #[must_use]
    pub const fn shift_end_utc_hour(self) -> u8 {
        match self {
            Self::Americas => 0,
            Self::Emea => 8,
            Self::Apac => 16,
        }
    }

    /// Canonical 8h shift width in seconds. The 3-region cycle sums to
    /// 24h exactly.
    #[must_use]
    pub const fn shift_seconds(self) -> u64 {
        8 * 60 * 60
    }

    /// Region active for a given UTC `hour` (0..=23). Saturating
    /// arithmetic: out-of-range hours fall back to Americas (the cron
    /// is canonically scheduled inside the Americas shift mid-window).
    #[must_use]
    pub const fn for_utc_hour(hour: u8) -> Self {
        if hour < 8 {
            Self::Emea
        } else if hour < 16 {
            Self::Apac
        } else {
            Self::Americas
        }
    }
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element list of [`Region`] for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_regions() -> &'static [Region; 3] {
    &[Region::Americas, Region::Emea, Region::Apac]
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
    fn region_canonical_slugs() {
        assert_eq!(Region::Americas.as_str(), "americas");
        assert_eq!(Region::Emea.as_str(), "emea");
        assert_eq!(Region::Apac.as_str(), "apac");
    }

    #[test]
    fn shift_boundaries_sum_to_24h() {
        // Sum of 3 × 8h shifts = 24h
        let total = Region::Americas.shift_seconds()
            + Region::Emea.shift_seconds()
            + Region::Apac.shift_seconds();
        assert_eq!(total, 24 * 60 * 60);
    }

    #[test]
    fn for_utc_hour_canonical() {
        assert_eq!(Region::for_utc_hour(0), Region::Emea);
        assert_eq!(Region::for_utc_hour(7), Region::Emea);
        assert_eq!(Region::for_utc_hour(8), Region::Apac);
        assert_eq!(Region::for_utc_hour(15), Region::Apac);
        assert_eq!(Region::for_utc_hour(16), Region::Americas);
        assert_eq!(Region::for_utc_hour(23), Region::Americas);
    }

    #[test]
    fn canonical_regions_surface_stable() {
        let regions = canonical_regions();
        assert_eq!(regions.len(), 3);
        assert_eq!(regions[0], Region::Americas);
    }
}
