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
//! - `POST /v1/admin/pilots` — create a new pilot tenant row (slug,
//!   optional initial `cap_bytes`). Mints a fresh tenant UUID, persists
//!   the row in the `NEW` lifecycle state, and returns the created
//!   [`PilotTenant`]. The operator follows up with `grant-tier`.
//! - `POST /v1/admin/pilots/{tenant_id}/grant-tier` — grant pilot
//!   tier (`tier`, `cap_bytes`). Mirrors `grant-pilot-tier.sh`
//!   semantics, including the `pilot_state` NEW → ACTIVE transition.
//! - `POST /v1/admin/pilots/{tenant_id}/checkin` — run the 24h
//!   activation check-in. Mirrors `pilot-24h-checkin.sh`: emits an
//!   alert row when the tenant has not uploaded a single blob 24h
//!   after `tier_granted_at_ms`.
//!
//! ## `/_internal/admin/pilots…` alias (#218 §2.1, ratified Q3)
//!
//! The SAME three handlers are additionally bound under
//! `/_internal/admin/pilots…` so the surface is operator-reachable
//! from the public edge with ZERO Worker changes: the Worker's
//! existing `/_internal/*` route-kind already does constant-time
//! internal-auth verification + strip-then-reinject + `_system`-DO
//! forwarding with the path preserved. Because that path strips the
//! client-suppliable `x-admin-principal` / `x-admin-scope` headers
//! and re-injects only internal-auth / request-id / route-kind /
//! tenant / token-prefix, the gate synthesizes the audit identity
//! [`INTERNAL_EDGE_PRINCIPAL`] for such calls — see
//! [`require_admin_scope`].
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
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::storage::d1_http::{D1HttpClient, D1Row};

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Canonical list-pilots route path. Also the canonical create-pilot
/// path: `GET` lists, `POST` creates — same axum path, different method.
pub const PILOTS_LIST_ROUTE: &str = "/v1/admin/pilots";

/// Canonical grant-tier route path (axum 0.7 `:name` capture).
pub const PILOTS_GRANT_TIER_ROUTE: &str = "/v1/admin/pilots/:tenant_id/grant-tier";

/// Canonical checkin route path (axum 0.7 `:name` capture).
pub const PILOTS_CHECKIN_ROUTE: &str = "/v1/admin/pilots/:tenant_id/checkin";

/// Internal-edge alias for the list route (#218 §2.1, ratified Q3):
/// same handler, reachable through the Worker's `/_internal/*` channel.
pub const INTERNAL_PILOTS_LIST_ROUTE: &str = "/_internal/admin/pilots";

/// Internal-edge alias for the grant-tier route (#218 §2.1).
pub const INTERNAL_PILOTS_GRANT_TIER_ROUTE: &str = "/_internal/admin/pilots/:tenant_id/grant-tier";

/// Internal-edge alias for the checkin route (#218 §2.1).
pub const INTERNAL_PILOTS_CHECKIN_ROUTE: &str = "/_internal/admin/pilots/:tenant_id/checkin";

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

/// Operator-only shared-secret header. The pilot-admin control plane is
/// operator-only (mirrors `/_internal/pat/mint`): the PRIMARY boundary is
/// the `CORELINK_INTERNAL_AUTH_KEY` shared secret verified constant-time,
/// NOT the client-forgeable `x-admin-scope` header (which previously stood
/// alone and let any caller self-assert admin).
///
/// Defense-in-depth: the Worker strips any client-supplied
/// `x-corelink-internal-auth` header on the public `/v1/*` path
/// (`x-corelink-internal-auth` is listed in `CLIENT_TRUST_HEADERS` and is
/// removed before the request reaches the container). The constant-time gate
/// below is a second independent layer — both must hold.
pub const ADMIN_INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Server-set route-kind header. The Worker unconditionally `.set()`s
/// this header on EVERY forward (it is deliberately NOT in
/// `CLIENT_TRUST_HEADERS` because the overwrite makes a client value
/// unreachable), so its value is server-trusted — a request that
/// reaches the container through the Worker carries the Worker's
/// route classification, never the client's.
pub const ROUTE_KIND_HEADER: &str = "x-corelink-route-kind";

/// Route-kind value the Worker sets on `/_internal/*` forwards
/// (`worker/src/index.ts`, internal route handling).
pub const ROUTE_KIND_INTERNAL: &str = "internal";

/// Synthesized audit principal for operator calls arriving through the
/// Worker's `/_internal/*` channel (#218 §2.2, ratified Q3 2026-06-10):
/// that path strips the client-suppliable `x-admin-principal` /
/// `x-admin-scope` headers and does NOT re-inject them, so after the
/// PRIMARY internal-auth gate passes the gate synthesizes this
/// principal — audit rows always carry a non-empty principal.
pub const INTERNAL_EDGE_PRINCIPAL: &str = "internal-edge-operator";

/// Audit event type — emitted on `create` (attempt + success).
pub const EVENT_TYPE_CREATED: &str = "corelink.admin.pilot_created.v1";

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

/// Hard cap on the create-pilot `slug` length (L4 input validation).
pub const MAX_SLUG_LEN: usize = 256;

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

    /// Create a brand-new pilot-tenant row in the `NEW` lifecycle
    /// state: `tier = "free"`, the supplied `cap_bytes`, no grant /
    /// blob timestamps yet. The caller supplies a freshly-minted
    /// `tenant_id` so the row's id is stable across the audit emit and
    /// the persist.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the underlying store is unavailable or
    /// the `tenant_id` already exists (the create is non-idempotent —
    /// a duplicate id is an operator/caller fault, surfaced as a
    /// distinct error so the handler can map it to `409 Conflict`).
    fn create(
        &self,
        tenant_id: Uuid,
        slug: &str,
        cap_bytes: u64,
        signup_at_ms: u64,
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

    fn create(
        &self,
        tenant_id: Uuid,
        slug: &str,
        cap_bytes: u64,
        signup_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        if guard.contains_key(&tenant_id) {
            return Err("tenant already exists");
        }
        let tenant = PilotTenant {
            tenant_id,
            slug: slug.to_string(),
            tier: "free".to_string(),
            cap_bytes,
            pilot_state: PilotState::New,
            signup_at_ms,
            tier_granted_at_ms: None,
            first_blob_at_ms: None,
        };
        guard.insert(tenant_id, tenant.clone());
        Ok(tenant)
    }
}

// -----------------------------------------------------------------------------
// D1-durable pilot-tenant store
// -----------------------------------------------------------------------------

/// Canonical `SELECT` projection for the `pilot_tenants` table (0065) —
/// every column the [`PilotTenant`] shape needs, in a fixed order.
/// Table + column names are compile-time constants (injection-safe);
/// only values are ever bound as `?n` params.
const PILOT_SELECT_COLS: &str =
    "tenant_id, slug, tier, cap_bytes, pilot_state, signup_at_ms, \
     tier_granted_at_ms, first_blob_at_ms";

/// Sync row-source seam over D1 — the sync↔async bridge point.
///
/// The [`PilotStore`] trait is synchronous (the pilot-admin handlers
/// call it inside the axum task), but on the native container D1 is
/// reachable only via the **async** [`D1HttpClient`]. The production
/// impl ([`D1HttpPilotDb`]) bridges each call through
/// `tokio::task::block_in_place` + `Handle::current().block_on(…)` —
/// the same single documented bridge as
/// [`crate::customer_d1::D1HttpCustomerDb`]; tests supply a hermetic
/// mock so the SQL/serialization is exercised without a live D1.
pub trait PilotD1: fmt::Debug + Send + Sync {
    /// Run one parameterised statement; return the result rows (empty
    /// for non-`RETURNING` writes).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport, HTTP, or decode
    /// failure.
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String>;
}

/// Production [`PilotD1`] over the CF D1 REST API. Single documented
/// sync↔async bridge point (mirrors `customer_d1::D1HttpCustomerDb`).
pub struct D1HttpPilotDb {
    /// Shared D1-over-HTTP client (owns + redacts the CF API token).
    d1: Arc<D1HttpClient>,
}

impl D1HttpPilotDb {
    /// Wire the row source over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

impl fmt::Debug for D1HttpPilotDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The client's own Debug is never surfaced here so a leaked
        // Debug can never expose the CF API token.
        f.debug_struct("D1HttpPilotDb")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl PilotD1 for D1HttpPilotDb {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        let d1 = Arc::clone(&self.d1);
        // The native server is `#[tokio::main]` (multi-thread); we are
        // inside an async task (the axum handler), so `block_in_place`
        // hands the worker thread back to the scheduler while
        // `Handle::current().block_on` drives the D1 round-trip.
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(sql, &binds).await })
        })
    }
}

/// D1-durable [`PilotStore`] over the `pilot_tenants` table (0065).
/// Rows survive container restarts (unlike [`InMemoryPilotStore`]).
pub struct D1PilotStore {
    /// D1 row source (production: [`D1HttpPilotDb`]).
    db: Arc<dyn PilotD1>,
}

impl D1PilotStore {
    /// Construct over a [`PilotD1`] row source.
    #[must_use]
    pub fn new(db: Arc<dyn PilotD1>) -> Self {
        Self { db }
    }

    /// Build the production D1-backed store from process env. `None`
    /// when the D1 config ([`crate::storage::StorageEnv`]) is
    /// absent/invalid — the caller then keeps the InMemory store
    /// (dev/CI), mirroring
    /// [`crate::customer_d1::D1CustomerHandler::from_env`]'s
    /// fail-closed pattern.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "admin_pilot: D1 client init failed"))
            .ok()?;
        Some(Arc::new(Self::new(Arc::new(D1HttpPilotDb::new(Arc::new(
            d1,
        ))))))
    }
}

impl fmt::Debug for D1PilotStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redaction marker only — never surface the inner client Debug.
        f.debug_struct("D1PilotStore")
            .field("db", &"[PilotD1]")
            .finish()
    }
}

/// Map a `pilot_tenants` D1 row into a [`PilotTenant`]. Returns a
/// `&'static str` when a required column is missing / mistyped (the
/// store fails CLOSED rather than fabricating a tenant).
fn pilot_row_to_tenant(row: &D1Row) -> Result<PilotTenant, &'static str> {
    let tenant_id = row
        .get("tenant_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("pilot_tenants: missing/invalid tenant_id")?;
    let slug = row
        .get("slug")
        .and_then(Value::as_str)
        .ok_or("pilot_tenants: missing slug")?
        .to_string();
    let tier = row
        .get("tier")
        .and_then(Value::as_str)
        .ok_or("pilot_tenants: missing tier")?
        .to_string();
    let cap_bytes = row
        .get("cap_bytes")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0))
        .ok_or("pilot_tenants: missing cap_bytes")?;
    let pilot_state = row
        .get("pilot_state")
        .and_then(Value::as_str)
        .and_then(PilotState::parse)
        .ok_or("pilot_tenants: missing/invalid pilot_state")?;
    let signup_at_ms = row
        .get("signup_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0))
        .ok_or("pilot_tenants: missing signup_at_ms")?;
    let tier_granted_at_ms = row
        .get("tier_granted_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0));
    let first_blob_at_ms = row
        .get("first_blob_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0));
    Ok(PilotTenant {
        tenant_id,
        slug,
        tier,
        cap_bytes,
        pilot_state,
        signup_at_ms,
        tier_granted_at_ms,
        first_blob_at_ms,
    })
}

impl PilotStore for D1PilotStore {
    fn list_by_state(
        &self,
        state: PilotState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<PilotTenant>, &'static str> {
        let sql = format!(
            "SELECT {PILOT_SELECT_COLS} FROM pilot_tenants \
             WHERE pilot_state = ?1 ORDER BY signup_at_ms ASC LIMIT ?2 OFFSET ?3"
        );
        let rows = self
            .db
            .query(
                &sql,
                vec![
                    json!(state.as_str()),
                    json!(i64::try_from(limit).unwrap_or(i64::MAX)),
                    json!(i64::try_from(offset).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|_| "store unavailable")?;
        rows.iter().map(pilot_row_to_tenant).collect()
    }

    fn get(&self, tenant_id: Uuid) -> Result<Option<PilotTenant>, &'static str> {
        let sql = format!(
            "SELECT {PILOT_SELECT_COLS} FROM pilot_tenants WHERE tenant_id = ?1 LIMIT 1"
        );
        let rows = self
            .db
            .query(&sql, vec![json!(tenant_id.to_string())])
            .map_err(|_| "store unavailable")?;
        match rows.first() {
            Some(row) => Ok(Some(pilot_row_to_tenant(row)?)),
            None => Ok(None),
        }
    }

    fn apply_grant_tier(
        &self,
        tenant_id: Uuid,
        tier: &str,
        cap_bytes: u64,
        granted_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        // Read-modify-write through the same store seam so the
        // grant-eligible-state invariant matches InMemory exactly.
        let existing = self.get(tenant_id)?.ok_or("tenant not found")?;
        if !matches!(
            existing.pilot_state,
            PilotState::New | PilotState::Reserved | PilotState::Provisioned
        ) {
            return Err("tenant not in grant-eligible state");
        }
        // Tenant-scoped, state-guarded UPDATE: the `pilot_state IN (...)`
        // predicate makes the write idempotent + race-safe (a concurrent
        // grant that already flipped the row to ACTIVE updates 0 rows).
        self.db
            .query(
                "UPDATE pilot_tenants \
                 SET tier = ?1, cap_bytes = ?2, pilot_state = 'ACTIVE', \
                     tier_granted_at_ms = ?3 \
                 WHERE tenant_id = ?4 \
                   AND pilot_state IN ('NEW', 'RESERVED', 'PROVISIONED')",
                vec![
                    json!(tier),
                    json!(i64::try_from(cap_bytes).unwrap_or(i64::MAX)),
                    json!(i64::try_from(granted_at_ms).unwrap_or(i64::MAX)),
                    json!(tenant_id.to_string()),
                ],
            )
            .map_err(|_| "store unavailable")?;
        // Re-read so the returned record reflects the persisted row.
        self.get(tenant_id)?.ok_or("tenant not found")
    }

    fn create(
        &self,
        tenant_id: Uuid,
        slug: &str,
        cap_bytes: u64,
        signup_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        // Pre-check existence: D1 HTTP `success` does not discriminate a
        // PRIMARY-KEY conflict cleanly across the wire, so confirm the id
        // is free before inserting (mirrors `d1_http::tenant_set_tier`'s
        // existence pre-check). A duplicate id is an operator fault → a
        // distinct error the handler maps to 409.
        if self.get(tenant_id)?.is_some() {
            return Err("tenant already exists");
        }
        self.db
            .query(
                "INSERT INTO pilot_tenants \
                 (tenant_id, slug, tier, cap_bytes, pilot_state, signup_at_ms, \
                  tier_granted_at_ms, first_blob_at_ms) \
                 VALUES (?1, ?2, 'free', ?3, 'NEW', ?4, NULL, NULL)",
                vec![
                    json!(tenant_id.to_string()),
                    json!(slug),
                    json!(i64::try_from(cap_bytes).unwrap_or(i64::MAX)),
                    json!(i64::try_from(signup_at_ms).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|_| "store unavailable")?;
        Ok(PilotTenant {
            tenant_id,
            slug: slug.to_string(),
            tier: "free".to_string(),
            cap_bytes,
            pilot_state: PilotState::New,
            signup_at_ms,
            tier_granted_at_ms: None,
            first_blob_at_ms: None,
        })
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
    /// Operator-only shared secret for the `x-corelink-internal-auth`
    /// gate (sourced from `CORELINK_INTERNAL_AUTH_KEY`). `None` when the
    /// key is unset at boot → every pilot-admin handler fails CLOSED
    /// (403) (mirrors the `internal_pat` fail-CLOSED posture).
    pub internal_auth_key: Option<Arc<str>>,
}

impl fmt::Debug for PilotAdminRouteState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PilotAdminRouteState")
            .finish_non_exhaustive()
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

/// Request body for `POST /v1/admin/pilots` (create a pilot tenant).
#[derive(Clone, Debug, Deserialize)]
pub struct CreatePilotBody {
    /// Display slug (lowercase, hyphen-delimited). Required, non-empty,
    /// `≤ MAX_SLUG_LEN` chars — validated at the route boundary (L4).
    pub slug: String,
    /// Optional initial storage cap in bytes (decimal-GB convention).
    /// Pre-grant tenants default to 0; the operator stamps the real cap
    /// via `grant-tier`.
    #[serde(default)]
    pub cap_bytes: Option<u64>,
}

/// Response body for `POST /v1/admin/pilots`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatePilotResponse {
    /// The newly-created pilot tenant (state `NEW`, tier `free`).
    pub tenant: PilotTenant,
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
///
/// Each handler is bound twice: at its canonical `/v1/admin/pilots…`
/// path AND at the `/_internal/admin/pilots…` alias (#218 §2.1,
/// ratified Q3) — the alias rides the Worker's existing `/_internal/*`
/// forwarding channel (constant-time internal-auth verification +
/// client-trust-header strip + `_system`-DO forward, path preserved),
/// making the surface operator-reachable from the public edge with
/// zero Worker changes. Both paths run the IDENTICAL gate
/// ([`require_admin_scope`]) — the alias is reachability, not a
/// privilege change.
pub fn router(state: PilotAdminRouteState) -> Router {
    Router::new()
        .route(PILOTS_LIST_ROUTE, get(handle_list).post(handle_create))
        .route(PILOTS_GRANT_TIER_ROUTE, post(handle_grant_tier))
        .route(PILOTS_CHECKIN_ROUTE, post(handle_checkin))
        .route(
            INTERNAL_PILOTS_LIST_ROUTE,
            get(handle_list).post(handle_create),
        )
        .route(INTERNAL_PILOTS_GRANT_TIER_ROUTE, post(handle_grant_tier))
        .route(INTERNAL_PILOTS_CHECKIN_ROUTE, post(handle_checkin))
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

async fn handle_create(
    State(state): State<PilotAdminRouteState>,
    headers: HeaderMap,
    Json(body): Json<CreatePilotBody>,
) -> Response {
    // No target tenant — create mints a NEW id, so the gate runs at the
    // global-operator scope (mirrors `handle_list`).
    let scope = match require_admin_scope(&state, &headers, None) {
        Ok(s) => s,
        Err(r) => return r,
    };
    // L4 input validation.
    let slug = body.slug.trim();
    if slug.is_empty() {
        return (StatusCode::BAD_REQUEST, "slug must be non-empty").into_response();
    }
    if slug.chars().count() > MAX_SLUG_LEN {
        return (StatusCode::BAD_REQUEST, "slug too long").into_response();
    }
    // L4 charset validation (F-03): enforce the documented "lowercase,
    // hyphen-delimited" slug contract — ASCII alphanumeric / `-` / `_` only.
    // Blocks null/control chars and other non-printables from reaching D1 /
    // audit logs (log-injection / row-shape hardening). SQLi is already
    // impossible here (all SQL is parameterised), so this is defence-in-depth.
    if !slug
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return (
            StatusCode::BAD_REQUEST,
            "slug must be lowercase alphanumeric/hyphen/underscore",
        )
            .into_response();
    }
    let cap_bytes = body.cap_bytes.unwrap_or(0);
    let tenant_id = Uuid::now_v7();
    let now_ms = state.wall_clock.now_ms();

    // Fail-CLOSED: emit audit BEFORE the mutation. If audit fails, abort.
    let pre_row = PilotAuditRow {
        event_type: EVENT_TYPE_CREATED.to_string(),
        principal: scope.principal.clone(),
        tenant_id: Some(tenant_id),
        at_unix_ms: now_ms,
        exit_status: "attempt".to_string(),
        payload: json!({
            "slug": slug,
            "cap_bytes": cap_bytes,
        }),
    };
    if state.audit_sink.emit(pre_row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    let tenant = match state.store.create(tenant_id, slug, cap_bytes, now_ms) {
        Ok(t) => t,
        Err(msg) => {
            let status = if msg == "tenant already exists" {
                StatusCode::CONFLICT
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            return (status, msg).into_response();
        }
    };

    let post_row = PilotAuditRow {
        event_type: EVENT_TYPE_CREATED.to_string(),
        principal: scope.principal,
        tenant_id: Some(tenant_id),
        at_unix_ms: now_ms,
        exit_status: "ok".to_string(),
        payload: json!({
            "slug": tenant.slug,
            "cap_bytes": tenant.cap_bytes,
            "pilot_state": tenant.pilot_state.as_str(),
        }),
    };
    if state.audit_sink.emit(post_row).is_err() {
        // The row already landed in the store, but the post-emit failed:
        // surface 503 so the operator retries. The pre-emit `attempt`
        // row is the SEC team's record that the create was issued.
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }

    (StatusCode::CREATED, Json(CreatePilotResponse { tenant })).into_response()
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
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "invalid tenant_id").into_response())
}

/// Constant-time verification of the operator shared secret.
///
/// Fail-CLOSED:
/// - `expected` is `None` (key not configured at boot) → `false`.
/// - header absent / wrong → `false`.
///
/// Pads the provided value to the expected length and runs a single
/// `ct_eq`, then folds in the real length-equality — so the secret LENGTH
/// is not leaked via an early-return short-circuit.
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

    // PRIMARY boundary (fail-CLOSED): the operator-only shared secret.
    // This replaces the previous design where the client-forgeable
    // `x-admin-scope` header was the SOLE gate — any tenant PAT could
    // set that header and self-assert admin. Now the request must clear
    // the `x-corelink-internal-auth` constant-time gate first; absent /
    // wrong secret, or unconfigured key → emit the canonical
    // unauthorized audit row BEFORE the 403.
    if !internal_auth_ok(state.internal_auth_key.as_ref(), headers) {
        let row = PilotAuditRow {
            event_type: EVENT_TYPE_UNAUTHORIZED.to_string(),
            principal: String::new(),
            tenant_id: target_tenant,
            at_unix_ms: now_ms,
            exit_status: "forbidden".to_string(),
            payload: serde_json::json!({
                "reason": "missing/invalid x-corelink-internal-auth",
            }),
        };
        return Err(emit_or_503(
            state,
            row,
            (StatusCode::FORBIDDEN, "operator auth required").into_response(),
        ));
    }

    let principal = headers
        .get(ADMIN_PRINCIPAL_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let route_kind = headers
        .get(ROUTE_KIND_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    // Internal-edge identity synthesis (#218 §2.2, ratified Q3
    // 2026-06-10): the Worker's `/_internal/*` path strips the
    // client-suppliable `x-admin-principal` / `x-admin-scope` headers
    // (CLIENT_TRUST_HEADERS) and re-injects ONLY internal-auth /
    // request-id / route-kind / tenant / token-prefix — so an operator
    // call through the public edge arrives with NO principal and NO
    // scope label. After the PRIMARY internal-auth gate above has
    // passed, an empty principal PLUS the server-set
    // `x-corelink-route-kind: internal` (the Worker unconditionally
    // overwrites that header on every forward — not client-forgeable)
    // synthesizes the audit identity [`INTERNAL_EDGE_PRINCIPAL`] with
    // the pilots scope treated as granted, so audit rows keep a
    // principal. `bound_tenant` stays `None` (global operator): the
    // Worker strips `x-admin-tenant` on this path too. Matches the
    // `/_internal/pat/mint` precedent (internal-auth-only gating).
    //
    // Requests with an explicit principal keep today's behavior
    // byte-identical; an empty principal WITHOUT the internal
    // route-kind still falls through to the 403 below.
    let scope = if principal.is_empty() && route_kind == ROUTE_KIND_INTERNAL {
        PilotAdminScope {
            principal: INTERNAL_EDGE_PRINCIPAL.to_string(),
            bound_tenant: None,
        }
    } else {
        let scope_header = headers
            .get(ADMIN_SCOPE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // SECONDARY label (NOT the sole gate): the internal-auth check above
        // is the boundary. We keep the scope-string + principal checks as a
        // defence-in-depth label so audit rows still carry a principal and a
        // misconfigured operator forward (no scope) is observable.
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
        PilotAdminScope {
            principal: principal.to_string(),
            bound_tenant,
        }
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
fn emit_or_503(state: &PilotAdminRouteState, row: PilotAuditRow, happy: Response) -> Response {
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
            // Configured key so unit tests that exercise the store /
            // scope logic can supply the secret header. Gate behaviour is
            // covered by the dedicated internal_auth_ok tests below.
            internal_auth_key: Some(Arc::from("test-internal-auth-key-32-bytes-x")),
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
        // #218 §2.1 aliases — the same handlers behind the Worker's
        // `/_internal/*` forwarding channel.
        assert_eq!(INTERNAL_PILOTS_LIST_ROUTE, "/_internal/admin/pilots");
        assert_eq!(
            INTERNAL_PILOTS_GRANT_TIER_ROUTE,
            "/_internal/admin/pilots/:tenant_id/grant-tier"
        );
        assert_eq!(
            INTERNAL_PILOTS_CHECKIN_ROUTE,
            "/_internal/admin/pilots/:tenant_id/checkin"
        );
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
        let rows = store.list_by_state(PilotState::New, 0, 50).expect("list");
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
    fn internal_auth_fails_closed_when_key_unset() {
        // Even with a forged x-corelink-internal-auth header, an unset
        // key means the operator gate fails CLOSED.
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "anything".parse().expect("header"),
        );
        assert!(!internal_auth_ok(None, &headers));
    }

    #[test]
    fn internal_auth_matches_only_exact_secret() {
        let key: Arc<str> = Arc::from("test-internal-auth-key-32-bytes-x");
        let empty = HeaderMap::new();
        assert!(!internal_auth_ok(Some(&key), &empty));
        let mut wrong = HeaderMap::new();
        wrong.insert(ADMIN_INTERNAL_AUTH_HEADER, "wrong".parse().expect("header"));
        assert!(!internal_auth_ok(Some(&key), &wrong));
        let mut right = HeaderMap::new();
        right.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        assert!(internal_auth_ok(Some(&key), &right));
    }

    /// `require_admin_scope` rejects (403) when the internal-auth header
    /// is absent even if a forged `x-admin-scope` is present — proving
    /// the scope header is no longer the sole gate.
    #[test]
    fn require_admin_scope_rejects_without_internal_auth() {
        let (state, _store, audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        // Forged scope + principal — the OLD sole gate. No internal-auth.
        headers.insert(
            ADMIN_SCOPE_HEADER,
            REQUIRED_ADMIN_SCOPE.parse().expect("header"),
        );
        headers.insert(ADMIN_PRINCIPAL_HEADER, "attacker".parse().expect("header"));
        let res = require_admin_scope(&state, &headers, None);
        assert!(res.is_err(), "must reject without internal-auth secret");
        // The unauthorized audit row was emitted BEFORE the 403.
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
    }

    /// With the correct internal-auth secret AND scope + principal, the
    /// gate passes.
    #[test]
    fn require_admin_scope_passes_with_internal_auth_and_scope() {
        let (state, _store, _audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        headers.insert(
            ADMIN_SCOPE_HEADER,
            REQUIRED_ADMIN_SCOPE.parse().expect("header"),
        );
        headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
        let scope = require_admin_scope(&state, &headers, None).expect("authorized");
        assert_eq!(scope.principal, "ops@root");
    }

    /// #218 §2.2 (ratified Q3): internal-auth + server-set
    /// `x-corelink-route-kind: internal` + NO principal/scope headers
    /// (the Worker strips them on the `/_internal/*` path) → the gate
    /// synthesizes the `internal-edge-operator` identity with a global
    /// (unbound) tenant scope.
    #[test]
    fn require_admin_scope_synthesizes_internal_edge_identity() {
        let (state, _store, audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        headers.insert(
            ROUTE_KIND_HEADER,
            ROUTE_KIND_INTERNAL.parse().expect("header"),
        );
        let scope = require_admin_scope(&state, &headers, None).expect("synthesized");
        assert_eq!(scope.principal, INTERNAL_EDGE_PRINCIPAL);
        assert!(scope.bound_tenant.is_none(), "global operator scope");
        // No unauthorized audit row — the gate admitted the request.
        let rows = audit.snapshot().expect("audit");
        assert!(rows.is_empty());
    }

    /// #218 §2.2: an empty principal WITHOUT the internal route-kind
    /// keeps today's 403 (the synthesis never fires for direct /
    /// non-internal forwards) and the unauthorized audit row is
    /// emitted BEFORE the 403.
    #[test]
    fn require_admin_scope_rejects_empty_principal_without_internal_route_kind() {
        let (state, _store, audit, _c) = fixture();
        for route_kind in [None, Some("reapi_v1"), Some("onboarding")] {
            let mut headers = HeaderMap::new();
            headers.insert(
                ADMIN_INTERNAL_AUTH_HEADER,
                "test-internal-auth-key-32-bytes-x".parse().expect("header"),
            );
            if let Some(kind) = route_kind {
                headers.insert(ROUTE_KIND_HEADER, kind.parse().expect("header"));
            }
            let res = require_admin_scope(&state, &headers, None);
            assert!(
                res.is_err(),
                "empty principal must 403 (kind={route_kind:?})"
            );
        }
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
    }

    /// #218 §2.2: the synthesis is SECONDARY to the internal-auth gate —
    /// `x-corelink-route-kind: internal` alone (no/wrong secret) is
    /// still rejected with the unauthorized audit row first. The
    /// route-kind header never substitutes for the PRIMARY boundary.
    #[test]
    fn internal_route_kind_without_internal_auth_still_403() {
        let (state, _store, audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        headers.insert(
            ROUTE_KIND_HEADER,
            ROUTE_KIND_INTERNAL.parse().expect("header"),
        );
        let res = require_admin_scope(&state, &headers, None);
        assert!(res.is_err(), "must reject without the operator secret");
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
    }

    /// #218 §2.2: an EXPLICIT principal keeps today's behavior
    /// byte-identical even when the route-kind is `internal` — the
    /// synthesis fires only on an EMPTY principal, so a direct
    /// operator call with explicit headers keeps its own identity
    /// (and still needs the scope label).
    #[test]
    fn explicit_principal_with_internal_route_kind_keeps_explicit_identity() {
        let (state, _store, _audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        headers.insert(
            ROUTE_KIND_HEADER,
            ROUTE_KIND_INTERNAL.parse().expect("header"),
        );
        headers.insert(
            ADMIN_SCOPE_HEADER,
            REQUIRED_ADMIN_SCOPE.parse().expect("header"),
        );
        headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
        let scope = require_admin_scope(&state, &headers, None).expect("authorized");
        assert_eq!(scope.principal, "ops@root", "explicit identity wins");
    }

    /// #218 §2.2: explicit principal WITHOUT the scope label is still
    /// 403 even under `route-kind: internal` — the explicit-header
    /// path is unchanged; only the empty-principal case synthesizes.
    #[test]
    fn explicit_principal_without_scope_under_internal_route_kind_still_403() {
        let (state, _store, audit, _c) = fixture();
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        headers.insert(
            ROUTE_KIND_HEADER,
            ROUTE_KIND_INTERNAL.parse().expect("header"),
        );
        headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
        let res = require_admin_scope(&state, &headers, None);
        assert!(
            res.is_err(),
            "explicit principal still requires the scope label"
        );
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
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

    // ── create (in-memory store) ─────────────────────────────────────────────

    #[test]
    fn in_memory_store_create_inserts_new_tenant_in_new_state() {
        let (_st, store, _au, _c) = fixture();
        let id = Uuid::now_v7();
        let created = store
            .create(id, "acme-builds", 0, 1_000)
            .expect("create");
        assert_eq!(created.tenant_id, id);
        assert_eq!(created.slug, "acme-builds");
        assert_eq!(created.tier, "free");
        assert_eq!(created.pilot_state, PilotState::New);
        assert_eq!(created.signup_at_ms, 1_000);
        assert!(created.tier_granted_at_ms.is_none());
        assert!(created.first_blob_at_ms.is_none());
        // The row is now durable in the store + grant-eligible.
        let fetched = store.get(id).expect("get").expect("present");
        assert_eq!(fetched, created);
    }

    #[test]
    fn in_memory_store_create_rejects_duplicate_id() {
        let (_st, store, _au, _c) = fixture();
        let id = Uuid::now_v7();
        store.create(id, "a", 0, 1).expect("first");
        let err = store.create(id, "b", 0, 2).expect_err("dup");
        assert_eq!(err, "tenant already exists");
    }

    // ── create handler (router) ──────────────────────────────────────────────

    /// Build the request headers that clear the operator gate.
    fn operator_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        headers.insert(
            ADMIN_SCOPE_HEADER,
            REQUIRED_ADMIN_SCOPE.parse().expect("header"),
        );
        headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
        headers
    }

    #[tokio::test]
    async fn handle_create_persists_and_returns_201() {
        let (state, store, audit, _c) = fixture();
        let resp = handle_create(
            State(state),
            operator_headers(),
            Json(CreatePilotBody {
                slug: "  beta-co  ".to_owned(),
                cap_bytes: Some(42),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        // Exactly one NEW-state tenant landed, slug trimmed.
        let rows = store.list_by_state(PilotState::New, 0, 50).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slug, "beta-co");
        assert_eq!(rows[0].cap_bytes, 42);
        // Audit: attempt then ok.
        let kinds: Vec<_> = audit
            .snapshot()
            .expect("audit")
            .into_iter()
            .map(|r| (r.event_type, r.exit_status))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (EVENT_TYPE_CREATED.to_owned(), "attempt".to_owned()),
                (EVENT_TYPE_CREATED.to_owned(), "ok".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn handle_create_rejects_empty_slug() {
        let (state, store, _audit, _c) = fixture();
        let resp = handle_create(
            State(state),
            operator_headers(),
            Json(CreatePilotBody {
                slug: "   ".to_owned(),
                cap_bytes: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(store
            .list_by_state(PilotState::New, 0, 50)
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn handle_create_rejects_without_internal_auth() {
        let (state, _store, _audit, _c) = fixture();
        // No operator secret header → 403 (gate is the PRIMARY boundary).
        let resp = handle_create(
            State(state),
            HeaderMap::new(),
            Json(CreatePilotBody {
                slug: "x".to_owned(),
                cap_bytes: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn handle_create_audit_failure_aborts_before_mutation() {
        let (state, store, audit, _c) = fixture();
        audit.inject_failure("sink down").expect("inject");
        let resp = handle_create(
            State(state),
            operator_headers(),
            Json(CreatePilotBody {
                slug: "x".to_owned(),
                cap_bytes: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        // Fail-CLOSED: nothing persisted.
        assert!(store
            .list_by_state(PilotState::New, 0, 50)
            .expect("list")
            .is_empty());
    }

    // ── D1 store (hermetic mock, no live D1) ─────────────────────────────────

    /// Hermetic mock [`PilotD1`]: canned result sets keyed by an SQL
    /// fragment; every (sql, binds) recorded for assertions. Mirrors
    /// `customer_d1::MockD1`.
    #[derive(Debug, Default)]
    struct MockPilotD1 {
        canned: Vec<(&'static str, Vec<D1Row>)>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
        fail: bool,
    }

    impl MockPilotD1 {
        fn with(canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
            Self {
                canned,
                calls: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::default()
            }
        }

        fn calls(&self) -> Vec<(String, Vec<Value>)> {
            self.calls.lock().expect("lock").clone()
        }
    }

    impl PilotD1 for MockPilotD1 {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            self.calls
                .lock()
                .expect("lock")
                .push((sql.to_owned(), binds));
            if self.fail {
                return Err("D1 HTTP 500: transport down".to_owned());
            }
            for (fragment, rows) in &self.canned {
                if sql.contains(fragment) {
                    return Ok(rows.clone());
                }
            }
            Ok(Vec::new())
        }
    }

    fn d1_row(pairs: &[(&str, Value)]) -> D1Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn pilot_d1_row(id: Uuid, state: &str, signup: i64) -> D1Row {
        d1_row(&[
            ("tenant_id", json!(id.to_string())),
            ("slug", json!("acme")),
            ("tier", json!("free")),
            ("cap_bytes", json!(0_i64)),
            ("pilot_state", json!(state)),
            ("signup_at_ms", json!(signup)),
            ("tier_granted_at_ms", Value::Null),
            ("first_blob_at_ms", Value::Null),
        ])
    }

    #[test]
    fn pilot_row_to_tenant_maps_every_column() {
        let id = Uuid::now_v7();
        let row = d1_row(&[
            ("tenant_id", json!(id.to_string())),
            ("slug", json!("acme-builds")),
            ("tier", json!("pilot")),
            ("cap_bytes", json!(100_i64)),
            ("pilot_state", json!("ACTIVE")),
            ("signup_at_ms", json!(1_000_i64)),
            ("tier_granted_at_ms", json!(5_000_i64)),
            ("first_blob_at_ms", Value::Null),
        ]);
        let t = pilot_row_to_tenant(&row).expect("map");
        assert_eq!(t.tenant_id, id);
        assert_eq!(t.slug, "acme-builds");
        assert_eq!(t.tier, "pilot");
        assert_eq!(t.cap_bytes, 100);
        assert_eq!(t.pilot_state, PilotState::Active);
        assert_eq!(t.signup_at_ms, 1_000);
        assert_eq!(t.tier_granted_at_ms, Some(5_000));
        assert!(t.first_blob_at_ms.is_none());
    }

    #[test]
    fn pilot_row_to_tenant_fails_closed_on_missing_column() {
        let row = d1_row(&[("slug", json!("acme"))]);
        assert!(pilot_row_to_tenant(&row).is_err());
    }

    #[test]
    fn d1_store_list_binds_state_limit_offset_and_maps_rows() {
        let id = Uuid::now_v7();
        let db = Arc::new(MockPilotD1::with(vec![(
            "FROM pilot_tenants",
            vec![pilot_d1_row(id, "NEW", 1_000)],
        )]));
        let store = D1PilotStore::new(db.clone());
        let rows = store.list_by_state(PilotState::New, 7, 25).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tenant_id, id);
        // Binds: state, limit, offset — values only, never the table name.
        let calls = db.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.contains("ORDER BY signup_at_ms ASC"));
        assert_eq!(
            calls[0].1,
            vec![json!("NEW"), json!(25_i64), json!(7_i64)]
        );
    }

    #[test]
    fn d1_store_get_returns_none_for_absent_row() {
        let db = Arc::new(MockPilotD1::with(vec![]));
        let store = D1PilotStore::new(db);
        assert!(store.get(Uuid::now_v7()).expect("get").is_none());
    }

    #[test]
    fn d1_store_create_pre_checks_then_inserts() {
        // Empty canned → get() returns None (free id), so create inserts.
        let db = Arc::new(MockPilotD1::with(vec![]));
        let store = D1PilotStore::new(db.clone());
        let id = Uuid::now_v7();
        let created = store.create(id, "beta", 9, 2_000).expect("create");
        assert_eq!(created.pilot_state, PilotState::New);
        assert_eq!(created.tier, "free");
        let calls = db.calls();
        // 1: existence SELECT, 2: INSERT.
        assert_eq!(calls.len(), 2);
        assert!(calls[0].0.contains("SELECT"));
        assert!(calls[1].0.contains("INSERT INTO pilot_tenants"));
        assert_eq!(
            calls[1].1,
            vec![
                json!(id.to_string()),
                json!("beta"),
                json!(9_i64),
                json!(2_000_i64),
            ]
        );
    }

    #[test]
    fn d1_store_create_rejects_existing_id() {
        let id = Uuid::now_v7();
        let db = Arc::new(MockPilotD1::with(vec![(
            "FROM pilot_tenants",
            vec![pilot_d1_row(id, "NEW", 1)],
        )]));
        let store = D1PilotStore::new(db);
        let err = store.create(id, "x", 0, 1).expect_err("dup");
        assert_eq!(err, "tenant already exists");
    }

    #[test]
    fn d1_store_grant_tier_rejects_already_active_without_update() {
        let id = Uuid::now_v7();
        let db = Arc::new(MockPilotD1::with(vec![(
            "FROM pilot_tenants",
            vec![pilot_d1_row(id, "ACTIVE", 1)],
        )]));
        let store = D1PilotStore::new(db.clone());
        let err = store
            .apply_grant_tier(id, "pilot", 1, 5)
            .expect_err("conflict");
        assert!(err.contains("not in grant-eligible state"));
        // Only the read happened — no UPDATE was issued.
        assert!(db.calls().iter().all(|(sql, _)| !sql.contains("UPDATE")));
    }

    #[test]
    fn d1_store_fails_closed_on_transport_error() {
        let store = D1PilotStore::new(Arc::new(MockPilotD1::failing()));
        assert!(store.list_by_state(PilotState::New, 0, 50).is_err());
        assert!(store.get(Uuid::now_v7()).is_err());
    }
}
