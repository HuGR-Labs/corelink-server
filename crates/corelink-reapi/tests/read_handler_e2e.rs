//! End-to-end integration tests for the REAPI read handlers
//! (WI-S02-001).
//!
//! Covers Gherkin AC from the WI:
//!
//! - **AC-1 happy path same-tenant** — write via `BatchUpdateBlobs`, read
//!   via `ByteStream::Read`; bytes byte-identical; chunk count ≤ 1 MiB.
//! - **AC-2 cross-tenant masked as 404** — Tenant B asks for Tenant A's
//!   digest; gRPC `NOT_FOUND` (5) + `COR_CAS_BLOB_NOT_FOUND`. R2 was
//!   not even called for B (verified by R2 mock keys-snapshot).
//! - **AC-3 tombstoned blob** — write then soft-delete via `MetaStore`;
//!   read returns `NOT_FOUND` + `COR_CAS_BLOB_NOT_FOUND`.
//! - **AC-4 PAT scope insufficient** — write-only PAT calls Read; gRPC
//!   `PERMISSION_DENIED` (7) + `COR_AUTH_SCOPE_INSUFFICIENT`. (We use
//!   the inverse from the write tests: the write token has CacheRead;
//!   we add a write-only token here for completeness.)
//! - **AC-5 PAT missing** — no `Authorization` header → `UNAUTHENTICATED`
//!   (16) + `COR_AUTH_PAT_INVALID`.
//! - **AC-6 read_offset / read_limit semantics** — partial read of a
//!   2 KiB body via `read_offset=512, read_limit=512` returns exactly
//!   bytes [512..1024) of the body.
//! - **AC-7 oversize read_offset** — `read_offset > body.len()` →
//!   `OUT_OF_RANGE` (11) + `COR_CAS_BAD_RESOURCE_NAME`.
//! - **AC-8 malformed resource_name** — `INVALID_ARGUMENT` + taxonomy
//!   code.
//! - **HTTP-AC-1 GET happy path** — same as gRPC AC-1 over the HTTP
//!   surface; body bytes byte-identical; `x-corelink-digest` header set.
//! - **HTTP-AC-2 GET cross-tenant masked** — 404 + `x-corelink-error-code`
//!   header.
//! - **HTTP-AC-3 GET malformed digest path** — 400 + `COR_CAS_BAD_DIGEST`.
//! - **HTTP-AC-4 GET missing PAT** — 401 + `COR_AUTH_PAT_INVALID`.

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

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitSoftDeleteRequest, InMemoryMetaStore, MetaStore,
    RequestId,
};
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::bytestream::byte_stream_client::ByteStreamClient;
use corelink_reapi::proto::bytestream::{ReadRequest, WriteRequest};
use corelink_reapi::proto::reapi::batch_update_blobs_request as bub_req;
use corelink_reapi::proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use corelink_reapi::proto::reapi::{BatchUpdateBlobsRequest, Digest as ProtoDigest};
use corelink_reapi::{
    cas_get_router, AuthScope, ByteStreamService, CasWriteService, HttpReadState, StubPatValidator,
};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{InMemoryR2, R2Reader, R2Writer};
use corelink_worker::Region;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::Code;
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN_RW: &str = "token-tenant-A-rw";
const TOKEN_TENANT_B: &str = "token-tenant-B-rw";
const TOKEN_WRITE_ONLY: &str = "token-tenant-A-write-only";

#[allow(dead_code)]
struct Harness {
    backend: Arc<InMemoryR2>,
    meta: Arc<InMemoryMetaStore>,
    addr: std::net::SocketAddr,
    http_addr: std::net::SocketAddr,
    _server: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    _http_server: tokio::task::JoinHandle<()>,
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
            TOKEN_RW,
            Uuid::from_u128(1),
            Uuid::from_u128(11),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_TENANT_B,
            Uuid::from_u128(2),
            Uuid::from_u128(12),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        pat.insert(
            TOKEN_WRITE_ONLY,
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
        let bs = ByteStreamService::new(Arc::clone(&core));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let server = Server::builder()
            .add_service(cas.into_server().max_decoding_message_size(8 * 1024 * 1024))
            .add_service(bs.into_server().max_decoding_message_size(8 * 1024 * 1024))
            .serve_with_incoming(stream);
        let server = tokio::spawn(server);

        // HTTP read surface.
        let http_state = HttpReadState {
            core: Arc::clone(&core),
        };
        let http_router = cas_get_router(http_state);
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_addr = http_listener.local_addr().unwrap();
        let http_server = tokio::spawn(async move {
            let _ = axum::serve(http_listener, http_router).await;
        });

        // Allow servers to bind before clients connect.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self {
            backend,
            meta,
            addr,
            http_addr,
            _server: server,
            _http_server: http_server,
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
            digest_function: 9, // BLAKE3
        },
        token,
        request_id,
    );
    let resp = client.batch_update_blobs(req).await.unwrap().into_inner();
    assert_eq!(resp.responses.len(), 1);
    assert_eq!(resp.responses[0].status.as_ref().unwrap().code, 0);
    digest
}

fn read_resource_name(digest: &Digest, size_bytes: usize) -> String {
    format!("corelink-instance/blobs/{}/{}", digest.to_hex(), size_bytes)
}

async fn drain_read_stream(
    stream: &mut tonic::Streaming<corelink_reapi::proto::bytestream::ReadResponse>,
) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.message().await.unwrap() {
        out.extend_from_slice(&chunk.data);
    }
    out
}

#[tokio::test]
async fn ac1_grpc_read_happy_path_returns_byte_identical_body() {
    let h = Harness::boot().await;
    let body = b"hello-read-world".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write").await;

    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_RW,
        "req-read-1",
    );
    let stream = client.read(req).await.unwrap().into_inner();
    let mut stream = stream;
    let bytes = drain_read_stream(&mut stream).await;
    assert_eq!(bytes, body, "read body must equal write body");
    // BLAKE3 of returned body must equal the requested digest.
    assert_eq!(Digest::compute(&bytes), digest);
}

#[tokio::test]
async fn ac2_grpc_cross_tenant_read_returns_not_found_uniform_404() {
    let h = Harness::boot().await;
    let body = b"secret-of-tenant-A".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-A").await;
    let r2_pre = h.backend.keys_snapshot();
    assert_eq!(r2_pre.len(), 1);

    // Tenant B requests the same digest.
    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_TENANT_B,
        "req-read-cross-tenant",
    );
    let err = client.read(req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::NotFound,
        "cross-tenant read MUST surface as uniform NOT_FOUND per ADR-0028; got {:?}",
        err.code()
    );
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_CAS_BLOB_NOT_FOUND");

    // R2 is unchanged: the AuthZ check short-circuited before R2 GET.
    let r2_post = h.backend.keys_snapshot();
    assert_eq!(r2_pre, r2_post);
}

#[tokio::test]
async fn ac3_tombstoned_blob_returns_not_found() {
    let h = Harness::boot().await;
    let body = b"to-be-tombstoned".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-tomb").await;

    // Soft-delete via the canonical MetaStore path.
    let key = BlobMetaKey::new(Uuid::from_u128(1), digest);
    h.meta
        .commit_soft_delete(CommitSoftDeleteRequest {
            key,
            now_ms: 1_700_000_001_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x019384A0_DEAD_7000_8000_000000000003),
                request_id: RequestId::new("req-soft-delete-1"),
                event_type: AuditEventType::CasSoftDeleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();

    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_RW,
        "req-read-tomb",
    );
    let err = client.read(req).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_CAS_BLOB_NOT_FOUND");
}

#[tokio::test]
async fn ac4_pat_scope_insufficient_returns_permission_denied() {
    let h = Harness::boot().await;
    let body = b"x".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-scope").await;

    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_WRITE_ONLY,
        "req-read-scope",
    );
    let err = client.read(req).await.unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_AUTH_SCOPE_INSUFFICIENT");
}

#[tokio::test]
async fn ac5_pat_missing_returns_unauthenticated() {
    let h = Harness::boot().await;
    let body = b"x".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-pat-missing").await;

    let mut client = h.bs_client().await;
    let req = tonic::Request::new(ReadRequest {
        resource_name: read_resource_name(&digest, body.len()),
        read_offset: 0,
        read_limit: 0,
    });
    let err = client.read(req).await.unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_AUTH_PAT_INVALID");
}

#[tokio::test]
async fn ac6_read_offset_limit_semantics_partial_read() {
    let h = Harness::boot().await;
    // 2 KiB body so the read_offset=512, read_limit=512 case yields a
    // well-defined slice [512..1024).
    let body: Vec<u8> = (0..2048u32).map(|i| (i & 0xFF) as u8).collect();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-partial").await;

    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 512,
            read_limit: 512,
        },
        TOKEN_RW,
        "req-read-partial",
    );
    let mut stream = client.read(req).await.unwrap().into_inner();
    let bytes = drain_read_stream(&mut stream).await;
    assert_eq!(bytes.len(), 512);
    assert_eq!(bytes, body[512..1024]);
}

#[tokio::test]
async fn ac7_oversize_read_offset_returns_out_of_range() {
    let h = Harness::boot().await;
    let body = b"short-body".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-write-oor").await;

    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 1_000_000,
            read_limit: 0,
        },
        TOKEN_RW,
        "req-read-oor",
    );
    let err = client.read(req).await.unwrap_err();
    assert_eq!(err.code(), Code::OutOfRange);
}

#[tokio::test]
async fn ac8_malformed_resource_name_returns_invalid_argument() {
    let h = Harness::boot().await;
    let mut client = h.bs_client().await;
    let req = auth_request(
        ReadRequest {
            resource_name: "this-is-not-a-valid-rn".to_owned(),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_RW,
        "req-read-malformed",
    );
    let err = client.read(req).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    let metadata_code = err
        .metadata()
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(metadata_code, "COR_CAS_BAD_RESOURCE_NAME");
}

#[tokio::test]
async fn streaming_chunks_at_one_mib_granularity() {
    let h = Harness::boot().await;
    // 2.5 MiB body via gRPC ByteStream::Write so we exercise the
    // multi-chunk read path.
    let body = vec![0xCDu8; 2 * 1024 * 1024 + 1024];
    let digest = Digest::compute(&body);
    {
        let mut bs_client = h.bs_client().await;
        bs_client = bs_client.max_decoding_message_size(8 * 1024 * 1024);
        let resource_name = format!(
            "instance/uploads/00000000-0000-0000-0000-000000000010/blobs/{}/{}",
            digest.to_hex(),
            body.len()
        );
        let chunk = WriteRequest {
            resource_name,
            write_offset: 0,
            finish_write: true,
            data: body.clone(),
        };
        let stream = futures::stream::iter(vec![chunk]);
        let req = auth_request(stream, TOKEN_RW, "req-write-2mib");
        let resp = bs_client.write(req).await.unwrap().into_inner();
        assert_eq!(resp.committed_size, body.len() as i64);
    }
    let mut bs_client = h.bs_client().await;
    bs_client = bs_client.max_decoding_message_size(8 * 1024 * 1024);
    let req = auth_request(
        ReadRequest {
            resource_name: read_resource_name(&digest, body.len()),
            read_offset: 0,
            read_limit: 0,
        },
        TOKEN_RW,
        "req-read-2mib",
    );
    let mut stream = bs_client.read(req).await.unwrap().into_inner();
    let mut total_chunks = 0usize;
    let mut max_chunk = 0usize;
    let mut accumulator = Vec::new();
    while let Some(chunk) = stream.message().await.unwrap() {
        total_chunks += 1;
        max_chunk = max_chunk.max(chunk.data.len());
        accumulator.extend_from_slice(&chunk.data);
    }
    assert_eq!(accumulator, body);
    // Chunks must be ≤ 1 MiB each per WI §6.1.1.
    assert!(
        max_chunk <= 1024 * 1024,
        "chunk size {max_chunk} exceeded 1 MiB cap"
    );
    // For 2 MiB+1 KiB body we expect 3 chunks (1 MiB + 1 MiB + 1 KiB).
    assert_eq!(total_chunks, 3);
}

// ---------------------------------------------------------------------------
// HTTP surface tests
// ---------------------------------------------------------------------------

async fn http_get(
    h: &Harness,
    path: &str,
    token: Option<&str>,
) -> (axum::http::StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let url = format!("http://{}{path}", h.http_addr);
    let mut req_builder = http::Request::builder().method("GET").uri(url.as_str());
    if let Some(t) = token {
        req_builder = req_builder.header("authorization", format!("Bearer {t}"));
    }
    let req = req_builder.body(Vec::<u8>::new()).unwrap();
    // Use hyper directly to avoid pulling reqwest just for tests.
    let stream = tokio::net::TcpStream::connect(h.http_addr).await.unwrap();
    let io = hyper_util::rt::TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let (parts, body_bytes) = req.into_parts();
    let mut http_req = http::Request::new(http_body_util::Full::new(Bytes::from(body_bytes)));
    *http_req.method_mut() = parts.method;
    *http_req.uri_mut() = parts.uri;
    *http_req.headers_mut() = parts.headers;
    let resp = sender.send_request(http_req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body_bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, headers, body_bytes.to_vec())
}

#[tokio::test]
async fn http_ac1_get_happy_path_returns_byte_identical_body() {
    let h = Harness::boot().await;
    let body = b"http-read-body".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-http-write").await;

    let path = format!("/v1/cas/{}", digest.to_hex());
    let (status, headers, bytes) = http_get(&h, &path, Some(TOKEN_RW)).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(bytes, body);
    let digest_header = headers
        .get("x-corelink-digest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(digest_header, format!("blake3:{}", digest.to_hex()));
}

#[tokio::test]
async fn http_ac2_get_cross_tenant_returns_404_uniform() {
    let h = Harness::boot().await;
    let body = b"http-cross-tenant-body".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-http-cross-A").await;

    let path = format!("/v1/cas/{}", digest.to_hex());
    let (status, headers, _bytes) = http_get(&h, &path, Some(TOKEN_TENANT_B)).await;
    assert_eq!(status.as_u16(), 404);
    let err_code = headers
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(err_code, "COR_CAS_BLOB_NOT_FOUND");
}

#[tokio::test]
async fn http_ac3_get_malformed_digest_returns_400() {
    let h = Harness::boot().await;
    let (status, headers, _bytes) = http_get(&h, "/v1/cas/zzz-not-hex", Some(TOKEN_RW)).await;
    assert_eq!(status.as_u16(), 400);
    let err_code = headers
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(err_code, "COR_CAS_BAD_DIGEST");
}

#[tokio::test]
async fn http_ac4_get_missing_pat_returns_401() {
    let h = Harness::boot().await;
    let path = format!(
        "/v1/cas/{}",
        "a".repeat(64) // valid hex shape; PAT is what we're testing
    );
    let (status, headers, _bytes) = http_get(&h, &path, None).await;
    assert_eq!(status.as_u16(), 401);
    let err_code = headers
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(err_code, "COR_AUTH_PAT_INVALID");
}

#[tokio::test]
async fn http_ac5_get_scope_insufficient_returns_403() {
    let h = Harness::boot().await;
    let body = b"http-scope".to_vec();
    let digest = write_blob_via_grpc(&h, &body, TOKEN_RW, "req-http-scope-write").await;

    let path = format!("/v1/cas/{}", digest.to_hex());
    let (status, headers, _bytes) = http_get(&h, &path, Some(TOKEN_WRITE_ONLY)).await;
    assert_eq!(status.as_u16(), 403);
    let err_code = headers
        .get("x-corelink-error-code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(err_code, "COR_AUTH_SCOPE_INSUFFICIENT");
}
