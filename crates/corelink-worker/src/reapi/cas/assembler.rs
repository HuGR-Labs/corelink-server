//! Manifest builder + streaming verifier trait + InMemory fake
//! (WI-S05-001 §6.1.5–§6.1.6).
//!
//! The handler delegates Merkle-tree build + dual-side verify to this
//! seam. Production wiring (delegate WI-S05-005) plugs a real
//! BLAKE3 Merkle codec + canonical envelope signer; the in-memory fake
//! ships here so the SplitBlob/SpliceBlob handler can be exercised
//! without dragging the upstream crate (which is not yet sealed) into
//! the worker's dependency graph.
//!
//! ## Tenant isolation seam
//!
//! [`ManifestKey::new`] is the only legal way to address a manifest;
//! it takes `(tenant_id, blob_digest)` by value. The streaming surface
//! ([`BlobAssembler::stream_chunks`]) likewise takes a tenant_id +
//! tenant_prefix pair so the chunk lookup it routes through is keyed
//! by `(tenant_id, chunk_digest)` — preserving the `INV-TENANT-ISOLATION`
//! defense at every hop.
//!
//! ## Streaming fail-fast invariant (`INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`)
//!
//! [`BlobAssembler::stream_chunks`] iterates the manifest's chunks in
//! canonical order, fetches each from the [`super::chunk_store::ChunkStore`],
//! verifies BLAKE3(bytes) == claimed_chunk_digest, and ONLY THEN fans
//! the bytes out to the [`ChunkSink`]. On the first hash mismatch the
//! method returns [`AssemblerError::ChunkVerificationFailed`] without
//! pushing the offending chunk to the sink — the caller's sink only
//! ever sees verified bytes (no partial-chunk leakage).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site"
)]

use core::fmt;
use core::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (infallible lock).
// `AssemblerError::Backend("…mutex poisoned")` is unreachable here.
use parking_lot::{Mutex, MutexGuard};

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_tenant_path::TenantPrefix;
use thiserror::Error;
use uuid::Uuid;

use super::chunk_store::{ChunkKey, ChunkStore, ChunkStoreError};
use super::session::BoundChunk;
use super::split_splice::SpliceOutcome;
use super::types::{BlobDigest, ManifestDigest};
use crate::region::Region;

/// Composite primary key for the manifest record store: `(tenant_id,
/// blob_digest)`. Constructing one is the only legal way to address a
/// manifest through the [`BlobAssembler`] trait surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManifestKey {
    tenant_id: Uuid,
    blob_digest: BlobDigest,
}

impl ManifestKey {
    /// Build a fresh [`ManifestKey`].
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

/// Sealed manifest record. Returned by [`BlobAssembler::lookup_by_blob`] and
/// [`BlobAssembler::lookup_by_manifest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestRecord {
    /// Tenant scoping (defense-in-depth — present in the lookup
    /// signature already; mirrored on the row).
    pub tenant_id: Uuid,
    /// Source blob digest the manifest summarizes.
    pub blob_digest: BlobDigest,
    /// Manifest tree root.
    pub manifest_digest: ManifestDigest,
    /// Number of chunks bound by the manifest.
    pub chunk_count: u32,
    /// Total bytes summed across all bound chunks.
    pub total_size_bytes: u64,
    /// Region the manifest was minted under.
    pub region: Region,
    /// Tenant prefix materialized at INSERT time.
    pub tenant_prefix: TenantPrefix,
    /// Bound chunks in canonical order — copied from the session at
    /// build time so the splice path stays single-source-of-truth on
    /// the manifest record (the session row is allowed to be GC'd in
    /// S-06).
    pub chunks: Vec<BoundChunk>,
    /// Wall-clock instant the manifest was sealed.
    pub created_at_ms: u64,
}

/// Sink receiving verified chunk bytes during [`BlobAssembler::stream_chunks`].
///
/// Trait shape kept sync to match the in-memory fake; production wiring
/// adapts a `tonic::Streaming<ByteStream>` handle to this surface.
pub trait ChunkSink: Send {
    /// Receive a verified chunk's bytes. Implementations MAY block on
    /// downstream backpressure but MUST NOT reorder chunks (the
    /// assembler always pushes in canonical index order).
    ///
    /// # Errors
    ///
    /// Returns a sink-class error string. The assembler bubbles up
    /// to [`AssemblerError::Backend`].
    fn write_chunk(&mut self, bytes: Bytes) -> Result<(), String>;
}

/// Errors surfaced by [`BlobAssembler`] methods.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AssemblerError {
    /// Manifest digest absent under THIS tenant.
    #[error("manifest not found")]
    ManifestNotFound,
    /// Manifest references a chunk that is missing from the chunk
    /// store.
    #[error("chunk {chunk_index} missing from chunk store")]
    ChunkMissing {
        /// Offending index in canonical order.
        chunk_index: u32,
    },
    /// Per-chunk hash verify failed mid-stream — no bytes were emitted
    /// for `chunk_index`.
    #[error("chunk {chunk_index} verification failed")]
    ChunkVerificationFailed {
        /// Offending index in canonical order.
        chunk_index: u32,
    },
    /// Build path: `(chunk_index, ordering)` violation in the bound
    /// list. Programmer error — the session store sequencing
    /// guarantees ordering. Surfaced for symmetry only.
    #[error("chunk ordering violation at index {index}: {reason}")]
    ChunkOrderingViolation {
        /// Offending index.
        index: u32,
        /// Short canonical reason.
        reason: &'static str,
    },
    /// Backend / sink failure.
    #[error("assembler backend unavailable: {0}")]
    Backend(String),
}

impl From<ChunkStoreError> for AssemblerError {
    fn from(value: ChunkStoreError) -> Self {
        Self::Backend(format!("chunk store: {value}"))
    }
}

/// Trait surface for the manifest builder + streaming verifier.
pub trait BlobAssembler: Send + Sync {
    /// Build + seal a manifest from the canonical-ordered bound chunks.
    /// Returns the sealed [`ManifestRecord`] (with the tree root
    /// computed). Idempotent on `(tenant_id, blob_digest)` — second
    /// build for the same input returns the prior record verbatim.
    ///
    /// # Errors
    ///
    /// Backend / ordering violation.
    fn build_manifest<'a>(
        &'a self,
        key: ManifestKey,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        chunks: &'a [BoundChunk],
        created_at_ms: u64,
    ) -> impl Future<Output = Result<ManifestRecord, AssemblerError>> + Send + 'a;

    /// Lookup the manifest by `(tenant_id, blob_digest)`. Returns
    /// `None` for unknown rows.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn lookup_by_blob<'a>(
        &'a self,
        key: &'a ManifestKey,
    ) -> impl Future<Output = Result<Option<ManifestRecord>, AssemblerError>> + Send + 'a;

    /// Lookup the manifest by `(tenant_id, manifest_digest)`. Returns
    /// `None` for unknown rows. Mirrors the SpliceBlob path: the
    /// caller addresses by manifest digest, but the lookup is still
    /// tenant-leftmost (cross-tenant manifest digest access yields
    /// `None`).
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn lookup_by_manifest<'a>(
        &'a self,
        tenant_id: Uuid,
        manifest_digest: &'a ManifestDigest,
    ) -> impl Future<Output = Result<Option<ManifestRecord>, AssemblerError>> + Send + 'a;

    /// Stream the manifest's chunks, fan out to `sink` after per-chunk
    /// hash verify. Returns the canonical [`SpliceOutcome`].
    ///
    /// # Errors
    ///
    /// On the FIRST per-chunk mismatch returns
    /// [`AssemblerError::ChunkVerificationFailed`] WITHOUT pushing
    /// the offending bytes to `sink`.
    fn stream_chunks<'a>(
        &'a self,
        tenant_id: Uuid,
        tenant_prefix: &'a TenantPrefix,
        region: Region,
        record: &'a ManifestRecord,
        sink: &'a mut dyn ChunkSink,
    ) -> impl Future<Output = Result<SpliceOutcome, AssemblerError>> + Send + 'a;
}

/// Sink that collects bytes into an internal `Vec<Bytes>` — primarily
/// for property tests + handler unit tests asserting reassembled bytes.
#[derive(Default)]
pub struct CollectingSink {
    inner: Vec<Bytes>,
}

impl fmt::Debug for CollectingSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CollectingSink")
            .field("count", &self.inner.len())
            .finish_non_exhaustive()
    }
}

impl CollectingSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Drain the captured chunks.
    #[must_use]
    pub fn take(&mut self) -> Vec<Bytes> {
        core::mem::take(&mut self.inner)
    }

    /// Borrow the captured chunks without taking.
    #[must_use]
    pub fn snapshot(&self) -> &[Bytes] {
        &self.inner
    }
}

impl ChunkSink for CollectingSink {
    fn write_chunk(&mut self, bytes: Bytes) -> Result<(), String> {
        self.inner.push(bytes);
        Ok(())
    }
}

/// In-memory blob assembler fake. Builds manifest digests by hashing
/// the canonical concatenation of the bound chunk digests (a thin
/// stand-in for the real Merkle codec landing in WI-S05-005); the
/// streaming verifier reuses the chunk store's bytes via per-chunk
/// BLAKE3 verify.
///
/// Generic over the concrete [`ChunkStore`] impl because the
/// `ChunkStore` trait surface uses `impl Future` return types
/// (matching the `corelink-meta::MetaStore` canonical pattern) and is
/// therefore not dyn-compatible. Production wiring picks a single
/// concrete type at handler-construction time, so the generic shape is
/// not a footgun.
///
/// Per-instance — no global state (F-001 closure 2026-05-01).
pub struct InMemoryBlobAssembler<C: ChunkStore = super::chunk_store::InMemoryChunkStore> {
    chunks: Arc<C>,
    inner: Mutex<AssemblerInner>,
}

#[derive(Default)]
struct AssemblerInner {
    by_blob: HashMap<ManifestKey, ManifestRecord>,
    by_manifest: HashMap<(Uuid, ManifestDigest), ManifestKey>,
}

impl<C: ChunkStore> fmt::Debug for InMemoryBlobAssembler<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryBlobAssembler").finish_non_exhaustive()
    }
}

impl<C: ChunkStore> InMemoryBlobAssembler<C> {
    /// Construct a fresh in-memory assembler bound to the given chunk
    /// store. The chunk store reference is shared (`Arc`) with the
    /// SplitSpliceHandler so the splice path can re-fetch chunks
    /// without an extra wiring step.
    #[must_use]
    pub fn new(chunks: Arc<C>) -> Self {
        Self {
            chunks,
            inner: Mutex::new(AssemblerInner::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, AssemblerInner>, AssemblerError> {
        // parking_lot lock is infallible; Err arm unreachable.
        Ok(self.inner.lock())
    }

    /// Compute the canonical manifest digest as BLAKE3 over the
    /// concatenation of `(chunk_index || chunk_digest_bytes)` per
    /// chunk in canonical order. The real codec (WI-S05-005) layers
    /// the proper RFC 6962-style domain separation on top; this is a
    /// faithful stand-in that yields a deterministic, content-defined
    /// root.
    fn compute_manifest_digest(chunks: &[BoundChunk]) -> ManifestDigest {
        let mut buf: Vec<u8> = Vec::with_capacity(chunks.len() * 36);
        for c in chunks {
            buf.extend_from_slice(&c.index.0.to_be_bytes());
            buf.extend_from_slice(c.digest.as_digest().to_hex().as_bytes());
        }
        ManifestDigest::from_digest(Digest::compute(&buf))
    }
}

impl<C: ChunkStore> BlobAssembler for InMemoryBlobAssembler<C> {
    fn build_manifest<'a>(
        &'a self,
        key: ManifestKey,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        chunks: &'a [BoundChunk],
        created_at_ms: u64,
    ) -> impl Future<Output = Result<ManifestRecord, AssemblerError>> + Send + 'a {
        async move {
            // Validate canonical ordering — defense-in-depth (the
            // session store already guarantees this).
            for (expected, c) in chunks.iter().enumerate() {
                let expected = u32::try_from(expected).map_err(|_| {
                    AssemblerError::Backend("chunk index overflow u32".to_string())
                })?;
                if c.index.0 != expected {
                    return Err(AssemblerError::ChunkOrderingViolation {
                        index: c.index.0,
                        reason: "out_of_order",
                    });
                }
            }

            let manifest_digest = Self::compute_manifest_digest(chunks);
            let chunk_count: u32 = chunks
                .len()
                .try_into()
                .map_err(|_| AssemblerError::Backend("chunk count overflow u32".to_string()))?;
            let total_size_bytes: u64 = chunks.iter().map(|c| c.size_bytes).sum();
            let record = ManifestRecord {
                tenant_id: key.tenant_id(),
                blob_digest: *key.blob_digest(),
                manifest_digest,
                chunk_count,
                total_size_bytes,
                region,
                tenant_prefix: *tenant_prefix,
                chunks: chunks.to_vec(),
                created_at_ms,
            };
            let mut g = self.lock()?;
            // Idempotent build — second call for the same key returns
            // the prior record verbatim.
            if let Some(existing) = g.by_blob.get(&key).cloned() {
                if existing.manifest_digest == manifest_digest {
                    return Ok(existing);
                }
                // Same blob_digest, different manifest digest → mismatch.
                // Realistically only happens if the canonical chunk list
                // diverges; surface as ordering violation.
                return Err(AssemblerError::ChunkOrderingViolation {
                    index: 0,
                    reason: "manifest_digest_mismatch",
                });
            }
            g.by_blob.insert(key, record.clone());
            g.by_manifest
                .insert((key.tenant_id(), manifest_digest), key);
            Ok(record)
        }
    }

    fn lookup_by_blob<'a>(
        &'a self,
        key: &'a ManifestKey,
    ) -> impl Future<Output = Result<Option<ManifestRecord>, AssemblerError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            Ok(g.by_blob.get(key).cloned())
        }
    }

    fn lookup_by_manifest<'a>(
        &'a self,
        tenant_id: Uuid,
        manifest_digest: &'a ManifestDigest,
    ) -> impl Future<Output = Result<Option<ManifestRecord>, AssemblerError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            let Some(key) = g.by_manifest.get(&(tenant_id, *manifest_digest)).copied() else {
                return Ok(None);
            };
            Ok(g.by_blob.get(&key).cloned())
        }
    }

    fn stream_chunks<'a>(
        &'a self,
        tenant_id: Uuid,
        _tenant_prefix: &'a TenantPrefix,
        _region: Region,
        record: &'a ManifestRecord,
        sink: &'a mut dyn ChunkSink,
    ) -> impl Future<Output = Result<SpliceOutcome, AssemblerError>> + Send + 'a {
        async move {
            let mut chunks_streamed: u32 = 0;
            let mut bytes_streamed: u64 = 0;
            for bound in &record.chunks {
                let chunk_key = ChunkKey::new(tenant_id, bound.digest);
                let r = self
                    .chunks
                    .lookup(chunk_key)
                    .await?
                    .ok_or(AssemblerError::ChunkMissing {
                        chunk_index: bound.index.0,
                    })?;
                // Per-chunk hash verify — this is the streaming
                // fail-fast invariant (`INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`).
                let recomputed = super::types::ChunkDigest::compute(&r.bytes);
                if recomputed != bound.digest {
                    return Err(AssemblerError::ChunkVerificationFailed {
                        chunk_index: bound.index.0,
                    });
                }
                // ONLY THEN fan-out to the sink. Sink-class errors
                // bubble up as Backend.
                let bytes = r.bytes.clone();
                let len_u64: u64 = bytes.len() as u64;
                sink.write_chunk(bytes).map_err(AssemblerError::Backend)?;
                chunks_streamed = chunks_streamed.saturating_add(1);
                bytes_streamed = bytes_streamed.saturating_add(len_u64);
            }
            Ok(SpliceOutcome {
                chunks_streamed,
                bytes_streamed,
            })
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
    use super::super::chunk_store::InMemoryChunkStore;
    use super::super::session::BoundChunk;
    use super::super::types::{ChunkDigest, ChunkIndex};
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    fn bd(seed: &[u8]) -> BlobDigest {
        BlobDigest::new(Digest::compute(seed), seed.len() as u64)
    }

    async fn upsert_chunks(store: &Arc<InMemoryChunkStore>, tenant: Uuid, blobs: &[&[u8]]) -> Vec<BoundChunk> {
        let prefix = fixed_prefix(tenant);
        let mut out = Vec::new();
        for (i, b) in blobs.iter().enumerate() {
            let cd = ChunkDigest::compute(b);
            store
                .upsert(
                    ChunkKey::new(tenant, cd),
                    Region::Wnam,
                    &prefix,
                    Bytes::from(b.to_vec()),
                )
                .await
                .unwrap();
            out.push(BoundChunk {
                index: ChunkIndex(i as u32),
                digest: cd,
                size_bytes: b.len() as u64,
            });
        }
        out
    }

    #[tokio::test]
    async fn build_manifest_idempotent() {
        let chunks_store = Arc::new(InMemoryChunkStore::new());
        let assembler = InMemoryBlobAssembler::new(Arc::clone(&chunks_store));
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        let bound = upsert_chunks(&chunks_store, tenant, &[b"a", b"b"]).await;
        let key = ManifestKey::new(tenant, bd(b"blob"));
        let r1 = assembler
            .build_manifest(key, Region::Wnam, &prefix, &bound, 1)
            .await
            .unwrap();
        let r2 = assembler
            .build_manifest(key, Region::Wnam, &prefix, &bound, 2)
            .await
            .unwrap();
        assert_eq!(r1.manifest_digest, r2.manifest_digest);
        assert_eq!(r1.chunk_count, 2);
        assert_eq!(r1.total_size_bytes, 2);
    }

    #[tokio::test]
    async fn lookup_by_manifest_cross_tenant_masks_to_none() {
        let chunks_store = Arc::new(InMemoryChunkStore::new());
        let assembler = InMemoryBlobAssembler::new(Arc::clone(&chunks_store));
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let prefix_a = fixed_prefix(tenant_a);
        let bound = upsert_chunks(&chunks_store, tenant_a, &[b"a"]).await;
        let key = ManifestKey::new(tenant_a, bd(b"blob"));
        let r = assembler
            .build_manifest(key, Region::Wnam, &prefix_a, &bound, 1)
            .await
            .unwrap();
        assert!(assembler
            .lookup_by_manifest(tenant_b, &r.manifest_digest)
            .await
            .unwrap()
            .is_none());
        assert!(assembler
            .lookup_by_manifest(tenant_a, &r.manifest_digest)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn stream_chunks_verified_round_trip() {
        let chunks_store = Arc::new(InMemoryChunkStore::new());
        let assembler = InMemoryBlobAssembler::new(Arc::clone(&chunks_store));
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        let bound = upsert_chunks(&chunks_store, tenant, &[b"AAA", b"BBB"]).await;
        let key = ManifestKey::new(tenant, bd(b"blob"));
        let r = assembler
            .build_manifest(key, Region::Wnam, &prefix, &bound, 1)
            .await
            .unwrap();
        let mut sink = CollectingSink::new();
        let outcome = assembler
            .stream_chunks(tenant, &prefix, Region::Wnam, &r, &mut sink)
            .await
            .unwrap();
        assert_eq!(outcome.chunks_streamed, 2);
        assert_eq!(outcome.bytes_streamed, 6);
        let collected: Vec<u8> = sink.take().into_iter().flatten().collect();
        assert_eq!(&collected, b"AAABBB");
    }

    #[tokio::test]
    async fn stream_chunks_tamper_aborts_before_emit() {
        let chunks_store = Arc::new(InMemoryChunkStore::new());
        let assembler = InMemoryBlobAssembler::new(Arc::clone(&chunks_store));
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        let bound = upsert_chunks(&chunks_store, tenant, &[b"AAA", b"BBB"]).await;
        let key = ManifestKey::new(tenant, bd(b"blob"));
        let r = assembler
            .build_manifest(key, Region::Wnam, &prefix, &bound, 1)
            .await
            .unwrap();
        // Tamper chunk index 0.
        let key0 = ChunkKey::new(tenant, bound[0].digest);
        assert!(chunks_store.tamper_for_test(key0));
        let mut sink = CollectingSink::new();
        let err = assembler
            .stream_chunks(tenant, &prefix, Region::Wnam, &r, &mut sink)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AssemblerError::ChunkVerificationFailed { chunk_index: 0 }
        ));
        assert!(
            sink.take().is_empty(),
            "no unverified bytes may reach the sink"
        );
    }

    #[tokio::test]
    async fn stream_chunks_missing_returns_chunk_missing() {
        let chunks_store = Arc::new(InMemoryChunkStore::new());
        let assembler = InMemoryBlobAssembler::new(Arc::clone(&chunks_store));
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        // Build manifest binding chunks that we DON'T put into the store.
        let bound = vec![
            BoundChunk {
                index: ChunkIndex(0),
                digest: ChunkDigest::compute(b"ghost"),
                size_bytes: 5,
            },
        ];
        let key = ManifestKey::new(tenant, bd(b"blob"));
        let r = assembler
            .build_manifest(key, Region::Wnam, &prefix, &bound, 1)
            .await
            .unwrap();
        let mut sink = CollectingSink::new();
        let err = assembler
            .stream_chunks(tenant, &prefix, Region::Wnam, &r, &mut sink)
            .await
            .unwrap_err();
        assert!(matches!(err, AssemblerError::ChunkMissing { chunk_index: 0 }));
    }
}
