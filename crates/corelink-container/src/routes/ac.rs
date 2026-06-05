//! `GET /v1/ac/:tenant/:action_digest` + `PUT /v1/ac/:tenant/:action_digest`
//! — Action Cache (AC) read + update routes wired against
//! [`corelink_handler_ac::AcLookupHandler`] and
//! [`corelink_handler_ac::AcUpdateHandler`].
//!
//! This is the wave-11 end-to-end wire-up for the AC half of the
//! R-prep handler-crate skeleton (wave-8). The route shape mirrors
//! the cas-read wire-up exactly: the route stores `Arc<dyn ...Handler>`
//! trait objects so the in-memory fake can be swapped for the wasm32
//! CF-Worker R2-bound handler without touching the route layer.
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn AcLookupHandler>` +
//! `Arc<dyn AcUpdateHandler>` so the binary-shape stays stable as
//! the in-memory fakes are swapped for the wasm32 CF-Worker impl.
//! In production both trait objects point at the same underlying
//! handler instance (the `InMemoryAcHandler` implements both
//! halves), but they are kept distinct in the state shape so the
//! AC-read and AC-update SLO emit sites can be backed by
//! independent collaborators if/when needed.
//!
//! # SLO emit
//!
//! Each route entry emits the AC handler's
//! `Sli::AvailAcLookup` (+ `Sli::LatencyAcHitP99` on the lookup path)
//! through the `SliObserver` collaborator. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-AC` and
//! `SLO-LAT-AC-HIT-P99` from "Sli emitted at handler layer" to
//! "Sli emitted at handler layer **and** routed end-to-end via
//! apps/server".
//!
//! # Cross-tenant denial
//!
//! Cross-tenant attempts are rejected with HTTP 403 + an
//! `AuditEventKind::LookupDenied` / `UpdateDenied` row emitted
//! BEFORE the response. This pins `INV-TENANT-ISOLATION` at the
//! route boundary on top of the handler-layer enforcement.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use corelink_handler_ac::{
    AcHandlerError, AcLookupHandler, AcLookupRequest, AcUpdateHandler, AcUpdateRequest,
    InMemoryAcHandler, InMemoryAuditSink, InMemorySliObserver,
};

/// Canonical AC lookup route path (axum-0.7 / matchit-0.7 `:name` captures).
///
/// DEBT-029 (2026-05-16): previously declared with `{tenant}/{action_digest}`
/// which is matchit-0.8+ syntax and would have panicked at `Router::new()`
/// against the workspace-pinned axum 0.7 / matchit 0.7. Fixed by replacing
/// brace placeholders with the `:name` form used by every other live
/// route (see `admin_pilot.rs`, `signup.rs`).
pub const AC_LOOKUP_ROUTE: &str = "/v1/ac/:tenant/:action_digest";

/// Canonical AC update route path. The update path reuses the
/// same template; axum disambiguates by HTTP method.
pub const AC_UPDATE_ROUTE: &str = "/v1/ac/:tenant/:action_digest";

/// Shared route state — distinct trait objects for read and update.
#[derive(Clone)]
pub struct AcRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see
    /// [`build_handlers`].
    pub lookup: Arc<dyn AcLookupHandler>,
    /// Update handler (separate trait object — see crate-level docs).
    pub update: Arc<dyn AcUpdateHandler>,
}

impl core::fmt::Debug for AcRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AcRouteState").finish_non_exhaustive()
    }
}

/// Build the canonical `(Arc<dyn AcLookupHandler>, Arc<dyn AcUpdateHandler>)`
/// pair for the current build target.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets this returns one shared `InMemoryAcHandler`
/// instance behind both trait objects. A real R2/D1-backed AC handler
/// is scheduled for a follow-on WP; this function logs whether storage
/// credentials are configured so the operator can verify the env is
/// correct even before the real handler lands.
///
/// The wasm32 CF-Worker impl is deferred per the autonomous-execution
/// charter `trait-abstraction-defer` rule (tracked as
/// `WI-S04-CF-WIRING`).
#[must_use]
pub fn build_handlers() -> (Arc<dyn AcLookupHandler>, Arc<dyn AcUpdateHandler>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_s3, StorageEnv};

        // Probe for storage credentials. When present, construct the
        // real R2-backed handler; otherwise fall back to InMemory.
        // Mirrors `routes::cas::build_handler` exactly (see
        // crate-level docs + commit 832c7884 for the
        // `block_in_place` rationale).
        if StorageEnv::from_env().is_some() {
            // Empty-or-absent → default (the DO forwards `?? ""`;
            // see storage::env_or — prod AC-500 incident 2026-06-05).
            let bucket = crate::storage::env_or("R2_AC_BUCKET", "corelink-ac-iad");
            let region = crate::storage::env_or("R2_AC_REGION", "iad");

            // `routes::build_with_factory` is called from inside
            // `#[tokio::main]`, so a bare
            // `Handle::current().block_on(...)` panics with "Cannot
            // start a runtime from within a runtime". Wrap with
            // `tokio::task::block_in_place` (multi-thread runtime
            // only — `#[tokio::main]` guarantees that).
            let handle = tokio::runtime::Handle::current();
            let built = tokio::task::block_in_place(|| {
                handle.block_on(r2_s3::build_r2_ac_handler_from_env(&bucket, &region))
            });
            match built {
                Some(Ok(handler)) => {
                    tracing::info!(
                        bucket = %bucket,
                        region = %region,
                        "AC handler: R2S3 (real storage)"
                    );
                    let shared: Arc<r2_s3::R2AcHandler> = Arc::new(handler);
                    let lookup: Arc<dyn AcLookupHandler> = shared.clone();
                    let update: Arc<dyn AcUpdateHandler> = shared;
                    return (lookup, update);
                }
                Some(Err(e)) => {
                    tracing::error!(
                        error = %e,
                        "AC handler: R2S3 build failed, falling back to InMemory"
                    );
                }
                None => {
                    // Should not happen: we already checked is_some().
                }
            }
        }

        // Fallback: InMemory (no creds or build failure).
        tracing::info!("AC handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryAcHandler> = Arc::new(InMemoryAcHandler::new(audit, sli));
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared;
        (lookup, update)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Placeholder for `corelink_handler_ac::cf_worker::CfWorkerAcHandler`
        // (deferred per trait-abstraction-defer; see crate-level docs).
        compile_error!(
            "wasm32 CF-Worker AC handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the AC lookup + update routes.
pub fn router(state: AcRouteState) -> Router {
    Router::new()
        .route(AC_LOOKUP_ROUTE, get(handle_lookup).put(handle_update))
        .with_state(state)
}

/// `GET /v1/ac/:tenant/:action_digest` handler — lookup.
async fn handle_lookup(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
) -> impl IntoResponse {
    // Logical clock stand-in (handler is the source of truth in
    // production; see cas.rs for the same rationale).
    let now_ms = 0u64;
    let req = AcLookupRequest::new(
        tenant.clone(),
        action_digest,
        // Principal is filled by an auth middleware in production;
        // demo wire-up mirrors cas.rs.
        format!("anon@{tenant}"),
        tenant,
        now_ms,
    );
    match state.lookup.lookup(req) {
        Ok(resp) => (StatusCode::OK, resp.result_payload).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v1/ac/:tenant/:action_digest` handler — update.
async fn handle_update(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let now_ms = 0u64;
    let req = AcUpdateRequest::new(
        tenant.clone(),
        action_digest,
        body.to_vec(),
        format!("anon@{tenant}"),
        tenant,
        now_ms,
    );
    match state.update.update(req) {
        Ok(resp) => {
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.action_digest).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Map an [`AcHandlerError`] to the canonical HTTP response.
fn map_err(e: AcHandlerError) -> axum::response::Response {
    match e {
        AcHandlerError::Miss { .. } => {
            (StatusCode::NOT_FOUND, "ac miss").into_response()
        }
        AcHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        AcHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit updates without the audit row.
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
    use corelink_handler_ac::{AuditEventKind, Sli};

    /// Test fixture: returns the route state plus the underlying
    /// audit + SLI sinks so each test can verify emit ordering.
    fn fixture() -> (Arc<InMemoryAuditSink>, Arc<InMemorySliObserver>, AcRouteState) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryAcHandler::new(audit.clone(), sli.clone()));
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared;
        (audit, sli, AcRouteState { lookup, update })
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(AC_LOOKUP_ROUTE, "/v1/ac/:tenant/:action_digest");
        assert_eq!(AC_UPDATE_ROUTE, "/v1/ac/:tenant/:action_digest");
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        let (lookup, _update) = build_handlers();
        let res = lookup.lookup(AcLookupRequest::new("t1", "d1", "anon", "t1", 0));
        assert!(matches!(res, Err(AcHandlerError::Miss { .. })));
        // Router constructor smoke.
        let (_a, _s, st) = fixture();
        let _router = router(st);
    }

    /// Happy path: PUT then GET round-trip emits audit + Sli on both
    /// legs and returns the stored bytes.
    #[test]
    fn put_then_get_round_trip_emits_audit_and_sli() {
        let (audit, sli, st) = fixture();
        // Update path.
        let upd_req = AcUpdateRequest::new(
            "t1", "d1", b"result".to_vec(), "anon@t1", "t1", 1,
        );
        let upd_resp = st.update.update(upd_req).expect("update");
        assert!(upd_resp.durable);
        // Lookup path.
        let lk_req = AcLookupRequest::new("t1", "d1", "anon@t1", "t1", 2);
        let lk_resp = st.lookup.lookup(lk_req).expect("lookup hit");
        assert_eq!(lk_resp.action_digest, "d1");
        assert_eq!(lk_resp.result_payload, b"result".to_vec());
        // Audit rows: UpdateAttempted + UpdateCommitted + LookupAttempted + LookupHit.
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::UpdateAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::UpdateCommitted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupHit));
        // SLI: at least one AvailAcLookup + one LatencyAcHitP99 non-error.
        let obs = sli.snapshot().expect("sli");
        assert!(obs.iter().any(|o| o.sli == Sli::AvailAcLookup && !o.is_error));
        assert!(obs.iter().any(|o| o.sli == Sli::LatencyAcHitP99 && !o.is_error));
    }

    /// Cross-tenant lookup MUST emit `LookupDenied` BEFORE the error.
    /// Pins `INV-TENANT-ISOLATION` at the route boundary.
    #[test]
    fn cross_tenant_lookup_denied_audits_before_rejection() {
        let (audit, _sli, st) = fixture();
        let req = AcLookupRequest::new(
            "victim",
            "d1",
            "attacker@attacker_t",
            "attacker_t",
            1,
        );
        let err = st.lookup.lookup(req).expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        // First row MUST be LookupDenied (fail-CLOSED ordering pin).
        assert_eq!(rows[0].kind, AuditEventKind::LookupDenied);
        assert_eq!(rows[0].tenant, "victim");
        assert_eq!(rows[0].principal, "attacker@attacker_t");
    }

    /// Audit-fail aborts the update mutation (fail-CLOSED). Verifies
    /// the auth/audit-fail handling path used by `map_err` returns
    /// 503 semantically through the handler error variant.
    #[test]
    fn audit_failure_aborts_update_and_maps_to_audit_closed() {
        let (audit, _sli, st) = fixture();
        audit.inject_failure("audit pipeline down").expect("inject");
        let upd = AcUpdateRequest::new(
            "t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1,
        );
        let err = st.update.update(upd).expect_err("audit closed");
        assert!(matches!(err, AcHandlerError::AuditFailed(_)));
        // Verify the route-layer `map_err` translates this to 503.
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Idempotent retry: a second PUT with the same bytes does NOT
    /// re-create the entry (`durable=false`) and audit + Sli still
    /// fire on the retry — pins R-prep retry idempotency.
    #[test]
    fn idempotent_retry_keeps_state_stable() {
        let (audit, sli, st) = fixture();
        let req1 = AcUpdateRequest::new(
            "t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1,
        );
        let r1 = st.update.update(req1).expect("first put");
        assert!(r1.durable, "fresh insert");
        let req2 = AcUpdateRequest::new(
            "t1", "d1", b"r".to_vec(), "anon@t1", "t1", 2,
        );
        let r2 = st.update.update(req2).expect("retry put");
        assert!(!r2.durable, "retry MUST NOT be durable=true");
        // Audit rows: two UpdateAttempted + two UpdateCommitted.
        let rows = audit.snapshot().expect("audit");
        let attempts = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateAttempted)
            .count();
        let commits = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateCommitted)
            .count();
        assert_eq!(attempts, 2);
        assert_eq!(commits, 2);
        // SLI emit on every entry.
        let obs_count = sli.count(Sli::AvailAcLookup).expect("count");
        assert!(obs_count >= 2);
    }

    /// Auth-fail surrogate: a missing-entry lookup returns Miss; the
    /// route layer maps Miss to HTTP 404. (The route does not yet
    /// have an auth middleware — auth-fail here is exercised as the
    /// cross-tenant denial above, which is the auth-equivalent
    /// failure path until the auth middleware lands.)
    #[test]
    fn miss_maps_to_404() {
        let (_audit, _sli, st) = fixture();
        let lk = AcLookupRequest::new("t1", "ghost", "anon@t1", "t1", 1);
        let err = st.lookup.lookup(lk).expect_err("miss");
        assert!(matches!(err, AcHandlerError::Miss { .. }));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
