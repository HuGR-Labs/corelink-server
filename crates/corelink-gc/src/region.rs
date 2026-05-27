//! Canonical GC region list — pins the schema CHECK constraint values.
//!
//! WI-S06-001 §1 + S-06 spec contract §5.1 enumerate 5 regions: `sam`,
//! `iad`, `lhr`, `nrt`, `syd` — the same canonical list as
//! `corelink_ac::schema::AcRegion` +
//! `corelink_cas::multipart_schema::MultipartRegion`. The schema CHECK
//! constraint `region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')` rejects
//! any other value at INSERT time on the `gc_run` table; this module is
//! the canonical Rust mirror.
//!
//! Each region is a sticky GC Durable Object pinning per WI §6.1.1 +
//! §9.1 (5 instances; one DO per region; no cross-region GC because GC
//! is region-scoped). Adding a new region (e.g. `gru`) is governed by
//! ADR-0036 Rule 3: it requires a new ADR + a new D1 migration that
//! updates the CHECK list (SQLite 12-step recipe per ADR-0036; never
//! raw `ALTER TABLE`).

use thiserror::Error;

/// One of the canonical 5 GC regions WI-S06-001 freezes at S-06 GA.
///
/// The variant order matches the SQL `CHECK` constraint list
/// left-to-right, and matches `corelink_ac::schema::AcRegion` +
/// `corelink_multipart_schema::MultipartRegion` — the same 5 regions
/// host AC envelopes + multipart chunks + manifests + GC checkpoint
/// rows. `Display` / [`GcRegion::as_str`] yield the lower-case bucket
/// suffix used in canonical metric labels (`region="sam"`) and in the
/// schema column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GcRegion {
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
///
/// Order is canonical — left-to-right matches the SQL CHECK literal
/// list AND the `GcRegion` variant order so the `migration_canonical`
/// test pins both at once.
pub const REGION_LIST: &[&str] = &["sam", "iad", "lhr", "nrt", "syd"];

/// Error surfaced when an unknown region literal is parsed.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("unknown gc region literal: {0:?}")]
pub struct UnknownRegion(pub String);

impl GcRegion {
    /// Lower-case ASCII suffix used in metric labels + the SQL
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

    /// Canonical Durable Object binding name pinned to the per-region
    /// GC worker (e.g. `GC_WORKER_SAM`). WI §6.1.7 — one DO per
    /// region; production wrangler.toml binds these names against the
    /// `corelink-gc-worker` worker class.
    #[must_use]
    pub const fn worker_binding_name(self) -> &'static str {
        match self {
            Self::Sam => "GC_WORKER_SAM",
            Self::Iad => "GC_WORKER_IAD",
            Self::Lhr => "GC_WORKER_LHR",
            Self::Nrt => "GC_WORKER_NRT",
            Self::Syd => "GC_WORKER_SYD",
        }
    }

    /// Every variant in canonical order.
    #[must_use]
    pub const fn all() -> &'static [GcRegion; 5] {
        &[Self::Sam, Self::Iad, Self::Lhr, Self::Nrt, Self::Syd]
    }

    /// Stable per-region ordinal (`0..5`). Used by the deterministic
    /// jitter helper [`crate::schedule::jitter_ms_for_region`] to
    /// spread cron tick wall-clock fires across the 5 regions evenly
    /// without coupling to any external RNG.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        match self {
            Self::Sam => 0,
            Self::Iad => 1,
            Self::Lhr => 2,
            Self::Nrt => 3,
            Self::Syd => 4,
        }
    }

    /// Parse a canonical region literal.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRegion`] if the literal is not one of the
    /// canonical 5.
    pub fn parse(literal: &str) -> Result<Self, UnknownRegion> {
        match literal {
            "sam" => Ok(Self::Sam),
            "iad" => Ok(Self::Iad),
            "lhr" => Ok(Self::Lhr),
            "nrt" => Ok(Self::Nrt),
            "syd" => Ok(Self::Syd),
            other => Err(UnknownRegion(other.to_owned())),
        }
    }
}

impl core::fmt::Display for GcRegion {
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
    fn region_list_matches_variant_iteration() {
        let from_iter: Vec<&str> = GcRegion::all().iter().map(|r| r.as_str()).collect();
        let from_const: Vec<&str> = REGION_LIST.to_vec();
        assert_eq!(from_iter, from_const);
        assert_eq!(from_iter.len(), 5);
    }

    #[test]
    fn parse_round_trip_for_every_region() {
        for region in GcRegion::all() {
            let parsed = GcRegion::parse(region.as_str()).expect("canonical parse");
            assert_eq!(parsed, *region);
        }
    }

    #[test]
    fn parse_rejects_unknown_literal() {
        assert!(GcRegion::parse("gru").is_err());
        assert!(GcRegion::parse("SAM").is_err()); // case-sensitive
        assert!(GcRegion::parse("").is_err());
    }

    #[test]
    fn ordinal_is_unique_per_variant() {
        let mut set = std::collections::HashSet::new();
        for r in GcRegion::all() {
            assert!(set.insert(r.ordinal()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn worker_binding_names_are_canonical() {
        for r in GcRegion::all() {
            let name = r.worker_binding_name();
            assert!(name.starts_with("GC_WORKER_"));
            assert!(name.ends_with(&r.as_str().to_uppercase()));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for r in GcRegion::all() {
            assert_eq!(format!("{r}"), r.as_str());
        }
    }
}
