//! `ByteStreamService` — `google.bytestream.ByteStream` gRPC service.
//! Implements `Write` (WI-S01-005) + `Read` (WI-S02-001) over the
//! canonical 1 MiB chunk semantics. `QueryWriteStatus` remains
//! `unimplemented` (resumable upload semantics land in S-05 multipart).
//!
//! Extracted from the monolithic `handler.rs` per Wave 33 Stream A2.1c
//! file-size discipline. The chunked-stream + Drop-guard fallback helpers
//! live in the sibling [`super::bytestream_stream`] module.

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::MetaStore;
use corelink_cas::r2_storage::R2Backend;
use tonic::{async_trait, Code, Request, Response, Status, Streaming};

use crate::capabilities::MAX_CAS_BLOB_SIZE_BYTES;
use crate::error_map::{
    HashErrorMapping, MetaErrorMapping, R2ErrorMapping, COR_CAS_BAD_DIGEST,
    COR_CAS_BAD_RESOURCE_NAME, COR_CAS_BLOB_TOO_LARGE,
};
use crate::orchestrator::{CasWriteOrchestrator, CommitPutPlan, OrchestratorError, OrphanReconciler};
use crate::pat::{AuthScope, PatValidator};
use crate::proto::bytestream::byte_stream_server::{ByteStream, ByteStreamServer};
use crate::proto::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use crate::read::{CasReadOrchestrator, ReadOutcome};

use super::audit_emit::{deterministic_audit_id, emit_read_miss_audit};
use super::bytestream_stream::{chunked_read_stream_with_audit, ReadAuditTail};
use super::helpers::{
    grpc_code_from_i32, make_status, miss_reason_label, miss_to_status_with_arm,
    parse_read_resource_name, parse_resource_name, pat_error_to_status, read_orch_error_to_status,
    region_str, slice_for_offset_limit,
};
use super::{Clock, HandlerCore};

/// Concrete `ByteStream` service. Implements `Write` (WI-S01-005) +
/// `Read` (WI-S02-001) over the canonical 1 MiB chunk semantics.
/// `QueryWriteStatus` remains `unimplemented` (resumable upload semantics
/// land in S-05 multipart).
pub struct ByteStreamService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    core: Arc<HandlerCore<V, B, M, R, C>>,
}

impl<V, B, M, R, C> std::fmt::Debug for ByteStreamService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStreamService").finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> Clone for ByteStreamService<V, B, M, R, C>
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

impl<V, B, M, R, C> ByteStreamService<V, B, M, R, C>
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
    pub fn into_server(self) -> ByteStreamServer<Self> {
        ByteStreamServer::new(self)
    }
}

#[async_trait]
impl<V, B, M, R, C> ByteStream for ByteStreamService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    type ReadStream = futures::stream::BoxStream<'static, Result<ReadResponse, Status>>;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        // Step 1 — auth + scope (cache-r required per WI §6.1.3, mapped
        // to typed AuthScope::CacheRead — see `pat.rs` rustdoc on the
        // canonical hyphen vs colon form: the typed enum's `as_str()`
        // emits the colon-form `cache:r` matching auth_stub_contract.md
        // §2 wire literal; auth_model.md §scope's hyphen-form is the
        // human-tier label and is normalized at the S-03 Clerk
        // adapter boundary).
        let (pat_ctx, storage_ctx) = self.core.authenticate(&request)?;
        pat_ctx
            .require_scope(AuthScope::CacheRead)
            .map_err(|e| pat_error_to_status(&e))?;

        // Step 2 — parse the ReadRequest's `resource_name` per REAPI v2.
        // Form: `[<instance_name>/]blobs/<digest_hash>/<size_bytes>`.
        let inner = request.into_inner();
        let parsed = parse_read_resource_name(&inner.resource_name)
            .map_err(|msg| make_status(Code::InvalidArgument, COR_CAS_BAD_RESOURCE_NAME, msg, 0))?;

        // Step 3 — REAPI v2 read_offset / read_limit semantics
        // validation. Negative read_offset / read_limit → OUT_OF_RANGE
        // / INVALID_ARGUMENT respectively (proto-level pre-check; the
        // orchestrator does not need to know about offsets — those are
        // a transport-layer chunking detail).
        if inner.read_offset < 0 {
            return Err(make_status(
                Code::OutOfRange,
                COR_CAS_BAD_RESOURCE_NAME,
                "ByteStream::Read read_offset is negative",
                0,
            ));
        }
        if inner.read_limit < 0 {
            return Err(make_status(
                Code::InvalidArgument,
                COR_CAS_BAD_RESOURCE_NAME,
                "ByteStream::Read read_limit is negative",
                0,
            ));
        }

        // Step 4 — orchestrate (AuthZ + R2 GET).
        let orch = CasReadOrchestrator::new(
            self.core.reader_arc().as_ref(),
            self.core.meta_arc().as_ref(),
        );
        let outcome = orch
            .read_blob(&storage_ctx, &parsed.digest)
            .await
            .map_err(|e| read_orch_error_to_status(&e))?;
        let (body, size_bytes) = match outcome {
            ReadOutcome::Hit { body, size_bytes } => (body, size_bytes),
            ReadOutcome::NotFound(reason) => {
                emit_read_miss_audit(
                    &storage_ctx,
                    pat_ctx.principal_id(),
                    pat_ctx.region(),
                    pat_ctx.request_id(),
                    &parsed.digest,
                    reason,
                );
                return Err(miss_to_status_with_arm(Some(miss_reason_label(reason))));
            }
        };

        // Step 5 — body integrity defense-in-depth: blob_meta-recorded
        // size MUST match observed bytes. A divergence here means an
        // out-of-band R2 mutation — surface as INTERNAL (programmer /
        // ops bug, not client-driven). Bit-rot detection at the BLAKE3
        // level is the client-verify layer (WI-S02-003); this assert is
        // a quick size-only sanity check that closes one specific
        // R2-corruption window without slowing the hot path.
        if body.len() as u64 != size_bytes {
            return Err(make_status(
                Code::Internal,
                crate::error_map::COR_INTERNAL,
                "blob_meta size_bytes does not match R2 body length (R2 corruption suspected)",
                size_bytes,
            ));
        }

        // Step 5b — size_bytes hint cross-check. If the client supplied
        // the optional `size_bytes` segment in resource_name, it MUST
        // match the blob_meta-recorded value; a mismatch indicates
        // tooling drift (or a confusion attack against the client) —
        // reject up-front so the client surfaces the inconsistency
        // (codex round-1 P3 fix: hint was previously ignored).
        if let Some(declared_size) = parsed.size_bytes {
            let declared_u64 = u64::try_from(declared_size).unwrap_or(u64::MAX);
            if declared_u64 != size_bytes {
                return Err(make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_RESOURCE_NAME,
                    "ByteStream::Read resource_name's declared size_bytes does not match the blob_meta-recorded size",
                    size_bytes,
                ));
            }
        }

        // Step 6 — apply read_offset + read_limit and stream chunks.
        let payload = slice_for_offset_limit(body, inner.read_offset, inner.read_limit)?;
        let payload_len = payload.len();
        let stream = chunked_read_stream_with_audit(
            payload,
            ReadAuditTail {
                tenant_id: storage_ctx.tenant_id(),
                principal_id: pat_ctx.principal_id(),
                region: pat_ctx.region(),
                client_request_id: pat_ctx.request_id().to_owned(),
                digest: parsed.digest,
                size_bytes,
                payload_len: payload_len as u64,
            },
        );

        Ok(Response::new(stream))
    }

    async fn write(
        &self,
        request: Request<Streaming<WriteRequest>>,
    ) -> Result<Response<WriteResponse>, Status> {
        // Auth.
        let (pat_ctx, storage_ctx) = self.core.authenticate(&request)?;
        pat_ctx
            .require_scope(AuthScope::CacheWrite)
            .map_err(|e| pat_error_to_status(&e))?;

        // Stream the chunks, byte-counting along the way. Abort early on
        // size-limit breach (no Content-Length trust per WI §9.4).
        let mut stream = request.into_inner();
        let mut accumulator = Vec::<u8>::new();
        let mut resource_name = String::new();
        let mut declared_digest: Option<Digest> = None;
        let mut declared_size: Option<i64> = None;
        let mut finished = false;
        let mut chunk_index: u64 = 0;
        let mut expected_offset: i64 = 0;
        let cap_usize = usize::try_from(MAX_CAS_BLOB_SIZE_BYTES).unwrap_or(usize::MAX);
        while let Some(chunk) = stream.message().await? {
            // Per `google.bytestream.ByteStream.Write` contract:
            // (a) `resource_name` MUST be set on the FIRST request and
            //     MUST match the first request's value on subsequent
            //     requests.
            // (b) `write_offset` MUST equal the running cumulative byte
            //     count.
            // (c) Any request after `finish_write=true` is an error (we
            //     break out of the loop on `finish_write` so this is
            //     enforced by absence).
            if chunk_index == 0 {
                if chunk.resource_name.is_empty() {
                    return Err(make_status(
                        Code::InvalidArgument,
                        COR_CAS_BAD_RESOURCE_NAME,
                        "ByteStream::Write resource_name MUST be set on the first request",
                        0,
                    ));
                }
                resource_name = chunk.resource_name.clone();
                let parsed = parse_resource_name(&resource_name).map_err(|msg| {
                    make_status(Code::InvalidArgument, COR_CAS_BAD_RESOURCE_NAME, msg, 0)
                })?;
                declared_digest = Some(parsed.digest);
                declared_size = Some(parsed.size_bytes);
            } else if !chunk.resource_name.is_empty() && chunk.resource_name != resource_name {
                // Mid-stream resource_name change. The Bytestream contract
                // says it "MUST match" the first value if set; rejecting
                // here protects against client confusion / wire-level
                // tampering.
                return Err(make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_RESOURCE_NAME,
                    "ByteStream::Write subsequent chunk's resource_name disagrees with the first chunk's value",
                    accumulator.len() as u64,
                ));
            }
            if chunk.write_offset != expected_offset {
                return Err(make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_RESOURCE_NAME,
                    "ByteStream::Write chunk's write_offset disagrees with the cumulative byte count of prior chunks",
                    accumulator.len() as u64,
                ));
            }
            if accumulator.len().saturating_add(chunk.data.len()) > cap_usize {
                return Err(make_status(
                    Code::ResourceExhausted,
                    COR_CAS_BLOB_TOO_LARGE,
                    "ByteStream::Write body exceeds 5 MiB single-blob cap (use multipart in S-05)",
                    cap_usize as u64,
                ));
            }
            accumulator.extend_from_slice(&chunk.data);
            expected_offset =
                expected_offset.saturating_add(i64::try_from(chunk.data.len()).unwrap_or(i64::MAX));
            chunk_index = chunk_index.saturating_add(1);
            if chunk.finish_write {
                finished = true;
                // Drain to assert the client really sent end-of-stream.
                // Any further `WriteRequest` after `finish_write=true` is
                // a Bytestream protocol violation per `google.bytestream`
                // contract — reject explicitly so clients with retry-loop
                // bugs do not silently lose post-finish data.
                if let Some(extra) = stream.message().await? {
                    let _ = extra; // diagnostic; do NOT echo client bytes.
                    return Err(make_status(
                        Code::InvalidArgument,
                        COR_CAS_BAD_RESOURCE_NAME,
                        "ByteStream::Write received an additional chunk after finish_write=true",
                        accumulator.len() as u64,
                    ));
                }
                break;
            }
        }
        if !finished {
            return Err(make_status(
                Code::InvalidArgument,
                COR_CAS_BAD_RESOURCE_NAME,
                "ByteStream::Write closed without finish_write=true on the trailing request",
                accumulator.len() as u64,
            ));
        }
        let digest = declared_digest.ok_or_else(|| {
            make_status(
                Code::InvalidArgument,
                COR_CAS_BAD_RESOURCE_NAME,
                "ByteStream::Write resource_name missing on the first request",
                0,
            )
        })?;
        if let Some(declared) = declared_size {
            let actual = i64::try_from(accumulator.len()).unwrap_or(i64::MAX);
            if declared != actual {
                return Err(make_status(
                    Code::InvalidArgument,
                    COR_CAS_BAD_DIGEST,
                    "ByteStream::Write resource_name's declared size_bytes does not match the streamed body length",
                    accumulator.len() as u64,
                ));
            }
        }

        let body = Bytes::from(accumulator);
        let now_ms = self.core.clock_ref().now_ms();
        let canonical = format!("blake3:{}", digest.to_hex());
        let audit_request_id = crate::orchestrator::audit_request_id_for_blob(
            pat_ctx.request_id(),
            storage_ctx.tenant_id(),
            &canonical,
        );
        let plan = CommitPutPlan {
            ctx: &storage_ctx,
            body: body.clone(),
            claimed_digest: digest,
            size_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
            digest_canonical_text: canonical,
            principal_id: pat_ctx.principal_id(),
            region_str: region_str(pat_ctx.region()),
            client_request_id: pat_ctx.request_id(),
            audit_request_id,
            audit_id: deterministic_audit_id(
                pat_ctx.request_id(),
                storage_ctx.tenant_id(),
                &digest,
            ),
            now_ms,
        };

        let orch = CasWriteOrchestrator::new(
            self.core.writer_arc(),
            self.core.meta_arc().as_ref(),
            self.core.reconciler_arc().as_ref(),
        );

        let result = orch.commit_put(plan).await;
        let committed_size = match result {
            Ok(_) => i64::try_from(body.len()).unwrap_or(i64::MAX),
            Err(OrchestratorError::HashMismatch(hm)) => {
                let m = hm.mapping();
                return Err(make_status(
                    grpc_code_from_i32(m.grpc_code),
                    m.taxonomy_code,
                    m.message,
                    body.len() as u64,
                ));
            }
            Err(OrchestratorError::R2(e)) => {
                let m = e.mapping();
                return Err(make_status(
                    grpc_code_from_i32(m.grpc_code),
                    m.taxonomy_code,
                    m.message,
                    body.len() as u64,
                ));
            }
            Err(OrchestratorError::Meta(e)) => {
                let m = e.mapping();
                return Err(make_status(
                    grpc_code_from_i32(m.grpc_code),
                    m.taxonomy_code,
                    m.message,
                    body.len() as u64,
                ));
            }
            Err(OrchestratorError::AuditEnvelopeSerialize(_)) => {
                return Err(make_status(
                    Code::Internal,
                    crate::error_map::COR_INTERNAL,
                    "audit envelope JSON serialization failed",
                    body.len() as u64,
                ));
            }
        };

        Ok(Response::new(WriteResponse { committed_size }))
    }

    async fn query_write_status(
        &self,
        _request: Request<QueryWriteStatusRequest>,
    ) -> Result<Response<QueryWriteStatusResponse>, Status> {
        Err(Status::unimplemented(
            "ByteStream::QueryWriteStatus lands in WI-S02-001 (resumable upload semantics)",
        ))
    }
}
