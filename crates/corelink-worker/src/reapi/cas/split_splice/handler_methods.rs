//! Inner async methods for [`super::handler::SplitSpliceHandlerImpl`] —
//! `init_split_inner`, `append_chunk_inner`, `finalize_split_inner`,
//! `abort_split_inner`, `splice_blob_inner`.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2).

use bytes::Bytes;

use super::super::assembler::{BlobAssembler, ChunkSink, ManifestKey};
use super::super::audit::{AuditSink, BlobEventType};
use super::super::chunk_store::{ChunkKey, ChunkStore};
use super::super::session::{SessionFinalize, SessionInit, SessionKey, SessionState, SessionStore};
use super::super::types::{
    BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId, MAX_CHUNKS_PER_BLOB,
};
use super::errors::{SpliceError, SplitError};
use super::handler::SplitSpliceHandlerImpl;
use super::types::{FinalizeSplitOutcome, InitSplitOutcome, SpliceOutcome, MAX_CHUNK_BYTES};
use crate::middleware::auth_ctx::AuthCtx;

impl<S, C, B, A> SplitSpliceHandlerImpl<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    pub(super) async fn init_split_inner(
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

    pub(super) async fn append_chunk_inner(
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

    pub(super) async fn finalize_split_inner(
        &self,
        ctx: &AuthCtx,
        session_id: SessionId,
        request_id: &str,
    ) -> Result<FinalizeSplitOutcome, SplitError> {
        self.check_region_split(ctx)?;
        Self::require_split_scope(ctx)?;

        // Idempotent finalize — second call returns the cached digest.
        if let Some(snap) = self.sessions.lookup(ctx.tenant_id(), session_id).await? {
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

    pub(super) async fn abort_split_inner(
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

    pub(super) async fn splice_blob_inner(
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
