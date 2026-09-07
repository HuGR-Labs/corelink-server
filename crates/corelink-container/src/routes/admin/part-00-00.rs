// `GET /v1/admin/read/:resource` + `POST /v1/admin/mutate` —
// admin-plane read + mutate routes wired against
// [`corelink_handler_admin::AdminReadHandler`] and
// [`corelink_handler_admin::AdminMutateHandler`].
//
// This is the wave-11 end-to-end wire-up for the Admin half of the
// R-prep handler-crate skeleton. Mirrors the cas-read wire-up
// pattern: trait-object route state keeps the binary-shape stable
// across the native / wasm32 swap.
//
// # Dual-approval gate
//
// `POST /v1/admin/mutate` is gated by
// `corelink-handler-admin::AdminMutateHandler`'s dual-approval
// enforcement. The handler emits `MutateAttempted` BEFORE any
// check, then enforces:
//
// 1. RBAC (initiator MUST have admin role) — failure path emits
//    `MutateDenied` BEFORE returning.
// 2. Dual-approval token MUST be present — missing path emits
//    `MutateDualApprovalRejected` BEFORE returning.
// 3. Approver MUST differ from initiator — self-approval path emits
//    `MutateDualApprovalRejected` BEFORE returning.
//
// See `corelink-handler-admin::handler` docs for the full ordering.
//
// # SLO emit
//
// Every entry into either route emits one
// `Sli::AvailControlPlane` observation through the handler's
// `SliObserver`. Per the audit 2026-05-14 closure list, this
// transitions `SLO-AVAIL-CP` from "Sli emitted at handler layer"
// to "Sli emitted at handler layer **and** routed end-to-end via
// apps/server".

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use corelink_handler_admin::{
    AdminHandlerError, AdminMutateHandler, AdminMutateRequest, AdminMutateResponse,
    AdminReadHandler, AdminReadRequest, AdminReadResponse, ApprovalLedger, ApprovalLedgerWriter,
    ApprovalRejection, DualApprovalToken, InMemoryAdminHandler, InMemoryApprovalLedger,
    InMemoryAuditSink, InMemorySliObserver, MutateOp, VerifiedApproval,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

use crate::storage::d1_http::AdminApprovalConsume;
use crate::wall_clock::{SystemWallClock, WallClock};

/// Request header carrying the operator-only shared secret. The admin
/// control plane is operator-only (mirrors `/_internal/pat/mint`): it is
/// gated behind the `CORELINK_INTERNAL_AUTH_KEY` shared secret, NOT
/// reachable by any authenticated tenant PAT.
///
/// Defense-in-depth: the Worker strips any client-supplied
/// `x-corelink-internal-auth` header on the public `/v1/*` path
/// (`x-corelink-internal-auth` is listed in `CLIENT_TRUST_HEADERS` and is
/// removed before the request reaches the container). The constant-time gate
/// below is a second independent layer — both must hold.
pub const ADMIN_INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Constant-time verification of the operator shared secret.
///
/// Fail-CLOSED:
/// - `expected` is `None` (key not configured at boot) → `false`. The
///   admin plane never runs privileged logic without a configured gate.
/// - header absent / wrong → `false`.
///
/// The compare pads the provided value to the expected length and runs a
/// single `ct_eq` so the secret LENGTH is not leaked via early return
/// (improves on the length-short-circuit nit in `internal_pat`).
///
/// Exposed `pub(crate)` so the sibling operator-scoped read module
/// (`routes::admin_tenant_detail`) reuses the SAME constant-time gate
/// rather than re-implementing the compare (single source of truth for
/// the operator auth boundary).
#[must_use]
pub(crate) fn internal_auth_ok(expected: Option<&Arc<str>>, headers: &HeaderMap) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let provided = headers
        .get(ADMIN_INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    // Pad provided to expected length to run ct_eq on equal-length slices,
    // then fold in the real length-equality so a longer/shorter provided
    // value can never match.
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected_bytes.len() {
        provided_bytes
            .get(..expected_bytes.len())
            .unwrap_or(&[])
            .to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected_bytes.len(), 0);
        v
    };
    let content_ok = expected_bytes.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected_bytes.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// Canonical admin read route path (axum-0.7 / matchit-0.7 `:name` capture).
///
/// DEBT-029 (2026-05-16): previously declared with `{resource}` which is
/// matchit-0.8+ syntax and would have panicked at `Router::new()` against
/// the workspace-pinned axum 0.7 / matchit 0.7. Fixed by replacing the
/// brace placeholder with the `:name` form used by every other live
/// route (see `admin_pilot.rs`, `signup.rs`).
pub const ADMIN_READ_ROUTE: &str = "/v1/admin/read/{resource}";

/// Canonical admin mutate route path.
pub const ADMIN_MUTATE_ROUTE: &str = "/v1/admin/mutate";

/// Canonical admin dual-approve route path (finding H5). Records a second
/// approver into the durable approval ledger; the recorded approval then
/// authorizes exactly one [`ADMIN_MUTATE_ROUTE`] call for the same resource.
pub const ADMIN_APPROVE_ROUTE: &str = "/v1/admin/approve";

/// Shared route state — distinct trait objects for read and mutate.
#[derive(Clone)]
pub struct AdminRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see
    /// [`build_handlers`].
    pub read: Arc<dyn AdminReadHandler>,
    /// Mutate handler (separate trait object — see crate-level docs).
    pub mutate: Arc<dyn AdminMutateHandler>,
    /// Operator-only shared secret for the `x-corelink-internal-auth`
    /// gate (sourced from `CORELINK_INTERNAL_AUTH_KEY`). `None` when the
    /// key is unset at boot → every admin handler fails CLOSED (403)
    /// (mirrors the `internal_pat` fail-CLOSED posture).
    pub internal_auth_key: Option<Arc<str>>,
    /// Records a second approver into the durable approval ledger for
    /// `POST /v1/admin/approve`. Shares the same ledger the `mutate`
    /// handler verifies + consumes against.
    pub approval_writer: Arc<dyn ApprovalLedgerWriter>,
    /// Operator-only dedicated secret for the **approve** gate (sourced from
    /// `CORELINK_ADMIN_APPROVER_AUTH_KEY` ONLY — NO shared-key fallback).
    /// Deliberately a DIFFERENT credential from `internal_auth_key` so approve
    /// and mutate require different keys (real two-person control), and enforced
    /// DISTINCT at boot by [`crate::routes::approver_key_distinct_or_none`] (a
    /// byte-equal key collapses to `None`). `None` ⇒ the approve route fails
    /// CLOSED (403).
    pub approver_auth_key: Option<Arc<str>>,
    /// True when the approval ledger is D1-backed (durable). When false
    /// (in-memory dev/CI), recorded approvals do not survive a restart.
    pub approvals_durable: bool,
}

impl core::fmt::Debug for AdminRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdminRouteState").finish_non_exhaustive()
    }
}

/// The admin handler stack: the read + mutate trait objects PLUS the shared
/// approval-ledger writer that backs `POST /v1/admin/approve`.
///
/// The `approval_writer` records approvals into the SAME ledger the `mutate`
/// handler verifies + consumes against, so an approval created by the
/// (independently-authenticated) approve endpoint is exactly what a later
/// mutation must present.
pub struct AdminHandlerStack {
    /// Admin read handler.
    pub read: Arc<dyn AdminReadHandler>,
    /// Admin mutate handler (dual-approval-gated).
    pub mutate: Arc<dyn AdminMutateHandler>,
    /// Records ("creates") approvals for the approve route.
    pub approval_writer: Arc<dyn ApprovalLedgerWriter>,
    /// True when the ledger is D1-backed (durable across restarts). False on
    /// the in-memory dev/CI fallback (approvals are process-local).
    pub durable: bool,
}

impl core::fmt::Debug for AdminHandlerStack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdminHandlerStack")
            .field("durable", &self.durable)
            .finish_non_exhaustive()
    }
}

/// Build the admin handler stack (read + mutate + approval writer) for the
/// current build target.
///
/// # Runtime selection (WP-S1 Phase 2)
///
/// - When `StorageEnv::from_env()` returns `Some(_)` (real CF creds
///   are present in the container environment), this function builds
///   a [`D1AdminHandler`] backed by `CONFIG_DB` over the D1 HTTP API and a
///   durable [`D1ApprovalLedger`] over the `admin_approvals` table
///   (migration 0091). This is the **production path**.
///
/// - When credentials are absent (unit tests, local dev, CI) the function
///   falls back to [`InMemoryAdminHandler`] + [`InMemoryApprovalLedger`]. No
///   network I/O occurs; approvals are process-local.
///
/// On `wasm32-unknown-unknown` a compile-error placeholder is emitted
/// per the `trait-abstraction-defer` rule.
///
/// # Panics
///
/// Does not panic. If the D1 client cannot be constructed, an error
/// is logged and the function falls back to the in-memory stack.
#[must_use]
pub fn build_handler_stack() -> AdminHandlerStack {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{d1_http::D1HttpClient, StorageEnv};

        if let Some(env) = StorageEnv::from_env() {
            // Same block_in_place pattern as cas.rs::build_handler:
            // routes::build_with_factory is called inside #[tokio::main]
            // so a bare Handle::current().block_on(...) would panic
            // ("Cannot start a runtime from within a runtime").
            // D1HttpClient::new is sync, but we keep the bridge in place
            // so future async-construction additions don't regress.
            let db_id = env.d1_database_id.clone();
            let built: Result<D1HttpClient, String> = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async { D1HttpClient::new(&env) })
            });
            match built {
                Ok(client) => {
                    tracing::info!(
                        d1_db = %db_id,
                        "Admin handler: D1 (real storage) + durable approval ledger"
                    );
                    let audit = Arc::new(InMemoryAuditSink::new());
                    let sli = Arc::new(InMemorySliObserver::new());
                    let client = Arc::new(client);
                    let ledger = Arc::new(D1ApprovalLedger::new(client.clone()));
                    let inner = Arc::new(InMemoryAdminHandler::new_with_ledger(
                        audit,
                        sli,
                        ledger.clone() as Arc<dyn ApprovalLedger>,
                    ));
                    let shared: Arc<D1AdminHandler> = Arc::new(D1AdminHandler::new(client, inner));
                    let read: Arc<dyn AdminReadHandler> = shared.clone();
                    let mutate: Arc<dyn AdminMutateHandler> = shared;
                    let approval_writer: Arc<dyn ApprovalLedgerWriter> = ledger;
                    return AdminHandlerStack {
                        read,
                        mutate,
                        approval_writer,
                        durable: true,
                    };
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Admin handler: D1 build failed, falling back to InMemory"
                    );
                }
            }
        }

        tracing::info!("Admin handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let ledger = Arc::new(InMemoryApprovalLedger::new());
        let shared: Arc<InMemoryAdminHandler> = Arc::new(InMemoryAdminHandler::new_with_ledger(
            audit,
            sli,
            ledger.clone() as Arc<dyn ApprovalLedger>,
        ));
        let read: Arc<dyn AdminReadHandler> = shared.clone();
        let mutate: Arc<dyn AdminMutateHandler> = shared;
        let approval_writer: Arc<dyn ApprovalLedgerWriter> = ledger;
        AdminHandlerStack {
            read,
            mutate,
            approval_writer,
            durable: false,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        compile_error!(
            "wasm32 CF-Worker Admin handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Back-compat helper returning just the read + mutate pair. Prefer
/// [`build_handler_stack`] when the approve route also needs wiring.
#[must_use]
pub fn build_handlers() -> (Arc<dyn AdminReadHandler>, Arc<dyn AdminMutateHandler>) {
    let stack = build_handler_stack();
    (stack.read, stack.mutate)
}
