//! Canonical AC region list — pins the schema CHECK constraint values.
//!
//! WI-S04-002 §1 enumerates 5 regions: `sam`, `iad`, `lhr`, `nrt`,
//! `syd`. The schema CHECK constraint
//! `region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')` rejects any other
//! value at INSERT time; this module is the canonical Rust mirror.
//!
//! Adding a new region (e.g. `gru`) is governed by ADR-0036 Rule 3:
//! it requires a new ADR + a new D1 migration that updates the CHECK
//! list (SQLite 12-step recipe per ADR-0036; never raw `ALTER TABLE`).

use thiserror::Error;

/// One of the canonical 5 AC regions WI-S04-002 freezes at S-04 GA.
///
/// The variant order matches the SQL `CHECK` constraint list left-to-right.
/// `Display` / [`AcRegion::as_str`] yield the lower-case bucket suffix
/// used in canonical R2 keys (`corelink-ac-<suffix>/...`) and in the
/// schema column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AcRegion {
    /// South America (Cloudflare region hint `sam`).
    Sam,
    /// US East — Ashburn / Virginia (Cloudflare region hint `iad`).
    Iad,
    /// Europe — London (Cloudflare region hint `lhr`).
    Lhr,
    /// Asia — Tokyo / Narita (Cloudflare region hint `nrt`).
    Nrt,
    /// Oceania — Sydney (Cloudflare region hint `syd`).
    Syd,
}

/// Canonical region literal as it appears in the schema CHECK list.
pub const REGION_LIST: &[&str] = &["sam", "iad", "lhr", "nrt", "syd"];

/// Error surfaced when an unknown region literal is parsed.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("unknown AC region literal: {0:?}")]
pub struct UnknownRegion(pub String);

impl AcRegion {
    /// Lower-case ASCII suffix used in canonical R2 keys + the SQL
    /// `region` column.
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

    /// Canonical R2 bucket name for this region (`corelink-ac-<suffix>`).
    #[must_use]
    pub const fn r2_bucket_name(self) -> &'static str {
        match self {
            Self::Sam => "corelink-ac-sam",
            Self::Iad => "corelink-ac-iad",
            Self::Lhr => "corelink-ac-lhr",
            Self::Nrt => "corelink-ac-nrt",
            Self::Syd => "corelink-ac-syd",
        }
    }

    /// Wrangler binding name for the per-region R2 bucket
    /// (`AC_BUCKET_<UPPER>`).
    #[must_use]
    pub const fn wrangler_binding(self) -> &'static str {
        match self {
            Self::Sam => "AC_BUCKET_SAM",
            Self::Iad => "AC_BUCKET_IAD",
            Self::Lhr => "AC_BUCKET_LHR",
            Self::Nrt => "AC_BUCKET_NRT",
            Self::Syd => "AC_BUCKET_SYD",
        }
    }

    /// Iterate every canonical region in the order pinned by the
    /// schema CHECK list.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Sam, Self::Iad, Self::Lhr, Self::Nrt, Self::Syd]
    }

    /// Parse a region literal as it appears in the schema column.
    /// Rejects any value not in the canonical 5-region list.
    pub fn parse(value: &str) -> Result<Self, UnknownRegion> {
        match value {
            "sam" => Ok(Self::Sam),
            "iad" => Ok(Self::Iad),
            "lhr" => Ok(Self::Lhr),
            "nrt" => Ok(Self::Nrt),
            "syd" => Ok(Self::Syd),
            other => Err(UnknownRegion(other.to_owned())),
        }
    }
}

impl core::fmt::Display for AcRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
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
    fn region_list_matches_all_iter() {
        let from_iter: Vec<&str> = AcRegion::all().iter().map(|r| r.as_str()).collect();
        assert_eq!(from_iter, REGION_LIST);
    }

    #[test]
    fn region_list_is_canonical_5() {
        assert_eq!(REGION_LIST.len(), 5);
        assert_eq!(REGION_LIST, &["sam", "iad", "lhr", "nrt", "syd"]);
    }

    #[test]
    fn parse_round_trip() {
        for region in AcRegion::all() {
            let parsed = AcRegion::parse(region.as_str()).unwrap();
            assert_eq!(parsed, *region);
        }
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(
            AcRegion::parse("gru").unwrap_err(),
            UnknownRegion("gru".to_string())
        );
        assert!(AcRegion::parse("").is_err());
        assert!(AcRegion::parse("SAM").is_err()); // case-sensitive
    }

    #[test]
    fn r2_bucket_naming_canonical() {
        assert_eq!(AcRegion::Sam.r2_bucket_name(), "corelink-ac-sam");
        assert_eq!(AcRegion::Iad.r2_bucket_name(), "corelink-ac-iad");
        assert_eq!(AcRegion::Lhr.r2_bucket_name(), "corelink-ac-lhr");
        assert_eq!(AcRegion::Nrt.r2_bucket_name(), "corelink-ac-nrt");
        assert_eq!(AcRegion::Syd.r2_bucket_name(), "corelink-ac-syd");
    }

    #[test]
    fn wrangler_bindings_canonical() {
        for region in AcRegion::all() {
            let binding = region.wrangler_binding();
            assert!(binding.starts_with("AC_BUCKET_"));
            assert_eq!(
                &binding["AC_BUCKET_".len()..],
                region.as_str().to_ascii_uppercase()
            );
        }
    }
}
