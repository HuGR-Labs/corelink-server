// Customer self-serve HTTP routes: `/v1/customer/*`
//
// Wires [`corelink_handler_customer`]'s 6 traits + `InMemoryCustomerHandler`
// into the axum router. The Worker injects `x-corelink-tenant-id` +
// `x-corelink-token-prefix` headers post-auth; these routes trust those
// headers exclusively and never accept client-supplied tenant identities.
//
// # Endpoints
//
// ```text
// GET  /v1/customer/overview
// GET  /v1/customer/usage?period=<period>
// GET  /v1/customer/audit?from=&to=&kind=
// GET  /v1/customer/billing
// POST /v1/customer/billing/portal
// GET  /v1/customer/keys
// POST /v1/customer/keys              { name, scopes[] }
// POST /v1/pats                       { label, scopes[] }
// POST /v1/customer/keys/:pat_id/revoke
// GET  /v1/customer/team
// POST /v1/customer/team/invite       { email, role }
// ```
//
// # Auth model
//
// All routes require the Worker-injected headers. Absent headers fall back
// to `"_unknown"` (fail-CLOSED: the handler emits `Unauthorized` on an
// unknown principal and the route maps it to 401).
//
// # Pattern
//
// Mirrors [`super::cas`] exactly: one `CustomerRouteState` struct holding
// trait objects, one `build_handlers()` factory, one `router(state)`.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
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
#[cfg(test)]
use corelink_ratelimit::RateLimiter;
#[cfg(test)]
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimitConfig, RateLimitDecision,
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
    /// Test-only seam for direct container route focals. Production PAT
    /// issuance is authorized by the tenant Durable Object lease, not by an
    /// in-process limiter that resets on recycle.
    #[cfg(test)]
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
            #[cfg(test)]
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
        #[cfg(test)]
        pat_issue_rate_limiter: build_pat_issue_rate_limiter(),
    }
}

/// Stable endpoint identity for the public self-serve mint bucket.
pub const PAT_ISSUE_ENDPOINT_ID: &str = "pat-issue";

/// Header stamped only by the tenant Durable Object after its durable,
/// serialized bucket decision. The Worker strips client copies before routing;
/// the container fails closed when the lease is absent.
pub const PAT_ISSUE_AUTHORIZED_HEADER: &str = "x-corelink-pat-issue-authorized";

/// Public PAT issuance is deliberately much slower than the generic data
/// plane: ten tokens per tenant per hour, with a ten-request initial burst.
/// The exact ratio preserves the one-token-per-360-second cadence without a
/// fixed-point truncation that would incorrectly defer the boundary to 361s.
#[must_use]
#[cfg(test)]
pub fn pat_issue_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig::with_fractional_refill_ratio(10, 3600, 10, 1, 86_400, 7 * 86_400)
        .unwrap_or_else(RateLimitConfig::canonical)
}

/// Build the per-tenant public PAT issuance limiter. The in-memory
/// implementation is scoped to the customer Durable Object instance; the
/// composite bucket key still includes the tenant and endpoint so tenants
/// cannot consume one another's allowance.
#[must_use]
#[cfg(test)]
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

/// True when `role` is a privileged team role (`owner` / `admin`) — granting it
/// is a write/admin mutation a read-only principal must not perform (cluster A).
/// `member` / `viewer` are non-privileged and allowed from any authenticated
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
    /// Role to assign (`"admin"` / `"member"` / `"viewer"`).
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
