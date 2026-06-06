//! Multipart session row store trait + InMemory fake (WI-S05-001 §6.1).
//!
//! Mirrors the trait-abstraction-defer pattern used by `reapi::ac::meta`:
//! the SplitBlob/SpliceBlob handler depends on the [`SessionStore`]
//! trait surface; this module ships [`InMemorySessionStore`] preserving
//! every documented semantic; the real Cloudflare D1 binding shim is
//! wired in WI-S05-004 (schema) + WI-S05-006 (binding) per the charter
//! trait-abstraction-defer pattern.
//!
//! ## Tenant isolation seam
//!
//! [`SessionKey::new`] is the only legal way to address a session row;
//! it takes `(tenant_id, blob_digest)` by value. The trait surface has
//! no method that exposes a "raw" `(tenant_id, session_id)` shape
//! without going through the keyed lookup, so cross-tenant probing is
//! unreachable through the public API.
//!
//! ## State machine + idempotency
//!
//! ```text
//!         init_split → Live ──────append_chunk(*) ─────┐
//!                       │                              │
//!                       │                              ▼
//!                       │                          finalize_split
//!                       │                              │
//!                       │                              ▼
//!                       │                         Finalized {manifest_digest, chunk_count}
//!                       │
//!                       └─────── abort_split ─────► Aborted
//! ```
//!
//! - `append_chunk` is rejected when state is not `Live`.
//! - `finalize` is idempotent (second call on the same Finalized
//!   session returns the cached digest via the canonical [`SessionStore::lookup`]
//!   path; the handler short-circuits prior to invoking
//!   [`SessionStore::finalize`] a second time).
//! - `abort` is idempotent on Aborted; rejected on Finalized
//!   (`INV-MULTIPART-FINALIZE-IRREVOCABLE`).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::collections::HashMap;
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (infallible lock).
// `SessionStoreError::Backend("…mutex poisoned")` is structurally
// unreachable on this in-memory backend; preserved on the error
// surface for transport-class failures on non-in-memory bindings.
use parking_lot::{Mutex, MutexGuard};

use corelink_tenant_path::TenantPrefix;
use thiserror::Error;
use uuid::Uuid;

use super::types::{BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId};
use crate::region::Region;

/// Composite primary key for the multipart session row store:
/// `(tenant_id, blob_digest)`. Constructing one is the only legal way
/// to address a row through the [`SessionStore`] trait surface — cross-
/// tenant lookups are structurally unreachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionKey {
    tenant_id: Uuid,
    blob_digest: BlobDigest,
}

impl SessionKey {
    /// Build a fresh [`SessionKey`].
    #[must_use]
    pub const fn new(tenant_id: Uuid, blob_digest: BlobDigest) -> Self {
        Self {
            tenant_id,
            blob_digest,
        }
    }

    /// Borrow the tenant id.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Borrow the blob digest.
    #[must_use]
    pub const fn blob_digest(&self) -> &BlobDigest {
        &self.blob_digest
    }
}

/// Session lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// Session is open for `append_chunk` calls.
    Live,
    /// Session was finalized — manifest is sealed.
    Finalized {
        /// Manifest digest produced by [`super::assembler::BlobAssembler`].
        manifest_digest: ManifestDigest,
        /// Number of chunks bound at finalize time.
        chunk_count: u32,
    },
    /// Session was aborted — no further mutations accepted.
    Aborted,
}

/// One row in the multipart_sessions table. Snapshot returned by
/// [`SessionStore::lookup`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    /// Composite PK.
    pub key: SessionKey,
    /// Server-minted session id (UUIDv7 in production).
    pub session_id: SessionId,
    /// Current state.
    pub state: SessionState,
    /// Region the row was minted under (residency anchor).
    pub region: Region,
    /// Tenant prefix materialized at INSERT time.
    pub tenant_prefix: TenantPrefix,
    /// Bound chunks in canonical order. Empty until first append.
    pub chunks: Vec<BoundChunk>,
    /// Unix epoch ms; sticky on first INSERT.
    pub created_at_ms: u64,
    /// Unix epoch ms; refreshed on every append + finalize + abort.
    pub last_activity_at_ms: u64,
}

/// One chunk slot bound to a session in canonical order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundChunk {
    /// Canonical index of the chunk in the manifest order.
    pub index: ChunkIndex,
    /// Content-addressed digest of the chunk bytes.
    pub digest: ChunkDigest,
    /// Chunk size in bytes (mirrors what `r2://chunk/<digest>` will
    /// hold in production).
    pub size_bytes: u64,
}

/// Initial parameters for [`SessionStore::open`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionInit {
    /// Composite PK.
    pub key: SessionKey,
    /// Region the session is pinned to.
    pub region: Region,
    /// Tenant prefix materialized at INSERT time.
    pub tenant_prefix: TenantPrefix,
    /// Wall-clock instant for the row's `created_at_ms` /
    /// `last_activity_at_ms` columns.
    pub created_at_ms: u64,
}

/// Finalize parameters for [`SessionStore::finalize`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionFinalize {
    /// Tenant scoping (defense-in-depth — mirrors the row's PK).
    pub tenant_id: Uuid,
    /// Server-minted session id.
    pub session_id: SessionId,
    /// Manifest digest produced by the assembler.
    pub manifest_digest: ManifestDigest,
    /// Number of chunks bound at finalize time.
    pub chunk_count: u32,
    /// Wall-clock instant for the `last_activity_at_ms` refresh.
    pub finalized_at_ms: u64,
}

/// Errors surfaced by [`SessionStore`] methods.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SessionStoreError {
    /// Row absent (or cross-tenant masked per ADR-0028 uniform-404
    /// freeze).
    #[error("session not found")]
    NotFound,
    /// Mutation attempted on a finalized session.
    #[error("session already finalized")]
    AlreadyFinalized,
    /// Mutation attempted on an aborted session.
    #[error("session aborted")]
    Aborted,
    /// `chunk_index` order violation: gap, out-of-order, or
    /// duplicate-with-different-bytes.
    #[error("chunk ordering violation at index {index}: {reason}")]
    OrderingViolation {
        /// Offending index.
        index: u32,
        /// Short canonical reason code (`"out_of_order"` /
        /// `"bytes_mismatch"`).
        reason: &'static str,
    },
    /// Backend storage failure.
    #[error("session backend unavailable: {0}")]
    Backend(String),
}

/// Trait surface for the multipart session row store.
///
/// Production wiring (WI-S05-004 schema + WI-S05-006 binding)
/// implements this against a Cloudflare D1 multipart_sessions table.
pub trait SessionStore: Send + Sync {
    /// Open a fresh session for `init.key`. Mints the canonical
    /// session id (UUIDv7-shaped in production; deterministic in the
    /// in-memory fake).
    ///
    /// # Errors
    ///
    /// Backend-class only — duplicate-blob handling is delegated to
    /// the handler via [`Self::find_live_by_blob`] BEFORE invoking
    /// `open` (see `SplitSpliceHandlerImpl::init_split_inner`).
    fn open<'a>(
        &'a self,
        init: SessionInit,
    ) -> impl Future<Output = Result<SessionId, SessionStoreError>> + Send + 'a;

    /// Look up the canonical session snapshot by `(tenant_id,
    /// session_id)`. Returns `None` when the row is absent under THIS
    /// tenant — cross-tenant lookups MUST mask as `None`.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn lookup<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> impl Future<Output = Result<Option<SessionSnapshot>, SessionStoreError>> + Send + 'a;

    /// Lookup by `(tenant_id, blob_digest)`; returns the live
    /// session_id if any. Used by the handler to enforce live-session
    /// echo semantics (SplitBlob is idempotent on the blob_digest).
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn find_live_by_blob<'a>(
        &'a self,
        tenant_id: Uuid,
        blob_digest: &'a BlobDigest,
    ) -> impl Future<Output = Result<Option<SessionId>, SessionStoreError>> + Send + 'a;

    /// Append a chunk to the session at the canonical `chunk_index`
    /// slot. Returns the new chunk count post-append.
    ///
    /// # Errors
    ///
    /// - [`SessionStoreError::NotFound`] / `AlreadyFinalized` /
    ///   `Aborted` per state.
    /// - [`SessionStoreError::OrderingViolation`] on gap,
    ///   out-of-order, or duplicate-with-different-bytes.
    fn append_chunk<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
        chunk_index: ChunkIndex,
        chunk_digest: ChunkDigest,
        size_bytes: u64,
    ) -> impl Future<Output = Result<u32, SessionStoreError>> + Send + 'a;

    /// Snapshot the bound chunks in canonical order.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn bound_chunks<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> impl Future<Output = Result<Vec<BoundChunk>, SessionStoreError>> + Send + 'a;

    /// Finalize the session. Idempotent on already-finalized rows
    /// matching the same manifest digest.
    ///
    /// # Errors
    ///
    /// Backend-class plus `Aborted` on aborted rows.
    fn finalize<'a>(
        &'a self,
        params: SessionFinalize,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send + 'a;

    /// Abort the session. Idempotent on Aborted; rejected on
    /// Finalized.
    ///
    /// # Errors
    ///
    /// Backend-class plus `AlreadyFinalized` on finalized rows.
    fn abort<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
        now_ms: u64,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send + 'a;

    /// Enumerate orphan candidates: sessions in [`SessionState::Live`]
    /// whose `last_activity_at_ms` is older than `now_ms - max_age_ms`
    /// and pinned to `region`. Used by the WI-S05-006 sweeper cron DO
    /// (`OrphanSweeper`) to discover sessions whose client never
    /// completed nor aborted (FM-060 `multipart-orphan` mitigation per
    /// PAT-SWEEPER-001).
    ///
    /// Bounded result set: at most `limit` rows. Production D1 binding
    /// implements this as a partial-index-driven SELECT
    /// (`uq_multipart_sessions_in_progress` per
    /// `corelink-multipart-schema` migration 0003) so the canonical
    /// scan never spans completed/aborted rows. Order is canonical
    /// `(tenant_id, last_activity_at_ms ASC)` so the sweeper drains
    /// the oldest backlog first.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn list_orphans<'a>(
        &'a self,
        region: Region,
        now_ms: u64,
        max_age_ms: u64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<OrphanCandidate>, SessionStoreError>> + Send + 'a;
}

/// One row surfaced by [`SessionStore::list_orphans`]: a stale Live
/// session ready for sweeper-driven abort.
///
/// The shape is intentionally narrow — sweeper consumers only need the
/// `(tenant_id, session_id, region, last_activity_at_ms)` quadruple to
/// emit the canonical `corelink.blob.split.aborted` audit record with
/// `reason = "orphan_swept"`. The full session snapshot stays accessible
/// via [`SessionStore::lookup`] when forensics (post-mortem walkthrough,
/// chaos PR triage) need to inspect the bound chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OrphanCandidate {
    /// Tenant scope (PK leftmost — sweeper enforces tenant-scoped
    /// abort downstream).
    pub tenant_id: Uuid,
    /// Server-minted session id.
    pub session_id: SessionId,
    /// Region the session was pinned to.
    pub region: Region,
    /// Wall-clock epoch ms of the most recent state-mutating call.
    pub last_activity_at_ms: u64,
}

/// In-memory multipart session store. Per-instance — no global state
/// (F-001 closure 2026-05-01: every shared collection lives on `Arc<Mutex<…>>`
/// fields, never on `static LazyLock`).
pub struct InMemorySessionStore {
    inner: Mutex<Inner>,
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemorySessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemorySessionStore")
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Inner {
    by_session_id: HashMap<(Uuid, SessionId), SessionSnapshot>,
    by_blob: HashMap<(Uuid, BlobDigest), SessionId>,
    /// Per-instance monotonic counter so the fake mints distinct
    /// session ids deterministically. Production (UUIDv7) is opaque.
    next_session_seq: u128,
    /// Per-(tenant_id, session_id) cache of `(chunk_index ->
    /// chunk_bytes_digest)` so `bytes_mismatch` detection can compare
    /// the previously-bound digest against the incoming digest without
    /// keeping the bytes themselves on the in-memory shape.
    bound_digests: HashMap<(Uuid, SessionId, u32), ChunkDigest>,
}

impl InMemorySessionStore {
    /// Construct a fresh in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Snapshot every persisted session — diagnostic helper for tests.
    pub fn snapshot(&self) -> Result<Vec<SessionSnapshot>, SessionStoreError> {
        let g = self.lock()?;
        Ok(g.by_session_id.values().cloned().collect())
    }

    /// Number of persisted sessions.
    pub fn len(&self) -> Result<usize, SessionStoreError> {
        Ok(self.lock()?.by_session_id.len())
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> Result<bool, SessionStoreError> {
        Ok(self.lock()?.by_session_id.is_empty())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, SessionStoreError> {
        // parking_lot lock is infallible (no poisoning). Signature
        // kept as `Result` for callers; `Err` is structurally
        // unreachable on this backend.
        Ok(self.inner.lock())
    }

    fn mint_session_id(inner: &mut Inner) -> SessionId {
        inner.next_session_seq = inner.next_session_seq.wrapping_add(1);
        // Encode the counter into a UUID — keeps the wire shape but
        // stays deterministic in tests. Production swaps for UUIDv7.
        let bytes = inner.next_session_seq.to_be_bytes();
        SessionId(Uuid::from_bytes(bytes))
    }
}

impl SessionStore for InMemorySessionStore {
    fn open<'a>(
        &'a self,
        init: SessionInit,
    ) -> impl Future<Output = Result<SessionId, SessionStoreError>> + Send + 'a {
        async move {
            let mut g = self.lock()?;
            let session_id = Self::mint_session_id(&mut g);
            let snap = SessionSnapshot {
                key: init.key,
                session_id,
                state: SessionState::Live,
                region: init.region,
                tenant_prefix: init.tenant_prefix,
                chunks: Vec::new(),
                created_at_ms: init.created_at_ms,
                last_activity_at_ms: init.created_at_ms,
            };
            g.by_session_id
                .insert((init.key.tenant_id(), session_id), snap);
            g.by_blob
                .insert((init.key.tenant_id(), *init.key.blob_digest()), session_id);
            Ok(session_id)
        }
    }

    fn lookup<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> impl Future<Output = Result<Option<SessionSnapshot>, SessionStoreError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            Ok(g.by_session_id.get(&(tenant_id, session_id)).cloned())
        }
    }

    fn find_live_by_blob<'a>(
        &'a self,
        tenant_id: Uuid,
        blob_digest: &'a BlobDigest,
    ) -> impl Future<Output = Result<Option<SessionId>, SessionStoreError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            let Some(&id) = g.by_blob.get(&(tenant_id, *blob_digest)) else {
                return Ok(None);
            };
            // Only return when the row is still Live.
            match g.by_session_id.get(&(tenant_id, id)) {
                Some(snap) if matches!(snap.state, SessionState::Live) => Ok(Some(id)),
                _ => Ok(None),
            }
        }
    }

    fn append_chunk<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
        chunk_index: ChunkIndex,
        chunk_digest: ChunkDigest,
        size_bytes: u64,
    ) -> impl Future<Output = Result<u32, SessionStoreError>> + Send + 'a {
        async move {
            let mut g = self.lock()?;
            let snap = g
                .by_session_id
                .get_mut(&(tenant_id, session_id))
                .ok_or(SessionStoreError::NotFound)?;
            match snap.state {
                SessionState::Live => {}
                SessionState::Finalized { .. } => {
                    return Err(SessionStoreError::AlreadyFinalized);
                }
                SessionState::Aborted => {
                    return Err(SessionStoreError::Aborted);
                }
            }
            let next_idx: u32 =
                snap.chunks.len().try_into().map_err(|_| {
                    SessionStoreError::Backend("chunk index overflow u32".to_string())
                })?;
            let submitted = chunk_index.0;
            // Idempotent retry on the same slot — must match the prior
            // digest bit-for-bit.
            let already_bound = g
                .bound_digests
                .get(&(tenant_id, session_id, submitted))
                .copied();
            if let Some(prior) = already_bound {
                if prior == chunk_digest {
                    // True idempotent retry — no-op; return current
                    // count without growing.
                    let snap_again = g
                        .by_session_id
                        .get(&(tenant_id, session_id))
                        .ok_or(SessionStoreError::NotFound)?;
                    let count: u32 = snap_again.chunks.len().try_into().map_err(|_| {
                        SessionStoreError::Backend("chunk index overflow u32".to_string())
                    })?;
                    return Ok(count);
                }
                return Err(SessionStoreError::OrderingViolation {
                    index: submitted,
                    reason: "bytes_mismatch",
                });
            }
            // Strict canonical ordering: the next slot the session
            // accepts is `next_idx`. We collapse "lower than next" and
            // "higher than next" into a single canonical reason —
            // both flavors are surface-level "out of order" from the
            // session's perspective; downstream chain consumers
            // distinguish via the `index` column.
            if submitted != next_idx {
                return Err(SessionStoreError::OrderingViolation {
                    index: submitted,
                    reason: "out_of_order",
                });
            }
            // Re-borrow `snap` mutably (the previous binding ended at the
            // immutable `bound_digests` lookup arm).
            let snap = g
                .by_session_id
                .get_mut(&(tenant_id, session_id))
                .ok_or(SessionStoreError::NotFound)?;
            snap.chunks.push(BoundChunk {
                index: chunk_index,
                digest: chunk_digest,
                size_bytes,
            });
            let new_count: u32 =
                snap.chunks.len().try_into().map_err(|_| {
                    SessionStoreError::Backend("chunk index overflow u32".to_string())
                })?;
            g.bound_digests
                .insert((tenant_id, session_id, submitted), chunk_digest);
            Ok(new_count)
        }
    }

    fn bound_chunks<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> impl Future<Output = Result<Vec<BoundChunk>, SessionStoreError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            let snap = g
                .by_session_id
                .get(&(tenant_id, session_id))
                .ok_or(SessionStoreError::NotFound)?;
            Ok(snap.chunks.clone())
        }
    }

    fn finalize<'a>(
        &'a self,
        params: SessionFinalize,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send + 'a {
        async move {
            let mut g = self.lock()?;
            let snap = g
                .by_session_id
                .get_mut(&(params.tenant_id, params.session_id))
                .ok_or(SessionStoreError::NotFound)?;
            match snap.state {
                SessionState::Live => {
                    snap.state = SessionState::Finalized {
                        manifest_digest: params.manifest_digest,
                        chunk_count: params.chunk_count,
                    };
                    snap.last_activity_at_ms = params.finalized_at_ms;
                    Ok(())
                }
                SessionState::Finalized {
                    manifest_digest, ..
                } => {
                    if manifest_digest == params.manifest_digest {
                        // Idempotent.
                        Ok(())
                    } else {
                        Err(SessionStoreError::Backend(format!(
                            "finalize manifest mismatch: existing={manifest_digest}, attempted={}",
                            params.manifest_digest
                        )))
                    }
                }
                SessionState::Aborted => Err(SessionStoreError::Aborted),
            }
        }
    }

    fn abort<'a>(
        &'a self,
        tenant_id: Uuid,
        session_id: SessionId,
        now_ms: u64,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send + 'a {
        async move {
            let mut g = self.lock()?;
            let snap = g
                .by_session_id
                .get_mut(&(tenant_id, session_id))
                .ok_or(SessionStoreError::NotFound)?;
            match snap.state {
                SessionState::Live => {
                    snap.state = SessionState::Aborted;
                    snap.last_activity_at_ms = now_ms;
                    Ok(())
                }
                SessionState::Aborted => Ok(()), // idempotent
                SessionState::Finalized { .. } => Err(SessionStoreError::AlreadyFinalized),
            }
        }
    }

    fn list_orphans<'a>(
        &'a self,
        region: Region,
        now_ms: u64,
        max_age_ms: u64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<OrphanCandidate>, SessionStoreError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            let mut out: Vec<OrphanCandidate> = g
                .by_session_id
                .values()
                .filter(|snap| {
                    matches!(snap.state, SessionState::Live)
                        && snap.region == region
                        && now_ms.saturating_sub(snap.last_activity_at_ms) >= max_age_ms
                })
                .map(|snap| OrphanCandidate {
                    tenant_id: snap.key.tenant_id(),
                    session_id: snap.session_id,
                    region: snap.region,
                    last_activity_at_ms: snap.last_activity_at_ms,
                })
                .collect();
            // Canonical order: (tenant_id, last_activity_at_ms ASC) so
            // the sweeper drains the oldest backlog first AND so
            // sibling shards produce identical traces under the same
            // dataset (deterministic; aids RB-FM-060 dry-run drift
            // detection).
            out.sort_unstable_by(|a, b| {
                a.tenant_id
                    .cmp(&b.tenant_id)
                    .then(a.last_activity_at_ms.cmp(&b.last_activity_at_ms))
                    .then(a.session_id.0.cmp(&b.session_id.0))
            });
            out.truncate(limit);
            Ok(out)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use corelink_hash::Digest;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    fn bd(seed: &[u8]) -> BlobDigest {
        BlobDigest::new(Digest::compute(seed), seed.len() as u64)
    }

    fn cd(seed: &[u8]) -> ChunkDigest {
        ChunkDigest::compute(seed)
    }

    #[tokio::test]
    async fn open_then_lookup_returns_live_session() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::nil();
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"x")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        let snap = store.lookup(tenant, id).await.unwrap().unwrap();
        assert_eq!(snap.state, SessionState::Live);
        assert!(snap.chunks.is_empty());
    }

    #[tokio::test]
    async fn cross_tenant_lookup_masks_to_none() {
        let store = InMemorySessionStore::new();
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant_a, bd(b"x")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant_a),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        // B sees nothing.
        assert!(store.lookup(tenant_b, id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_chunk_canonical_order() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        let c0 = cd(b"c0");
        let c1 = cd(b"c1");
        let n0 = store
            .append_chunk(tenant, id, ChunkIndex(0), c0, 2)
            .await
            .unwrap();
        let n1 = store
            .append_chunk(tenant, id, ChunkIndex(1), c1, 2)
            .await
            .unwrap();
        assert_eq!(n0, 1);
        assert_eq!(n1, 2);
    }

    #[tokio::test]
    async fn append_chunk_out_of_order_rejected() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        let err = store
            .append_chunk(tenant, id, ChunkIndex(1), cd(b"x"), 1)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SessionStoreError::OrderingViolation { index: 1, .. }
        ));
    }

    #[tokio::test]
    async fn append_chunk_bytes_mismatch_rejected() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"original"), 8)
            .await
            .unwrap();
        let err = store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"DIFFERENT"), 9)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SessionStoreError::OrderingViolation {
                reason: "bytes_mismatch",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn finalize_then_append_rejected() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"a"), 1)
            .await
            .unwrap();
        store
            .finalize(SessionFinalize {
                tenant_id: tenant,
                session_id: id,
                manifest_digest: ManifestDigest::from_digest(Digest::compute(b"m")),
                chunk_count: 1,
                finalized_at_ms: 2,
            })
            .await
            .unwrap();
        let err = store
            .append_chunk(tenant, id, ChunkIndex(1), cd(b"b"), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, SessionStoreError::AlreadyFinalized));
    }

    #[tokio::test]
    async fn abort_idempotent_finalize_rejected() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        store.abort(tenant, id, 2).await.unwrap();
        store.abort(tenant, id, 3).await.unwrap(); // idempotent
        let err = store
            .finalize(SessionFinalize {
                tenant_id: tenant,
                session_id: id,
                manifest_digest: ManifestDigest::from_digest(Digest::compute(b"m")),
                chunk_count: 0,
                finalized_at_ms: 4,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, SessionStoreError::Aborted));
    }

    #[tokio::test]
    async fn idempotent_append_same_bytes_ok() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(b"y")),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        let n1 = store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"same"), 4)
            .await
            .unwrap();
        let n2 = store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"same"), 4)
            .await
            .unwrap();
        assert_eq!(n1, 1);
        assert_eq!(n2, 1, "idempotent retry must NOT grow the chunk count");
    }

    #[tokio::test]
    async fn find_live_by_blob_returns_some_then_none_post_finalize() {
        let store = InMemorySessionStore::new();
        let tenant = Uuid::from_u128(1);
        let blob = bd(b"y");
        let id = store
            .open(SessionInit {
                key: SessionKey::new(tenant, blob),
                region: Region::Wnam,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms: 1,
            })
            .await
            .unwrap();
        let live = store.find_live_by_blob(tenant, &blob).await.unwrap();
        assert_eq!(live, Some(id));
        store
            .append_chunk(tenant, id, ChunkIndex(0), cd(b"a"), 1)
            .await
            .unwrap();
        store
            .finalize(SessionFinalize {
                tenant_id: tenant,
                session_id: id,
                manifest_digest: ManifestDigest::from_digest(Digest::compute(b"m")),
                chunk_count: 1,
                finalized_at_ms: 2,
            })
            .await
            .unwrap();
        let live = store.find_live_by_blob(tenant, &blob).await.unwrap();
        assert!(live.is_none(), "live lookup must return None post-finalize");
    }
}
