//! `BatchReadBlobs` Step 4e — final input-order response composition +
//! per-slot audit emission. Extracted from
//! [`super::batch_read`] per Wave 33 Stream A2.1c file-size discipline.
//!
//! Per WI-S02-002 §4e + ADR-0028 audit emission parity with
//! `ByteStream::Read`: each per-slot miss surfaces as the canonical CE
//! event type for its [`MissReason`] arm; each successfully delivered
//! Fetch slot emits `corelink.cas.read_completed`. The wire response
//! stays the uniform 404 per ADR-0028.

use crate::error_map::GRPC_INVALID_ARGUMENT;
use crate::error_map::GRPC_NOT_FOUND;
use crate::pat::TenantContext;
use crate::proto::reapi::batch_read_blobs_response;
use crate::read::MissReason;
use corelink_replication::region_resolver::TenantCtx as StorageTenantCtx;

use super::audit_emit_batch::{
    emit_read_completed_audit_post_stream_at_slot, emit_read_miss_audit_at_slot,
};
use super::batch_read::{Decision, FetchOutcome};

/// 4e. Compose final responses in input order. Per-slot audit emission
/// lands here so the S-09 forensics / chain consumer sees
/// `corelink.cas.read_miss` (uniform NeverExisted + CrossTenantMasked +
/// Tombstoned per ADR-0028) for batch miss probes AND
/// `corelink.cas.read_completed` for successful per-blob delivery — at
/// parity with `ByteStream::Read` (codex round-2 P1 SEAL fix).
pub(super) fn batch_read_compose_responses(
    storage_ctx: &StorageTenantCtx,
    pat_ctx: &TenantContext,
    decisions: Vec<Decision>,
    fetched: &[Option<FetchOutcome>],
) -> Vec<batch_read_blobs_response::Response> {
    let n_input = decisions.len();
    let mut responses: Vec<batch_read_blobs_response::Response> = Vec::with_capacity(n_input);
    for (idx, decision) in decisions.into_iter().enumerate() {
        let resp = match decision {
            Decision::BadDigest {
                proto_digest,
                message,
                grpc_code,
            } => batch_read_blobs_response::Response {
                digest: proto_digest,
                data: vec![],
                compressor: 0,
                status: Some(crate::proto::google_rpc::Status {
                    code: grpc_code,
                    message,
                    details: vec![],
                }),
            },
            Decision::Miss {
                proto_digest,
                digest,
                reason,
            } => {
                // Audit emission parity with `ByteStream::Read` (ADR-0028
                // + WI-S02-002 §11): each per-slot miss surfaces as the
                // canonical CE event type for its [`MissReason`] —
                // `read_miss` for NeverExisted (low-severity, conflated
                // with CrossTenantMasked per ADR-0028; S-09 reclassifier
                // folds true cross-tenant via the global digest index),
                // `tombstoned_read_attempt` for legitimate post-GC reads.
                // The wire response stays the uniform 404 (codex round-2
                // P2 SEAL fix). The `slot_idx` token is mixed into the
                // audit `id` derivation so duplicate digests in the same
                // batch do NOT collide on the `audit_outbox` PK (codex
                // round-3 P2 SEAL fix).
                emit_read_miss_audit_at_slot(
                    storage_ctx,
                    pat_ctx.principal_id(),
                    pat_ctx.region(),
                    pat_ctx.request_id(),
                    &digest,
                    reason,
                    idx,
                );
                batch_read_blobs_response::Response {
                    digest: Some(proto_digest),
                    data: vec![],
                    compressor: 0,
                    status: Some(crate::proto::google_rpc::Status {
                        code: GRPC_NOT_FOUND,
                        message: "blob not found".to_owned(),
                        details: vec![],
                    }),
                }
            }
            Decision::ExceedsSingleCap { proto_digest } => batch_read_blobs_response::Response {
                digest: Some(proto_digest),
                data: vec![],
                compressor: 0,
                status: Some(crate::proto::google_rpc::Status {
                    code: 9, // FAILED_PRECONDITION
                    message:
                        "blob exceeds 4 MiB BatchReadBlobs inline cap; use ByteStream::Read"
                            .to_owned(),
                    details: vec![],
                }),
            },
            Decision::ExceedsAggregate { proto_digest } => batch_read_blobs_response::Response {
                digest: Some(proto_digest),
                data: vec![],
                compressor: 0,
                status: Some(crate::proto::google_rpc::Status {
                    code: 9, // FAILED_PRECONDITION
                    message: "blob skipped: BatchReadBlobs aggregate cap (4 MiB) exceeded; use ByteStream::Read for this digest"
                        .to_owned(),
                    details: vec![],
                }),
            },
            Decision::CallerSizeMismatch {
                proto_digest,
                row_size,
            } => batch_read_blobs_response::Response {
                digest: Some(proto_digest),
                data: vec![],
                compressor: 0,
                status: Some(crate::proto::google_rpc::Status {
                    code: GRPC_INVALID_ARGUMENT,
                    message: format!(
                        "digest.size_bytes does not match the blob_meta-recorded size ({row_size})"
                    ),
                    details: vec![],
                }),
            },
            Decision::MetaTransport {
                proto_digest,
                mapping,
            } => batch_read_blobs_response::Response {
                digest: Some(proto_digest),
                data: vec![],
                compressor: 0,
                status: Some(crate::proto::google_rpc::Status {
                    code: mapping.grpc_code,
                    message: mapping.message.to_owned(),
                    details: vec![],
                }),
            },
            Decision::Fetch {
                proto_digest,
                digest,
                row_size,
            } => match fetched.get(idx).and_then(|s| s.clone()) {
                Some(FetchOutcome::Body(body)) => {
                    // R2 corruption guard: row size must equal body length
                    // (defense-in-depth, same shape as ByteStream::Read
                    // step 5).
                    if (body.len() as u64) != row_size {
                        batch_read_blobs_response::Response {
                            digest: Some(proto_digest),
                            data: vec![],
                            compressor: 0,
                            status: Some(crate::proto::google_rpc::Status {
                                code: crate::error_map::GRPC_INTERNAL,
                                message: "blob_meta size_bytes does not match R2 body length"
                                    .to_owned(),
                                details: vec![],
                            }),
                        }
                    } else {
                        // Audit emission parity with `ByteStream::Read`: a
                        // successfully delivered batch slot emits
                        // `corelink.cas.read_completed`. The batch path
                        // delivers ALL bytes inline before sending the
                        // response (no Drop guard / abort window), so we
                        // emit the canonical `bytes_sent == size_bytes`
                        // shape here. The `slot_idx` token is mixed into
                        // the audit `id` derivation so per-slot
                        // duplicates of the SAME digest do NOT collide on
                        // the `audit_outbox` PK (codex round-3 P2 SEAL
                        // fix).
                        let body_len = body.len() as u64;
                        emit_read_completed_audit_post_stream_at_slot(
                            storage_ctx.tenant_id(),
                            pat_ctx.principal_id(),
                            pat_ctx.region(),
                            pat_ctx.request_id(),
                            &digest,
                            row_size,
                            body_len,
                            body_len,
                            1, // batch slot is delivered as one frame
                            idx,
                        );
                        batch_read_blobs_response::Response {
                            digest: Some(proto_digest),
                            data: body.to_vec(),
                            compressor: 0,
                            status: Some(crate::proto::google_rpc::Status {
                                code: 0,
                                message: "ok".to_owned(),
                                details: vec![],
                            }),
                        }
                    }
                }
                Some(FetchOutcome::R2Orphan) => {
                    // R2 orphan detection: alive blob_meta but R2 reported
                    // NotFound — emit the canonical SEV-2
                    // `corelink.cas.r2_orphan_detected` audit envelope so
                    // the GC reconciler signal lights up; response stays
                    // uniform 404.
                    emit_read_miss_audit_at_slot(
                        storage_ctx,
                        pat_ctx.principal_id(),
                        pat_ctx.region(),
                        pat_ctx.request_id(),
                        &digest,
                        MissReason::R2OrphanRow,
                        idx,
                    );
                    batch_read_blobs_response::Response {
                        digest: Some(proto_digest),
                        data: vec![],
                        compressor: 0,
                        status: Some(crate::proto::google_rpc::Status {
                            code: GRPC_NOT_FOUND,
                            message: "blob not found".to_owned(),
                            details: vec![],
                        }),
                    }
                }
                Some(FetchOutcome::Other(mapping)) => batch_read_blobs_response::Response {
                    digest: Some(proto_digest),
                    data: vec![],
                    compressor: 0,
                    status: Some(crate::proto::google_rpc::Status {
                        code: mapping.grpc_code,
                        message: mapping.message.to_owned(),
                        details: vec![],
                    }),
                },
                None => batch_read_blobs_response::Response {
                    digest: Some(proto_digest),
                    data: vec![],
                    compressor: 0,
                    status: Some(crate::proto::google_rpc::Status {
                        code: crate::error_map::GRPC_INTERNAL,
                        message: "BatchReadBlobs Pass 2 yielded no result for slot".to_owned(),
                        details: vec![],
                    }),
                },
            },
        };
        responses.push(resp);
    }
    responses
}
