//! Smoke end-to-end: spin the adapter, point it at a wiremock-mocked
//! upstream OCI registry, simulate `brew install` with curl-style
//! HTTPS GETs. Per `specs/_proposals/adapters/brew.md` §8 row 1 — the
//! second request MUST hit cache (zero upstream HTTP traffic on the
//! re-fetch).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items
)]

mod common;

use std::sync::Arc;

use common::{
    default_bottle_limit, spin_adapter, InMemoryCas, StaticTenantResolver,
};
use corelink_adapter_brew::audit::EVENT_TYPE_CACHE_FILL;
use corelink_audit::ports::InMemoryAuditEmitter;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOTTLE_BYTES: &[u8] = b"\x1f\x8b\x08\x00fake-tar-gz-payload";
const BOTTLE_PATH: &str = "/v2/homebrew/core/curl-8.5.0.bottle.tar.gz";
const PAT: &str = "hugr-pat_tenant_smoke";
const TENANT: &str = "tenant-smoke";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn second_request_is_a_cache_hit() {
    let upstream = MockServer::start().await;

    // Expect EXACTLY 1 upstream hit across BOTH client requests — the
    // second request must be served from CAS.
    Mock::given(method("GET"))
        .and(path(BOTTLE_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(BOTTLE_BYTES.to_vec())
                .insert_header("content-type", "application/octet-stream"),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT, TENANT));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let client = reqwest::Client::new();

    // First request: cache miss, upstream hit, CAS fill, 1 audit row.
    let r1 = client
        .get(format!("http://{addr}{BOTTLE_PATH}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), 200);
    assert_eq!(r1.bytes().await.unwrap().as_ref(), BOTTLE_BYTES);

    // Second request: cache hit, ZERO additional upstream traffic.
    let r2 = client
        .get(format!("http://{addr}{BOTTLE_PATH}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), 200);
    assert_eq!(r2.bytes().await.unwrap().as_ref(), BOTTLE_BYTES);

    // wiremock's `expect(1)` verifies in Drop — explicit assert here
    // surfaces the failure as a clean panic on the test thread.
    let received = upstream.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        1,
        "expected ONE upstream hit total; saw {} ({:?})",
        received.len(),
        received.iter().map(|r| r.url.path().to_owned()).collect::<Vec<_>>()
    );

    // Audit: exactly one cache-fill row.
    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EVENT_TYPE_CACHE_FILL);
    assert_eq!(events[0].tenant_id, TENANT);
    assert_eq!(events[0].payload["adapter"], "brew");
    assert_eq!(events[0].payload["integrity"], "best-effort");
    assert_eq!(events[0].payload["bottle_size_bytes"], BOTTLE_BYTES.len());

    // CAS: exactly one entry, namespaced to TENANT.
    assert_eq!(cas.len(), 1);
    let key = cas.keys().pop().unwrap();
    assert_eq!(key.0, TENANT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn url_variants_collapse_to_single_cache_entry() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(BOTTLE_BYTES.to_vec()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT, TENANT));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit,
        default_bottle_limit(),
    )
    .await;

    let client = reqwest::Client::new();

    // Three "different" URLs that canonicalize to the same CAS key.
    let variants = [
        "/v2/homebrew/core/curl",
        "/v2/homebrew/core/curl/",
        "/V2/HOMEBREW/CORE/CURL?cdn=us",
    ];

    for v in variants {
        let resp = client
            .get(format!("http://{addr}{v}"))
            .header("Authorization", format!("Bearer {PAT}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "variant {v}");
    }

    assert_eq!(
        cas.len(),
        1,
        "URL variants MUST collapse to one CAS entry"
    );
}
