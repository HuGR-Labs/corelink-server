// Pilot admin HTTP routes (Wave-29 stream-3 wire-up of the wave-27
// placeholder scripts).
//
// Replaces the three wave-27 placeholder scripts
// (`scripts/admin/list-pilot-tenants.sh`, `grant-pilot-tier.sh`,
// `pilot-24h-checkin.sh`) with proper admin endpoints that the
// operator-facing Docusaurus admin page consumes via fetch.
//
// # Surface
//
// - `GET  /v1/admin/pilots?state=<NEW|ACTIVE|GRADUATED|TERMINATED>`
//   — paginated list of pilot tenants filtered by lifecycle state.
// - `POST /v1/admin/pilots` — create a new pilot tenant row (slug,
//   optional initial `cap_bytes`). Mints a fresh tenant UUID, persists
//   the row in the `NEW` lifecycle state, and returns the created
//   [`PilotTenant`]. The operator follows up with `grant-tier`.
// - `POST /v1/admin/pilots/{tenant_id}/grant-tier` — grant pilot
//   tier (`tier`, `cap_bytes`). Mirrors `grant-pilot-tier.sh`
//   semantics, including the `pilot_state` NEW → ACTIVE transition.
// - `POST /v1/admin/pilots/{tenant_id}/checkin` — run the 24h
//   activation check-in. Mirrors `pilot-24h-checkin.sh`: emits an
//   alert row when the tenant has not uploaded a single blob 24h
//   after `tier_granted_at_ms`.
//
// ## `/_internal/admin/pilots…` alias (#218 §2.1, ratified Q3)
//
// The SAME three handlers are additionally bound under
// `/_internal/admin/pilots…` so the surface is operator-reachable
// from the public edge with ZERO Worker changes: the Worker's
// existing `/_internal/*` route-kind already does constant-time
// internal-auth verification + strip-then-reinject + `_system`-DO
// forwarding with the path preserved. Because that path strips the
// client-suppliable `x-admin-principal` / `x-admin-scope` headers
// and re-injects only internal-auth / request-id / route-kind /
// tenant / token-prefix, the gate synthesizes the audit identity
// [`INTERNAL_EDGE_PRINCIPAL`] for such calls — see
// [`require_admin_scope`].
//
// # Auth model
//
// Every route is gated by the admin JWT scope
// `corelink:admin:pilots`. Production wiring (matches the wave-15
// admin handler discipline) slots a tower middleware in front that:
//
//   1. Validates the Clerk session JWT signature + expiry.
//   2. Extracts the `scope` claim and pins it on the request via
//      `X-Admin-Scope` (canonical lowercase, one or more
//      space-separated scopes).
//   3. Extracts the admin principal id and pins it on the request
//      via `X-Admin-Principal`.
//
// The routes here re-check the scope at the route boundary (5-Layer
// Defense — never trust the middleware exclusively) and emit a
// `corelink.security.admin_pilot_unauthorized.v1` audit row BEFORE
// the 403 when a non-admin principal probes the surface.
//
// ## 5-Layer Defense
//
// Per S-03 + wave-15 admin handler:
//
//   L1: tower middleware (JWT signature + expiry)            — out of scope here
//   L2: scope re-check at the route boundary                 — `require_admin_scope`
//   L3: tenant-scoped authorization                          — `PilotAdminScope::allows_tenant`
//   L4: input validation                                     — `parse_tenant_uuid` + `parse_state`
//   L5: fail-CLOSED audit emit BEFORE every state mutation   — `emit_or_503`
//
// # Fail-CLOSED audit ordering
//
// Every state mutation emits a `corelink.admin.pilot_*.v1` audit
// row BEFORE the mutation lands. If the audit sink returns `Err`
// the route aborts with `503 Service Unavailable` and the
// mutation is NOT applied. This mirrors the
// `corelink-handler-admin` `MutateAttempted` → `MutateCommitted`
// ordering.
//
// # Charter compliance
//
// - `#![forbid(unsafe_code)]` (crate-wide via `lib.rs`).
// - No `unwrap()` / `expect()` / `panic!()` on the request path —
//   all `Result`s map to a typed error and an HTTP response.
// - DCO sign-off + Co-Authored-By land at commit time.

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
pub const PILOTS_GRANT_TIER_ROUTE: &str = "/v1/admin/pilots/{tenant_id}/grant-tier";

/// Canonical checkin route path (axum 0.7 `:name` capture).
pub const PILOTS_CHECKIN_ROUTE: &str = "/v1/admin/pilots/{tenant_id}/checkin";

/// Internal-edge alias for the list route (#218 §2.1, ratified Q3):
/// same handler, reachable through the Worker's `/_internal/*` channel.
pub const INTERNAL_PILOTS_LIST_ROUTE: &str = "/_internal/admin/pilots";

/// Internal-edge alias for the grant-tier route (#218 §2.1).
pub const INTERNAL_PILOTS_GRANT_TIER_ROUTE: &str = "/_internal/admin/pilots/{tenant_id}/grant-tier";

/// Internal-edge alias for the checkin route (#218 §2.1).
pub const INTERNAL_PILOTS_CHECKIN_ROUTE: &str = "/_internal/admin/pilots/{tenant_id}/checkin";

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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct InMemoryPilotStore {
    by_id: Mutex<HashMap<Uuid, PilotTenant>>,
    fail_with: Mutex<Option<&'static str>>,
}
