//! Canary region canonical taxonomy (3-region ship gate criterion).
//!
//! ## Why these 3 regions
//!
//! Per WI-S09-007 §1 + sprint contract §6 DoD ship gate criterion:
//! 3 R2 region hints `enam` (Eastern North America) + `weur` (Western
//! Europe) + `apac` (Asia-Pacific) per `data_model.md §2.1` canonical
//! R2 region hints (Lote 10.9bis P1 R4 P1-10 corrected from IATA
//! colocodes IAD/LHR/BOM — those are CF Workers `request.cf.colo`
//! identifiers, NOT R2 region hints; mixing the two surface causes
//! cross-region replication confusion). Each canary region maps to a
//! parent [`corelink_analytics::Region`] for cross-crate label
//! cartesian consistency without label drift.
//!
//! ## Why 3 regions specific
//!
//! Each region tests independent CF region + R2 storage + DO instance
//! + D1 database; cross-region failure modes detected (e.g., DO
//! routing primary_region misconfigured, R2 cross-region replication
//! lag). Single-region canary insufficient for multi-region production
//! confidence. Per sprint contract §10 anti-scope: synthetic
//! monitoring beyond 3 regions is anti-scope.

use corelink_analytics::Region;

/// Canonical 3-region canary deployment taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// sprints (e.g. S-14 enterprise tier additional region).
///
/// ## Region mapping
///
/// - `Enam` → `Region::Iad` (Eastern North America; primary US East
///   Ashburn).
/// - `Weur` → `Region::Lhr` (Western Europe; primary EU London).
/// - `Apac` → `Region::Bom` (Asia-Pacific; primary APAC Mumbai).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CanaryRegion {
    /// `enam` — Eastern North America R2 region hint per
    /// `data_model.md §2.1`. Primary CF region: `iad` (US East
    /// Ashburn).
    Enam,
    /// `weur` — Western Europe R2 region hint per `data_model.md
    /// §2.1`. Primary CF region: `lhr` (EU London).
    Weur,
    /// `apac` — Asia-Pacific R2 region hint per `data_model.md §2.1`.
    /// Primary CF region: `bom` (APAC Mumbai).
    Apac,
}

impl CanaryRegion {
    /// Canonical R2 region hint label string per `data_model.md
    /// §2.1`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enam => "enam",
            Self::Weur => "weur",
            Self::Apac => "apac",
        }
    }

    /// Map the canary region to its primary canonical
    /// [`corelink_analytics::Region`] CF colocode for cross-crate
    /// label cartesian consistency.
    #[must_use]
    pub const fn primary_cf_region(self) -> Region {
        match self {
            Self::Enam => Region::Iad,
            Self::Weur => Region::Lhr,
            Self::Apac => Region::Bom,
        }
    }
}

impl core::fmt::Display for CanaryRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element canary region list — pinned at the type
/// system layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_canary_regions() -> &'static [CanaryRegion; 3] {
    &[CanaryRegion::Enam, CanaryRegion::Weur, CanaryRegion::Apac]
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
    fn r2_region_hint_strings_pinned() {
        assert_eq!(CanaryRegion::Enam.as_str(), "enam");
        assert_eq!(CanaryRegion::Weur.as_str(), "weur");
        assert_eq!(CanaryRegion::Apac.as_str(), "apac");
    }

    #[test]
    fn primary_cf_region_mapping_pinned() {
        assert_eq!(CanaryRegion::Enam.primary_cf_region(), Region::Iad);
        assert_eq!(CanaryRegion::Weur.primary_cf_region(), Region::Lhr);
        assert_eq!(CanaryRegion::Apac.primary_cf_region(), Region::Bom);
    }

    #[test]
    fn canonical_list_has_exactly_three_unique_regions() {
        let v = canonical_canary_regions();
        assert_eq!(v.len(), 3);
        let mut set = std::collections::HashSet::new();
        for r in v {
            assert!(set.insert(r.as_str()), "duplicate canary region slug: {r}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn primary_cf_regions_unique_across_canary_regions() {
        let v = canonical_canary_regions();
        let mut set = std::collections::HashSet::new();
        for r in v {
            assert!(set.insert(r.primary_cf_region().as_str()));
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", CanaryRegion::Enam), "enam");
        assert_eq!(format!("{}", CanaryRegion::Weur), "weur");
        assert_eq!(format!("{}", CanaryRegion::Apac), "apac");
    }

    #[test]
    fn ordering_canonical_enam_weur_apac() {
        assert!(CanaryRegion::Enam < CanaryRegion::Weur);
        assert!(CanaryRegion::Weur < CanaryRegion::Apac);
    }
}
