//! Customer self-serve HTTP routes: `/v1/customer/*`
//!
//! Wires [`corelink_handler_customer`]'s 6 traits + `InMemoryCustomerHandler`
//! into the axum router. The Worker injects `x-corelink-tenant-id` +
//! `x-corelink-token-prefix` headers post-auth; these routes trust those
//! headers exclusively and never accept client-supplied tenant identities.
//!
//! # Endpoints
//!
//! ```text
//! GET  /v1/customer/overview
//! GET  /v1/customer/usage?period=<period>
//! GET  /v1/customer/audit?from=&to=&kind=
//! GET  /v1/customer/billing
//! POST /v1/customer/billing/portal
//! GET  /v1/customer/keys
//! POST /v1/customer/keys              { name, scopes[] }
//! POST /v1/pats                       { label, scopes[] }
//! POST /v1/customer/keys/:pat_id/revoke
//! GET  /v1/customer/team
//! POST /v1/customer/team/invite       { email, role }
//! ```
//!
//! # Auth model
//!
//! All routes require the Worker-injected headers. Absent headers fall back
//! to `"_unknown"` (fail-CLOSED: the handler emits `Unauthorized` on an
//! unknown principal and the route maps it to 401).
//!
//! # Pattern
//!
//! Mirrors [`super::cas`] exactly: one `CustomerRouteState` struct holding
//! trait objects, one `build_handlers()` factory, one `router(state)`.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use corelink_handler_customer::{
    AuditQueryRequest, BillingRequest, CustomerAuditHandler, CustomerBillingHandler,
    CustomerHandlerError, CustomerKeysHandler, CustomerOverviewHandler, CustomerTeamHandler,
    CustomerUsageHandler, InMemoryAuditSink, InMemoryCustomerHandler, InMemorySliObserver,
    KeyCreateRequest, KeyRevokeRequest, KeysListRequest, OverviewRequest, PortalRequest,
    TeamInviteRequest, TeamListRequest, TeamRemoveRequest, UsageRequest,
};
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimitConfig, RateLimitDecision, RateLimiter,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

// ─── Route state ─────────────────────────────────────────────────────────────

/// Shared route state — one trait object per handler surface.
///
/// `InMemoryCustomerHandler` implements all 6 traits so `build_handlers`
/// shares one `Arc` behind all 6 slots. Production can swap each slot
/// independently once a D1-backed impl lands.
#[derive(Clone)]
pub struct CustomerRouteState {
    /// Overview handler.
    pub overview: Arc<dyn CustomerOverviewHandler>,
    /// Usage handler.
    pub usage: Arc<dyn CustomerUsageHandler>,
    /// Billing + portal-url handler.
    pub billing: Arc<dyn CustomerBillingHandler>,
    /// PAT list / create / revoke handler.
    pub keys: Arc<dyn CustomerKeysHandler>,
    /// Team list / invite handler.
    pub team: Arc<dyn CustomerTeamHandler>,
    /// Audit query handler.
    pub audit: Arc<dyn CustomerAuditHandler>,
    /// Optional native PAT possession gate (cycle-2 nuclear red-team, cluster A
    /// — CRITICAL). `Some` in production (`PAT_SIGNING_KEY` + D1) — re-runs the
    /// full Argon2id Option-B verify on the bearer PAT against the claimed
    /// tenant at the TOP of each handler, BEFORE any storage/handler access, so
    /// a leaked `PAT_SIGNING_KEY` cannot HMAC-forge a PAT that reaches the
    /// control plane and mints a genuine `cas:rw` PAT for a victim tenant. The
    /// gate is SKIPPED for Clerk-session callers (already edge-verified — they
    /// carry `x-corelink-token-prefix: clerk` and no bearer). `None` in dev/CI
    /// (skipped — same posture as the native plane). Mirrors
    /// [`super::cas::CasRouteState::pat_gate`]. See [`crate::native_pat_gate`].
    pub pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    /// Self-serve account-deletion requester (C-ACCTDEL): backs
    /// `POST /v1/customer/account/delete`. `Some` in production once the D1
    /// requester + erasure sink are wired (by `routes.rs`, mirroring how
    /// `pat_gate` is wired there); `None` in dev/CI — the route then fails
    /// CLOSED (503) rather than silently acknowledging a GDPR erasure it cannot
    /// honor. See [`AccountDeletionRequester`].
    pub account_deletion: Option<Arc<dyn AccountDeletionRequester>>,
    /// Self-serve tenant bulk-export source (SEAM): backs
    /// `POST /v1/customer/account/export` — the full portability bundle (CAS+AC
    /// blob bytes + RBAC/DPA/audit records) the CLI `corelink tenant export`
    /// needs. `Some` in production once the CAS/AC handlers + D1 row source are
    /// wired (by `routes.rs`, mirroring `account_deletion`); `None` in dev/CI —
    /// the export route then fails CLOSED (503) rather than serve an empty bundle.
    /// See [`crate::routes::customer_export::TenantExportSource`].
    pub export: Option<Arc<dyn crate::routes::customer_export::TenantExportSource>>,
    /// Per-tenant rate limiter for the heavy export route (always present — a
    /// cheap in-process token bucket, burst 2 / 1 per 300s). Mirrors how
    /// `audit_export` self-rate-limits its export in the container.
    pub export_rate_limiter: Arc<dyn corelink_ratelimit::RateLimiter>,
    /// Per-tenant limiter for public PAT issuance (10 requests/hour, burst 10).
    /// This is separate from the generic data-plane limiter and is applied on
    /// both Clerk and canonical-PAT authentication paths.
    pub pat_issue_rate_limiter: Arc<dyn RateLimiter>,
}

impl core::fmt::Debug for CustomerRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CustomerRouteState").finish_non_exhaustive()
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the production `CustomerRouteState` from process env
/// (dashboard revival WP-3). When the D1 config
/// ([`crate::storage::StorageEnv`]) is present, all 6 slots share one
/// [`crate::customer_d1::D1CustomerHandler`] (real dashboard data);
/// otherwise dev/CI falls back to [`build_handlers`]'s
/// `InMemoryCustomerHandler` — mirroring
/// [`crate::adapter_pat::PatVerifier::from_env`]'s fail-closed
/// env-gate pattern.
#[must_use]
pub fn build_handlers_from_env() -> CustomerRouteState {
    match crate::customer_d1::D1CustomerHandler::from_env() {
        Some(shared) => CustomerRouteState {
            overview: shared.clone(),
            usage: shared.clone(),
            billing: shared.clone(),
            keys: shared.clone(),
            team: shared.clone(),
            audit: shared,
            // Wired by `routes.rs` from `native_pat_gate_from_env()` (mirrors
            // the native CAS/AC/Bazel/Turbo states); `None` here so the factory
            // stays env-pure (dev/CI default = skipped).
            pat_gate: None,
            // Wired by `routes.rs` (the erasure sink is a cross-module collaborator
            // built alongside the DSR worker); `None` here keeps the factory
            // env-pure — the account-delete route then fails CLOSED (503).
            account_deletion: None,
            // Wired by `routes.rs` from the CAS/AC handlers + D1; `None` here keeps
            // the factory env-pure — the export route then fails CLOSED (503).
            export: None,
            export_rate_limiter: crate::routes::customer_export::build_export_rate_limiter(),
            pat_issue_rate_limiter: build_pat_issue_rate_limiter(),
        },
        None => {
            tracing::warn!(
                "StorageEnv unset/invalid; /v1/customer/* backed by \
                 InMemoryCustomerHandler (dev/CI mode)"
            );
            build_handlers()
        }
    }
}

/// Build the canonical `CustomerRouteState` using `InMemoryCustomerHandler`
/// (shared behind all 6 trait-object slots via a single `Arc`).
///
/// Dev/CI wiring; production uses [`build_handlers_from_env`] which
/// swaps in the D1-backed handler when the D1 env is present.
#[must_use]
pub fn build_handlers() -> CustomerRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared: Arc<InMemoryCustomerHandler> = Arc::new(InMemoryCustomerHandler::new(audit, sli));
    CustomerRouteState {
        overview: shared.clone(),
        usage: shared.clone(),
        billing: shared.clone(),
        keys: shared.clone(),
        team: shared.clone(),
        audit: shared,
        // Dev/CI default = skipped; `routes.rs` overwrites with the env gate.
        pat_gate: None,
        // Dev/CI default = unwired; the account-delete route fails CLOSED (503).
        account_deletion: None,
        // Dev/CI default = unwired; the export route fails CLOSED (503).
        export: None,
        export_rate_limiter: crate::routes::customer_export::build_export_rate_limiter(),
        pat_issue_rate_limiter: build_pat_issue_rate_limiter(),
    }
}

/// Stable endpoint identity for the public self-serve mint bucket.
pub const PAT_ISSUE_ENDPOINT_ID: &str = "pat-issue";

/// Public PAT issuance is deliberately much slower than the generic data
/// plane: ten tokens per tenant per hour, with a ten-request initial burst.
/// The exact ratio preserves the one-token-per-360-second cadence without a
/// fixed-point truncation that would incorrectly defer the boundary to 361s.
#[must_use]
pub fn pat_issue_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig::with_fractional_refill_ratio(10, 3600, 10, 1, 86_400, 7 * 86_400)
        .unwrap_or_else(RateLimitConfig::canonical)
}

/// Build the per-tenant public PAT issuance limiter. The in-memory
/// implementation is scoped to the customer Durable Object instance; the
/// composite bucket key still includes the tenant and endpoint so tenants
/// cannot consume one another's allowance.
#[must_use]
pub fn build_pat_issue_rate_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(InMemoryTokenBucketRateLimiter::new(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        pat_issue_rate_limit_config(),
    ))
}

/// Build the production self-serve account-deletion requester: a D1 row source
/// ([`crate::customer_d1::D1HttpCustomerDb`]) + the in-process DSR erasure sink
/// ([`crate::routes::dsr::build_in_process_erasure_sink`] — the SAME worker the
/// Clerk-webhook consumer runs). `None` when the storage/erase env is
/// unconfigured (dev/CI) → the `POST /v1/customer/account/delete` route then
/// fails CLOSED (503), never a silently-unhonored GDPR erasure. Wired by
/// `routes.rs` (the sink is a cross-module collaborator built alongside the DSR
/// worker), mirroring `native_pat_gate_from_env` → `pat_gate`.
#[must_use]
pub fn account_deletion_from_env() -> Option<Arc<dyn AccountDeletionRequester>> {
    let db = crate::customer_d1::D1HttpCustomerDb::from_env()?;
    let sink = crate::routes::dsr::build_in_process_erasure_sink()?;
    Some(Arc::new(D1AccountDeletionRequester::new(
        Arc::new(db),
        sink,
    )))
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all `/v1/customer/*` routes.
pub fn router(state: CustomerRouteState) -> Router {
    Router::new()
        .route("/v1/customer/overview", get(handle_overview))
        .route("/v1/customer/usage", get(handle_usage))
        .route("/v1/customer/audit", get(handle_audit))
        .route("/v1/customer/billing", get(handle_billing))
        .route("/v1/customer/billing/portal", post(handle_billing_portal))
        .route(
            "/v1/customer/keys",
            get(handle_keys_list).post(handle_keys_create),
        )
        // Public self-serve PAT issuance is an alias over the same trusted
        // customer-plane state as the dashboard key form.  In particular, it
        // does NOT use the operator-only `/_internal/pat/mint` authority.
        .route("/v1/pats", post(handle_pats_create))
        .route(
            "/v1/customer/keys/{pat_id}/revoke",
            post(handle_keys_revoke),
        )
        .route("/v1/customer/team", get(handle_team_list))
        .route("/v1/customer/team/invite", post(handle_team_invite))
        .route("/v1/customer/team/{user_id}", delete(handle_team_remove))
        .route("/v1/customer/account/delete", post(handle_account_delete))
        .route("/v1/customer/account/export", post(handle_account_export))
        .with_state(state)
}

// ─── Header helpers ───────────────────────────────────────────────────────────

/// Read `name` from `headers`; return `default` when absent or non-ASCII.
fn header_or(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `auth_tenant::AuthTenant`'s sentinel set.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// Read the authenticated `x-corelink-tenant-id`, **fail-CLOSED**.
///
/// F-defense: the previous version fell back to the `"_unknown"` sentinel on a
/// missing/empty header, so an unauthenticated request silently flowed into the
/// handler under a sentinel tenant (every customer surface reads/writes that
/// tenant's billing/keys/team data). We now mirror `auth_tenant::AuthTenant`:
/// a missing/empty/sentinel value is an `Err(())` the handler maps to `401`.
fn tenant(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers
        .get("x-corelink-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
        return Err(());
    }
    Ok(raw.to_owned())
}

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// Read `x-corelink-token-prefix`; fail-CLOSED to `"_unknown"`.
fn principal(headers: &HeaderMap) -> String {
    header_or(headers, "x-corelink-token-prefix", "_unknown")
}

/// The `x-corelink-token-prefix` value the Worker stamps for a Clerk-session
/// caller on the customer plane (see `worker/src/index.ts` `customer_v1` arm,
/// `h.set("x-corelink-token-prefix", "clerk")`). A Clerk caller was already
/// edge-verified (the Worker validated the session JWT + resolved the tenant
/// from D1) and carries NO bearer PAT — the Worker `delete`s `authorization`
/// before forwarding — so the PAT Argon2id backstop is not applicable to it and
/// is skipped (mirrors how the native plane only runs the gate on the PAT path).
const CLERK_TOKEN_PREFIX: &str = "clerk";

/// The server-trusted team-RBAC role header the Worker forwards on the customer
/// plane (the D1-resolved `team_member` role, migration 0074: `owner`/`admin`/
/// `member`/`viewer`). The Worker is the SOLE setter (client copies are stripped
/// at the edge), so the container may trust it — currently the OWNER-only gate on
/// account deletion (`handle_account_delete`).
const ROLE_HEADER: &str = "x-corelink-role";

/// Logical wall-clock: 0 in routes (handler-provided `at_unix_ms` acts as
/// stand-in; production wiring threads a real clock collaborator).
fn now_ms() -> u64 {
    0u64
}

/// Run the native PAT possession backstop (cycle-2 nuclear red-team, cluster A)
/// when it is wired, EXCEPT for Clerk-session callers.
///
/// The control plane (`/v1/customer/*`) is reached by two caller classes:
/// - **PAT callers** (CLI): the Worker forwards the raw `Authorization: Bearer
///   <pat>` AND `x-corelink-token-prefix: <pat-prefix>`. The Worker proved
///   possession with an HMAC-only fast check, so a leaked `PAT_SIGNING_KEY`
///   lets an attacker forge a valid-looking PAT for ANY tenant. This gate
///   re-runs the FULL Argon2id Option-B verify (same `PatVerifier` the native
///   plane uses) and binds it to the claimed `tenant` — a forged PAT (right
///   HMAC, wrong/no random secret) ⇒ 401; a genuine PAT for tenant A presented
///   for tenant B ⇒ 401.
/// - **Clerk-session callers** (dashboard): the Worker sets
///   `x-corelink-token-prefix: clerk` and forwards NO bearer (it `delete`s
///   `authorization`). They were already edge-verified, so the PAT backstop is
///   skipped — running it would 401 every legitimate dashboard request.
///
/// `Some(resp)` ⇒ REJECT (401 forged/wrong-tenant / 503 verifier fault);
/// `None` ⇒ proceed (PAT verified, Clerk caller, or gate absent in dev/CI).
/// Called at the TOP of each handler, AFTER the fail-CLOSED tenant resolution,
/// BEFORE any storage/handler access.
async fn pat_gate_reject(
    state: &CustomerRouteState,
    tenant: &str,
    headers: &HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    // Clerk-session callers are edge-verified and carry no bearer — skip the
    // PAT Argon2id check for them (mirrors the native plane only gating the PAT
    // path). The token-prefix is a server-trusted header (the Worker strips any
    // client-supplied value before setting it).
    if principal(headers) == CLERK_TOKEN_PREFIX {
        return None;
    }
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}

/// True when the requested mint `scopes` ask for ANY write or admin/owner
/// capability — the privilege a read-only principal must NOT be able to grant
/// itself (cluster A self-escalation: a `cas:r` PAT minting a `cas:rw` PAT).
///
/// The customer mint accepts free-form scope strings; the admin-ui sends
/// `cache:read` / `cache:write`, and the internal spellings (`cas:rw` / `cas:w`
/// / `read-write` / `admin`) and role names (`Owner`) also confer write/admin
/// privilege. We treat the token set as the union of the cache-write grammar
/// (`crate::scope::requires_cache_write`, which already covers `cas:rw` /
/// `cas:w` / `read-write` / `admin`) PLUS the customer-plane spellings
/// `cache:write` and the privileged role names `owner` / `admin`. Match is
/// case-insensitive + exact-token (no substring), so `cache:read` /
/// `read-only` / `cas:r` are NOT flagged (a read-only mint is allowed from any
/// authenticated caller).
fn mint_requests_write(scopes: &[String]) -> bool {
    // SINGLE source of truth with the D1 scope persister
    // (`customer_d1::map_requested_scopes`), via `scope::classify_requested_scopes`.
    // rt-nuclear #15: this gate previously used exact-token match while the
    // persister used substring match (`s.contains("write")`), so `"writes"` was
    // FALSE here yet persisted `read-write` — a read-only PAT self-escalated.
    // The gate fires iff the mint would persist a write/admin credential — the
    // EXACT invariant the persister enforces (`mint_requests_write(v)` ⟺
    // `classify(v) ∈ {ReadWrite, Admin}`). Unrecognized tokens are NOT flagged
    // here (a benign non-cache token like "viewer" must not read as a write
    // mint); they are instead REJECTED by the persister with 4xx, which is where
    // the escalation is actually closed — so `"writes"` reaches the persister and
    // is refused there rather than silently persisted as read-write.
    matches!(
        crate::scope::classify_requested_scopes(scopes),
        Ok(crate::scope::RequestedScopeClass::ReadWrite | crate::scope::RequestedScopeClass::Admin)
    )
}

/// True when `role` is a privileged team role (`Owner` / `Admin`) — granting it
/// is a write/admin mutation a read-only principal must not perform (cluster A).
/// `Developer` / `Viewer` are non-privileged and allowed from any authenticated
/// caller. Case-insensitive exact match.
fn role_is_privileged(role: &str) -> bool {
    matches!(role.trim().to_ascii_lowercase().as_str(), "owner" | "admin")
}

/// The CALLER's D1-resolved team role, from the server-trusted [`ROLE_HEADER`]
/// (the Worker is the sole setter; client copies are stripped at the edge).
/// Empty / absent ⇒ `""` (fail-CLOSED at every role gate below).
fn caller_role(headers: &HeaderMap) -> String {
    header_or(headers, ROLE_HEADER, "")
        .trim()
        .to_ascii_lowercase()
}

/// True when the caller is the tenant OWNER or an ADMIN — the team-management /
/// billing capability (invite/remove seats, open the billing portal, cancel the
/// subscription). A plain `member`/`viewer` must NOT manage the team or billing
/// (RBAC hardening: the coarse `x-corelink-scope` granted every non-viewer
/// `read-write billing`, collapsing owner/admin/member — the role restores the
/// distinction). Fail-CLOSED on an unknown/empty role.
fn caller_is_owner_or_admin(headers: &HeaderMap) -> bool {
    matches!(caller_role(headers).as_str(), "owner" | "admin")
}

/// Billing-management / financial-PII gate (F-018).
///
/// The billing surfaces — the Stripe billing-portal mint (manage/remove payment
/// methods, **cancel the subscription**), the billing detail, the account
/// overview (billing status + amount-due + financial PII), and the audit log
/// (security/PII events) — are an owner/admin capability, NOT cache access. A
/// cache token is a credential meant only to READ/WRITE the cache; it must not
/// open the billing portal, mutate the subscription, or read financial/audit PII.
///
/// We gate on `requires_billing_admin` (finding H17): billing is a DEDICATED
/// capability, not cache write. The Worker forwards `billing` in `x-corelink-scope`
/// for a NON-viewer dashboard Clerk session (`read-write billing`), so a dashboard
/// user keeps billing access as before; an owner-grade `admin`/`owner` also passes.
/// A minted cache PAT (`read-write`/`cas:rw`/`cas:w`/`cas:r`) is NEVER granted
/// `billing`, so a leaked CI cache token can no longer open the portal, read
/// financial PII, or cancel the subscription — the H17 hole (was `requires_cache_write`).
///
/// `Some(resp)` ⇒ REJECT 403 (insufficient scope); `None` ⇒ proceed.
/// Called AFTER the fail-CLOSED tenant resolution + the PAT possession backstop,
/// BEFORE any storage/handler access.
fn billing_pii_gate_reject(headers: &HeaderMap) -> Option<axum::response::Response> {
    // We gate on `requires_billing_admin` (finding H17): billing is a DEDICATED
    // capability, not cache write. The Worker forwards `billing` in
    // `x-corelink-scope` ONLY for an OWNER/ADMIN dashboard Clerk session (a plain
    // `member`/`viewer` gets `read-write`/`read-only` — no `billing`), so a member
    // can no longer open the portal, read financial PII, or cancel the subscription
    // (RBAC hardening — the role restriction lives at the Worker's sole scope
    // setter). A minted cache PAT (`read-write`/`cas:rw`/…) is NEVER granted
    // `billing`, so a leaked CI cache token also cannot reach it.
    let caller_scope = header_or(headers, crate::scope::SCOPE_HEADER, "");
    if crate::scope::requires_billing_admin(&caller_scope) {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            "insufficient scope for billing / account-PII access",
        )
            .into_response(),
    )
}

// ─── Query param shapes ───────────────────────────────────────────────────────

/// `GET /v1/customer/usage` query params.
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Optional billing period filter (e.g. `"2026-05"`).
    pub period: Option<String>,
}

/// `GET /v1/customer/audit` query params.
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Optional ISO-8601 since filter (maps to `AuditQueryRequest::since`).
    pub from: Option<String>,
    /// Ignored in this surface (InMemory has no `to` filter); preserved
    /// for forward-compatibility with the admin-ui contract.
    pub to: Option<String>,
    /// Optional event-type filter (comma-separated string).
    pub kind: Option<String>,
}

// ─── POST body shapes ─────────────────────────────────────────────────────────

/// `POST /v1/customer/keys` request body.
#[derive(Debug, Deserialize)]
pub struct CreatePatBody {
    /// Human-readable name for the new PAT.
    pub name: String,
    /// Scopes to grant (e.g. `["cache:read", "cache:write"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// `POST /v1/pats` request body.
///
/// The public contract calls the human-readable key name a `label`; the
/// dashboard's older `/v1/customer/keys` form calls the same field `name`.
/// Both feed the same mint implementation so their tenant, principal, scope,
/// audit, and shown-once-token guarantees cannot drift.
#[derive(Debug, Deserialize)]
pub struct PatIssueBody {
    /// Human-readable label for the new PAT.
    pub label: String,
    /// Scopes to grant (e.g. `["cache:read", "cache:write"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// `POST /v1/customer/team/invite` request body.
#[derive(Debug, Deserialize)]
pub struct InviteBody {
    /// Email address to invite.
    pub email: String,
    /// Role to assign (`"Owner"` / `"Admin"` / `"Developer"` / `"Viewer"`).
    pub role: String,
}

// ─── Handler fns ─────────────────────────────────────────────────────────────

/// `GET /v1/customer/overview`
async fn handle_overview(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession backstop (cluster A) — reject a forged-HMAC / wrong-
    // tenant PAT BEFORE any storage access. Skipped for Clerk callers + dev/CI.
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Billing/PII gate (F-018): the overview carries billing status + amount-due
    // + financial PII — an owner/admin surface, not cache access. A read-only
    // (`cas:r`) cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = OverviewRequest::new(t, p, now_ms());
    match state.overview.overview(req) {
        Ok(resp) => {
            let body: Value = json!({
                "tenant_id":       resp.tenant_id,
                "tenant_name":     resp.tenant_name,
                "plan":            resp.plan,
                "usage": {
                    "period":      resp.usage.period,
                    "cas_bytes":   resp.usage.cas_bytes,
                    "quota_bytes": resp.usage.quota_bytes,
                    "reads":       resp.usage.reads,
                    "writes":      resp.usage.writes,
                },
                "billing": {
                    "status":             resp.billing.status,
                    "next_invoice_at":    resp.billing.next_invoice_at,
                    "amount_due_cents":   resp.billing.amount_due_cents,
                    "currency":           resp.billing.currency,
                },
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
                "recent_activity": resp.recent_activity.iter().map(|e| json!({
                    "event_id":   e.event_id,
                    "ts":         e.ts,
                    "event_type": e.event_type,
                    "severity":   e.severity,
                    "actor":      e.actor,
                    "summary":    e.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/usage?period=`
async fn handle_usage(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<UsageQuery>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    let p = principal(&headers);
    let req = UsageRequest::new(t, p, q.period, now_ms());
    match state.usage.usage(req) {
        Ok(resp) => {
            let body: Value = json!({
                "period":        resp.period,
                "cas_bytes":     resp.cas_bytes,
                "reads":         resp.reads,
                "writes":        resp.writes,
                "request_count": resp.request_count,
                "quota_bytes":   resp.quota_bytes,
                // `hit_rate` serializes as JSON `null` when None (no cache reads).
                "hit_rate":            resp.hit_rate,
                "time_saved_seconds":  resp.time_saved_seconds,
                "dollars_saved_cents": resp.dollars_saved_cents,
                "daily": resp.daily.iter().map(|d| json!({
                    "day":       d.day,
                    "reads":     d.reads,
                    "writes":    d.writes,
                    "cas_bytes": d.cas_bytes,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/audit?from=&to=&kind=`
async fn handle_audit(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Billing/PII gate (F-018): the audit log surfaces security/account PII
    // events — an owner/admin surface, not cache access. A read-only (`cas:r`)
    // cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    // `kind` is comma-separated; split into event_types Vec.
    let event_types: Vec<String> = q
        .kind
        .as_deref()
        .map(|k| {
            k.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    // `from` maps to `since` in the handler surface; `to` has no
    // analogue in AuditQueryRequest (InMemory doesn't filter by end-date).
    let req = AuditQueryRequest::new(t, p, q.from, event_types, now_ms());
    match state.audit.query(req) {
        Ok(resp) => {
            let body: Value = json!({
                "rows": resp.rows.iter().map(|r| json!({
                    "event_id":   r.event_id,
                    "ts":         r.ts,
                    "event_type": r.event_type,
                    "severity":   r.severity,
                    "actor":      r.actor,
                    "summary":    r.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/billing`
async fn handle_billing(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Billing/PII gate (F-018): billing detail (status / plan / invoices /
    // amounts) is financial PII — an owner/admin surface, not cache access. A
    // read-only (`cas:r`) cache token must not read it.
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = BillingRequest::new(t, p, now_ms());
    match state.billing.billing(req) {
        Ok(resp) => {
            let body: Value = json!({
                "status":                resp.status,
                "plan":                  resp.plan,
                "current_period_start":  resp.current_period_start,
                "current_period_end":    resp.current_period_end,
                "amount_due_cents":      resp.amount_due_cents,
                "currency":              resp.currency,
                "invoices": resp.invoices.iter().map(|i| json!({
                    "invoice_id":  i.invoice_id,
                    "issued_at":   i.issued_at,
                    "amount_cents": i.amount_cents,
                    "status":      i.status,
                    "hosted_url":  i.hosted_url,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/billing/portal`
async fn handle_billing_portal(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Billing-management gate (F-018): minting the Stripe billing-portal session
    // lets the caller manage/remove payment methods + CANCEL the subscription —
    // an owner/admin op, not cache access. A read-only (`cas:r`) cache token must
    // not open it (cluster A + CK-4 cross-tenant via F-006).
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    let p = principal(&headers);
    let req = PortalRequest::new(t, p, now_ms());
    match state.billing.portal_url(req) {
        Ok(resp) => {
            let body: Value = json!({ "portal_url": resp.portal_url });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/keys`
async fn handle_keys_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Privilege gate (rt-nuclear cycle-2 #7): key MANAGEMENT (enumerating the
    // tenant's PATs / BYOK status) is an admin op, not cache access. A read-only
    // (`cas:r`) cache token must not enumerate other principals' credentials —
    // info-disclosure + the recon step of the revoke attack. Mirror the
    // mint/revoke gate. Dashboard (`read-write`) + `cas:rw` pass; `cas:r` → 403.
    let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
    if !crate::scope::requires_cache_write(&caller_scope) {
        return (
            StatusCode::FORBIDDEN,
            "insufficient scope to list credentials",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = KeysListRequest::new(t, p, now_ms());
    match state.keys.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pats": resp.pats.iter().map(|row| json!({
                    "pat_id":      row.pat_id,
                    "name":        row.name,
                    "scopes":      row.scopes,
                    "created_at":  row.created_at,
                    "last_used_at": row.last_used_at,
                    "revoked_at":  row.revoked_at,
                })).collect::<Vec<_>>(),
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys`
async fn handle_keys_create(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<CreatePatBody>,
) -> axum::response::Response {
    create_pat_response(state, headers, body.name, body.scopes).await
}

/// `POST /v1/pats` — customer-authorized PAT issuance.
///
/// This deliberately shares the dashboard implementation instead of calling
/// `/_internal/pat/mint`: only the Worker-authenticated customer principal may
/// select the tenant, and the operator mint credential never reaches this path.
async fn handle_pats_create(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<PatIssueBody>,
) -> axum::response::Response {
    create_pat_response(state, headers, body.label, body.scopes).await
}

/// Common customer-authorized PAT mint flow for the dashboard and public API.
async fn create_pat_response(
    state: CustomerRouteState,
    headers: HeaderMap,
    name: String,
    scopes: Vec<String>,
) -> axum::response::Response {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Privilege-escalation gate (cluster A): a read-only principal must NOT be
    // able to MINT a write/admin credential (which would let a `cas:r` PAT
    // bootstrap a `cas:rw` PAT for itself — a self-escalation that survives key
    // rotation). The caller's capability comes from the Worker-trusted
    // `x-corelink-scope` header (the same `cache:write` capability the native
    // CAS/AC write handlers enforce via `CacheScope::can_write`). If the
    // requested scopes ask for any write/admin capability AND the caller lacks
    // cache-write, reject 403 BEFORE the mint. Clerk-session callers carry the
    // dashboard `read-write` scope (Worker-set), so they are unaffected.
    if mint_requests_write(&scopes) {
        let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
        if !crate::scope::requires_cache_write(&caller_scope) {
            return (
                StatusCode::FORBIDDEN,
                "insufficient scope to mint a write/admin credential",
            )
                .into_response();
        }
    }
    let bucket_tenant = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, t.as_bytes());
    let bucket_key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(
        bucket_tenant,
        PAT_ISSUE_ENDPOINT_ID,
    );
    let limiter_now_ms = crate::wall_clock::default_wall_clock().now_ms();
    if limiter_now_ms == 0 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "rate-limit clock unavailable",
        )
            .into_response();
    }
    match state
        .pat_issue_rate_limiter
        .try_acquire(bucket_tenant, bucket_key, 1, limiter_now_ms)
    {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => {}
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    "PAT issuance rate limit exceeded",
                )
                    .into_response();
                if let Ok(value) = retry_after_secs.to_string().parse() {
                    response
                        .headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, value);
                }
                return response;
            }
            _ => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    "PAT issuance rate limit exceeded",
                )
                    .into_response();
            }
        },
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "rate-limit pipeline failed",
            )
                .into_response();
        }
    }
    let p = principal(&headers);
    let req = KeyCreateRequest::new(t, p, name, scopes, now_ms());
    match state.keys.create(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
                "token": resp.token,
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys/:pat_id/revoke`
async fn handle_keys_revoke(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Path(pat_id): Path<String>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Privilege gate (rt-nuclear cycle-2 #7): revoking a credential is a
    // destructive admin op. A read-only (`cas:r`) principal must NOT be able to
    // revoke ANY credential in the tenant (incl. the owner's) — that is an
    // intra-tenant credential-DoS / owner-lockout. Mirror the mint gate
    // (`handle_keys_create`): require cache-write capability. Dashboard
    // (`read-write`) + `cas:rw` callers pass; a read-only token is rejected 403.
    let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
    if !crate::scope::requires_cache_write(&caller_scope) {
        return (
            StatusCode::FORBIDDEN,
            "insufficient scope to revoke a credential",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = KeyRevokeRequest::new(t, p, pat_id, now_ms());
    match state.keys.revoke(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/team`
async fn handle_team_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    let p = principal(&headers);
    let req = TeamListRequest::new(t, p, now_ms());
    match state.team.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "members": resp.members.iter().map(|m| json!({
                    "user_id":   m.user_id,
                    "email":     m.email,
                    "role":      m.role,
                    "joined_at": m.joined_at,
                    "status":    m.status,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/team/invite`
async fn handle_team_invite(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<InviteBody>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage/handler access.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Team-management RBAC (owner/admin only). The coarse `x-corelink-scope`
    // granted EVERY non-viewer `read-write billing`, so the prior cache-write gate
    // let a plain `member` invite seats — including a privileged `admin`, and (via
    // the member→owner escalation the account-delete audit flagged) an `owner`
    // seat. Gate on the D1-resolved role instead (migration 0074):
    //   1. `owner` is NEVER self-serve-invitable — there is exactly one owner (the
    //      tenant creator). Reject outright so the escalation chain (invite-owner →
    //      accept → resolve owner → delete tenant) is closed at the source.
    //      (`normalize_invite_role` also maps owner→admin as defense-in-depth.)
    //   2. inviting a privileged `admin` requires the caller be the OWNER.
    //   3. any invite at all requires the caller be owner OR admin (a plain
    //      member/viewer cannot add seats).
    if body.role.trim().eq_ignore_ascii_case("owner") {
        return (
            StatusCode::FORBIDDEN,
            "the owner role is not grantable via a team invite",
        )
            .into_response();
    }
    if !caller_is_owner_or_admin(&headers) {
        return (
            StatusCode::FORBIDDEN,
            "only the owner or an admin may invite team members",
        )
            .into_response();
    }
    if role_is_privileged(&body.role) && caller_role(&headers) != "owner" {
        return (
            StatusCode::FORBIDDEN,
            "only the owner may invite a privileged (admin) role",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = TeamInviteRequest::new(t, p, body.email, body.role, now_ms());
    match state.team.invite(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "member": {
                    "user_id":   resp.member.user_id,
                    "email":     resp.member.email,
                    "role":      resp.member.role,
                    "joined_at": resp.member.joined_at,
                    "status":    resp.member.status,
                },
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `DELETE /v1/customer/team/:user_id` — remove a member's seat. Owner/admin
/// only (cache-write scope); flips the seat to `removed` AND revokes the
/// member's PATs (the load-bearing security effect). Returns 200 with the
/// removed member + the revoked-PAT count.
async fn handle_team_remove(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401 first.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // Team-management RBAC (owner/admin only). Removing a seat is a destructive
    // admin op that revokes another principal's credentials — a plain `member`
    // must NOT be able to remove teammates / revoke their PATs (intra-tenant
    // lockout / credential-DoS). The coarse cache-write gate granted every
    // non-viewer this; gate on the D1-resolved role instead (the owner row itself
    // is separately protected from removal in `customer_d1`).
    if !caller_is_owner_or_admin(&headers) {
        return (
            StatusCode::FORBIDDEN,
            "only the owner or an admin may remove a team member",
        )
            .into_response();
    }
    let p = principal(&headers);
    let req = TeamRemoveRequest::new(t, p, user_id, now_ms());
    match state.team.remove(req) {
        Ok(resp) => {
            let body: Value = json!({
                "member": {
                    "user_id":   resp.member.user_id,
                    "email":     resp.member.email,
                    "role":      resp.member.role,
                    "joined_at": resp.member.joined_at,
                    "status":    resp.member.status,
                },
                "revoked_pats": resp.revoked_pats,
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/account/delete` — self-serve GDPR account erasure (C-ACCTDEL).
///
/// A customer erases their OWN account: this is a **Clerk-session-only** surface
/// (`x-corelink-token-prefix: clerk`, set by the Worker after edge-verifying the
/// session and stripping the bearer). A cache PAT (`cas:r` / `cas:rw`) is a
/// data-plane credential and MUST NOT trigger account erasure — a PAT caller gets
/// 403 (mirrors the e2e contract that the erasure-request surface is session-auth'd).
///
/// On accept it mirrors the Clerk `user.deleted` path: build the canonical
/// `dsr.queued.v1` message + `INSERT OR IGNORE` a `dsr_requested` anchor row, then
/// enqueue — returning **202 Accepted**. Idempotent (the deterministic `dsr_id` +
/// `INSERT OR IGNORE` make a repeat request a no-op). Fail-CLOSED: when the
/// requester is unwired (dev/CI) → 503; on a D1/transport fault → 500 (never a
/// silent 202 we cannot honor).
async fn handle_account_delete(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401 first.
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Clerk-session ONLY. A customer deletes their OWN account via the dashboard;
    // a cache PAT must not erase the account. (The Worker stamps the `clerk`
    // token-prefix for an edge-verified session and forwards NO bearer.)
    if principal(&headers) != CLERK_TOKEN_PREFIX {
        return (
            StatusCode::FORBIDDEN,
            "account deletion requires a dashboard (Clerk) session",
        )
            .into_response();
    }
    // OWNER-only (team RBAC, migration 0074). This erases the WHOLE tenant
    // (`request_erasure(&t)` below), so a non-owner seat — `admin`/`member`/
    // `viewer`, which all resolve to the OWNING tenant and all carry a Clerk
    // session — must NOT be able to nuke every teammate's account. The Worker
    // forwards the D1-resolved role as the server-trusted `x-corelink-role`
    // (client copies stripped); fail-CLOSED on anything but `owner`.
    if header_or(&headers, ROLE_HEADER, "") != "owner" {
        return (
            StatusCode::FORBIDDEN,
            "account deletion requires the tenant OWNER",
        )
            .into_response();
    }
    let Some(requester) = state.account_deletion.as_ref() else {
        // Fail-CLOSED: the erasure path is not wired (dev/CI / unconfigured).
        // NEVER ack a GDPR erasure we cannot durably honor.
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "account deletion not configured",
        )
            .into_response();
    };
    match requester.request_erasure(&t) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(json!({ "ok": true, "status": "erasure_requested" })),
        )
            .into_response(),
        // No provisioned account to erase (already deleted / never provisioned).
        // Idempotent ACK (202) — mirrors the webhook's "no tenant → ack" no-op so
        // the client UX is uniform and a double-submit is safe.
        Err(AccountDeletionError::NotFound) => (
            StatusCode::ACCEPTED,
            Json(json!({ "ok": true, "status": "no_account" })),
        )
            .into_response(),
        Err(AccountDeletionError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "account delete: erasure request failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "erasure request failed").into_response()
        }
    }
}

/// `POST /v1/customer/account/export` — self-serve tenant bulk export (SEAM).
///
/// Streams the full portability bundle (content-addressed NDJSON): the tenant's
/// CAS + AC blob BYTES + the D1 governance records (RBAC/team, DPA/consent, audit
/// slice). Unblocks the CLI `corelink tenant export` (PR #708) and serves GDPR
/// Art.20 data portability for the whole tenant.
///
/// Auth: owner/admin only. Accepts EITHER a dashboard Clerk session OR a
/// write-capable (`cas:rw`) PAT — the bundle exposes the tenant's team PII, DPA
/// records, audit log AND every blob, so a read-only (`cas:r`) cache token is NOT
/// sufficient (gated on `requires_cache_write`, exactly like the billing/keys/team
/// surfaces). The native PAT-possession backstop runs first (a leaked
/// `PAT_SIGNING_KEY` cannot forge a bearer for a victim tenant). Rate-limited
/// per-tenant. Audited: a durable `account.export` row is written BEFORE any bytes
/// are disclosed. Fail-CLOSED: unwired source → 503; a gather/audit fault → 5xx —
/// never a partial 200.
async fn handle_account_export(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> axum::response::Response {
    use crate::routes::customer_export::TenantExportError;

    // 1. Fail-CLOSED tenant resolution (401 on missing/sentinel).
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // 2. Native PAT possession backstop (forged / wrong-tenant PAT → 401/503).
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // 3. Owner/admin + PII gate: the bundle carries team PII / DPA / audit / all
    //    blobs — a read-only cache token must not export it (F-018 sibling).
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    // 4. Require the export source (fail-CLOSED 503 when unwired — dev/CI).
    let Some(source) = state.export.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "tenant export not configured",
        )
            .into_response();
    };
    // 5. Per-tenant rate limit (heavy full-tenant read). The tenant may be a
    //    non-UUID fixture id, so bucket on a deterministic v5 UUID derived from it.
    let export_now_ms = crate::wall_clock::default_wall_clock().now_ms();
    let bucket_tenant = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, t.as_bytes());
    let bucket_key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(
        bucket_tenant,
        crate::routes::customer_export::EXPORT_ENDPOINT_ID,
    );
    match state
        .export_rate_limiter
        .try_acquire(bucket_tenant, bucket_key, 1, export_now_ms)
    {
        Ok(outcome) => match outcome.decision {
            corelink_ratelimit::RateLimitDecision::Allow { .. } => {}
            corelink_ratelimit::RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                return resp;
            }
            // The decision enum is `#[non_exhaustive]`; any future non-Allow arm
            // is treated as a deny so the route never streams under an unknown
            // decision shape.
            _ => return (StatusCode::TOO_MANY_REQUESTS, "rate-limited").into_response(),
        },
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response();
        }
    }
    // 6. Gather governance + the blob index UP FRONT (fail-CLOSED — never a
    //    partial-looking 200). Small: D1 rows + a (digest,size) index.
    let metadata = match source.metadata_records(&t) {
        Ok(m) => m,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: source unavailable");
            return (StatusCode::SERVICE_UNAVAILABLE, "export source unavailable").into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: metadata gather failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export gather failed").into_response();
        }
    };
    let blobs = match source.blob_index(&t) {
        Ok(b) => b,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: blob index unavailable");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "export storage unavailable",
            )
                .into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: blob index failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export enumerate failed").into_response();
        }
    };
    // 7. Audit BEFORE disclosure (durable `account.export` row). A fault here
    //    aborts fail-CLOSED so a disclosure is never unlogged.
    if let Err(e) = source.record_export_audit(&t, blobs.len(), metadata.len()) {
        tracing::error!(error = %e, tenant = %t, "tenant export: audit write failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "export audit failed").into_response();
    }
    tracing::info!(
        event = "customer.account.export",
        tenant = %t,
        blob_count = blobs.len(),
        metadata_count = metadata.len(),
        "tenant export streaming"
    );
    // 8. Stream the content-addressed NDJSON bundle. Blob bytes are fetched
    //    lazily one at a time (memory-bounded).
    let body_stream = crate::routes::customer_export::build_export_stream(
        source,
        t,
        export_now_ms,
        metadata,
        blobs,
    );
    let body = axum::body::Body::new(http_body_util::StreamBody::new(body_stream));
    let mut resp = (StatusCode::OK, body).into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str("application/x-ndjson") {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, val);
    }
    if let (Ok(name), Ok(val)) = (
        axum::http::HeaderName::from_bytes(b"x-corelink-export-schema"),
        axum::http::HeaderValue::from_str(crate::routes::customer_export::EXPORT_SCHEMA),
    ) {
        resp.headers_mut().insert(name, val);
    }
    resp
}

// ─── Account-deletion (DSR erasure) collaborators (C-ACCTDEL) ──────────────────

/// Failure modes of a self-serve account-deletion request.
#[derive(Debug)]
pub enum AccountDeletionError {
    /// No account/tenant row exists to erase (already deleted / never
    /// provisioned) — the route maps this to an idempotent 202 no-op.
    NotFound,
    /// A D1 / transport / configuration fault — the route fails CLOSED (500) so
    /// the obligation is retried, never silently dropped.
    Internal(String),
}

/// Self-serve account-erasure requester (route collaborator). The production
/// impl is [`D1AccountDeletionRequester`]; tests supply a mock. Wired into
/// [`CustomerRouteState::account_deletion`] by `routes.rs` (the erasure sink is a
/// cross-module collaborator built alongside the DSR worker), exactly as
/// `pat_gate` is wired there.
pub trait AccountDeletionRequester: Send + Sync + core::fmt::Debug {
    /// Durably anchor + enqueue a GDPR erasure for `tenant_id`. Idempotent
    /// (deterministic `dsr_id` + `INSERT OR IGNORE`). Uses a real wall clock for
    /// the SLA-anchor `queued_at_ms` (the route's logical clock is 0).
    ///
    /// # Errors
    ///
    /// [`AccountDeletionError::NotFound`] when no tenant row exists;
    /// [`AccountDeletionError::Internal`] on any D1 / transport / config fault.
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError>;
}

/// Transport seam for the built `dsr.queued.v1` message. The genuinely
/// cross-service piece (the container has no CF Queue producer binding): the
/// production sink is wired in `routes.rs` over the in-process DSR erasure worker
/// (or a queue producer). Kept behind a trait so [`D1AccountDeletionRequester`]
/// owns the message construction + the `dsr_requested` anchor (the C-ACCTDEL
/// D1-observable effects) hermetically, with the transport injected.
pub trait DsrErasureSink: Send + Sync + core::fmt::Debug {
    /// Enqueue an already-anchored `dsr.queued.v1` erasure message.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any transport failure (the requester maps it to
    /// [`AccountDeletionError::Internal`] → 500 fail-CLOSED).
    fn enqueue(&self, message: &Value) -> Result<(), String>;
}

/// `dev.hugr.corelink.dsr.queued.v1` schema id (FROZEN — mirrors
/// `apps/signup-worker/src/webhooks/clerk.ts buildErasureQueueMessage`).
const DSR_QUEUED_SCHEMA: &str = "dev.hugr.corelink.dsr.queued.v1";

/// Deterministic, name-based (v5-shaped) UUID from a subject key — byte-for-byte
/// the `deterministicDsrId` algorithm in `clerk.ts` (`SHA-256("corelink-dsr-v1:"
/// + key)`, first 16 bytes, version 5 + RFC-4122 variant). Keying on the Clerk
/// user id gives the SAME `dsr_id` as the webhook path, so a dashboard-initiated
/// delete and a Clerk `user.deleted` for the same account are idempotency-compatible.
#[must_use]
pub(crate) fn deterministic_dsr_id(subject_key: &str) -> String {
    let digest = Sha256::digest(format!("corelink-dsr-v1:{subject_key}").as_bytes());
    let mut b = [0u8; 16];
    // First 16 bytes of the SHA-256 digest (the digest is 32 bytes — never short).
    b.copy_from_slice(digest.get(..16).unwrap_or(&[0u8; 16]));
    b[6] = (b[6] & 0x0f) | 0x50; // version 5 (name-based)
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
                                 // Render via the uuid crate (lowercase, hyphenated 8-4-4-4-12) — no manual
                                 // slicing; the version/variant bits set above survive verbatim.
    uuid::Uuid::from_bytes(b).to_string()
}

/// Production [`AccountDeletionRequester`] over the [`crate::customer_d1::CustomerD1`]
/// row-source seam + an injected [`DsrErasureSink`]. Mirrors the Clerk
/// `user.deleted` path: resolve the account's Clerk id, derive the deterministic
/// `dsr_id`, honor an operator legal hold, build the canonical `dsr.queued.v1`
/// message, `INSERT OR IGNORE` the `dsr_requested` anchor (G4), then enqueue.
pub struct D1AccountDeletionRequester {
    /// D1 row source (production: `customer_d1::D1HttpCustomerDb`).
    db: Arc<dyn crate::customer_d1::CustomerD1>,
    /// Erasure-message transport (wired in `routes.rs`).
    sink: Arc<dyn DsrErasureSink>,
}

impl core::fmt::Debug for D1AccountDeletionRequester {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AccountDeletionRequester")
            .field("db", &"[CustomerD1]")
            .field("sink", &self.sink)
            .finish()
    }
}

impl D1AccountDeletionRequester {
    /// Wire the requester over a D1 row source + an erasure sink.
    #[must_use]
    pub fn new(db: Arc<dyn crate::customer_d1::CustomerD1>, sink: Arc<dyn DsrErasureSink>) -> Self {
        Self { db, sink }
    }

    /// Real wall-clock unix-ms (the SLA anchor — the route's logical clock is 0).
    fn now_ms() -> u64 {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        )
        .unwrap_or(0)
    }

    /// `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)` hex — the per-DSR erasure salt
    /// (GDPR Art. 4(5) unlinkable pseudonymization), mirroring `clerk.ts`
    /// `deriveErasureSalt`. Fail-CLOSED: the key is REQUIRED here (the requester
    /// is only wired in configured/prod envs); an absent/empty key is an
    /// operator fault that must surface as 500, never a predictable salt.
    fn derive_salt_hex(dsr_id: &str) -> Result<String, AccountDeletionError> {
        let key = std::env::var("ERASURE_SALT_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AccountDeletionError::Internal(
                    "ERASURE_SALT_KEY unset — refusing to derive a predictable erasure salt \
                     (fail-CLOSED)"
                        .to_owned(),
                )
            })?;
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes()).map_err(|e| {
            AccountDeletionError::Internal(format!("erasure salt key invalid: {e}"))
        })?;
        mac.update(dsr_id.as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    /// Is `tenant_id` under an operator legal hold? Mirrors the webhook's
    /// `tenantUnderLegalHold` posture against the FROZEN C-LEGALHOLD schema
    /// (migration 0076: a row's presence == held). A query error (table not yet
    /// provisioned) → `false`: returning `true` on error would make EVERY
    /// deletion a no-op preservation and silently break the live erasure
    /// obligation (a far larger harm than the not-yet-built hold feature).
    fn under_legal_hold(&self, tenant_id: &str) -> bool {
        match self.db.query(
            "SELECT 1 AS held FROM tenant_legal_hold WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        ) {
            Ok(rows) => !rows.is_empty(),
            Err(_) => false,
        }
    }
}

impl AccountDeletionRequester for D1AccountDeletionRequester {
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError> {
        // Resolve the account row + its Clerk id (subject key for the deterministic
        // dsr_id). No row → nothing to erase (already deleted) → NotFound.
        let rows = self
            .db
            .query(
                "SELECT clerk_user_id FROM tenant WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant_id)],
            )
            .map_err(|e| AccountDeletionError::Internal(format!("tenant lookup failed: {e}")))?;
        let Some(row) = rows.into_iter().next() else {
            return Err(AccountDeletionError::NotFound);
        };
        // Prefer the Clerk user id (dsr_id parity with the webhook path); fall back
        // to the tenant id when absent (still deterministic + idempotent).
        let subject_key = row
            .get("clerk_user_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(tenant_id);
        let dsr_id = deterministic_dsr_id(subject_key);
        let clerk_user_id = row
            .get("clerk_user_id")
            .and_then(Value::as_str)
            .map(str::to_owned);

        let salt_hex = Self::derive_salt_hex(&dsr_id)?;
        let legal_hold = self.under_legal_hold(tenant_id);
        let now_ms = Self::now_ms();

        // Canonical dsr.queued.v1 envelope (mirrors buildErasureQueueMessage):
        // subject_id == tenant_id (1 Clerk user : 1 tenant — the tenant is the
        // deletion unit); source distinguishes the self-serve trigger.
        let message = json!({
            "schema": DSR_QUEUED_SCHEMA,
            "dsr_id": dsr_id,
            "tenant_id": tenant_id,
            "subject_id": tenant_id,
            "erasure_salt_hex": salt_hex,
            "queued_at_ms": now_ms,
            "legal_hold": legal_hold,
            "source": "customer.account.delete",
            "clerk_user_id": clerk_user_id,
        });

        // G4 (WI-S11-008): write the durable "DSR requested" anchor BEFORE enqueue
        // so the 24h verify sweep can detect an SLA breach even if the erasure
        // fails before any backend tombstone lands. Idempotent: dsr_id is
        // deterministic, so a repeat request is an INSERT-OR-IGNORE no-op. This
        // row is ALSO the legitimacy gate the /_internal/dsr/erase consumer checks
        // (dsr_requested must exist for (dsr_id, tenant)) — writing it first makes
        // the subsequent enqueue authorized by construction.
        self.db
            .query(
                "INSERT OR IGNORE INTO dsr_requested (dsr_id, tenant_id, requested_at, status) \
                 VALUES (?1, ?2, ?3, 'requested')",
                vec![
                    json!(dsr_id),
                    json!(tenant_id),
                    json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|e| {
                AccountDeletionError::Internal(format!("dsr_requested anchor write failed: {e}"))
            })?;

        // Enqueue the erasure message (transport injected by routes.rs).
        self.sink
            .enqueue(&message)
            .map_err(|e| AccountDeletionError::Internal(format!("erasure enqueue failed: {e}")))?;
        Ok(())
    }
}

// ─── Error mapping ────────────────────────────────────────────────────────────

/// Map a [`CustomerHandlerError`] to the canonical HTTP response.
fn map_err(e: CustomerHandlerError) -> axum::response::Response {
    tracing::warn!(error = ?e, "customer handler error");
    match e {
        CustomerHandlerError::Unauthorized(_) => {
            (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
        }
        CustomerHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CustomerHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        CustomerHandlerError::AuditFailed(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // HONEST v1 (dashboard revival WP-3): an endpoint the concrete
        // handler does not implement yet is an explicit 501 — never
        // fabricated data, never a misleading 404/500. Team invites are
        // now fully implemented (D1 create/list/remove + signup-worker
        // accept, ADR-S33-001 / migration 0074), so this generic arm is a
        // defensive fallback for any future NotImplemented surface, NOT a
        // team-invite stub. The message stays generic + honest accordingly.
        CustomerHandlerError::NotImplemented(_) => (
            StatusCode::NOT_IMPLEMENTED,
            "this endpoint is not yet implemented",
        )
            .into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

#[cfg(test)]
mod tests;
