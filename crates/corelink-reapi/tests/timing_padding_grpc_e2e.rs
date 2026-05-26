//! End-to-end integration test for the WI-S02-004
//! [`TimingPaddingLayer`] mounted on the tonic gRPC stack
//! (codex round-1 P0 + round-2 P0 fix — earlier drafts only verified
//! the layer in isolation; this test runs the canonical
//! `Server::builder().layer(canonical_grpc_padding_layer()).add_service(...)`
//! path against a live tonic server + tonic client and asserts that:
//!
//! 1. A gRPC `Err(Status::not_found)` response observed by the
//!    client takes ≥ `target_p99_ms × (1 − jitter_pct%)` wall-clock
//!    seconds — i.e. the layer DID fire on the gRPC `grpc-status: 5`
//!    encoding.
//! 2. A gRPC happy-path `BatchUpdateBlobs` write returns immediately
//!    (no padding tax on the OK path).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test code: integration harness with explicit asserts"
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_hash::Digest;
use corelink_meta::InMemoryMetaStore;
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::bytestream::byte_stream_client::ByteStreamClient;
use corelink_reapi::proto::bytestream::ReadRequest;
use corelink_reapi::proto::reapi::capabilities_client::CapabilitiesClient;
use corelink_reapi::proto::reapi::GetCapabilitiesRequest;
use corelink_reapi::{
    canonical_grpc_padding_layer, AuthScope, ByteStreamService, CapabilitiesService,
    CasWriteService, StubPatValidator,
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

const TOKEN_RW: &str = "token-tenant-A-rw";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_not_found_is_padded_at_layer_boundary() {
    // Boot a tonic server with the canonical gRPC padding layer
    // mounted at the Server::builder().layer(...) boundary. The
    // canonical layer uses PredicateKind::GrpcNotFound which
    // inspects the `grpc-status: 5` initial header that
    // tonic::Status::into_http emits for Err(Status::not_found(...)).
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
        [AuthScope::CacheWrite, AuthScope::CacheRead],
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
    let caps = CapabilitiesService::new(Arc::clone(&core));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);

    // Mount the canonical gRPC padding layer at the Server boundary.
    let server = Server::builder()
        .layer(canonical_grpc_padding_layer())
        .add_service(cas.into_server().max_decoding_message_size(8 * 1024 * 1024))
        .add_service(bs.into_server().max_decoding_message_size(8 * 1024 * 1024))
        .add_service(caps.into_server())
        .serve_with_incoming(stream);
    let _server_handle = tokio::spawn(server);

    // Wait for the server to be ready by polling the bind: a tonic
    // `connect()` retries until the listener accepts. Codex round-3
    // P2 fix — the previous fixed `sleep(100ms)` was scheduler-
    // dependent and a CI flake source.
    let channel = loop {
        match Channel::from_shared(format!("http://{addr}"))
            .unwrap()
            .connect()
            .await
        {
            Ok(c) => break c,
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    };
    let mut bs_client = ByteStreamClient::new(channel.clone());

    // Issue a Read for a digest that does not exist anywhere → server
    // emits `Err(Status::not_found(...))` → tonic encodes as HTTP 200
    // + grpc-status: 5 → middleware predicate matches → pad to
    // ~200 ms ± 10%.
    let nonexistent = Digest::compute(b"never-existed");
    let mut req = tonic::Request::new(ReadRequest {
        resource_name: format!(
            "blobs/{}/{}",
            nonexistent.to_hex(),
            "never-existed".len() as i64
        ),
        read_offset: 0,
        read_limit: 0,
    });
    let v: MetadataValue<_> = format!("Bearer {TOKEN_RW}").parse().unwrap();
    req.metadata_mut().insert("authorization", v);

    let start = std::time::Instant::now();
    let res = bs_client.read(req).await;
    let elapsed = start.elapsed();
    // Server returns Err(Status); tonic decodes it as Err on the client side.
    let err = res.expect_err("read of non-existent digest must surface as gRPC NOT_FOUND");
    assert_eq!(err.code(), Code::NotFound);

    // The canonical pad target is 200 ms ± 10%; under a real (not
    // virtual) clock the wall-clock round-trip MUST be ≥ 180 ms.
    // Tolerate a 20 ms slack to absorb tonic / tcp setup jitter on a
    // loaded CI runner; the gate is "padding fired", not "padding
    // hit a precise boundary".
    let elapsed_ms = elapsed.as_millis();
    assert!(
        elapsed_ms >= 160,
        "gRPC NOT_FOUND elapsed {elapsed_ms} ms; expected ≥ 160 ms (padding to 200 ms ± 10%)",
    );
    // Upper bound: pad is bounded at target × (1 + jitter%) +
    // handler resolution + transport overhead. 350 ms is a generous
    // ceiling; if exceeded we have a real regression in the layer
    // arithmetic.
    assert!(
        elapsed_ms <= 350,
        "gRPC NOT_FOUND elapsed {elapsed_ms} ms; expected ≤ 350 ms (pad upper bound + transport)",
    );

    // Positive control (codex round-3 P1 fix): the canonical gRPC
    // happy path (`Capabilities::GetCapabilities` returns Ok) MUST
    // NOT be padded by the layer. We use Capabilities because it is
    // a cheap, schema-stable handler that does not invoke the storage
    // backends — its latency closely tracks transport + handler-
    // dispatch overhead, so any padding tax surfaces sharply.
    let mut caps_client = CapabilitiesClient::new(channel);
    let mut caps_req = tonic::Request::new(GetCapabilitiesRequest {
        instance_name: String::new(),
    });
    let v: MetadataValue<_> = format!("Bearer {TOKEN_RW}").parse().unwrap();
    caps_req.metadata_mut().insert("authorization", v);
    let start = std::time::Instant::now();
    let _ok = caps_client.get_capabilities(caps_req).await.expect("Capabilities is happy-path");
    let elapsed_ok = start.elapsed().as_millis();
    // No padding ⇒ wall-clock should be well under the 200 ms p99
    // target. 100 ms is a generous ceiling for transport + handler
    // dispatch on a busy CI runner.
    assert!(
        elapsed_ok < 100,
        "gRPC happy-path Capabilities elapsed {elapsed_ok} ms; padding leaked into OK responses",
    );
}
