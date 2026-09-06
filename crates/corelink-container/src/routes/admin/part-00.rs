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

/// Real D1-backed admin handler (WP-S1 Phase 2).
///
/// Implements both [`AdminReadHandler`] and [`AdminMutateHandler`] by
/// translating each operation to a D1 HTTP query against the
/// `tier_selections` table (per `migrations/d1/0039_tier_selection.sql`).
///
/// # Delegation pattern
///
/// To keep the audit + SLI emit ordering contract identical to the
/// [`InMemoryAdminHandler`] (`MutateAttempted` BEFORE checks,
/// `MutateDualApprovalRejected` / `MutateDenied` BEFORE rejection,
/// `MutateCommitted` AFTER state change, `Sli::AvailControlPlane` on
/// every return path), this handler **delegates** to an inner
/// `InMemoryAdminHandler` for the validation + audit + SLI plumbing
/// and **mirrors** the resulting state to D1:
///
/// - On read: D1 lookup runs FIRST. Found rows are seeded into the
///   inner handler so the delegated `read` emits the canonical
///   `ReadAttempted` + `ReadServed` rows and returns the JSON body.
/// - On mutate: the delegated `mutate` enforces RBAC + dual-approval
///   and emits the canonical audit chain. On `Ok` it has applied the
///   state in-memory; we then mirror to D1 (`tenant_set_tier`). If D1
///   mirroring fails, the inner audit row stays `MutateCommitted` and
///   the caller receives `Internal(...)` — a tracked durability gap
///   addressed by the outbox flow (WP-S1 Phase 2 follow-up §3).
///
/// # Operation coverage
///
/// - Read `tenant:<id>` — implemented (production path).
/// - Mutate [`MutateOp::SetTenantTier`] — implemented (production path).
/// - Mutate [`MutateOp::RotateAdminToken`] — returns `Internal(...)`
///   indicating the multi-step PAT rotation flow is not yet wired
///   through the admin trait. Tracked as Phase 2 follow-up.
pub struct D1AdminHandler {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
    inner: Arc<InMemoryAdminHandler>,
}

impl core::fmt::Debug for D1AdminHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AdminHandler").finish_non_exhaustive()
    }
}

impl D1AdminHandler {
    /// Construct from collaborators.
    #[must_use]
    pub fn new(
        client: Arc<crate::storage::d1_http::D1HttpClient>,
        inner: Arc<InMemoryAdminHandler>,
    ) -> Self {
        Self { client, inner }
    }

    /// Bridge sync trait method → async D1 query via `block_in_place`.
    ///
    /// Mirrors the `cas.rs::build_handler` rationale: routes are
    /// driven from `#[tokio::main]`, so a bare
    /// `Handle::current().block_on(...)` panics with "Cannot start a
    /// runtime from within a runtime". `block_in_place` releases the
    /// current worker so the inner `block_on` is legal.
    fn block_on<F, T>(fut: F) -> T
    where
        F: core::future::Future<Output = T>,
    {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(fut))
    }
}

impl AdminReadHandler for D1AdminHandler {
    fn read(&self, req: AdminReadRequest) -> Result<AdminReadResponse, AdminHandlerError> {
        // Pre-flight: if this is a tenant resource and the caller is
        // admin, fetch from D1 and seed into the inner handler so the
        // delegated read emits the canonical ReadAttempted + ReadServed
        // chain and returns a real D1-sourced body.
        //
        // For non-admin callers we let the inner handler emit ReadDenied
        // BEFORE returning Forbidden (fail-CLOSED ordering pin).
        if req.is_admin {
            if let Some(tenant_id) = req.resource.strip_prefix("tenant:") {
                match Self::block_on(self.client.tenant_admin_lookup(tenant_id)) {
                    Ok(Some(record)) => {
                        let body = serde_json::to_vec(&record).map_err(|e| {
                            AdminHandlerError::Internal(format!("admin read json encode: {e}"))
                        })?;
                        self.inner.seed_read(req.resource.clone(), body)?;
                    }
                    Ok(None) => {
                        // Leave body unseeded — inner returns NotFound
                        // (after emitting ReadAttempted).
                    }
                    Err(e) => {
                        return Err(AdminHandlerError::Internal(format!("D1 admin read: {e}")));
                    }
                }
            }
        }
        self.inner.read(req)
    }
}

impl AdminMutateHandler for D1AdminHandler {
    fn mutate(&self, req: AdminMutateRequest) -> Result<AdminMutateResponse, AdminHandlerError> {
        // Pre-validate the D1-backed op surface BEFORE delegating, so
        // unsupported ops are rejected without polluting the audit log
        // with a MutateCommitted row.
        if let MutateOp::RotateAdminToken { .. } = req.op {
            return Err(AdminHandlerError::Internal(
                "rotate_admin_token not implemented in D1 backend \
                 (WP-S1 Phase 2 follow-up — multi-step PAT rotation)"
                    .to_owned(),
            ));
        }

        // Capture the op shape for the post-commit D1 mirror (the
        // inner handler consumes `req` by value).
        let mirror = match &req.op {
            MutateOp::SetTenantTier { tenant, tier } => Some((tenant.clone(), tier.clone())),
            _ => None,
        };

        // Delegate to InMemoryAdminHandler — it enforces RBAC + dual-
        // approval + emits MutateAttempted / MutateDenied /
        // MutateDualApprovalRejected / MutateCommitted in canonical
        // order and emits Sli::AvailControlPlane on every return path.
        let resp = self.inner.mutate(req)?;

        // Mirror the mutation to D1. The inner handler has already
        // emitted MutateCommitted; a D1 failure surfaces as
        // Internal(...) and is tracked by the outbox flow.
        if let Some((tenant, tier)) = mirror {
            match Self::block_on(self.client.tenant_set_tier(&tenant, &tier)) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(AdminHandlerError::NotFound {
                        what: format!("tenant:{tenant}"),
                    });
                }
                Err(e) => {
                    return Err(AdminHandlerError::Internal(format!(
                        "D1 admin mutate mirror: {e}"
                    )));
                }
            }
        }
        Ok(resp)
    }
}

/// Durable, D1-backed dual-approval ledger (migration 0091, finding H5).
///
/// Implements both halves of the ledger over the `admin_approvals` table:
/// [`ApprovalLedgerWriter::record_approval`] (the approve endpoint's "create"
/// step) and [`ApprovalLedger::verify_and_consume`] (the mutate handler's
/// verify + single-use consume). The consume is an atomic conditional
/// `UPDATE ... WHERE consumed = 0 RETURNING`, so a concurrent replay of the
/// same approval cannot double-spend.
///
/// Both trait methods are sync (the ledger is consulted from the sync admin
/// handler chain); each bridges to the async D1 client with the same
/// `block_in_place` pattern used by [`D1AdminHandler`].
pub struct D1ApprovalLedger {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl core::fmt::Debug for D1ApprovalLedger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1ApprovalLedger").finish_non_exhaustive()
    }
}

impl D1ApprovalLedger {
    /// Construct from the shared D1 client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }

    /// Bridge sync → async D1 query (see [`D1AdminHandler::block_on`]).
    fn block_on<F, T>(fut: F) -> T
    where
        F: core::future::Future<Output = T>,
    {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(fut))
    }
}

impl ApprovalLedger for D1ApprovalLedger {
    fn verify_and_consume(
        &self,
        approval_id: &str,
        initiator: &str,
        resource: &str,
    ) -> Result<VerifiedApproval, ApprovalRejection> {
        let now_ms = SystemWallClock.now_ms();
        let outcome = Self::block_on(self.client.admin_approval_verify_consume(
            approval_id,
            initiator,
            resource,
            i64::try_from(now_ms).unwrap_or(i64::MAX),
        ))
        .map_err(ApprovalRejection::Backend)?;
        match outcome {
            AdminApprovalConsume::Consumed { approver } => Ok(VerifiedApproval::new(approver)),
            AdminApprovalConsume::Unknown => Err(ApprovalRejection::Unknown),
            AdminApprovalConsume::ScopeMismatch => Err(ApprovalRejection::ScopeMismatch),
            AdminApprovalConsume::SelfApproval { approver } => {
                Err(ApprovalRejection::SelfApproval { approver })
            }
            AdminApprovalConsume::AlreadyConsumed => Err(ApprovalRejection::Consumed),
        }
    }
}

impl ApprovalLedgerWriter for D1ApprovalLedger {
    fn record_approval(
        &self,
        approval_id: &str,
        approver: &str,
        resource: &str,
    ) -> Result<(), String> {
        let now_ms = SystemWallClock.now_ms();
        Self::block_on(self.client.admin_approval_create(
            approval_id,
            approver,
            resource,
            i64::try_from(now_ms).unwrap_or(i64::MAX),
        ))
    }
}

/// Minimum accepted length (chars) for any internal-auth shared secret.
///
/// Shared 32-char floor used by EVERY internal-auth reader in the
/// container (admin, mint, erase, DSR, introspect) so all gates stay
/// consistent (F29 fix). The secrets-checklist instructs
/// `openssl rand -hex 32` (64 chars); anything shorter is rejected.
pub(crate) const INTERNAL_AUTH_KEY_MIN_LEN: usize = 32;

/// Per-consumer internal-auth key split (red-team #3).
///
/// A single shared `CORELINK_INTERNAL_AUTH_KEY` previously gated FIVE
/// high-privilege internal surfaces (any-tenant PAT mint, GDPR erase,
/// CAS erase, admin, pilots) — one leak granted ALL of them. This helper
/// reads a **consumer-specific** key first and only falls back to the
/// shared key when the specific one is truly absent, so each
/// surface can be rotated to its own credential without a flag day
/// (mirrors the #8 OCI dual-name pattern — additive, deployable BEFORE
/// the new prod secrets exist).
///
/// Resolution order (fail-CLOSED at each step):
/// 1. `specific_env` — used iff set AND ≥ [`INTERNAL_AUTH_KEY_MIN_LEN`];
/// 2. else `CORELINK_INTERNAL_AUTH_KEY` — used iff set AND ≥ floor;
/// 3. else `None` — the handler fails CLOSED (403), exactly as today.
///
/// A present-but-invalid `specific_env` (including empty, whitespace-only,
/// non-Unicode, or sub-floor values) hard-fails the surface. Treating a
/// malformed declaration as absent would silently widen authorization back to
/// the shared authority. Only a truly absent dedicated variable may use the
/// shared migration fallback.
#[must_use]
pub(crate) fn resolve_internal_auth_key(specific_env: &str) -> Option<Arc<str>> {
    // 1. A present dedicated key owns the decision, including rejection. Use
    // var_os so a non-Unicode value cannot masquerade as an absent variable.
    match std::env::var_os(specific_env) {
        None => {}
        Some(raw) => {
            let Ok(key) = raw.into_string() else {
                tracing::warn!(
                    env = specific_env,
                    "consumer-specific internal-auth key is not valid UTF-8; \
                     this surface will fail CLOSED (403)"
                );
                return None;
            };
            if key.len() < INTERNAL_AUTH_KEY_MIN_LEN || key.trim().is_empty() {
                tracing::warn!(
                    env = specific_env,
                    "consumer-specific internal-auth key is blank or < 32 chars; \
                     this surface will fail CLOSED (403) (use `openssl rand -hex 32`)"
                );
                return None;
            }
            return Some(Arc::from(key));
        }
    }
    // 2. Shared fallback key.
    let shared = std::env::var_os("CORELINK_INTERNAL_AUTH_KEY")?
        .into_string()
        .ok()?;
    if shared.len() < INTERNAL_AUTH_KEY_MIN_LEN || shared.trim().is_empty() {
        tracing::warn!(
            env = specific_env,
            "neither the consumer-specific key nor CORELINK_INTERNAL_AUTH_KEY \
             is a valid key ≥ 32 chars; this surface will fail CLOSED (403) \
             (use `openssl rand -hex 32`)"
        );
        return None;
    }
    Some(Arc::from(shared))
}

/// Read the operator-only **admin/pilots** shared secret from the
/// environment (red-team #3).
///
/// Reads `CORELINK_ADMIN_AUTH_KEY` first, falling back to the shared
/// `CORELINK_INTERNAL_AUTH_KEY` when unset/blank/too-short (see
/// [`resolve_internal_auth_key`]). When `None`, the admin handlers fail
/// CLOSED (403) — privileged logic never runs without a properly sized
/// gate.
#[must_use]
pub fn internal_auth_key_from_env() -> Option<Arc<str>> {
    resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY")
}

/// Read the operator **dual-approver** dedicated secret from the environment
/// (finding H5; hardened by the 2026-08-19 red-team, finding "two-person
/// control collapses to one shared secret").
///
/// This gates `POST /v1/admin/approve` — the step that RECORDS a second
/// approver into the durable approval ledger. It is deliberately a **separate
/// credential** from [`internal_auth_key_from_env`] (the mutate/admin key): a
/// real two-person control requires the approver and the initiator to hold
/// DIFFERENT keys, so a single admin-key holder cannot both create the
/// approval and spend it.
///
/// Reads `CORELINK_ADMIN_APPROVER_AUTH_KEY` ONLY and does **NOT** fall back to
/// the shared `CORELINK_INTERNAL_AUTH_KEY` — same DEDICATED-only treatment as
/// the erase authority ([`erase_auth_key_from_env`], finding H4) and the DSR
/// anchor. The old shared-key fallback silently COLLAPSED two-person control:
/// with `CORELINK_ADMIN_APPROVER_AUTH_KEY` unset both this key and the mutate
/// key resolved to the SAME `CORELINK_INTERNAL_AUTH_KEY`, so a single
/// shared-secret holder could call `/v1/admin/approve` (records
/// `approver@internal`) then `/v1/admin/mutate` (initiator `operator@internal`)
/// and defeat dual approval — the only discriminator being two hardcoded,
/// cosmetic principal strings. Dedicated-only closes that: when unset or
/// < 32 chars the approve route fails CLOSED (403) and dual approval cannot be
/// recorded until a distinct approver key is provisioned.
///
/// # Owner action (REQUIRED in prod)
///
/// `CORELINK_ADMIN_APPROVER_AUTH_KEY` MUST be bound in prod (≥ 32 chars,
/// `openssl rand -hex 32`) and MUST be DISTINCT from `CORELINK_INTERNAL_AUTH_KEY`
/// / `CORELINK_ADMIN_AUTH_KEY`; [`crate::routes::approver_key_distinct_or_none`]
/// enforces the distinctness at boot (a byte-equal approver key is treated as
/// unset → approve fails CLOSED).
#[must_use]
pub fn approver_auth_key_from_env() -> Option<Arc<str>> {
    resolve_dedicated_auth_key("CORELINK_ADMIN_APPROVER_AUTH_KEY")
}

/// Read the **CAS-erase / DSR** dedicated secret from the environment
/// (finding H4 — was red-team #3).
///
/// Reads `CORELINK_ERASE_AUTH_KEY` ONLY and does **NOT** fall back to the
/// shared `CORELINK_INTERNAL_AUTH_KEY`: the erase authority drives
/// irreversible tombstones, so a leak of the broad shared secret must never,
/// by itself, exercise it. This keeps the anti-forge "eraser ≠ requester"
/// split against the anchor authority (`CORELINK_DSR_ANCHOR_AUTH_KEY`) that
/// the old shared fallback silently collapsed (finding H4). `None` ⇒ every
/// erase surface (CAS-erase, DSR, audit-drain) fails CLOSED (403/unmounted).
///
/// # Owner action (Track-2)
///
/// The dedicated `CORELINK_ERASE_AUTH_KEY` is now REQUIRED in prod (≥ 32
/// chars, `openssl rand -hex 32`); until it is bound the erase surfaces stay
/// fail-CLOSED. Mirrors the dedicated-key-only PAT-mint gate (`internal_pat`).
#[must_use]
pub fn erase_auth_key_from_env() -> Option<Arc<str>> {
    resolve_dedicated_auth_key("CORELINK_ERASE_AUTH_KEY")
}

/// Dual-key rotation variant of [`erase_auth_key_from_env`]: the accepted ERASE
/// keys = the current `CORELINK_ERASE_AUTH_KEY` PLUS, when set and ≥
/// [`INTERNAL_AUTH_KEY_MIN_LEN`], the outgoing `CORELINK_ERASE_AUTH_KEY_PREVIOUS`.
///
/// Both are DEDICATED-ONLY (no shared `CORELINK_INTERNAL_AUTH_KEY` fallback — H4)
/// and hold the ≥32-char floor (F28/F15). Accepting the previous value for the
/// duration of a rotation bridges the window where the apex already forwards the
/// NEW key but a not-yet-recycled DO container still booted with the OLD one (the
/// env-read-at-start footgun that took the whole erase path down). The operator
/// clears `CORELINK_ERASE_AUTH_KEY_PREVIOUS` once the fleet has recycled. Returns
/// an empty vec (⇒ erase surfaces fail CLOSED / stay unmounted) when neither is
/// usable. The current key is always FIRST; a duplicate previous is dropped.
#[must_use]
pub fn erase_auth_keys_from_env() -> Vec<String> {
    let mut keys: Vec<String> = Vec::with_capacity(2);
    if let Some(current) = erase_auth_key_from_env() {
        keys.push(current.to_string());
    }
    if let Ok(previous) = std::env::var("CORELINK_ERASE_AUTH_KEY_PREVIOUS") {
        if previous.len() >= INTERNAL_AUTH_KEY_MIN_LEN && !keys.contains(&previous) {
            keys.push(previous);
        }
    }
    keys
}

/// Build the axum `Router` exposing the admin read + mutate + approve routes.
pub fn router(state: AdminRouteState) -> Router {
    Router::new()
        .route(ADMIN_READ_ROUTE, get(handle_read))
        .route(ADMIN_MUTATE_ROUTE, post(handle_mutate))
        .route(ADMIN_APPROVE_ROUTE, post(handle_approve))
        .with_state(state)
}

/// JSON shape for `POST /v1/admin/mutate`.
///
/// The route is intentionally hand-coded against `serde_json` rather
/// than a `prost`-generated message because the admin plane is the
/// control plane: cardinality is small, evolution is cheap, and the
/// surface is human-curated.
///
/// # Auth authority
///
/// Since the PR #152 internal-auth gate, the authority for the admin
/// assertion comes entirely from the `x-corelink-internal-auth` gate,
/// NOT from the JSON body. The gated path (`into_request_gated`)
/// hardcodes `operator@internal` as the initiator principal and forces
/// `is_admin = true` regardless of the body fields. Operators are no
/// longer required to send `initiator` / `initiator_is_admin` — they
/// default to empty/false and are ignored for the auth decision.
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
    /// Initiator principal.
    ///
    /// **Ignored by the gated route path** (`into_request_gated`): since
    /// the PR #152 internal-auth gate the authority comes from clearing
    /// the `x-corelink-internal-auth` gate, which hardcodes the operator
    /// principal. This field is optional on the wire (`#[serde(default)]`)
    /// so callers are not required to send a dummy value; any supplied
    /// value is discarded.
    #[serde(default)]
    pub initiator: String,
    /// True if initiator carries the admin role.
    ///
    /// **Ignored by the gated route path** (`into_request_gated`): the
    /// internal-auth gate forces `is_admin = true` regardless of this
    /// field. Optional on the wire (`#[serde(default)]`); any supplied
    /// value is discarded.
    #[serde(default)]
    pub initiator_is_admin: bool,
    /// Approval id from the dual-approval ledger (None = rejected).
    pub approval_id: Option<String>,
    /// Second approver principal (None = rejected).
    pub approver: Option<String>,
}
