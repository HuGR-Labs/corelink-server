//! [`MissReason`] — disambiguated 404-cache reason for the negative cache.
//!
//! The enum is **forward-future-ready** but **HTTP-uniform at GA** per
//! ADR-0028 (`MissReason → HTTP 404 uniform freeze`, S-02 GA decision; cf.
//! WI-S02-005 §9.10): every variant maps to a single wire 404 with
//! `error_code = COR_CAS_BLOB_NOT_FOUND` so an attacker cannot
//! distinguish `NeverExisted` vs. `Tombstoned` vs. `CrossTenantMasked`
//! from the response. The variant survives so that internal audit chains
//! (S-09) and admin-plane tooling can correlate forensics by reason.
//!
//! # Mapping to the read-orchestrator `corelink_reapi::read::MissReason`
//!
//! `corelink-reapi`'s `MissReason` enumerates `{NeverExisted, Tombstoned,
//! R2OrphanRow}` — the three *runtime* arms produced by the read
//! orchestrator before the 404 leaves the handler. The negative-cache
//! `MissReason` is the *cache-stored* taxonomy and intentionally **does
//! not** carry the `R2OrphanRow` arm:
//!
//! - `R2OrphanRow` describes a transient inconsistency between `blob_meta`
//!   (alive row) and R2 (missing object). It is repaired by the GC
//!   reconciler within ≤ 24 h. Caching this state would risk extending the
//!   orphan window past its repair SLO; we deliberately bypass the cache
//!   for orphan responses.
//! - `NotFound` (cache) folds the read-side `NeverExisted` arm: a
//!   `(tenant, digest)` row is absent from `blob_meta`. Crucially, the
//!   trait-level `MetaStore::get` cannot disambiguate "never existed
//!   anywhere" from "exists under another tenant's PK" — both surface
//!   `Ok(None)` to the orchestrator. The cache stores the conflated
//!   answer.
//! - `Tombstoned` (cache) tracks soft-deleted blobs (`deleted_at IS NOT
//!   NULL`). Internal audit retains the distinction; the wire does not.
//! - `CrossTenantMasked` is reserved for an offline reclassification path
//!   (S-09 chain consumer with global digest visibility) that **may**, in
//!   the future, populate the cache with a stricter reason than the
//!   handler can locally compute.
//!
//! # Wire encoding
//!
//! The cache-stored value is a single ASCII byte (the [`MissReason::tag`])
//! so KV reads/writes are minimal-cost. We deliberately avoid JSON: the
//! cache value carries no other fields, and a single-byte tag is forward-
//! compatible (unknown bytes round-trip as
//! [`crate::cache::kv::KvError::Corrupt`]).

use core::fmt;

use crate::cache::kv::KvError;

/// Disambiguated 404-cache reason.
///
/// Forward-future-ready taxonomy whose wire surface is **uniform 404 +
/// `COR_CAS_BLOB_NOT_FOUND`** per ADR-0028. Variant distinction is
/// forensic-only and never leaks to clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissReason {
    /// `(tenant, digest)` row is absent from `blob_meta`. Could be
    /// genuinely never-existed **or** cross-tenant masked (the trait
    /// cannot distinguish; see module rustdoc).
    NotFound,
    /// `(tenant, digest)` row exists but `deleted_at IS NOT NULL` — the
    /// blob was tombstoned by S-06 GC soft-delete and is invisible per
    /// WI-S02-001 §6.1.6.
    Tombstoned,
    /// Cross-tenant cache reclassification slot reserved for the offline
    /// audit-chain consumer (S-09). Not produced by the read handler at
    /// S-02 GA; round-trips through the cache so future producers can
    /// emit it without a schema bump.
    CrossTenantMasked,
}

impl MissReason {
    /// Stable single-byte ASCII tag persisted in KV. Must remain wire-
    /// stable across releases: a value byte that no longer parses signals
    /// either a corruption or a cross-version downgrade and is mapped to
    /// [`KvError::Corrupt`] by [`Self::from_tag`].
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::NotFound => b'n',
            Self::Tombstoned => b't',
            Self::CrossTenantMasked => b'x',
        }
    }

    /// Parse the [`Self::tag`] back into a variant.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Corrupt`] for any byte that does not name a
    /// known variant (forward-compat: unknown future tags downgrade to
    /// "unparseable" so the read handler treats it as a cache miss
    /// rather than mis-route on a bogus reason).
    pub fn from_tag(tag: u8) -> Result<Self, KvError> {
        match tag {
            b'n' => Ok(Self::NotFound),
            b't' => Ok(Self::Tombstoned),
            b'x' => Ok(Self::CrossTenantMasked),
            other => Err(KvError::Corrupt {
                detail: format!("unknown MissReason tag byte: 0x{other:02x}"),
            }),
        }
    }

    /// Human-readable label, used in metrics + audit.
    ///
    /// The surface is stable (MUST NOT change without a metrics
    /// dashboard migration).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Tombstoned => "tombstoned",
            Self::CrossTenantMasked => "cross_tenant_masked",
        }
    }
}

impl fmt::Display for MissReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: unwrap/panic on a failing assertion is itself a test failure"
)]
mod unit_tests {
    use super::*;

    #[test]
    fn tag_roundtrip_every_variant() {
        for r in [
            MissReason::NotFound,
            MissReason::Tombstoned,
            MissReason::CrossTenantMasked,
        ] {
            let tag = r.tag();
            let back = MissReason::from_tag(tag).expect("known tag must parse");
            assert_eq!(back, r);
        }
    }

    #[test]
    fn unknown_tag_is_corrupt() {
        let err = MissReason::from_tag(b'z').expect_err("z is unknown");
        match err {
            KvError::Corrupt { detail } => {
                assert!(
                    detail.contains("0x7a"),
                    "diagnostic must include hex byte: {detail}"
                );
            }
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn tags_are_pairwise_distinct() {
        let tags: [u8; 3] = [
            MissReason::NotFound.tag(),
            MissReason::Tombstoned.tag(),
            MissReason::CrossTenantMasked.tag(),
        ];
        let mut sorted: Vec<u8> = tags.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "tags must be pairwise distinct");
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", MissReason::NotFound), "not_found");
        assert_eq!(format!("{}", MissReason::Tombstoned), "tombstoned");
        assert_eq!(
            format!("{}", MissReason::CrossTenantMasked),
            "cross_tenant_masked"
        );
    }
}
