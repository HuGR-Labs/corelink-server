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
    }
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
    Some(Arc::new(D1AccountDeletionRequester::new(Arc::new(db), sink)))
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
        .route("/v1/customer/keys/{pat_id}/revoke", post(handle_keys_revoke))
        .route("/v1/customer/team", get(handle_team_list))
        .route("/v1/customer/team/invite", post(handle_team_invite))
        .route("/v1/customer/team/{user_id}", delete(handle_team_remove))
        .route("/v1/customer/account/delete", post(handle_account_delete))
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

/// Billing-management / financial-PII gate (F-018).
///
/// The billing surfaces — the Stripe billing-portal mint (manage/remove payment
/// methods, **cancel the subscription**), the billing detail, the account
/// overview (billing status + amount-due + financial PII), and the audit log
/// (security/PII events) — are an owner/admin capability, NOT cache access. A
/// read-only (`cas:r`) cache token is a credential meant only to READ the cache;
/// it must not open the billing portal, mutate the subscription, or read
/// financial/audit PII.
///
/// We gate on `requires_cache_write` (the same Worker-trusted `x-corelink-scope`
/// capability the key-management + privileged-team siblings enforce at
/// `customer.rs` keys-list/create/revoke + team-invite) — the maximal owner/admin
/// proxy available on this plane: dashboard Clerk-session callers carry the
/// `read-write` scope (Worker-set), and `cas:rw` / `admin` PATs pass; a `cas:r`
/// PAT → 403. Cache scopes must NOT grant billing-management or PII reads.
///
/// `Some(resp)` ⇒ REJECT 403 (insufficient scope); `None` ⇒ proceed.
/// Called AFTER the fail-CLOSED tenant resolution + the PAT possession backstop,
/// BEFORE any storage/handler access.
fn billing_pii_gate_reject(headers: &HeaderMap) -> Option<axum::response::Response> {
    let caller_scope = header_or(headers, crate::scope::SCOPE_HEADER, "");
    if crate::scope::requires_cache_write(&caller_scope) {
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
    // Privilege-escalation gate (cluster A): a read-only principal must NOT be
    // able to MINT a write/admin credential (which would let a `cas:r` PAT
    // bootstrap a `cas:rw` PAT for itself — a self-escalation that survives key
    // rotation). The caller's capability comes from the Worker-trusted
    // `x-corelink-scope` header (the same `cache:write` capability the native
    // CAS/AC write handlers enforce via `CacheScope::can_write`). If the
    // requested scopes ask for any write/admin capability AND the caller lacks
    // cache-write, reject 403 BEFORE the mint. Clerk-session callers carry the
    // dashboard `read-write` scope (Worker-set), so they are unaffected.
    if mint_requests_write(&body.scopes) {
        let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
        if !crate::scope::requires_cache_write(&caller_scope) {
            return (
                StatusCode::FORBIDDEN,
                "insufficient scope to mint a write/admin credential",
            )
                .into_response();
        }
    }
    let p = principal(&headers);
    let req = KeyCreateRequest::new(t, p, body.name, body.scopes, now_ms());
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
    // Privilege-escalation gate (cluster A): inviting a privileged role
    // (`Owner` / `Admin`) is a write/admin mutation — a read-only principal
    // must not be able to add a privileged member. Mirrors the keys-create
    // scope gate. A read-only caller may still invite a `Developer` / `Viewer`.
    if role_is_privileged(&body.role) {
        let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
        if !crate::scope::requires_cache_write(&caller_scope) {
            return (
                StatusCode::FORBIDDEN,
                "insufficient scope to invite a privileged role",
            )
                .into_response();
        }
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
    // Privilege gate: removing a seat is a destructive admin op that revokes
    // another principal's credentials. A read-only (`cas:r`) caller must NOT be
    // able to remove a seat (intra-tenant credential-DoS / member lockout) —
    // require cache-write capability (mirrors keys-revoke + invite-privileged).
    let caller_scope = header_or(&headers, crate::scope::SCOPE_HEADER, "");
    if !crate::scope::requires_cache_write(&caller_scope) {
        return (
            StatusCode::FORBIDDEN,
            "insufficient scope to remove a team member",
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
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes())
            .map_err(|e| AccountDeletionError::Internal(format!("erasure salt key invalid: {e}")))?;
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
        CustomerHandlerError::NotImplemented(_) => {
            (StatusCode::NOT_IMPLEMENTED, "this endpoint is not yet implemented").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use corelink_handler_customer::request::{
        ByokStatus, InvoiceRow, OverviewBilling, OverviewUsage, PatRow,
    };
    use corelink_handler_customer::{BillingResponse, OverviewResponse, UsageResponse};
    use tower::ServiceExt;

    /// Build a fixture state backed by shared InMemoryCustomerHandler
    /// and return both the state and the underlying handler for seeding.
    fn fixture() -> (CustomerRouteState, Arc<InMemoryCustomerHandler>) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryCustomerHandler> =
            Arc::new(InMemoryCustomerHandler::new(audit, sli));
        let state = CustomerRouteState {
            overview: shared.clone(),
            usage: shared.clone(),
            billing: shared.clone(),
            keys: shared.clone(),
            team: shared.clone(),
            audit: shared.clone(),
            pat_gate: None,
            account_deletion: None,
        };
        (state, shared)
    }

    // ── Route table smoke tests ───────────────────────────────────────────────

    #[test]
    fn build_handlers_returns_usable_state() {
        // Smoke: build_handlers() constructs without panic.
        let state = build_handlers();
        let _router = router(state);
    }

    #[test]
    fn router_constructs_from_fixture() {
        let (state, _) = fixture();
        let _r = router(state);
    }

    // ── Tenant resolution ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn overview_reads_tenant_from_header() {
        let (state, shared) = fixture();
        // Seed an overview for tenant-xyz so the handler returns OK.
        let overview = OverviewResponse::new(
            "tenant-xyz",
            "XYZ Corp",
            "starter",
            OverviewUsage::new("2026-05", 0, 1_000_000, 0, 0),
            OverviewBilling::new("active", "2026-06-01T00:00:00Z", 2900, "usd"),
            ByokStatus::new("none", None, None),
            vec![],
        );
        shared.seed_overview("tenant-xyz", overview).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            // F-018: the overview carries billing/PII — requires a write-capable
            // (dashboard `read-write` / `cas:rw`) caller, like keys/team mgmt.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-tenant-id", "tenant-xyz")
            .header("x-corelink-token-prefix", "clpat_abc")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["tenant_id"], "tenant-xyz");
        assert_eq!(v["tenant_name"], "XYZ Corp");
        assert_eq!(v["plan"], "starter");
    }

    #[tokio::test]
    async fn overview_unknown_tenant_returns_404() {
        // With no seed the InMemory handler returns NotFound -> 404.
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            // F-018: billing/PII surface — pass the write-capable scope so the
            // request reaches the handler (asserting the 404, not the scope 403).
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-tenant-id", "ghost")
            .header("x-corelink-token-prefix", "clpat_x")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_tenant_header_is_401_fail_closed() {
        // F-defense: a missing tenant header must FAIL-CLOSED with 401 — NOT
        // fall back to the `"_unknown"` sentinel (the old behavior, which let
        // an unauthenticated request reach the handler under a sentinel
        // tenant). The 401 fires BEFORE any handler/storage access.
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sentinel_tenant_header_is_401_fail_closed() {
        // A sentinel value in the header is not a real authenticated tenant
        // and must be rejected 401, same as a missing header.
        let (state, _) = fixture();
        let app = router(state);
        for sentinel in ["_unknown", "_anonymous", "_system", "_pending", "   "] {
            let req = Request::builder()
                .uri("/v1/customer/overview")
                .method("GET")
                .header("x-corelink-tenant-id", sentinel)
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.expect("oneshot");
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "sentinel {sentinel:?} must fail closed"
            );
        }
    }

    // ── Usage route ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn usage_route_returns_200_with_period() {
        let (state, shared) = fixture();
        let usage =
            UsageResponse::new("2026-05", 512, 100, 50, 1_000_000, vec![], 200, Some(0.75), 900, 1);
        shared.seed_usage("t1", usage).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/usage?period=2026-05")
            .method("GET")
            .header("x-corelink-tenant-id", "t1")
            .header("x-corelink-token-prefix", "clpat_t1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["period"], "2026-05");
        assert_eq!(v["reads"], 100u64);
        assert_eq!(v["writes"], 50u64);
        assert_eq!(v["request_count"], 200u64);
        assert_eq!(v["hit_rate"], 0.75);
        assert_eq!(v["time_saved_seconds"], 900u64);
        assert_eq!(v["dollars_saved_cents"], 1u64);
    }

    // ── Billing routes ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn billing_route_returns_200() {
        let (state, shared) = fixture();
        let billing = BillingResponse::new(
            "active",
            "starter",
            "2026-05-01",
            "2026-06-01",
            2900,
            "usd",
            vec![InvoiceRow::new(
                "inv_001",
                "2026-05-01",
                2900,
                "paid",
                "https://invoice.stripe.com/inv_001",
            )],
        );
        shared.seed_billing("t2", billing).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing")
            .method("GET")
            // F-018: billing detail is financial PII — requires a write-capable
            // (dashboard `read-write` / `cas:rw`) caller.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-tenant-id", "t2")
            .header("x-corelink-token-prefix", "clpat_t2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "active");
        assert_eq!(v["invoices"].as_array().expect("invoices").len(), 1);
    }

    #[tokio::test]
    async fn billing_portal_returns_portal_url() {
        let (state, shared) = fixture();
        // Billing portal only needs billing seeded for happy path on the
        // real billing handler — but portal_url in InMemory doesn't look
        // up the billing store; it always returns a stub URL. However we
        // still seed billing to avoid any future path divergence.
        let billing = BillingResponse::new(
            "active",
            "team",
            "2026-05-01",
            "2026-06-01",
            4900,
            "usd",
            vec![],
        );
        shared.seed_billing("t3", billing).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            // F-018: opening the Stripe billing portal (cancel subscription /
            // manage payment methods) requires a write-capable owner/admin
            // caller — a bare cache PAT must NOT reach it. Send the dashboard
            // `read-write` scope on the happy path.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-tenant-id", "t3")
            .header("x-corelink-token-prefix", "clpat_t3")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let url = v["portal_url"].as_str().expect("portal_url string");
        assert!(url.starts_with("https://billing.stripe.com/"));
    }

    // ── F-018: billing / account-PII scope gate ───────────────────────────────
    //
    // A read-only (`cas:r`) cache PAT must NOT open the Stripe billing portal
    // (cancel subscription / manage payment methods) nor read billing / account /
    // audit financial-PII. Before this gate the route's own happy-path test sent
    // NO scope header and asserted 200 — proving a non-write principal succeeded.
    // These tests assert the gate fires (403) for a read-only caller on EVERY
    // billing/PII surface, and that a write-capable caller still passes.

    /// A read-only caller MUST NOT open the Stripe billing portal (403).
    #[tokio::test]
    async fn read_only_caller_cannot_open_billing_portal() {
        let (state, _shared) = fixture(); // None gate isolates the scope check.
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
            .header("x-corelink-token-prefix", "clpat_ro")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-only caller MUST NOT read the account overview (billing/PII) (403).
    #[tokio::test]
    async fn read_only_caller_cannot_read_overview() {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header("x-corelink-token-prefix", "clpat_ro")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-only caller MUST NOT read billing detail (financial PII) (403).
    #[tokio::test]
    async fn read_only_caller_cannot_read_billing() {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing")
            .method("GET")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header("x-corelink-token-prefix", "clpat_ro")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-only caller MUST NOT read the audit log (account PII) (403).
    #[tokio::test]
    async fn read_only_caller_cannot_read_audit() {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/audit")
            .method("GET")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header("x-corelink-token-prefix", "clpat_ro")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A missing scope header (fail-CLOSED) ALSO denies the billing portal: a
    /// bare cache PAT with no scope must not reach the money/PII surface.
    #[tokio::test]
    async fn missing_scope_cannot_open_billing_portal() {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header("x-corelink-token-prefix", "clpat_ro")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A write-capable (`cas:rw`) PAT caller passes the F-018 gate (the gate is a
    /// NO-OP for a sufficiently-scoped principal) — billing/PII stays reachable
    /// for owners/admins + the dashboard.
    #[tokio::test]
    async fn write_capable_caller_can_open_billing_portal() {
        let (state, shared) = fixture();
        let billing =
            BillingResponse::new("active", "pro", "2026-05-01", "2026-06-01", 5000, "usd", vec![]);
        shared.seed_billing("rw-tenant", billing).expect("seed");
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "rw-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("x-corelink-token-prefix", "clpat_rw")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── Keys routes ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn keys_create_returns_201_with_token() {
        let (state, _) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "ci-key",
            "scopes": ["cache:read", "cache:write"]
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "t4")
            .header("x-corelink-token-prefix", "clpat_t4")
            // Minting a write credential (`cache:write`) requires a write-capable
            // caller (cluster-A scope gate): a read-only principal cannot
            // self-escalate. A real write-scoped PAT caller carries this header.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["pat"]["name"], "ci-key");
        assert!(v["token"].as_str().expect("token").starts_with("clpat_"));
    }

    #[tokio::test]
    async fn keys_list_returns_seeded_pats() {
        let (state, shared) = fixture();
        let pat = PatRow::new(
            "pat_001",
            "my-key",
            vec!["cache:read".into()],
            "2026-05-01T00:00:00Z",
            None,
            None,
        );
        shared.seed_pat("t5", pat).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("GET")
            .header("x-corelink-tenant-id", "t5")
            // #336 (rt-nuclear #7): listing credentials requires a cache-write
            // (dashboard `read-write`) scope — a read-only PAT must not enumerate keys.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-token-prefix", "clpat_t5")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["pats"].as_array().expect("pats").len(), 1);
        assert_eq!(v["pats"][0]["name"], "my-key");
    }

    #[tokio::test]
    async fn keys_revoke_returns_revoked_pat() {
        let (state, shared) = fixture();
        let pat = PatRow::new(
            "pat_rev",
            "revoke-me",
            vec![],
            "2026-05-01T00:00:00Z",
            None,
            None,
        );
        shared.seed_pat("t6", pat).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys/pat_rev/revoke")
            .method("POST")
            .header("x-corelink-tenant-id", "t6")
            // #336 (rt-nuclear #7): revoking a credential requires a cache-write
            // (dashboard `read-write`) scope — a read-only PAT must not revoke keys.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-token-prefix", "clpat_t6")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        // revoked_at is now set.
        assert!(v["pat"]["revoked_at"].is_string());
    }

    // ── Team routes ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn team_invite_returns_201_with_member() {
        let (state, _) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "email": "alice@example.com",
            "role": "Developer"
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["member"]["email"], "alice@example.com");
        assert_eq!(v["member"]["role"], "Developer");
        assert_eq!(v["member"]["status"], "invited");
    }

    #[tokio::test]
    async fn team_remove_without_write_scope_is_403() {
        // A read-only caller (no cache-write scope) must NOT remove a seat —
        // the privilege gate fires 403 (intra-tenant member-lockout defense).
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/team/user_victim")
            .method("DELETE")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("x-corelink-scope", "read-only")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn team_remove_with_write_scope_flips_seat() {
        // Seed a member via invite, then remove it with a write-scoped caller.
        let (state, _) = fixture();
        let app = router(state);
        let invite_body = serde_json::to_string(&serde_json::json!({
            "email": "bob@example.com", "role": "Developer"
        }))
        .unwrap();
        let invite = Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("content-type", "application/json")
            .body(Body::from(invite_body))
            .unwrap();
        let iresp = app.clone().oneshot(invite).await.expect("invite oneshot");
        assert_eq!(iresp.status(), StatusCode::CREATED);
        let ibytes = to_bytes(iresp.into_body(), 1 << 20).await.expect("body");
        let iv: serde_json::Value = serde_json::from_slice(&ibytes).expect("json");
        let user_id = iv["member"]["user_id"].as_str().expect("user_id").to_owned();

        let remove = Request::builder()
            .uri(format!("/v1/customer/team/{user_id}"))
            .method("DELETE")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("x-corelink-scope", "read-write")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(remove).await.expect("remove oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["member"]["status"], "removed");
        assert!(v["revoked_pats"].is_number());
    }

    // ── Audit query route ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn audit_route_returns_empty_rows_for_new_tenant() {
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/audit")
            .method("GET")
            // F-018: the audit log carries security/account PII — requires a
            // write-capable (dashboard `read-write` / `cas:rw`) caller.
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-tenant-id", "t8")
            .header("x-corelink-token-prefix", "clpat_t8")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["rows"].as_array().expect("rows").len(), 0);
    }

    // ── Error mapping ─────────────────────────────────────────────────────────

    #[test]
    fn map_err_unauthorized_is_401() {
        let resp = map_err(CustomerHandlerError::Unauthorized("no session".into()));
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn map_err_cross_tenant_is_403() {
        let resp = map_err(CustomerHandlerError::CrossTenantDenied {
            caller: "a".into(),
            requested_tenant: "b".into(),
        });
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn map_err_not_found_is_404() {
        let resp = map_err(CustomerHandlerError::NotFound { what: "x".into() });
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn map_err_audit_failed_is_503() {
        let resp = map_err(CustomerHandlerError::AuditFailed("pipe down".into()));
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn map_err_internal_is_500() {
        let resp = map_err(CustomerHandlerError::Internal("boom".into()));
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn map_err_not_implemented_is_501() {
        // The generic NotImplemented arm maps to an explicit 501. Team
        // invites are now fully implemented (D1 + signup-worker accept);
        // this arm is the defensive fallback for any future unimplemented
        // customer surface, so the copy is generic (not team-specific).
        let resp = map_err(CustomerHandlerError::NotImplemented(
            "some future endpoint".into(),
        ));
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn build_handlers_from_env_falls_back_to_in_memory_without_d1_env() {
        // Dev/CI (no StorageEnv): the env-gated factory must still
        // produce a usable state (InMemory fallback) without panicking.
        // (If a developer machine exports the full StorageEnv this still
        // constructs — the D1 handler does no I/O at build time.)
        let state = build_handlers_from_env();
        let _router = router(state);
    }

    // ── Native PAT possession backstop (cluster A) ────────────────────────────
    //
    // The customer control plane was UN-gated: a leaked PAT_SIGNING_KEY let an
    // attacker HMAC-forge a valid-looking PAT for any tenant, which reached
    // `handle_keys_create` with NO possession check and minted a genuine cas:rw
    // PAT for the victim. These tests prove the gate closes that chain when it
    // is `Some`, preserves dev behavior when `None`, and skips Clerk callers.

    use crate::adapter_pat::PatRow as VerifierPatRow;
    use crate::native_pat_gate::testing::verifier_with_row;
    use crate::native_pat_gate::NativePatGate;
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("32-byte key"))
    }

    /// Mint a real PAT for `tenant_u128`; return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(key: &PatSigningKey, tenant_u128: u128) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant_u128));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant_u128 + 1)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .expect("mint");
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    /// Build a fixture state whose `pat_gate` is wired over a single known PAT
    /// row for `tenant` (Argon2id-verifiable). Mirrors the native-plane tests.
    fn fixture_with_gate(
        token_id: String,
        pat_hash: String,
        tenant: String,
        key: Arc<PatSigningKey>,
    ) -> (CustomerRouteState, Arc<InMemoryCustomerHandler>) {
        let (mut state, shared) = fixture();
        let row = VerifierPatRow {
            tenant_id: tenant,
            pat_hash,
            scope: "cas:rw".to_owned(),
        };
        let verifier = verifier_with_row(token_id, row, key);
        state.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
        (state, shared)
    }

    /// KILLING cluster-A test: a forged-HMAC PAT (no real random secret) is
    /// REJECTED 401 on `POST /v1/customer/keys` when the gate is wired — it can
    /// no longer mint a genuine PAT for the victim tenant. Here D1 stores the
    /// hash of a DIFFERENT secret for the presented token_id, so Argon2id fails.
    #[tokio::test]
    async fn forged_pat_rejected_401_on_keys_create() {
        let key = test_key();
        // The PAT the attacker presents (right HMAC/format)…
        let (pt, tid, _hash, tenant) = mint_pat(&key, 100);
        // …but D1 holds the hash of a DIFFERENT secret ⇒ Argon2id possession
        // check fails (models the forged/leaked-HMAC token).
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 101);
        let (state, _shared) = fixture_with_gate(tid, other_hash, tenant.clone(), key);
        let app = router(state);

        let body = serde_json::to_string(&serde_json::json!({
            "name": "attacker-key",
            "scopes": ["cache:read"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "clpat_forged")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "a forged-HMAC PAT must be rejected by the Argon2id backstop, never mint"
        );
    }

    /// A genuine PAT for its own tenant PASSES the backstop and mints (201).
    #[tokio::test]
    async fn genuine_pat_passes_backstop_and_mints() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 102);
        let (state, _shared) = fixture_with_gate(tid, hash, tenant.clone(), key);
        let app = router(state);

        let body = serde_json::to_string(&serde_json::json!({
            "name": "legit-key",
            "scopes": ["cache:read"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", tenant)
            // rw scope so the (read-only) mint passes the scope gate too.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// A genuine PAT for tenant A presented against tenant B's header is REJECTED
    /// 401 (cross-tenant takeover defense) — the gate binds possession to the
    /// claimed tenant.
    #[tokio::test]
    async fn genuine_pat_for_wrong_tenant_rejected_on_keys_create() {
        let key = test_key();
        let (pt, tid, hash, tenant_a) = mint_pat(&key, 103);
        // Gate is wired for tenant_a's PAT, but the request claims a different
        // tenant in the header (the attacker's victim).
        let (state, _shared) = fixture_with_gate(tid, hash, tenant_a, key);
        let app = router(state);

        let body = serde_json::to_string(&serde_json::json!({
            "name": "x", "scopes": ["cache:read"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "victim-tenant-zzz")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pt}"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Clerk-session callers (edge-verified, no bearer) STILL pass when the gate
    /// is wired — the backstop is skipped for `x-corelink-token-prefix: clerk`.
    #[tokio::test]
    async fn clerk_caller_skips_backstop_and_mints() {
        let key = test_key();
        // Gate wired for some unrelated PAT; the Clerk request carries NO bearer.
        let (_pt, tid, hash, gated_tenant) = mint_pat(&key, 104);
        let (state, _shared) = fixture_with_gate(tid, hash, gated_tenant, key);
        let app = router(state);

        let body = serde_json::to_string(&serde_json::json!({
            "name": "dashboard-key",
            "scopes": ["cache:read", "cache:write"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "dashboard-tenant")
            // Clerk sentinel + the dashboard read-write scope the Worker sets.
            .header("x-corelink-token-prefix", "clerk")
            .header(crate::scope::SCOPE_HEADER, "read-write")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "a Clerk-session caller is edge-verified and must skip the PAT backstop"
        );
    }

    /// `None`-gate preserves the prior dev/CI behavior: no bearer, no PAT gate,
    /// the mint still succeeds (the gate is purely additive in prod).
    #[tokio::test]
    async fn none_gate_preserves_dev_behavior() {
        let (state, _shared) = fixture(); // pat_gate: None
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "dev-key", "scopes": ["cache:read", "cache:write"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "dev-tenant")
            // dev/CI: scope present so the read-write mint passes the scope gate.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // ── Scope gate on mint (cluster A self-escalation) ────────────────────────

    /// A read-only caller (`cas:r`) CANNOT mint a write credential — 403 BEFORE
    /// the mint. This kills the self-escalation chain (a read-only PAT bootstraps
    /// a read-write PAT for itself, which would survive key rotation).
    #[tokio::test]
    async fn read_only_caller_cannot_mint_write_pat() {
        let (state, _shared) = fixture(); // None gate isolates the scope check.
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "escalation", "scopes": ["cache:read", "cache:write"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-only caller MAY still mint a read-only credential (the gate only
    /// blocks privilege ESCALATION, not lateral read-only mints).
    #[tokio::test]
    async fn read_only_caller_may_mint_read_only_pat() {
        let (state, _shared) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "ro-key", "scopes": ["cache:read"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// rt-nuclear cycle-2 #7: a read-only (`cas:r`) caller MUST NOT revoke a
    /// credential — revoking any PAT in the tenant (incl. the owner's) is an
    /// intra-tenant credential-DoS / owner-lockout. The scope gate fires before
    /// any revoke logic, so a fake pat_id still 403s (not 404).
    #[tokio::test]
    async fn read_only_caller_cannot_revoke_credential() {
        let (state, _shared) = fixture(); // None gate isolates the scope check.
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys/some-pat-id/revoke")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// rt-nuclear cycle-2 #7: a read-only caller MUST NOT enumerate the tenant's
    /// credentials (the recon step of the revoke attack + info-disclosure).
    #[tokio::test]
    async fn read_only_caller_cannot_list_credentials() {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("GET")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-write caller CAN mint a write credential (happy path — the scope
    /// gate is a NO-OP for a sufficiently-scoped principal).
    #[tokio::test]
    async fn read_write_caller_can_mint_write_pat() {
        let (state, _shared) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "rw-key", "scopes": ["cache:read", "cache:write"],
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "rw-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// Pure-unit coverage of the `mint_requests_write` classifier (kills mutants
    /// on the token set): write/admin/owner spellings flag; read-only do not.
    #[test]
    fn mint_requests_write_classifier() {
        for s in [
            "cache:write",
            "cas:rw",
            "cas:w",
            "read-write",
            "admin",
            "Owner",
            "WRITE",
        ] {
            assert!(
                mint_requests_write(&[s.to_owned()]),
                "{s:?} must be classified as a write/admin mint"
            );
        }
        for s in ["cache:read", "cas:r", "read-only", "viewer", ""] {
            assert!(
                !mint_requests_write(&[s.to_owned()]),
                "{s:?} must NOT be classified as a write/admin mint"
            );
        }
        // A list containing ANY write token flags.
        assert!(mint_requests_write(&["cache:read".into(), "cache:write".into()]));
    }

    /// A read-only caller cannot invite a privileged role; a non-privileged
    /// invite from a read-only caller is permitted (mirrors the keys-mint gate).
    #[tokio::test]
    async fn read_only_caller_cannot_invite_privileged_role() {
        let (state, _shared) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "email": "evil@example.com", "role": "Owner",
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "ro-tenant")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn role_is_privileged_classifier() {
        assert!(role_is_privileged("Owner"));
        assert!(role_is_privileged("admin"));
        assert!(!role_is_privileged("Developer"));
        assert!(!role_is_privileged("Viewer"));
        assert!(!role_is_privileged(""));
    }

    // ── C-ACCTDEL: account-delete route + requester ───────────────────────────

    use std::sync::Mutex;

    use crate::storage::d1_http::D1Row;

    /// Records the calls it captured; configurable to fail / report NotFound.
    #[derive(Debug, Default)]
    struct MockRequester {
        calls: Mutex<Vec<String>>,
        fail: bool,
        not_found: bool,
    }

    impl AccountDeletionRequester for MockRequester {
        fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError> {
            self.calls.lock().unwrap().push(tenant_id.to_owned());
            if self.not_found {
                return Err(AccountDeletionError::NotFound);
            }
            if self.fail {
                return Err(AccountDeletionError::Internal("boom".to_owned()));
            }
            Ok(())
        }
    }

    fn fixture_with_requester(req: Arc<dyn AccountDeletionRequester>) -> CustomerRouteState {
        let (mut state, _) = fixture();
        state.account_deletion = Some(req);
        state
    }

    #[tokio::test]
    async fn account_delete_clerk_session_returns_202() {
        let requester = Arc::new(MockRequester::default());
        let app = router(fixture_with_requester(requester.clone()));
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clerk")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "confirm": true }).to_string()))
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(requester.calls.lock().unwrap().as_slice(), ["t-acct"]);
    }

    #[tokio::test]
    async fn account_delete_pat_caller_is_403() {
        // A cache PAT (non-`clerk` prefix) must NOT trigger account erasure.
        let requester = Arc::new(MockRequester::default());
        let app = router(fixture_with_requester(requester.clone()));
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clpat_abc")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(
            requester.calls.lock().unwrap().is_empty(),
            "a PAT caller must never reach the requester"
        );
    }

    #[tokio::test]
    async fn account_delete_missing_tenant_is_401() {
        let app = router(fixture_with_requester(Arc::new(MockRequester::default())));
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-token-prefix", "clerk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn account_delete_unwired_fails_closed_503() {
        // account_deletion = None (dev/CI) → fail-CLOSED, never a silent 202.
        let (state, _) = fixture();
        let app = router(state);
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clerk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn account_delete_internal_error_is_500() {
        let requester = Arc::new(MockRequester {
            fail: true,
            ..MockRequester::default()
        });
        let app = router(fixture_with_requester(requester));
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clerk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn account_delete_no_account_is_idempotent_202() {
        let requester = Arc::new(MockRequester {
            not_found: true,
            ..MockRequester::default()
        });
        let app = router(fixture_with_requester(requester));
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clerk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(r).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[test]
    fn deterministic_dsr_id_is_v5_shaped_and_stable() {
        let id = deterministic_dsr_id("user_2abc");
        // Canonical UUID shape, version nibble = 5, RFC-4122 variant (8/9/a/b).
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        assert_eq!(&parts[2][0..1], "5", "version 5 nibble");
        assert!(matches!(&parts[3][0..1], "8" | "9" | "a" | "b"), "RFC-4122 variant");
        // Deterministic.
        assert_eq!(id, deterministic_dsr_id("user_2abc"));
        assert_ne!(id, deterministic_dsr_id("user_other"));
    }

    /// Hermetic D1 mock: canned rows keyed by an SQL fragment; records writes.
    #[derive(Debug, Default)]
    struct MockReqD1 {
        tenant_rows: Vec<D1Row>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
    }

    impl crate::customer_d1::CustomerD1 for MockReqD1 {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            self.calls.lock().unwrap().push((sql.to_owned(), binds));
            if sql.contains("FROM tenant WHERE tenant_id") {
                return Ok(self.tenant_rows.clone());
            }
            Ok(Vec::new())
        }
    }

    #[derive(Debug, Default)]
    struct MockSink {
        last: Mutex<Option<Value>>,
    }

    impl DsrErasureSink for MockSink {
        fn enqueue(&self, message: &Value) -> Result<(), String> {
            *self.last.lock().unwrap() = Some(message.clone());
            Ok(())
        }
    }

    fn d1row(pairs: &[(&str, Value)]) -> D1Row {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
    }

    // The requester reads the process-global `ERASURE_SALT_KEY`; all three
    // behaviors are sequenced in ONE test so a parallel sibling can never observe
    // a half-mutated env (the rest of the suite never touches this var).
    #[test]
    fn requester_dsr_anchor_message_and_salt_failclosed() {
        // Phase A: salt key SET → success; anchor written BEFORE enqueue; the
        // enqueued message mirrors buildErasureQueueMessage's shape.
        std::env::set_var("ERASURE_SALT_KEY", "test-erasure-salt-key-0123456789abcdef");
        let db = Arc::new(MockReqD1 {
            tenant_rows: vec![d1row(&[("clerk_user_id", json!("user_2abc"))])],
            ..MockReqD1::default()
        });
        let sink = Arc::new(MockSink::default());
        let requester = D1AccountDeletionRequester::new(db.clone(), sink.clone());
        requester.request_erasure("t-acct").expect("must succeed");

        let calls = db.calls.lock().unwrap().clone();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT OR IGNORE INTO dsr_requested"))
            .expect("dsr_requested INSERT must run");
        let expected_dsr = deterministic_dsr_id("user_2abc");
        assert_eq!(insert.1[0], json!(expected_dsr));
        assert_eq!(insert.1[1], json!("t-acct"));

        let msg = sink.last.lock().unwrap().clone().expect("a message was enqueued");
        assert_eq!(msg["schema"], DSR_QUEUED_SCHEMA);
        assert_eq!(msg["dsr_id"], json!(expected_dsr));
        assert_eq!(msg["tenant_id"], "t-acct");
        assert_eq!(msg["subject_id"], "t-acct");
        assert_eq!(msg["source"], "customer.account.delete");
        assert_eq!(msg["legal_hold"], json!(false));
        assert!(msg["erasure_salt_hex"].as_str().is_some_and(|s| s.len() == 64));

        // Phase B: no tenant row → NotFound, no enqueue.
        let db2 = Arc::new(MockReqD1::default());
        let sink2 = Arc::new(MockSink::default());
        let r2 = D1AccountDeletionRequester::new(db2, sink2.clone());
        assert!(matches!(
            r2.request_erasure("ghost"),
            Err(AccountDeletionError::NotFound)
        ));
        assert!(sink2.last.lock().unwrap().is_none(), "no enqueue on NotFound");

        // Phase C: salt key UNSET → fail-CLOSED (Internal), no enqueue.
        std::env::remove_var("ERASURE_SALT_KEY");
        let db3 = Arc::new(MockReqD1 {
            tenant_rows: vec![d1row(&[("clerk_user_id", json!("user_2abc"))])],
            ..MockReqD1::default()
        });
        let sink3 = Arc::new(MockSink::default());
        let r3 = D1AccountDeletionRequester::new(db3, sink3.clone());
        assert!(matches!(
            r3.request_erasure("t-acct"),
            Err(AccountDeletionError::Internal(_))
        ));
        assert!(
            sink3.last.lock().unwrap().is_none(),
            "no enqueue when the salt key is unset (fail-CLOSED)"
        );
    }
}
