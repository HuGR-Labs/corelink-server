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

use common::{FixedTenant, InMemCas, InMemKv};

const TEST_PAT: &str = "hugr-pat_npm_test";

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
async fn search_returns_501() {
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
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
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
                .header(header::AUTHORIZATION, "Bearer hugr-pat_forged")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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
    let body = resp.into_body().collect().await.expect("collect").to_bytes();
    let body_str = std::str::from_utf8(&body).expect("utf8");
    assert!(body_str.to_lowercase().contains("auth"));
}
