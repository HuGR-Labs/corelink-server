//! End-to-end gRPC integration tests for `FindMissingBlobs`
//! (WI-S02-002 §8 Gherkin AC scenarios).
//!
//! Covers:
//!
//! - **AC-FM-1 happy path** — Tenant A writes 3 blobs; queries with 5
//!   digests (3 present + 2 absent); response has the 2 absent only,
//!   in input order.
//! - **AC-FM-2 cross-tenant masked** — Tenant B asks for Tenant A's
//!   digest; response surfaces it as missing (no oracle).
//! - **AC-FM-3 batch size limit** — 1001 digests → OUT_OF_RANGE +
//!   `COR_CAS_BATCH_SIZE_EXCEEDED`.
//! - **AC-FM-4 PAT scope insufficient** — write-only PAT cannot
//!   invoke FindMissingBlobs → `PERMISSION_DENIED`.
//! - **AC-FM-5 cache:r alone is insufficient** — read-only PAT
//!   without `cache:find-missing` is rejected (canonical scope
//!   separation per `auth_model.md §3.1 L188` — the discovery
//!   capability is a distinct scope from download read).
//! - **AC-FM-6 cache:find-missing alone is sufficient** — discovery-
//!   only PAT can invoke without cache:r.
//! - **AC-FM-7 PAT missing** — no Authorization header →
//!   UNAUTHENTICATED.
//! - **AC-FM-8 malformed digest** — top-level INVALID_ARGUMENT.
//! - **AC-FM-9 tombstoned blob** — surfaces as missing.
//! - **AC-FM-10 duplicate input digest** — preserved as duplicate
//!   output (server does NOT collapse client-side dedup).
//! - **AC-FM-11 empty input** — returns empty `missing_blob_digests`
//!   (no top-level error).

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

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitSoftDeleteRequest, InMemoryMetaStore, MetaStore,
    RequestId,
};
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::reapi::batch_update_blobs_request as bub_req;
use corelink_reapi::proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use corelink_reapi::proto::reapi::{
    BatchUpdateBlobsRequest, Digest as ProtoDigest, FindMissingBlobsRequest,
};
use corelink_reapi::{AuthScope, CasWriteService, StubPatValidator};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_replication::region_resolver::Region;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::Code;
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN_RW_A: &str = "token-A-rw";
const TOKEN_RW_B: &str = "token-B-rw";
const TOKEN_WRITE_ONLY_A: &str = "token-A-write-only";
const TOKEN_READ_ONLY_A: &str = "token-A-read-only";
const TOKEN_FIND_ONLY_A: &str = "token-A-find-only";

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
            [
                AuthScope::CacheRead,
                AuthScope::CacheWrite,
                AuthScope::CacheFindMissing,
            ],
        );
        pat.insert(
            TOKEN_RW_B,
            Uuid::from_u128(2),
            Uuid::from_u128(12),
            Region::Wnam,
            [
                AuthScope::CacheRead,
                AuthScope::CacheWrite,
                AuthScope::CacheFindMissing,
            ],
        );
        pat.insert(
            TOKEN_WRITE_ONLY_A,
            Uuid::from_u128(3),
            Uuid::from_u128(13),
            Region::Wnam,
            [AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_READ_ONLY_A,
            Uuid::from_u128(1),
            Uuid::from_u128(14),
            Region::Wnam,
            [AuthScope::CacheRead],
        );
        pat.insert(
            TOKEN_FIND_ONLY_A,
            Uuid::from_u128(1),
            Uuid::from_u128(15),
            Region::Wnam,
            [AuthScope::CacheFindMissing],
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

        // Allow the server to bind before clients connect.
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
async fn ac_fm_1_happy_path_returns_only_absent_subset_in_input_order() {
    let h = Harness::boot().await;
    let d_present_1 = write_blob_via_grpc(&h, b"have-1", TOKEN_RW_A, "req-h1").await;
    let d_present_2 = write_blob_via_grpc(&h, b"have-2", TOKEN_RW_A, "req-h2").await;
    let d_present_3 = write_blob_via_grpc(&h, b"have-3", TOKEN_RW_A, "req-h3").await;
    let d_absent_1 = Digest::compute(b"missing-1");
    let d_absent_2 = Digest::compute(b"missing-2");

    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![
                pd(d_present_1, 6),
                pd(d_absent_1, 9),
                pd(d_present_2, 6),
                pd(d_absent_2, 9),
                pd(d_present_3, 6),
            ],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-fm-happy",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    let missing: Vec<String> = resp
        .missing_blob_digests
        .iter()
        .map(|p| p.hash.clone())
        .collect();
    assert_eq!(
        missing,
        vec![d_absent_1.to_hex(), d_absent_2.to_hex()],
        "response must list only absent digests, in input order"
    );
}

#[tokio::test]
async fn ac_fm_2_cross_tenant_masked_as_missing() {
    let h = Harness::boot().await;
    // A writes a secret blob.
    let d_a = write_blob_via_grpc(&h, b"A-secret", TOKEN_RW_A, "req-A1").await;

    // B asks if A's blob exists. It must surface as "missing" (no
    // existence oracle).
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d_a, 8)],
            digest_function: 0,
        },
        TOKEN_RW_B,
        "req-B-probe",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.missing_blob_digests.len(), 1);
    assert_eq!(resp.missing_blob_digests[0].hash, d_a.to_hex());
}

#[tokio::test]
async fn ac_fm_3_batch_size_limit_surfaces_out_of_range() {
    let h = Harness::boot().await;
    let mut digests = Vec::with_capacity(1001);
    for i in 0..1001u32 {
        digests.push(pd(Digest::compute(format!("body-{i}").as_bytes()), 7));
    }
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: digests,
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-fm-overflow",
    );
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::OutOfRange);
    let taxonomy = err
        .metadata()
        .get("x-corelink-error-code")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert_eq!(taxonomy, "COR_CAS_BATCH_SIZE_EXCEEDED");
}

#[tokio::test]
async fn ac_fm_4_pat_scope_insufficient_for_write_only_token() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"anything");
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d, 8)],
            digest_function: 0,
        },
        TOKEN_WRITE_ONLY_A,
        "req-write-only",
    );
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn ac_fm_5_cache_read_alone_does_not_imply_find_missing() {
    // WI-S02-002 SEAL codex round-2 P1 fix: `auth_model.md §3.1 L188`
    // treats `cache:r` and `cache:find-missing` as DISTINCT scopes.
    // A read-only PAT (no `cache:find-missing`) MUST be rejected when
    // it tries to invoke `FindMissingBlobs` — otherwise the new
    // `cache:find-missing` scope is unenforceable as a discovery-only
    // boundary for download-capable tokens.
    let h = Harness::boot().await;
    let d = Digest::compute(b"r-only");
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d, 6)],
            digest_function: 0,
        },
        TOKEN_READ_ONLY_A,
        "req-read-only",
    );
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
    let taxonomy = err
        .metadata()
        .get("x-corelink-error-code")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert_eq!(taxonomy, "COR_AUTH_SCOPE_INSUFFICIENT");
}

#[tokio::test]
async fn ac_fm_6_cache_find_missing_alone_is_sufficient() {
    let h = Harness::boot().await;
    let d_present = write_blob_via_grpc(&h, b"find-only", TOKEN_RW_A, "req-find-pre").await;
    let d_absent = Digest::compute(b"never-find-only");
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d_present, 9), pd(d_absent, 15)],
            digest_function: 0,
        },
        TOKEN_FIND_ONLY_A,
        "req-find-only",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.missing_blob_digests.len(), 1);
    assert_eq!(resp.missing_blob_digests[0].hash, d_absent.to_hex());
}

#[tokio::test]
async fn ac_fm_7_missing_pat_yields_unauthenticated() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"x");
    let mut client = h.cas_client().await;
    // No bearer token.
    let req = tonic::Request::new(FindMissingBlobsRequest {
        instance_name: String::new(),
        blob_digests: vec![pd(d, 1)],
        digest_function: 0,
    });
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn ac_fm_8_malformed_digest_top_level_invalid_argument() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![ProtoDigest {
                hash: "not-hex".to_owned(),
                size_bytes: 0,
            }],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-bad-digest",
    );
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    let taxonomy = err
        .metadata()
        .get("x-corelink-error-code")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert_eq!(taxonomy, "COR_CAS_BAD_DIGEST");
}

#[tokio::test]
async fn ac_fm_9_tombstoned_blob_surfaces_as_missing() {
    let h = Harness::boot().await;
    let d = write_blob_via_grpc(&h, b"doomed", TOKEN_RW_A, "req-doomed").await;

    // Tombstone via direct meta-store mutation (the GC path lands in
    // S-06 alongside its own RPC; for now we drive the soft-delete
    // by hand to exercise the tombstone-vs-missing equivalence).
    let key = BlobMetaKey::new(Uuid::from_u128(1), d);
    h.meta
        .commit_soft_delete(CommitSoftDeleteRequest {
            key,
            now_ms: 1_700_000_001_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0xDEAD_DEAD_BEEF_BEEF),
                request_id: RequestId::new("req-tomb-fm"),
                event_type: AuditEventType::CasSoftDeleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();

    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d, 6)],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-tomb-find",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.missing_blob_digests.len(), 1);
    assert_eq!(resp.missing_blob_digests[0].hash, d.to_hex());
}

#[tokio::test]
async fn ac_fm_10_duplicate_input_preserves_duplicates_in_response() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"never-batched");
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d, 13), pd(d, 13), pd(d, 13)],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-fm-dup",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.missing_blob_digests.len(), 3);
    for p in &resp.missing_blob_digests {
        assert_eq!(p.hash, d.to_hex());
    }
}

#[tokio::test]
async fn ac_fm_11_empty_input_returns_empty_response() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![],
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-fm-empty",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert!(resp.missing_blob_digests.is_empty());
}

#[tokio::test]
async fn ac_fm_12_non_blake3_digest_function_rejected() {
    let h = Harness::boot().await;
    let d = Digest::compute(b"x");
    let mut client = h.cas_client().await;
    // SHA256 = 1 in the canonical proto.
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![pd(d, 1)],
            digest_function: 1,
        },
        TOKEN_RW_A,
        "req-fm-sha256",
    );
    let err = client.find_missing_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    let taxonomy = err
        .metadata()
        .get("x-corelink-error-code")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert_eq!(taxonomy, "COR_CAS_DIGEST_FUNCTION_UNSUPPORTED");
}

#[tokio::test]
async fn ac_fm_13_at_max_batch_size_completes_successfully() {
    let h = Harness::boot().await;
    let mut digests = Vec::with_capacity(1000);
    for i in 0..1000u32 {
        digests.push(pd(Digest::compute(format!("body-{i}").as_bytes()), 7));
    }
    let mut client = h.cas_client().await;
    let req = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: digests,
            digest_function: 0,
        },
        TOKEN_RW_A,
        "req-fm-1000",
    );
    let resp = client.find_missing_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.missing_blob_digests.len(), 1000);
}

#[tokio::test]
async fn ac_fm_14_mixed_size_same_hash_per_slot_independence() {
    // codex round-2 P1 / P2 SEAL coverage: REAPI Digest identity is
    // `(hash, size_bytes)`. A request that carries the same hash with
    // both the canonical size AND a wrong size MUST surface only the
    // wrong-size slot as missing — irrespective of input ordering.
    // This pins the per-slot reconstruction logic in the
    // `FindMissingBlobs` handler against the size-aware orchestrator
    // (`find_missing_with_sizes`) outcome.
    let h = Harness::boot().await;
    // Pre-write a blob whose canonical body length is 6 bytes
    // (b"r-only" → 6).
    let d_present = write_blob_via_grpc(&h, b"r-only", TOKEN_RW_A, "req-mixed-size").await;

    // Order A: [present-correct, present-wrongsize] — only slot 1 missing.
    let mut client_a = h.cas_client().await;
    let req_a = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![
                pd(d_present, 6),   // canonical size
                pd(d_present, 999), // wrong size — REAPI identity says missing
            ],
            digest_function: 0,
        },
        TOKEN_FIND_ONLY_A,
        "req-mixed-size-A",
    );
    let resp_a = client_a
        .find_missing_blobs(req_a)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        resp_a.missing_blob_digests.len(),
        1,
        "exactly the wrong-size slot must surface as missing"
    );
    assert_eq!(resp_a.missing_blob_digests[0].hash, d_present.to_hex());
    assert_eq!(resp_a.missing_blob_digests[0].size_bytes, 999);

    // Order B: [present-wrongsize, present-correct] — only slot 0 missing.
    let mut client_b = h.cas_client().await;
    let req_b = auth_request(
        FindMissingBlobsRequest {
            instance_name: String::new(),
            blob_digests: vec![
                pd(d_present, 999), // wrong size first
                pd(d_present, 6),   // canonical size second
            ],
            digest_function: 0,
        },
        TOKEN_FIND_ONLY_A,
        "req-mixed-size-B",
    );
    let resp_b = client_b
        .find_missing_blobs(req_b)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp_b.missing_blob_digests.len(), 1);
    assert_eq!(resp_b.missing_blob_digests[0].hash, d_present.to_hex());
    assert_eq!(resp_b.missing_blob_digests[0].size_bytes, 999);
}
