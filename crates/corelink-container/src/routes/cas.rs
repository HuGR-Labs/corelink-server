//! `GET /v1/cas/:tenant/:hash` — CAS read route wired against
//! [`corelink_handler_cas::CasReadHandler`].
//!
//! This is the **example end-to-end wire-up** demonstrating how the
//! R-prep handler-crate skeleton plugs into `apps/server`. The
//! production CF-Worker R2-bound handler would replace
//! `InMemoryCasHandler` under the `#[cfg(target_arch = "wasm32")]`
//! branch in the trait-object construction site (see crate-level
//! `corelink_handler_cas` doc).
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn CasReadHandler>` so the route
//! table stays binary-shape stable as we swap the in-memory fake for
//! the wasm32 CF-Worker impl. The same shape is used for the
//! Stripe webhook route's
//! [`corelink_stripe_real::webhook_dispatch::StateMaterializer`]
//! collaborator (wave 16 unification —
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`
//! §unification).
//!
//! # SLO emit
//!
//! Every entry into this route emits one `Sli::AvailCasGet` + one
//! `Sli::LatencyCasGetP99` observation through the
//! `SliObserver` collaborator on the handler. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-CAS-GET`
//! from "Sli-bound deferred" to "Sli emitted at handler layer".

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
    InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
};

/// Canonical CAS read route path (matchit-0.7 / axum-0.7 `:name` captures).
///
/// MUST use the `:name` form — matchit 0.7.3 (the version transitively
/// pinned via `axum = "0.7"`) parses `{name}` as **literal path bytes**
/// rather than a capture, which would silently route every real request
/// to a router-level 404. This was DEBT-029 (closed wave-30 stream-1)
/// on `ac.rs` + `admin.rs`; DEBT-029-cas closes the same surface here.
pub const CAS_READ_ROUTE: &str = "/v1/cas/:tenant/:hash";

/// Canonical CAS write route path. The write path reuses the same
/// template; axum disambiguates by HTTP method (GET vs PUT).
pub const CAS_WRITE_ROUTE: &str = "/v1/cas/:tenant/:hash";

/// Shared route state — distinct trait objects for read and write.
#[derive(Clone)]
pub struct CasRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see [`build_handlers`].
    pub read: Arc<dyn CasReadHandler>,
    /// Write handler (separate trait object — see crate-level docs).
    pub write: Arc<dyn CasWriteHandler>,
    /// Optional 410-Gone tombstone store (hugit-P2 seam B, WP-B). When present,
    /// `GET` consults it FIRST: an erased `(tenant, hash)` short-circuits to
    /// HTTP 410 Gone (never 404, never 200) before the R2 lookup. `None` (the
    /// default — in-memory / test builds, and prod until PR #254's R2 erase
    /// adapter lands) ⇒ no tombstone gate, classic 200/404 behaviour. Backed by
    /// [`crate::routes::cas_erase::D1TombstoneStore`] in prod.
    pub tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
}

impl core::fmt::Debug for CasRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasRouteState").finish_non_exhaustive()
    }
}

/// Build the canonical `Arc<dyn CasReadHandler>` for the current
/// build target and runtime environment.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets the function probes for storage credentials at
/// runtime:
///
/// - When `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`,
///   `R2_S3_ENDPOINT`, `CLOUDFLARE_ACCOUNT_ID`, `CF_API_TOKEN`, and
///   `D1_DATABASE_ID` are all present in the environment,
///   [`R2CasHandler`](crate::storage::r2_s3::R2CasHandler) is
///   constructed against `corelink-cas-prod` (override via
///   `R2_CAS_BUCKET`). This is the **production path**.
///
/// - When credentials are absent (unit tests, local dev, CI) the
///   function falls back to `InMemoryCasHandler`. No network I/O
///   occurs.
///
/// On `wasm32-unknown-unknown` (Cloudflare Worker target) a
/// compile-error placeholder is emitted per the
/// `trait-abstraction-defer` rule; the wasm32 binding is out of
/// scope for WP-S1.
///
/// # Panics
///
/// Does not panic. If the R2 client cannot be constructed
/// (malformed endpoint URL, etc.) an error is logged and the
/// function falls back to `InMemoryCasHandler`.
#[must_use]
pub fn build_handlers() -> (Arc<dyn CasReadHandler>, Arc<dyn CasWriteHandler>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_s3, StorageEnv};

        // Probe for storage credentials.
        if StorageEnv::from_env().is_some() {
            // Credentials are present — try to build the real handler.
            // Empty-or-absent → default (see storage::env_or; mirrors the
            // AC fix for the 2026-06-05 prod incident — latent here).
            let bucket = crate::storage::env_or("R2_CAS_BUCKET", "corelink-cas-prod");
            let region = crate::storage::env_or("R2_CAS_REGION", "iad");

            // Construction is now sync-only (commit ead0f37a removed the
            // aws_config::defaults() IMDS probe). We keep block_in_place +
            // block_on around the async signature in case future R2 init
            // grows network work; the cost is zero when the inner future
            // resolves immediately.
            let handle = tokio::runtime::Handle::current();
            let built = tokio::task::block_in_place(|| {
                handle.block_on(r2_s3::build_r2_cas_handler_from_env(&bucket, &region))
            });
            match built {
                Some(Ok(handler)) => {
                    tracing::info!(
                        bucket = %bucket,
                        region = %region,
                        "CAS handler: R2S3 (real storage)"
                    );
                    // R2CasHandler implements both CasReadHandler and
                    // CasWriteHandler against the same R2 bucket; share
                    // one Arc behind both trait objects.
                    let shared: Arc<r2_s3::R2CasHandler> = Arc::new(handler);
                    let read: Arc<dyn CasReadHandler> = shared.clone();
                    let write: Arc<dyn CasWriteHandler> = shared;
                    return (read, write);
                }
                Some(Err(e)) => {
                    tracing::error!(
                        error = %e,
                        "CAS handler: R2S3 build failed, falling back to InMemory"
                    );
                }
                None => {
                    // Should not happen: we already checked is_some().
                }
            }
        }

        // Fallback: InMemory (no creds or build failure).
        tracing::info!("CAS handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryCasHandler> = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared;
        (read, write)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Placeholder for `corelink_handler_cas::cf_worker::CfWorkerCasHandler`
        // (deferred per trait-abstraction-defer; see crate-level docs).
        // Until that impl lands, the wasm32 build does not include
        // this route — the binary entry asserts the cfg at link time.
        compile_error!(
            "wasm32 CF-Worker CAS handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the CAS read + write routes.
///
/// Both routes share the `/v1/cas/:tenant/:hash` template; axum
/// disambiguates by HTTP method (GET vs PUT).
pub fn router(state: CasRouteState) -> Router {
    Router::new()
        .route(CAS_READ_ROUTE, get(handle_read).put(handle_write))
        .with_state(state)
}

/// `GET /v1/cas/:tenant/:hash` handler.
///
/// The authenticated tenant (DO-injected `x-corelink-tenant-id`,
/// PAT-resolved by the Worker) is bound by the
/// [`crate::auth_tenant::AuthTenant`] extractor and is the SOLE
/// isolation key. The path `:tenant` is a client-controllable echo
/// that MUST match it (mismatch ⇒ 403 before any storage access).
async fn handle_read(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
) -> impl IntoResponse {
    // The path `:tenant` is a client echo that MUST equal the
    // authenticated tenant; mismatch is a cross-tenant attempt and is
    // denied 403 BEFORE any storage access (no tenant quoted in the
    // body). Mirrors bazel_v2 / ac.rs.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-READ capability
    // (`cas:rw` or `cas:r`) in the Worker-trusted `x-corelink-scope` header.
    // BEFORE any storage access. Today every prod PAT is `cas:rw` so this is
    // a NO-OP for current traffic; it establishes the gate for tiered tokens.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // 410-Gone tombstone gate (hugit-P2 seam B, WP-B). When a tombstone store is
    // wired, an erased `(tenant, hash)` short-circuits to HTTP 410 Gone — BEFORE
    // the R2 GET, so it is a single keyed D1 lookup off the hot path. An erased
    // artifact MUST return 410 (never 404 "never existed", never 200 resurrect).
    // A lookup-transport error fails OPEN to the normal read path: a transient
    // D1 blip must not 410 a live blob (the erase write-side is the source of
    // truth; the read gate is advisory). `None` ⇒ classic 200/404.
    if let Some(tombstones) = state.tombstones.as_ref() {
        match tombstones.is_tombstoned(&auth.0, &hash).await {
            Ok(true) => return (StatusCode::GONE, "erased").into_response(),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "cas: tombstone gate lookup failed; serving normally");
            }
        }
    }
    // Logical clock stand-in: production wiring threads a
    // `WallClock` collaborator. We use the handler-supplied
    // `at_unix_ms` to keep the route logic-free.
    let now_ms = 0u64;
    let req = CasReadRequest::new(
        auth.0.clone(),
        hash,
        // Principal is filled in by the auth middleware in
        // production; demo wire-up mirrors ac.rs.
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.read.read(req) {
        Ok(resp) => (StatusCode::OK, resp.bytes).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v1/cas/:tenant/:hash` handler.
///
/// Accepts raw bytes in the body; the client claims the content hash
/// via the URL path. The handler enforces hash equality before
/// committing to durable storage. On success: 201 Created (fresh
/// insert) or 200 OK (idempotent re-write).
async fn handle_write(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // See `handle_read`: the authenticated tenant is the sole
    // isolation key; the path `:tenant` is a client echo that must
    // match. Deny 403 BEFORE any storage access on mismatch.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-WRITE capability
    // (`cas:rw`) BEFORE any storage access. A read-only (`cas:r`) token is
    // rejected here. NO-OP for current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    let now_ms = 0u64;
    let req = CasWriteRequest::new(
        auth.0.clone(),
        hash,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.write.write(req) {
        Ok(resp) => {
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.content_hash).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Map a [`CasHandlerError`] to the canonical HTTP response.
fn map_err(e: CasHandlerError) -> axum::response::Response {
    // Log the full error before mapping so operators retain a
    // diagnostic record (R2/S3 detail) without exposing internals to
    // the caller.
    tracing::warn!(error = ?e, "CAS handler error");
    match e {
        CasHandlerError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        CasHandlerError::HashMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "content hash mismatch").into_response()
        }
        CasHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CasHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit writes without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

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
    use corelink_handler_cas::handler::fake_hash;

    fn fixture() -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared;
        CasRouteState {
            read,
            write,
            tombstones: None,
        }
    }

    /// Fixture with a tombstone store pre-seeded with `(tenant, hash)` so the
    /// read gate returns 410 Gone (WP-B).
    fn fixture_with_tombstone(tenant: &str, hash: &str) -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared;
        let store = Arc::new(crate::routes::cas_erase::InMemoryTombstoneStore::new());
        store.seed(tenant, hash);
        let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> = store;
        CasRouteState {
            read,
            write,
            tombstones: Some(tombstones),
        }
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(CAS_READ_ROUTE, "/v1/cas/:tenant/:hash");
        assert_eq!(CAS_WRITE_ROUTE, "/v1/cas/:tenant/:hash");
    }

    #[test]
    fn route_constant_uses_matchit_0_7_colon_syntax_not_curly_braces() {
        // DEBT-029-cas regression net: matchit 0.7.3 parses `{name}`
        // as LITERAL path bytes, not a capture. Any reintroduction of
        // the `{name}` form would silently route every real request
        // to a router-level 404. Pin both the absence of `{` and the
        // presence of `:tenant` + `:hash` so renames don't drift.
        assert!(
            !CAS_READ_ROUTE.contains('{'),
            "CAS_READ_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
        );
        assert!(CAS_READ_ROUTE.contains(":tenant"));
        assert!(CAS_READ_ROUTE.contains(":hash"));
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        // Smoke test the build_handlers shape on the native target.
        let (read, _write) = build_handlers();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        // We don't have a seed entry point on the trait alone, so
        // we drive the read against a known-empty handler and assert
        // it returns NotFound. The full wire-up is exercised via
        // the handler-crate's own unit tests.
        let res = read.read(CasReadRequest::new("t1", hash, "anon", "t1", 0));
        assert!(matches!(res, Err(CasHandlerError::NotFound { .. })));
        // Build state to satisfy the router constructor.
        let _router = router(fixture());
    }

    /// PUT then GET round-trip via the route state drives both trait
    /// objects against the shared InMemory backing store — pins that
    /// the new write trait object writes to the SAME backing store the
    /// read trait object reads from (regression net for a future
    /// refactor that accidentally splits the backing store).
    #[test]
    fn put_then_get_round_trip_through_route_state() {
        let st = fixture();
        let bytes = b"corelink-cas-put-then-get".to_vec();
        let hash = fake_hash(&bytes);

        // PUT
        let upd = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        let upd_resp = st.write.write(upd).expect("write");
        assert!(upd_resp.durable, "fresh insert must be durable=true");

        // GET — must return the same bytes
        let rd = CasReadRequest::new("t1", hash.clone(), "anon@t1", "t1", 2);
        let rd_resp = st.read.read(rd).expect("read hit");
        assert_eq!(rd_resp.bytes, bytes);
        assert_eq!(rd_resp.content_hash, hash);
    }

    /// Idempotent retry on the write path: a second PUT with the same
    /// bytes returns durable=false (mirrors AC + handler-crate
    /// semantics).
    #[test]
    fn idempotent_write_retry_returns_durable_false() {
        let st = fixture();
        let bytes = b"r".to_vec();
        let hash = fake_hash(&bytes);
        let req1 = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        assert!(st.write.write(req1).expect("first").durable);
        let req2 = CasWriteRequest::new("t1", hash, bytes, "anon@t1", "t1", 2);
        assert!(!st.write.write(req2).expect("retry").durable);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Authenticated tenant injected via `x-corelink-tenant-id` so the
    /// `AuthTenant` extractor admits the request; the scope header then
    /// gates read vs write.
    const TEST_TENANT: &str = "t1";

    /// A `cas:rw` PUT writes successfully (current prod scope — happy path).
    #[tokio::test]
    async fn put_with_rw_scope_succeeds() {
        let app = router(fixture());
        let bytes = b"cas-scope-rw".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// A `cas:r` (read-only) PUT is rejected 403 "insufficient scope"
    /// BEFORE storage.
    #[tokio::test]
    async fn put_with_read_only_scope_returns_403_insufficient_scope() {
        let app = router(fixture());
        let bytes = b"cas-scope-ro".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    /// A `cas:r` GET is allowed (read-only token reads). The artifact is
    /// absent so the route returns 404 — proving the scope gate PASSED
    /// (a denied scope would 403 before storage).
    #[tokio::test]
    async fn get_with_read_only_scope_passes_gate_then_404() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/deadbeef"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Fail-CLOSED: a GET with NO scope header is rejected 403.
    #[tokio::test]
    async fn get_with_missing_scope_returns_403() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/deadbeef"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── 410-Gone tombstone read gate (hugit-P2 seam B, WP-B) ─────────────────

    /// A GET for an ERASED `(tenant, hash)` returns HTTP 410 Gone — NOT 404,
    /// NOT 200. The tombstone gate runs after the scope gate and before the R2
    /// read.
    #[tokio::test]
    async fn get_erased_hash_returns_410_gone() {
        const ERASED: &str = "erasedhash01";
        let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/{ERASED}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::GONE);
    }

    /// With a tombstone store wired, a NON-erased hash still 404s (the gate is
    /// per-hash, not a blanket block).
    #[tokio::test]
    async fn get_non_erased_hash_with_store_still_404() {
        let app = router(fixture_with_tombstone(TEST_TENANT, "some-other-hash"));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/livehash99"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
