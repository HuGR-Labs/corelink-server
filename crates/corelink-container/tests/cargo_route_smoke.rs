//! Integration smoke tests for the container's `/cargo/<tenant>/<key>`
//! sccache surface (FINDING Gap 1 mount).
//!
//! These drive the FULL container cargo router — `nest_service("/cargo",
//! …)` + the scope/path-rewrite gate + the adapter's `/:key` handlers —
//! via `tower::ServiceExt::oneshot`, with a hermetic stub `TenantResolver`
//! (so no D1) and an in-memory CAS handler (so no R2). They pin:
//!
//! * the 3-segment path `/cargo/<tenant>/<key>` REACHES the adapter handler
//!   (a naive `nest` would 404 it before the gate);
//! * the per-operation scope gate (read-only PAT ⇒ 403 on PUT) from the
//!   server-trusted `x-corelink-scope` header;
//! * PAT auth failure (missing / unknown) ⇒ 401;
//! * tenant comes from the PAT resolver, never the path segment;
//! * a PUT→GET round-trip serves the stored bytes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use corelink_adapter_host::cargo::ports::{
    ResolvedTenant, TenantResolveError, TenantResolver,
};
use corelink_handler_cas::{
    handler::fake_hash, CasReadHandler, CasWriteHandler, InMemoryAuditSink, InMemoryCasHandler,
    InMemorySliObserver,
};
use corelink_server::routes::cargo;
use tower::ServiceExt;

const PAT: &str = "corelink_pat_known-good-token";
const TENANT: &str = "tenant-from-pat";
const SCOPE_RW: &str = "cas:rw";
const SCOPE_RO: &str = "cas:r";

/// Hermetic PAT→tenant resolver: one known PAT → `TENANT`, everything else
/// `InvalidPat`. Stands in for the D1-backed resolver in tests.
#[derive(Debug)]
struct StubResolver;

#[async_trait::async_trait]
impl TenantResolver for StubResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        if pat_plaintext == PAT {
            Ok(TENANT.to_owned())
        } else {
            Err(TenantResolveError::InvalidPat)
        }
    }

    /// The known-good PAT carries cache WRITE (so the PUT/GET round-trip
    /// succeeds through the gate's two-layer write check); everything else is
    /// `InvalidPat`. The read-only-scope PUT 403 comes from the
    /// `x-corelink-scope` HEADER gate, BEFORE this `can_write` check runs.
    async fn resolve_with_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<ResolvedTenant, TenantResolveError> {
        if pat_plaintext == PAT {
            Ok(ResolvedTenant {
                tenant_id: TENANT.to_owned(),
                can_write: true,
            })
        } else {
            Err(TenantResolveError::InvalidPat)
        }
    }
}

fn cargo_router() -> axum::Router {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared;
    // No $-ceiling gate in this smoke test (the route shape is identical with
    // or without it; the gate is exercised by the tenant_quota unit tests).
    cargo::router(read, write, Arc::new(StubResolver), None)
    // (no separate verifier arg: the gate's two-layer write check now uses the
    // resolver's `resolve_with_capability` — a single PAT verification.)
}

/// A valid sccache key: the in-memory handler's `fake_hash` of the bytes,
/// which is a 64-char lowercase hex string (also satisfies the adapter's
/// key validation).
fn key_for(bytes: &[u8]) -> String {
    fake_hash(bytes)
}

#[tokio::test]
async fn put_then_get_round_trip_through_cargo_router() {
    let app = cargo_router();
    let bytes = b"corelink-cargo-sccache-artifact".to_vec();
    let key = key_for(&bytes);

    // PUT /cargo/<tenant>/<key> with a write scope.
    let put = Request::builder()
        .method("PUT")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::from(bytes.clone()))
        .unwrap();
    let put_resp = app.clone().oneshot(put).await.unwrap();
    assert_eq!(put_resp.status(), StatusCode::OK, "PUT must succeed");

    // GET the same key → 200 + exact bytes.
    let get = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::empty())
        .unwrap();
    let get_resp = app.oneshot(get).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK, "GET after PUT must hit");
    let got = to_bytes(get_resp.into_body(), 1 << 20).await.unwrap();
    assert_eq!(got.as_ref(), bytes.as_slice(), "bytes must round-trip");
}

#[tokio::test]
async fn get_miss_reaches_handler_returns_404_not_router_miss() {
    // A well-formed 3-segment cargo path with a valid PAT + scope but an
    // unstored key MUST reach the handler and 404 — proving the mount wiring
    // (a naive `nest` would 404 at the router before the handler).
    let app = cargo_router();
    let key = key_for(b"never-stored");
    let req = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "miss → handler 404");
}

#[tokio::test]
async fn read_only_scope_is_forbidden_on_put() {
    // x-corelink-scope = cas:r → the per-op gate denies PUT with 403 BEFORE
    // the adapter runs (no write reaches CAS).
    let app = cargo_router();
    let bytes = b"should-not-be-written".to_vec();
    let key = key_for(&bytes);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RO)
        .body(Body::from(bytes))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "read-only PAT must not PUT"
    );
}

#[tokio::test]
async fn read_only_scope_allows_get() {
    // The same cas:r scope MUST still allow reads (a stored key GETs 200).
    let app = cargo_router();
    let bytes = b"readable-artifact".to_vec();
    let key = key_for(&bytes);

    // Seed with a rw PUT first.
    let put = Request::builder()
        .method("PUT")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::from(bytes.clone()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(put).await.unwrap().status(),
        StatusCode::OK
    );

    // Read it back with a read-only scope.
    let get = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RO)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(get).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "cas:r must allow GET");
}

#[tokio::test]
async fn unmapped_methods_are_403_even_with_rw_scope() {
    // Fail-CLOSED method gate: DELETE/PATCH carry a FULL cas:rw scope but are
    // not a mapped cache operation (GET/HEAD/PUT), so the gate must deny them
    // (403) rather than fall through to downstream routing.
    for method in ["DELETE", "PATCH"] {
        let app = cargo_router();
        let key = key_for(b"unmapped-method");
        let req = Request::builder()
            .method(method)
            .uri(format!("/cargo/{TENANT}/{key}"))
            .header("Authorization", format!("Bearer {PAT}"))
            .header("x-corelink-scope", SCOPE_RW)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "{method} with cas:rw must fail closed at the gate"
        );
    }
}

#[tokio::test]
async fn missing_pat_returns_401() {
    let app = cargo_router();
    let key = key_for(b"x");
    let req = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("x-corelink-scope", SCOPE_RW)
        // no Authorization header
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_pat_returns_401() {
    let app = cargo_router();
    let key = key_for(b"y");
    let req = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/{key}"))
        .header("Authorization", "Bearer corelink_pat_unknown-token")
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tenant_comes_from_pat_not_path() {
    // PUT under one path-tenant, GET under a DIFFERENT path-tenant. Because
    // the resolver maps the PAT → TENANT regardless of the path segment,
    // both operations hit the SAME tenant namespace, so the GET reads back
    // the bytes the PUT stored — proving the path tenant is not trusted.
    let app = cargo_router();
    let bytes = b"path-tenant-ignored".to_vec();
    let key = key_for(&bytes);

    let put = Request::builder()
        .method("PUT")
        .uri(format!("/cargo/path-tenant-A/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::from(bytes.clone()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(put).await.unwrap().status(),
        StatusCode::OK
    );

    let get = Request::builder()
        .method("GET")
        .uri(format!("/cargo/path-tenant-B-different/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(get).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "tenant is PAT-derived: a different path tenant still reads the same namespace"
    );
    let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    assert_eq!(got.as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn malformed_short_key_is_rejected() {
    // A key that is not 64-hex must NOT 200 — the adapter's key validation
    // rejects it (400) once the gate forwards it.
    let app = cargo_router();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/cargo/{TENANT}/short-not-hex"))
        .header("Authorization", format!("Bearer {PAT}"))
        .header("x-corelink-scope", SCOPE_RW)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
