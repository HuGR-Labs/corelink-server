//! Pure-logic SplitBlob/SpliceBlob handler (WI-S05-001 §1, §6.1).
//!
//! [`SplitSpliceHandlerImpl`] is the canonical handler that orchestrates
//! the multipart-upload SplitBlob flow (init session → append chunks →
//! finalize manifest, OR abort) and the SpliceBlob fan-out flow
//! (manifest lookup → per-chunk verified reassembly streamed to the
//! caller). The trait surface ([`SplitSpliceHandler`]) is the
//! integration seam consumed by the gRPC tonic + axum REST wrappers
//! (deferred to the WI-S05-006 conformance suite landing — same
//! trait-abstraction-defer pattern that `reapi::ac` follows).
//!
//! ## 5-Layer Defense (auth_model.md §8.1 + ADR-0035)
//!
//! Every handler call enforces:
//!
//! 1. **Layer 1 — auth verify** is upstream (`AuthLayer`); the handler
//!    receives an [`AuthCtx`] reference whose construction was gated by
//!    the verifier.
//! 2. **Layer 2 — D1/session enforcement**: the
//!    [`super::session::SessionStore`] surface is
//!    `(tenant_id, session_id)`-keyed via [`SessionKey::new`]; the
//!    [`super::chunk_store::ChunkStore`] surface is
//!    `(tenant_id, chunk_digest)`-keyed via
//!    [`super::chunk_store::ChunkKey::new`]; the
//!    [`super::assembler::BlobAssembler`] surface is
//!    `(tenant_id, manifest_digest)`-keyed via
//!    [`super::assembler::ManifestKey::new`]. Cross-tenant lookups are
//!    structurally unreachable.
//! 3. **Layer 3 — scope check**: `auth_ctx.has_scope(SCOPE_CACHE_W)` for
//!    SplitBlob (init/append/finalize/abort); `SCOPE_CACHE_R` for
//!    SpliceBlob.
//! 4. **Layer 4 — HMAC tenant prefix**: every persisted artefact
//!    (chunk, manifest envelope, session row) is keyed under the
//!    materialized [`TenantPrefix`] read off [`AuthCtx::tenant_prefix`]
//!    so the storage seam never sees a raw `tenant_id` plaintext (per
//!    ADR-0035 H-3).
//! 5. **Layer 5 — audit emit**: every terminal flow path emits a typed
//!    record via [`super::audit::AuditSink`].
//!
//! The 5-layer enforcement is structural — the handler refuses to
//! compile against a key built without an `AuthCtx`-derived
//! `(tenant_id, …)` pair; the session/chunk/assembler trait surfaces
//! refuse to expose any cross-tenant probing API.
//!
//! ## Idempotency contract (`INV-MULTIPART-IDEMPOTENT`)
//!
//! - Re-`init_split` for a `(tenant_id, blob_digest)` whose session is
//!   live returns the existing [`SessionId`] (echo).
//! - Re-`init_split` for a `(tenant_id, blob_digest)` whose finalize
//!   already produced a manifest returns
//!   [`SplitOutcome::AlreadyChunked`] echoing the existing
//!   [`ManifestDigest`].
//! - `append_chunk` is idempotent on `(session_id, chunk_index)` — same
//!   `chunk_index` re-submitted with the **same** chunk bytes is a
//!   no-op (refcount unchanged); a re-submit with **different** bytes
//!   is rejected with [`SplitError::ChunkOrderingViolation`] preserving
//!   `INV-CAS-IMMUTABILITY`.
//! - `finalize_split` is idempotent on `session_id` — second call
//!   returns the cached [`ManifestDigest`] without re-building.
//! - `abort_split` on a finalized session is a `SessionAlreadyFinalized`
//!   (no destruction of finalized state — preserves
//!   `INV-MULTIPART-FINALIZE-IRREVOCABLE`).
//!
//! ## Streaming SpliceBlob fail-fast invariant
//!
//! [`SplitSpliceHandler::splice_blob`] streams chunks back to the
//! caller in canonical order via the [`super::assembler::BlobAssembler`]
//! trait. Per `INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`, the assembler
//! cancels the stream on the FIRST per-chunk hash mismatch — no
//! unverified bytes ever reach the caller. The handler bubbles the
//! cancellation up as [`SpliceError::ChunkVerificationFailed`] with
//! the offending `chunk_index`.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker reapi::ac canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use bytes::Bytes;
use corelink_pat::{SCOPE_CACHE_R, SCOPE_CACHE_W};
use thiserror::Error;

use super::assembler::{AssemblerError, BlobAssembler, ChunkSink, ManifestKey};
use super::audit::{AuditSink, AuditSinkError, BlobAuditRecord, BlobEventType};
use super::chunk_store::{ChunkKey, ChunkStore, ChunkStoreError};
use super::session::{
    SessionFinalize, SessionInit, SessionKey, SessionState, SessionStore, SessionStoreError,
};
use super::types::{
    BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId, MAX_CHUNKS_PER_BLOB,
};
use crate::middleware::auth_ctx::AuthCtx;
use crate::Region;

/// Maximum bytes per chunk submission. Mirrors the chunker default
/// (2 MiB) per ADR-0022 / sprint contract §5.1; oversize submissions
/// are rejected to keep the per-request stack budget bounded.
pub const MAX_CHUNK_BYTES: usize = 2 * 1024 * 1024;

/// Outcome of [`SplitSpliceHandler::init_split`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitSplitOutcome {
    /// Fresh session created. The handler emits `blob.split.start`.
    Started {
        /// Server-minted session id (UUIDv7 in production wiring).
        session_id: SessionId,
    },
    /// Session for `(tenant_id, blob_digest)` already exists in the
    /// `Live` state — handler echoes the existing id.
    LiveSessionEcho {
        /// Session id of the previously-started session.
        session_id: SessionId,
    },
    /// Blob is already chunked + finalized — handler echoes the
    /// existing manifest digest without re-running the pipeline.
    AlreadyChunked {
        /// Manifest digest of the cached finalize.
        manifest_digest: ManifestDigest,
        /// Number of chunks bound to the manifest.
        chunk_count: u32,
    },
}

/// Outcome of a successful [`SplitSpliceHandler::finalize_split`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizeSplitOutcome {
    /// Manifest digest produced by the assembler.
    pub manifest_digest: ManifestDigest,
    /// Number of chunks bound to the manifest.
    pub chunk_count: u32,
    /// Whether this was a fresh finalize or an idempotent echo of a
    /// prior finalize on the same session.
    pub idempotent: bool,
}

/// Outcome of [`SplitSpliceHandler::splice_blob`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpliceOutcome {
    /// Number of chunks streamed (matches manifest's `chunk_count`).
    pub chunks_streamed: u32,
    /// Total bytes streamed (sum of every chunk's size).
    pub bytes_streamed: u64,
}

/// Errors surfaced by the SplitBlob flow methods.
///
/// 9-variant taxonomy aligned with WI §1 + §23 error mapping.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SplitError {
    /// `(tenant_id, session_id)` row absent. Maps to 404 +
    /// `COR_MULTIPART_SESSION_NOT_FOUND`.
    #[error("split session not found")]
    SessionNotFound,
    /// Session is already finalized (cannot append/abort). Maps to 409 +
    /// `COR_MULTIPART_SESSION_FINALIZED`.
    #[error("split session already finalized")]
    SessionAlreadyFinalized,
    /// Session is aborted (cannot append/finalize). Maps to 410 +
    /// `COR_MULTIPART_SESSION_ABORTED`.
    #[error("split session aborted")]
    SessionAborted,
    /// `chunk_index` order violation: gap or duplicate-with-different-
    /// bytes. Maps to 422 + `COR_MULTIPART_CHUNK_ORDERING`.
    #[error("chunk ordering violation at index {index}: {reason}")]
    ChunkOrderingViolation {
        /// Offending index.
        index: u32,
        /// Short canonical reason (`"out_of_order"` / `"gap"` /
        /// `"bytes_mismatch"`).
        reason: &'static str,
    },
    /// Chunk bytes exceed [`MAX_CHUNK_BYTES`]. Maps to 413 +
    /// `COR_MULTIPART_CHUNK_TOO_LARGE`.
    #[error("chunk exceeds max size: {size} > {max}")]
    ChunkTooLarge {
        /// Submitted chunk size.
        size: usize,
        /// Configured max.
        max: usize,
    },
    /// Manifest's chunk count would exceed [`MAX_CHUNKS_PER_BLOB`].
    /// Maps to 413 + `COR_MULTIPART_BLOB_TOO_LARGE`.
    #[error("blob exceeds max chunks: {count} > {max}")]
    BlobTooLarge {
        /// Submitted chunk count.
        count: u32,
        /// Configured max.
        max: u32,
    },
    /// `auth_ctx.scopes()` lacks the required bit. Maps to 403 +
    /// `COR_AUTH_SCOPE_INSUFFICIENT`.
    #[error("scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits.
        required: u64,
    },
    /// Region pinning mismatch. Maps to 500 + `COR_INTERNAL`.
    #[error("handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable (D1 / R2 / KV). Maps to 503 +
    /// `COR_MULTIPART_BACKEND_UNAVAILABLE`.
    #[error("multipart backend unavailable: {0}")]
    BackendUnavailable(String),
}

impl SplitError {
    /// Stable canonical error code (`COR_MULTIPART_*` per WI §23).
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::SessionNotFound => "COR_MULTIPART_SESSION_NOT_FOUND",
            Self::SessionAlreadyFinalized => "COR_MULTIPART_SESSION_FINALIZED",
            Self::SessionAborted => "COR_MULTIPART_SESSION_ABORTED",
            Self::ChunkOrderingViolation { .. } => "COR_MULTIPART_CHUNK_ORDERING",
            Self::ChunkTooLarge { .. } => "COR_MULTIPART_CHUNK_TOO_LARGE",
            Self::BlobTooLarge { .. } => "COR_MULTIPART_BLOB_TOO_LARGE",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_MULTIPART_BACKEND_UNAVAILABLE",
        }
    }

    /// Short canonical audit reason — fed into
    /// [`BlobAuditRecord::reason`] so the chain consumer can pivot on
    /// the failure mode without parsing the message body.
    #[must_use]
    pub const fn audit_reason(&self) -> &'static str {
        match self {
            Self::SessionNotFound => "session_not_found",
            Self::SessionAlreadyFinalized => "session_already_finalized",
            Self::SessionAborted => "session_aborted",
            Self::ChunkOrderingViolation { reason, .. } => reason,
            Self::ChunkTooLarge { .. } => "chunk_too_large",
            Self::BlobTooLarge { .. } => "blob_too_large",
            Self::ScopeInsufficient { .. } => "scope_insufficient",
            Self::RegionMismatch { .. } => "region_mismatch",
            Self::BackendUnavailable(_) => "backend_unavailable",
        }
    }
}

impl From<SessionStoreError> for SplitError {
    fn from(value: SessionStoreError) -> Self {
        match value {
            SessionStoreError::NotFound => Self::SessionNotFound,
            SessionStoreError::AlreadyFinalized => Self::SessionAlreadyFinalized,
            SessionStoreError::Aborted => Self::SessionAborted,
            SessionStoreError::OrderingViolation { index, reason } => {
                Self::ChunkOrderingViolation { index, reason }
            }
            SessionStoreError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<ChunkStoreError> for SplitError {
    fn from(value: ChunkStoreError) -> Self {
        Self::BackendUnavailable(format!("chunk store: {value}"))
    }
}

impl From<AssemblerError> for SplitError {
    fn from(value: AssemblerError) -> Self {
        match value {
            AssemblerError::ChunkOrderingViolation { index, reason } => {
                Self::ChunkOrderingViolation { index, reason }
            }
            other => Self::BackendUnavailable(format!("assembler: {other}")),
        }
    }
}

impl From<AuditSinkError> for SplitError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("audit sink: {value}"))
    }
}

/// Errors surfaced by [`SplitSpliceHandler::splice_blob`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpliceError {
    /// Manifest digest absent. Maps to 404 +
    /// `COR_MULTIPART_MANIFEST_NOT_FOUND`.
    #[error("manifest not found")]
    ManifestNotFound,
    /// Per-chunk hash verify failed mid-stream. Stream is cancelled
    /// before any further bytes reach the sink. Maps to 422 +
    /// `COR_MULTIPART_CHUNK_VERIFY_FAILED` (CRITICAL audit emit —
    /// tampering signal).
    #[error("chunk {chunk_index} verification failed")]
    ChunkVerificationFailed {
        /// Offending chunk index in canonical order.
        chunk_index: u32,
    },
    /// Manifest references a chunk that is missing from the chunk
    /// store. Maps to 422 + `COR_MULTIPART_CHUNK_MISSING`.
    #[error("chunk {chunk_index} missing from chunk store")]
    ChunkMissing {
        /// Offending chunk index in canonical order.
        chunk_index: u32,
    },
    /// `auth_ctx.scopes()` lacks `SCOPE_CACHE_R`. Maps to 403.
    #[error("scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits.
        required: u64,
    },
    /// Region pinning mismatch. Maps to 500.
    #[error("handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable. Maps to 503.
    #[error("splice backend unavailable: {0}")]
    BackendUnavailable(String),
}

impl SpliceError {
    /// Stable canonical error code (`COR_MULTIPART_*` per WI §23).
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::ManifestNotFound => "COR_MULTIPART_MANIFEST_NOT_FOUND",
            Self::ChunkVerificationFailed { .. } => "COR_MULTIPART_CHUNK_VERIFY_FAILED",
            Self::ChunkMissing { .. } => "COR_MULTIPART_CHUNK_MISSING",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_MULTIPART_BACKEND_UNAVAILABLE",
        }
    }
}

impl From<AssemblerError> for SpliceError {
    fn from(value: AssemblerError) -> Self {
        match value {
            AssemblerError::ManifestNotFound => Self::ManifestNotFound,
            AssemblerError::ChunkMissing { chunk_index } => Self::ChunkMissing { chunk_index },
            AssemblerError::ChunkVerificationFailed { chunk_index } => {
                Self::ChunkVerificationFailed { chunk_index }
            }
            AssemblerError::ChunkOrderingViolation { .. } => Self::BackendUnavailable(
                "assembler ordering violation on splice path (programmer error)".to_string(),
            ),
            AssemblerError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<ChunkStoreError> for SpliceError {
    fn from(value: ChunkStoreError) -> Self {
        Self::BackendUnavailable(format!("chunk store: {value}"))
    }
}

impl From<AuditSinkError> for SpliceError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("audit sink: {value}"))
    }
}

/// Canonical SplitBlob/SpliceBlob handler trait. The gRPC tonic service
/// wrapper + axum REST router (deferred to WI-S05-006 conformance
/// suite) both consume this trait.
pub trait SplitSpliceHandler: Send + Sync {
    /// SplitBlob step [0]: open a multipart upload session for
    /// `(tenant_id, blob_digest)`. Idempotent — see
    /// [`InitSplitOutcome`] variants.
    ///
    /// # Errors
    ///
    /// See [`SplitError`].
    fn init_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        blob_digest: &'a BlobDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<InitSplitOutcome, SplitError>> + Send + 'a;

    /// SplitBlob step [k]: append a chunk to the open session at the
    /// canonical `chunk_index` slot. Idempotent on
    /// `(session_id, chunk_index, chunk_bytes)`.
    ///
    /// # Errors
    ///
    /// See [`SplitError`]. In particular [`SplitError::ChunkOrderingViolation`]
    /// captures the three canonical anomalies (gap, out-of-order,
    /// bytes-mismatch on idempotent retry).
    fn append_chunk<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        chunk_index: ChunkIndex,
        chunk_bytes: Bytes,
        request_id: &'a str,
    ) -> impl Future<Output = Result<ChunkDigest, SplitError>> + Send + 'a;

    /// SplitBlob step [N]: finalize a session into a sealed manifest.
    /// Idempotent on `session_id` — second call returns the cached
    /// digest.
    ///
    /// # Errors
    ///
    /// See [`SplitError`].
    fn finalize_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        request_id: &'a str,
    ) -> impl Future<Output = Result<FinalizeSplitOutcome, SplitError>> + Send + 'a;

    /// SplitBlob escape: abort an in-progress session. Idempotent on
    /// already-aborted sessions (echoes); a finalized session yields
    /// [`SplitError::SessionAlreadyFinalized`] (preserves
    /// `INV-MULTIPART-FINALIZE-IRREVOCABLE`).
    ///
    /// # Errors
    ///
    /// See [`SplitError`].
    fn abort_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        request_id: &'a str,
    ) -> impl Future<Output = Result<(), SplitError>> + Send + 'a;

    /// SpliceBlob: reassemble the manifest's chunks back into the
    /// original blob byte stream, fan-out per-chunk verified to `sink`
    /// in canonical order. Returns once every chunk has been verified +
    /// emitted; aborts at the FIRST per-chunk hash mismatch (no
    /// unverified bytes ever reach the sink).
    ///
    /// # Errors
    ///
    /// See [`SpliceError`].
    fn splice_blob<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        manifest_digest: &'a ManifestDigest,
        sink: &'a mut dyn ChunkSink,
        request_id: &'a str,
    ) -> impl Future<Output = Result<SpliceOutcome, SpliceError>> + Send + 'a;
}

/// Builder for [`SplitSpliceHandlerImpl`].
#[allow(clippy::module_name_repetitions, missing_debug_implementations)]
pub struct SplitSpliceHandlerBuilder<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    /// Region the handler is pinned to.
    pub region: Region,
    /// Multipart session row store.
    pub sessions: Arc<S>,
    /// Chunk content-addressable store.
    pub chunks: Arc<C>,
    /// Manifest builder + verifier (delegate WI-S05-005 in production).
    pub assembler: Arc<B>,
    /// Audit sink.
    pub audit: Arc<A>,
    /// Wall-clock seam for `created_at_ms` capture.
    pub clock: Arc<dyn Clock>,
}

/// Wall-clock seam — production wires [`SystemClock`]; tests pin a
/// fixed instant via [`FakeClock`].
pub trait Clock: Send + Sync + fmt::Debug {
    /// Current Unix epoch ms.
    fn now_ms(&self) -> u64;
}

/// `SystemTime`-backed clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

/// Test-only deterministic clock.
#[derive(Debug)]
pub struct FakeClock {
    now: std::sync::atomic::AtomicU64,
}

impl FakeClock {
    /// Construct a fake clock pinned at `start_ms`.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            now: std::sync::atomic::AtomicU64::new(start_ms),
        }
    }

    /// Move the clock forward by `delta_ms`.
    pub fn advance_ms(&self, delta_ms: u64) {
        self.now
            .fetch_add(delta_ms, std::sync::atomic::Ordering::AcqRel);
    }

    /// Pin the clock to an absolute value.
    pub fn set_ms(&self, abs_ms: u64) {
        self.now
            .store(abs_ms, std::sync::atomic::Ordering::Release);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Canonical pure-logic SplitBlob/SpliceBlob handler.
///
/// Holds `Arc`-shared dependencies so a single handler instance can be
/// cloned across spawned gRPC + REST tasks. All state-bearing
/// dependencies are interior-mutable — the session store, chunk store,
/// assembler, and audit sink each hold their own locks; the handler
/// itself has no per-request mutable state.
pub struct SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    region: Region,
    sessions: Arc<S>,
    chunks: Arc<C>,
    assembler: Arc<B>,
    audit: Arc<A>,
    clock: Arc<dyn Clock>,
}

impl<S, C, B, A> fmt::Debug for SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SplitSpliceHandlerImpl")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, A> SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    /// Construct a handler from the canonical builder.
    #[must_use]
    pub fn new(b: SplitSpliceHandlerBuilder<S, C, B, A>) -> Self {
        Self {
            region: b.region,
            sessions: b.sessions,
            chunks: b.chunks,
            assembler: b.assembler,
            audit: b.audit,
            clock: b.clock,
        }
    }

    /// Region this handler is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    fn check_region_split(&self, ctx: &AuthCtx) -> Result<(), SplitError> {
        if ctx.region() != self.region {
            return Err(SplitError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    fn check_region_splice(&self, ctx: &AuthCtx) -> Result<(), SpliceError> {
        if ctx.region() != self.region {
            return Err(SpliceError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    fn require_split_scope(ctx: &AuthCtx) -> Result<(), SplitError> {
        if ctx.has_scope(SCOPE_CACHE_W) {
            Ok(())
        } else {
            Err(SplitError::ScopeInsufficient {
                required: SCOPE_CACHE_W,
            })
        }
    }

    fn require_splice_scope(ctx: &AuthCtx) -> Result<(), SpliceError> {
        if ctx.has_scope(SCOPE_CACHE_R) {
            Ok(())
        } else {
            Err(SpliceError::ScopeInsufficient {
                required: SCOPE_CACHE_R,
            })
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "9-arg shape mirrors the canonical BlobAuditRecord field set + ctx for request-time clock + tenant_id derivation; further compaction would obscure the audit envelope contract — same waiver as `reapi::ac::handler::ActionCacheHandlerImpl::make_record`"
    )]
    fn make_record(
        &self,
        event_type: BlobEventType,
        ctx: &AuthCtx,
        request_id: &str,
        blob_digest: Option<BlobDigest>,
        manifest_digest: Option<ManifestDigest>,
        session_id: Option<SessionId>,
        chunk_index: Option<ChunkIndex>,
        reason: &'static str,
    ) -> BlobAuditRecord {
        BlobAuditRecord {
            event_type,
            tenant_id: ctx.tenant_id(),
            region: ctx.region(),
            blob_digest,
            manifest_digest,
            session_id,
            chunk_index,
            request_id: request_id.to_string(),
            reason,
            now_ms: self.clock.now_ms(),
        }
    }
}

impl<S, C, B, A> SplitSpliceHandler for SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    fn init_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        blob_digest: &'a BlobDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<InitSplitOutcome, SplitError>> + Send + 'a {
        async move { self.init_split_inner(ctx, blob_digest, request_id).await }
    }

    fn append_chunk<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        chunk_index: ChunkIndex,
        chunk_bytes: Bytes,
        request_id: &'a str,
    ) -> impl Future<Output = Result<ChunkDigest, SplitError>> + Send + 'a {
        async move {
            self.append_chunk_inner(ctx, session_id, chunk_index, chunk_bytes, request_id)
                .await
        }
    }

    fn finalize_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        request_id: &'a str,
    ) -> impl Future<Output = Result<FinalizeSplitOutcome, SplitError>> + Send + 'a {
        async move {
            self.finalize_split_inner(ctx, session_id, request_id)
                .await
        }
    }

    fn abort_split<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        session_id: SessionId,
        request_id: &'a str,
    ) -> impl Future<Output = Result<(), SplitError>> + Send + 'a {
        async move { self.abort_split_inner(ctx, session_id, request_id).await }
    }

    fn splice_blob<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        manifest_digest: &'a ManifestDigest,
        sink: &'a mut dyn ChunkSink,
        request_id: &'a str,
    ) -> impl Future<Output = Result<SpliceOutcome, SpliceError>> + Send + 'a {
        async move {
            self.splice_blob_inner(ctx, manifest_digest, sink, request_id)
                .await
        }
    }
}

impl<S, C, B, A> SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    async fn init_split_inner(
        &self,
        ctx: &AuthCtx,
        blob_digest: &BlobDigest,
        request_id: &str,
    ) -> Result<InitSplitOutcome, SplitError> {
        // Layer 1 — region + scope.
        self.check_region_split(ctx)?;
        Self::require_split_scope(ctx)?;

        // Idempotency: if a manifest is already cached on the
        // assembler for `(tenant_id, blob_digest)` we echo the existing
        // shape without minting a new session (INV-MULTIPART-IDEMPOTENT).
        let lookup_key = ManifestKey::new(ctx.tenant_id(), *blob_digest);
        if let Some(record) = self.assembler.lookup_by_blob(&lookup_key).await? {
            self.audit.emit(self.make_record(
                BlobEventType::SplitStart,
                ctx,
                request_id,
                Some(*blob_digest),
                Some(record.manifest_digest),
                None,
                None,
                "already_chunked",
            ))?;
            return Ok(InitSplitOutcome::AlreadyChunked {
                manifest_digest: record.manifest_digest,
                chunk_count: record.chunk_count,
            });
        }

        // Live-session echo: same `(tenant_id, blob_digest)` already
        // has an open session.
        if let Some(existing) = self
            .sessions
            .find_live_by_blob(ctx.tenant_id(), blob_digest)
            .await?
        {
            self.audit.emit(self.make_record(
                BlobEventType::SplitStart,
                ctx,
                request_id,
                Some(*blob_digest),
                None,
                Some(existing),
                None,
                "live_session_echo",
            ))?;
            return Ok(InitSplitOutcome::LiveSessionEcho {
                session_id: existing,
            });
        }

        // Fresh session. The session store mints the canonical
        // session_id (UUIDv7 in production wiring) so the handler does
        // not need a getrandom dependency at the pure-logic layer.
        let now_ms = self.clock.now_ms();
        let key = SessionKey::new(ctx.tenant_id(), *blob_digest);
        let session_id = self
            .sessions
            .open(SessionInit {
                key,
                region: self.region,
                tenant_prefix: *ctx.tenant_prefix(),
                created_at_ms: now_ms,
            })
            .await?;

        self.audit.emit(self.make_record(
            BlobEventType::SplitStart,
            ctx,
            request_id,
            Some(*blob_digest),
            None,
            Some(session_id),
            None,
            "started",
        ))?;
        Ok(InitSplitOutcome::Started { session_id })
    }

    async fn append_chunk_inner(
        &self,
        ctx: &AuthCtx,
        session_id: SessionId,
        chunk_index: ChunkIndex,
        chunk_bytes: Bytes,
        request_id: &str,
    ) -> Result<ChunkDigest, SplitError> {
        self.check_region_split(ctx)?;
        Self::require_split_scope(ctx)?;

        // Bound the per-chunk submission size — preserves the
        // per-request stack budget the streaming pipeline assumes.
        if chunk_bytes.len() > MAX_CHUNK_BYTES {
            self.audit.emit(self.make_record(
                BlobEventType::SplitChunkAppended,
                ctx,
                request_id,
                None,
                None,
                Some(session_id),
                Some(chunk_index),
                "chunk_too_large",
            ))?;
            return Err(SplitError::ChunkTooLarge {
                size: chunk_bytes.len(),
                max: MAX_CHUNK_BYTES,
            });
        }

        // Defense-in-depth — the trait surface accepts a session_id
        // not a `(tenant_id, session_id)` pair, but the store enforces
        // tenant-scoped lookup; we re-validate the session resolves
        // under THIS ctx's tenant_id before mutating any state.
        let session = self
            .sessions
            .lookup(ctx.tenant_id(), session_id)
            .await?
            .ok_or(SplitError::SessionNotFound)?;
        match session.state {
            SessionState::Live => {}
            SessionState::Finalized { .. } => {
                return Err(SplitError::SessionAlreadyFinalized);
            }
            SessionState::Aborted => {
                return Err(SplitError::SessionAborted);
            }
        }
        if session.key.tenant_id() != ctx.tenant_id() {
            return Err(SplitError::BackendUnavailable(
                "session row tenant_id mismatch ctx (defense-in-depth)".to_string(),
            ));
        }

        // Hash the chunk inline (BLAKE3 is the canonical CAS hash).
        let chunk_digest = ChunkDigest::compute(&chunk_bytes);

        // Append to the session FIRST — the session store is the
        // sequencing oracle (tracks `next_index`, rejects duplicates
        // with bytes-mismatch). On Ok, the session's `chunks` vector
        // grows; on Err, no state mutates.
        let bound_count = self
            .sessions
            .append_chunk(
                ctx.tenant_id(),
                session_id,
                chunk_index,
                chunk_digest,
                chunk_bytes.len() as u64,
            )
            .await?;
        if bound_count > MAX_CHUNKS_PER_BLOB {
            return Err(SplitError::BlobTooLarge {
                count: bound_count,
                max: MAX_CHUNKS_PER_BLOB,
            });
        }

        // Persist the chunk content (UPSERT — idempotent on
        // `(tenant_id, chunk_digest)` per CAP-CAS-010).
        let chunk_key = ChunkKey::new(ctx.tenant_id(), chunk_digest);
        self.chunks
            .upsert(
                chunk_key,
                self.region,
                ctx.tenant_prefix(),
                chunk_bytes.clone(),
            )
            .await?;

        self.audit.emit(self.make_record(
            BlobEventType::SplitChunkAppended,
            ctx,
            request_id,
            None,
            None,
            Some(session_id),
            Some(chunk_index),
            "appended",
        ))?;
        Ok(chunk_digest)
    }

    async fn finalize_split_inner(
        &self,
        ctx: &AuthCtx,
        session_id: SessionId,
        request_id: &str,
    ) -> Result<FinalizeSplitOutcome, SplitError> {
        self.check_region_split(ctx)?;
        Self::require_split_scope(ctx)?;

        // Idempotent finalize — second call returns the cached digest.
        if let Some(snap) = self
            .sessions
            .lookup(ctx.tenant_id(), session_id)
            .await?
        {
            if let SessionState::Finalized {
                manifest_digest,
                chunk_count,
            } = snap.state
            {
                self.audit.emit(self.make_record(
                    BlobEventType::SplitFinalized,
                    ctx,
                    request_id,
                    Some(*snap.key.blob_digest()),
                    Some(manifest_digest),
                    Some(session_id),
                    None,
                    "idempotent",
                ))?;
                return Ok(FinalizeSplitOutcome {
                    manifest_digest,
                    chunk_count,
                    idempotent: true,
                });
            }
        } else {
            return Err(SplitError::SessionNotFound);
        }

        // Pull the bound chunks list (canonical-ordered) from the
        // session — this is the assembler's only input on the build
        // path, so cross-tenant misrouting is structurally impossible
        // (the session store keeps the chunks vector keyed by
        // `(tenant_id, session_id)` PK).
        let bound_chunks = self
            .sessions
            .bound_chunks(ctx.tenant_id(), session_id)
            .await?;
        let blob_digest = self
            .sessions
            .lookup(ctx.tenant_id(), session_id)
            .await?
            .ok_or(SplitError::SessionNotFound)?
            .key
            .blob_digest()
            .to_owned();

        // Build the manifest. The assembler is the canonical Merkle
        // builder integration seam (delegates to WI-S05-005 in
        // production wiring).
        let manifest_key = ManifestKey::new(ctx.tenant_id(), blob_digest);
        let now_ms = self.clock.now_ms();
        let record = self
            .assembler
            .build_manifest(
                manifest_key,
                self.region,
                ctx.tenant_prefix(),
                &bound_chunks,
                now_ms,
            )
            .await?;

        // Mark the session as finalized (idempotent on the store side).
        self.sessions
            .finalize(SessionFinalize {
                tenant_id: ctx.tenant_id(),
                session_id,
                manifest_digest: record.manifest_digest,
                chunk_count: record.chunk_count,
                finalized_at_ms: now_ms,
            })
            .await?;

        self.audit.emit(self.make_record(
            BlobEventType::SplitFinalized,
            ctx,
            request_id,
            Some(blob_digest),
            Some(record.manifest_digest),
            Some(session_id),
            None,
            "finalized",
        ))?;
        Ok(FinalizeSplitOutcome {
            manifest_digest: record.manifest_digest,
            chunk_count: record.chunk_count,
            idempotent: false,
        })
    }

    async fn abort_split_inner(
        &self,
        ctx: &AuthCtx,
        session_id: SessionId,
        request_id: &str,
    ) -> Result<(), SplitError> {
        self.check_region_split(ctx)?;
        Self::require_split_scope(ctx)?;

        let snap = self
            .sessions
            .lookup(ctx.tenant_id(), session_id)
            .await?
            .ok_or(SplitError::SessionNotFound)?;
        match snap.state {
            SessionState::Live | SessionState::Aborted => {
                self.sessions
                    .abort(ctx.tenant_id(), session_id, self.clock.now_ms())
                    .await?;
                self.audit.emit(self.make_record(
                    BlobEventType::SplitAborted,
                    ctx,
                    request_id,
                    Some(*snap.key.blob_digest()),
                    None,
                    Some(session_id),
                    None,
                    match snap.state {
                        SessionState::Live => "aborted",
                        SessionState::Aborted => "idempotent",
                        SessionState::Finalized { .. } => "unreachable",
                    },
                ))?;
                Ok(())
            }
            SessionState::Finalized { .. } => Err(SplitError::SessionAlreadyFinalized),
        }
    }

    async fn splice_blob_inner(
        &self,
        ctx: &AuthCtx,
        manifest_digest: &ManifestDigest,
        sink: &mut dyn ChunkSink,
        request_id: &str,
    ) -> Result<SpliceOutcome, SpliceError> {
        self.check_region_splice(ctx)?;
        Self::require_splice_scope(ctx)?;

        // Manifest lookup is tenant-scoped via the assembler trait.
        let record = self
            .assembler
            .lookup_by_manifest(ctx.tenant_id(), manifest_digest)
            .await?
            .ok_or(SpliceError::ManifestNotFound)?;

        // Stream + verify per chunk. Cancellation on first per-chunk
        // mismatch is structurally enforced by the assembler — the
        // sink only receives bytes AFTER per-chunk hash verify
        // succeeds.
        let outcome = self
            .assembler
            .stream_chunks(
                ctx.tenant_id(),
                ctx.tenant_prefix(),
                self.region,
                &record,
                sink,
            )
            .await?;

        self.audit.emit(self.make_record(
            BlobEventType::SpliceOk,
            ctx,
            request_id,
            Some(record.blob_digest),
            Some(record.manifest_digest),
            None,
            None,
            "ok",
        ))?;
        Ok(outcome)
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
    use crate::middleware::auth_ctx::__test_helpers::make_auth_ctx;
    use crate::middleware::auth_ctx::{AuthMethod, PrincipalId};
    use crate::reapi::cas::assembler::InMemoryBlobAssembler;
    use crate::reapi::cas::audit::InMemoryAuditSink;
    use crate::reapi::cas::chunk_store::InMemoryChunkStore;
    use crate::reapi::cas::session::InMemorySessionStore;
    use corelink_hash::Digest;
    use corelink_pat::{PatEnv, PatId, PatScopes};
    use corelink_tenant_path::TenantDerivationKey;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_tdk() -> Arc<TenantDerivationKey> {
        Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
    }

    fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
        let pat_id = PatId(Uuid::nil());
        make_auth_ctx(
            PrincipalId(Uuid::nil()),
            tenant,
            region,
            scopes,
            AuthMethod::Pat {
                env: PatEnv::Pat,
                pat_id,
            },
            fixed_tdk(),
        )
    }

    #[allow(dead_code, reason = "session + assembler handles kept on Arc for handler-shared lifecycle even when individual tests do not borrow them directly")]
    struct Wiring {
        handler: SplitSpliceHandlerImpl<
            InMemorySessionStore,
            InMemoryChunkStore,
            InMemoryBlobAssembler,
            InMemoryAuditSink,
        >,
        sessions: Arc<InMemorySessionStore>,
        chunks: Arc<InMemoryChunkStore>,
        assembler: Arc<InMemoryBlobAssembler>,
        audit: Arc<InMemoryAuditSink>,
        clock: Arc<FakeClock>,
    }

    fn wire(region: Region) -> Wiring {
        let sessions = Arc::new(InMemorySessionStore::new());
        let chunks = Arc::new(InMemoryChunkStore::new());
        let assembler = Arc::new(InMemoryBlobAssembler::new(Arc::clone(&chunks)));
        let audit = Arc::new(InMemoryAuditSink::new());
        let clock = Arc::new(FakeClock::new(1_000_000));
        let handler = SplitSpliceHandlerImpl::new(SplitSpliceHandlerBuilder {
            region,
            sessions: Arc::clone(&sessions),
            chunks: Arc::clone(&chunks),
            assembler: Arc::clone(&assembler),
            audit: Arc::clone(&audit),
            clock: Arc::clone(&clock) as Arc<dyn Clock>,
        });
        Wiring {
            handler,
            sessions,
            chunks,
            assembler,
            audit,
            clock,
        }
    }

    fn blob_digest(seed: &[u8]) -> BlobDigest {
        BlobDigest::new(Digest::compute(seed), seed.len() as u64)
    }

    #[tokio::test]
    async fn split_init_append_finalize_happy_path() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let bd = blob_digest(b"blob-1");
        let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
        let session_id = match init {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("expected Started, got {other:?}"),
        };
        // Append two chunks.
        let c0 = Bytes::from_static(b"chunk-zero-bytes");
        let c1 = Bytes::from_static(b"chunk-one-bytes");
        let _d0 = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), c0.clone(), "req-c0")
            .await
            .unwrap();
        let _d1 = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(1), c1.clone(), "req-c1")
            .await
            .unwrap();
        // Finalize.
        let fin = w
            .handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
        assert_eq!(fin.chunk_count, 2);
        assert!(!fin.idempotent);
        // Idempotent finalize echoes.
        let fin2 = w
            .handler
            .finalize_split(&ctx, session_id, "req-fin-2")
            .await
            .unwrap();
        assert_eq!(fin2.manifest_digest, fin.manifest_digest);
        assert!(fin2.idempotent);
        // Audit captured.
        assert_eq!(w.audit.snapshot_of(BlobEventType::SplitStart).len(), 1);
        assert_eq!(
            w.audit.snapshot_of(BlobEventType::SplitChunkAppended).len(),
            2
        );
        assert_eq!(w.audit.snapshot_of(BlobEventType::SplitFinalized).len(), 2);
    }

    #[tokio::test]
    async fn split_init_idempotent_after_finalize() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let bd = blob_digest(b"blob-2");
        let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
        let session_id = match init {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("expected Started, got {other:?}"),
        };
        let c0 = Bytes::from_static(b"only-chunk");
        w.handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), c0, "req-c0")
            .await
            .unwrap();
        let fin = w
            .handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
        // Re-init for the same blob_digest yields AlreadyChunked.
        let init2 = w.handler.init_split(&ctx, &bd, "req-init-2").await.unwrap();
        match init2 {
            InitSplitOutcome::AlreadyChunked {
                manifest_digest,
                chunk_count,
            } => {
                assert_eq!(manifest_digest, fin.manifest_digest);
                assert_eq!(chunk_count, 1);
            }
            other => panic!("expected AlreadyChunked, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn split_init_live_session_echo() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-live");
        let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
        let session_id = match init {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("expected Started, got {other:?}"),
        };
        let init2 = w.handler.init_split(&ctx, &bd, "req-init-2").await.unwrap();
        match init2 {
            InitSplitOutcome::LiveSessionEcho { session_id: id } => {
                assert_eq!(id, session_id);
            }
            other => panic!("expected LiveSessionEcho, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cross_tenant_session_lookup_returns_not_found() {
        let w = wire(Region::Wnam);
        let tenant_a = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let tenant_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
        let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-xt");
        let init = w
            .handler
            .init_split(&ctx_a, &bd, "req-init-a")
            .await
            .unwrap();
        let session_id = match init {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("got {other:?}"),
        };
        // Tenant B tries to append to A's session.
        let err = w
            .handler
            .append_chunk(
                &ctx_b,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"x"),
                "req-c0-b",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SplitError::SessionNotFound));
        assert_eq!(err.cor_code(), "COR_MULTIPART_SESSION_NOT_FOUND");
    }

    #[tokio::test]
    async fn append_after_finalize_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-final");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        w.handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"a"),
                "req",
            )
            .await
            .unwrap();
        w.handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
        let err = w
            .handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(1),
                Bytes::from_static(b"b"),
                "req-late",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SplitError::SessionAlreadyFinalized));
    }

    #[tokio::test]
    async fn abort_then_append_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-abort");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        w.handler
            .abort_split(&ctx, session_id, "req-abort")
            .await
            .unwrap();
        // Idempotent abort.
        w.handler
            .abort_split(&ctx, session_id, "req-abort-2")
            .await
            .unwrap();
        // Append rejected.
        let err = w
            .handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"x"),
                "req-late",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SplitError::SessionAborted));
        assert_eq!(err.cor_code(), "COR_MULTIPART_SESSION_ABORTED");
    }

    #[tokio::test]
    async fn finalize_then_abort_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-fa");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        w.handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"x"),
                "req",
            )
            .await
            .unwrap();
        w.handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
        let err = w
            .handler
            .abort_split(&ctx, session_id, "req-abort")
            .await
            .unwrap_err();
        assert!(matches!(err, SplitError::SessionAlreadyFinalized));
    }

    #[tokio::test]
    async fn append_chunk_ordering_violation_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-ord");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        // Out-of-order: try to append index 1 before 0.
        let err = w
            .handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(1),
                Bytes::from_static(b"x"),
                "req",
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SplitError::ChunkOrderingViolation { index: 1, .. }
        ));
    }

    #[tokio::test]
    async fn append_chunk_idempotent_same_bytes_ok() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-idem");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        let bytes = Bytes::from_static(b"the-same-bytes");
        let d0 = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), bytes.clone(), "req-1")
            .await
            .unwrap();
        let d0b = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), bytes.clone(), "req-2")
            .await
            .unwrap();
        assert_eq!(d0, d0b);
    }

    #[tokio::test]
    async fn append_chunk_idempotent_different_bytes_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-bytes-mis");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        w.handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"original"),
                "req-1",
            )
            .await
            .unwrap();
        let err = w
            .handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"DIFFERENT"),
                "req-2",
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SplitError::ChunkOrderingViolation {
                reason: "bytes_mismatch",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn append_chunk_too_large_rejected() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-big");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        let huge = Bytes::from(vec![0u8; MAX_CHUNK_BYTES + 1]);
        let err = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), huge, "req")
            .await
            .unwrap_err();
        assert!(matches!(err, SplitError::ChunkTooLarge { .. }));
        assert_eq!(err.cor_code(), "COR_MULTIPART_CHUNK_TOO_LARGE");
    }

    #[tokio::test]
    async fn missing_scope_rejected_403_split() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::empty());
        let bd = blob_digest(b"blob-noscope");
        let err = w.handler.init_split(&ctx, &bd, "req").await.unwrap_err();
        assert!(matches!(
            err,
            SplitError::ScopeInsufficient {
                required: SCOPE_CACHE_W
            }
        ));
        assert_eq!(err.cor_code(), "COR_AUTH_SCOPE_INSUFFICIENT");
    }

    #[tokio::test]
    async fn region_mismatch_returns_internal_split() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Weur, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-region");
        let err = w.handler.init_split(&ctx, &bd, "req").await.unwrap_err();
        assert!(matches!(err, SplitError::RegionMismatch { .. }));
    }

    #[tokio::test]
    async fn splice_blob_happy_path() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let bd = blob_digest(b"blob-splice");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        let c0 = Bytes::from_static(b"AAAA");
        let c1 = Bytes::from_static(b"BBBB");
        let c2 = Bytes::from_static(b"CCCC");
        w.handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), c0.clone(), "req")
            .await
            .unwrap();
        w.handler
            .append_chunk(&ctx, session_id, ChunkIndex(1), c1.clone(), "req")
            .await
            .unwrap();
        w.handler
            .append_chunk(&ctx, session_id, ChunkIndex(2), c2.clone(), "req")
            .await
            .unwrap();
        let fin = w
            .handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();

        // Splice.
        let mut sink = super::super::assembler::CollectingSink::new();
        let outcome = w
            .handler
            .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "req-spl")
            .await
            .unwrap();
        assert_eq!(outcome.chunks_streamed, 3);
        assert_eq!(outcome.bytes_streamed, 12);
        // Sink received the full blob.
        let collected: Vec<Bytes> = sink.take();
        let mut all = Vec::new();
        for c in collected {
            all.extend_from_slice(&c);
        }
        assert_eq!(&all, b"AAAABBBBCCCC");
    }

    #[tokio::test]
    async fn splice_blob_unknown_manifest_404() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
        let unknown = ManifestDigest::from_bytes([0u8; 32]);
        let mut sink = super::super::assembler::CollectingSink::new();
        let err = w
            .handler
            .splice_blob(&ctx, &unknown, &mut sink, "req")
            .await
            .unwrap_err();
        assert!(matches!(err, SpliceError::ManifestNotFound));
        assert_eq!(err.cor_code(), "COR_MULTIPART_MANIFEST_NOT_FOUND");
    }

    #[tokio::test]
    async fn splice_blob_chunk_verify_failure_aborts_stream() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(
            tenant,
            Region::Wnam,
            PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
        );
        let bd = blob_digest(b"blob-tamper");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        let c0 = Bytes::from_static(b"AAAA");
        let c1 = Bytes::from_static(b"BBBB");
        w.handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), c0, "req")
            .await
            .unwrap();
        let d1 = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(1), c1, "req")
            .await
            .unwrap();
        let fin = w
            .handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
        // Tamper chunk 1 in the chunk store.
        let key = ChunkKey::new(tenant, d1);
        assert!(w.chunks.tamper_for_test(key));
        let mut sink = super::super::assembler::CollectingSink::new();
        let err = w
            .handler
            .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "req-spl")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SpliceError::ChunkVerificationFailed { chunk_index: 1 }
        ));
        // Sink received chunk 0 only — chunk 1 never fanned out.
        let collected: Vec<Bytes> = sink.take();
        let mut all = Vec::new();
        for c in collected {
            all.extend_from_slice(&c);
        }
        assert_eq!(&all, b"AAAA", "no unverified bytes may reach the sink");
    }

    #[tokio::test]
    async fn audit_emit_on_chunk_too_large_path() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-big-audit");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        let huge = Bytes::from(vec![0u8; MAX_CHUNK_BYTES + 1]);
        let _ = w
            .handler
            .append_chunk(&ctx, session_id, ChunkIndex(0), huge, "req")
            .await;
        // Audit captured the failure.
        let appended = w.audit.snapshot_of(BlobEventType::SplitChunkAppended);
        assert!(
            appended.iter().any(|r| r.reason == "chunk_too_large"),
            "must emit audit on chunk_too_large path"
        );
    }

    #[tokio::test]
    async fn clock_advance_does_not_affect_session_lifecycle() {
        // Session lifecycle is event-driven, not wall-clock-driven —
        // make sure advancing the clock does not flip a Live session
        // into something stale (sweeper handling lives in WI-S05-006).
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let bd = blob_digest(b"blob-clock");
        let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
            InitSplitOutcome::Started { session_id } => session_id,
            other => panic!("{other:?}"),
        };
        w.clock.advance_ms(7 * 24 * 60 * 60 * 1000);
        // Append still works.
        w.handler
            .append_chunk(
                &ctx,
                session_id,
                ChunkIndex(0),
                Bytes::from_static(b"x"),
                "req",
            )
            .await
            .unwrap();
        // Finalize still works.
        w.handler
            .finalize_split(&ctx, session_id, "req-fin")
            .await
            .unwrap();
    }

}
