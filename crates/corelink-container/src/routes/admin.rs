//! `GET /v1/admin/read/:resource` + `POST /v1/admin/mutate` —
//! admin-plane read + mutate routes wired against
//! [`corelink_handler_admin::AdminReadHandler`] and
//! [`corelink_handler_admin::AdminMutateHandler`].
//!
//! This is the wave-11 end-to-end wire-up for the Admin half of the
//! R-prep handler-crate skeleton. Mirrors the cas-read wire-up
//! pattern: trait-object route state keeps the binary-shape stable
//! across the native / wasm32 swap.
//!
//! # Dual-approval gate
//!
//! `POST /v1/admin/mutate` is gated by
//! `corelink-handler-admin::AdminMutateHandler`'s dual-approval
//! enforcement. The handler emits `MutateAttempted` BEFORE any
//! check, then enforces:
//!
//! 1. RBAC (initiator MUST have admin role) — failure path emits
//!    `MutateDenied` BEFORE returning.
//! 2. Dual-approval token MUST be present — missing path emits
//!    `MutateDualApprovalRejected` BEFORE returning.
//! 3. Approver MUST differ from initiator — self-approval path emits
//!    `MutateDualApprovalRejected` BEFORE returning.
//!
//! See `corelink-handler-admin::handler` docs for the full ordering.
//!
//! # SLO emit
//!
//! Every entry into either route emits one
//! `Sli::AvailControlPlane` observation through the handler's
//! `SliObserver`. Per the audit 2026-05-14 closure list, this
//! transitions `SLO-AVAIL-CP` from "Sli emitted at handler layer"
//! to "Sli emitted at handler layer **and** routed end-to-end via
//! apps/server".

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use corelink_handler_admin::{
    AdminHandlerError, AdminMutateHandler, AdminMutateRequest, AdminReadHandler,
    AdminReadRequest, DualApprovalToken, InMemoryAdminHandler, InMemoryAuditSink,
    InMemorySliObserver, MutateOp,
};
use serde::Deserialize;

/// Canonical admin read route path (axum-0.7 / matchit-0.7 `:name` capture).
///
/// DEBT-029 (2026-05-16): previously declared with `{resource}` which is
/// matchit-0.8+ syntax and would have panicked at `Router::new()` against
/// the workspace-pinned axum 0.7 / matchit 0.7. Fixed by replacing the
/// brace placeholder with the `:name` form used by every other live
/// route (see `admin_pilot.rs`, `signup.rs`).
pub const ADMIN_READ_ROUTE: &str = "/v1/admin/read/:resource";

/// Canonical admin mutate route path.
pub const ADMIN_MUTATE_ROUTE: &str = "/v1/admin/mutate";

/// Shared route state — distinct trait objects for read and mutate.
#[derive(Clone)]
pub struct AdminRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see
    /// [`build_handlers`].
    pub read: Arc<dyn AdminReadHandler>,
    /// Mutate handler (separate trait object — see crate-level docs).
    pub mutate: Arc<dyn AdminMutateHandler>,
}

impl core::fmt::Debug for AdminRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdminRouteState").finish_non_exhaustive()
    }
}

/// Build the canonical
/// `(Arc<dyn AdminReadHandler>, Arc<dyn AdminMutateHandler>)` pair
/// for the current build target.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets this returns one shared `InMemoryAdminHandler`
/// instance behind both trait objects. A real D1-backed admin handler
/// is scheduled for a follow-on WP; this function logs whether storage
/// credentials are configured. See
/// [`crate::routes::ac::build_handlers`] for the
/// trait-abstraction-defer rationale.
#[must_use]
pub fn build_handlers() -> (Arc<dyn AdminReadHandler>, Arc<dyn AdminMutateHandler>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Log whether real storage creds are present (real admin
        // handler lands in WP-S1 Phase 2).
        if crate::storage::StorageEnv::from_env().is_some() {
            tracing::info!(
                "Admin handler: InMemory (real D1 admin handler lands in WP-S1 Phase 2; \
                 storage env is configured)"
            );
        } else {
            tracing::info!("Admin handler: InMemory (no storage credentials configured)");
        }
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryAdminHandler> =
            Arc::new(InMemoryAdminHandler::new(audit, sli));
        let read: Arc<dyn AdminReadHandler> = shared.clone();
        let mutate: Arc<dyn AdminMutateHandler> = shared;
        (read, mutate)
    }
    #[cfg(target_arch = "wasm32")]
    {
        compile_error!(
            "wasm32 CF-Worker Admin handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the admin read + mutate routes.
pub fn router(state: AdminRouteState) -> Router {
    Router::new()
        .route(ADMIN_READ_ROUTE, get(handle_read))
        .route(ADMIN_MUTATE_ROUTE, post(handle_mutate))
        .with_state(state)
}

/// JSON shape for `POST /v1/admin/mutate`.
///
/// The route is intentionally hand-coded against `serde_json` rather
/// than a `prost`-generated message because the admin plane is the
/// control plane: cardinality is small, evolution is cheap, and the
/// surface is human-curated. Production wiring threads the initiator
/// principal + admin-role flag through an auth middleware; the JSON
/// body carries only the op + approval-token fields.
#[derive(Clone, Debug, Deserialize)]
pub struct AdminMutateBody {
    /// Operation kind (`set_tenant_tier` / `rotate_admin_token`).
    pub op_kind: String,
    /// Tenant ID (required for `set_tenant_tier`).
    pub tenant: Option<String>,
    /// New tier (required for `set_tenant_tier`).
    pub tier: Option<String>,
    /// Token ID (required for `rotate_admin_token`).
    pub token_id: Option<String>,
    /// Initiator principal (production: from auth middleware).
    pub initiator: String,
    /// True if initiator carries the admin role.
    pub initiator_is_admin: bool,
    /// Approval id from the dual-approval ledger (None = rejected).
    pub approval_id: Option<String>,
    /// Second approver principal (None = rejected).
    pub approver: Option<String>,
}

impl AdminMutateBody {
    /// Parse the body into an [`AdminMutateRequest`].
    ///
    /// # Errors
    ///
    /// Returns a static error string when the op-kind / required
    /// fields combination is invalid (e.g. `set_tenant_tier` without
    /// `tenant` + `tier`).
    pub fn into_request(self, at_unix_ms: u64) -> Result<AdminMutateRequest, &'static str> {
        let op = match self.op_kind.as_str() {
            "set_tenant_tier" => {
                let tenant = self.tenant.ok_or("set_tenant_tier requires tenant")?;
                let tier = self.tier.ok_or("set_tenant_tier requires tier")?;
                MutateOp::set_tenant_tier(tenant, tier)
            }
            "rotate_admin_token" => {
                let token_id = self
                    .token_id
                    .ok_or("rotate_admin_token requires token_id")?;
                MutateOp::rotate_admin_token(token_id)
            }
            _ => return Err("unknown op_kind"),
        };
        let approval = match (self.approval_id, self.approver) {
            (Some(id), Some(approver)) => Some(DualApprovalToken::new(id, approver)),
            (None, None) => None,
            _ => return Err("approval_id + approver must be set together"),
        };
        Ok(AdminMutateRequest::new(
            op,
            self.initiator,
            self.initiator_is_admin,
            approval,
            at_unix_ms,
        ))
    }
}

/// `GET /v1/admin/read/:resource` handler.
async fn handle_read(
    State(state): State<AdminRouteState>,
    Path(resource): Path<String>,
) -> impl IntoResponse {
    let now_ms = 0u64;
    // Demo wire-up: the auth middleware (production) injects the
    // principal + is_admin flag; here we accept the resource path
    // directly and assume an admin principal.
    let req = AdminReadRequest::new(resource, "admin@root", true, now_ms);
    match state.read.read(req) {
        Ok(resp) => (StatusCode::OK, resp.body).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /v1/admin/mutate` handler.
async fn handle_mutate(
    State(state): State<AdminRouteState>,
    Json(body): Json<AdminMutateBody>,
) -> impl IntoResponse {
    let now_ms = 0u64;
    let req = match body.into_request(now_ms) {
        Ok(r) => r,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    match state.mutate.mutate(req) {
        Ok(resp) => (StatusCode::OK, resp.resource).into_response(),
        Err(e) => map_err(e),
    }
}

/// Map an [`AdminHandlerError`] to the canonical HTTP response.
fn map_err(e: AdminHandlerError) -> axum::response::Response {
    match e {
        AdminHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "admin not found").into_response()
        }
        AdminHandlerError::Forbidden { .. } => {
            (StatusCode::FORBIDDEN, "forbidden").into_response()
        }
        AdminHandlerError::DualApprovalMissing => (
            StatusCode::FORBIDDEN,
            "dual-approval required",
        )
            .into_response(),
        AdminHandlerError::DualApprovalSelfApproval { .. } => (
            StatusCode::FORBIDDEN,
            "self-approval rejected",
        )
            .into_response(),
        AdminHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never mutate.
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
    use corelink_handler_admin::{AuditEventKind, Sli};

    fn fixture() -> (
        Arc<InMemoryAuditSink>,
        Arc<InMemorySliObserver>,
        Arc<InMemoryAdminHandler>,
        AdminRouteState,
    ) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryAdminHandler::new(audit.clone(), sli.clone()));
        let read: Arc<dyn AdminReadHandler> = shared.clone();
        let mutate: Arc<dyn AdminMutateHandler> = shared.clone();
        (audit, sli, shared, AdminRouteState { read, mutate })
    }

    #[test]
    fn route_constants_match_canonical_paths() {
        assert_eq!(ADMIN_READ_ROUTE, "/v1/admin/read/:resource");
        assert_eq!(ADMIN_MUTATE_ROUTE, "/v1/admin/mutate");
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        let (read, _mutate) = build_handlers();
        let res = read.read(AdminReadRequest::new("ghost", "admin", true, 0));
        assert!(matches!(res, Err(AdminHandlerError::NotFound { .. })));
        let (_a, _s, _shared, st) = fixture();
        let _router = router(st);
    }

    /// Happy path: admin read returns the seeded body + emits
    /// `ReadAttempted` + `ReadServed` audit rows and the
    /// `Sli::AvailControlPlane` observation.
    #[test]
    fn admin_read_happy_emits_audit_and_sli() {
        let (audit, sli, shared, st) = fixture();
        shared.seed_read("tenant:t1", b"{}").expect("seed");
        let req = AdminReadRequest::new("tenant:t1", "admin@root", true, 1);
        let resp = st.read.read(req).expect("read");
        assert_eq!(resp.resource, "tenant:t1");
        assert_eq!(resp.body, b"{}".to_vec());
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::ReadAttempted);
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::ReadServed));
        assert!(sli
            .snapshot()
            .expect("sli")
            .iter()
            .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
    }

    /// Auth-fail: a non-admin principal MUST be rejected with
    /// `Forbidden` and the audit row `ReadDenied` MUST be emitted
    /// BEFORE the rejection (fail-CLOSED ordering pin).
    #[test]
    fn admin_read_auth_fail_audits_before_denial() {
        let (audit, _sli, _shared, st) = fixture();
        let req = AdminReadRequest::new("tenant:t1", "alice", false, 1);
        let err = st.read.read(req).expect_err("forbidden");
        assert!(matches!(err, AdminHandlerError::Forbidden { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
    }

    /// Cross-tenant equivalent: admin reads are not tenant-scoped at
    /// the handler boundary, so `INV-TENANT-ISOLATION` is enforced
    /// through RBAC (Forbidden) for non-admin principals attempting
    /// to read a tenant resource they don't own.
    #[test]
    fn admin_read_cross_tenant_via_rbac_denial() {
        let (audit, _sli, _shared, st) = fixture();
        // Non-admin principal attempting to read another tenant's data.
        let req = AdminReadRequest::new("tenant:victim", "attacker", false, 1);
        let err = st.read.read(req).expect_err("denied");
        assert!(matches!(err, AdminHandlerError::Forbidden { .. }));
        let rows = audit.snapshot().expect("audit");
        // Audit row pins the principal + resource attempted —
        // analyst can reconstruct the cross-tenant probe.
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
        assert_eq!(rows[0].principal, "attacker");
        assert_eq!(rows[0].resource, "tenant:victim");
    }

    /// Dual-approval missing MUST be rejected with audit
    /// `MutateDualApprovalRejected` BEFORE the response.
    #[test]
    fn admin_mutate_dual_approval_missing_rejected() {
        let (audit, _sli, _shared, st) = fixture();
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", "Team"),
            "alice",
            true,
            None,
            1,
        );
        let err = st.mutate.mutate(req).expect_err("dual approval");
        assert!(matches!(err, AdminHandlerError::DualApprovalMissing));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::MutateAttempted);
        assert_eq!(rows[1].kind, AuditEventKind::MutateDualApprovalRejected);
    }

    /// Self-approval (initiator == approver) MUST be rejected with
    /// audit `MutateDualApprovalRejected` BEFORE the response.
    #[test]
    fn admin_mutate_self_approval_rejected() {
        let (audit, _sli, _shared, st) = fixture();
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", "Team"),
            "alice",
            true,
            Some(DualApprovalToken::new("a1", "alice")),
            1,
        );
        let err = st.mutate.mutate(req).expect_err("self approval");
        assert!(matches!(
            err,
            AdminHandlerError::DualApprovalSelfApproval { .. }
        ));
        let rows = audit.snapshot().expect("audit");
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::MutateDualApprovalRejected));
    }

    /// Happy mutate path: emits `MutateAttempted` + `MutateCommitted`,
    /// records `Sli::AvailControlPlane` non-error.
    #[test]
    fn admin_mutate_happy_commits_and_audits() {
        let (audit, sli, shared, st) = fixture();
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", "Team"),
            "alice",
            true,
            Some(DualApprovalToken::new("a1", "bob")),
            1,
        );
        let resp = st.mutate.mutate(req).expect("commit");
        assert_eq!(resp.resource, "tenant:t1");
        assert_eq!(resp.approval_id, "a1");
        assert_eq!(shared.applied_snapshot().expect("snap").len(), 1);
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::MutateCommitted));
        assert!(sli
            .snapshot()
            .expect("sli")
            .iter()
            .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
    }

    /// Audit failure MUST abort the mutation — fail-CLOSED ordering.
    #[test]
    fn admin_mutate_audit_failure_aborts_before_state_change() {
        let (audit, _sli, shared, st) = fixture();
        audit.inject_failure("audit pipeline down").expect("inject");
        let req = AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", "Team"),
            "alice",
            true,
            Some(DualApprovalToken::new("a1", "bob")),
            1,
        );
        let err = st.mutate.mutate(req).expect_err("audit closed");
        assert!(matches!(err, AdminHandlerError::AuditFailed(_)));
        assert!(shared.applied_snapshot().expect("snap").is_empty());
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Idempotent retry: a second mutation with the same approval-id
    /// re-records `MutateCommitted` and stays consistent (the
    /// in-memory handler overwrites the applied entry; the test
    /// pins that no panic / no audit-row loss happens on retry).
    #[test]
    fn admin_mutate_idempotent_retry_stable_state() {
        let (audit, _sli, shared, st) = fixture();
        for at in [1u64, 2u64] {
            let req = AdminMutateRequest::new(
                MutateOp::set_tenant_tier("t1", "Team"),
                "alice",
                true,
                Some(DualApprovalToken::new("a1", "bob")),
                at,
            );
            st.mutate.mutate(req).expect("commit");
        }
        // One applied resource (overwritten on retry).
        assert_eq!(shared.applied_snapshot().expect("snap").len(), 1);
        let rows = audit.snapshot().expect("audit");
        let commits = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::MutateCommitted)
            .count();
        assert_eq!(commits, 2, "every retry emits an audit row");
    }

    /// Body parsing: `set_tenant_tier` without `tier` MUST fail at
    /// the route boundary before reaching the handler.
    #[test]
    fn admin_mutate_body_parse_rejects_missing_fields() {
        let body = AdminMutateBody {
            op_kind: "set_tenant_tier".into(),
            tenant: Some("t1".into()),
            tier: None,
            token_id: None,
            initiator: "alice".into(),
            initiator_is_admin: true,
            approval_id: Some("a1".into()),
            approver: Some("bob".into()),
        };
        let err = body.into_request(0).expect_err("missing tier");
        assert!(err.contains("tier"));
    }
}
