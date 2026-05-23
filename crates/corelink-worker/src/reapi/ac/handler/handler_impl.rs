//! Handler constructor + helper methods + `ActionCacheHandler` trait
//! impl trampolines.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::future::Future;
use std::sync::Arc;

use super::super::audit::{AcAuditRecord, AcEventType, AuditSink};
use super::super::merkle::MerkleVerifier;
use super::super::meta::AcMetaStore;
use super::super::outputs::OutputsCheck;
use super::super::sig::Signer;
use super::super::types::{ActionDigest, ActionResult, ResultHash};
use super::builder::{ActionCacheHandlerBuilder, ActionCacheHandlerImpl};
use super::envelope_store::AcEnvelopeStore;
use super::errors::{AcError, GetActionResult, UpdateActionResult};
use super::handler_trait::ActionCacheHandler;
use crate::cache::kv::KvBackend;
use crate::middleware::auth_ctx::AuthCtx;

impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Construct a handler from the canonical builder.
    #[must_use]
    pub fn new(b: ActionCacheHandlerBuilder<M, E, V, S, O, A, K>) -> Self {
        Self {
            region: b.region,
            meta: b.meta,
            envelope_store: b.envelope_store,
            merkle: b.merkle,
            signer: b.signer,
            outputs: b.outputs,
            audit: b.audit,
            neg_cache: b.neg_cache,
            sig_key_id: b.sig_key_id,
            path_key_id: b.path_key_id,
            ttl_extend_ms: b.ttl_extend_ms,
            clock: b.clock,
            action_result_stash: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Region this handler is pinned to.
    #[must_use]
    pub const fn region(&self) -> crate::Region {
        self.region
    }

    /// Configured TTL extension (ms).
    #[must_use]
    pub const fn ttl_extend_ms(&self) -> u64 {
        self.ttl_extend_ms
    }

    pub(super) fn check_region(&self, ctx: &AuthCtx) -> Result<(), AcError> {
        if ctx.region() != self.region {
            return Err(AcError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    pub(super) fn require_scope(ctx: &AuthCtx, required: u64) -> Result<(), AcError> {
        if ctx.has_scope(required) {
            Ok(())
        } else {
            Err(AcError::ScopeInsufficient { required })
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "8-arg shape mirrors the canonical AcAuditRecord field set + ctx for request-time clock + tenant_id derivation; further compaction would obscure the audit envelope contract"
    )]
    pub(super) fn make_record(
        &self,
        event_type: AcEventType,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        request_id: &str,
        result_hash: Option<ResultHash>,
        reason: &'static str,
        missing_outputs: Vec<corelink_hash::Digest>,
    ) -> AcAuditRecord {
        AcAuditRecord {
            event_type,
            tenant_id: ctx.tenant_id(),
            region: ctx.region(),
            action_digest: *action_digest,
            result_hash,
            request_id: request_id.to_string(),
            reason,
            missing_outputs,
            now_ms: self.clock.now_ms(),
        }
    }
}

impl<M, E, V, S, O, A, K> ActionCacheHandler for ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn get_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<GetActionResult, AcError>> + Send + 'a {
        async move { self.get_action_result_inner(ctx, action_digest, request_id).await }
    }

    fn update_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        action_result: ActionResult,
        request_id: &'a str,
    ) -> impl Future<Output = Result<UpdateActionResult, AcError>> + Send + 'a {
        async move {
            self.update_action_result_inner(ctx, action_digest, action_result, request_id)
                .await
        }
    }
}
