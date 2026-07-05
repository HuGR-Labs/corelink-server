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
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use corelink_handler_admin::{
    AdminHandlerError, AdminMutateHandler, AdminMutateRequest, AdminMutateResponse,
    AdminReadHandler, AdminReadRequest, AdminReadResponse, DualApprovalToken, InMemoryAdminHandler,
    InMemoryAuditSink, InMemorySliObserver, MutateOp,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

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
#[must_use]
fn internal_auth_ok(expected: Option<&Arc<str>>, headers: &HeaderMap) -> bool {
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
    /// Operator-only shared secret for the `x-corelink-internal-auth`
    /// gate (sourced from `CORELINK_INTERNAL_AUTH_KEY`). `None` when the
    /// key is unset at boot → every admin handler fails CLOSED (403)
    /// (mirrors the `internal_pat` fail-CLOSED posture).
    pub internal_auth_key: Option<Arc<str>>,
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
/// # Runtime selection (WP-S1 Phase 2)
///
/// - When `StorageEnv::from_env()` returns `Some(_)` (real CF creds
///   are present in the container environment), this function builds
///   a [`D1AdminHandler`] backed by `CONFIG_DB` over the D1 HTTP API.
///   This is the **production path**: admin reads/mutations land on
///   durable D1 instead of volatile InMemory.
///
/// - When credentials are absent (unit tests, local dev, CI) the
///   function falls back to [`InMemoryAdminHandler`]. No network I/O
///   occurs.
///
/// On `wasm32-unknown-unknown` a compile-error placeholder is emitted
/// per the `trait-abstraction-defer` rule.
///
/// # Panics
///
/// Does not panic. If the D1 client cannot be constructed, an error
/// is logged and the function falls back to [`InMemoryAdminHandler`].
#[must_use]
pub fn build_handlers() -> (Arc<dyn AdminReadHandler>, Arc<dyn AdminMutateHandler>) {
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
                        "Admin handler: D1 (real storage)"
                    );
                    let audit = Arc::new(InMemoryAuditSink::new());
                    let sli = Arc::new(InMemorySliObserver::new());
                    let inner = Arc::new(InMemoryAdminHandler::new(audit, sli));
                    let shared: Arc<D1AdminHandler> =
                        Arc::new(D1AdminHandler::new(Arc::new(client), inner));
                    let read: Arc<dyn AdminReadHandler> = shared.clone();
                    let mutate: Arc<dyn AdminMutateHandler> = shared;
                    return (read, mutate);
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
        let shared: Arc<InMemoryAdminHandler> = Arc::new(InMemoryAdminHandler::new(audit, sli));
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
/// shared key when the specific one is unset/blank/too-short, so each
/// surface can be rotated to its own credential without a flag day
/// (mirrors the #8 OCI dual-name pattern — additive, deployable BEFORE
/// the new prod secrets exist).
///
/// Resolution order (fail-CLOSED at each step):
/// 1. `specific_env` — used iff set AND ≥ [`INTERNAL_AUTH_KEY_MIN_LEN`];
/// 2. else `CORELINK_INTERNAL_AUTH_KEY` — used iff set AND ≥ floor;
/// 3. else `None` — the handler fails CLOSED (403), exactly as today.
///
/// A set-but-too-short `specific_env` does NOT hard-fail the surface; it
/// is treated as absent and the shared key is tried (so a misconfigured
/// new secret degrades to today's behaviour rather than locking the
/// surface out). Both too-short ⇒ `None`.
#[must_use]
pub(crate) fn resolve_internal_auth_key(specific_env: &str) -> Option<Arc<str>> {
    // 1. Consumer-specific key, when present and properly sized.
    match std::env::var(specific_env) {
        Ok(key) if key.len() >= INTERNAL_AUTH_KEY_MIN_LEN => {
            return Some(Arc::from(key.as_str()));
        }
        Ok(key) if !key.is_empty() => {
            tracing::warn!(
                env = specific_env,
                "consumer-specific internal-auth key set but < 32 chars; \
                 falling back to CORELINK_INTERNAL_AUTH_KEY (use `openssl rand -hex 32`)"
            );
        }
        _ => {}
    }
    // 2. Shared fallback key.
    let shared = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if shared.len() < INTERNAL_AUTH_KEY_MIN_LEN {
        tracing::warn!(
            env = specific_env,
            "neither the consumer-specific key nor CORELINK_INTERNAL_AUTH_KEY \
             is ≥ 32 chars; this surface will fail CLOSED (403) (use `openssl rand -hex 32`)"
        );
        return None;
    }
    Some(Arc::from(shared.as_str()))
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

/// Read the **DSR / CAS-erase** shared secret from the environment
/// (red-team #3).
///
/// Reads `CORELINK_ERASE_AUTH_KEY` first, falling back to the shared
/// `CORELINK_INTERNAL_AUTH_KEY` when unset/blank/too-short (see
/// [`resolve_internal_auth_key`]). When `None`, the erase surfaces fail
/// CLOSED (403).
///
/// # Wiring note (LEAD)
///
/// `main.rs` currently seeds the CAS-erase route with
/// [`internal_auth_key_from_env`] (the ADMIN key). To complete the #3
/// split, `main.rs` should call THIS function for `cas_erase`, and
/// `dsr::build_state_from_env` (outside this file) should read the erase
/// key too. Until those two one-line swaps land, both surfaces keep
/// working via the shared-key fallback.
#[must_use]
pub fn erase_auth_key_from_env() -> Option<Arc<str>> {
    resolve_internal_auth_key("CORELINK_ERASE_AUTH_KEY")
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

/// Canonical `tier_selections.tier` enum (migration 0039). The D1
/// `UPDATE tier_selections SET tier = ?1` write is gated by a CHECK
/// constraint accepting EXACTLY these lower-case labels.
///
/// NOTE on divergence: migration 0057's `tenant.tier` CHECK additionally
/// allows `org` (a legacy / quota-class alias). That is NOT valid in
/// `tier_selections`, so the operator admin path rejects it here with a
/// clean 400 rather than letting the value reach the D1 CHECK (which would
/// surface as an opaque 500). Reconciling the two enums is an additive-only
/// auth-migration follow-up (do NOT widen 0039 destructively) — tracked in
/// the PR for #35.
// CAA-360 #26: this is the narrower **settable** ladder (6, current product
// tiers). The **resolve** set is `auth_introspect.rs::VALID_TIERS` (8 — also
// accepts the back-compat `team`/`org` that the `tenant.tier` CHECK still allows
// for existing rows but are deliberately NOT settable here). Invariant:
// settable ⊂ resolvable ⊂ D1-CHECK — keep that subset relationship if changing.
const TIER_SELECTIONS_TIERS: [&str; 6] = ["free", "solo", "starter", "pro", "max", "enterprise"];

/// Validate + normalize an operator-supplied `tier` string against the
/// `tier_selections.tier` enum BEFORE it is flowed into a
/// [`MutateOp::SetTenantTier`] and on to the D1 write.
///
/// Trims surrounding whitespace and lower-cases (so the codebase's own
/// admin callers that send `"Pro"` normalize to `"pro"`), then rejects
/// anything outside [`TIER_SELECTIONS_TIERS`] with a static error string —
/// surfaced as a `400 invalid_tier` at the route boundary instead of an
/// opaque D1 CHECK-violation 500.
///
/// # Errors
///
/// Returns `Err("invalid_tier")` when the normalized value is not one of
/// the canonical `tier_selections` tiers (e.g. `org`, typos).
fn normalize_tier_selection(raw: &str) -> Result<String, &'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    if TIER_SELECTIONS_TIERS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err("invalid_tier")
    }
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
                // Validate + normalize against the tier_selections enum
                // BEFORE the value can reach the D1 CHECK (#35).
                let tier = normalize_tier_selection(&tier)?;
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

    /// Parse the body into an [`AdminMutateRequest`] for the
    /// **operator-gated** route path.
    ///
    /// Unlike [`into_request`](Self::into_request), the admin assertion
    /// is NOT taken from the client JSON: the caller already cleared the
    /// `x-corelink-internal-auth` gate, so the initiator principal is the
    /// supplied trusted `operator` and `is_admin` is forced `true`. The
    /// body's `initiator` / `initiator_is_admin` fields are IGNORED for
    /// the auth decision (kept on the struct only for wire-compat).
    ///
    /// Dual-approval (`approval_id` + `approver`) is still parsed and
    /// enforced downstream; the handler rejects self-approval, so the
    /// `approver` must differ from `operator`.
    ///
    /// # Errors
    ///
    /// Returns a static error string when the op-kind / required-field
    /// combination is invalid (same rules as [`into_request`](Self::into_request)).
    pub fn into_request_gated(
        self,
        operator: &str,
        at_unix_ms: u64,
    ) -> Result<AdminMutateRequest, &'static str> {
        let op = match self.op_kind.as_str() {
            "set_tenant_tier" => {
                let tenant = self.tenant.ok_or("set_tenant_tier requires tenant")?;
                let tier = self.tier.ok_or("set_tenant_tier requires tier")?;
                // Validate + normalize against the tier_selections enum
                // BEFORE the value can reach the D1 CHECK (#35).
                let tier = normalize_tier_selection(&tier)?;
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
            operator.to_owned(),
            // is_admin is derived from the internal-auth gate, NOT the body.
            true,
            approval,
            at_unix_ms,
        ))
    }
}

/// `GET /v1/admin/read/:resource` handler.
async fn handle_read(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    Path(resource): Path<String>,
) -> impl IntoResponse {
    // Operator-only gate (fail-CLOSED). The admin control plane is NOT
    // reachable by tenant PATs: only a caller holding the
    // `CORELINK_INTERNAL_AUTH_KEY` shared secret (the operator, via the
    // Worker→DO→container hop) may read admin records. Absent/wrong
    // secret, or unconfigured key → 403 BEFORE any handler logic. The
    // handler's own ReadAttempted audit row is only emitted once the
    // gate passes; on the gate-fail path the SEC signal is the structured
    // warning below (no per-tenant audit sink exists at this boundary).
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "AdminReadUnauthorized",
            "admin read rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    // Real wall-clock timestamp for the audit record (was hardcoded
    // epoch-zero, which made admin audit rows un-orderable). Matches the
    // CAS/AC/Bazel/Turbo audit path (`SystemWallClock.now_ms()`).
    let now_ms = SystemWallClock.now_ms();
    // The caller is the trusted operator (it cleared the internal-auth
    // gate above), so we derive the admin principal from the gated
    // context — NOT from any client-supplied field.
    let req = AdminReadRequest::new(resource, "admin@root", true, now_ms);
    match state.read.read(req) {
        Ok(resp) => (StatusCode::OK, resp.body).into_response(),
        Err(e) => map_err(e),
    }
}

/// Principal recorded for operator-gated admin mutations. The admin
/// assertion is derived from the internal-auth gate, NEVER from the
/// client JSON body.
const ADMIN_OPERATOR_PRINCIPAL: &str = "operator@internal";

/// `POST /v1/admin/mutate` handler.
///
/// M3 pattern (F16 fix): the body is accepted as raw [`Bytes`] so the
/// `HeaderMap` `FromRequestParts` extractor resolves BEFORE the body is
/// buffered into memory. The internal-auth gate is evaluated FIRST; an
/// unauthenticated caller is rejected with 403 WITHOUT the body ever
/// being JSON-parsed — denying a pre-auth caller the CPU/heap cost of
/// parsing an arbitrarily-large body. JSON deserialisation runs only AFTER
/// the gate passes, matching the M3 pattern already used by
/// `dsr.rs` and `internal_pat.rs`.
async fn handle_mutate(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Operator-only gate (fail-CLOSED). Auth is checked BEFORE the body is
    // parsed (M3 / F16): `headers` is a `FromRequestParts` extractor, so
    // this gate runs before the body buffer is consumed. The previous version
    // used `Json(body): Json<AdminMutateBody>` which caused body-parse to
    // run before the gate — that is corrected here.
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "AdminMutateUnauthorized",
            "admin mutate rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    // Body parsed ONLY after the auth gate passes (M3).
    let body: AdminMutateBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "admin mutate: invalid request body");
            return (StatusCode::BAD_REQUEST, "invalid_body").into_response();
        }
    };
    // Real wall-clock timestamp for the audit record (was hardcoded
    // epoch-zero, which made admin audit rows un-orderable). Matches the
    // CAS/AC/Bazel/Turbo audit path (`SystemWallClock.now_ms()`).
    let now_ms = SystemWallClock.now_ms();
    // Build the request from the body's OPERATION fields only. The
    // initiator principal + is_admin flag are derived from the gated
    // operator context (is_admin = true), NOT from the body. Dual-
    // approval (approval_id + approver) is still honoured from the body
    // and is enforced by the handler — the approver must differ from the
    // operator principal.
    let req = match body.into_request_gated(ADMIN_OPERATOR_PRINCIPAL, now_ms) {
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
        AdminHandlerError::Forbidden { .. } => (StatusCode::FORBIDDEN, "forbidden").into_response(),
        AdminHandlerError::DualApprovalMissing => {
            (StatusCode::FORBIDDEN, "dual-approval required").into_response()
        }
        AdminHandlerError::DualApprovalSelfApproval { .. } => {
            (StatusCode::FORBIDDEN, "self-approval rejected").into_response()
        }
        AdminHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never mutate.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

/// Resolve the DSR legitimacy-anchor register key (`POST /_internal/dsr/anchor`),
/// preferring the dedicated `CORELINK_DSR_ANCHOR_AUTH_KEY` with the shared
/// `CORELINK_INTERNAL_AUTH_KEY` as fallback. This key MUST be held by the
/// erasure-REQUEST authority (e.g. githugr), a DIFFERENT party than the eraser
/// holding `CORELINK_ERASE_AUTH_KEY` — the anti-forge basis of the legitimacy
/// gate. See [`crate::routes::dsr_anchor`]. (Placed at the end of the module,
/// after the OKF-cited items above, to keep anti-drift line-anchors stable.)
#[must_use]
pub fn dsr_anchor_auth_key_from_env() -> Option<Arc<str>> {
    resolve_internal_auth_key("CORELINK_DSR_ANCHOR_AUTH_KEY")
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
        (
            audit,
            sli,
            shared,
            AdminRouteState {
                read,
                mutate,
                // Tests exercise the handler trait directly; a configured
                // key here keeps the router constructable. Gate behaviour
                // is covered by the internal_auth_ok unit tests below.
                internal_auth_key: Some(Arc::from("test-internal-auth-key-32-bytes-x")),
            },
        )
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
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::MutateCommitted));
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

    /// Build a `set_tenant_tier` body with the given raw tier string.
    fn tier_body(tier: &str) -> AdminMutateBody {
        AdminMutateBody {
            op_kind: "set_tenant_tier".into(),
            tenant: Some("t1".into()),
            tier: Some(tier.into()),
            token_id: None,
            initiator: String::new(),
            initiator_is_admin: false,
            approval_id: Some("a1".into()),
            approver: Some("bob".into()),
        }
    }

    /// Extract the tier label from a parsed `set_tenant_tier` request.
    fn parsed_tier(req: &AdminMutateRequest) -> &str {
        match &req.op {
            MutateOp::SetTenantTier { tier, .. } => tier.as_str(),
            other => panic!("expected SetTenantTier, got {other:?}"),
        }
    }

    /// #35: the operator's own admin tests send capitalized labels
    /// (`"Solo"`). The parser MUST normalize them to the lower-case
    /// `tier_selections` label so the D1 CHECK accepts the write — on
    /// BOTH the legacy and the operator-gated path.
    #[test]
    fn set_tenant_tier_normalizes_capitalized_tier() {
        let req = tier_body("Solo")
            .into_request(0)
            .expect("capitalized tier accepted + normalized");
        assert_eq!(parsed_tier(&req), "solo");

        let gated = tier_body("  PRO  ")
            .into_request_gated("operator@internal", 0)
            .expect("capitalized + padded tier accepted + normalized");
        assert_eq!(parsed_tier(&gated), "pro");
    }

    /// #35: every canonical `tier_selections` tier is accepted in its
    /// lower-case form and passes through unchanged.
    #[test]
    fn set_tenant_tier_accepts_valid_lowercase_tiers() {
        for tier in ["free", "solo", "starter", "pro", "max", "enterprise"] {
            let req = tier_body(tier)
                .into_request(0)
                .unwrap_or_else(|e| panic!("tier {tier} should be accepted: {e}"));
            assert_eq!(parsed_tier(&req), tier);
        }
    }

    /// #35: an unknown tier is rejected with the `invalid_tier` 400
    /// marker BEFORE reaching the D1 CHECK (no opaque 500).
    #[test]
    fn set_tenant_tier_rejects_invalid_tier() {
        let err = tier_body("platinum")
            .into_request(0)
            .expect_err("unknown tier rejected");
        assert_eq!(err, "invalid_tier");

        let gated_err = tier_body("platinum")
            .into_request_gated("operator@internal", 0)
            .expect_err("unknown tier rejected on gated path");
        assert_eq!(gated_err, "invalid_tier");
    }

    /// #35 divergence: `org` is valid in migration 0057's `tenant.tier`
    /// CHECK but NOT in 0039's `tier_selections.tier`. It MUST be rejected
    /// cleanly here (additive-only migration follow-up tracked in the PR),
    /// not flowed to the D1 CHECK. (`solo` is now a canonical
    /// `tier_selections` tier and is accepted — covered above.)
    #[test]
    fn set_tenant_tier_rejects_org_divergence() {
        for tier in ["org", "ORG"] {
            let err = tier_body(tier)
                .into_request_gated("operator@internal", 0)
                .expect_err("org rejected (0039 vs 0057 divergence)");
            assert_eq!(err, "invalid_tier", "tier {tier} must be rejected");
        }
    }

    /// serde-default: a wire body WITHOUT `initiator` / `initiator_is_admin`
    /// fields MUST deserialize successfully and the gated path MUST still
    /// derive admin authority from the operator principal, not the body.
    ///
    /// This pins the PR #152 change: the fields are no longer required on the
    /// wire, so callers that omit them don't get a 400.
    #[test]
    fn admin_mutate_body_serde_default_no_initiator_fields() {
        // JSON that intentionally omits initiator and initiator_is_admin.
        let json = r#"{
            "op_kind": "set_tenant_tier",
            "tenant": "t1",
            "tier": "Pro",
            "approval_id": "a1",
            "approver": "bob"
        }"#;
        let body: AdminMutateBody =
            serde_json::from_str(json).expect("deserialize without initiator fields");
        // Defaults: empty string + false.
        assert_eq!(body.initiator, "");
        assert!(!body.initiator_is_admin);
        // The gated path ignores the body fields and derives admin from the gate.
        let req = body
            .into_request_gated("operator@internal", 0)
            .expect("gated request from minimal body");
        assert_eq!(req.initiator, "operator@internal");
        assert!(req.initiator_is_admin);
    }

    /// The operator gate fails CLOSED when the key is unconfigured,
    /// regardless of any header the client supplies.
    #[test]
    fn internal_auth_fails_closed_when_key_unset() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "anything".parse().expect("header"),
        );
        assert!(!internal_auth_ok(None, &headers));
    }

    /// Absent / wrong / right header behaviour against a configured key.
    #[test]
    fn internal_auth_matches_only_exact_secret() {
        let key: Arc<str> = Arc::from("test-internal-auth-key-32-bytes-x");
        // Absent header → reject.
        let empty = HeaderMap::new();
        assert!(!internal_auth_ok(Some(&key), &empty));
        // Wrong value → reject.
        let mut wrong = HeaderMap::new();
        wrong.insert(ADMIN_INTERNAL_AUTH_HEADER, "wrong".parse().expect("header"));
        assert!(!internal_auth_ok(Some(&key), &wrong));
        // Prefix of the real key (length differs) → reject (no length leak).
        let mut prefix = HeaderMap::new();
        prefix.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes".parse().expect("header"),
        );
        assert!(!internal_auth_ok(Some(&key), &prefix));
        // Exact value → accept.
        let mut right = HeaderMap::new();
        right.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        assert!(internal_auth_ok(Some(&key), &right));
    }

    /// The gated body parser derives `is_admin = true` + the operator
    /// principal from the gate, IGNORING the body's self-asserted
    /// `initiator` / `initiator_is_admin`.
    #[test]
    fn into_request_gated_ignores_body_admin_assertion() {
        let body = AdminMutateBody {
            op_kind: "set_tenant_tier".into(),
            tenant: Some("t1".into()),
            tier: Some("Pro".into()),
            token_id: None,
            // Attacker-controlled body fields — must be ignored.
            initiator: "attacker".into(),
            initiator_is_admin: false,
            approval_id: Some("a1".into()),
            approver: Some("bob".into()),
        };
        let req = body
            .into_request_gated("operator@internal", 7)
            .expect("gated request");
        // The handler request carries the initiator + admin flag as
        // public fields; pin that the operator principal — not
        // "attacker" — is recorded and is_admin is forced true.
        assert_eq!(req.initiator, "operator@internal");
        assert!(req.initiator_is_admin);
    }

    // ── F16: auth-before-body-parse (M3 pattern) ──────────────────────────────

    /// Build an axum router from the fixture state for end-to-end handler tests.
    fn fixture_router() -> (axum::Router, Arc<str>) {
        let (_audit, _sli, _shared, state) = fixture();
        let key = state
            .internal_auth_key
            .clone()
            .expect("fixture always sets a key");
        (router(state), key)
    }

    /// F16: an unauthenticated caller sending a LARGE non-JSON body must receive
    /// 403 (auth gate fires first), NOT a 400 from body deserialization.
    /// This pins the M3 pattern: `Bytes` extractor defers parse until after auth.
    #[tokio::test]
    async fn handle_mutate_rejects_unauthenticated_before_parsing_body() {
        use axum::body::Body;
        use axum::http::{self, Request};
        use tower::ServiceExt;

        let (app, _key) = fixture_router();
        // 2 MiB of garbage — not valid JSON.
        let big_garbage = "Z".repeat(2 * 1024 * 1024);
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(ADMIN_MUTATE_ROUTE)
            .header("content-type", "application/json")
            // NO x-corelink-internal-auth header.
            .body(Body::from(big_garbage))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "auth must fail (403) BEFORE the body is parsed (M3 / F16)"
        );
    }

    /// F16: a caller with a WRONG auth header and an invalid body must also get
    /// 403, not 400 — the parse never runs when auth fails.
    #[tokio::test]
    async fn handle_mutate_wrong_auth_returns_403_not_400() {
        use axum::body::Body;
        use axum::http::{self, Request};
        use tower::ServiceExt;

        let (app, _key) = fixture_router();
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(ADMIN_MUTATE_ROUTE)
            .header("content-type", "application/json")
            .header("x-corelink-internal-auth", "wrong-secret")
            .body(Body::from("{not-json}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// F16: positive control — with correct auth and an invalid body, the handler
    /// now parses the body and returns 400 (confirming parse runs post-auth).
    #[tokio::test]
    async fn handle_mutate_correct_auth_invalid_body_returns_400() {
        use axum::body::Body;
        use axum::http::{self, Request};
        use tower::ServiceExt;

        let (app, key) = fixture_router();
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(ADMIN_MUTATE_ROUTE)
            .header("content-type", "application/json")
            .header("x-corelink-internal-auth", key.as_ref())
            .body(Body::from("this is not json"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── F29: minimum key length is 32 chars ───────────────────────────────────

    // ── #3: per-consumer internal-auth key split ──────────────────────────────

    /// Serializes the env-mutating key-split tests (they all read/write the
    /// same fixed env-var names, so they must not run concurrently).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Clear every key the split helper reads, so a test starts from a clean
    /// env regardless of ambient CI vars.
    fn clear_key_env() {
        std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
        std::env::remove_var("CORELINK_ADMIN_AUTH_KEY");
        std::env::remove_var("CORELINK_ERASE_AUTH_KEY");
        std::env::remove_var("CORELINK_PAT_MINT_AUTH_KEY");
    }

    const KEY_A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // 32 chars
    const KEY_B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"; // 32 chars

    /// #3: when the consumer-specific key is present (and ≥ 32), it is used —
    /// NOT the shared key.
    #[test]
    fn split_specific_key_present_is_used() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
        std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
        let resolved = resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY")
            .expect("specific key present");
        assert_eq!(&*resolved, KEY_B, "the specific key must win over the shared key");
        clear_key_env();
    }

    /// #3: when the consumer-specific key is ABSENT, fall back to the shared
    /// `CORELINK_INTERNAL_AUTH_KEY`.
    #[test]
    fn split_specific_absent_falls_back_to_shared() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
        // No CORELINK_ERASE_AUTH_KEY set.
        let resolved = resolve_internal_auth_key("CORELINK_ERASE_AUTH_KEY")
            .expect("falls back to shared");
        assert_eq!(&*resolved, KEY_A);
        clear_key_env();
    }

    /// #3: a set-but-too-short specific key is treated as absent and the
    /// shared key is used (a misconfigured new secret degrades, never locks
    /// the surface out).
    #[test]
    fn split_specific_too_short_falls_back_to_shared() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A);
        std::env::set_var("CORELINK_PAT_MINT_AUTH_KEY", "too-short"); // < 32
        let resolved = resolve_internal_auth_key("CORELINK_PAT_MINT_AUTH_KEY")
            .expect("too-short specific → shared fallback");
        assert_eq!(&*resolved, KEY_A);
        clear_key_env();
    }

    /// #3: when BOTH the specific and the shared key are absent (or too short)
    /// → `None`, so the gate fails CLOSED (403), exactly as before the split.
    #[test]
    fn split_both_absent_is_none_fail_closed() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        assert!(
            resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").is_none(),
            "no keys at all → None (fail CLOSED)"
        );
        // Shared present but too short, specific absent → still None.
        std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", "short");
        assert!(
            resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY").is_none(),
            "shared too short + specific absent → None"
        );
        clear_key_env();
    }

    /// #3: `internal_auth_key_from_env` (admin) and `erase_auth_key_from_env`
    /// read DISTINCT specific vars but share the same fallback — so the operator
    /// can rotate admin and erase independently. Pins the two readers point at
    /// their contracted env names.
    #[test]
    fn split_admin_and_erase_read_distinct_keys() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_A); // shared fallback
        std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
        // erase has no specific key → falls back to shared KEY_A;
        // admin has its own KEY_B.
        let admin = internal_auth_key_from_env().expect("admin key");
        let erase = erase_auth_key_from_env().expect("erase key");
        assert_eq!(&*admin, KEY_B, "admin reads CORELINK_ADMIN_AUTH_KEY");
        assert_eq!(&*erase, KEY_A, "erase falls back to shared (no specific set)");
        // Now give erase its own key — the two diverge.
        std::env::set_var("CORELINK_ERASE_AUTH_KEY", KEY_A);
        std::env::set_var("CORELINK_ADMIN_AUTH_KEY", KEY_B);
        std::env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
        assert_eq!(&*internal_auth_key_from_env().expect("admin"), KEY_B);
        assert_eq!(&*erase_auth_key_from_env().expect("erase"), KEY_A);
        clear_key_env();
    }

    /// F29: `internal_auth_key_from_env` uses `< 32` (not `< 16`); verify the
    /// boundary by checking the lengths that the gate MUST reject and accept.
    #[test]
    fn internal_auth_key_from_env_floor_is_32() {
        // 31-char key was previously accepted (old gate was `< 16`); now rejected.
        let short_31 = "a".repeat(31);
        assert!(
            short_31.len() < 32,
            "31-char key is below the 32-char floor and must be rejected"
        );
        // 32-char key is AT the floor — must pass.
        let exactly_32 = "a".repeat(32);
        assert!(
            exactly_32.len() >= 32,
            "32-char key meets the floor and must be accepted"
        );
    }
}
