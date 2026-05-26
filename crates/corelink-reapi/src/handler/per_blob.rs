//! Per-blob orchestration helper consumed by
//! `CasWriteService::batch_update_blobs`. Extracted from the monolithic
//! `handler.rs` per Wave 33 Stream A2.1c file-size discipline so
//! `helpers.rs` stays under the L2.10 hard cap.

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::MetaStore;
use corelink_cas::r2_storage::R2Backend;
use corelink_worker::{Region, TenantCtx as StorageTenantCtx};
use uuid::Uuid;

use crate::capabilities::MAX_CAS_BLOB_SIZE_BYTES;
use crate::error_map::{
    HashErrorMapping, MetaErrorMapping, R2ErrorMapping, GRPC_INVALID_ARGUMENT,
    GRPC_RESOURCE_EXHAUSTED,
};
use crate::orchestrator::{
    CasPutOutcome, CasWriteOrchestrator, CommitPutPlan, OrchestratorError, OrphanReconciler,
};
use crate::proto::reapi as reapi_proto;
use crate::proto::reapi::{batch_update_blobs_response, Digest as ProtoDigest};

use super::audit_emit::deterministic_audit_id;
use super::helpers::region_str;
use super::Clock;

#[allow(
    clippy::too_many_arguments,
    reason = "single call site; struct-bundling each call's args adds noise without real reuse"
)]
pub(super) async fn process_one_blob<B, M, R, C>(
    orch: &CasWriteOrchestrator<'_, B, M, R>,
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    request_id: &str,
    sub: reapi_proto::batch_update_blobs_request::Request,
    clock: &C,
) -> batch_update_blobs_response::Response
where
    B: R2Backend,
    M: MetaStore,
    R: OrphanReconciler,
    C: Clock,
{
    // Per-blob compressor validation (codex round-4 P2 SEAL fix):
    // REAPI v2.12.0 `BatchUpdateBlobsRequest.Request.compressor` is
    // the encoding of the inline `data` field. CoreLink S-01 only
    // accepts `IDENTITY` (= 0) — `Capabilities.GetCapabilities`
    // returns empty `supported_batch_update_compressors`, so a
    // ZSTD/DEFLATE/BROTLI-compressed upload would silently hash the
    // compressed bytes against the declared digest and fail with
    // `COR_CAS_DIGEST_MISMATCH` (or, worse, accept ambiguous bytes
    // if the digest happens to match). Reject early with
    // `INVALID_ARGUMENT` + `COR_CAS_DIGEST_FUNCTION_UNSUPPORTED`
    // (the same taxonomy code already covers
    // capability-negotiation failures on the BatchUpdateBlobs
    // surface). Compressed-batch upload ships with WI-S05-005.
    if sub.compressor != 0 {
        return per_blob_failure(
            sub.digest.clone(),
            GRPC_INVALID_ARGUMENT,
            "BatchUpdateBlobsRequest.Request.compressor must be IDENTITY (= 0); CoreLink S-01 supports IDENTITY only",
        );
    }
    let proto_digest = match sub.digest {
        Some(d) => d,
        None => {
            return per_blob_failure(
                None,
                GRPC_INVALID_ARGUMENT,
                "BatchUpdateBlobsRequest.Request.digest is missing",
            );
        }
    };
    let digest = match Digest::from_hex(&proto_digest.hash) {
        Ok(d) => d,
        Err(_) => {
            return per_blob_failure(
                Some(proto_digest.clone()),
                GRPC_INVALID_ARGUMENT,
                "digest hex is malformed (expected 64 lowercase hex chars)",
            );
        }
    };
    // Validate `digest.size_bytes == data.len()` per REAPI v2.12.0
    // BatchUpdateBlobsRequest invariant. A mismatch is a malformed request
    // (NOT a hash poisoning attempt) and maps to gRPC INVALID_ARGUMENT +
    // taxonomy `COR_CAS_BAD_DIGEST` — distinct from `COR_CAS_DIGEST_MISMATCH`
    // (gRPC ABORTED) which is the BLAKE3-of-body disagreement.
    if proto_digest.size_bytes < 0 {
        return per_blob_failure(
            Some(proto_digest.clone()),
            GRPC_INVALID_ARGUMENT,
            "digest.size_bytes is negative",
        );
    }
    let declared_size = u64::try_from(proto_digest.size_bytes).unwrap_or(u64::MAX);
    let body_len = u64::try_from(sub.data.len()).unwrap_or(u64::MAX);
    if declared_size != body_len {
        return per_blob_failure(
            Some(proto_digest.clone()),
            GRPC_INVALID_ARGUMENT,
            "digest.size_bytes does not match data.len()",
        );
    }
    let body = Bytes::from(sub.data);
    if body.len() > usize::try_from(MAX_CAS_BLOB_SIZE_BYTES).unwrap_or(usize::MAX) {
        return per_blob_failure(
            Some(proto_digest),
            GRPC_RESOURCE_EXHAUSTED,
            "blob exceeds 5 MiB single-blob cap",
        );
    }
    let canonical = format!("blake3:{}", digest.to_hex());
    let now_ms = clock.now_ms();
    let audit_id = deterministic_audit_id(request_id, storage_ctx.tenant_id(), &digest);
    let audit_request_id = crate::orchestrator::audit_request_id_for_blob(
        request_id,
        storage_ctx.tenant_id(),
        &canonical,
    );
    let plan = CommitPutPlan {
        ctx: storage_ctx,
        body: body.clone(),
        claimed_digest: digest,
        size_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
        digest_canonical_text: canonical,
        principal_id,
        region_str: region_str(region),
        client_request_id: request_id,
        audit_request_id,
        audit_id,
        now_ms,
    };
    match orch.commit_put(plan).await {
        Ok(out) => {
            tracing::debug!(
                outcome = ?out.outcome,
                digest = %proto_digest.hash,
                "BatchUpdateBlobs per-blob success"
            );
            batch_update_blobs_response::Response {
                digest: Some(proto_digest),
                status: Some(crate::proto::google_rpc::Status {
                    code: 0,
                    message: match out.outcome {
                        CasPutOutcome::Fresh => "ok".to_owned(),
                        CasPutOutcome::Idempotent => "ok (idempotent)".to_owned(),
                    },
                    details: vec![],
                }),
            }
        }
        Err(OrchestratorError::HashMismatch(hm)) => {
            let m = hm.mapping();
            per_blob_failure(Some(proto_digest), m.grpc_code, m.message)
        }
        Err(OrchestratorError::R2(e)) => {
            let m = e.mapping();
            per_blob_failure(Some(proto_digest), m.grpc_code, m.message)
        }
        Err(OrchestratorError::Meta(e)) => {
            let m = e.mapping();
            per_blob_failure(Some(proto_digest), m.grpc_code, m.message)
        }
        Err(OrchestratorError::AuditEnvelopeSerialize(_)) => per_blob_failure(
            Some(proto_digest),
            crate::error_map::GRPC_INTERNAL,
            "audit envelope JSON serialization failed",
        ),
    }
}

pub(super) fn per_blob_failure(
    digest: Option<ProtoDigest>,
    grpc_code: i32,
    message: &str,
) -> batch_update_blobs_response::Response {
    batch_update_blobs_response::Response {
        digest,
        status: Some(crate::proto::google_rpc::Status {
            code: grpc_code,
            message: message.to_owned(),
            details: vec![],
        }),
    }
}
