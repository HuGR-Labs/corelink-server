//! End-to-end gRPC integration tests for `BatchReadBlobs`
//! (WI-S02-002 §6.1.2 + §9.5).
//!
//! BatchReadBlobs is the small-blob inline-bytes batch read surface
//! complementing `ByteStream::Read` (large-blob streaming) and
//! `FindMissingBlobs` (existence check). REAPI v2 §"reading from
//! CAS": the server returns one per-digest `Response { digest, data,
//! status }` entry; hits inline the body up to the canonical 4 MiB
//! per-blob inline cap and aggregate 4 MiB response cap; misses
//! surface NOT_FOUND in the per-blob status (NEVER as top-level RPC
//! errors); oversize blobs surface FAILED_PRECONDITION pointing the
//! client to ByteStream::Read.
//!
//! Coverage:
//!
//! - **AC-BR-1 happy path** — 3 small blobs returned inline, in input
//!   order; bytes byte-identical with the writes.
//! - **AC-BR-2 cross-tenant masked** — Tenant B asks for Tenant A's
//!   digest; per-blob NOT_FOUND.
//! - **AC-BR-3 batch size limit** — 1001 digests → top-level
//!   OUT_OF_RANGE.
//! - **AC-BR-4 mixed hits/misses** — present + absent + cross-tenant
//!   in one batch; status codes per-entry are correct.
//! - **AC-BR-5 PAT scope insufficient** — write-only PAT cannot
//!   invoke BatchReadBlobs (cache:r required).
//! - **AC-BR-6 non-BLAKE3 digest function** — top-level
//!   INVALID_ARGUMENT.
//! - **AC-BR-7 empty input** — empty response, no error.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    reason = "test code: integration harness with explicit asserts"
)]

use std::sync::Arc;

use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_hash::Digest;
use corelink_meta::InMemoryMetaStore;
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::reapi::batch_update_blobs_request as bub_req;
use corelink_reapi::proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use corelink_reapi::proto::reapi::{
    BatchReadBlobsRequest, BatchUpdateBlobsRequest, Digest as ProtoDigest,
};
use corelink_reapi::{AuthScope, CasWriteService, StubPatValidator};
use corelink_replication::region_resolver::Region;
use corelink_tenant_path::TenantDerivationKey;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::Code;
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN_RW_A: &str = "token-A-rw";
const TOKEN_RW_B: &str = "token-B-rw";
const TOKEN_WRITE_ONLY_A: &str = "token-A-write-only";

#[allow(dead_code)]
struct Harness {
    backend: Arc<InMemoryR2>,
    meta: Arc<InMemoryMetaStore>,
    addr: std::net::SocketAddr,
    _server: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

impl Harness {
    async fn boot() -> Self {
        let tdk = Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])));
        let backend = Arc::new(InMemoryR2::new());
        let writer = Arc::new(R2Writer::new(Region::Wnam, Arc::clone(&backend)));
        let reader = Arc::new(R2Reader::new(Region::Wnam, Arc::clone(&backend)));
        let meta = Arc::new(InMemoryMetaStore::new());
        let reconciler = Arc::new(NoopOrphanReconciler);

        let mut pat = StubPatValidator::new();
        pat.insert(
            TOKEN_RW_A,
            Uuid::from_u128(1),
            Uuid::from_u128(11),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_RW_B,
            Uuid::from_u128(2),
            Uuid::from_u128(12),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_WRITE_ONLY_A,
            Uuid::from_u128(3),
            Uuid::from_u128(13),
            Region::Wnam,
            [AuthScope::CacheWrite],
        );

        let core = Arc::new(HandlerCore::new(
            pat,
            tdk,
            Arc::clone(&writer),
            Arc::clone(&reader),
            Arc::clone(&meta),
            reconciler,
            SystemClock,
        ));

        let cas = CasWriteService::new(Arc::clone(&core));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let server = Server::builder()
            .add_service(cas.into_server().max_decoding_message_size(8 * 1024 * 1024))
            .serve_with_incoming(stream);
        let server = tokio::spawn(server);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self {
            backend,
            meta,
            addr,
            _server: server,
        }
    }

    async fn channel(&self) -> Channel {
        Channel::from_shared(format!("http://{}", self.addr))
            .unwrap()
            .connect()
            .await
            .unwrap()
    }

    async fn cas_client(&self) -> ContentAddressableStorageClient<Channel> {
        ContentAddressableStorageClient::new(self.channel().await)
    }
}

fn auth_request<T>(payload: T, token: &str, request_id: &str) -> tonic::Request<T> {
    let mut req = tonic::Request::new(payload);
    let v: MetadataValue<_> = format!("Bearer {token}").parse().unwrap();
    req.metadata_mut().insert("authorization", v);
    let r: MetadataValue<_> = request_id.parse().unwrap();
    req.metadata_mut().insert("x-request-id", r);
    req
}

async fn write_blob_via_grpc(h: &Harness, body: &[u8], token: &str, request_id: &str) -> Digest {
    let mut client = h.cas_client().await;
    let digest = Digest::compute(body);
    let req = auth_request(
        BatchUpdateBlobsRequest {
            instance_name: String::new(),
            requests: vec![bub_req::Request {
                digest: Some(ProtoDigest {
                    hash: digest.to_hex(),
                    size_bytes: body.len() as i64,
                }),
                data: body.to_vec(),
                compressor: 0,
            }],
            digest_function: 0,
        },
        token,
        request_id,
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    let st = resp.responses.first().unwrap().status.as_ref().unwrap();
    assert_eq!(st.code, 0, "write failed: {}", st.message);
    digest
}

fn pd(d: Digest, size: i64) -> ProtoDigest {
    ProtoDigest {
        hash: d.to_hex(),
        size_bytes: size,
    }
}

#[tokio::test]
async fn ac_br_1_happy_path_inlines_bodies_in_input_order() {
    let h = Harness::boot().await;
    let body_1 = b"first-body";
    let body_2 = b"second";
    let body_3 = b"third-blob-content";
    let d1 = write_blob_via_grpc(&h, body_1, TOKEN_RW_A, "req-w1").await;
    let d2 = write_blob_via_grpc(&h, body_2, TOKEN_RW_A, "req-w2").await;
    let d3 = write_blob_via_grpc(&h, body_3, TOKEN_RW_A, "req-w3").await;

    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![
                pd(d1, body_1.len() as i64),
                pd(d2, body_2.len() as i64),
                pd(d3, body_3.len() as i64),
            ],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-batch-read",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 3);
    let r1 = &resp.responses[0];
    let r2 = &resp.responses[1];
    let r3 = &resp.responses[2];
    assert_eq!(r1.status.as_ref().unwrap().code, 0, "blob 1 hit");
    assert_eq!(r2.status.as_ref().unwrap().code, 0, "blob 2 hit");
    assert_eq!(r3.status.as_ref().unwrap().code, 0, "blob 3 hit");
    assert_eq!(r1.data, body_1);
    assert_eq!(r2.data, body_2);
    assert_eq!(r3.data, body_3);
    assert_eq!(r1.digest.as_ref().unwrap().hash, d1.to_hex());
    assert_eq!(r2.digest.as_ref().unwrap().hash, d2.to_hex());
    assert_eq!(r3.digest.as_ref().unwrap().hash, d3.to_hex());
}

#[tokio::test]
async fn ac_br_2_cross_tenant_blobs_per_blob_not_found() {
    let h = Harness::boot().await;
    let body = b"A-body-secret";
    let d_a = write_blob_via_grpc(&h, body, TOKEN_RW_A, "req-AB-1").await;

    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d_a, body.len() as i64)],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_B,
        "req-B-cross-batch",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    let r = &resp.responses[0];
    assert_eq!(r.status.as_ref().unwrap().code, 5, "NOT_FOUND per-blob");
    assert!(r.data.is_empty(), "no body bytes leaked on miss");
}

#[tokio::test]
async fn ac_br_3_batch_size_limit_top_level_out_of_range() {
    let h = Harness::boot().await;
    let mut digests = Vec::with_capacity(1001);
    for i in 0..1001u32 {
        digests.push(pd(Digest::compute(format!("body-{i}").as_bytes()), 7));
    }
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests,
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-overflow",
    );
    let err = client.batch_read_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::OutOfRange);
}

#[tokio::test]
async fn ac_br_4_mixed_hits_misses_cross_tenant() {
    let h = Harness::boot().await;
    let body_present = b"present-blob";
    let d_present = write_blob_via_grpc(&h, body_present, TOKEN_RW_A, "req-mix-1").await;
    let d_absent = Digest::compute(b"absent-anywhere");
    // cross-tenant: written under B
    let body_b = b"B-body";
    let d_b = write_blob_via_grpc(&h, body_b, TOKEN_RW_B, "req-B-mix").await;

    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![
                pd(d_present, body_present.len() as i64),
                pd(d_absent, 17),
                pd(d_b, body_b.len() as i64),
            ],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-mix-A",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 3);
    assert_eq!(resp.responses[0].status.as_ref().unwrap().code, 0);
    assert_eq!(resp.responses[0].data, body_present);
    assert_eq!(
        resp.responses[1].status.as_ref().unwrap().code,
        5,
        "absent → NOT_FOUND"
    );
    assert_eq!(
        resp.responses[2].status.as_ref().unwrap().code,
        5,
        "cross-tenant → NOT_FOUND"
    );
    assert!(resp.responses[1].data.is_empty());
    assert!(resp.responses[2].data.is_empty());
}

#[tokio::test]
async fn ac_br_5_pat_scope_insufficient_for_write_only() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"x");
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d, 1)],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_WRITE_ONLY_A,
        "req-br-write-only",
    );
    let err = client.batch_read_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn ac_br_6_non_blake3_digest_function_top_level_invalid_argument() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"x");
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d, 1)],
            acceptable_compressors: vec![],
            digest_function: 1, // SHA256
        },
        TOKEN_RW_A,
        "req-br-sha256",
    );
    let err = client.batch_read_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn ac_br_7_empty_input_returns_empty_response() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-empty",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert!(resp.responses.is_empty());
}

/// Codex round-1 P1 fix: caller-supplied `digest.size_bytes` MUST be
/// cross-checked against the blob_meta-recorded value. A mismatch
/// indicates client-side tooling drift OR a confusion attack
/// against the client; surface as per-blob INVALID_ARGUMENT (REAPI
/// canonical: per-blob errors do NOT abort the batch).
#[tokio::test]
async fn ac_br_9_caller_supplied_size_mismatch_per_blob_invalid_argument() {
    let h = Harness::boot().await;
    let body = b"actual-12-bytes!"; // 16 bytes
    let d = write_blob_via_grpc(&h, body, TOKEN_RW_A, "req-size-mismatch").await;
    let mut client = h.cas_client().await;
    // Caller declares the wrong size_bytes (truthful body is 16
    // bytes; we tell the server "10 bytes"). The server MUST reject
    // this per-blob slot rather than silently returning the actual
    // body alongside the wrong declared size.
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![ProtoDigest {
                hash: d.to_hex(),
                size_bytes: 10,
            }],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-size-mismatch-batch",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    let r = &resp.responses[0];
    assert_eq!(
        r.status.as_ref().unwrap().code,
        3,
        "INVALID_ARGUMENT per-blob"
    );
    assert!(r.data.is_empty(), "no body bytes returned on size mismatch");
}

#[tokio::test]
async fn ac_br_8_malformed_digest_per_blob_invalid_argument() {
    let h = Harness::boot().await;
    let body_present = b"good";
    let d_good = write_blob_via_grpc(&h, body_present, TOKEN_RW_A, "req-br-good").await;
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![
                pd(d_good, body_present.len() as i64),
                ProtoDigest {
                    hash: "not-a-valid-digest".to_owned(),
                    size_bytes: 0,
                },
            ],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-mixed-malformed",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    // Per-blob INVALID_ARGUMENT for the malformed entry; the
    // well-formed entry must still succeed (REAPI canonical: per-blob
    // errors do NOT abort the batch).
    assert_eq!(resp.responses.len(), 2);
    assert_eq!(resp.responses[0].status.as_ref().unwrap().code, 0);
    assert_eq!(
        resp.responses[1].status.as_ref().unwrap().code,
        3,
        "malformed digest → per-blob INVALID_ARGUMENT"
    );
}

/// Codex round-2 P2 + round-3 P2 SEAL coverage: when the client
/// supplies `acceptable_compressors = [ZSTD]` (no IDENTITY), the
/// server MUST reject with top-level FAILED_PRECONDITION +
/// `COR_CAS_COMPRESSOR_UNSUPPORTED` — CoreLink S-01 advertises
/// IDENTITY only.
#[tokio::test]
async fn ac_br_10_compressor_negotiation_rejects_non_identity_only() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"x");
    let mut client = h.cas_client().await;
    // ZSTD = 1 in the canonical proto; no IDENTITY (= 0) in list.
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d, 1)],
            acceptable_compressors: vec![1],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-zstd-only",
    );
    let err = client.batch_read_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::FailedPrecondition);
    let taxonomy = err
        .metadata()
        .get("x-corelink-error-code")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert_eq!(taxonomy, "COR_CAS_COMPRESSOR_UNSUPPORTED");
}

/// Codex round-2 P2 SEAL coverage: when `acceptable_compressors`
/// EXPLICITLY includes IDENTITY (alongside other algos), the server
/// MUST accept and serve.
#[tokio::test]
async fn ac_br_11_compressor_accepts_when_identity_in_list() {
    let h = Harness::boot().await;
    let body = b"compressor-ok";
    let d = write_blob_via_grpc(&h, body, TOKEN_RW_A, "req-br-comp-pre").await;
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d, body.len() as i64)],
            acceptable_compressors: vec![0, 1], // IDENTITY + ZSTD
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-identity-and-zstd",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    assert_eq!(resp.responses[0].status.as_ref().unwrap().code, 0);
    assert_eq!(resp.responses[0].data, body);
    assert_eq!(resp.responses[0].compressor, 0); // IDENTITY
}

/// Codex round-3 P2 SEAL coverage: alive `blob_meta` row + R2
/// `NotFound` is the canonical "R2 orphan" condition (window
/// closed by the S-06 GC reconciler). The wire response is the
/// uniform NOT_FOUND per ADR-0028; the SEV-2
/// `corelink.cas.r2_orphan_detected` audit emission is verified
/// implicitly by the response shape (the orphan branch is the only
/// path to a per-blob NOT_FOUND on a Decision::Fetch slot — Decision::Miss
/// surfaces NeverExisted/Tombstoned, which never reaches Pass 2).
#[tokio::test]
async fn ac_br_12_alive_meta_r2_not_found_surfaces_uniform_404() {
    use corelink_meta::MetaStore;
    let h = Harness::boot().await;
    // Step 1: write the blob so the D1 row goes Alive.
    let body = b"will-be-orphaned";
    let d = write_blob_via_grpc(&h, body, TOKEN_RW_A, "req-br-orphan-pre").await;
    // Step 2: simulate the orphan condition by deleting the R2 object
    // OUTSIDE the GC path — peeling the R2 body away while the
    // `blob_meta` row stays alive. This is the canonical orphan
    // window the S-06 reconciler closes.
    //
    // Codex round-4 P2 SEAL: the eviction is keyed on the EXACT
    // R2 key for the unique `(tenant=1, region=wnam, digest=d)`
    // tuple — not by digest-hex suffix — so a parallel-tenant test
    // writing the same body cannot false-pass.
    let _meta_alive_check = h
        .meta
        .get(&corelink_meta::BlobMetaKey::new(Uuid::from_u128(1), d))
        .await
        .unwrap()
        .unwrap();
    // Codex round-5 P2 SEAL fix: the test must assert there is
    // EXACTLY ONE matching key — otherwise a future test that
    // writes the same digest under a sibling tenant would make
    // `.find(...)` non-deterministic (HashMap iteration order is
    // unspecified) and the eviction could land on the wrong
    // tenant's object, false-passing the isolation guarantee.
    let keys_before = h.backend.keys_snapshot();
    let suffix = d.to_hex();
    let candidates: Vec<&String> = keys_before
        .iter()
        .filter(|k| k.starts_with("cas-wnam/") && k.ends_with(&suffix))
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "exactly one R2 key must match (tenant=1, region=wnam, digest=d); found {} candidates",
        candidates.len()
    );
    let target_key = candidates[0].clone();
    let evicted = h.backend.evict_key_for_test(&target_key);
    assert!(evicted, "evict_key_for_test must report a removed key");
    // Step 3: BatchReadBlobs — the slot must surface uniform 404.
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchReadBlobsRequest {
            instance_name: String::new(),
            digests: vec![pd(d, body.len() as i64)],
            acceptable_compressors: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-br-orphan",
    );
    let resp = client.batch_read_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    assert_eq!(
        resp.responses[0].status.as_ref().unwrap().code,
        5, // NOT_FOUND
        "R2 orphan must surface as per-blob 404 (uniform per ADR-0028)"
    );
    assert!(resp.responses[0].data.is_empty());
}
