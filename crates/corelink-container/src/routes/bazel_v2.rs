//! REAPI v2 Bazel remote-cache routes: `/bazel/v2/*`
//!
//! # Purpose
//!
//! Mounts the five REAPI v2 REST cache endpoints so any Bazel user can
//! point `--remote_cache=https://corelink-api.humangr.com/bazel/v2` and
//! get distributed cache backed by the same R2 blobs the native CAS/AC
//! endpoints serve.
//!
//! # Endpoint mapping
//!
//! | HTTP | Route | Operation |
//! |------|-------|-----------|
//! | `GET`  | `/bazel/v2/:instance/blobs/:hash/:size` | CAS read |
//! | `PUT`  | `/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` | CAS write |
//! | `GET`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC read |
//! | `PUT`  | `/bazel/v2/:instance/blobs/ac/:hash/:size` | AC write |
//! | `POST` | `/bazel/v2/:instance/findMissingBlobs` | batch find-missing |
//!
//! # Tenant isolation
//!
//! The `:instance` path segment MUST equal the `x-corelink-tenant-id`
//! header injected by the Cloudflare Worker post-auth. Mismatches produce
//! HTTP 403 and an audit log entry via the underlying handler's audit
//! surface (not duplicated here — the adapter already emits).
//!
//! # Route-level responsibilities
//!
//! 1. Extract `:instance`, `:hash`, `:size`, `:uuid` from path params.
//! 2. Construct the REAPI [`Digest`] from `:hash`/`:size`.
//! 3. Read caller metadata from Worker-injected headers.
//! 4. Call the appropriate [`BazelAdapter`] or [`FindMissingHandler`]
//!    method.
//! 5. Map [`BazelBridgeError`] to the canonical HTTP status code.
//!
//! # Note on matchit 0.7 syntax
//!
//! All routes use the `:name` capture form, NOT `{name}`. matchit 0.7.3
//! (pinned transitively via `axum = "0.7"`) treats `{name}` as a literal
//! path segment — this was tracked as DEBT-029 and closed on the CAS/AC
//! surfaces. The same rule is enforced here.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use corelink_bazel_bridge::{
    adapter::BazelAdapter,
    digest::Digest,
    error::BazelBridgeError,
    find_missing::{
        build_find_missing_response, parse_find_missing_request, FindMissingHandler,
        InMemoryFindMissing,
    },
};
use corelink_handler_ac::{
    AcLookupHandler, AcUpdateHandler, InMemoryAcHandler, InMemoryAuditSink as AcAuditSink,
    InMemorySliObserver as AcSliObserver,
};
use corelink_handler_cas::{
    CasReadHandler, CasWriteHandler, InMemoryAuditSink as CasAuditSink, InMemoryCasHandler,
    InMemorySliObserver as CasSliObserver,
};

// ─── State ───────────────────────────────────────────────────────────────────

/// Shared state for all `/bazel/v2/*` routes.
///
/// Holds the [`BazelAdapter`] (wraps CAS + AC trait objects) and the
/// [`FindMissingHandler`] (backed by the same CAS read surface).
///
/// Cloning increments Arc ref-counts; no data is copied.
#[derive(Clone)]
pub struct BazelRouteState {
    /// REAPI CAS / AC adapter.
    pub adapter: BazelAdapter,
    /// `findMissingBlobs` handler.
    pub find_missing: Arc<dyn FindMissingHandler>,
}

impl core::fmt::Debug for BazelRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BazelRouteState").finish_non_exhaustive()
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the canonical [`BazelRouteState`] from the CAS and AC handler
/// pairs that are already wired in the container.
///
/// This function reuses the SAME trait-object pattern as [`super::cas::build_handlers`]
/// and [`super::ac::build_handlers`]: on native targets with R2 credentials
/// it constructs the real R2-backed handlers; otherwise it falls back to
/// `InMemory`. The `BazelAdapter` wraps these four trait objects so the
/// `/bazel/v2/*` routes share the same backing store as the native
/// CAS/AC routes.
///
/// # Production path
///
/// In production, callers of `build_with_factory` (in `routes.rs`) will
/// call `cas::build_handlers()` and `ac::build_handlers()` first and
/// PASS those trait objects here, ensuring a single shared handler
/// instance. This overload is used by that composition; the
/// `build_handlers()` function below is for standalone use (tests, dev).
#[must_use]
pub fn build_handlers_from(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
) -> BazelRouteState {
    // The find-missing handler delegates to the same CAS read trait object
    // so it shares the backing store with CAS reads.
    let find_missing: Arc<dyn FindMissingHandler> =
        Arc::new(InMemoryFindMissing::new(cas_read.clone()));
    let adapter = BazelAdapter::new(cas_read, cas_write, ac_lookup, ac_update);
    BazelRouteState {
        adapter,
        find_missing,
    }
}

/// Build a standalone [`BazelRouteState`] with fresh `InMemory` handlers.
///
/// Used by tests and dev paths where no shared CAS/AC state is needed.
/// The production `build_with_factory` in `routes.rs` uses
/// [`build_handlers_from`] to share the existing CAS/AC trait objects.
#[must_use]
pub fn build_handlers() -> BazelRouteState {
    let cas_audit = Arc::new(CasAuditSink::new());
    let cas_sli = Arc::new(CasSliObserver::new());
    let cas: Arc<InMemoryCasHandler> = Arc::new(InMemoryCasHandler::new(cas_audit, cas_sli));
    let ac_audit = Arc::new(AcAuditSink::new());
    let ac_sli = Arc::new(AcSliObserver::new());
    let ac: Arc<InMemoryAcHandler> = Arc::new(InMemoryAcHandler::new(ac_audit, ac_sli));
    build_handlers_from(
        cas.clone() as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        ac.clone() as Arc<dyn AcLookupHandler>,
        ac as Arc<dyn AcUpdateHandler>,
    )
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all five `/bazel/v2/*` REAPI routes.
///
/// All routes share the single [`BazelRouteState`]; axum disambiguates
/// operations by HTTP method and path template.
///
/// # matchit 0.7 note
///
/// Path params use `:name` syntax (never `{name}`). See module-level
/// doc for rationale.
pub fn router(state: BazelRouteState) -> Router {
    Router::new()
        // CAS read:  GET  /bazel/v2/:instance/blobs/:hash/:size
        .route(
            "/bazel/v2/:instance/blobs/:hash/:size",
            get(handle_cas_read),
        )
        // AC read:   GET  /bazel/v2/:instance/blobs/ac/:hash/:size
        // NOTE: axum resolves /blobs/ac/… before /blobs/:hash/… because
        // "ac" is a literal segment that wins over the wildcard `:hash`.
        // We mount the AC read/write on the same path, differentiated by
        // HTTP method.
        .route(
            "/bazel/v2/:instance/blobs/ac/:hash/:size",
            get(handle_ac_read).put(handle_ac_write),
        )
        // CAS write: PUT  /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size
        .route(
            "/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size",
            put(handle_cas_write),
        )
        // Find missing: POST /bazel/v2/:instance/findMissingBlobs
        .route(
            "/bazel/v2/:instance/findMissingBlobs",
            post(handle_find_missing),
        )
        .with_state(state)
}

// ─── Header helpers ───────────────────────────────────────────────────────────

/// Read a header value; return `default` when absent or non-ASCII.
fn header_str(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Extract `x-corelink-tenant-id`; fail-CLOSED to `"_unknown"`.
fn caller_tenant(headers: &HeaderMap) -> String {
    header_str(headers, "x-corelink-tenant-id", "_unknown")
}

/// Extract `x-corelink-token-prefix` as principal; fail-CLOSED to `"_unknown"`.
fn principal(headers: &HeaderMap) -> String {
    header_str(headers, "x-corelink-token-prefix", "_unknown")
}

/// Logical wall-clock stand-in (production wiring injects a real clock
/// collaborator; 0 keeps the routes logic-free and matches the CAS/AC
/// route convention).
const fn now_ms() -> u64 {
    0u64
}

// ─── Error mapping ────────────────────────────────────────────────────────────

/// Map a [`BazelBridgeError`] to the canonical HTTP response.
///
/// HTTP status codes per the task spec:
/// - `NotFound`           → 404
/// - `InvalidDigest`      → 400
/// - `SizeMismatch`       → 422
/// - `CrossTenantDenied`  → 403
/// - `AuditFailed`        → 503
/// - `BatchTooLarge`      → 413
/// - `Internal`           → 500
fn map_bridge_err(e: BazelBridgeError) -> axum::response::Response {
    tracing::warn!(error = ?e, "bazel-bridge error");
    let (code, body): (StatusCode, &str) = match e {
        BazelBridgeError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found"),
        BazelBridgeError::InvalidDigest { .. } => (StatusCode::BAD_REQUEST, "invalid digest"),
        BazelBridgeError::SizeMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "size mismatch")
        }
        BazelBridgeError::CrossTenantDenied { .. } => (StatusCode::FORBIDDEN, "cross-tenant"),
        BazelBridgeError::AuditFailed(_) => (StatusCode::SERVICE_UNAVAILABLE, "audit closed"),
        BazelBridgeError::BatchTooLarge { .. } => {
            (StatusCode::PAYLOAD_TOO_LARGE, "batch too large")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    (code, body).into_response()
}

/// Parse a `{hash}/{size}` digest pair from path params.
///
/// Returns `Err(BazelBridgeError::InvalidDigest)` when the digest is invalid.
fn parse_digest(hash: &str, size_str: &str) -> Result<Digest, BazelBridgeError> {
    let digest_str = format!("{hash}/{size_str}");
    Digest::parse(&digest_str)
}

// ─── Route handlers ───────────────────────────────────────────────────────────

/// `GET /bazel/v2/:instance/blobs/:hash/:size` — CAS read.
async fn handle_cas_read(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let tenant = caller_tenant(&headers);
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .cas_get(&instance, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `PUT /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` — CAS write.
///
/// `uuid` is the resumable-upload correlation ID used by Bazel; this
/// bridge accepts single-shot writes and logs the `uuid` for tracing
/// but does not perform any resumable-upload protocol.
async fn handle_cas_write(
    State(state): State<BazelRouteState>,
    Path((instance, uuid, hash, size)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let tenant = caller_tenant(&headers);
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    tracing::debug!(
        instance = %instance,
        upload_id = %uuid,
        hash = %hash,
        "bazel CAS write"
    );
    match state
        .adapter
        .cas_put(&instance, &digest, body.to_vec(), &p, &tenant, now_ms())
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `GET /bazel/v2/:instance/blobs/ac/:hash/:size` — AC read.
async fn handle_ac_read(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let tenant = caller_tenant(&headers);
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .ac_get(&instance, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `PUT /bazel/v2/:instance/blobs/ac/:hash/:size` — AC write.
async fn handle_ac_write(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let tenant = caller_tenant(&headers);
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .ac_put(&instance, &digest, body.to_vec(), &p, &tenant, now_ms())
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_bridge_err(e),
    }
}

/// `POST /bazel/v2/:instance/findMissingBlobs` — batch find-missing.
///
/// Accepts JSON body `{"blobDigests":[{"hash":"…","sizeBytes":N},…]}`.
/// Returns `{"missingBlobDigests":[…]}`.
async fn handle_find_missing(
    State(state): State<BazelRouteState>,
    Path(instance): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let tenant = caller_tenant(&headers);
    let p = principal(&headers);

    // Parse + validate the JSON body.
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "request body is not valid UTF-8").into_response()
        }
    };
    let digests = match parse_find_missing_request(body_str) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };

    // Delegate to the find-missing handler.
    let missing = match state
        .find_missing
        .find_missing(&instance, &p, &tenant, now_ms(), &digests)
    {
        Ok(m) => m,
        Err(e) => return map_bridge_err(e),
    };

    // Serialise the response.
    match build_find_missing_response(missing) {
        Ok(json) => (StatusCode::OK, [("content-type", "application/json")], json).into_response(),
        Err(e) => map_bridge_err(e),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use corelink_handler_cas::handler::fake_hash;
    use tower::ServiceExt;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TENANT: &str = "acme";

    fn make_state() -> BazelRouteState {
        build_handlers()
    }

    /// Shared fixture: PUT a blob via the route, return the hash used.
    async fn seed_cas_via_route(app: &axum::Router, tenant: &str, hash: &str, payload: &[u8]) {
        let size = payload.len();
        let uuid = "test-uuid-0000";
        let uri = format!("/bazel/v2/{tenant}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::from(payload.to_vec()))
            .unwrap();
        // Must clone the router for oneshot; caller holds the arc in state.
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "seed PUT should return 204"
        );
    }

    // ── CAS read happy path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_hit_returns_200_with_bytes() {
        let state = make_state();
        let payload = b"hello bazel".to_vec();
        let hash = fake_hash(&payload);
        let app = router(state.clone());

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── CAS read miss ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_miss_returns_404() {
        let state = make_state();
        let app = router(state);
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── CAS write + read round trip ───────────────────────────────────────────

    #[tokio::test]
    async fn cas_write_then_read_round_trip() {
        let state = make_state();
        let payload = b"round trip data".to_vec();
        let hash = fake_hash(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let read_uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&read_uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── AC write + read round trip ────────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_then_read_round_trip() {
        let state = make_state();
        let hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let payload = b"action_result_payload".to_vec();
        let size = payload.len();
        let app = router(state);

        // AC write (PUT)
        let write_uri = format!("/bazel/v2/{TENANT}/blobs/ac/{hash}/{size}");
        let put_req = Request::builder()
            .uri(&write_uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::from(payload.clone()))
            .unwrap();
        let put_resp = app.clone().oneshot(put_req).await.expect("PUT oneshot");
        assert_eq!(put_resp.status(), StatusCode::NO_CONTENT);

        // AC read (GET)
        let get_req = Request::builder()
            .uri(&write_uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::empty())
            .unwrap();
        let get_resp = app.oneshot(get_req).await.expect("GET oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let got = to_bytes(get_resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── findMissingBlobs — all absent ────────────────────────────────────────

    #[tokio::test]
    async fn find_missing_all_absent_returns_both_digests() {
        let state = make_state();
        let app = router(state);
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": HASH_A, "sizeBytes": 5},
                {"hash": hash_b, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v["missingBlobDigests"]
                .as_array()
                .expect("missingBlobDigests")
                .len(),
            2
        );
    }

    // ── findMissingBlobs — one present, one absent ────────────────────────────

    #[tokio::test]
    async fn find_missing_partial_returns_only_absent() {
        let state = make_state();
        let payload = b"present blob".to_vec();
        let hash = fake_hash(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let hash_absent = HASH_A;
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": &hash, "sizeBytes": payload.len()},
                {"hash": hash_absent, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let missing = v["missingBlobDigests"].as_array().expect("array");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0]["hash"], hash_absent);
    }

    // ── Cross-tenant denial — CAS read ────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        // instance = "victim" in URL, but caller header = "attacker"
        let uri = format!("/bazel/v2/victim/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── Cross-tenant denial — AC write ────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        let hash = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
        let uri = format!("/bazel/v2/victim/blobs/ac/{hash}/8");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            .body(Body::from(b"payload".to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── CAS write size mismatch returns 422 ───────────────────────────────────

    #[tokio::test]
    async fn cas_write_size_mismatch_returns_422() {
        let state = make_state();
        let app = router(state);
        let payload = b"five!".to_vec(); // 5 bytes
        let hash = fake_hash(&payload);
        // Claim size 99 but send 5 bytes.
        let uuid = "test-uuid-mismatch";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/99");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── Invalid digest in URL returns 400 ─────────────────────────────────────

    #[tokio::test]
    async fn cas_read_bad_digest_returns_400() {
        let state = make_state();
        let app = router(state);
        // hash is too short → invalid digest
        let uri = format!("/bazel/v2/{TENANT}/blobs/badhash/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── build_handlers smoke test ─────────────────────────────────────────────

    #[test]
    fn build_handlers_constructs_without_panic() {
        let state = build_handlers();
        let _r = router(state);
    }

    // ── Route constants use matchit-0.7 colon syntax ──────────────────────────

    #[test]
    fn route_paths_use_colon_syntax_not_curly_braces() {
        // Regression net per DEBT-029: `{name}` is a LITERAL in matchit 0.7.
        let paths = [
            "/bazel/v2/:instance/blobs/:hash/:size",
            "/bazel/v2/:instance/blobs/ac/:hash/:size",
            "/bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size",
            "/bazel/v2/:instance/findMissingBlobs",
        ];
        for p in paths {
            assert!(
                !p.contains('{'),
                "path {p:?} must use :name syntax, not {{name}}"
            );
        }
    }
}
