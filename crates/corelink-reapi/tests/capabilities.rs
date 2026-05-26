//! gRPC `Capabilities.GetCapabilities` integration test.
//!
//! Spins a tonic server harness backed by an in-memory R2 + an in-memory
//! MetaStore + a stub PAT validator and asserts the wire payload of
//! `GetCapabilities` matches WI-S01-005 §6.1.3 byte-for-byte.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test code: harness orchestration"
)]

use std::sync::Arc;

use corelink_meta::InMemoryMetaStore;
use corelink_reapi::handler::{HandlerCore, SystemClock};
use corelink_reapi::orchestrator::NoopOrphanReconciler;
use corelink_reapi::proto::reapi::capabilities_client::CapabilitiesClient;
use corelink_reapi::proto::reapi::GetCapabilitiesRequest;
use corelink_reapi::{CapabilitiesService, StubPatValidator};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_replication::region_resolver::Region;
use tokio::net::TcpListener;
use tonic::transport::Server;
use zeroize::Zeroizing;

#[tokio::test]
async fn get_capabilities_advertises_canonical_caps() {
    let tdk = Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])));
    let backend = Arc::new(InMemoryR2::new());
    let writer = Arc::new(R2Writer::new(Region::Wnam, Arc::clone(&backend)));
    let reader = Arc::new(R2Reader::new(Region::Wnam, Arc::clone(&backend)));
    let meta = Arc::new(InMemoryMetaStore::new());
    let reconciler = Arc::new(NoopOrphanReconciler);
    let core = Arc::new(HandlerCore::new(
        StubPatValidator::new(),
        tdk,
        writer,
        reader,
        meta,
        reconciler,
        SystemClock,
    ));

    let svc = CapabilitiesService::new(Arc::clone(&core));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server = Server::builder()
        .add_service(svc.into_server())
        .serve_with_incoming(stream);
    let server_handle = tokio::spawn(server);
    // Brief pause so the server is ready to accept the connect below.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let endpoint = format!("http://{addr}");
    let mut client = CapabilitiesClient::connect(endpoint).await.unwrap();
    let resp = client
        .get_capabilities(GetCapabilitiesRequest {
            instance_name: String::new(),
        })
        .await
        .unwrap()
        .into_inner();

    let cache = resp
        .cache_capabilities
        .expect("cache_capabilities populated");
    assert_eq!(
        cache.max_batch_total_size_bytes,
        4 * 1024 * 1024,
        "MaxBatchTotalSizeBytes is 4 MiB canonical (WI-S01-005 §6.1.3)"
    );
    assert_eq!(
        cache.max_cas_blob_size_bytes,
        5 * 1024 * 1024,
        "max_cas_blob_size_bytes is 5 MiB canonical (matches SINGLE_BLOB_LIMIT_BYTES)"
    );
    // BLAKE3 must be advertised first (S-01 canonical hash; tag 9).
    assert_eq!(cache.digest_functions[0], 9);
    // SemVer is REAPI v2.12.0.
    let lo = resp.low_api_version.expect("low_api_version populated");
    assert_eq!((lo.major, lo.minor, lo.patch), (2, 12, 0));
    let hi = resp.high_api_version.expect("high_api_version populated");
    assert_eq!((hi.major, hi.minor, hi.patch), (2, 12, 0));

    server_handle.abort();
}
