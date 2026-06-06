//! `blob_meta` LRU row + soft-delete store trait + InMemory fake.
//!
//! ## Scope vs S-06 GC
//!
//! This module covers the **BLOB-only** soft-delete envelope per Lote
//! 10.7bis P0-8: chunks (with `chunks.refcount` lifecycle) are owned by
//! S-06 GC sweep + reconcile; eviction NEVER touches the chunks table.
//!
//! ## INV-EVICT-SOFT-DELETE-FIRST (HIGH)
//!
//! Eviction sets `blob_meta.deleted_at_ms = now()`; physical R2 delete
//! is delegated to S-06 WI-S06-004 cron post-grace 72h. The fake
//! mirrors the canonical SQL:
//!
//! ```sql
//! UPDATE blob_meta
//!   SET deleted_at = ?
//!   WHERE tenant_id = ?
//!     AND digest = ?
//!     AND deleted_at IS NULL
//! RETURNING size_bytes;
//! ```
//!
//! - `deleted_at IS NULL` — idempotent guard (re-eviction is a no-op).
//! - `RETURNING size_bytes` — the byte count surfaces in the
//!   `bytes_reclaimed` aggregate.
//!
//! ## InMemory fake — what it mirrors from S-01 `blob_meta` table
//!
//! The fake stores `(tenant_id, digest) → BlobLruRow { size_bytes,
//! created_at_ms, last_accessed_at_ms, deleted_at_ms }` and computes
//! soft-delete via per-key `BTreeMap::get_mut` + the `deleted_at IS
//! NULL` predicate.

use std::collections::BTreeMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical chunk-digest hex length (BLAKE3-256 hex).
pub const BLOB_DIGEST_HEX_LEN: usize = 64;

/// Errors surfaced by [`BlobMetaSoftDeleteStore`] backends.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlobMetaError {
    /// Backend transport failure.
    #[error("blob_meta backend error: {0}")]
    Backend(String),
    /// Programmer wiring error — the digest hex shape is invalid.
    #[error("invalid blob digest shape: {reason}")]
    InvalidDigest {
        /// Short reason code.
        reason: &'static str,
    },
}

/// Canonical blob digest (BLAKE3-256 hex). Distinct type from the
/// `corelink-dedup::BlobDigest` (which is the chunk-digest layer);
/// eviction operates on whole-blob digests stored in `blob_meta`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvictionBlobDigest(String);

impl EvictionBlobDigest {
    /// Construct from a 64-char lower-hex string.
    ///
    /// # Errors
    ///
    /// Returns [`BlobMetaError::InvalidDigest`] when the input is not
    /// 64 chars OR contains non-hex characters.
    pub fn new(hex: impl Into<String>) -> Result<Self, BlobMetaError> {
        let s: String = hex.into();
        if s.len() != BLOB_DIGEST_HEX_LEN {
            return Err(BlobMetaError::InvalidDigest {
                reason: "length != 64",
            });
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(BlobMetaError::InvalidDigest {
                reason: "non-hex character",
            });
        }
        Ok(Self(s))
    }

    /// Borrow the canonical hex.
    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// First-8-hex prefix (privacy-friendly forensic trail; matches
    /// the GC sweep audit pattern).
    #[must_use]
    pub fn hex8(&self) -> &str {
        let n = self.0.len().min(8);
        self.0.get(..n).unwrap_or_default()
    }
}

impl core::fmt::Display for EvictionBlobDigest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Materialised projection of a `blob_meta` row used by the eviction
/// phase for the LRU + TTL decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobLruRow {
    /// Canonical hex digest.
    pub digest: EvictionBlobDigest,
    /// Blob payload size (bytes).
    pub size_bytes: u64,
    /// Created instant (`blob_meta.created_at_ms`).
    pub created_at_ms: u64,
    /// LRU watermark — `blob_meta.last_accessed_at_ms`. Updated by
    /// hot-path (WI-S07-004) on every CAS GET.
    pub last_accessed_at_ms: u64,
    /// Soft-delete tombstone — `Some(swept_at_ms)` once eviction (or
    /// S-06 sweep) soft-deletes the row.
    pub deleted_at_ms: Option<u64>,
}

/// Outcome surfaced by [`BlobMetaSoftDeleteStore::soft_delete_for_eviction`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SoftDeleteOutcome {
    /// Soft-delete fired. Carries the prev-state size in bytes for
    /// the `bytes_reclaimed` aggregate.
    Deleted {
        /// `blob_meta.size_bytes` of the row just soft-deleted.
        size_bytes: u64,
    },
    /// Idempotent re-run — the row was already soft-deleted OR absent.
    AlreadyResolved,
}

/// Outcome surfaced by [`BlobMetaSoftDeleteStore::update_last_accessed_at_ms`].
///
/// Mirrors the canonical conditional UPDATE envelope (WI-S07-004 §6.1.6):
///
/// ```sql
/// UPDATE blob_meta
///   SET last_accessed_at = ?
///   WHERE tenant_id = ?
///     AND digest = ?
///     AND deleted_at IS NULL
///     AND last_accessed_at < ?
/// ```
///
/// The `last_accessed_at < ?` predicate enforces strict monotonicity
/// (INV-AC-TTL-MONOTONIC inheritance pattern): an out-of-order write
/// with `new_value <= existing_value` is silently dropped — the SQL
/// UPDATE returns 0 rows affected, the in-memory fake returns
/// [`LruUpdateOutcome::Skipped`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LruUpdateOutcome {
    /// The conditional UPDATE fired — `last_accessed_at_ms` advanced
    /// monotonically from `prev_ms` to `new_ms`.
    Updated {
        /// Prior `last_accessed_at_ms` value (pre-update).
        prev_ms: u64,
        /// New `last_accessed_at_ms` value (post-update).
        new_ms: u64,
    },
    /// Conditional predicate failed — `new_ms <= existing.last_accessed_at_ms`
    /// (out-of-order write); UPDATE skipped to preserve monotonicity.
    Skipped {
        /// Existing `last_accessed_at_ms` value that is `>= new_ms`.
        existing_ms: u64,
    },
    /// Row absent OR soft-deleted (`deleted_at IS NOT NULL`); the UPDATE
    /// is a no-op (LRU is best-effort; deleted rows are never updated).
    AlreadyResolved,
}

/// Trait surfaced by every `blob_meta` soft-delete backend
/// (production D1 row UPDATE / in-memory fake).
pub trait BlobMetaSoftDeleteStore: Send + Sync + core::fmt::Debug {
    /// Lookup the current `blob_meta` row. Returns `Ok(None)` when
    /// the row does not exist or belongs to a different tenant
    /// (Layer 4 envelope).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
    ) -> Result<Option<BlobLruRow>, BlobMetaError>;

    /// Soft-delete a `blob_meta` row for the eviction worker. Mirrors
    /// the canonical SQL:
    ///
    /// ```sql
    /// UPDATE blob_meta
    ///   SET deleted_at = ?
    ///   WHERE tenant_id = ?
    ///     AND digest = ?
    ///     AND deleted_at IS NULL
    /// RETURNING size_bytes;
    /// ```
    ///
    /// Returns [`SoftDeleteOutcome::Deleted`] when the UPDATE fires;
    /// [`SoftDeleteOutcome::AlreadyResolved`] when the row was already
    /// soft-deleted OR absent (idempotent re-run no-op).
    ///
    /// # Errors
    ///
    /// Backend transport errors. Cross-tenant attempts surface as a
    /// `Backend` error (sqlx prepared rejects in production; the
    /// in-memory fake mirrors via the tenant-leftmost PK lookup).
    fn soft_delete_for_eviction(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        now_ms: u64,
    ) -> Result<SoftDeleteOutcome, BlobMetaError>;

    /// List candidate digests for a tenant whose `last_accessed_at_ms
    /// < cutoff_ms` AND `deleted_at IS NULL`. Order: `last_accessed_at_ms
    /// ASC` (coldest first). Bounded by `limit` (D1 batch cap; mirrors
    /// `MAX_LRU_BATCH_SIZE`).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn list_lru_candidates(
        &self,
        tenant_id: Uuid,
        cutoff_ms: u64,
        limit: usize,
    ) -> Result<Vec<BlobLruRow>, BlobMetaError>;

    /// Conditional monotone UPDATE of `blob_meta.last_accessed_at_ms`
    /// driven by the WI-S07-004 LRU tracker batch flush. Mirrors the
    /// canonical SQL:
    ///
    /// ```sql
    /// UPDATE blob_meta
    ///   SET last_accessed_at = ?
    ///   WHERE tenant_id = ?
    ///     AND digest = ?
    ///     AND deleted_at IS NULL
    ///     AND last_accessed_at < ?
    /// ```
    ///
    /// The `last_accessed_at < new_ms` predicate enforces strict
    /// monotonicity (INV-AC-TTL-MONOTONIC inheritance pattern). Returns:
    ///
    /// - [`LruUpdateOutcome::Updated`] on a successful write.
    /// - [`LruUpdateOutcome::Skipped`] when `new_ms <= existing.last_accessed_at_ms`
    ///   (out-of-order write).
    /// - [`LruUpdateOutcome::AlreadyResolved`] when the row is absent
    ///   OR `deleted_at IS NOT NULL`.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn update_last_accessed_at_ms(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        new_ms: u64,
    ) -> Result<LruUpdateOutcome, BlobMetaError>;
}

/// In-memory `blob_meta` soft-delete store.
///
/// F-001 closure: per-instance `Mutex` (NOT process-global
/// `static LazyLock<Mutex<>>`).
#[derive(Debug, Default)]
pub struct InMemoryBlobMetaSoftDeleteStore {
    inner: Mutex<BTreeMap<(Uuid, EvictionBlobDigest), BlobLruRow>>,
}

impl InMemoryBlobMetaSoftDeleteStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a fresh row (test wiring).
    ///
    /// # Errors
    ///
    /// Returns [`BlobMetaError::Backend`] on mutex poisoning.
    pub fn push_row(&self, tenant_id: Uuid, row: BlobLruRow) -> Result<(), BlobMetaError> {
        let mut g = self.inner.lock().map_err(|_| {
            BlobMetaError::Backend("blob_meta soft-delete store mutex poisoned".to_string())
        })?;
        g.insert((tenant_id, row.digest.clone()), row);
        Ok(())
    }

    /// Snapshot a row.
    #[must_use]
    pub fn snapshot(&self, tenant_id: Uuid, digest: &EvictionBlobDigest) -> Option<BlobLruRow> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, digest.clone())).cloned()
    }

    /// Total row count.
    #[must_use]
    pub fn rows_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }
}

impl BlobMetaSoftDeleteStore for InMemoryBlobMetaSoftDeleteStore {
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
    ) -> Result<Option<BlobLruRow>, BlobMetaError> {
        let g = self.inner.lock().map_err(|_| {
            BlobMetaError::Backend("blob_meta soft-delete store mutex poisoned".to_string())
        })?;
        Ok(g.get(&(tenant_id, digest.clone())).cloned())
    }

    fn soft_delete_for_eviction(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        now_ms: u64,
    ) -> Result<SoftDeleteOutcome, BlobMetaError> {
        let mut g = self.inner.lock().map_err(|_| {
            BlobMetaError::Backend("blob_meta soft-delete store mutex poisoned".to_string())
        })?;
        let Some(row) = g.get_mut(&(tenant_id, digest.clone())) else {
            return Ok(SoftDeleteOutcome::AlreadyResolved);
        };
        if row.deleted_at_ms.is_some() {
            return Ok(SoftDeleteOutcome::AlreadyResolved);
        }
        let size_bytes = row.size_bytes;
        row.deleted_at_ms = Some(now_ms);
        Ok(SoftDeleteOutcome::Deleted { size_bytes })
    }

    fn list_lru_candidates(
        &self,
        tenant_id: Uuid,
        cutoff_ms: u64,
        limit: usize,
    ) -> Result<Vec<BlobLruRow>, BlobMetaError> {
        let g = self.inner.lock().map_err(|_| {
            BlobMetaError::Backend("blob_meta soft-delete store mutex poisoned".to_string())
        })?;
        let mut candidates: Vec<BlobLruRow> = g
            .iter()
            .filter(|((t, _), row)| {
                *t == tenant_id
                    && row.deleted_at_ms.is_none()
                    && row.last_accessed_at_ms < cutoff_ms
            })
            .map(|(_, row)| row.clone())
            .collect();
        candidates.sort_by_key(|r| r.last_accessed_at_ms);
        candidates.truncate(limit);
        Ok(candidates)
    }

    fn update_last_accessed_at_ms(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        new_ms: u64,
    ) -> Result<LruUpdateOutcome, BlobMetaError> {
        let mut g = self.inner.lock().map_err(|_| {
            BlobMetaError::Backend("blob_meta soft-delete store mutex poisoned".to_string())
        })?;
        let Some(row) = g.get_mut(&(tenant_id, digest.clone())) else {
            return Ok(LruUpdateOutcome::AlreadyResolved);
        };
        if row.deleted_at_ms.is_some() {
            return Ok(LruUpdateOutcome::AlreadyResolved);
        }
        if new_ms <= row.last_accessed_at_ms {
            return Ok(LruUpdateOutcome::Skipped {
                existing_ms: row.last_accessed_at_ms,
            });
        }
        let prev_ms = row.last_accessed_at_ms;
        row.last_accessed_at_ms = new_ms;
        Ok(LruUpdateOutcome::Updated { prev_ms, new_ms })
    }
}

impl From<BlobMetaError> for crate::error::EvictionError {
    fn from(err: BlobMetaError) -> Self {
        crate::error::EvictionError::Backend(err.to_string())
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

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn dig(seed: u8) -> EvictionBlobDigest {
        let bytes = [seed; 32];
        let mut s = String::with_capacity(64);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        EvictionBlobDigest::new(s).unwrap()
    }

    fn row(seed: u8, last_accessed_ms: u64, size: u64) -> BlobLruRow {
        BlobLruRow {
            digest: dig(seed),
            size_bytes: size,
            created_at_ms: 1,
            last_accessed_at_ms: last_accessed_ms,
            deleted_at_ms: None,
        }
    }

    #[test]
    fn digest_rejects_short_input() {
        let err = EvictionBlobDigest::new("abcd").unwrap_err();
        assert!(matches!(err, BlobMetaError::InvalidDigest { .. }));
    }

    #[test]
    fn digest_rejects_uppercase() {
        let err = EvictionBlobDigest::new("A".repeat(64)).unwrap_err();
        assert!(matches!(err, BlobMetaError::InvalidDigest { .. }));
    }

    #[test]
    fn digest_accepts_canonical_lower_hex() {
        let canonical = "0".repeat(64);
        let d = EvictionBlobDigest::new(canonical.clone()).unwrap();
        assert_eq!(d.as_hex(), canonical);
    }

    #[test]
    fn digest_hex8_returns_first_eight_chars() {
        let d = dig(0xab);
        assert_eq!(d.hex8(), "abababab");
    }

    #[test]
    fn lookup_returns_none_for_missing() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let r = s.lookup(ten_a(), &dig(1)).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn lookup_round_trips_inserted_row() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let r = row(1, 100, 4096);
        s.push_row(ten_a(), r.clone()).unwrap();
        let got = s.lookup(ten_a(), &r.digest).unwrap().unwrap();
        assert_eq!(got, r);
    }

    #[test]
    fn cross_tenant_lookup_returns_none() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let r = row(1, 100, 4096);
        s.push_row(ten_a(), r.clone()).unwrap();
        let got = s.lookup(ten_b(), &r.digest).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn soft_delete_fires_once_then_idempotent() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let r = row(1, 100, 4096);
        s.push_row(ten_a(), r.clone()).unwrap();
        let out = s.soft_delete_for_eviction(ten_a(), &r.digest, 200).unwrap();
        assert!(matches!(
            out,
            SoftDeleteOutcome::Deleted { size_bytes: 4096 }
        ));
        // Idempotent re-run.
        let out2 = s.soft_delete_for_eviction(ten_a(), &r.digest, 300).unwrap();
        assert!(matches!(out2, SoftDeleteOutcome::AlreadyResolved));
    }

    #[test]
    fn soft_delete_absent_row_returns_already_resolved() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let out = s.soft_delete_for_eviction(ten_a(), &dig(99), 200).unwrap();
        assert!(matches!(out, SoftDeleteOutcome::AlreadyResolved));
    }

    #[test]
    fn cross_tenant_soft_delete_does_not_affect_other_tenant() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let r = row(1, 100, 4096);
        s.push_row(ten_a(), r.clone()).unwrap();
        s.push_row(ten_b(), r.clone()).unwrap();
        s.soft_delete_for_eviction(ten_a(), &r.digest, 200).unwrap();
        let got_b = s.lookup(ten_b(), &r.digest).unwrap().unwrap();
        assert!(got_b.deleted_at_ms.is_none());
    }

    #[test]
    fn list_lru_candidates_returns_coldest_first() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        s.push_row(ten_a(), row(2, 50, 2)).unwrap();
        s.push_row(ten_a(), row(3, 200, 3)).unwrap();
        // cutoff_ms = 150 → only seeds 1 and 2 are cold enough.
        let v = s.list_lru_candidates(ten_a(), 150, 100).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].last_accessed_at_ms, 50);
        assert_eq!(v[1].last_accessed_at_ms, 100);
    }

    #[test]
    fn list_lru_candidates_respects_limit() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        for i in 0..10 {
            s.push_row(ten_a(), row(i, u64::from(i), 1)).unwrap();
        }
        let v = s.list_lru_candidates(ten_a(), 100, 3).unwrap();
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn list_lru_candidates_excludes_soft_deleted() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 50, 1)).unwrap();
        s.push_row(ten_a(), row(2, 60, 1)).unwrap();
        // Soft-delete seed 1.
        s.soft_delete_for_eviction(ten_a(), &dig(1), 100).unwrap();
        let v = s.list_lru_candidates(ten_a(), 200, 100).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].last_accessed_at_ms, 60);
    }

    #[test]
    fn list_lru_candidates_tenant_scoped() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 50, 1)).unwrap();
        s.push_row(ten_b(), row(2, 60, 1)).unwrap();
        let v_a = s.list_lru_candidates(ten_a(), 200, 100).unwrap();
        let v_b = s.list_lru_candidates(ten_b(), 200, 100).unwrap();
        assert_eq!(v_a.len(), 1);
        assert_eq!(v_b.len(), 1);
        assert_ne!(v_a[0].digest, v_b[0].digest);
    }

    // ---- update_last_accessed_at_ms (WI-S07-004) --------------------

    #[test]
    fn update_last_accessed_at_ms_advances_when_strictly_greater() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        let out = s.update_last_accessed_at_ms(ten_a(), &dig(1), 200).unwrap();
        assert!(matches!(
            out,
            LruUpdateOutcome::Updated {
                prev_ms: 100,
                new_ms: 200
            }
        ));
        let r = s.lookup(ten_a(), &dig(1)).unwrap().unwrap();
        assert_eq!(r.last_accessed_at_ms, 200);
    }

    #[test]
    fn update_last_accessed_at_ms_skipped_when_equal() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        let out = s.update_last_accessed_at_ms(ten_a(), &dig(1), 100).unwrap();
        assert!(matches!(
            out,
            LruUpdateOutcome::Skipped { existing_ms: 100 }
        ));
        let r = s.lookup(ten_a(), &dig(1)).unwrap().unwrap();
        assert_eq!(r.last_accessed_at_ms, 100);
    }

    #[test]
    fn update_last_accessed_at_ms_skipped_when_less() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        let out = s.update_last_accessed_at_ms(ten_a(), &dig(1), 50).unwrap();
        assert!(matches!(
            out,
            LruUpdateOutcome::Skipped { existing_ms: 100 }
        ));
    }

    #[test]
    fn update_last_accessed_at_ms_no_op_when_row_absent() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        let out = s
            .update_last_accessed_at_ms(ten_a(), &dig(99), 200)
            .unwrap();
        assert!(matches!(out, LruUpdateOutcome::AlreadyResolved));
    }

    #[test]
    fn update_last_accessed_at_ms_no_op_when_soft_deleted() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        s.soft_delete_for_eviction(ten_a(), &dig(1), 150).unwrap();
        let out = s.update_last_accessed_at_ms(ten_a(), &dig(1), 200).unwrap();
        assert!(matches!(out, LruUpdateOutcome::AlreadyResolved));
    }

    #[test]
    fn update_last_accessed_at_ms_tenant_scoped() {
        let s = InMemoryBlobMetaSoftDeleteStore::new();
        s.push_row(ten_a(), row(1, 100, 1)).unwrap();
        s.push_row(ten_b(), row(1, 100, 1)).unwrap();
        s.update_last_accessed_at_ms(ten_a(), &dig(1), 200).unwrap();
        let r_b = s.lookup(ten_b(), &dig(1)).unwrap().unwrap();
        // Tenant B's row is unaffected.
        assert_eq!(r_b.last_accessed_at_ms, 100);
    }
}
