//! `CasWriteService` — `ContentAddressableStorage` gRPC service surface.
//! Hosts the canonical three REAPI v2 RPCs:
//!
//! - `batch_update_blobs` — bounded-concurrency per-blob orchestration.
//! - `batch_read_blobs` — two-pass D1 + R2 pipeline (helpers in
//!   [`super::batch_read`] + [`super::batch_read_compose`]).
//! - `find_missing_blobs` — size-aware discovery surface.
//!
//! Extracted from the monolithic `handler.rs` per Wave 33 Stream A2.1c
//! file-size discipline.

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_meta::MetaStore;
use corelink_worker::storage::r2::R2Backend;
use tonic::{async_trait, Code, Request, Response, Status};

use crate::capabilities::MAX_BATCH_TOTAL_SIZE_BYTES;
use crate::error_map::{
    FindMissingErrorMapping, COR_CAS_BAD_DIGEST, COR_CAS_BATCH_SIZE_EXCEEDED,
    COR_CAS_BATCH_TOO_LARGE, COR_CAS_DIGEST_FUNCTION_UNSUPPORTED,
};
use crate::find_missing::{FindMissingOrchestrator, MAX_FIND_MISSING_BATCH_SIZE};
use crate::orchestrator::{CasWriteOrchestrator, OrphanReconciler};
use crate::pat::{AuthScope, PatValidator};
use crate::proto::reapi::content_addressable_storage_server::{
    ContentAddressableStorage, ContentAddressableStorageServer,
};
use crate::proto::reapi::{
    batch_update_blobs_response, BatchReadBlobsRequest, BatchReadBlobsResponse,
    BatchUpdateBlobsRequest, BatchUpdateBlobsResponse, Digest as ProtoDigest,
    FindMissingBlobsRequest, FindMissingBlobsResponse,
};

use super::audit_emit_batch::emit_find_missing_batch_audit;
use super::batch_read::{
    batch_read_decide, batch_read_pass1_d1, batch_read_pass2_r2, batch_read_prevalidate_slots,
    batch_read_validate_request,
};
use super::batch_read_compose::batch_read_compose_responses;
use super::helpers::{grpc_code_from_i32, make_status, pat_error_to_status};
use super::per_blob::process_one_blob;
use super::{Clock, HandlerCore};

/// Concrete `ContentAddressableStorage` service.
pub struct CasWriteService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    core: Arc<HandlerCore<V, B, M, R, C>>,
}

impl<V, B, M, R, C> std::fmt::Debug for CasWriteService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasWriteService").finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> Clone for CasWriteService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

impl<V, B, M, R, C> CasWriteService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    /// Wrap a `HandlerCore` into the gRPC service.
    #[must_use]
    pub fn new(core: Arc<HandlerCore<V, B, M, R, C>>) -> Self {
        Self { core }
    }

    /// Wrap into a tonic-server-ready service.
    #[must_use]
    pub fn into_server(self) -> ContentAddressableStorageServer<Self> {
        ContentAddressableStorageServer::new(self)
    }
}

#[async_trait]
impl<V, B, M, R, C> ContentAddressableStorage for CasWriteService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    async fn batch_update_blobs(
        &self,
        request: Request<BatchUpdateBlobsRequest>,
    ) -> Result<Response<BatchUpdateBlobsResponse>, Status> {
        // Step 1 — auth.
        let (pat_ctx, storage_ctx) = self.core.authenticate(&request)?;
        pat_ctx
            .require_scope(AuthScope::CacheWrite)
            .map_err(|e| pat_error_to_status(&e))?;

        // Step 2 — payload pre-validation.
        let inner = request.into_inner();
        // 2a. Digest-function negotiation: S-01 advertises BLAKE3 ONLY (per
        //     `Capabilities.GetCapabilities`). REAPI v2.12.0 lets the
        //     client leave `digest_function` unset (UNKNOWN=0) and rely on
        //     hash-length inference — we accept that path because BLAKE3
        //     is the only function we actually verify against. An EXPLICIT
        //     non-BLAKE3 declaration is rejected up-front rather than
        //     letting per-blob hash-mismatch errors leak into the response
        //     (which would misclassify a digest-function-mismatch as a
        //     content-mismatch ABORTED).
        if inner.digest_function != 0
            && inner.digest_function != crate::capabilities::DigestFunction::Blake3 as i32
        {
            return Err(make_status(
                Code::InvalidArgument,
                COR_CAS_DIGEST_FUNCTION_UNSUPPORTED,
                "BatchUpdateBlobsRequest.digest_function declares a non-BLAKE3 hash; CoreLink S-01 advertises BLAKE3 only via Capabilities.GetCapabilities",
                0,
            ));
        }
        // 2b. Aggregate ≤ 4 MiB cap (canonical `MaxBatchTotalSizeBytes`).
        let aggregate_bytes: u64 = inner
            .requests
            .iter()
            .map(|r| u64::try_from(r.data.len()).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add);
        if aggregate_bytes > u64::try_from(MAX_BATCH_TOTAL_SIZE_BYTES).unwrap_or(u64::MAX) {
            return Err(make_status(
                Code::ResourceExhausted,
                COR_CAS_BATCH_TOO_LARGE,
                "BatchUpdateBlobsRequest aggregate exceeds MaxBatchTotalSizeBytes (4 MiB) — fragment the batch or use ByteStream::Write",
                aggregate_bytes,
            ));
        }

        // Step 3 — per-blob orchestration with bounded concurrency.
        //
        // Per WI-S01-005 §9.5 the production target is
        // `buffer_unordered(16)`. We preserve REAPI's
        // BatchUpdateBlobsResponse-order = BatchUpdateBlobsRequest-order
        // contract by emitting per-index futures and re-sorting outputs by
        // `(index, response)` after the unordered drain.
        let orch = CasWriteOrchestrator::new(
            self.core.writer_arc(),
            self.core.meta_arc().as_ref(),
            self.core.reconciler_arc().as_ref(),
        );
        const BOUNDED_CONCURRENCY: usize = 16;
        use futures::stream::StreamExt;
        let principal_id = pat_ctx.principal_id();
        let region = pat_ctx.region();
        let request_id = pat_ctx.request_id().to_owned();
        let storage_ctx_ref = &storage_ctx;
        let clock_ref = self.core.clock_ref();
        let orch_ref = &orch;
        let request_id_ref = &request_id;
        let mut indexed: Vec<(usize, batch_update_blobs_response::Response)> =
            futures::stream::iter(inner.requests.into_iter().enumerate())
                .map(|(idx, sub)| async move {
                    let resp = process_one_blob(
                        orch_ref,
                        storage_ctx_ref,
                        principal_id,
                        region,
                        request_id_ref.as_str(),
                        sub,
                        clock_ref,
                    )
                    .await;
                    (idx, resp)
                })
                .buffer_unordered(BOUNDED_CONCURRENCY)
                .collect()
                .await;
        indexed.sort_by_key(|&(i, _)| i);
        let responses: Vec<_> = indexed.into_iter().map(|(_, r)| r).collect();

        Ok(Response::new(BatchUpdateBlobsResponse { responses }))
    }

    async fn batch_read_blobs(
        &self,
        request: Request<BatchReadBlobsRequest>,
    ) -> Result<Response<BatchReadBlobsResponse>, Status> {
        // Step 1 — auth + scope (`cache:r` required; same as
        // ByteStream::Read).
        let (pat_ctx, storage_ctx) = self.core.authenticate(&request)?;
        pat_ctx
            .require_scope(AuthScope::CacheRead)
            .map_err(|e| pat_error_to_status(&e))?;

        let inner = request.into_inner();

        // Step 2 + 2b + 3 — request-level validation.
        let (digests, inline_cap_bytes, single_blob_inline_cap_bytes) =
            batch_read_validate_request(inner)?;
        let n_input = digests.len();

        // Step 4 — TWO-PASS dispatch (codex round-1 P0 fix —
        // memory-bounded by construction). Phase helpers live in the
        // sibling `batch_read` + `batch_read_compose` modules per Wave 33
        // Stream A2.1c file-size discipline.
        let slots = batch_read_prevalidate_slots(digests);
        let size_results = batch_read_pass1_d1(
            self.core.meta_arc().as_ref(),
            storage_ctx.tenant_id(),
            &slots,
            n_input,
        )
        .await;
        let decisions = batch_read_decide(
            &slots,
            size_results,
            inline_cap_bytes,
            single_blob_inline_cap_bytes,
        );
        let fetched = batch_read_pass2_r2(
            self.core.reader_arc().as_ref(),
            &storage_ctx,
            &decisions,
            n_input,
        )
        .await;
        let responses = batch_read_compose_responses(&storage_ctx, &pat_ctx, decisions, &fetched);

        // WI-S02-004 / ADR-0023 — single-digest probe attack vector:
        // when EVERY response in this batch is a NOT_FOUND, the
        // overall response shape is a "miss" from the attacker's
        // perspective. Insert the `MissMarker` extension so the
        // timing-padding layer pads the response. Codex round-6 P1
        // fix — earlier draft only padded ByteStream::Read; batch
        // handlers were unprotected against single-digest probes.
        let all_not_found = !responses.is_empty()
            && responses.iter().all(|r| {
                r.status
                    .as_ref()
                    .is_some_and(|s| s.code == crate::error_map::GRPC_NOT_FOUND)
            });
        let mut response = Response::new(BatchReadBlobsResponse { responses });
        if all_not_found {
            response
                .extensions_mut()
                .insert(corelink_worker::middleware::MissMarker::new());
        }
        Ok(response)
    }

    async fn find_missing_blobs(
        &self,
        request: Request<FindMissingBlobsRequest>,
    ) -> Result<Response<FindMissingBlobsResponse>, Status> {
        // Step 1 — auth + scope. Per WI-S02-002 §6.1.2 +
        // `auth_model.md §3.1 L188` the canonical scope for
        // `FindMissingBlobs` is **strictly** `cache:find-missing`
        // (discovery-only; does NOT imply download capability). The
        // canonical taxonomy treats `cache:r` and `cache:find-missing`
        // as DISTINCT capabilities — the auth model deliberately
        // separates them so a CI / read-only token can be issued that
        // discovers existence WITHOUT being able to download (and,
        // symmetrically, so a download token can be issued that does
        // NOT carry batch-discovery capability). Codex round-1 P1
        // (WI-S02-002 SEAL): the earlier "read implies discovery"
        // shortcut expanded policy beyond the canonical scope table
        // and made the new scope unenforceable for read-only tokens.
        // We strictly require `cache:find-missing` here.
        let (pat_ctx, storage_ctx) = self.core.authenticate(&request)?;
        pat_ctx
            .require_scope(AuthScope::CacheFindMissing)
            .map_err(|e| pat_error_to_status(&e))?;

        let inner = request.into_inner();

        // Step 2 — digest-function negotiation (BLAKE3 only).
        if inner.digest_function != 0
            && inner.digest_function != crate::capabilities::DigestFunction::Blake3 as i32
        {
            return Err(make_status(
                Code::InvalidArgument,
                COR_CAS_DIGEST_FUNCTION_UNSUPPORTED,
                "FindMissingBlobsRequest.digest_function declares a non-BLAKE3 hash; CoreLink advertises BLAKE3 only via Capabilities.GetCapabilities",
                0,
            ));
        }

        // Step 3 — batch-size cap.
        if inner.blob_digests.len() > MAX_FIND_MISSING_BATCH_SIZE {
            return Err(make_status(
                Code::OutOfRange,
                COR_CAS_BATCH_SIZE_EXCEEDED,
                "FindMissingBlobsRequest carries more digests than the canonical CoreLink batch cap (1000)",
                inner.blob_digests.len() as u64,
            ));
        }

        // Step 4 — pre-validate digests up-front. A malformed digest
        // mid-batch is a top-level INVALID_ARGUMENT (NOT a per-digest
        // 404 that would silently mask a tooling bug). REAPI v2
        // canonical: malformed inputs are top-level errors.
        //
        // We carry the declared `size_bytes` alongside each `Digest`
        // so the orchestrator can enforce the canonical REAPI
        // `Digest = (hash, size_bytes)` identity (codex round-2 P1
        // SEAL fix): a request `(H, wrong_size)` whose underlying
        // blob `(H, real_size)` exists with `real_size != wrong_size`
        // surfaces as MISSING — a tooling bug that ships a wrong
        // size MUST NOT be silently masked as "present", and the
        // wire response stays REAPI-conformant.
        let mut parsed: Vec<(Digest, Option<u64>)> = Vec::with_capacity(inner.blob_digests.len());
        for pd in &inner.blob_digests {
            let d = Digest::from_hex(&pd.hash).map_err(|_| {
                make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_DIGEST,
                    "digest hex is malformed (expected 64 lowercase hex chars)",
                    0,
                )
            })?;
            if pd.size_bytes < 0 {
                return Err(make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_DIGEST,
                    "digest.size_bytes is negative",
                    0,
                ));
            }
            // SAFETY of `as u64`: pd.size_bytes is non-negative here
            // (checked above), so the cast widens losslessly.
            #[allow(clippy::cast_sign_loss, reason = "non-negative checked above")]
            let declared = pd.size_bytes as u64;
            parsed.push((d, Some(declared)));
        }

        // Step 5 — dispatch the find_missing orchestrator (REAPI
        // size-aware variant).
        let orch = FindMissingOrchestrator::new(self.core.meta_arc().as_ref());
        let outcome = orch
            .find_missing_with_sizes(&storage_ctx, &parsed)
            .await
            .map_err(|e| {
                let m = e.mapping();
                make_status(
                    grpc_code_from_i32(m.grpc_code),
                    m.taxonomy_code,
                    m.message,
                    0,
                )
            })?;

        // Step 6 — emit metrics + audit (low-severity per WI §6.1.5;
        // the FindMissingBlobs surface is high-volume and SHOULD NOT
        // emit per-digest CE envelopes — that would pollute the audit
        // chain). We log a single batch-summary line to
        // `corelink.audit` so SRE can dashboard request rate +
        // missing-rate without per-digest noise.
        emit_find_missing_batch_audit(
            &storage_ctx,
            pat_ctx.principal_id(),
            pat_ctx.region(),
            pat_ctx.request_id(),
            parsed.len(),
            outcome.missing.len(),
            outcome.d1_lookups,
        );

        // Step 7 — recompose response in input order. The
        // orchestrator's `slot_is_missing: Vec<bool>` is the canonical
        // per-slot answer (codex round-2 P1 SEAL fix); a single linear
        // pass over `parsed` paired with `slot_is_missing` is
        // unambiguous even when the input contains the SAME hash with
        // DIFFERENT declared sizes (`[(H, real), (H, wrong)]` —
        // REAPI Digest identity = `(hash, size_bytes)`, so each slot
        // gets its own answer).
        let mut missing_blob_digests: Vec<ProtoDigest> = Vec::with_capacity(outcome.missing.len());
        for (i, (digest, declared)) in parsed.iter().enumerate() {
            if matches!(outcome.slot_is_missing.get(i), Some(true)) {
                let size_bytes = match declared {
                    Some(s) => i64::try_from(*s).unwrap_or(i64::MAX),
                    None => 0,
                };
                missing_blob_digests.push(ProtoDigest {
                    hash: digest.to_hex(),
                    size_bytes,
                });
            }
        }

        // WI-S02-004 / ADR-0023 — single-digest probe attack vector:
        // when EVERY input digest is reported missing, the response
        // shape is a "miss" from the attacker's perspective. Insert
        // the `MissMarker` extension so the timing-padding layer
        // pads the response. Codex round-6 P1 fix.
        let all_missing = !parsed.is_empty() && missing_blob_digests.len() == parsed.len();
        let mut response = Response::new(FindMissingBlobsResponse {
            missing_blob_digests,
        });
        if all_missing {
            response
                .extensions_mut()
                .insert(corelink_worker::middleware::MissMarker::new());
        }
        Ok(response)
    }
}
