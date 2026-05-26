//! Pilot admin HTTP routes (Wave-29 stream-3 wire-up of the wave-27
//! placeholder scripts).
//!
//! Replaces the three wave-27 placeholder scripts
//! (`scripts/admin/list-pilot-tenants.sh`, `grant-pilot-tier.sh`,
//! `pilot-24h-checkin.sh`) with proper admin endpoints that the
//! operator-facing Docusaurus admin page consumes via fetch.
//!
//! # Surface
//!
//! - `GET  /v1/admin/pilots?state=<NEW|ACTIVE|GRADUATED|TERMINATED>`
//!   — paginated list of pilot tenants filtered by lifecycle state.
//! - `POST /v1/admin/pilots/{tenant_id}/grant-tier` — grant pilot
//!   tier (`tier`, `cap_bytes`). Mirrors `grant-pilot-tier.sh`
//!   semantics, including the `pilot_state` NEW → ACTIVE transition.
//! - `POST /v1/admin/pilots/{tenant_id}/checkin` — run the 24h
//!   activation check-in. Mirrors `pilot-24h-checkin.sh`: emits an
//!   alert row when the tenant has not uploaded a single blob 24h
//!   after `tier_granted_at_ms`.
//!
//! # Auth model
//!
//! Every route is gated by the admin JWT scope
//! `corelink:admin:pilots`. Production wiring (matches the wave-15
//! admin handler discipline) slots a tower middleware in front that:
//!
//!   1. Validates the Clerk session JWT signature + expiry.
//!   2. Extracts the `scope` claim and pins it on the request via
//!      `X-Admin-Scope` (canonical lowercase, one or more
//!      space-separated scopes).
//!   3. Extracts the admin principal id and pins it on the request
//!      via `X-Admin-Principal`.
//!
//! The routes here re-check the scope at the route boundary (5-Layer
//! Defense — never trust the middleware exclusively) and emit a
//! `corelink.security.admin_pilot_unauthorized.v1` audit row BEFORE
//! the 403 when a non-admin principal probes the surface.
//!
//! ## 5-Layer Defense
//!
//! Per S-03 + wave-15 admin handler:
//!
//!   L1: tower middleware (JWT signature + expiry)            — out of scope here
//!   L2: scope re-check at the route boundary                 — `require_admin_scope`
//!   L3: tenant-scoped authorization                          — `PilotAdminScope::allows_tenant`
//!   L4: input validation                                     — `parse_tenant_uuid` + `parse_state`
//!   L5: fail-CLOSED audit emit BEFORE every state mutation   — `emit_or_503`
//!
//! # Fail-CLOSED audit ordering
//!
//! Every state mutation emits a `corelink.admin.pilot_*.v1` audit
//! row BEFORE the mutation lands. If the audit sink returns `Err`
//! the route aborts with `503 Service Unavailable` and the
//! mutation is NOT applied. This mirrors the
//! `corelink-handler-admin` `MutateAttempted` → `MutateCommitted`
//! ordering.
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]` (crate-wide via `lib.rs`).
//! - No `unwrap()` / `expect()` / `panic!()` on the request path —
//!   all `Result`s map to a typed error and an HTTP response.
//! - DCO sign-off + Co-Authored-By land at commit time.

use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Canonical list-pilots route path.
pub const PILOTS_LIST_ROUTE: &str = "/v1/admin/pilots";

/// Canonical grant-tier route path (axum 0.7 `:name` capture).
pub const PILOTS_GRANT_TIER_ROUTE: &str = "/v1/admin/pilots/:tenant_id/grant-tier";

/// Canonical checkin route path (axum 0.7 `:name` capture).
pub const PILOTS_CHECKIN_ROUTE: &str = "/v1/admin/pilots/:tenant_id/checkin";

/// HTTP header carrying the operator-validated admin scope claim
/// (production: set by the JWT-validating tower middleware).
pub const ADMIN_SCOPE_HEADER: &str = "x-admin-scope";

/// HTTP header carrying the operator-validated admin principal id
/// (production: set by the JWT-validating tower middleware).
pub const ADMIN_PRINCIPAL_HEADER: &str = "x-admin-principal";

/// HTTP header used by the legacy admin handler (kept for parity in
/// tests / cross-tenant scope assertion).
pub const ADMIN_TENANT_HEADER: &str = "x-admin-tenant";

/// Required admin scope literal.
pub const REQUIRED_ADMIN_SCOPE: &str = "corelink:admin:pilots";

/// Audit event type — emitted on `grant-tier` success.
pub const EVENT_TYPE_TIER_GRANTED: &str = "corelink.admin.pilot_tier_granted.v1";

/// Audit event type — emitted on `checkin` execution (alert or healthy).
pub const EVENT_TYPE_CHECKIN: &str = "corelink.admin.pilot_checkin.v1";

/// Audit event type — emitted BEFORE 403 on missing/wrong scope.
pub const EVENT_TYPE_UNAUTHORIZED: &str = "corelink.security.admin_pilot_unauthorized.v1";

/// Audit event type — emitted BEFORE 403 on cross-tenant attempt.
pub const EVENT_TYPE_CROSS_TENANT: &str = "corelink.security.admin_pilot_cross_tenant.v1";

/// 24h window (in milliseconds) for the activation check-in.
pub const CHECKIN_WINDOW_MS: u64 = 86_400_000;

/// Default page size for the `GET /v1/admin/pilots` listing.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// Hard cap on page size — defends against unbounded listing.
pub const MAX_PAGE_SIZE: usize = 200;

// -----------------------------------------------------------------------------
// State enum
// -----------------------------------------------------------------------------

/// Pilot lifecycle state — mirrors the `pilot_state` column on the
/// D1 `tenants` row (wave-27 schema).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PilotState {
    /// Signup token redeemed; tier not yet granted.
    New,
    /// Reserved for cap-warning rollout (alias for NEW pre-grant).
    Reserved,
    /// Pre-grant provisioning step finished; ready for grant.
    Provisioned,
    /// Pilot tier granted; tenant within the 90-day pilot window.
    Active,
    /// Pilot succeeded; tenant transitioned to a paid plan.
    Graduated,
    /// Pilot offboarded.
    Terminated,
}

impl PilotState {
    /// Parse a state from its canonical uppercase string form.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "NEW" => Some(Self::New),
            "RESERVED" => Some(Self::Reserved),
            "PROVISIONED" => Some(Self::Provisioned),
            "ACTIVE" => Some(Self::Active),
            "GRADUATED" => Some(Self::Graduated),
            "TERMINATED" => Some(Self::Terminated),
            _ => None,
        }
    }

    /// Canonical uppercase string form.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::New => "NEW",
            Self::Reserved => "RESERVED",
            Self::Provisioned => "PROVISIONED",
            Self::Active => "ACTIVE",
            Self::Graduated => "GRADUATED",
            Self::Terminated => "TERMINATED",
        }
    }
}

impl fmt::Display for PilotState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// -----------------------------------------------------------------------------
// Tenant record
// -----------------------------------------------------------------------------

/// Per-tenant pilot record — the canonical JSON shape returned by
/// `GET /v1/admin/pilots` and reused as the input for grant-tier +
/// checkin (mirrors `apps/server/src/admin/tier_list.rs::TenantSummary`
/// from the wave-27 placeholder script comments).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PilotTenant {
    /// Canonical tenant UUID.
    pub tenant_id: Uuid,
    /// Display slug (lowercase, hyphen-delimited).
    pub slug: String,
    /// Tier — pre-grant tenants carry `"free"`, post-grant `"pilot"`.
    pub tier: String,
    /// Storage cap in bytes (decimal-GB convention).
    pub cap_bytes: u64,
    /// Pilot lifecycle state.
    pub pilot_state: PilotState,
    /// Signup completion timestamp (ms-epoch).
    pub signup_at_ms: u64,
    /// Tier-grant timestamp (ms-epoch); `None` pre-grant.
    pub tier_granted_at_ms: Option<u64>,
    /// First-blob timestamp (ms-epoch); `None` if the tenant has not
    /// yet uploaded a single blob.
    pub first_blob_at_ms: Option<u64>,
}

// -----------------------------------------------------------------------------
// Audit row + sink
// -----------------------------------------------------------------------------

/// Canonical audit row emitted by every pilot-admin route.
///
/// The shape matches the wave-15 admin-handler audit envelope: one
/// row per route entry, fail-CLOSED if the sink returns `Err`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PilotAuditRow {
    /// Canonical event type (one of the `EVENT_TYPE_*` constants).
    pub event_type: String,
    /// Admin principal id (from the validated JWT).
    pub principal: String,
    /// Target tenant id (None for the list route).
    pub tenant_id: Option<Uuid>,
    /// Emit timestamp (ms-epoch).
    pub at_unix_ms: u64,
    /// Exit status — `ok`, `forbidden`, `not_found`, `bad_request`,
    /// `audit_closed`. The list of statuses is intentionally small
    /// for the SEC team paging anchor.
    pub exit_status: String,
    /// Free-form per-event payload (e.g. `tier`, `cap_bytes`,
    /// `alert_emitted`).
    pub payload: serde_json::Value,
}

/// Trait surface for the pilot-admin audit sink.
///
/// Production wiring binds an `Arc<dyn PilotAuditSink>` backed by
/// the canonical audit-chain producer; tests bind
/// [`InMemoryPilotAuditSink`].
pub trait PilotAuditSink: fmt::Debug + Send + Sync {
    /// Emit a single audit row. Fail-CLOSED: if `Err`, the route
    /// aborts with `503` and the mutation is NOT applied.
    ///
    /// # Errors
    ///
    /// Returns a static `&'static str` when the underlying sink is
    /// unavailable. The string is forwarded to the audit team as the
    /// failure reason.
    fn emit(&self, row: PilotAuditRow) -> Result<(), &'static str>;
}

/// In-memory audit sink used by tests + dev/CI.
#[derive(Debug, Default)]
pub struct InMemoryPilotAuditSink {
    rows: Mutex<Vec<PilotAuditRow>>,
    fail_with: Mutex<Option<&'static str>>,
}

impl InMemoryPilotAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a failure that subsequent `emit` calls will return.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn inject_failure(&self, reason: &'static str) -> Result<(), &'static str> {
        let mut guard = self.fail_with.lock().map_err(|_| "audit-sink poisoned")?;
        *guard = Some(reason);
        Ok(())
    }

    /// Snapshot every emitted audit row.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<PilotAuditRow>, &'static str> {
        let guard = self.rows.lock().map_err(|_| "audit-sink poisoned")?;
        Ok(guard.clone())
    }
}

impl PilotAuditSink for InMemoryPilotAuditSink {
    fn emit(&self, row: PilotAuditRow) -> Result<(), &'static str> {
        let fail = match self.fail_with.lock() {
            Ok(g) => *g,
            Err(_) => return Err("audit-sink poisoned"),
        };
        if let Some(reason) = fail {
            return Err(reason);
        }
        let mut guard = self.rows.lock().map_err(|_| "audit-sink poisoned")?;
        guard.push(row);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Tenant store
// -----------------------------------------------------------------------------

/// Trait surface for the pilot-tenant store.
///
/// Production wiring binds against the D1 `tenants` table; tests bind
/// [`InMemoryPilotStore`].
pub trait PilotStore: fmt::Debug + Send + Sync {
    /// List pilot tenants matching `state`, ordered by `signup_at_ms`
    /// ASC, with a hard cap at `limit` rows after skipping `offset`.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the underlying store is unavailable.
    fn list_by_state(
        &self,
        state: PilotState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<PilotTenant>, &'static str>;

    /// Lookup a single tenant by id.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the underlying store is unavailable
    /// (distinct from `Ok(None)` for "not found").
    fn get(&self, tenant_id: Uuid) -> Result<Option<PilotTenant>, &'static str>;

    /// Apply a grant-tier mutation: stamp `tier`, `cap_bytes`,
    /// `tier_granted_at_ms`, transition `pilot_state` → ACTIVE.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the underlying store is unavailable
    /// or the tenant is not in a grant-eligible state.
    fn apply_grant_tier(
        &self,
        tenant_id: Uuid,
        tier: &str,
        cap_bytes: u64,
        granted_at_ms: u64,
    ) -> Result<PilotTenant, &'static str>;
}

/// In-memory pilot-tenant store — used by tests + dev/CI.
#[derive(Debug, Default)]
pub struct InMemoryPilotStore {
    by_id: Mutex<HashMap<Uuid, PilotTenant>>,
    fail_with: Mutex<Option<&'static str>>,
}

impl InMemoryPilotStore {
    /// Construct an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a tenant into the store.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn seed(&self, tenant: PilotTenant) -> Result<(), &'static str> {
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        guard.insert(tenant.tenant_id, tenant);
        Ok(())
    }

    /// Inject a failure that subsequent calls will return.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn inject_failure(&self, reason: &'static str) -> Result<(), &'static str> {
        let mut guard = self.fail_with.lock().map_err(|_| "store poisoned")?;
        *guard = Some(reason);
        Ok(())
    }
}

impl PilotStore for InMemoryPilotStore {
    fn list_by_state(
        &self,
        state: PilotState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<PilotTenant>, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        let mut rows: Vec<PilotTenant> = guard
            .values()
            .filter(|t| t.pilot_state == state)
            .cloned()
            .collect();
        rows.sort_by_key(|t| t.signup_at_ms);
        let end = offset.saturating_add(limit).min(rows.len());
        let start = offset.min(rows.len());
        Ok(rows.get(start..end).map(<[_]>::to_vec).unwrap_or_default())
    }

    fn get(&self, tenant_id: Uuid) -> Result<Option<PilotTenant>, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        Ok(guard.get(&tenant_id).cloned())
    }

    fn apply_grant_tier(
        &self,
        tenant_id: Uuid,
        tier: &str,
        cap_bytes: u64,
        granted_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        let tenant = guard.get_mut(&tenant_id).ok_or("tenant not found")?;
        if !matches!(
            tenant.pilot_state,
            PilotState::New | PilotState::Reserved | PilotState::Provisioned
        ) {
            return Err("tenant not in grant-eligible state");
        }
        tenant.tier = tier.to_string();
        tenant.cap_bytes = cap_bytes;
        tenant.pilot_state = PilotState::Active;
        tenant.tier_granted_at_ms = Some(granted_at_ms);
        Ok(tenant.clone())
    }
}

// -----------------------------------------------------------------------------
// Route state + scope guard
// -----------------------------------------------------------------------------

/// Shared route state — store + audit sink + wall clock.
#[derive(Clone)]
pub struct PilotAdminRouteState {
    /// Tenant store (production: D1-backed; tests: in-memory).
    pub store: Arc<dyn PilotStore>,
    /// Audit sink (production: audit-chain producer; tests:
    /// in-memory).
    pub audit_sink: Arc<dyn PilotAuditSink>,
    /// Wall clock (production: `SystemWallClock`; tests:
    /// `InMemoryFakeWallClock`).
    pub wall_clock: Arc<dyn crate::wall_clock::WallClock>,
}

impl fmt::Debug for PilotAdminRouteState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PilotAdminRouteState").finish_non_exhaustive()
    }
}

/// Parsed admin scope claim — extracted from the
/// `X-Admin-Scope` header by [`require_admin_scope`].
#[derive(Clone, Debug)]
pub struct PilotAdminScope {
    /// Validated admin principal id.
    pub principal: String,
    /// Tenant-scope bound to the admin claim, if any. `None` ==
    /// global pilot-admin (operator). When present, the admin's
    /// authority is limited to the bound tenant — cross-tenant
    /// probes emit `EVENT_TYPE_CROSS_TENANT` BEFORE the 403.
    pub bound_tenant: Option<Uuid>,
}

impl PilotAdminScope {
    /// Returns `true` if this scope is allowed to operate on
    /// `target_tenant`. Global admins (no `bound_tenant`) are
    /// allowed; tenant-scoped admins must match exactly.
    #[must_use]
    pub fn allows_tenant(&self, target_tenant: Uuid) -> bool {
        match self.bound_tenant {
            None => true,
            Some(bound) => bound == target_tenant,
        }
    }
}

// -----------------------------------------------------------------------------
// Query + body shapes
// -----------------------------------------------------------------------------

/// Query string for `GET /v1/admin/pilots`.
#[derive(Clone, Debug, Deserialize)]
pub struct ListPilotsQuery {
    /// Required state filter (uppercase canonical form).
    pub state: String,
    /// Optional 0-based offset (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Optional page size (default `DEFAULT_PAGE_SIZE`, max
    /// `MAX_PAGE_SIZE`).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response body for `GET /v1/admin/pilots`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListPilotsResponse {
    /// Echoed state filter.
    pub state: PilotState,
    /// Returned rows.
    pub rows: Vec<PilotTenant>,
    /// 0-based offset applied.
    pub offset: usize,
    /// Page size applied.
    pub limit: usize,
}

/// Request body for `POST /v1/admin/pilots/{tenant_id}/grant-tier`.
#[derive(Clone, Debug, Deserialize)]
pub struct GrantTierBody {
    /// Tier to grant — MUST be `"pilot"` (parity with the wave-27
    /// `grant-pilot-tier.sh` script).
    pub tier: String,
    /// Storage cap in bytes (decimal-GB convention).
    pub cap_bytes: u64,
}

/// Response body for `POST /v1/admin/pilots/{tenant_id}/grant-tier`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrantTierResponse {
    /// Post-mutation tenant record.
    pub tenant: PilotTenant,
}

/// Response body for `POST /v1/admin/pilots/{tenant_id}/checkin`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckinResponse {
    /// Tenant id checked.
    pub tenant_id: Uuid,
    /// `true` when the tenant is in the `no-blob silent-fail`
    /// window (24h post-grant with `first_blob_at_ms = None`).
    pub alert_emitted: bool,
    /// Time-since-grant in ms; `None` pre-grant.
    pub age_ms: Option<u64>,
}

// -----------------------------------------------------------------------------
// Router builder
// -----------------------------------------------------------------------------

/// Build the axum sub-router for the pilot-admin surface.
pub fn router(state: PilotAdminRouteState) -> Router {
    Router::new()
        .route(PILOTS_LIST_ROUTE, get(handle_list))
        .route(PILOTS_GRANT_TIER_ROUTE, post(handle_grant_tier))
        .route(PILOTS_CHECKIN_ROUTE, post(handle_checkin))
        .with_state(state)
}

/// Build the canonical
/// `(Arc<dyn PilotStore>, Arc<dyn PilotAuditSink>)` pair backed by
/// in-memory fakes — used by dev/CI + the default `build()` path in
/// `routes.rs`.
#[must_use]
pub fn build_handlers() -> (Arc<dyn PilotStore>, Arc<dyn PilotAuditSink>) {
    let store: Arc<dyn PilotStore> = Arc::new(InMemoryPilotStore::new());
    let audit: Arc<dyn PilotAuditSink> = Arc::new(InMemoryPilotAuditSink::new());
    (store, audit)
}

// -----------------------------------------------------------------------------
// Handlers
// -----------------------------------------------------------------------------

async fn handle_list(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Query(query): Query<ListPilotsQuery>,
) -> Response {
    let scope = match require_admin_scope(&state, &headers, None) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let parsed_state = match PilotState::parse(query.state.as_str()) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "invalid state filter").into_response(),
    };
    let offset = query.offset.unwrap_or(0);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let rows = match state.store.list_by_state(parsed_state, offset, limit) {
        Ok(r) => r,
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "store unavailable").into_response(),
    };
    let now_ms = state.wall_clock.now_ms();
    let payload = serde_json::json!({
        "state": parsed_state.as_str(),
        "rows_returned": rows.len(),
        "offset": offset,
        "limit": limit,
    });
    let row = PilotAuditRow {
        event_type: "corelink.admin.pilot_list.v1".to_string(),
        principal: scope.principal,
        tenant_id: None,
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload,
    };
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    let body = ListPilotsResponse {
        state: parsed_state,
        rows,
        offset,
        limit,
    };
    (StatusCode::OK, Json(body)).into_response()
}

async fn handle_grant_tier(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<GrantTierBody>,
) -> Response {
    let tenant_uuid = match parse_tenant_uuid(&tenant_id) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let scope = match require_admin_scope(&state, &headers, Some(tenant_uuid)) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if body.tier != "pilot" {
        return (StatusCode::BAD_REQUEST, "tier must be 'pilot'").into_response();
    }
    if body.cap_bytes == 0 {
        return (StatusCode::BAD_REQUEST, "cap_bytes must be > 0").into_response();
    }
    let now_ms = state.wall_clock.now_ms();

    // Fail-CLOSED: emit audit BEFORE mutation. If audit fails, abort.
    let pre_row = PilotAuditRow {
        event_type: EVENT_TYPE_TIER_GRANTED.to_string(),
        principal: scope.principal.clone(),
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: "attempt".to_string(),
        payload: serde_json::json!({
            "tier": body.tier,
            "cap_bytes": body.cap_bytes,
        }),
    };
    if state.audit_sink.emit(pre_row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    let tenant = match state
        .store
        .apply_grant_tier(tenant_uuid, &body.tier, body.cap_bytes, now_ms)
    {
        Ok(t) => t,
        Err(msg) => {
            let status = if msg == "tenant not found" {
                StatusCode::NOT_FOUND
            } else if msg == "tenant not in grant-eligible state" {
                StatusCode::CONFLICT
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            return (status, msg).into_response();
        }
    };

    let post_row = PilotAuditRow {
        event_type: EVENT_TYPE_TIER_GRANTED.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload: serde_json::json!({
            "tier": tenant.tier,
            "cap_bytes": tenant.cap_bytes,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(post_row).is_err() {
        // The mutation already landed in the store, but the
        // post-emit failed: surface 503 so the operator retries.
        // The post-emit failure is the SEC team's hard signal that
        // the audit pipeline is degraded; the pre-emit landed so
        // the SEC team still has the `attempt` row.
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    (StatusCode::OK, Json(GrantTierResponse { tenant })).into_response()
}

async fn handle_checkin(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Response {
    let tenant_uuid = match parse_tenant_uuid(&tenant_id) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let scope = match require_admin_scope(&state, &headers, Some(tenant_uuid)) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let tenant = match state.store.get(tenant_uuid) {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "tenant not found").into_response(),
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "store unavailable").into_response(),
    };
    let now_ms = state.wall_clock.now_ms();
    let (alert, age_ms) = match tenant.tier_granted_at_ms {
        Some(granted_at) => {
            let age = now_ms.saturating_sub(granted_at);
            let alert = tenant.pilot_state == PilotState::Active
                && age >= CHECKIN_WINDOW_MS
                && tenant.first_blob_at_ms.is_none();
            (alert, Some(age))
        }
        None => (false, None),
    };

    let row = PilotAuditRow {
        event_type: EVENT_TYPE_CHECKIN.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_uuid),
        at_unix_ms: now_ms,
        exit_status: if alert {
            "alert".to_string()
        } else {
            "ok".to_string()
        },
        payload: serde_json::json!({
            "alert_emitted": alert,
            "age_ms": age_ms,
            "first_blob_at_ms": tenant.first_blob_at_ms,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    (
        StatusCode::OK,
        Json(CheckinResponse {
            tenant_id: tenant_uuid,
            alert_emitted: alert,
            age_ms,
        }),
    )
        .into_response()
}

// -----------------------------------------------------------------------------
// 5-Layer Defense helpers
// -----------------------------------------------------------------------------

/// Parse + validate the `{tenant_id}` path capture as a lowercase
/// hyphenated UUID.
#[allow(clippy::result_large_err)]
fn parse_tenant_uuid(raw: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(raw)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid tenant_id").into_response())
}

/// L2 + L3 + L5: enforce that the caller carries
/// `corelink:admin:pilots` in the validated scope claim, and that
/// (if tenant-scoped) the bound tenant matches the target tenant.
/// On failure: emit the canonical audit row BEFORE returning the
/// 403/401/503.
///
/// Returns the parsed [`PilotAdminScope`] on success; otherwise a
/// fully-formed `Response`.
#[allow(clippy::result_large_err)]
fn require_admin_scope(
    state: &PilotAdminRouteState,
    headers: &HeaderMap,
    target_tenant: Option<Uuid>,
) -> Result<PilotAdminScope, Response> {
    let now_ms = state.wall_clock.now_ms();
    let principal = headers
        .get(ADMIN_PRINCIPAL_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let scope_header = headers
        .get(ADMIN_SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let has_scope = scope_header
        .split_whitespace()
        .any(|s| s == REQUIRED_ADMIN_SCOPE);
    if principal.is_empty() || !has_scope {
        let row = PilotAuditRow {
            event_type: EVENT_TYPE_UNAUTHORIZED.to_string(),
            principal: principal.to_string(),
            tenant_id: target_tenant,
            at_unix_ms: now_ms,
            exit_status: "forbidden".to_string(),
            payload: serde_json::json!({
                "reason": if principal.is_empty() {
                    "missing X-Admin-Principal"
                } else {
                    "missing corelink:admin:pilots scope"
                },
            }),
        };
        return Err(emit_or_503(
            state,
            row,
            (StatusCode::FORBIDDEN, "admin scope required").into_response(),
        ));
    }
    let bound_tenant = headers
        .get(ADMIN_TENANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| Uuid::parse_str(s).ok());
    let scope = PilotAdminScope {
        principal: principal.to_string(),
        bound_tenant,
    };
    if let Some(target) = target_tenant {
        if !scope.allows_tenant(target) {
            let row = PilotAuditRow {
                event_type: EVENT_TYPE_CROSS_TENANT.to_string(),
                principal: scope.principal.clone(),
                tenant_id: Some(target),
                at_unix_ms: now_ms,
                exit_status: "forbidden".to_string(),
                payload: serde_json::json!({
                    "bound_tenant": scope.bound_tenant.map(|u| u.to_string()),
                    "target_tenant": target.to_string(),
                }),
            };
            return Err(emit_or_503(
                state,
                row,
                (StatusCode::FORBIDDEN, "cross-tenant admin probe").into_response(),
            ));
        }
    }
    Ok(scope)
}

/// Emit an audit row; on failure, return a `503` response (the
/// caller's intended response is discarded). Mirrors the wave-20
/// closure pattern from `audit_export.rs`.
fn emit_or_503(
    state: &PilotAdminRouteState,
    row: PilotAuditRow,
    happy: Response,
) -> Response {
    match state.audit_sink.emit(row) {
        Ok(()) => happy,
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response(),
    }
}

// -----------------------------------------------------------------------------
// Tests (unit; integration tests live in apps/server/tests/admin_pilot.rs)
// -----------------------------------------------------------------------------

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
    use crate::wall_clock::InMemoryFakeWallClock;

    fn sample_tenant(state: PilotState, signup_at: u64) -> PilotTenant {
        PilotTenant {
            tenant_id: Uuid::now_v7(),
            slug: "acme-builds".to_string(),
            tier: "free".to_string(),
            cap_bytes: 0,
            pilot_state: state,
            signup_at_ms: signup_at,
            tier_granted_at_ms: None,
            first_blob_at_ms: None,
        }
    }

    fn fixture() -> (
        PilotAdminRouteState,
        Arc<InMemoryPilotStore>,
        Arc<InMemoryPilotAuditSink>,
        Arc<InMemoryFakeWallClock>,
    ) {
        let store = Arc::new(InMemoryPilotStore::new());
        let audit = Arc::new(InMemoryPilotAuditSink::new());
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let state = PilotAdminRouteState {
            store: store.clone() as Arc<dyn PilotStore>,
            audit_sink: audit.clone() as Arc<dyn PilotAuditSink>,
            wall_clock: clock.clone() as Arc<dyn crate::wall_clock::WallClock>,
        };
        (state, store, audit, clock)
    }

    #[test]
    fn pilot_state_round_trip_through_canonical_strings() {
        for s in [
            PilotState::New,
            PilotState::Reserved,
            PilotState::Provisioned,
            PilotState::Active,
            PilotState::Graduated,
            PilotState::Terminated,
        ] {
            let txt = s.as_str();
            let parsed = PilotState::parse(txt).expect("round trip");
            assert_eq!(parsed, s);
        }
        assert!(PilotState::parse("BOGUS").is_none());
    }

    #[test]
    fn route_constants_match_canonical_paths() {
        assert_eq!(PILOTS_LIST_ROUTE, "/v1/admin/pilots");
        assert_eq!(
            PILOTS_GRANT_TIER_ROUTE,
            "/v1/admin/pilots/:tenant_id/grant-tier"
        );
        assert_eq!(PILOTS_CHECKIN_ROUTE, "/v1/admin/pilots/:tenant_id/checkin");
    }

    #[test]
    fn in_memory_store_list_filters_by_state_and_orders_by_signup() {
        let (_st, store, _au, _c) = fixture();
        let mut a = sample_tenant(PilotState::New, 1_000);
        let mut b = sample_tenant(PilotState::Active, 2_000);
        let mut c = sample_tenant(PilotState::New, 500);
        a.slug = "a".into();
        b.slug = "b".into();
        c.slug = "c".into();
        store.seed(a.clone()).expect("seed");
        store.seed(b.clone()).expect("seed");
        store.seed(c.clone()).expect("seed");
        let rows = store
            .list_by_state(PilotState::New, 0, 50)
            .expect("list");
        assert_eq!(rows.len(), 2);
        // Sorted by signup_at_ms ASC.
        assert_eq!(rows[0].slug, "c");
        assert_eq!(rows[1].slug, "a");
    }

    #[test]
    fn in_memory_store_grant_tier_transitions_new_to_active() {
        let (_st, store, _au, _c) = fixture();
        let t = sample_tenant(PilotState::New, 1_000);
        let id = t.tenant_id;
        store.seed(t).expect("seed");
        let updated = store
            .apply_grant_tier(id, "pilot", 100_000_000_000, 5_000)
            .expect("grant");
        assert_eq!(updated.pilot_state, PilotState::Active);
        assert_eq!(updated.tier, "pilot");
        assert_eq!(updated.cap_bytes, 100_000_000_000);
        assert_eq!(updated.tier_granted_at_ms, Some(5_000));
    }

    #[test]
    fn in_memory_store_grant_tier_rejects_already_active() {
        let (_st, store, _au, _c) = fixture();
        let t = sample_tenant(PilotState::Active, 1_000);
        let id = t.tenant_id;
        store.seed(t).expect("seed");
        let err = store
            .apply_grant_tier(id, "pilot", 1, 5_000)
            .expect_err("double grant");
        assert!(err.contains("not in grant-eligible state"));
    }

    #[test]
    fn admin_scope_allows_tenant_when_global() {
        let scope = PilotAdminScope {
            principal: "ops@root".into(),
            bound_tenant: None,
        };
        assert!(scope.allows_tenant(Uuid::now_v7()));
    }

    #[test]
    fn admin_scope_rejects_cross_tenant_when_bound() {
        let bound = Uuid::now_v7();
        let target = Uuid::now_v7();
        let scope = PilotAdminScope {
            principal: "ops@tenant".into(),
            bound_tenant: Some(bound),
        };
        assert!(scope.allows_tenant(bound));
        assert!(!scope.allows_tenant(target));
    }

    #[test]
    fn audit_sink_inject_failure_surfaces_err() {
        let sink = InMemoryPilotAuditSink::new();
        sink.inject_failure("audit pipeline down").expect("inject");
        let err = sink
            .emit(PilotAuditRow {
                event_type: "x".into(),
                principal: "p".into(),
                tenant_id: None,
                at_unix_ms: 0,
                exit_status: "ok".into(),
                payload: serde_json::Value::Null,
            })
            .expect_err("inject");
        assert_eq!(err, "audit pipeline down");
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        let (_store, audit) = build_handlers();
        let err = audit.emit(PilotAuditRow {
            event_type: "x".into(),
            principal: "p".into(),
            tenant_id: None,
            at_unix_ms: 0,
            exit_status: "ok".into(),
            payload: serde_json::Value::Null,
        });
        assert!(err.is_ok());
    }
}
