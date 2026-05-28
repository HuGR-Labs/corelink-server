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
            let bucket = std::env::var("R2_CAS_BUCKET")
                .unwrap_or_else(|_| "corelink-cas-prod".to_owned());
            let region = std::env::var("R2_CAS_REGION")
                .unwrap_or_else(|_| "iad".to_owned());

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
        let shared: Arc<InMemoryCasHandler> =
            Arc::new(InMemoryCasHandler::new(audit, sli));
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
/// For demonstration purposes the route reads the tenant as both
/// the path tenant and the caller's authenticated tenant. Production
/// wiring threads the authenticated tenant from a tower middleware
/// (which validates the bearer / mTLS / PAT upstream).
async fn handle_read(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
) -> impl IntoResponse {
    // Logical clock stand-in: production wiring threads a
    // `WallClock` collaborator. We use the handler-supplied
    // `at_unix_ms` to keep the route logic-free.
    let now_ms = 0u64;
    let req = CasReadRequest::new(
        tenant.clone(),
        hash,
        // Principal is filled in by the auth middleware in
        // production; demo wire-up uses the tenant as the principal.
        format!("anon@{tenant}"),
        tenant,
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
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let now_ms = 0u64;
    let req = CasWriteRequest::new(
        tenant.clone(),
        hash,
        body.to_vec(),
        format!("anon@{tenant}"),
        tenant,
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
    match e {
        CasHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        CasHandlerError::HashMismatch { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "content hash mismatch",
        )
            .into_response(),
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
        CasRouteState { read, write }
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
        let res = read.read(CasReadRequest::new(
            "t1",
            hash,
            "anon",
            "t1",
            0,
        ));
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
        let upd = CasWriteRequest::new(
            "t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1,
        );
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
        let req1 = CasWriteRequest::new(
            "t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1,
        );
        assert!(st.write.write(req1).expect("first").durable);
        let req2 = CasWriteRequest::new(
            "t1", hash, bytes, "anon@t1", "t1", 2,
        );
        assert!(!st.write.write(req2).expect("retry").durable);
    }
}
