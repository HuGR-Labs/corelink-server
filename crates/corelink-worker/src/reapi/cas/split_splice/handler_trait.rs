//! Canonical SplitBlob/SpliceBlob handler trait.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker reapi::ac canonical pattern"
)]

use core::future::Future;

use bytes::Bytes;

use super::super::assembler::ChunkSink;
use super::super::types::{BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId};
use super::errors::{SpliceError, SplitError};
use super::types::{FinalizeSplitOutcome, InitSplitOutcome, SpliceOutcome};
use crate::middleware::auth_ctx::AuthCtx;

/// Canonical SplitBlob/SpliceBlob handler trait. The gRPC tonic service
/// wrapper + axum REST router (deferred to WI-S05-006 conformance
/// suite) both consume this trait.
pub trait SplitSpliceHandler: Send + Sync {
    /// SplitBlob step \[0\]: open a multipart upload session for
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

    /// SplitBlob step \[k\]: append a chunk to the open session at the
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

    /// SplitBlob step \[N\]: finalize a session into a sealed manifest.
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
