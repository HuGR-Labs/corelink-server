//! Dedup index trait + InMemory fake + canonical
//! `find_missing_blobs` set-difference orchestrator.
//!
//! ## Surface
//!
//! Given a [`Uuid`] tenant id and an arbitrary list of [`BlobDigest`]s
//! (chunk digests; BLAKE3-256 hex), return the SUBSET of digests that
//! the requesting tenant does NOT possess (alive + visible under
//! `(tenant_id, chunk_digest)` PK; soft-deleted rows are treated as
//! missing per WI §6.1 R-S07-1 "deleted_at IS NULL" predicate so the
//! S-06 grace window 72 h is respected — a soft-deleted chunk's
//! re-upload acts as an undelete via INSERT ON CONFLICT).
//!
//! Per ADR-0028 the result is "missing" both when the digest never
//! existed AND when it exists under a DIFFERENT tenant — the trait
//! surface is structurally unable to distinguish, which is the
//! load-bearing CTRL-ISO-005 ("no cross-tenant existence oracle")
//! guarantee.
//!
//! ## Order is load-bearing
//!
//! 1. **Per-batch tenant-scoped lookup** — the SQL `IN (?, ?, ...)` is
//!    keyed leftmost on `tenant_id` so a Tenant B query for a digest
//!    that exists ONLY under Tenant A surfaces in `missing` exactly
//!    as if it had never been written.
//! 2. **R2 NEVER consulted on the find-missing path** — D1 (the
//!    `chunks` table) is the authoritative metadata index.
//! 3. **Bounded batch size** — per [`MAX_FIND_MISSING_BATCH_SIZE`] (250)
//!    the handler MUST chunk inputs > 250 before reaching this trait
//!    (Lote 10.5bis P0 lesson; D1 `IN`-list cap). Larger inputs surface
//!    as [`crate::dedup::DedupError::BatchTooLarge`] from the trait layer (the
//!    handler's job to chunk).
//!
//! ## Cross-tenant masking (CTRL-ISO-005 + ADR-0028)
//!
//! Tenant B asks `find_missing_blobs([D])` where D is owned by Tenant A.
//! The lookup uses `(B.tenant_id, D)` as the PK — A's row is invisible.
//! The orchestrator emits D in `missing` exactly as if D had never been
//! written anywhere. From B's perspective, the response is structurally
//! indistinguishable from a never-existed digest.
//!
//! This is the canonical defense against the cross-tenant existence
//! oracle (CTRL-ISO-005); the property test
//! `prop_tenant_isolation` exercises 10k iter at the public API
//! surface to enforce this at CI time.
//!
//! ## InMemory fake — what it mirrors from S-05 `chunks` table
//!
//! Per WI-S07-001 §6.1 (Lote 10.7bis P0-1 fix) the dedup table is the
//! pre-existing S-05 `chunks` table:
//!
//! ```sql
//! -- ALREADY EXISTS in `chunks` table (S-05 WI-S05-004 §6.1):
//! --   tenant_id           TEXT        NOT NULL,
//! --   chunk_digest        TEXT        NOT NULL,
//! --   refcount            INTEGER     NOT NULL DEFAULT 1,
//! --   deleted_at          INTEGER     NULL,        -- soft-delete grace
//! --   PRIMARY KEY (tenant_id, chunk_digest),
//! --   CHECK (refcount >= 0)
//! ```
//!
//! NO new migration needed. The fake stores `(tenant_id, chunk_digest)
//! → ChunksRowSummary { refcount, deleted_at_ms }` and computes
//! set-difference via per-digest `BTreeMap::contains_key` + the
//! `deleted_at IS NULL` predicate.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::dedup::error::DedupError;

/// Canonical hex digest length (BLAKE3-256 hex). Aligned with
/// `corelink-multipart-schema::CHUNK_DIGEST_HEX_LEN`.
pub const CHUNK_DIGEST_HEX_LEN: usize = 64;

/// Canonical max batch size per `find_missing_blobs` invocation
/// (Lote 10.5bis P0 lesson; D1 `IN`-list cap of 250 rows). Larger
/// inputs MUST be chunked client-side OR by the gRPC handler before
/// reaching this trait.
pub const MAX_FIND_MISSING_BATCH_SIZE: usize = 250;

/// Canonical dedup index config. Cross-tenant dedup is OFF by default
/// per spec_contract §10.s07.4 (CTRL-ISO-005 enforcement); flipping to
/// `true` requires an ADR + BYOE (S-14) + Privacy Lead signoff.
///
/// `Default` derives `cross_tenant_enabled = false` — the safe-by-
/// default value. Flipping requires an explicit struct construction
/// at the call site, surfacing the override in code review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DedupConfig {
    /// Cross-tenant dedup gate. When `false` (default) any cross-tenant
    /// query path returns [`DedupError::CrossTenantBlocked`]. The gate
    /// is intentionally type-system enforced at the trait surface (the
    /// trait takes a `tenant_id` parameter; the orchestrator binds
    /// every lookup to that scope) — this flag exists to document the
    /// invariant and to reserve the future cross-tenant code path.
    pub cross_tenant_enabled: bool,
}

/// Canonical chunk digest (BLAKE3-256 hex). Stored as a typed wrapper
/// around `String` so the trait surface rejects non-hex / wrong-length
/// inputs at the type level.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobDigest(String);

impl BlobDigest {
    /// Construct a [`BlobDigest`] from a 64-char lower-hex string.
    ///
    /// # Errors
    ///
    /// Returns [`DedupError::InvalidChunkDigest`] when the input is not
    /// 64 chars OR contains non-hex characters.
    pub fn new(hex: impl Into<String>) -> Result<Self, DedupError> {
        let s: String = hex.into();
        if s.len() != CHUNK_DIGEST_HEX_LEN {
            return Err(DedupError::InvalidChunkDigest {
                reason: "length != 64",
            });
        }
        if !is_lower_hex(&s) {
            return Err(DedupError::InvalidChunkDigest {
                reason: "non-hex character",
            });
        }
        Ok(Self(s))
    }

    /// Borrow the canonical hex representation.
    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for BlobDigest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Materialised `chunks` row summary used by the in-memory fake. The
/// real D1 row carries more columns (size_bytes, region, r2_object_key,
/// etc.) but the dedup path only needs the dedup-relevant subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunksRowSummary {
    /// Reference count (0 = candidate for S-06 GC sweep). The dedup
    /// path treats `refcount = 0 AND deleted_at IS NULL` as PRESENT
    /// (the chunk body is still in R2; sweep has not soft-deleted yet).
    pub refcount: u32,
    /// Soft-delete tombstone — `Some(swept_at_ms)` once sweep
    /// soft-deletes the row. The dedup path treats a soft-deleted row
    /// as MISSING per WI §6.1 R-S07-1 ("deleted_at IS NULL" predicate)
    /// so the S-06 grace window 72 h forces the client to re-upload
    /// (re-INSERT increments refcount + clears `deleted_at`).
    pub deleted_at_ms: Option<u64>,
}

/// Per-batch outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMissingBlobsOutcome {
    /// Digests that the requesting tenant does NOT possess. Order is
    /// preserved relative to the input slice (REAPI clients correlate
    /// response order with request order — same contract as the
    /// BLOB-level `corelink-reapi::find_missing` from WI-S02-002).
    pub missing: Vec<BlobDigest>,
    /// Number of digests in the input batch (== `digests.len()`;
    /// surfaced for metric emission + criterion benchmark
    /// observability).
    pub queried_count: usize,
}

/// Pure-logic batch existence-check trait. Production wiring binds this
/// against the real D1 `chunks` table (S-05 schema); the in-memory
/// fake mirrors the load-bearing invariants.
pub trait DedupIndex: Send + Sync + core::fmt::Debug {
    /// Per-batch existence-check: return the subset of `digests` that
    /// the requesting tenant does NOT possess.
    ///
    /// **Tenant-scoped strict** (CTRL-ISO-005): the lookup uses
    /// `(tenant_id, chunk_digest)` as the PK key, leftmost on
    /// `tenant_id`. Cross-tenant rows are structurally invisible.
    ///
    /// **Soft-delete grace respect** (S-06 inheritance): rows whose
    /// `deleted_at IS NOT NULL` are treated as MISSING (the chunk body
    /// is in the grace window; client re-upload acts as undelete).
    ///
    /// **Bounded batch size**: input MUST be <=
    /// [`MAX_FIND_MISSING_BATCH_SIZE`] (250); larger inputs surface as
    /// [`DedupError::BatchTooLarge`] (handler's job to chunk).
    ///
    /// # Errors
    ///
    /// Returns [`DedupError::BatchTooLarge`] when `digests.len() >
    /// MAX_FIND_MISSING_BATCH_SIZE`. Returns [`DedupError::IndexBackend`]
    /// on any backend transport failure. The orchestrator is total-or-
    /// fail (no partial responses; ADR-0028 §"FindMissingBlobs is
    /// total-or-fail").
    fn find_missing_blobs(
        &self,
        tenant_id: Uuid,
        digests: &[BlobDigest],
    ) -> Result<FindMissingBlobsOutcome, DedupError>;
}

/// In-memory dedup index. Mirrors the canonical `(tenant_id,
/// chunk_digest)` PK + `deleted_at IS NULL` predicate semantic of the
/// S-05 `chunks` table.
///
/// F-001 closure: per-instance `Mutex` (NOT process-global
/// `static LazyLock<Mutex<>>`).
#[derive(Debug, Default)]
pub struct InMemoryDedupIndex {
    inner: Mutex<BTreeMap<(Uuid, String), ChunksRowSummary>>,
}

impl InMemoryDedupIndex {
    /// Construct a fresh empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or upsert a `chunks` row. Mirrors the canonical
    /// `INSERT ... ON CONFLICT (tenant_id, chunk_digest) DO UPDATE
    /// SET refcount = refcount + 1, deleted_at = NULL` semantic
    /// (re-upload of a soft-deleted chunk acts as undelete).
    ///
    /// Returns `true` if a NEW row was inserted, `false` if an
    /// existing row's refcount was incremented (dedup hit). The
    /// boolean drives the dedup-on-write counter increment in
    /// [`crate::dedup::write::record_chunk_write`].
    ///
    /// # Errors
    ///
    /// Returns [`DedupError::IndexBackend`] on mutex poisoning. The
    /// fake never poisons under normal use; this surfaces only if a
    /// caller panicked while holding the inner lock.
    pub fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<bool, DedupError> {
        let key = (tenant_id, digest.as_hex().to_string());
        let mut guard = self.inner.lock().map_err(|_| {
            DedupError::IndexBackend(
                "in-memory dedup index mutex poisoned".to_string(),
            )
        })?;
        match guard.get_mut(&key) {
            None => {
                guard.insert(
                    key,
                    ChunksRowSummary {
                        refcount: 1,
                        deleted_at_ms: None,
                    },
                );
                Ok(true)
            }
            Some(row) => {
                row.refcount = row.refcount.saturating_add(1);
                // Re-upload of a soft-deleted chunk acts as undelete
                // (mirrors the production INSERT ON CONFLICT semantic).
                row.deleted_at_ms = None;
                Ok(false)
            }
        }
    }

    /// Soft-delete a row (S-06 GC sweep). Mirrors `UPDATE chunks SET
    /// deleted_at = ? WHERE tenant_id = ? AND chunk_digest = ? AND
    /// deleted_at IS NULL`.
    ///
    /// Returns `true` if the row was soft-deleted; `false` if the row
    /// does not exist OR was already soft-deleted.
    ///
    /// # Errors
    ///
    /// Returns [`DedupError::IndexBackend`] on mutex poisoning.
    pub fn soft_delete(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
    ) -> Result<bool, DedupError> {
        let key = (tenant_id, digest.as_hex().to_string());
        let mut guard = self.inner.lock().map_err(|_| {
            DedupError::IndexBackend(
                "in-memory dedup index mutex poisoned".to_string(),
            )
        })?;
        match guard.get_mut(&key) {
            None => Ok(false),
            Some(row) if row.deleted_at_ms.is_some() => Ok(false),
            Some(row) => {
                row.deleted_at_ms = Some(now_ms);
                Ok(true)
            }
        }
    }

    /// Total row count (cross-tenant; for cardinality assertions in
    /// tests only — production callers MUST go through
    /// [`DedupIndex::find_missing_blobs`] which is tenant-scoped).
    #[must_use]
    pub fn rows_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Tenant-scoped row count.
    #[must_use]
    pub fn rows_count_for_tenant(&self, tenant_id: Uuid) -> usize {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.keys().filter(|(t, _)| *t == tenant_id).count()
    }
}

impl DedupIndex for InMemoryDedupIndex {
    fn find_missing_blobs(
        &self,
        tenant_id: Uuid,
        digests: &[BlobDigest],
    ) -> Result<FindMissingBlobsOutcome, DedupError> {
        if digests.len() > MAX_FIND_MISSING_BATCH_SIZE {
            return Err(DedupError::BatchTooLarge {
                got: digests.len(),
                limit: MAX_FIND_MISSING_BATCH_SIZE,
            });
        }
        if digests.is_empty() {
            return Ok(FindMissingBlobsOutcome {
                missing: Vec::new(),
                queried_count: 0,
            });
        }
        let guard = self.inner.lock().map_err(|_| {
            DedupError::IndexBackend(
                "in-memory dedup index mutex poisoned".to_string(),
            )
        })?;
        let mut missing: Vec<BlobDigest> = Vec::with_capacity(digests.len());
        for d in digests {
            let key = (tenant_id, d.as_hex().to_string());
            let present = matches!(
                guard.get(&key),
                Some(row) if row.deleted_at_ms.is_none()
            );
            if !present {
                missing.push(d.clone());
            }
        }
        Ok(FindMissingBlobsOutcome {
            missing,
            queried_count: digests.len(),
        })
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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

    fn ten_a() -> Uuid {
        Uuid::from_u128(0x1)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0x2)
    }

    fn dig(seed: u8) -> BlobDigest {
        let bytes = [seed; 32];
        BlobDigest::new(hex_encode_lower(&bytes)).unwrap()
    }

    fn hex_encode_lower(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    // -- BlobDigest validation -------------------------------------

    #[test]
    fn blob_digest_rejects_short_input() {
        let err = BlobDigest::new("abcd").unwrap_err();
        assert!(matches!(err, DedupError::InvalidChunkDigest { .. }));
    }

    #[test]
    fn blob_digest_rejects_uppercase_hex() {
        // 64 chars but uppercase — canonical form is lower-hex.
        let upper = "A".repeat(64);
        let err = BlobDigest::new(upper).unwrap_err();
        assert!(matches!(err, DedupError::InvalidChunkDigest { .. }));
    }

    #[test]
    fn blob_digest_rejects_non_hex_character() {
        let mut bad = "0".repeat(63);
        bad.push('z');
        let err = BlobDigest::new(bad).unwrap_err();
        assert!(matches!(err, DedupError::InvalidChunkDigest { .. }));
    }

    #[test]
    fn blob_digest_accepts_canonical_lower_hex() {
        let canonical = "0".repeat(64);
        let d = BlobDigest::new(canonical.clone()).unwrap();
        assert_eq!(d.as_hex(), canonical);
        assert_eq!(d.to_string(), canonical);
    }

    // -- DedupConfig default ---------------------------------------

    #[test]
    fn dedup_config_default_disables_cross_tenant() {
        let c = DedupConfig::default();
        assert!(!c.cross_tenant_enabled);
    }

    // -- find_missing_blobs core semantics -------------------------

    #[test]
    fn empty_input_returns_empty_outcome() {
        let idx = InMemoryDedupIndex::new();
        let out = idx.find_missing_blobs(ten_a(), &[]).unwrap();
        assert!(out.missing.is_empty());
        assert_eq!(out.queried_count, 0);
    }

    #[test]
    fn all_present_returns_empty_missing() {
        let idx = InMemoryDedupIndex::new();
        let d1 = dig(1);
        let d2 = dig(2);
        let d3 = dig(3);
        idx.upsert_chunk(ten_a(), &d1).unwrap();
        idx.upsert_chunk(ten_a(), &d2).unwrap();
        idx.upsert_chunk(ten_a(), &d3).unwrap();
        let out = idx
            .find_missing_blobs(ten_a(), &[d1, d2, d3])
            .unwrap();
        assert!(out.missing.is_empty());
        assert_eq!(out.queried_count, 3);
    }

    #[test]
    fn all_absent_returns_full_input_in_input_order() {
        let idx = InMemoryDedupIndex::new();
        let d1 = dig(1);
        let d2 = dig(2);
        let d3 = dig(3);
        let inputs = [d1.clone(), d2.clone(), d3.clone()];
        let out = idx.find_missing_blobs(ten_a(), &inputs).unwrap();
        assert_eq!(out.missing, vec![d1, d2, d3]);
        assert_eq!(out.queried_count, 3);
    }

    #[test]
    fn partial_present_subset_preserves_input_order() {
        let idx = InMemoryDedupIndex::new();
        let p1 = dig(0x10);
        let p2 = dig(0x11);
        let a1 = dig(0x20);
        let a2 = dig(0x21);
        let a3 = dig(0x22);
        idx.upsert_chunk(ten_a(), &p1).unwrap();
        idx.upsert_chunk(ten_a(), &p2).unwrap();
        let inputs = [
            p1.clone(),
            a1.clone(),
            p2.clone(),
            a2.clone(),
            a3.clone(),
        ];
        let out = idx.find_missing_blobs(ten_a(), &inputs).unwrap();
        assert_eq!(out.missing, vec![a1, a2, a3]);
        assert_eq!(out.queried_count, 5);
    }

    /// CTRL-ISO-005 — Tenant B asks for digests Tenant A owns. AuthZ
    /// check returns `None` because the PK `(B.tenant_id, digest)`
    /// does not exist; the orchestrator returns the digest in
    /// `missing` — uniform with truly-never-existed.
    #[test]
    fn cross_tenant_blobs_surface_as_missing() {
        let idx = InMemoryDedupIndex::new();
        let d_a1 = dig(0x30);
        let d_a2 = dig(0x31);
        let d_a3 = dig(0x32);
        idx.upsert_chunk(ten_a(), &d_a1).unwrap();
        idx.upsert_chunk(ten_a(), &d_a2).unwrap();
        idx.upsert_chunk(ten_a(), &d_a3).unwrap();
        // Tenant B asks for them — all surface as missing.
        let out = idx
            .find_missing_blobs(
                ten_b(),
                &[d_a1.clone(), d_a2.clone(), d_a3.clone()],
            )
            .unwrap();
        assert_eq!(out.missing, vec![d_a1, d_a2, d_a3]);
        assert_eq!(out.queried_count, 3);
    }

    #[test]
    fn soft_deleted_chunks_surface_as_missing() {
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x40);
        idx.upsert_chunk(ten_a(), &d).unwrap();
        // Soft-delete via S-06 grace path.
        let was_soft_deleted = idx.soft_delete(ten_a(), &d, 1_700_000_000_000).unwrap();
        assert!(was_soft_deleted);
        let out = idx
            .find_missing_blobs(ten_a(), std::slice::from_ref(&d))
            .unwrap();
        assert_eq!(out.missing, vec![d]);
    }

    #[test]
    fn re_upload_after_soft_delete_undeletes() {
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x41);
        idx.upsert_chunk(ten_a(), &d).unwrap();
        idx.soft_delete(ten_a(), &d, 1_700_000_000_000).unwrap();
        // Client re-uploads after seeing `d` in `missing` — undelete.
        let inserted_again = idx.upsert_chunk(ten_a(), &d).unwrap();
        // Was already in the table (refcount-bump), but `deleted_at`
        // cleared → returns `false` (Reused).
        assert!(!inserted_again);
        let out = idx
            .find_missing_blobs(ten_a(), std::slice::from_ref(&d))
            .unwrap();
        assert!(out.missing.is_empty(), "post-undelete should be present");
    }

    #[test]
    fn duplicate_input_yields_duplicate_response() {
        // Bazel/Buck2 client deduping is the client's job; if we
        // collapsed dedup on the server, an off-by-one would surface
        // as silent client confusion. Pin the contract: input N
        // copies → output N copies (when the digest is missing).
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x50);
        let out = idx
            .find_missing_blobs(ten_a(), &[d.clone(), d.clone(), d.clone()])
            .unwrap();
        assert_eq!(out.missing, vec![d.clone(), d.clone(), d]);
        assert_eq!(out.queried_count, 3);
    }

    #[test]
    fn batch_at_max_completes() {
        let idx = InMemoryDedupIndex::new();
        let digests: Vec<BlobDigest> = (0..MAX_FIND_MISSING_BATCH_SIZE as u8)
            .map(dig)
            .collect();
        let out = idx.find_missing_blobs(ten_a(), &digests).unwrap();
        assert_eq!(out.missing.len(), MAX_FIND_MISSING_BATCH_SIZE);
        assert_eq!(out.queried_count, MAX_FIND_MISSING_BATCH_SIZE);
    }

    #[test]
    fn batch_above_max_rejected() {
        let idx = InMemoryDedupIndex::new();
        // 251 > MAX (250). We can't blow seed range u8 (0..=255), so
        // a generated batch of 251 distinct digests fits.
        let digests: Vec<BlobDigest> =
            (0..(MAX_FIND_MISSING_BATCH_SIZE + 1) as u8).map(dig).collect();
        let err = idx.find_missing_blobs(ten_a(), &digests).unwrap_err();
        match err {
            DedupError::BatchTooLarge { got, limit } => {
                assert_eq!(got, MAX_FIND_MISSING_BATCH_SIZE + 1);
                assert_eq!(limit, MAX_FIND_MISSING_BATCH_SIZE);
            }
            other => panic!("expected BatchTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn upsert_first_inserts_then_reuses() {
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x60);
        let inserted = idx.upsert_chunk(ten_a(), &d).unwrap();
        assert!(inserted, "first upsert should report inserted=true");
        let reused = idx.upsert_chunk(ten_a(), &d).unwrap();
        assert!(!reused, "second upsert should report inserted=false (reused)");
        let reused_again = idx.upsert_chunk(ten_a(), &d).unwrap();
        assert!(!reused_again);
    }

    #[test]
    fn rows_count_per_tenant() {
        let idx = InMemoryDedupIndex::new();
        idx.upsert_chunk(ten_a(), &dig(1)).unwrap();
        idx.upsert_chunk(ten_a(), &dig(2)).unwrap();
        idx.upsert_chunk(ten_b(), &dig(3)).unwrap();
        assert_eq!(idx.rows_count(), 3);
        assert_eq!(idx.rows_count_for_tenant(ten_a()), 2);
        assert_eq!(idx.rows_count_for_tenant(ten_b()), 1);
    }

    #[test]
    fn idempotent_repeated_calls_same_result() {
        // Determinism property hand-rolled at unit level (property
        // test exercises 10k iter at the public API surface).
        let idx = InMemoryDedupIndex::new();
        let d1 = dig(1);
        let d2 = dig(2);
        let d3 = dig(3);
        idx.upsert_chunk(ten_a(), &d1).unwrap();
        let inputs = [d1, d2, d3];
        let r1 = idx.find_missing_blobs(ten_a(), &inputs).unwrap();
        let r2 = idx.find_missing_blobs(ten_a(), &inputs).unwrap();
        let r3 = idx.find_missing_blobs(ten_a(), &inputs).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    #[test]
    fn soft_delete_idempotent() {
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x70);
        idx.upsert_chunk(ten_a(), &d).unwrap();
        assert!(idx.soft_delete(ten_a(), &d, 1).unwrap());
        // Second soft_delete is a no-op (returns false).
        assert!(!idx.soft_delete(ten_a(), &d, 2).unwrap());
        // Soft-delete on an absent row is also a no-op.
        assert!(!idx.soft_delete(ten_a(), &dig(0x71), 1).unwrap());
    }

    #[test]
    fn cross_tenant_soft_delete_does_not_affect_other_tenant() {
        let idx = InMemoryDedupIndex::new();
        let d = dig(0x80);
        idx.upsert_chunk(ten_a(), &d).unwrap();
        idx.upsert_chunk(ten_b(), &d).unwrap();
        // Soft-delete A's row.
        idx.soft_delete(ten_a(), &d, 1).unwrap();
        // A sees `d` as missing.
        let out_a = idx
            .find_missing_blobs(ten_a(), std::slice::from_ref(&d))
            .unwrap();
        assert_eq!(out_a.missing, vec![d.clone()]);
        // B still sees `d` as present.
        let out_b = idx
            .find_missing_blobs(ten_b(), std::slice::from_ref(&d))
            .unwrap();
        assert!(out_b.missing.is_empty());
    }

    #[test]
    fn outcome_equality_drives_determinism_assertions() {
        // FindMissingBlobsOutcome derives Eq so property tests can
        // assert byte-for-byte equivalence.
        let a = FindMissingBlobsOutcome {
            missing: vec![dig(1)],
            queried_count: 2,
        };
        let b = FindMissingBlobsOutcome {
            missing: vec![dig(1)],
            queried_count: 2,
        };
        assert_eq!(a, b);
    }
}
