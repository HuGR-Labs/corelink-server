//! [`SplitSpliceHandlerImpl`] — canonical pure-logic SplitBlob/SpliceBlob
//! handler. Struct definition + constructor + region/scope helpers +
//! audit-record builder + trait-impl trampolines (which delegate into
//! [`super::handler_methods`] for the per-method async bodies).
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker reapi::ac canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use bytes::Bytes;
use corelink_pat::{SCOPE_CACHE_R, SCOPE_CACHE_W};

use super::super::assembler::{BlobAssembler, ChunkSink};
use super::super::audit::{AuditSink, BlobAuditRecord, BlobEventType};
use super::super::chunk_store::ChunkStore;
use super::super::session::SessionStore;
use super::super::types::{BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId};
use super::builder::{Clock, SplitSpliceHandlerBuilder};
use super::errors::{SpliceError, SplitError};
use super::handler_trait::SplitSpliceHandler;
use super::types::{FinalizeSplitOutcome, InitSplitOutcome, SpliceOutcome};
use crate::middleware::auth_ctx::AuthCtx;
use crate::Region;

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
    pub(super) region: Region,
    pub(super) sessions: Arc<S>,
    pub(super) chunks: Arc<C>,
    pub(super) assembler: Arc<B>,
    pub(super) audit: Arc<A>,
    pub(super) clock: Arc<dyn Clock>,
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

    pub(super) fn check_region_split(&self, ctx: &AuthCtx) -> Result<(), SplitError> {
        if ctx.region() != self.region {
            return Err(SplitError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    pub(super) fn check_region_splice(&self, ctx: &AuthCtx) -> Result<(), SpliceError> {
        if ctx.region() != self.region {
            return Err(SpliceError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    pub(super) fn require_split_scope(ctx: &AuthCtx) -> Result<(), SplitError> {
        if ctx.has_scope(SCOPE_CACHE_W) {
            Ok(())
        } else {
            Err(SplitError::ScopeInsufficient {
                required: SCOPE_CACHE_W,
            })
        }
    }

    pub(super) fn require_splice_scope(ctx: &AuthCtx) -> Result<(), SpliceError> {
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
    pub(super) fn make_record(
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
