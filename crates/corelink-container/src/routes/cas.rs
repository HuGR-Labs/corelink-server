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
//! `specs/_audits/2026-05-15-stripe-webhook-production.md`
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
    CasHandlerError, CasReadHandler, CasReadRequest, InMemoryAuditSink,
    InMemoryCasHandler, InMemorySliObserver,
};

/// Canonical CAS read route path (matchit-0.7 / axum-0.7 `:name` captures).
///
/// MUST use the `:name` form — matchit 0.7.3 (the version transitively
/// pinned via `axum = "0.7"`) parses `{name}` as **literal path bytes**
/// rather than a capture, which would silently route every real request
/// to a router-level 404. This was DEBT-029 (closed wave-30 stream-1)
/// on `ac.rs` + `admin.rs`; DEBT-029-cas closes the same surface here.
pub const CAS_READ_ROUTE: &str = "/v1/cas/:tenant/:hash";

/// Shared route state.
#[derive(Clone)]
pub struct CasRouteState {
    /// Production wiring builds this `Arc<dyn CasReadHandler>` from
    /// the appropriate native or wasm32 impl; see [`build_handler`].
    pub handler: Arc<dyn CasReadHandler>,
}

impl core::fmt::Debug for CasRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasRouteState").finish_non_exhaustive()
    }
}

/// Build the canonical `Arc<dyn CasReadHandler>` for the current
/// build target.
///
/// On native targets (Linux/Darwin where `apps/server` runs as a
/// container), this returns an `InMemoryCasHandler` wrapping
/// in-memory audit + SLI sinks — production wiring replaces those
/// sinks with the durable `corelink-audit::OutboxEmitter` and a
/// real Prometheus-backed `SliObserver` (forthcoming WI).
///
/// On `wasm32-unknown-unknown` (Cloudflare Worker target) this
/// would build a `CfWorkerCasHandler` against R2 + KV bindings.
/// That impl is **deferred** per the autonomous-execution charter
/// `trait-abstraction-defer` rule; the cfg-gate site below shows
/// the slot.
#[must_use]
pub fn build_handler() -> Arc<dyn CasReadHandler> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        Arc::new(InMemoryCasHandler::new(audit, sli))
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

/// Build the axum `Router` exposing the CAS read route.
pub fn router(state: CasRouteState) -> Router {
    Router::new()
        .route(CAS_READ_ROUTE, get(handle_read))
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
    match state.handler.read(req) {
        Ok(resp) => (StatusCode::OK, resp.bytes).into_response(),
        Err(e) => match e {
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
                // bytes without the audit row.
                (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
            }
            _ => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response()
            }
        },
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
        let h = Arc::new(InMemoryCasHandler::new(audit, sli));
        CasRouteState { handler: h }
    }

    #[test]
    fn route_constant_matches_canonical_path() {
        assert_eq!(CAS_READ_ROUTE, "/v1/cas/:tenant/:hash");
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
    fn build_handler_returns_usable_handler() {
        // Smoke test the build_handler shape on the native target.
        let h = build_handler();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        // We don't have a seed entry point on the trait alone, so
        // we drive the read against a known-empty handler and assert
        // it returns NotFound. The full wire-up is exercised via
        // the handler-crate's own unit tests.
        let res = h.read(CasReadRequest::new(
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
}
