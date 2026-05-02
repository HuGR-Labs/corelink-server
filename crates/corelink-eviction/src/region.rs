//! Canonical 5-region literal list mirrored across the codebase.
//!
//! Mirrors `corelink-gc::region::GcRegion` byte-for-byte —
//! tenant.primary_region drives DO routing per WI-S07-002 §6.1.6 +
//! Lote 10.7bis P0-9. The CHECK constraint in
//! `migrations/d1/0008_tenant_storage_state.sql` pins the same literal
//! list so the SQL row + the in-memory enum cannot drift.

use thiserror::Error;

/// Canonical 5-region literal list (matches `gc_run.region` +
/// `tenant_storage_state.region` SQL CHECK domain).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum EvictionRegion {
    /// `sam` — São Paulo region.
    Sam,
    /// `iad` — US East (Ashburn).
    Iad,
    /// `lhr` — UK (London).
    Lhr,
    /// `nrt` — Japan (Tokyo).
    Nrt,
    /// `syd` — Australia (Sydney).
    Syd,
}

/// Pinned canonical 5-region list — for cross-component regression
/// tests + dashboard widget configuration.
pub const REGION_LIST: [EvictionRegion; 5] = [
    EvictionRegion::Sam,
    EvictionRegion::Iad,
    EvictionRegion::Lhr,
    EvictionRegion::Nrt,
    EvictionRegion::Syd,
];

impl EvictionRegion {
    /// Canonical lower-snake-case mnemonic (matches the SQL CHECK
    /// literal byte-for-byte).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sam => "sam",
            Self::Iad => "iad",
            Self::Lhr => "lhr",
            Self::Nrt => "nrt",
            Self::Syd => "syd",
        }
    }

    /// Parse a canonical mnemonic.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRegion`] if the input is not one of the
    /// canonical 5 literals.
    pub fn parse(s: &str) -> Result<Self, UnknownRegion> {
        match s {
            "sam" => Ok(Self::Sam),
            "iad" => Ok(Self::Iad),
            "lhr" => Ok(Self::Lhr),
            "nrt" => Ok(Self::Nrt),
            "syd" => Ok(Self::Syd),
            other => Err(UnknownRegion(other.to_string())),
        }
    }
}

impl core::fmt::Display for EvictionRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Surfaced when [`EvictionRegion::parse`] sees a non-canonical
/// region literal.
#[derive(Debug, Error)]
#[error("unknown eviction region: {0}")]
pub struct UnknownRegion(pub String);

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
    fn region_list_canonical() {
        assert_eq!(REGION_LIST.len(), 5);
        assert_eq!(REGION_LIST[0].as_str(), "sam");
        assert_eq!(REGION_LIST[1].as_str(), "iad");
        assert_eq!(REGION_LIST[2].as_str(), "lhr");
        assert_eq!(REGION_LIST[3].as_str(), "nrt");
        assert_eq!(REGION_LIST[4].as_str(), "syd");
    }

    #[test]
    fn parse_round_trips_every_region() {
        for r in REGION_LIST {
            assert_eq!(EvictionRegion::parse(r.as_str()).unwrap(), r);
        }
    }

    #[test]
    fn parse_rejects_unknown() {
        let err = EvictionRegion::parse("ord").unwrap_err();
        assert!(format!("{err}").contains("ord"));
    }

    #[test]
    fn display_matches_as_str() {
        for r in REGION_LIST {
            assert_eq!(format!("{r}"), r.as_str());
        }
    }
}
