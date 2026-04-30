//! Tonic-based gRPC handlers for `BatchUpdateBlobs`, `Capabilities`, and
//! `ByteStream::Write` (host-server feature).
//!
//! ## Surface
//!
//! - [`CasWriteService`] — implements
//!   `build.bazel.remote.execution.v2.ContentAddressableStorage` (write half).
//! - [`CapabilitiesService`] — implements `Capabilities.GetCapabilities`.
//! - [`ByteStreamService`] — implements `google.bytestream.ByteStream`.
//!
//! All three services share a single `Arc<HandlerCore<…>>` so the auth
//! validator + storage + meta + orphan reconciler can be wired once at
//! deploy time and the gRPC services can be instantiated cheaply per RPC.
//!
//! ## Auth seam
//!
//! Authentication runs as the first step of each RPC. We extract the
//! `authorization: Bearer <token>` header from the gRPC `Metadata`, run
//! the injected [`PatValidator`], and bind the resulting
//! [`crate::pat::TenantContext`]. From there a
//! [`corelink_worker::TenantCtx`] is derived (TDK lookup + prefix
//! derivation runs inline at the start of each authenticated RPC).
//!
//! ## TDK plumbing
//!
//! The auth-stub layer (S-01) does NOT carry the TDK; the handler holds an
//! `Arc<TenantDerivationKey>` (production: read once at deploy from
//! Cloudflare Secrets) and uses it together with the validated `tenant_id`
//! to construct the storage-side `TenantCtx`. This matches `tenant.rs`'s
//! "TDK lives in the auth/dispatcher layer; per-request ctx never carries
//! it" rule.

#![allow(
    clippy::result_large_err,
    reason = "tonic::Status is canonical wire-error type sized by tonic; \
              boxing here would break tonic-server trait bounds"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::MetaStore;
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{R2Backend, R2Reader, R2Writer};
use corelink_worker::{Region, TenantCtx as StorageTenantCtx};
use tonic::{async_trait, Code, Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::capabilities::{
    server_capabilities, MAX_BATCH_TOTAL_SIZE_BYTES, MAX_CAS_BLOB_SIZE_BYTES,
};
use crate::error_map::{
    miss_mapping, FindMissingErrorMapping, HashErrorMapping, MetaErrorMapping, R2ErrorMapping,
    ReadErrorMapping, COR_AUTH_PAT_INVALID, COR_AUTH_SCOPE_INSUFFICIENT, COR_CAS_BAD_DIGEST,
    COR_CAS_BAD_RESOURCE_NAME, COR_CAS_BATCH_SIZE_EXCEEDED, COR_CAS_BATCH_TOO_LARGE,
    COR_CAS_BLOB_TOO_LARGE, COR_CAS_DIGEST_FUNCTION_UNSUPPORTED, GRPC_INVALID_ARGUMENT,
    GRPC_NOT_FOUND, GRPC_PERMISSION_DENIED, GRPC_RESOURCE_EXHAUSTED, GRPC_UNAUTHENTICATED,
};
use crate::find_missing::{FindMissingOrchestrator, MAX_FIND_MISSING_BATCH_SIZE};
use crate::orchestrator::{
    CasPutOutcome, CasWriteOrchestrator, CommitPutPlan, OrchestratorError, OrphanReconciler,
};
use crate::pat::{AuthScope, AuthStubError, PatValidator};
use crate::proto::reapi as reapi_proto;
use crate::proto::reapi::capabilities_server::{Capabilities, CapabilitiesServer};
use crate::proto::reapi::content_addressable_storage_server::{
    ContentAddressableStorage, ContentAddressableStorageServer,
};
use crate::proto::reapi::{
    batch_read_blobs_response, batch_update_blobs_response, BatchReadBlobsRequest,
    BatchReadBlobsResponse, BatchUpdateBlobsRequest, BatchUpdateBlobsResponse,
    Digest as ProtoDigest, FindMissingBlobsRequest, FindMissingBlobsResponse,
    GetCapabilitiesRequest, ServerCapabilities,
};
use crate::read::{CasReadOrchestrator, MissReason, ReadOutcome};

/// Canonical chunk size for `ByteStream::Read` streaming responses.
///
/// 1 MiB matches the canonical REAPI v2 conformance recommendation per
/// `remote_cache_product_profile.md §7.2.1` and stays well below the
/// gRPC default `max_decoding_message_size` (4 MiB) so existing clients
/// (Bazel/Buck2) decode without bumping limits. Worker peak memory per
/// concurrent request is therefore bounded by `chunk + framing overhead`
/// — ≪ the 50 MiB hard ceiling per WI-S02-001 §10.1.3.
///
/// ## Streaming surface scope (WI-S02-001 v1.1.0 §6.1.5 clarification)
///
/// The current [`crate::read::CasReadOrchestrator`] returns the full
/// blob body via [`corelink_worker::storage::r2::R2Reader::get`] which
/// itself returns `bytes::Bytes` (full materialization at the storage
/// adapter seam). Combined with the S-01 single-blob 5 MiB cap
/// (`SINGLE_BLOB_LIMIT_BYTES`), Worker peak memory per concurrent read
/// is bounded by `5 MiB body + 1 MiB chunk overhead = ~6 MiB` — well
/// below the 50 MiB hard ceiling for any S-02 supported blob size.
///
/// True end-to-end streaming (R2 SDK streamed response forwarded
/// byte-for-byte without an intermediate materialization) requires
/// extending `R2Backend::get` → `R2Backend::get_stream`; that change
/// lands in **WI-S05-005** (multipart read) alongside
/// `R2Backend::put_multipart`. Until then the 1 GiB AC scenario in
/// §10.1.3 is unreachable by construction (writes are capped at 5 MiB
/// in S-01).
pub const READ_CHUNK_SIZE_BYTES: usize = 1024 * 1024;

/// Clock seam — production wires `std::time::SystemTime`; tests pin a fixed
/// instant via the `Clock` trait. Pure trait so the handler can compile to
/// wasm32 without pulling chrono into the surface; we only need Unix ms
/// (the audit envelope deliberately omits CE `time` — see
/// [`crate::audit`] module rustdoc).
pub trait Clock: Send + Sync {
    /// Wall-clock instant in Unix milliseconds.
    fn now_ms(&self) -> u64;
}

/// Wall-clock impl backed by `std::time::SystemTime`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

/// Convenience type holding the cross-cutting handler dependencies. All
/// three gRPC services borrow this `Arc` — instantiating a service is
/// cheap (no per-RPC allocation in the hot path).
///
/// ## Read seam (WI-S02-001)
///
/// Adds an [`R2Reader`] alongside the existing `R2Writer`. Both are
/// pinned to the same `Region` (the auth dispatcher must hand the
/// handler a region-pinned core) and share the same backend `Arc<B>`.
/// The reader is constructed once at deploy time and cloned cheaply
/// per-request through the per-service `Arc<HandlerCore>`.
pub struct HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    pat: V,
    tdk: Arc<TenantDerivationKey>,
    writer: Arc<R2Writer<B>>,
    reader: Arc<R2Reader<B>>,
    meta: Arc<M>,
    reconciler: Arc<R>,
    clock: C,
}

impl<V, B, M, R, C> std::fmt::Debug for HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerCore").finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    /// Construct a fresh handler core. The six dependencies (auth, TDK,
    /// writer, reader, meta, reconciler) are sharable across service
    /// clones. `writer.region()` and `reader.region()` MUST agree —
    /// the constructor asserts via `debug_assert_eq!` so any wiring bug
    /// surfaces in CI rather than at first cross-tenant request.
    #[must_use]
    pub fn new(
        pat: V,
        tdk: Arc<TenantDerivationKey>,
        writer: Arc<R2Writer<B>>,
        reader: Arc<R2Reader<B>>,
        meta: Arc<M>,
        reconciler: Arc<R>,
        clock: C,
    ) -> Self {
        debug_assert_eq!(
            writer.region(),
            reader.region(),
            "HandlerCore: writer.region() and reader.region() must agree (caller wiring bug)"
        );
        Self {
            pat,
            tdk,
            writer,
            reader,
            meta,
            reconciler,
            clock,
        }
    }

    /// Borrow the metadata store. Used by the HTTP read handler to
    /// invoke the [`crate::read::CasReadOrchestrator`] without taking
    /// ownership of the core.
    #[must_use]
    pub fn meta(&self) -> &M {
        self.meta.as_ref()
    }

    /// Borrow the R2 reader.
    #[must_use]
    pub fn reader(&self) -> &R2Reader<B> {
        self.reader.as_ref()
    }

    /// Borrow the PAT validator.
    #[must_use]
    pub const fn pat(&self) -> &V {
        &self.pat
    }

    /// Borrow the TDK so HTTP handlers can derive a [`StorageTenantCtx`].
    #[must_use]
    pub fn tdk(&self) -> &TenantDerivationKey {
        self.tdk.as_ref()
    }

    /// Borrow the clock.
    #[must_use]
    pub const fn clock(&self) -> &C {
        &self.clock
    }

    /// Authenticate a tonic `Request`, returning the parsed
    /// `TenantContext` plus the storage-side `TenantCtx` (which carries
    /// the HMAC-derived prefix).
    fn authenticate<T>(
        &self,
        req: &Request<T>,
    ) -> Result<(crate::pat::TenantContext, StorageTenantCtx), Status> {
        let token = extract_bearer(req).map_err(|e| pat_error_to_status(&e))?;
        let request_id = extract_request_id(req);
        let pat_ctx = self
            .pat
            .authenticate(&token, &request_id)
            .map_err(|e| pat_error_to_status(&e))?;
        let storage_ctx = StorageTenantCtx::new(&self.tdk, pat_ctx.tenant_id(), pat_ctx.region());
        Ok((pat_ctx, storage_ctx))
    }
}

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
            &self.core.writer,
            self.core.meta.as_ref(),
            self.core.reconciler.as_ref(),
        );
        const BOUNDED_CONCURRENCY: usize = 16;
        use futures::stream::StreamExt;
        let principal_id = pat_ctx.principal_id();
        let region = pat_ctx.region();
        let request_id = pat_ctx.request_id().to_owned();
        let storage_ctx_ref = &storage_ctx;
        let clock_ref = &self.core.clock;
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

        // Step 2 — digest-function negotiation (BLAKE3 only; same
        // contract as BatchUpdateBlobs).
        if inner.digest_function != 0
            && inner.digest_function != crate::capabilities::DigestFunction::Blake3 as i32
        {
            return Err(make_status(
                Code::InvalidArgument,
                COR_CAS_DIGEST_FUNCTION_UNSUPPORTED,
                "BatchReadBlobsRequest.digest_function declares a non-BLAKE3 hash; CoreLink advertises BLAKE3 only via Capabilities.GetCapabilities",
                0,
            ));
        }

        // Step 2b — `acceptable_compressors` honor (codex round-2 P2
        // SEAL fix). REAPI v2 §`BatchReadBlobsRequest`: an empty
        // `acceptable_compressors` list MUST be treated as "client
        // accepts the default IDENTITY encoding"; a non-empty list
        // restricts the server's permitted response encodings. CoreLink
        // S-01 advertises IDENTITY only via `CacheCapabilities` (no
        // ZSTD/DEFLATE/BROTLI server-side compression yet). If a
        // client supplies a non-empty list that does NOT include
        // `Compressor.IDENTITY` (= 0) the server cannot satisfy the
        // request — surface `FAILED_PRECONDITION` with a hint
        // pointing at `GetCapabilities`. (Compressed batch responses
        // ship with the multipart write surface in WI-S05-005 / the
        // `compressed-blobs` ByteStream resources.)
        if !inner.acceptable_compressors.is_empty()
            && !inner.acceptable_compressors.contains(&0)
        {
            return Err(make_status(
                Code::FailedPrecondition,
                crate::error_map::COR_CAS_COMPRESSOR_UNSUPPORTED,
                "BatchReadBlobsRequest.acceptable_compressors must include Compressor.IDENTITY (= 0) or be empty; CoreLink S-01 supports IDENTITY only (compressed batch responses ship in WI-S05-005)",
                0,
            ));
        }

        // Step 3 — batch-size cap (canonical 1000 — same as
        // FindMissingBlobs; REAPI v2 recommendation).
        if inner.digests.len() > MAX_FIND_MISSING_BATCH_SIZE {
            return Err(make_status(
                Code::OutOfRange,
                COR_CAS_BATCH_SIZE_EXCEEDED,
                "BatchReadBlobsRequest carries more digests than the canonical CoreLink batch cap (1000)",
                inner.digests.len() as u64,
            ));
        }

        // Step 4 — TWO-PASS dispatch (codex round-1 P0 fix —
        // memory-bounded by construction):
        //
        //   Pass 1: parallel D1 lookups for `(presence, size_bytes,
        //           declared-size-cross-check)`. NO body bytes
        //           materialized. Cumulative `aggregate_running_bytes`
        //           is computed in input order from the size_bytes
        //           returned per row; entries for which inclusion
        //           would exceed the canonical 4 MiB aggregate cap
        //           are flagged for FAILED_PRECONDITION fallback —
        //           the body is NEVER fetched.
        //   Pass 2: parallel R2 GET only for the slots that fit. Per
        //           slot we additionally cross-check
        //           `body.len() == size_bytes` (R2 corruption guard,
        //           defense-in-depth — same shape as ByteStream::Read
        //           handler step 5).
        //
        // Worst-case Worker memory: 1000 D1 hits + at most ⌈4 MiB /
        // smallest hit⌉ R2 bodies in flight → bounded by 4 MiB hot
        // window + per-row metadata, well under the 50 MiB ceiling
        // even at the 1000-digest batch cap. The earlier
        // single-pass variant could materialize 1000 × 4 MiB = 4 GiB
        // before the aggregate-cap trim ran (codex round-1 P0).
        //
        // Per-blob caller-supplied `size_bytes` cross-check
        // (codex round-1 P1 fix): a malformed `(hash, wrong_size)`
        // tuple is a tooling bug — REAPI v2 §
        // "BatchReadBlobsRequest.Digest" mandates size_bytes match
        // the body. We surface the mismatch as INVALID_ARGUMENT for
        // that per-blob slot (parity with `ByteStream::Read` step
        // 5b's stricter check).
        let inline_cap_bytes: u64 =
            u64::try_from(MAX_BATCH_TOTAL_SIZE_BYTES).unwrap_or(u64::MAX);
        let single_blob_inline_cap_bytes: u64 = inline_cap_bytes; // 4 MiB

        // 4a. Pre-validate digests + decode hex per-index.
        #[derive(Clone)]
        enum SlotState {
            // Digest hex was malformed; surface
            // INVALID_ARGUMENT per-blob.
            BadDigest {
                proto_digest: Option<ProtoDigest>,
                message: String,
            },
            // Well-formed; needs the D1 metadata pass.
            Pending {
                proto_digest: ProtoDigest,
                digest: Digest,
            },
        }
        let n_input = inner.digests.len();
        let mut slots: Vec<SlotState> = Vec::with_capacity(n_input);
        for pd in inner.digests.into_iter() {
            let parsed_digest = Digest::from_hex(&pd.hash);
            let pd_size = pd.size_bytes;
            if pd_size < 0 {
                slots.push(SlotState::BadDigest {
                    proto_digest: Some(pd),
                    message: "digest.size_bytes is negative".to_owned(),
                });
                continue;
            }
            match parsed_digest {
                Err(_) => {
                    slots.push(SlotState::BadDigest {
                        proto_digest: Some(pd),
                        message: "digest hex is malformed (expected 64 lowercase hex chars)".to_owned(),
                    });
                }
                Ok(d) => {
                    slots.push(SlotState::Pending {
                        proto_digest: pd,
                        digest: d,
                    });
                }
            }
        }

        // 4b. Pass 1 — bounded-parallel D1 lookups for size_bytes +
        // miss-reason. Per ADR-0028 the wire response for every miss
        // arm is uniform `NOT_FOUND`, but the audit-emit envelope MUST
        // disambiguate NeverExisted (low-severity) from Tombstoned
        // (legitimate post-GC) so the S-09 forensics pipeline can
        // distinguish ordinary cache-miss probes from genuine
        // tombstone-read attempts (codex round-2 P2 SEAL fix).
        //
        // Each future yields `(idx, Result<MetaPass1Result, ErrorMapping>)`.
        #[derive(Clone, Copy, Debug)]
        enum MetaPass1Result {
            /// Row absent OR exists in another tenant's scope (uniform
            /// per ADR-0028 — no oracle).
            NeverExisted,
            /// Row exists but `deleted_at` is set.
            Tombstoned,
            /// Row exists + alive; size_bytes is the canonical size.
            Alive { size_bytes: u64 },
        }
        use futures::stream::StreamExt;
        const BOUNDED_CONCURRENCY: usize = 16;
        let pending_indices: Vec<(usize, Digest)> = slots
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| match s {
                SlotState::Pending { digest, .. } => Some((idx, *digest)),
                SlotState::BadDigest { .. } => None,
            })
            .collect();
        let meta_ref: &M = self.core.meta.as_ref();
        let tenant_id = storage_ctx.tenant_id();
        let mut size_results: Vec<
            Option<Result<MetaPass1Result, crate::error_map::ErrorMapping>>,
        > = (0..n_input).map(|_| None).collect();
        let mut size_stream = futures::stream::iter(pending_indices.clone())
            .map(move |(idx, digest)| async move {
                let key = corelink_meta::BlobMetaKey::new(tenant_id, digest);
                let row_opt = meta_ref.get(&key).await;
                let mapped: Result<MetaPass1Result, crate::error_map::ErrorMapping> = match row_opt {
                    Err(e) => Err(e.mapping()),
                    Ok(None) => Ok(MetaPass1Result::NeverExisted),
                    Ok(Some(row)) => {
                        if row.is_alive() {
                            Ok(MetaPass1Result::Alive {
                                size_bytes: row.size_bytes,
                            })
                        } else {
                            Ok(MetaPass1Result::Tombstoned)
                        }
                    }
                };
                (idx, mapped)
            })
            .buffer_unordered(BOUNDED_CONCURRENCY);
        while let Some((idx, mapped)) = size_stream.next().await {
            if let Some(slot) = size_results.get_mut(idx) {
                *slot = Some(mapped);
            }
        }

        // 4c. Decide per-slot which bodies to fetch (running aggregate
        // cap in input order; missed/oversize/aggregate-cap-rejected
        // slots flagged here so Pass 2 only does R2 GETs for hits
        // that fit).
        #[derive(Clone)]
        enum Decision {
            BadDigest {
                proto_digest: Option<ProtoDigest>,
                message: String,
                grpc_code: i32,
            },
            Miss {
                proto_digest: ProtoDigest,
                digest: Digest,
                reason: MissReason,
            },
            ExceedsSingleCap {
                proto_digest: ProtoDigest,
            },
            ExceedsAggregate {
                proto_digest: ProtoDigest,
            },
            CallerSizeMismatch {
                proto_digest: ProtoDigest,
                row_size: u64,
            },
            MetaTransport {
                proto_digest: ProtoDigest,
                mapping: crate::error_map::ErrorMapping,
            },
            Fetch {
                proto_digest: ProtoDigest,
                digest: Digest,
                row_size: u64,
            },
        }
        let mut running: u64 = 0;
        let mut decisions: Vec<Decision> = Vec::with_capacity(n_input);
        for (idx, slot) in slots.iter().enumerate() {
            match slot {
                SlotState::BadDigest {
                    proto_digest,
                    message,
                } => {
                    decisions.push(Decision::BadDigest {
                        proto_digest: proto_digest.clone(),
                        message: message.clone(),
                        grpc_code: GRPC_INVALID_ARGUMENT,
                    });
                }
                SlotState::Pending {
                    proto_digest,
                    digest,
                } => {
                    let mapped = match size_results.get(idx).and_then(|s| s.clone()) {
                        Some(v) => v,
                        None => {
                            // Defensive — should never happen as Pass 1
                            // fills every Pending slot. Treat as transport
                            // failure for safety.
                            decisions.push(Decision::MetaTransport {
                                proto_digest: proto_digest.clone(),
                                mapping: crate::error_map::ErrorMapping {
                                    taxonomy_code: crate::error_map::COR_INTERNAL,
                                    grpc_code: crate::error_map::GRPC_INTERNAL,
                                    message: "BatchReadBlobs Pass 1 yielded no result for slot",
                                },
                            });
                            continue;
                        }
                    };
                    match mapped {
                        Err(mapping) => {
                            decisions.push(Decision::MetaTransport {
                                proto_digest: proto_digest.clone(),
                                mapping,
                            });
                        }
                        Ok(MetaPass1Result::NeverExisted) => {
                            decisions.push(Decision::Miss {
                                proto_digest: proto_digest.clone(),
                                digest: *digest,
                                reason: MissReason::NeverExisted,
                            });
                        }
                        Ok(MetaPass1Result::Tombstoned) => {
                            decisions.push(Decision::Miss {
                                proto_digest: proto_digest.clone(),
                                digest: *digest,
                                reason: MissReason::Tombstoned,
                            });
                        }
                        Ok(MetaPass1Result::Alive {
                            size_bytes: row_size,
                        }) => {
                            // Caller-supplied size cross-check
                            // (codex round-1 P1).
                            let declared = u64::try_from(proto_digest.size_bytes)
                                .unwrap_or(u64::MAX);
                            if declared != row_size {
                                decisions.push(Decision::CallerSizeMismatch {
                                    proto_digest: proto_digest.clone(),
                                    row_size,
                                });
                                continue;
                            }
                            // Single-blob inline cap.
                            if row_size > single_blob_inline_cap_bytes {
                                decisions.push(Decision::ExceedsSingleCap {
                                    proto_digest: proto_digest.clone(),
                                });
                                continue;
                            }
                            // Aggregate cap (running, in input order).
                            let after = running.saturating_add(row_size);
                            if after > inline_cap_bytes {
                                decisions.push(Decision::ExceedsAggregate {
                                    proto_digest: proto_digest.clone(),
                                });
                                continue;
                            }
                            running = after;
                            decisions.push(Decision::Fetch {
                                proto_digest: proto_digest.clone(),
                                digest: *digest,
                                row_size,
                            });
                        }
                    }
                }
            }
        }

        // 4d. Pass 2 — bounded-parallel R2 GETs ONLY for the Fetch
        // slots. Worst-case in-flight bytes: BOUNDED_CONCURRENCY ×
        // single-blob 4 MiB cap = 64 MiB; the aggregate-cap trim
        // means total RETURNED bytes never exceed 4 MiB — but we do
        // need to keep the in-flight working set bounded. We chunk
        // BOUNDED_CONCURRENCY at 8 here (lower than the Pass 1
        // limit) so peak Worker memory stays comfortably under the
        // 50 MiB ceiling even on a fully-saturated batch. Note: the
        // production deployment will further cap by the binding
        // adapter's R2 client connection pool.
        let reader_ref: &corelink_worker::storage::r2::R2Reader<B> =
            self.core.reader.as_ref();
        const FETCH_CONCURRENCY: usize = 8;
        let storage_ctx_ref = &storage_ctx;
        let fetch_targets: Vec<(usize, Digest)> = decisions
            .iter()
            .enumerate()
            .filter_map(|(idx, d)| match d {
                Decision::Fetch { digest, .. } => Some((idx, *digest)),
                _ => None,
            })
            .collect();
        // Pass 2 result type: a typed enum so the orphan condition
        // (alive blob_meta + `R2Error::NotFound`) is structurally
        // distinguishable from generic Err mappings — the earlier
        // pointer-equality sentinel approach was fragile to
        // refactors that rebuilt the string (codex round-3 P2 SEAL
        // fix).
        #[derive(Clone)]
        enum FetchOutcome {
            Body(Bytes),
            R2Orphan,
            Other(crate::error_map::ErrorMapping),
        }
        let mut fetched: Vec<Option<FetchOutcome>> = (0..n_input).map(|_| None).collect();
        let mut fetch_stream = futures::stream::iter(fetch_targets)
            .map(move |(idx, digest)| async move {
                let res = reader_ref.get(storage_ctx_ref, &digest).await;
                let mapped: FetchOutcome = match res {
                    Ok(b) => FetchOutcome::Body(b),
                    // R2 orphan row — alive blob_meta + R2 NotFound.
                    // Per ADR-0028 surface as uniform 404 on the
                    // wire (GC reconciler in S-06 repairs the
                    // orphan window) AND tag the slot so the
                    // response-composition phase emits the
                    // canonical SEV-2 `r2_orphan_detected` audit.
                    Err(corelink_worker::storage::error::R2Error::NotFound) => {
                        FetchOutcome::R2Orphan
                    }
                    Err(e) => FetchOutcome::Other(e.mapping()),
                };
                (idx, mapped)
            })
            .buffer_unordered(FETCH_CONCURRENCY);
        while let Some((idx, mapped)) = fetch_stream.next().await {
            if let Some(slot) = fetched.get_mut(idx) {
                *slot = Some(mapped);
            }
        }

        // 4e. Compose final responses in input order. Per-slot audit
        // emission lands here so the S-09 forensics / chain consumer
        // sees `corelink.cas.read_miss` (uniform NeverExisted +
        // CrossTenantMasked + Tombstoned per ADR-0028) for batch
        // miss probes AND `corelink.cas.read_completed` for
        // successful per-blob delivery — at parity with
        // `ByteStream::Read` (codex round-2 P1 SEAL fix).
        let mut responses: Vec<batch_read_blobs_response::Response> =
            Vec::with_capacity(n_input);
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
                    // Audit emission parity with `ByteStream::Read`
                    // (ADR-0028 + WI-S02-002 §11): each per-slot
                    // miss surfaces as the canonical CE event type
                    // for its [`MissReason`] — `read_miss` for
                    // NeverExisted (low-severity, conflated with
                    // CrossTenantMasked per ADR-0028; S-09
                    // reclassifier folds true cross-tenant via the
                    // global digest index), `tombstoned_read_attempt`
                    // for legitimate post-GC reads. The wire
                    // response stays the uniform 404 (codex round-2
                    // P2 SEAL fix). The `slot_idx` token is mixed
                    // into the audit `id` derivation so duplicate
                    // digests in the same batch do NOT collide on
                    // the `audit_outbox` PK (codex round-3 P2 SEAL
                    // fix).
                    emit_read_miss_audit_at_slot(
                        &storage_ctx,
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
                Decision::ExceedsSingleCap { proto_digest } => {
                    batch_read_blobs_response::Response {
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
                    }
                }
                Decision::ExceedsAggregate { proto_digest } => {
                    batch_read_blobs_response::Response {
                        digest: Some(proto_digest),
                        data: vec![],
                        compressor: 0,
                        status: Some(crate::proto::google_rpc::Status {
                            code: 9, // FAILED_PRECONDITION
                            message: "blob skipped: BatchReadBlobs aggregate cap (4 MiB) exceeded; use ByteStream::Read for this digest"
                                .to_owned(),
                            details: vec![],
                        }),
                    }
                }
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
                        // R2 corruption guard: row size must equal
                        // body length (defense-in-depth, same shape
                        // as ByteStream::Read step 5).
                        if (body.len() as u64) != row_size {
                            batch_read_blobs_response::Response {
                                digest: Some(proto_digest),
                                data: vec![],
                                compressor: 0,
                                status: Some(crate::proto::google_rpc::Status {
                                    code: crate::error_map::GRPC_INTERNAL,
                                    message:
                                        "blob_meta size_bytes does not match R2 body length"
                                            .to_owned(),
                                    details: vec![],
                                }),
                            }
                        } else {
                            // Audit emission parity with
                            // `ByteStream::Read`: a successfully
                            // delivered batch slot emits
                            // `corelink.cas.read_completed`. The
                            // batch path delivers ALL bytes inline
                            // before sending the response (no Drop
                            // guard / abort window), so we emit the
                            // canonical `bytes_sent == size_bytes`
                            // shape here. The `slot_idx` token is
                            // mixed into the audit `id` derivation
                            // so per-slot duplicates of the SAME
                            // digest do NOT collide on the
                            // `audit_outbox` PK (codex round-3 P2
                            // SEAL fix).
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
                        // R2 orphan detection: alive blob_meta but
                        // R2 reported NotFound — emit the canonical
                        // SEV-2 `corelink.cas.r2_orphan_detected`
                        // audit envelope so the GC reconciler signal
                        // lights up; response stays uniform 404.
                        emit_read_miss_audit_at_slot(
                            &storage_ctx,
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
                            message: "BatchReadBlobs Pass 2 yielded no result for slot"
                                .to_owned(),
                            details: vec![],
                        }),
                    },
                },
            };
            responses.push(resp);
        }

        Ok(Response::new(BatchReadBlobsResponse { responses }))
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
        let mut parsed: Vec<(Digest, Option<u64>)> =
            Vec::with_capacity(inner.blob_digests.len());
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
            #[allow(
                clippy::cast_sign_loss,
                reason = "non-negative checked above"
            )]
            let declared = pd.size_bytes as u64;
            parsed.push((d, Some(declared)));
        }

        // Step 5 — dispatch the find_missing orchestrator (REAPI
        // size-aware variant).
        let orch = FindMissingOrchestrator::new(self.core.meta.as_ref());
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
        let mut missing_blob_digests: Vec<ProtoDigest> =
            Vec::with_capacity(outcome.missing.len());
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

        Ok(Response::new(FindMissingBlobsResponse {
            missing_blob_digests,
        }))
    }
}

/// Concrete `Capabilities` service.
pub struct CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    core: Arc<HandlerCore<V, B, M, R, C>>,
}

impl<V, B, M, R, C> std::fmt::Debug for CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilitiesService")
            .finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> Clone for CapabilitiesService<V, B, M, R, C>
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

impl<V, B, M, R, C> CapabilitiesService<V, B, M, R, C>
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
    pub fn into_server(self) -> CapabilitiesServer<Self> {
        CapabilitiesServer::new(self)
    }
}

#[async_trait]
impl<V, B, M, R, C> Capabilities for CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    async fn get_capabilities(
        &self,
        _request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<ServerCapabilities>, Status> {
        // GetCapabilities is unauthenticated by REAPI v2 conformance — every
        // client probes capabilities BEFORE attaching credentials so the
        // wire-side discovery path is symmetric. Per CoreLink S-01: we still
        // emit canonical values regardless of caller.
        let caps = server_capabilities();
        let proto = reapi_proto::ServerCapabilities {
            cache_capabilities: Some(reapi_proto::CacheCapabilities {
                digest_functions: caps
                    .cache_capabilities
                    .digest_functions
                    .iter()
                    .map(|f| *f as i32)
                    .collect(),
                action_cache_update_capabilities: None,
                cache_priority_capabilities: None,
                max_batch_total_size_bytes: caps.cache_capabilities.max_batch_total_size_bytes,
                symlink_absolute_path_strategy: 0,
                supported_compressors: vec![],
                supported_batch_update_compressors: vec![],
                max_cas_blob_size_bytes: caps.cache_capabilities.max_cas_blob_size_bytes,
            }),
            execution_capabilities: None,
            deprecated_api_version: None,
            low_api_version: Some(crate::proto::semver::SemVer {
                major: caps.api_version.major,
                minor: caps.api_version.minor,
                patch: caps.api_version.patch,
                prerelease: String::new(),
            }),
            high_api_version: Some(crate::proto::semver::SemVer {
                major: caps.api_version.major,
                minor: caps.api_version.minor,
                patch: caps.api_version.patch,
                prerelease: String::new(),
            }),
        };
        Ok(Response::new(proto))
    }
}

// ---------------------------------------------------------------------------
// ByteStream::Write
// ---------------------------------------------------------------------------

use crate::proto::bytestream::byte_stream_server::{ByteStream, ByteStreamServer};
use crate::proto::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};

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
        f.debug_struct("ByteStreamService")
            .finish_non_exhaustive()
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
        let parsed = parse_read_resource_name(&inner.resource_name).map_err(|msg| {
            make_status(Code::InvalidArgument, COR_CAS_BAD_RESOURCE_NAME, msg, 0)
        })?;

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
        let orch = CasReadOrchestrator::new(self.core.reader.as_ref(), self.core.meta.as_ref());
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
                return Err(miss_to_status());
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
        let now_ms = self.core.clock.now_ms();
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
            &self.core.writer,
            self.core.meta.as_ref(),
            self.core.reconciler.as_ref(),
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

// ---------------------------------------------------------------------------
// Per-blob processing
// ---------------------------------------------------------------------------

#[allow(
    clippy::too_many_arguments,
    reason = "single call site; struct-bundling each call's args adds noise without real reuse"
)]
async fn process_one_blob<B, M, R, C>(
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

fn per_blob_failure(
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

// ---------------------------------------------------------------------------
// gRPC plumbing helpers
// ---------------------------------------------------------------------------

fn extract_bearer<T>(req: &Request<T>) -> Result<String, AuthStubError> {
    let raw = req
        .metadata()
        .get("authorization")
        .ok_or(AuthStubError::PatInvalid)?;
    let s = raw.to_str().map_err(|_| AuthStubError::PatInvalid)?.trim();
    // RFC 7235 §2.1 — auth-scheme is case-insensitive. Match `Bearer ` with a
    // lowercase prefix probe so any variant (`BEARER`, `BeArEr`, …) parses.
    let space_pos = s.find(' ').ok_or(AuthStubError::PatInvalid)?;
    // Safe-by-construction slicing: `space_pos` is a byte index returned by
    // `str::find` and therefore a valid char boundary.
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let scheme = &s[..space_pos];
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthStubError::PatInvalid);
    }
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let token = s[space_pos + 1..].trim_start();
    if token.is_empty() {
        return Err(AuthStubError::PatInvalid);
    }
    Ok(token.to_owned())
}

fn extract_request_id<T>(req: &Request<T>) -> String {
    req.metadata()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string())
}

fn pat_error_to_status(e: &AuthStubError) -> Status {
    let (code, taxonomy, msg) = match e {
        AuthStubError::PatInvalid => (
            Code::Unauthenticated,
            COR_AUTH_PAT_INVALID,
            "PAT invalid or expired",
        ),
        AuthStubError::ScopeInsufficient { .. } => (
            Code::PermissionDenied,
            COR_AUTH_SCOPE_INSUFFICIENT,
            "PAT scope does not allow this operation",
        ),
    };
    let _grpc_int = match code {
        Code::Unauthenticated => GRPC_UNAUTHENTICATED,
        Code::PermissionDenied => GRPC_PERMISSION_DENIED,
        _ => 0,
    };
    make_status(code, taxonomy, msg, 0)
}

fn make_status(code: Code, taxonomy: &str, message: &str, contextual_size: u64) -> Status {
    let mut st = Status::new(code, message.to_owned());
    if let Ok(v) = tonic::metadata::MetadataValue::try_from(taxonomy) {
        st.metadata_mut().insert("x-corelink-error-code", v);
    }
    if contextual_size > 0 {
        if let Ok(v) = tonic::metadata::MetadataValue::try_from(contextual_size.to_string()) {
            st.metadata_mut().insert("x-corelink-context-size", v);
        }
    }
    st
}

fn grpc_code_from_i32(c: i32) -> Code {
    match c {
        3 => Code::InvalidArgument,
        5 => Code::NotFound,
        7 => Code::PermissionDenied,
        8 => Code::ResourceExhausted,
        10 => Code::Aborted,
        11 => Code::OutOfRange,
        13 => Code::Internal,
        14 => Code::Unavailable,
        16 => Code::Unauthenticated,
        _ => Code::Unknown,
    }
}

const fn region_str(r: Region) -> &'static str {
    // Only the 3 S-01 regions are canonical (WI-S01-003 §6.1.4): wnam,
    // weur, sam. ENAM lands in S-14 region expansion; this fn evolves at
    // that point.
    match r {
        Region::Wnam => "wnam",
        Region::Weur => "weur",
        Region::Sam => "sam",
    }
}

/// Mint a deterministic UUIDv7-shaped id from
/// `(client_request_id, tenant_uuid, digest)`. Two retries with the same
/// triple produce the same id, which makes the `audit_outbox.id` PK
/// stable across retries.
///
/// Tenant is included in the input so that two distinct tenants reusing
/// the SAME `(client_request_id, digest)` pair cannot collide on the
/// `audit_outbox.id` PK — closing the cross-tenant collision risk codex
/// round-2 surfaced as a High issue. The same shape is used by
/// `audit_request_id_for_blob` so the PK + `(request_id, event_type)`
/// dedup key are consistently tenant-scoped.
///
/// Implementation: BLAKE3-derived; we lift the first 16 bytes and stamp
/// the UUIDv7 version + variant nibbles so the resulting bytes parse as a
/// well-formed UUIDv7 (`version=7`, `variant=10b`). The "timestamp"
/// portion is content-derived, not wall-clock; the real ingest timestamp
/// lives in `audit_outbox.enqueued_at` (`request.now_ms`). UUIDv7's own
/// timestamp prefix is a sortability hint for D1 indexing, and a
/// content-derived "stamp" preserves sort stability per request without
/// varying with wall-clock between retries.
fn deterministic_audit_id(client_request_id: &str, tenant: Uuid, digest: &Digest) -> Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"corelink-reapi-audit-id-v2\0");
    hasher.update(client_request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(digest.to_hex().as_bytes());
    let out = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&out.as_bytes()[..16]);
    // UUIDv7 markings: version 0b0111 in byte 6 high nibble; variant 0b10
    // in byte 8 high two bits.
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Slot-aware variant of [`deterministic_audit_id`] for batch handlers
/// (codex round-3 P2 SEAL fix). Mixes a `slot_idx` token into the
/// hashed material so per-slot duplicates of the same digest within a
/// single request emit distinct CE `id`s — preventing
/// `audit_outbox` PK collisions when a Bazel client batches the same
/// digest twice in a `BatchReadBlobs` request. The non-slot variant
/// remains the canonical id for non-batch handlers (ByteStream::Read,
/// HTTP read), which only see at most one event per
/// (request_id, digest) pair.
fn deterministic_audit_id_at_slot(
    client_request_id: &str,
    tenant: Uuid,
    digest: &Digest,
    slot_idx: usize,
) -> Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"corelink-reapi-audit-id-v2-slot\0");
    hasher.update(client_request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(digest.to_hex().as_bytes());
    hasher.update(b"\0");
    hasher.update(&(slot_idx as u64).to_le_bytes());
    let out = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&out.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Parsed REAPI ByteStream::Read resource_name. Symmetric to
/// `ParsedResource` (write-side; module-private) but for the read
/// surface — there is no `uploads/<uuid>` segment in the read form
/// per REAPI v2 §"reading from the CAS".
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedReadResource {
    /// The 64-char-hex BLAKE3-256 digest extracted from the second
    /// segment after `blobs/`.
    pub digest: Digest,
    /// Optional size hint — `None` if the client omitted the size_bytes
    /// segment (REAPI v2 §read accepts the bare-digest form). The
    /// handler cross-checks against the blob_meta-recorded size when
    /// the hint is `Some(_)`, surfacing a mismatch as
    /// `INVALID_ARGUMENT` to discourage tooling drift; the
    /// blob_meta-recorded value is the canonical source of truth for
    /// stream framing.
    pub size_bytes: Option<i64>,
}

/// Parse a REAPI ByteStream::Read resource_name. Canonical forms (per
/// REAPI v2 + remote_cache_product_profile.md §7.2.1):
///
/// - `<instance_name>/blobs/<digest_hash>/<size_bytes>`
/// - `blobs/<digest_hash>/<size_bytes>` (instance empty)
/// - `<instance_name>/blobs/<digest_hash>` (size omitted; legacy
///   bare-digest form accepted by REAPI v2 conformance suite).
///
/// `<digest_hash>` MUST be 64 lowercase hex chars (BLAKE3-256). The
/// optional `<size_bytes>` segment must parse as a non-negative `i64`.
/// Trailing segments after `<size_bytes>` are rejected — `blobs/<h>/<s>`
/// is the canonical tail; anything beyond is a wire-protocol violation.
///
/// Exposed `pub` so `cargo-fuzz` smoke tests can exercise the parser
/// against arbitrary bytes without rebuilding the whole gRPC stack.
///
/// # Errors
///
/// Returns a `&'static str` diagnostic on malformed input. Never
/// panics; the parser is total over `str` input.
pub fn parse_read_resource_name(name: &str) -> Result<ParsedReadResource, &'static str> {
    let parts: Vec<&str> = name.split('/').collect();
    let blobs_pos = parts
        .iter()
        .position(|p| *p == "blobs")
        .ok_or("missing 'blobs' segment in resource_name")?;
    let after = parts
        .get(blobs_pos..)
        .ok_or("malformed resource_name: missing tail")?;
    // After splitting on `/`, the slice is `["blobs", hash, size?, …]`.
    // Codex round-1 P3: the prior implementation used `[_, hash, size_str, ..]`
    // which silently accepted trailing garbage (`blobs/<h>/<s>/etc/junk`).
    // Tighten to exact-match arms so the parser is total + canonical.
    let (hash, size_bytes) = match after {
        [_, hash] => (*hash, None),
        [_, hash, size_str] => {
            let size: i64 = size_str
                .parse()
                .map_err(|_| "resource_name: size_bytes is not a non-negative integer")?;
            if size < 0 {
                return Err("resource_name: size_bytes is negative");
            }
            (*hash, Some(size))
        }
        _ => {
            return Err(
                "malformed resource_name: expected exactly blobs/<hash>[/<size>]; trailing segments are not permitted",
            );
        }
    };
    let digest = Digest::from_hex(hash).map_err(|_| "resource_name: digest hex malformed")?;
    Ok(ParsedReadResource {
        digest,
        size_bytes,
    })
}

/// Apply REAPI v2 `read_offset` / `read_limit` semantics to a body.
///
/// Per `google.bytestream` §read:
/// - `read_offset` MUST be in `[0, body.len()]`. Equal to `body.len()`
///   yields an empty stream (legitimate trailing read). Greater than
///   `body.len()` is `OUT_OF_RANGE`.
/// - `read_limit == 0` ⇒ "no limit" (read to end).
/// - `read_limit > 0` ⇒ stream `min(read_limit, body.len() - offset)` bytes.
fn slice_for_offset_limit(body: Bytes, offset: i64, limit: i64) -> Result<Bytes, Status> {
    let body_len = body.len();
    let body_len_i64 = i64::try_from(body_len).unwrap_or(i64::MAX);
    if offset > body_len_i64 {
        return Err(make_status(
            Code::OutOfRange,
            COR_CAS_BAD_RESOURCE_NAME,
            "ByteStream::Read read_offset exceeds blob size",
            body_len as u64,
        ));
    }
    let offset_usize = usize::try_from(offset).unwrap_or(usize::MAX);
    let remaining = body_len.saturating_sub(offset_usize);
    let take = if limit == 0 {
        remaining
    } else {
        usize::try_from(limit).unwrap_or(usize::MAX).min(remaining)
    };
    Ok(body.slice(offset_usize..offset_usize.saturating_add(take)))
}

/// Audit emission tail bundle. Captured by
/// [`chunked_read_stream_with_audit`] so the read_completed envelope is
/// emitted **post-stream-completion**, with the actually-delivered
/// `bytes_sent` count — partial reads (`read_offset`/`read_limit`) and
/// client disconnects therefore audit accurately, not as full reads.
/// Codex round-1 P2(a) fix.
struct ReadAuditTail {
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: String,
    digest: Digest,
    /// blob_meta-recorded full body size (from D1 row).
    size_bytes: u64,
    /// Length of the payload we actually slice into chunks (post
    /// `read_offset` + `read_limit`). Travels into the audit envelope
    /// alongside `size_bytes` so SIEM can spot partial-read patterns.
    payload_len: u64,
}

/// Build a `ReadStream` of `ReadResponse` chunks at `READ_CHUNK_SIZE_BYTES`
/// granularity AND emit the `corelink.cas.read_completed` audit
/// envelope after the LAST chunk has been polled (so a client
/// disconnect MID-stream audits as a partial / aborted read, not a
/// successful full read).
///
/// The audit emission is wrapped via `futures::stream::unfold` so the
/// terminal "emit the audit envelope" branch fires when the underlying
/// chunk iterator has yielded its last frame. If the stream is dropped
/// before completion (client cancellation), the inner state's `Drop`
/// path emits a structured `tracing::warn!` for SRE forensics.
///
/// Empty body case: REAPI v2 §"reading from CAS" allows reading the
/// canonical empty blob (BLAKE3 of `b""`); we emit a single empty
/// `ReadResponse{data: []}` followed by end-of-stream so clients see
/// a non-empty stream they can collect with the same code path.
fn chunked_read_stream_with_audit(
    body: Bytes,
    audit: ReadAuditTail,
) -> futures::stream::BoxStream<'static, Result<ReadResponse, Status>> {
    use futures::StreamExt;
    let chunks = chunk_bytes_for_read(body, READ_CHUNK_SIZE_BYTES);
    let total_chunks = chunks.len();

    // Convert chunks to `Vec<u8>` once so the iterator is `'static`.
    let frames: Vec<ReadResponse> = chunks
        .into_iter()
        .map(|c| ReadResponse { data: c.to_vec() })
        .collect();

    // State for the unfold: (frames_remaining_iter, bytes_sent, audit).
    let bytes_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let bytes_sent_for_unfold = std::sync::Arc::clone(&bytes_sent);

    let frames_iter = frames.into_iter();
    let unfold_state = (frames_iter, bytes_sent_for_unfold);

    let stream = futures::stream::unfold(unfold_state, |(mut iter, bytes_sent)| async move {
        match iter.next() {
            Some(frame) => {
                bytes_sent.fetch_add(
                    frame.data.len() as u64,
                    std::sync::atomic::Ordering::AcqRel,
                );
                Some((Ok(frame), (iter, bytes_sent)))
            }
            None => None,
        }
    });

    // Wrap in a `ReadCompleteAudit` Drop guard so the audit envelope is
    // emitted regardless of completion path (success → terminal None;
    // cancellation → guard Drop). The guard checks `bytes_sent` at
    // emission time so partial reads are audited correctly.
    let guard = ReadCompleteAuditGuard {
        audit,
        bytes_sent,
        total_chunks,
        emitted: false,
    };
    let stream_with_guard = futures::stream::unfold(
        (Box::pin(stream)
            as std::pin::Pin<
                Box<dyn futures::Stream<Item = Result<ReadResponse, Status>> + Send>,
            >, guard),
        |(mut s, mut guard)| async move {
            match s.as_mut().next().await {
                Some(item) => Some((item, (s, guard))),
                None => {
                    // Terminal branch — emit the read_completed envelope
                    // with the actual bytes_sent count.
                    guard.emit_completion();
                    None
                }
            }
        },
    );
    stream_with_guard.boxed()
}

/// Drop-time fallback: client cancelled the stream before the last
/// chunk was polled. Emit a warn-line (no info-level "completed")
/// so SIEM can distinguish abandoned vs completed reads.
struct ReadCompleteAuditGuard {
    audit: ReadAuditTail,
    bytes_sent: std::sync::Arc<std::sync::atomic::AtomicU64>,
    total_chunks: usize,
    emitted: bool,
}

impl ReadCompleteAuditGuard {
    fn emit_completion(&mut self) {
        if self.emitted {
            return;
        }
        self.emitted = true;
        let bytes_sent = self
            .bytes_sent
            .load(std::sync::atomic::Ordering::Acquire);
        emit_read_completed_audit_post_stream(
            self.audit.tenant_id,
            self.audit.principal_id,
            self.audit.region,
            &self.audit.client_request_id,
            &self.audit.digest,
            self.audit.size_bytes,
            self.audit.payload_len,
            bytes_sent,
            self.total_chunks,
        );
    }
}

impl Drop for ReadCompleteAuditGuard {
    fn drop(&mut self) {
        if self.emitted {
            return;
        }
        let bytes_sent = self
            .bytes_sent
            .load(std::sync::atomic::Ordering::Acquire);
        // Cancellation path — partial / aborted read. Distinct event
        // type so dashboards can graph cancellation rate separately.
        tracing::warn!(
            target: "corelink.audit",
            event_type = "corelink.cas.read_aborted",
            tenant = %self.audit.tenant_id,
            principal = %self.audit.principal_id,
            region = %region_str(self.audit.region),
            digest = %self.audit.digest.to_hex(),
            size_bytes = self.audit.size_bytes,
            payload_len = self.audit.payload_len,
            bytes_sent = bytes_sent,
            "CAS read aborted (client cancelled stream before completion)"
        );
    }
}

/// Split a `Bytes` body into a `Vec<Bytes>` of chunks at `chunk_size`
/// granularity. Empty body → `vec![Bytes::new()]` (one empty frame for
/// REAPI conformance — clients expect at least one ReadResponse).
fn chunk_bytes_for_read(body: Bytes, chunk_size: usize) -> Vec<Bytes> {
    if chunk_size == 0 {
        return vec![body];
    }
    if body.is_empty() {
        return vec![Bytes::new()];
    }
    let mut out = Vec::with_capacity(body.len().div_ceil(chunk_size));
    let mut start = 0usize;
    while start < body.len() {
        let end = start.saturating_add(chunk_size).min(body.len());
        out.push(body.slice(start..end));
        start = end;
    }
    out
}

/// Map [`crate::read::ReadOrchestratorError`] → tonic `Status`.
fn read_orch_error_to_status(e: &crate::read::ReadOrchestratorError) -> Status {
    let m = e.mapping();
    make_status(grpc_code_from_i32(m.grpc_code), m.taxonomy_code, m.message, 0)
}

/// Map a `ReadOutcome::NotFound` into the canonical wire status. Per
/// ADR-0028 every [`MissReason`] variant maps to the same wire 404 +
/// `COR_CAS_BLOB_NOT_FOUND` taxonomy code; the disambiguation lives in
/// the audit envelope, not in the wire response.
fn miss_to_status() -> Status {
    let m = miss_mapping();
    make_status(grpc_code_from_i32(m.grpc_code), m.taxonomy_code, m.message, 0)
}

/// Public re-export of the read-completed audit emitter (module-private
/// `emit_read_completed_audit_post_stream`) for the HTTP read handler
/// in [`crate::http_read`]. Same shape; the cross-module helper keeps
/// audit envelopes byte-identical between gRPC and HTTP transports
/// (codex round-1 P1(b) parity fix).
#[allow(
    clippy::too_many_arguments,
    reason = "shared cross-module audit emitter; bundling args adds a struct noise without reuse"
)]
pub fn emit_read_completed_audit_pub(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
) {
    emit_read_completed_audit_post_stream(
        tenant_id,
        principal_id,
        region,
        client_request_id,
        digest,
        size_bytes,
        payload_len,
        bytes_sent,
        total_chunks,
    );
}

/// Public re-export of the read-miss audit emitter (module-private
/// `emit_read_miss_audit`) for the HTTP read handler in
/// [`crate::http_read`]. Same shape; ensures HTTP and gRPC 404 paths
/// emit byte-identical envelopes (codex round-1 P1(b) parity fix).
pub fn emit_read_miss_audit_pub(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
) {
    emit_read_miss_audit(
        storage_ctx,
        principal_id,
        region,
        client_request_id,
        digest,
        reason,
    );
}

/// Emit the canonical `corelink.cas.read_completed` audit envelope on
/// a successful FULL stream completion.
///
/// `bytes_sent` is the actually-delivered byte count (post
/// `read_offset`/`read_limit`); when `bytes_sent < payload_len` we
/// emit a partial-read marker (codex P2(a) fix — partial reads no
/// longer audit as full reads).
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
fn emit_read_completed_audit_post_stream(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id(client_request_id, tenant_id, digest);
    // Codex round-2 P3 fix: the envelope's `data.size_bytes` field is
    // the canonical blob_meta-recorded body length per the audit
    // contract (matches the write-side `read_completed` semantics in
    // CTRL-AUDIT-003). The actually-delivered byte count travels
    // alongside as a structured tracing field (`bytes_sent`) and into
    // the audit chain (S-09) via a separate column once the
    // out-of-band event surface ships.
    let envelope = crate::audit::AuditEnvelopeBuilder::read_completed(
        crate::audit::AuditPrincipal {
            tenant_id,
            principal_id,
            region: region_str(region),
        },
        client_request_id,
        canonical,
        size_bytes,
        crate::audit::AuditTime { envelope_id },
    )
    .build();
    if let Ok(json) = envelope.to_json() {
        tracing::info!(
            target: "corelink.audit",
            event_type = crate::audit::REAPI_READ_COMPLETED,
            tenant = %tenant_id,
            principal = %principal_id,
            region = %region_str(region),
            digest = %digest.to_hex(),
            size_bytes = size_bytes,
            payload_len = payload_len,
            bytes_sent = bytes_sent,
            total_chunks = total_chunks,
            envelope = %json,
            "CAS read completed"
        );
    }
}

/// Emit a per-MissReason audit envelope on a 404 path. Per ADR-0028
/// the disambiguation lives here, never on the wire.
///
/// Codex round-1 P2(b) + round-4 P2 fix: each [`MissReason`] now emits
/// a DISTINCT CE event type so the SEV-1 cross-tenant alert is no
/// longer poisoned by ordinary cache misses or tombstone reads.
///
/// Severity matrix (per WI §11 + ADR-0028):
/// - [`MissReason::NeverExisted`] → `info!` + [`crate::audit::REAPI_READ_MISS`].
///   The conflated NeverExisted/CrossTenantMasked arm cannot be
///   disambiguated at the read-side without a side-channel oracle;
///   the S-09 chain consumer reclassifies to `cross_tenant_attempt`
///   (SEV-1) using the offline global digest index.
/// - [`MissReason::Tombstoned`] → `info!` +
///   [`crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT`]. Legitimate
///   post-GC read.
/// - [`MissReason::R2OrphanRow`] → `error!` +
///   [`crate::audit::REAPI_R2_ORPHAN_DETECTED`]. SEV-2 maps to the
///   GC reconcile signal.
fn emit_read_miss_audit(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id(client_request_id, storage_ctx.tenant_id(), digest);
    let envelope = match reason {
        // Codex round-4 P2 fix: emit the low-severity `read_miss`
        // event type for the conflated NeverExisted/CrossTenantMasked
        // arm. The S-09 chain consumer reclassifies to
        // `cross_tenant_attempt` (SEV-1) using the offline global
        // digest index when applicable; the read-side handler can't
        // disambiguate without a side-channel oracle and therefore
        // does NOT trip the SEV-1 alert here.
        MissReason::NeverExisted => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_READ_MISS,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::Tombstoned => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::R2OrphanRow => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_R2_ORPHAN_DETECTED,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
    };
    let json = envelope.to_json().unwrap_or_default();
    let event_type = envelope.r#type;
    match reason {
        MissReason::NeverExisted => {
            // Severity is `info!` — the conflated NeverExisted /
            // CrossTenantMasked arm is high-volume by definition
            // (every Bazel/Buck2 cache-miss probe surfaces here). The
            // S-09 chain consumer reclassifies to `warn!` /
            // `cross_tenant_attempt` SEV-1 only when its global digest
            // index confirms cross-tenant ownership; the read-side
            // handler stays silent on the alert lane (codex round-4
            // P2 fix).
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "never_existed_or_cross_tenant",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read miss (uniform 404 per ADR-0028; classification deferred to S-09 chain consumer)"
            );
        }
        MissReason::Tombstoned => {
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "tombstoned",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read of tombstoned blob (uniform 404)"
            );
        }
        MissReason::R2OrphanRow => {
            tracing::error!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "r2_orphan_row",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read encountered R2 orphan row (GC reconcile pending)"
            );
        }
    }
}

/// Slot-aware variant of [`emit_read_completed_audit_post_stream`]
/// for batch handlers (codex round-3 P2 SEAL fix). The audit envelope
/// `id` mixes the `slot_idx` so per-slot duplicates of the same
/// digest in a single `BatchReadBlobs` request do NOT collide on the
/// `audit_outbox` PK.
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
fn emit_read_completed_audit_post_stream_at_slot(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
    slot_idx: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id =
        deterministic_audit_id_at_slot(client_request_id, tenant_id, digest, slot_idx);
    let envelope = crate::audit::AuditEnvelopeBuilder::read_completed(
        crate::audit::AuditPrincipal {
            tenant_id,
            principal_id,
            region: region_str(region),
        },
        client_request_id,
        canonical,
        size_bytes,
        crate::audit::AuditTime { envelope_id },
    )
    .build();
    if let Ok(json) = envelope.to_json() {
        tracing::info!(
            target: "corelink.audit",
            event_type = crate::audit::REAPI_READ_COMPLETED,
            tenant = %tenant_id,
            principal = %principal_id,
            region = %region_str(region),
            digest = %digest.to_hex(),
            size_bytes = size_bytes,
            payload_len = payload_len,
            bytes_sent = bytes_sent,
            total_chunks = total_chunks,
            slot_idx = slot_idx,
            envelope = %json,
            "CAS read completed (batch slot)"
        );
    }
}

/// Slot-aware variant of [`emit_read_miss_audit`] for batch handlers
/// (codex round-3 P2 SEAL fix). The audit envelope `id` mixes the
/// `slot_idx` to keep per-slot CE ids unique even when the same
/// digest appears multiple times in the request.
fn emit_read_miss_audit_at_slot(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
    slot_idx: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id_at_slot(
        client_request_id,
        storage_ctx.tenant_id(),
        digest,
        slot_idx,
    );
    let envelope = match reason {
        MissReason::NeverExisted => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_READ_MISS,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::Tombstoned => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::R2OrphanRow => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_R2_ORPHAN_DETECTED,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
    };
    let json = envelope.to_json().unwrap_or_default();
    let event_type = envelope.r#type;
    match reason {
        MissReason::NeverExisted => {
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "never_existed_or_cross_tenant",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read miss (uniform 404 per ADR-0028; classification deferred to S-09 chain consumer)"
            );
        }
        MissReason::Tombstoned => {
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "tombstoned",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read of tombstoned blob (uniform 404)"
            );
        }
        MissReason::R2OrphanRow => {
            tracing::error!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "r2_orphan_row",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read encountered R2 orphan row (GC reconcile pending)"
            );
        }
    }
}

/// Emit a single batch-summary audit line for a `FindMissingBlobs`
/// invocation. WI-S02-002 §6.1.5 mandates batch-level audit (NOT
/// per-digest CE envelopes) so the audit chain is not flooded by
/// 1000-digest cache-miss probes.
///
/// The line goes to `corelink.audit` at `info!` level; SRE
/// dashboards can trend `(missing / total)` ratio per tenant for
/// regression monitoring.
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
fn emit_find_missing_batch_audit(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    requested: usize,
    missing: usize,
    d1_lookups: usize,
) {
    tracing::info!(
        target: "corelink.audit",
        event_type = "corelink.cas.find_missing_batch",
        tenant = %storage_ctx.tenant_id(),
        principal = %principal_id,
        region = %region_str(region),
        request_id = %client_request_id,
        requested = requested,
        missing = missing,
        d1_lookups = d1_lookups,
        "FindMissingBlobs batch processed"
    );
}

struct ParsedResource {
    digest: Digest,
    size_bytes: i64,
}

/// Parse a REAPI ByteStream resource_name. Canonical forms:
///
/// - `<instance>/uploads/<uuid>/blobs/<digest_hash>/<size_bytes>`
/// - `uploads/<uuid>/blobs/<digest_hash>/<size_bytes>` (instance empty)
///
/// We accept either. The `<digest_hash>` MUST be 64 lowercase hex
/// characters (BLAKE3-256 / SHA-256). `<size_bytes>` must parse as a
/// non-negative `i64`.
fn parse_resource_name(name: &str) -> Result<ParsedResource, &'static str> {
    let parts: Vec<&str> = name.split('/').collect();
    // Walk to find `uploads`; everything from there must be
    // `uploads/<uuid>/blobs/<digest>/<size>`.
    let uploads_pos = parts
        .iter()
        .position(|p| *p == "uploads")
        .ok_or("missing 'uploads' segment in resource_name")?;
    let after = parts
        .get(uploads_pos..)
        .ok_or("malformed resource_name: missing tail")?;
    let &[_, _uuid, blobs, hash, size_bytes_str, ..] = after else {
        return Err("malformed resource_name: expected uploads/<uuid>/blobs/<hash>/<size>");
    };
    if blobs != "blobs" {
        return Err("malformed resource_name: expected 'blobs' segment after upload uuid");
    }
    let digest = Digest::from_hex(hash).map_err(|_| "resource_name: digest hex malformed")?;
    let size_bytes: i64 = size_bytes_str
        .parse()
        .map_err(|_| "resource_name: size_bytes is not a non-negative integer")?;
    if size_bytes < 0 {
        return Err("resource_name: size_bytes is negative");
    }
    Ok(ParsedResource { digest, size_bytes })
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

    #[test]
    fn parse_resource_name_canonical() {
        let name = "instance/uploads/0000-uuid/blobs/d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24/11";
        let parsed = parse_resource_name(name).unwrap();
        assert_eq!(parsed.size_bytes, 11);
        assert_eq!(
            parsed.digest.to_hex(),
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }

    #[test]
    fn parse_resource_name_rejects_short_form() {
        assert!(parse_resource_name("nope").is_err());
        assert!(parse_resource_name("uploads/uuid").is_err());
        assert!(parse_resource_name("uploads/uuid/blobs/notenoughhex/0").is_err());
    }

    #[test]
    fn parse_read_resource_name_canonical_with_size() {
        let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
        let name = format!("corelink-instance/blobs/{h}/11");
        let parsed = parse_read_resource_name(&name).unwrap();
        assert_eq!(parsed.digest.to_hex(), h);
        assert_eq!(parsed.size_bytes, Some(11));
    }

    #[test]
    fn parse_read_resource_name_canonical_bare_digest() {
        let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
        let name = format!("corelink-instance/blobs/{h}");
        let parsed = parse_read_resource_name(&name).unwrap();
        assert_eq!(parsed.digest.to_hex(), h);
        assert_eq!(parsed.size_bytes, None);
    }

    #[test]
    fn parse_read_resource_name_rejects_trailing_garbage() {
        // codex round-1 P3 fix: trailing segments must be rejected.
        let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
        let name = format!("corelink-instance/blobs/{h}/11/extra-junk");
        assert!(parse_read_resource_name(&name).is_err());
        let name2 = format!("corelink-instance/blobs/{h}/11/x/y/z");
        assert!(parse_read_resource_name(&name2).is_err());
    }

    #[test]
    fn parse_read_resource_name_rejects_negative_size() {
        let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
        let name = format!("corelink-instance/blobs/{h}/-1");
        assert!(parse_read_resource_name(&name).is_err());
    }

    #[test]
    fn parse_read_resource_name_rejects_malformed_hash() {
        let name = "corelink-instance/blobs/zzz/11";
        assert!(parse_read_resource_name(name).is_err());
    }

    #[test]
    fn parse_read_resource_name_rejects_missing_blobs_segment() {
        assert!(parse_read_resource_name("corelink-instance/uploads/abc").is_err());
    }

    #[test]
    fn deterministic_audit_id_is_stable_per_request() {
        let d = Digest::compute(b"hello");
        let t = Uuid::from_u128(1);
        let a = deterministic_audit_id("req-1", t, &d);
        let b = deterministic_audit_id("req-1", t, &d);
        assert_eq!(a, b);
        // version + variant nibbles are well-formed.
        let bs = a.as_bytes();
        assert_eq!(bs[6] & 0xF0, 0x70);
        assert_eq!(bs[8] & 0xC0, 0x80);
    }

    #[test]
    fn extract_bearer_is_case_insensitive() {
        // codex round-2 Low fix: RFC 7235 §2.1 auth-scheme is
        // case-insensitive.
        for variant in ["Bearer", "bearer", "BEARER", "BeArEr"] {
            let mut req = Request::new(());
            let v: tonic::metadata::MetadataValue<_> =
                format!("{variant} secret-token").parse().unwrap();
            req.metadata_mut().insert("authorization", v);
            let token = extract_bearer(&req).unwrap();
            assert_eq!(token, "secret-token");
        }
    }

    #[test]
    fn extract_bearer_rejects_non_bearer_scheme() {
        let mut req = Request::new(());
        let v: tonic::metadata::MetadataValue<_> = "Basic dXNlcjpwYXNz".parse().unwrap();
        req.metadata_mut().insert("authorization", v);
        assert_eq!(extract_bearer(&req), Err(AuthStubError::PatInvalid));
    }

    #[test]
    fn deterministic_audit_id_varies_per_request_or_digest_or_tenant() {
        let d1 = Digest::compute(b"a");
        let d2 = Digest::compute(b"b");
        let t1 = Uuid::from_u128(1);
        let t2 = Uuid::from_u128(2);
        assert_ne!(
            deterministic_audit_id("req-1", t1, &d1),
            deterministic_audit_id("req-1", t1, &d2)
        );
        assert_ne!(
            deterministic_audit_id("req-1", t1, &d1),
            deterministic_audit_id("req-2", t1, &d1)
        );
        // codex round-2 High fix: tenant scoping.
        assert_ne!(
            deterministic_audit_id("req-1", t1, &d1),
            deterministic_audit_id("req-1", t2, &d1)
        );
    }

    #[test]
    fn region_str_matches_canonical_lowercase() {
        assert_eq!(region_str(Region::Wnam), "wnam");
        assert_eq!(region_str(Region::Weur), "weur");
        assert_eq!(region_str(Region::Sam), "sam");
    }
}
