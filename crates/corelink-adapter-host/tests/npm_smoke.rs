#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::type_complexity,
    clippy::clone_on_copy,
    reason = "tests are allowed to use these primitives"
)]
//! Smoke (e2e-ish) integration tests.
//!
//! Spins up the adapter router with in-memory fakes for CAS / KV /
//! upstream / tenant resolver / audit emitter and exercises the main
//! HTTP paths.

#[path = "npm_common.rs"]
mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use corelink_adapter_host::npm::{
    config::NpmAdapterConfig,
    server::{build_router, AdapterState},
    upstream::UpstreamClient,
};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use http_body_util::BodyExt;
use tower::ServiceExt;
use url::Url;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{FailingKv, FixedTenant, InMemCas, InMemKv};
use corelink_adapter_host::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES;
use corelink_adapter_host::npm::ports::KvStore;

const TEST_PAT: &str = "corelink_npm_test";

/// Build adapter state whose upstream client points at `upstream_base`
/// (a wiremock mock) so the search proxy can be exercised end-to-end
/// without touching the real registry.
fn make_state_with_upstream(upstream_base: &str) -> AdapterState {
    let cas: Arc<dyn corelink_adapter_host::npm::ports::CasStore> = Arc::new(InMemCas::default());
    let kv: Arc<dyn corelink_adapter_host::npm::ports::KvStore> = Arc::new(InMemKv::default());
    let resolver = FixedTenant::new(TEST_PAT);
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
    let base = Url::parse(upstream_base).expect("upstream url");
    let config = NpmAdapterConfig::new(
        "127.0.0.1:0".parse().expect("bind"),
        base.clone(),
        300,
        256 * 1024 * 1024,
        cas,
        kv,
        resolver,
        auditor,
    );
    AdapterState::new(
        Arc::new(config),
        Arc::new(UpstreamClient::new(base).expect("client")),
    )
}

/// Build adapter state with a caller-supplied KV handle (so a test can inject a
/// failing/outage KV) and an upstream pointed at a wiremock base.
fn make_state_with_upstream_and_kv(upstream_base: &str, kv: Arc<dyn KvStore>) -> AdapterState {
    let cas: Arc<dyn corelink_adapter_host::npm::ports::CasStore> = Arc::new(InMemCas::default());
    let resolver = FixedTenant::new(TEST_PAT);
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
    let base = Url::parse(upstream_base).expect("upstream url");
    let config = NpmAdapterConfig::new(
        "127.0.0.1:0".parse().expect("bind"),
        base.clone(),
        300,
        256 * 1024 * 1024,
        cas,
        kv,
        resolver,
        auditor,
    );
    AdapterState::new(
        Arc::new(config),
        Arc::new(UpstreamClient::new(base).expect("client")),
    )
}

fn make_state() -> AdapterState {
    let cas: Arc<dyn corelink_adapter_host::npm::ports::CasStore> = Arc::new(InMemCas::default());
    let kv: Arc<dyn corelink_adapter_host::npm::ports::KvStore> = Arc::new(InMemKv::default());
    let resolver = FixedTenant::new(TEST_PAT);
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
    let config = NpmAdapterConfig::new(
        "127.0.0.1:0".parse().expect("bind"),
        Url::parse("https://registry.npmjs.org").expect("url"),
        300,
        256 * 1024 * 1024,
        cas,
        kv,
        resolver,
        auditor,
    );
    AdapterState::new(
        Arc::new(config),
        Arc::new(
            UpstreamClient::new(Url::parse("https://registry.npmjs.org").expect("url"))
                .expect("client"),
        ),
    )
}

#[tokio::test]
async fn ping_returns_200() {
    let state = make_state();
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/-/ping")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn search_requires_auth() {
    // Search is now a real, PAT-gated upstream proxy (no longer a 501 stub).
    // An UNauthenticated request is rejected at the auth layer (401) BEFORE any
    // upstream network call — a hermetic proof the endpoint is wired + gated.
    // The proxy/validation/clamp logic itself is covered by the pure-logic unit
    // tests in `npm::search` + the SSRF join tests in `npm::upstream`.
    let state = make_state();
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/-/v1/search?text=lodash")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authenticated_search_proxies_upstream() {
    // End-to-end: an authenticated search reaches the SSRF-guarded upstream
    // proxy, which forwards `text`/`size`/`from` and returns the canonical
    // `{ objects, total, time }` envelope verbatim to the client.
    let upstream = MockServer::start().await;
    let body = serde_json::json!({
        "objects": [{ "package": { "name": "lodash", "version": "4.17.21" } }],
        "total": 1,
        "time": "Wed"
    });
    Mock::given(method("GET"))
        .and(path("/-/v1/search"))
        .and(query_param("text", "lodash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&upstream)
        .await;

    let state = make_state_with_upstream(&upstream.uri());
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/-/v1/search?text=lodash&size=5")
                .header(header::AUTHORIZATION, format!("Bearer {TEST_PAT}"))
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["objects"][0]["package"]["name"], "lodash");
    assert_eq!(json["total"], 1);
}

#[tokio::test]
async fn missing_auth_returns_401() {
    let state = make_state();
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/lodash")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forged_pat_returns_401() {
    let state = make_state();
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/lodash")
                .header(header::AUTHORIZATION, "Bearer corelink_forged")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oversized_metadata_is_served_200_not_503() {
    // PROD REGRESSION LOCK (2026-07-19): a large-metadata package (e.g. `react`
    // ~6.8 MiB) previously 503'd because caching the packument overflowed the KV
    // value limit and that CACHE-WRITE error failed the READ. The mirror must
    // now proxy-through: 200 + the exact upstream body, write skipped.
    let upstream = MockServer::start().await;
    // A valid packument padded ABOVE the metadata cache cap.
    let mut body = br#"{"name":"react","versions":{},"_pad":""#.to_vec();
    body.resize(body.len() + DEFAULT_METADATA_CACHE_MAX_BYTES + 4096, b'a');
    body.extend_from_slice(br#""}"#);
    assert!(body.len() > DEFAULT_METADATA_CACHE_MAX_BYTES);
    Mock::given(method("GET"))
        .and(path("/react"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&upstream)
        .await;

    let kv = Arc::new(InMemKv::default());
    let kv_handle: Arc<dyn KvStore> = kv.clone();
    let state = make_state_with_upstream_and_kv(&upstream.uri(), kv_handle);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/react")
                .header(header::AUTHORIZATION, format!("Bearer {TEST_PAT}"))
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "oversized metadata must be 200, not 503"
    );
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes();
    assert_eq!(
        bytes.as_ref(),
        body.as_slice(),
        "full upstream body must be served"
    );
    // Proactive size-skip: nothing was written to the cache.
    assert!(
        kv.snapshot().is_empty(),
        "oversized packument must NOT be cached"
    );
}

#[tokio::test]
async fn metadata_kv_outage_is_served_200_not_503() {
    // A KV backend OUTAGE on a normal-size packument must also degrade to
    // proxy-through (a cache being down must never break `npm install`).
    let upstream = MockServer::start().await;
    let body = br#"{"name":"is-odd","versions":{}}"#.to_vec();
    Mock::given(method("GET"))
        .and(path("/is-odd"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&upstream)
        .await;

    let kv: Arc<dyn KvStore> = Arc::new(FailingKv);
    let state = make_state_with_upstream_and_kv(&upstream.uri(), kv);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/is-odd")
                .header(header::AUTHORIZATION, format!("Bearer {TEST_PAT}"))
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "KV outage must degrade to 200, not 503"
    );
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes();
    assert_eq!(bytes.as_ref(), body.as_slice());
}

#[tokio::test]
async fn body_of_401_includes_auth_text() {
    let state = make_state();
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/lodash")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes();
    let body_str = std::str::from_utf8(&body).expect("utf8");
    assert!(body_str.to_lowercase().contains("auth"));
}
