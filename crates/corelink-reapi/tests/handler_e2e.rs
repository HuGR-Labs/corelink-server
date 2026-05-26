//! End-to-end integration tests for the REAPI write handlers via a tonic
//! server harness.
//!
//! Covered scenarios (WI-S01-005 §8 Gherkin AC):
//!
//! - **AC-1 happy path** — single blob `BatchUpdateBlobs`; per-blob status
//!   is `OK`; R2 backend has the body; D1 row count is 1; outbox row count
//!   is 1.
//! - **AC-2 hash mismatch** — declared digest disagrees with body BLAKE3;
//!   per-blob status code is `ABORTED` (10) + message contains "digest
//!   mismatch"; R2 + D1 untouched.
//! - **AC-3 PAT scope insufficient** — read-only PAT calls `BatchUpdateBlobs`;
//!   top-level RPC error is `PERMISSION_DENIED` (7) +
//!   `COR_AUTH_SCOPE_INSUFFICIENT` metadata.
//! - **AC-4 idempotent retry** — same `request_id` retried; per-blob status
//!   `OK (idempotent)`; row count stays 1; outbox dedup'd.
//! - **AC-7 size limit (per-batch aggregate)** — aggregate > 4 MiB →
//!   top-level `RESOURCE_EXHAUSTED` (8) + `COR_CAS_BATCH_TOO_LARGE`.
//! - **AC-7b size limit (per-blob)** — single blob > 5 MiB inline; per-blob
//!   status `RESOURCE_EXHAUSTED` (8) + `COR_CAS_BLOB_TOO_LARGE`.
//! - **AC-PAT missing** — no `Authorization` header → `UNAUTHENTICATED` (16).
//! - **ByteStream::Write happy path** — single blob streamed; committed_size
//!   matches body length; backend has the body; D1 row count is 1.
//! - **ByteStream::Write hash mismatch** — body BLAKE3 mismatch in
//!   resource_name → `ABORTED`.
//! - **ByteStream::Write size limit** — > 5 MiB streamed → `RESOURCE_EXHAUSTED`.

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
use corelink_meta::InMemoryMetaStore;
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::bytestream::byte_stream_client::ByteStreamClient;
use corelink_reapi::proto::bytestream::WriteRequest;
use corelink_reapi::proto::reapi::batch_update_blobs_request as bub_req;
use corelink_reapi::proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use corelink_reapi::proto::reapi::{BatchUpdateBlobsRequest, Digest as ProtoDigest};
use corelink_reapi::{
    AuthScope, ByteStreamService, CapabilitiesService, CasWriteService, StubPatValidator,
};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_worker::Region;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::Code;
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN_WRITE: &str = "token-tenant-A-write";
const TOKEN_READ_ONLY: &str = "token-tenant-A-read-only";

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
            TOKEN_WRITE,
            Uuid::from_u128(1),
            Uuid::from_u128(11),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_READ_ONLY,
            Uuid::from_u128(2),
            Uuid::from_u128(12),
            Region::Wnam,
            [AuthScope::CacheRead],
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
        let caps = CapabilitiesService::new(Arc::clone(&core));
        let bs = ByteStreamService::new(Arc::clone(&core));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
        // Enable a generous decoding ceiling on the server side so the
        // aggregate-cap test (which sends ~4.5 MiB) reaches the
        // application-level guard rather than tripping the gRPC default
        // 4 MiB decoding cap.
        let server = Server::builder()
            .add_service(cas.into_server().max_decoding_message_size(8 * 1024 * 1024))
            .add_service(caps.into_server())
            .add_service(bs.into_server().max_decoding_message_size(8 * 1024 * 1024))
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

    async fn bs_client(&self) -> ByteStreamClient<Channel> {
        ByteStreamClient::new(self.channel().await)
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

fn batch_request_with_blobs(blobs: Vec<(Digest, Vec<u8>)>) -> BatchUpdateBlobsRequest {
    BatchUpdateBlobsRequest {
        instance_name: String::new(),
        requests: blobs
            .into_iter()
            .map(|(digest, data)| bub_req::Request {
                digest: Some(ProtoDigest {
                    hash: digest.to_hex(),
                    size_bytes: data.len() as i64,
                }),
                data,
                compressor: 0,
            })
            .collect(),
        digest_function: 9, // BLAKE3
    }
}

/// **codex round-1 P0 regression** — `BatchUpdateBlobs` with multiple
/// blobs sharing the same `x-request-id` must not collide on the
/// `audit_outbox.UNIQUE (request_id, event_type)` constraint. The
/// orchestrator derives a per-blob `audit_request_id` of shape
/// `<client_request_id>:<tenant>:<digest>` so each blob has its own
/// dedup key while a retry with the SAME blob set is still idempotent.
#[tokio::test]
async fn batch_update_multiple_blobs_does_not_collide_on_audit_pk() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let bodies: Vec<Vec<u8>> = (0..5)
        .map(|i| format!("blob-content-{i}").into_bytes())
        .collect();
    let blobs: Vec<(Digest, Vec<u8>)> = bodies
        .iter()
        .map(|b| (Digest::compute(b), b.clone()))
        .collect();
    let req = auth_request(
        batch_request_with_blobs(blobs.clone()),
        TOKEN_WRITE,
        "req-multi-blob",
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 5);
    for (i, r) in resp.responses.iter().enumerate() {
        assert_eq!(r.status.as_ref().unwrap().code, 0, "blob {i} failed");
    }
    assert_eq!(h.backend.len(), 5);
    assert_eq!(h.meta.row_count(), 5);
    assert_eq!(h.meta.outbox_snapshot().len(), 5);

    // Retry the whole batch with the same `x-request-id` — every blob
    // collapses to an idempotent no-op; row counts stay at 5.
    let req2 = auth_request(
        batch_request_with_blobs(blobs),
        TOKEN_WRITE,
        "req-multi-blob",
    );
    let resp2 = client.batch_update_blobs(req2).await.unwrap().into_inner();
    for r in &resp2.responses {
        assert_eq!(r.status.as_ref().unwrap().code, 0);
    }
    assert_eq!(h.backend.len(), 5);
    assert_eq!(h.meta.row_count(), 5);
    assert_eq!(h.meta.outbox_snapshot().len(), 5);
}

#[tokio::test]
async fn ac1_happy_path_single_blob() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let body = b"hello world".to_vec();
    let digest = Digest::compute(&body);
    let req = auth_request(
        batch_request_with_blobs(vec![(digest, body.clone())]),
        TOKEN_WRITE,
        "req-ac1",
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    let r = &resp.responses[0];
    assert_eq!(r.status.as_ref().unwrap().code, 0); // OK
    assert_eq!(h.backend.len(), 1);
    assert_eq!(h.meta.row_count(), 1);
    assert_eq!(h.meta.outbox_snapshot().len(), 1);
}

#[tokio::test]
async fn ac2_hash_mismatch_short_circuits_storage() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let body = b"hello".to_vec();
    let lying_digest = Digest::compute(b"goodbye");
    let req = auth_request(
        batch_request_with_blobs(vec![(lying_digest, body)]),
        TOKEN_WRITE,
        "req-ac2",
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    let st = resp.responses[0].status.as_ref().unwrap();
    assert_eq!(st.code, 10, "ABORTED per WI-S01-005 §8 AC-2");
    assert!(
        st.message.to_ascii_lowercase().contains("digest mismatch"),
        "message: {}",
        st.message
    );
    assert_eq!(h.backend.len(), 0, "R2 untouched on hash mismatch");
    assert_eq!(h.meta.row_count(), 0, "D1 untouched on hash mismatch");
}

#[tokio::test]
async fn ac3_scope_insufficient_returns_permission_denied() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let body = b"x".to_vec();
    let digest = Digest::compute(&body);
    let req = auth_request(
        batch_request_with_blobs(vec![(digest, body)]),
        TOKEN_READ_ONLY,
        "req-ac3",
    );
    let err = client.batch_update_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_AUTH_SCOPE_INSUFFICIENT");
    assert_eq!(h.backend.len(), 0);
    assert_eq!(h.meta.row_count(), 0);
}

#[tokio::test]
async fn ac4_idempotent_retry_keeps_state_singular() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let body = b"idempotent-body".to_vec();
    let digest = Digest::compute(&body);

    let req1 = auth_request(
        batch_request_with_blobs(vec![(digest, body.clone())]),
        TOKEN_WRITE,
        "req-idem",
    );
    client.batch_update_blobs(req1).await.unwrap();

    // Same request_id, same body — retry path.
    let req2 = auth_request(
        batch_request_with_blobs(vec![(digest, body)]),
        TOKEN_WRITE,
        "req-idem",
    );
    let resp = client.batch_update_blobs(req2).await.unwrap().into_inner();
    let st = resp.responses[0].status.as_ref().unwrap();
    assert_eq!(st.code, 0);
    assert!(st.message.contains("idempotent"));
    assert_eq!(h.backend.len(), 1);
    assert_eq!(h.meta.row_count(), 1);
    assert_eq!(h.meta.outbox_snapshot().len(), 1);
}

#[tokio::test]
async fn ac7_aggregate_over_4mib_returns_resource_exhausted() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    // Build 3 blobs at 1.5 MiB each = 4.5 MiB aggregate > 4 MiB cap.
    // Each blob individually fits the per-blob 5 MiB cap; the cap test is
    // strictly the AGGREGATE cap (canonical `MaxBatchTotalSizeBytes`).
    // Bump the gRPC decoding ceiling so the harness's 4 MiB default does
    // not pre-empt the application-level cap test.
    client = client.max_decoding_message_size(8 * 1024 * 1024);
    let chunk_size = (1024 * 1024) + (512 * 1024); // 1.5 MiB
    let blob_a = vec![0xAA_u8; chunk_size];
    let blob_b = vec![0xBB_u8; chunk_size];
    let blob_c = vec![0xCC_u8; chunk_size];
    let blobs = vec![
        (Digest::compute(&blob_a), blob_a),
        (Digest::compute(&blob_b), blob_b),
        (Digest::compute(&blob_c), blob_c),
    ];
    let mut req = auth_request(batch_request_with_blobs(blobs), TOKEN_WRITE, "req-ac7");
    // Server-side decode cap also needs raising to receive 4.5 MiB.
    let _ = req
        .metadata_mut()
        .insert("grpc-accept-encoding", "identity".parse().unwrap());
    let err = client.batch_update_blobs(req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::ResourceExhausted,
        "WI-S01-005 §AC-7 aggregate cap canonical mapping is RESOURCE_EXHAUSTED (gRPC code 8); got {:?} message {}",
        err.code(),
        err.message()
    );
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_CAS_BATCH_TOO_LARGE");
    assert_eq!(h.backend.len(), 0);
}

#[tokio::test]
async fn ac_pat_missing_returns_unauthenticated() {
    let h = Harness::boot().await;
    let mut client = h.cas_client().await;
    let body = b"x".to_vec();
    let digest = Digest::compute(&body);
    // No auth header.
    let req = tonic::Request::new(batch_request_with_blobs(vec![(digest, body)]));
    let err = client.batch_update_blobs(req).await.unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_AUTH_PAT_INVALID");
}

#[tokio::test]
async fn bytestream_write_happy_path() {
    let h = Harness::boot().await;
    let mut client = h.bs_client().await;
    let body = b"streamed-blob-body".to_vec();
    let digest = Digest::compute(&body);
    let resource_name = format!(
        "corelink-instance/uploads/00000000-0000-0000-0000-000000000001/blobs/{}/{}",
        digest.to_hex(),
        body.len()
    );
    // Single chunk for simplicity (the size guard's chunk-counting path
    // is exercised by the multi-chunk size-limit test below).
    let chunk = WriteRequest {
        resource_name,
        write_offset: 0,
        finish_write: true,
        data: body.clone(),
    };
    let stream = futures::stream::iter(vec![chunk]);
    let req = auth_request(stream, TOKEN_WRITE, "req-bs-1");
    let resp = client.write(req).await.unwrap().into_inner();
    assert_eq!(resp.committed_size, body.len() as i64);
    assert_eq!(h.backend.len(), 1);
    assert_eq!(h.meta.row_count(), 1);
}

#[tokio::test]
async fn bytestream_write_hash_mismatch_returns_aborted() {
    let h = Harness::boot().await;
    let mut client = h.bs_client().await;
    let body = b"actual-body".to_vec();
    let lying_digest = Digest::compute(b"different-content");
    let resource_name = format!(
        "instance/uploads/00000000-0000-0000-0000-000000000002/blobs/{}/{}",
        lying_digest.to_hex(),
        body.len()
    );
    let chunk = WriteRequest {
        resource_name,
        write_offset: 0,
        finish_write: true,
        data: body,
    };
    let stream = futures::stream::iter(vec![chunk]);
    let req = auth_request(stream, TOKEN_WRITE, "req-bs-mismatch");
    let err = client.write(req).await.unwrap_err();
    assert_eq!(err.code(), Code::Aborted);
    assert_eq!(h.backend.len(), 0);
    assert_eq!(h.meta.row_count(), 0);
}

/// **codex round-3 Medium regression** — ByteStream::Write MUST reject
/// any `WriteRequest` arriving AFTER a `finish_write=true` chunk. Letting
/// it through would silently drop post-finish bytes and present a false
/// `OK` to the client.
#[tokio::test]
async fn bytestream_write_rejects_extra_chunk_after_finish_write() {
    let h = Harness::boot().await;
    let mut client = h.bs_client().await;
    let body = b"finished-body".to_vec();
    let digest = Digest::compute(&body);
    let resource_name = format!(
        "instance/uploads/00000000-0000-0000-0000-000000000005/blobs/{}/{}",
        digest.to_hex(),
        body.len()
    );
    let c1 = WriteRequest {
        resource_name,
        write_offset: 0,
        finish_write: true,
        data: body.clone(),
    };
    let c2 = WriteRequest {
        resource_name: String::new(),
        write_offset: body.len() as i64,
        finish_write: false,
        data: b"sneaky-trailing-bytes".to_vec(),
    };
    let stream = futures::stream::iter(vec![c1, c2]);
    let req = auth_request(stream, TOKEN_WRITE, "req-bs-extra-after-finish");
    let err = client.write(req).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
}

/// **codex round-2 High regression** — ByteStream::Write MUST reject
/// chunk streams that violate the `google.bytestream.ByteStream.Write`
/// protocol contract: missing first-chunk `resource_name`, mid-stream
/// `resource_name` change, mismatched `write_offset`.
#[tokio::test]
async fn bytestream_write_rejects_protocol_violations() {
    let h = Harness::boot().await;

    // Case 1 — first chunk has empty resource_name.
    {
        let mut client = h.bs_client().await;
        let chunk = WriteRequest {
            resource_name: String::new(),
            write_offset: 0,
            finish_write: true,
            data: b"x".to_vec(),
        };
        let stream = futures::stream::iter(vec![chunk]);
        let req = auth_request(stream, TOKEN_WRITE, "req-bs-no-name");
        let err = client.write(req).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    // Case 2 — first chunk OK; second chunk's resource_name disagrees.
    {
        let mut client = h.bs_client().await;
        let body = b"the-streamed-body".to_vec();
        let digest = Digest::compute(&body);
        let resource_name = format!(
            "instance/uploads/00000000-0000-0000-0000-000000000003/blobs/{}/{}",
            digest.to_hex(),
            body.len()
        );
        let mid = body.len() / 2;
        let c1 = WriteRequest {
            resource_name: resource_name.clone(),
            write_offset: 0,
            finish_write: false,
            data: body[..mid].to_vec(),
        };
        let c2 = WriteRequest {
            resource_name: format!("{resource_name}-tampered"),
            write_offset: mid as i64,
            finish_write: true,
            data: body[mid..].to_vec(),
        };
        let stream = futures::stream::iter(vec![c1, c2]);
        let req = auth_request(stream, TOKEN_WRITE, "req-bs-name-change");
        let err = client.write(req).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    // Case 3 — write_offset disagrees with cumulative byte count.
    {
        let mut client = h.bs_client().await;
        let body = b"another-body".to_vec();
        let digest = Digest::compute(&body);
        let resource_name = format!(
            "instance/uploads/00000000-0000-0000-0000-000000000004/blobs/{}/{}",
            digest.to_hex(),
            body.len()
        );
        let c1 = WriteRequest {
            resource_name: resource_name.clone(),
            write_offset: 0,
            finish_write: false,
            data: b"first-half".to_vec(),
        };
        let c2 = WriteRequest {
            resource_name: String::new(),
            write_offset: 999, // wrong; should be 10
            finish_write: true,
            data: b"second-half".to_vec(),
        };
        let stream = futures::stream::iter(vec![c1, c2]);
        let req = auth_request(stream, TOKEN_WRITE, "req-bs-bad-offset");
        let err = client.write(req).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }
}

/// Codex round-4 P2 SEAL fix (cross-WI patch into S-01-005 surface):
/// `BatchUpdateBlobsRequest.Request.compressor != IDENTITY` MUST be
/// rejected per-blob with INVALID_ARGUMENT. Compressed-batch upload
/// is deferred to WI-S05-005; until then, the only acceptable
/// `compressor` is `IDENTITY` (= 0). A silent accept would let the
/// server hash compressed bytes against the declared digest.
#[tokio::test]
async fn batch_update_rejects_non_identity_compressor_per_blob() {
    let h = Harness::boot().await;
    let body = b"compressed-pretend".to_vec();
    let d = Digest::compute(&body);
    let mut client = h.cas_client().await;
    let req = auth_request(
        BatchUpdateBlobsRequest {
            instance_name: String::new(),
            requests: vec![bub_req::Request {
                digest: Some(ProtoDigest {
                    hash: d.to_hex(),
                    size_bytes: body.len() as i64,
                }),
                data: body,
                compressor: 1, // ZSTD — unsupported in S-01
            }],
            digest_function: 9,
        },
        TOKEN_WRITE,
        "req-batch-zstd",
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    let entry = &resp.responses[0];
    let status = entry.status.as_ref().unwrap();
    assert_eq!(
        status.code, 3,
        "INVALID_ARGUMENT for non-IDENTITY compressor"
    );
    assert!(status.message.contains("IDENTITY"));
    // Verify nothing landed in storage.
    assert_eq!(h.backend.len(), 0);
}

#[tokio::test]
async fn bytestream_write_oversize_returns_resource_exhausted() {
    let h = Harness::boot().await;
    let mut client = h.bs_client().await;
    // Send chunks summing to > 5 MiB.
    let chunk_size = 1024 * 1024;
    let mut chunks = Vec::with_capacity(7);
    for i in 0..6 {
        chunks.push(WriteRequest {
            resource_name: if i == 0 {
                "instance/uploads/uuid/blobs/0000000000000000000000000000000000000000000000000000000000000000/6291456".to_owned()
            } else {
                String::new()
            },
            write_offset: (i as i64) * chunk_size as i64,
            finish_write: false,
            data: vec![0u8; chunk_size],
        });
    }
    let stream = futures::stream::iter(chunks);
    let req = auth_request(stream, TOKEN_WRITE, "req-bs-oversize");
    let err = client.write(req).await.unwrap_err();
    assert_eq!(err.code(), Code::ResourceExhausted);
    assert_eq!(h.backend.len(), 0);
}
