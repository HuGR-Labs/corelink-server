//! Canonical multipart region list — pins the schema CHECK constraint values.
//!
//! WI-S05-004 §1 + S-05 spec contract §5.1 enumerate 5 regions: `sam`,
//! `iad`, `lhr`, `nrt`, `syd` — same canonical list as WI-S04-002
//! (Action Cache). The schema CHECK constraints
//! `region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')` reject any other
//! value at INSERT time on each of `chunks` / `multipart_sessions`;
//! this module is the canonical Rust mirror.
//!
//! Each region has TWO canonical R2 bucket families:
//!
//! - `corelink-chunk-<region>` — chunked CAS body parts (`chunks` table
//!   `r2_object_key` column references this bucket).
//! - `corelink-manifest-<region>` — manifest envelope (Merkle root +
//!   ordered chunk list).
//!
//! Adding a new region (e.g. `gru`) is governed by ADR-0036 Rule 3: it
//! requires a new ADR + a new D1 migration that updates the CHECK list
//! (SQLite 12-step recipe per ADR-0036; never raw `ALTER TABLE`).

use thiserror::Error;

/// One of the canonical 5 multipart regions WI-S05-004 freezes at S-05 GA.
///
/// The variant order matches the SQL `CHECK` constraint list left-to-right,
/// and matches `corelink_ac::schema::AcRegion` (intentional — the same 5
/// regions host AC envelopes + multipart chunks + manifests). `Display` /
/// [`MultipartRegion::as_str`] yield the lower-case bucket suffix used in
/// canonical R2 keys (`corelink-{chunk,manifest}-<suffix>/...`) and in the
/// schema column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MultipartRegion {
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
#[error("unknown multipart region literal: {0:?}")]
pub struct UnknownRegion(pub String);

impl MultipartRegion {
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

    /// Canonical R2 bucket name for the per-region chunk bucket
    /// (`corelink-chunk-<suffix>`).
    #[must_use]
    pub const fn chunk_bucket_name(self) -> &'static str {
        match self {
            Self::Sam => "corelink-chunk-sam",
            Self::Iad => "corelink-chunk-iad",
            Self::Lhr => "corelink-chunk-lhr",
            Self::Nrt => "corelink-chunk-nrt",
            Self::Syd => "corelink-chunk-syd",
        }
    }

    /// Canonical R2 bucket name for the per-region manifest bucket
    /// (`corelink-manifest-<suffix>`).
    #[must_use]
    pub const fn manifest_bucket_name(self) -> &'static str {
        match self {
            Self::Sam => "corelink-manifest-sam",
            Self::Iad => "corelink-manifest-iad",
            Self::Lhr => "corelink-manifest-lhr",
            Self::Nrt => "corelink-manifest-nrt",
            Self::Syd => "corelink-manifest-syd",
        }
    }

    /// Wrangler binding name for the per-region chunk R2 bucket
    /// (`CHUNK_BUCKET_<UPPER>`).
    #[must_use]
    pub const fn chunk_wrangler_binding(self) -> &'static str {
        match self {
            Self::Sam => "CHUNK_BUCKET_SAM",
            Self::Iad => "CHUNK_BUCKET_IAD",
            Self::Lhr => "CHUNK_BUCKET_LHR",
            Self::Nrt => "CHUNK_BUCKET_NRT",
            Self::Syd => "CHUNK_BUCKET_SYD",
        }
    }

    /// Wrangler binding name for the per-region manifest R2 bucket
    /// (`MANIFEST_BUCKET_<UPPER>`).
    #[must_use]
    pub const fn manifest_wrangler_binding(self) -> &'static str {
        match self {
            Self::Sam => "MANIFEST_BUCKET_SAM",
            Self::Iad => "MANIFEST_BUCKET_IAD",
            Self::Lhr => "MANIFEST_BUCKET_LHR",
            Self::Nrt => "MANIFEST_BUCKET_NRT",
            Self::Syd => "MANIFEST_BUCKET_SYD",
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

impl core::fmt::Display for MultipartRegion {
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
        let from_iter: Vec<&str> = MultipartRegion::all().iter().map(|r| r.as_str()).collect();
        assert_eq!(from_iter, REGION_LIST);
    }

    #[test]
    fn region_list_is_canonical_5() {
        assert_eq!(REGION_LIST.len(), 5);
        assert_eq!(REGION_LIST, &["sam", "iad", "lhr", "nrt", "syd"]);
    }

    #[test]
    fn parse_round_trip() {
        for region in MultipartRegion::all() {
            let parsed = MultipartRegion::parse(region.as_str()).unwrap();
            assert_eq!(parsed, *region);
        }
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(
            MultipartRegion::parse("gru").unwrap_err(),
            UnknownRegion("gru".to_string())
        );
        assert!(MultipartRegion::parse("").is_err());
        assert!(MultipartRegion::parse("SAM").is_err()); // case-sensitive
    }

    #[test]
    fn chunk_bucket_naming_canonical() {
        assert_eq!(
            MultipartRegion::Sam.chunk_bucket_name(),
            "corelink-chunk-sam"
        );
        assert_eq!(
            MultipartRegion::Iad.chunk_bucket_name(),
            "corelink-chunk-iad"
        );
        assert_eq!(
            MultipartRegion::Lhr.chunk_bucket_name(),
            "corelink-chunk-lhr"
        );
        assert_eq!(
            MultipartRegion::Nrt.chunk_bucket_name(),
            "corelink-chunk-nrt"
        );
        assert_eq!(
            MultipartRegion::Syd.chunk_bucket_name(),
            "corelink-chunk-syd"
        );
    }

    #[test]
    fn manifest_bucket_naming_canonical() {
        assert_eq!(
            MultipartRegion::Sam.manifest_bucket_name(),
            "corelink-manifest-sam"
        );
        assert_eq!(
            MultipartRegion::Iad.manifest_bucket_name(),
            "corelink-manifest-iad"
        );
        assert_eq!(
            MultipartRegion::Lhr.manifest_bucket_name(),
            "corelink-manifest-lhr"
        );
        assert_eq!(
            MultipartRegion::Nrt.manifest_bucket_name(),
            "corelink-manifest-nrt"
        );
        assert_eq!(
            MultipartRegion::Syd.manifest_bucket_name(),
            "corelink-manifest-syd"
        );
    }

    #[test]
    fn wrangler_bindings_canonical() {
        for region in MultipartRegion::all() {
            let chunk = region.chunk_wrangler_binding();
            let manifest = region.manifest_wrangler_binding();
            assert!(chunk.starts_with("CHUNK_BUCKET_"));
            assert!(manifest.starts_with("MANIFEST_BUCKET_"));
            assert_eq!(
                &chunk["CHUNK_BUCKET_".len()..],
                region.as_str().to_ascii_uppercase()
            );
            assert_eq!(
                &manifest["MANIFEST_BUCKET_".len()..],
                region.as_str().to_ascii_uppercase()
            );
        }
    }
}
