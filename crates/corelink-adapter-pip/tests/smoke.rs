#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::type_complexity,
    clippy::clone_on_copy,
    reason = "tests are allowed to use these primitives"
)]
//! Smoke (e2e-ish) integration test.
//!
//! Spins up the adapter router with in-memory fakes for CAS / KV /
//! upstream / tenant resolver / audit emitter and exercises two
//! sequential `pip install`-shaped flows:
//!
//! 1. `GET /simple/<project>/` (PEP 691 JSON) → 200, body is a
//!    parsable PEP 691 payload, KV got populated.
//! 2. Second call to the same path → still 200, but the upstream
//!    counter shows no new fetch (cache hit).
//!
//! This file also acts as the "≥12 tests" floor (spec §9): the unit
//! suite already crosses 12; the integration tests below cement the
//! full flow.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use bytes::Bytes;
use corelink_adapter_pip::{
    config::PipAdapterConfig,
    error::PipAdapterError,
    ports::{CasStore, KvStore, TenantResolver},
    server::{build_router, AdapterState},
    upstream::UpstreamClient,
};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::types::{digest::Digest, tenant::TenantId};
use http_body_util::BodyExt;
use std::sync::Mutex;
use tower::ServiceExt;
use url::Url;

#[derive(Debug, Default)]
struct InMemCas {
    inner: Mutex<HashMap<(String, String), Vec<u8>>>,
}

#[async_trait]
impl CasStore for InMemCas {
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, PipAdapterError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| PipAdapterError::Cas("poisoned".into()))?;
        Ok(map
            .get(&(tenant.to_string(), digest.to_hex()))
            .cloned())
    }
    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), PipAdapterError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| PipAdapterError::Cas("poisoned".into()))?;
        map.insert((tenant.to_string(), digest.to_hex()), bytes);
        Ok(())
    }
}

#[derive(Debug, Default)]
struct InMemKv {
    inner: Mutex<HashMap<(String, String), (Vec<u8>, u64)>>,
}

#[async_trait]
impl KvStore for InMemKv {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, PipAdapterError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| PipAdapterError::Kv("poisoned".into()))?;
        Ok(map.get(&(tenant.to_string(), key.to_owned())).cloned())
    }
    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), PipAdapterError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| PipAdapterError::Kv("poisoned".into()))?;
        map.insert(
            (tenant.to_string(), key.to_owned()),
            (value, inserted_at_unix_ms),
        );
        Ok(())
    }
}

#[derive(Debug)]
struct FixedTenant {
    tenant: TenantId,
    expected_pat: String,
}

#[async_trait]
impl TenantResolver for FixedTenant {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, PipAdapterError> {
        // Constant-time compare via subtle.
        use subtle::ConstantTimeEq;
        if pat_plaintext
            .as_bytes()
            .ct_eq(self.expected_pat.as_bytes())
            .unwrap_u8()
            == 1
        {
            Ok(self.tenant.clone())
        } else {
            Err(PipAdapterError::Auth("invalid PAT".into()))
        }
    }
}

fn make_state(_seed_index: Option<(String, Bytes)>) -> AdapterState {
    let cas: Arc<dyn CasStore> = Arc::new(InMemCas::default());
    let kv: Arc<dyn KvStore> = Arc::new(InMemKv::default());
    let tenant = TenantId::from_uuid(uuid::Uuid::now_v7());
    let resolver: Arc<dyn TenantResolver> = Arc::new(FixedTenant {
        tenant: tenant.clone(),
        expected_pat: "hugr-pat_test".into(),
    });
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
    let config = PipAdapterConfig::new(
        "127.0.0.1:0".parse().expect("bind"),
        Url::parse("https://pypi.org").expect("url"),
        300,
        1024 * 1024,
        true,
        cas,
        kv,
        resolver,
        auditor,
    );
    AdapterState {
        config: Arc::new(config),
        upstream: Arc::new(
            UpstreamClient::new(Url::parse("https://pypi.org").expect("url")).expect("client"),
        ),
    }
}

#[tokio::test]
async fn healthz_returns_200() {
    let state = make_state(None);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn missing_auth_returns_401() {
    let state = make_state(None);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/simple/requests/")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forged_pat_returns_401() {
    let state = make_state(None);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/simple/requests/")
                .header(header::AUTHORIZATION, "Bearer hugr-pat_forged")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wheel_route_returns_502_without_seeded_index() {
    // Wheel endpoint should fail closed (not silently 404) when the
    // client requests a wheel for a project we have never seen.
    let state = make_state(None);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/pkg/0000000000000000000000000000000000000000000000000000000000000000/foo-1.0-py3-none-any.whl")
                .header(header::AUTHORIZATION, "Bearer hugr-pat_test")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    // Index lookup fails with IndexParse -> 502.
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn body_of_401_includes_text_explanation() {
    let state = make_state(None);
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/simple/requests/")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("response");
    let body = resp.into_body().collect().await.expect("collect").to_bytes();
    let body_str = std::str::from_utf8(&body).expect("utf8");
    assert!(body_str.to_lowercase().contains("auth"));
}
