//! Production **D1-backed customer handler** (dashboard revival WP-3):
//! [`D1CustomerHandler`] implements all 6 `corelink-handler-customer`
//! traits over the live Cloudflare D1 database, replacing the
//! `InMemoryCustomerHandler` 404-stub so real tenants get real
//! dashboard data.
//!
//! # HONEST v1 contract
//!
//! Real data where a deployed table exists; explicit empty / zero /
//! `501 Not Implemented` where it doesn't. NEVER fabricated data.
//! Per-endpoint matrix (deployed schema in parentheses):
//!
//! | Endpoint            | Source of truth                                            |
//! |---------------------|------------------------------------------------------------|
//! | overview            | `tenant` (0023/0031/0056/0057) + `tenant_storage_state` (0008) `SUM(bytes_used)` / `MAX(bytes_quota)` + `tenant_billing` (0055) + `byok_envelope` (0030) existence; `tenant_name` = `tenant_id`; `recent_activity` = `[]` (no suitable table) |
//! | usage               | real `cas_bytes`/`quota_bytes` from `tenant_storage_state`; `reads`/`writes` = 0 + `daily` = `[]` (no per-day table) — only the CURRENT period is retained, an earlier period honestly reports 0 bytes |
//! | audit               | `rows: []` (no suitable customer-audit table yet)          |
//! | billing             | `tenant_billing` (0055) + `tier_selections` (0039/0062); status map FROZEN (see [`map_billing_status`]); `invoices` = `[]` (no invoice-history surface yet) |
//! | billing/portal      | `tenant_billing.stripe_customer_id` → Stripe billing-portal session; no customer id → 404 "no billing account" |
//! | keys list           | `pat` (0037/0054 + WP-2 columns `name`, `revoked_at_ms` — migration 0063, parallel PR); `last_used_at` = `None` (not tracked) |
//! | keys create         | `corelink_pat::mint` + D1 `INSERT`; scope map FROZEN (`["cache:read"]`→`read-only`, anything-with-write→`read-write`, `admin` NEVER grantable); token returned once, never logged |
//! | keys revoke         | `UPDATE pat SET revoked_at_ms=?` tenant-scoped; idempotent |
//! | team                | single synthesized **Owner** row from `tenant.clerk_user_id`, email `"—"` |
//! | team/invite         | INSERT a `team_member` row (status `invited`, SHA-256 `email_hash` per CTRL-PRIV-001, migration 0074); NO synchronous Clerk call (the email round-trip is an owner follow-up, OB-1) |
//!
//! # The sync↔async bridge
//!
//! The 6 handler traits are synchronous (shared shape with the wasm32
//! CF-Worker target); on the native container D1 is reachable only via
//! the **async** [`D1HttpClient`]. Exactly like
//! [`crate::billing_d1_http::D1HttpBillingWriter::run`], each sync call
//! drives the async client through `tokio::task::block_in_place` +
//! `Handle::current().block_on(…)` — valid because the native server is
//! `#[tokio::main]` (multi-thread). The bridge lives behind the
//! [`CustomerD1`] seam so unit tests exercise the handler hermetically
//! with a mock row source (mirroring the billing writer's test layout).
//! The Stripe portal call (blocking `reqwest`) crosses the same bridge
//! behind [`PortalSessions`].
//!
//! # SECURITY / fail-CLOSED
//!
//! - **Audit-before-lookup + SLI on every return path** per the trait
//!   docs (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` +
//!   `INV-HANDLER-SLI-EMIT-ENTRY`); audit emit failure aborts with
//!   [`CustomerHandlerError::AuditFailed`] BEFORE any D1 round-trip.
//! - **Fail-CLOSED on D1 transport error:** any transport / non-2xx /
//!   decode error maps to [`CustomerHandlerError::Internal`] (→ 500),
//!   never to fabricated empty data.
//! - **Parameterised SQL only** — every dynamic value is a positional
//!   bind; the tenant scope rides in `WHERE tenant_id = ?` on every
//!   statement (INV-TENANT-ISOLATION).
//! - **Token returned once, never logged**; the PAT plaintext only
//!   exists in the [`KeyCreateResponse`].
//! - **Wall clock, never the request timestamp:** the routes pass
//!   `at_unix_ms = 0` (see `routes/customer.rs::now_ms`), so all
//!   timestamps come from the injected [`WallClock`].

use std::sync::Arc;
use std::time::Duration;

use corelink_handler_customer::observer::Sli;
use corelink_handler_customer::request::{
    ByokStatus, CustomerAuditEventRow, DailyUsageBucket, InvoiceRow, OverviewBilling,
    OverviewUsage, PatRow, TeamMemberRow,
};
use corelink_handler_customer::{
    AuditEvent, AuditEventKind, AuditQueryRequest, AuditQueryResponse, AuditSink, BillingRequest,
    BillingResponse, CustomerAuditHandler, CustomerBillingHandler, CustomerHandlerError,
    CustomerKeysHandler, CustomerOverviewHandler, CustomerTeamHandler, CustomerUsageHandler,
    KeyCreateRequest, KeyCreateResponse, KeyRevokeRequest, KeyRevokeResponse, KeysListRequest,
    KeysListResponse, OverviewRequest, OverviewResponse, PortalRequest, PortalResponse,
    SliObservation, SliObserver, TeamInviteRequest, TeamInviteResponse, TeamListRequest,
    TeamListResponse, TeamRemoveRequest, TeamRemoveResponse, UsageRequest, UsageResponse,
};
use corelink_pat::{
    mint::mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_R,
    SCOPE_CACHE_RW,
};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::wall_clock::WallClock;

// ─── Seams (sync trait surfaces over the async/blocking backends) ────────────

/// Sync row-source seam over D1. The production impl
/// ([`D1HttpCustomerDb`]) bridges to the async [`D1HttpClient`]; tests
/// supply a hermetic mock (mirrors `billing_d1_http`'s test layout).
pub trait CustomerD1: Send + Sync + core::fmt::Debug {
    /// Run one parameterised statement; return the result rows (empty
    /// for non-`RETURNING` writes). `Err(String)` is a transport /
    /// decode / D1-level failure (fail-CLOSED at the caller).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport, HTTP, or decode
    /// failure.
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String>;
}

/// Production [`CustomerD1`] over the CF D1 REST API. Single documented
/// sync↔async bridge point (same pattern as
/// `billing_d1_http::D1HttpBillingWriter::run`).
pub struct D1HttpCustomerDb {
    /// Shared D1-over-HTTP client (owns + redacts the CF API token).
    d1: Arc<D1HttpClient>,
}

impl D1HttpCustomerDb {
    /// Wire the row source over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

impl core::fmt::Debug for D1HttpCustomerDb {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The client's own Debug is never surfaced here so a leaked
        // Debug can never expose the CF API token.
        f.debug_struct("D1HttpCustomerDb")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl CustomerD1 for D1HttpCustomerDb {
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

/// Stripe billing-portal seam. The production impl drives the blocking
/// [`corelink_stripe_real::StripeRealClient`]; tests supply a mock.
pub trait PortalSessions: Send + Sync + core::fmt::Debug {
    /// Create a short-lived billing-portal session for
    /// `stripe_customer_id`; return the hosted portal URL.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any Stripe transport / API failure.
    fn create(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
        idempotency_key: &str,
    ) -> Result<String, String>;
}

/// Production [`PortalSessions`] over the real Stripe HTTPS client
/// (`reqwest::blocking`, driven under `block_in_place` — same bridge
/// rationale as [`D1HttpCustomerDb`]).
pub struct StripePortalSessions {
    /// Real Stripe client (owns + redacts the bearer key).
    stripe: Arc<corelink_stripe_real::StripeRealClient>,
}

impl StripePortalSessions {
    /// Wire the portal creator over a shared Stripe client.
    #[must_use]
    pub fn new(stripe: Arc<corelink_stripe_real::StripeRealClient>) -> Self {
        Self { stripe }
    }
}

impl core::fmt::Debug for StripePortalSessions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripePortalSessions")
            .field("stripe", &"[StripeRealClient]")
            .finish()
    }
}

impl PortalSessions for StripePortalSessions {
    fn create(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
        idempotency_key: &str,
    ) -> Result<String, String> {
        let stripe = Arc::clone(&self.stripe);
        tokio::task::block_in_place(move || {
            stripe.create_billing_portal_session(stripe_customer_id, return_url, idempotency_key)
        })
        .map(|s| s.url)
        .map_err(|e| format!("stripe billing-portal session failed: {e}"))
    }
}

// ─── Production audit / SLI collaborators ────────────────────────────────────

/// Production [`AuditSink`]: structured `tracing` emit ingested by the
/// CF Logs pipeline (same posture as `routes/internal_pat.rs`'s
/// `PatMinted` audit emit — full D1 audit emit is a follow-up). The
/// tracing emit is infallible, so the fail-CLOSED `AuditFailed` arm is
/// exercised only by injected-failure tests; unlike `InMemoryAuditSink`
/// this sink is **bounded** (no per-request Vec growth in production).
#[derive(Debug, Default)]
pub struct TracingCustomerAuditSink;

impl TracingCustomerAuditSink {
    /// Construct the canonical tracing-backed sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AuditSink for TracingCustomerAuditSink {
    fn emit(&self, event: AuditEvent) -> Result<(), String> {
        tracing::info!(
            kind = event.kind.slug(),
            tenant = %event.tenant,
            principal = %event.principal,
            resource = %event.resource,
            at_unix_ms = event.at_unix_ms,
            "customer audit event"
        );
        Ok(())
    }
}

/// Production [`SliObserver`]: structured `tracing` emit (control-plane
/// availability observations; the prometheus wiring is a follow-up).
#[derive(Debug, Default)]
pub struct TracingCustomerSliObserver;

impl TracingCustomerSliObserver {
    /// Construct the canonical tracing-backed observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SliObserver for TracingCustomerSliObserver {
    fn observe(&self, obs: SliObservation) {
        tracing::debug!(
            sli = ?obs.sli,
            is_error = obs.is_error,
            latency_us = obs.latency_us,
            "customer SLI observation"
        );
    }
}

// ─── Frozen maps ──────────────────────────────────────────────────────────────

/// FROZEN billing status map (`tenant_billing.status` → dashboard
/// status): `paid`→`active`, `past_due`→`past_due`,
/// `canceled`→`canceled`, `incomplete`→`past_due`,
/// `inactive`/no-row/unknown→`inactive`.
///
/// AUDIT REV-S5 (low, KNOWN-LIMITATION, deferred — contract-level fix):
/// `incomplete` (Stripe's "subscription created, first payment not yet
/// settled") is collapsed onto `past_due`, so a pending FIRST payment is
/// surfaced to the dashboard as a renewal FAILURE. The audit recommends a
/// distinct `pending`/`awaiting_payment` status. We do NOT introduce one
/// here: the dashboard status set is FROZEN and shared cross-team —
/// `apps/admin-ui/src/lib/customer-types.ts` types it as the closed union
/// `"trialing" | "active" | "past_due" | "canceled" | "inactive"`, and
/// `specs/03_architecture/data_model.md` pins the `tenant_billing.status`
/// CHECK to `('active','past_due','canceled')`. Emitting a new `pending`
/// string from this map alone would drift the backend off that frozen
/// contract (the frontend renders `data.status` verbatim and could not
/// classify it). A real fix must land as a coordinated change across the
/// TS union + the spec CHECK + (optionally) a new `BillingResponse` field
/// — out of scope for a `customer_d1.rs`-only patch. The `incomplete` arm
/// is split out below (still → `past_due`, byte-for-byte identical output)
/// purely to make this decision explicit and give that future fix an
/// anchor; it changes NO emitted value.
#[must_use]
pub fn map_billing_status(d1_status: Option<&str>) -> &'static str {
    match d1_status {
        Some("paid") => "active",
        Some("past_due") => "past_due",
        // KNOWN-LIMITATION (REV-S5): would ideally map to a distinct
        // `pending`, but the frozen cross-team status union has no such
        // value — keep `past_due` until that contract is widened.
        Some("incomplete") => "past_due",
        Some("canceled") => "canceled",
        _ => "inactive",
    }
}

/// FROZEN requested-scopes → D1 `pat.scope` map: `admin` is NEVER
/// grantable self-serve (`Err`); anything carrying a write capability →
/// `read-write`; otherwise (incl. the canonical `["cache:read"]` and an
/// empty request — least privilege) → `read-only`.
fn map_requested_scopes(requested: &[String]) -> Result<&'static str, CustomerHandlerError> {
    // SINGLE source of truth with the mint escalation gate
    // (`routes::customer::mint_requests_write`), via `scope::classify_requested_scopes`.
    // rt-nuclear #15: this used to substring-match (`s.contains("write")`) while
    // the gate exact-matched, so `"writes"` skipped the gate yet persisted
    // `read-write` (read-only PAT self-escalation). Now both share one exact-token
    // classifier, and unrecognized tokens are REJECTED (fail-CLOSED) rather than
    // silently mapped to a privilege.
    match crate::scope::classify_requested_scopes(requested) {
        Ok(crate::scope::RequestedScopeClass::ReadOnly) => Ok("read-only"),
        Ok(crate::scope::RequestedScopeClass::ReadWrite) => Ok("read-write"),
        Ok(crate::scope::RequestedScopeClass::Admin) => Err(CustomerHandlerError::Unauthorized(
            "the admin scope is not grantable via self-serve key creation".to_owned(),
        )),
        Err(token) => Err(CustomerHandlerError::Unauthorized(format!(
            "unrecognized scope token {token:?}; valid self-serve scopes: cache:read, cache:write"
        ))),
    }
}

/// D1 `pat.scope` string → dashboard scopes list (inverse of
/// [`map_requested_scopes`] for the canonical values; unknown legacy
/// values are surfaced verbatim rather than guessed at).
fn scope_to_list(scope: &str) -> Vec<String> {
    match scope {
        "read-only" => vec!["cache:read".to_owned()],
        "read-write" => vec!["cache:read".to_owned(), "cache:write".to_owned()],
        "" => vec![],
        other => vec![other.to_owned()],
    }
}

/// Map a dashboard role string onto the FROZEN `team_member.role` CHECK domain
/// (migration 0074: `owner` / `admin` / `member` / `viewer`). The admin-ui sends
/// `"Owner"` / `"Admin"` / `"Developer"` / `"Viewer"`; `Developer` (and any
/// unrecognized value) collapses to the least-privileged `member` so the INSERT
/// can never violate the CHECK (which would surface as a 500). Case-insensitive.
fn normalize_invite_role(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "owner" => "owner",
        "admin" => "admin",
        "viewer" => "viewer",
        // `developer` + anything else → the least-privileged seat (CHECK-safe).
        _ => "member",
    }
}

// ─── Calendar helpers (no chrono/time dependency in this crate) ──────────────

/// Days-since-epoch → civil (y, m, d). Howard Hinnant's `civil_from_days`
/// algorithm (public domain), exact for the full i64 day range we use.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Unix-ms → `"YYYY-MM-DDTHH:MM:SSZ"` (UTC, second precision).
#[must_use]
pub fn ms_to_iso8601(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400); // [0, 86399]
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mi = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mi:02}:{ss:02}Z")
}

/// Unix-ms → billing period `"YYYY-MM"` (UTC).
#[must_use]
pub fn period_from_ms(unix_ms: u64) -> String {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX);
    let (y, m, _) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}")
}

// ─── Row-extraction helpers ───────────────────────────────────────────────────

/// Extract an optional string column (`None` for SQL NULL / absent).
fn col_opt_str(row: &D1Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Extract an optional integer column (`None` for SQL NULL / absent).
fn col_opt_i64(row: &D1Row, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

// ─── D1CustomerHandler ────────────────────────────────────────────────────────

/// Self-serve PAT TTL: 90 days (the canonical rotation cadence from
/// migration 0037's `pat.expires_ms` contract).
const SELF_SERVE_PAT_TTL: Duration = Duration::from_secs(90 * 86_400);

/// Production D1-backed customer handler. Implements all 6
/// `corelink-handler-customer` traits over the [`CustomerD1`] seam.
/// See the module docs for the per-endpoint HONEST-v1 matrix.
pub struct D1CustomerHandler {
    /// D1 row source (production: [`D1HttpCustomerDb`]).
    db: Arc<dyn CustomerD1>,
    /// Stripe billing-portal creator; `None` when `STRIPE_SECRET_KEY`
    /// is absent (portal requests then fail CLOSED with 500, never a
    /// fabricated URL).
    portal: Option<Arc<dyn PortalSessions>>,
    /// `return_url` for portal sessions (the dashboard billing page).
    portal_return_url: String,
    /// PAT signing key + key generation; `None` when `PAT_SIGNING_KEY`
    /// is absent (key creation then fails CLOSED with 500).
    signing: Option<(Arc<PatSigningKey>, u32)>,
    /// Audit sink collaborator (fail-CLOSED ordering per trait docs).
    audit: Arc<dyn AuditSink>,
    /// SLI observer collaborator (emit on EVERY return path).
    sli: Arc<dyn SliObserver>,
    /// Wall clock — the routes pass `at_unix_ms = 0`, so every real
    /// timestamp comes from here.
    clock: Arc<dyn WallClock>,
}

impl core::fmt::Debug for D1CustomerHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Redaction marker only: the signing key + Stripe bearer must
        // never surface through a leaked Debug.
        f.debug_struct("D1CustomerHandler")
            .field("db", &"[CustomerD1]")
            .field("portal", &self.portal.is_some())
            .field("signing", &self.signing.is_some())
            .finish_non_exhaustive()
    }
}

impl D1CustomerHandler {
    /// Construct from explicit collaborators (production wiring + tests).
    #[must_use]
    pub fn new(
        db: Arc<dyn CustomerD1>,
        portal: Option<Arc<dyn PortalSessions>>,
        portal_return_url: String,
        signing: Option<(Arc<PatSigningKey>, u32)>,
        audit: Arc<dyn AuditSink>,
        sli: Arc<dyn SliObserver>,
        clock: Arc<dyn WallClock>,
    ) -> Self {
        Self {
            db,
            portal,
            portal_return_url,
            signing,
            audit,
            sli,
            clock,
        }
    }

    /// Build the production handler from process env. `None` when the
    /// D1 config ([`crate::storage::StorageEnv`]) is absent/invalid —
    /// the caller then keeps the InMemory handler (dev/CI), mirroring
    /// [`crate::adapter_pat::PatVerifier::from_env`]'s fail-closed
    /// pattern. The Stripe client and PAT signing key are OPTIONAL
    /// per-capability inputs: when absent, only billing-portal /
    /// key-creation fail CLOSED (500) while every read surface stays
    /// real.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "customer_d1: D1 client init failed"))
            .ok()?;

        // PAT signing key — optional capability (key creation).
        let signing = crate::storage::non_empty_env("PAT_SIGNING_KEY")
            .and_then(|hex_str| {
                hex::decode(hex_str.trim())
                    .map_err(|_| {
                        tracing::warn!(
                            "customer_d1: PAT_SIGNING_KEY is not valid hex; \
                             self-serve key creation will fail CLOSED (500)"
                        );
                    })
                    .ok()
            })
            .and_then(|bytes| {
                PatSigningKey::from_bytes(bytes)
                    .map_err(|e| {
                        tracing::warn!(
                            error = %e,
                            "customer_d1: PAT_SIGNING_KEY invalid; \
                             self-serve key creation will fail CLOSED (500)"
                        );
                    })
                    .ok()
            })
            .map(|key| (Arc::new(key), 1u32));
        if signing.is_none() {
            tracing::warn!(
                "customer_d1: PAT_SIGNING_KEY unset/invalid; \
                 POST /v1/customer/keys will fail CLOSED (500)"
            );
        }

        // Stripe client — optional capability (billing portal).
        let portal: Option<Arc<dyn PortalSessions>> =
            match corelink_stripe_real::StripeRealClient::from_env() {
                Ok(stripe) => Some(Arc::new(StripePortalSessions::new(Arc::new(stripe)))),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "customer_d1: Stripe client init failed; \
                         POST /v1/customer/billing/portal will fail CLOSED (500)"
                    );
                    None
                }
            };

        let portal_return_url = std::env::var("CORELINK_PORTAL_RETURN_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://corelink-app.humangr.com/en/customer/billing".to_owned());

        Some(Arc::new(Self::new(
            Arc::new(D1HttpCustomerDb::new(Arc::new(d1))),
            portal,
            portal_return_url,
            signing,
            Arc::new(TracingCustomerAuditSink::new()),
            Arc::new(TracingCustomerSliObserver::new()),
            crate::wall_clock::default_wall_clock(),
        )))
    }

    // ── Shared plumbing ───────────────────────────────────────────────────────

    /// Emit one `Sli::AvailControlPlane` observation.
    fn emit_sli(&self, is_error: bool) {
        self.sli
            .observe(SliObservation::new(Sli::AvailControlPlane, is_error, 0));
    }

    /// Emit an audit event (wall-clock timestamped — NEVER the request's
    /// `at_unix_ms`, which the routes pin to 0); `AuditFailed` +
    /// error-SLI on sink failure (fail-CLOSED, BEFORE any lookup).
    fn emit_audit(
        &self,
        kind: AuditEventKind,
        tenant: &str,
        principal: &str,
        resource: &str,
    ) -> Result<(), CustomerHandlerError> {
        self.audit
            .emit(AuditEvent::new(
                kind,
                tenant,
                principal,
                resource,
                self.clock.now_ms(),
            ))
            .map_err(|e| {
                self.emit_sli(true);
                CustomerHandlerError::AuditFailed(e)
            })
    }

    /// Run one parameterised D1 statement; fail-CLOSED: any transport /
    /// decode error maps to `Internal` (→ 500) + error SLI. NEVER
    /// degraded to fabricated empty data.
    fn run(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, CustomerHandlerError> {
        self.db.query(sql, binds).map_err(|e| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!("customer_d1: {e}"))
        })
    }

    /// Fetch the tenant row (`tier` / `clerk_user_id` / `byok_status` /
    /// `created_at_ms`); `Ok(None)` when the tenant does not exist.
    fn tenant_row(&self, tenant_id: &str) -> Result<Option<D1Row>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT tenant_id, tier, clerk_user_id, byok_status, created_at_ms \
             FROM tenant WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        Ok(rows.into_iter().next())
    }

    /// Tenant-not-found error (the canonical 404 shape).
    fn tenant_not_found(&self, tenant_id: &str) -> CustomerHandlerError {
        self.emit_sli(true);
        CustomerHandlerError::NotFound {
            what: format!("tenant={tenant_id}"),
        }
    }

    /// Real storage usage: `SUM(bytes_used)` across the tenant's
    /// per-region rows + the `MAX(bytes_quota)` snapshot (the quota is
    /// denormalized per region row; `MAX` avoids multiplying it by the
    /// region count). Zero rows → honest (0, 0).
    fn storage_usage(&self, tenant_id: &str) -> Result<(u64, u64), CustomerHandlerError> {
        let rows = self.run(
            "SELECT COALESCE(SUM(bytes_used), 0) AS bytes_used, \
                    COALESCE(MAX(bytes_quota), 0) AS bytes_quota \
             FROM tenant_storage_state WHERE tenant_id = ?1",
            vec![json!(tenant_id)],
        )?;
        let row = rows.into_iter().next().unwrap_or_default();
        let used = col_opt_i64(&row, "bytes_used").unwrap_or(0);
        let quota = col_opt_i64(&row, "bytes_quota").unwrap_or(0);
        Ok((
            u64::try_from(used).unwrap_or(0),
            u64::try_from(quota).unwrap_or(0),
        ))
    }

    /// `tenant_billing` row (0055), if any.
    fn billing_row(&self, tenant_id: &str) -> Result<Option<D1Row>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT status, plan, stripe_customer_id, current_period_end_ms \
             FROM tenant_billing WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        Ok(rows.into_iter().next())
    }

    /// BYOK status for the dashboard. HONEST mapping: a tenant with NO
    /// `byok_envelope` row has no BYOK configured → `"none"` — the
    /// `tenant.byok_status` column (0031) defaults to `'active'` for
    /// every tenant because it is the kill-switch state, NOT a
    /// "customer configured BYOK" flag, so it is only surfaced once an
    /// envelope exists.
    fn byok_status(
        &self,
        tenant_id: &str,
        tenant: Option<&D1Row>,
    ) -> Result<ByokStatus, CustomerHandlerError> {
        let envelope = self.run(
            "SELECT 1 AS present FROM byok_envelope WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        if envelope.is_empty() {
            return Ok(ByokStatus::new("none", None, None));
        }
        let status = tenant
            .and_then(|row| col_opt_str(row, "byok_status"))
            .unwrap_or_else(|| "active".to_owned());
        Ok(ByokStatus::new(status, None, None))
    }
}

// ─── Trait impls ──────────────────────────────────────────────────────────────

impl CustomerOverviewHandler for D1CustomerHandler {
    fn overview(&self, req: OverviewRequest) -> Result<OverviewResponse, CustomerHandlerError> {
        // Attempted audit BEFORE any lookup (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::OverviewAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;
        let (cas_bytes, quota_bytes) = self.storage_usage(&req.caller_tenant)?;
        let billing = self.billing_row(&req.caller_tenant)?;
        let byok = self.byok_status(&req.caller_tenant, Some(&tenant))?;

        let plan = col_opt_str(&tenant, "tier").unwrap_or_else(|| "free".to_owned());
        let next_invoice_at = billing
            .as_ref()
            .and_then(|b| col_opt_i64(b, "current_period_end_ms"))
            .map(ms_to_iso8601)
            .unwrap_or_default();
        let billing_status = map_billing_status(
            billing
                .as_ref()
                .and_then(|b| col_opt_str(b, "status"))
                .as_deref(),
        );

        let resp = OverviewResponse::new(
            req.caller_tenant.clone(),
            // HONEST v1: no display-name column exists; the tenant id IS
            // the name (never a fabricated company name).
            req.caller_tenant.clone(),
            plan,
            OverviewUsage::new(
                period_from_ms(self.clock.now_ms()),
                cas_bytes,
                quota_bytes,
                // reads/writes are not tracked per-tenant yet: honest 0.
                0,
                0,
            ),
            // amount_due_cents is not materialized in D1: honest 0.
            OverviewBilling::new(billing_status, next_invoice_at, 0, "usd"),
            byok,
            // No customer-facing activity table yet: honest empty.
            Vec::<CustomerAuditEventRow>::new(),
        );

        self.emit_audit(
            AuditEventKind::OverviewServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}

impl CustomerUsageHandler for D1CustomerHandler {
    fn usage(&self, req: UsageRequest) -> Result<UsageResponse, CustomerHandlerError> {
        self.emit_audit(
            AuditEventKind::UsageAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        if self.tenant_row(&req.caller_tenant)?.is_none() {
            return Err(self.tenant_not_found(&req.caller_tenant));
        }
        let (cas_bytes, quota_bytes) = self.storage_usage(&req.caller_tenant)?;

        let current_period = period_from_ms(self.clock.now_ms());
        let requested = req.period.clone().unwrap_or_else(|| current_period.clone());
        // HONEST: only the CURRENT period's running counter exists in
        // `tenant_storage_state`; a historical period has no retained
        // data and reports 0 bytes rather than mislabeling today's.
        let period_bytes = if requested == current_period {
            cas_bytes
        } else {
            0
        };

        let resp = UsageResponse::new(
            requested,
            period_bytes,
            // reads/writes are not tracked per-tenant yet: honest 0.
            0,
            0,
            quota_bytes,
            // No per-day rollup table yet: honest empty.
            Vec::<DailyUsageBucket>::new(),
        );

        self.emit_audit(
            AuditEventKind::UsageServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}

impl CustomerBillingHandler for D1CustomerHandler {
    fn billing(&self, req: BillingRequest) -> Result<BillingResponse, CustomerHandlerError> {
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;
        let billing = self.billing_row(&req.caller_tenant)?;
        // Only an ACTIVE subscription's tier is the customer's real plan; a
        // `pending_checkout` row (paid tier written at click time, before
        // payment) must NOT surface as the active plan. Mirrors the
        // enforcement filter in `worker/src/lib/quota.ts::getTierForTenant`.
        let tier_rows = self.run(
            "SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active' LIMIT 1",
            vec![json!(req.caller_tenant)],
        )?;

        // Plan slug: tier_selections (canonical FSM) first, then the
        // tenant.tier fallback (0057's documented read order).
        let plan = tier_rows
            .first()
            .and_then(|r| col_opt_str(r, "tier"))
            .or_else(|| col_opt_str(&tenant, "tier"))
            .unwrap_or_else(|| "free".to_owned());
        let status = map_billing_status(
            billing
                .as_ref()
                .and_then(|b| col_opt_str(b, "status"))
                .as_deref(),
        );
        let current_period_end = billing
            .as_ref()
            .and_then(|b| col_opt_i64(b, "current_period_end_ms"))
            .map(ms_to_iso8601)
            .unwrap_or_default();

        let resp = BillingResponse::new(
            status,
            plan,
            // Period START is not materialized in tenant_billing: honest
            // empty string, never a guessed date.
            String::new(),
            current_period_end,
            // amount_due_cents is not materialized in D1: honest 0.
            0,
            "usd",
            // No invoice-history read surface yet: honest empty.
            Vec::<InvoiceRow>::new(),
        );

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }

    fn portal_url(&self, req: PortalRequest) -> Result<PortalResponse, CustomerHandlerError> {
        // Portal is a billing sub-action (audit symmetry with InMemory).
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "portal",
        )?;

        let billing = self.billing_row(&req.caller_tenant)?;
        let Some(customer_id) = billing
            .as_ref()
            .and_then(|b| col_opt_str(b, "stripe_customer_id"))
            .filter(|c| !c.is_empty())
        else {
            // FROZEN: no Stripe customer id → 404 "no billing account".
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("no billing account for tenant={}", req.caller_tenant),
            });
        };

        let Some(portal) = self.portal.as_ref() else {
            // Fail-CLOSED: Stripe unconfigured is an operator fault, not
            // a customer 404.
            self.emit_sli(true);
            return Err(CustomerHandlerError::Internal(
                "customer_d1: Stripe client not configured; portal unavailable".to_owned(),
            ));
        };
        // Idempotency key varies per request on purpose: portal sessions
        // are short-lived and a fresh one per click is the Stripe-
        // documented shape.
        let idem = format!("portal:{}:{}", req.caller_tenant, self.clock.now_ms());
        let url = portal
            .create(&customer_id, &self.portal_return_url, &idem)
            .map_err(|e| {
                self.emit_sli(true);
                CustomerHandlerError::Internal(format!("customer_d1: {e}"))
            })?;

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "portal",
        )?;
        self.emit_sli(false);
        Ok(PortalResponse::new(url))
    }
}

impl CustomerKeysHandler for D1CustomerHandler {
    fn list(&self, req: KeysListRequest) -> Result<KeysListResponse, CustomerHandlerError> {
        // Read-only, always self-tenant-scoped (same as InMemory: no
        // Attempted audit kind exists for list).
        let rows = self.run(
            // `name` + `revoked_at_ms` are the WP-2 columns (migration
            // 0063, landing in a parallel PR — this PR depends on it).
            "SELECT pat_id, name, scope, created_ms, revoked_at_ms \
             FROM pat WHERE tenant_id = ?1 ORDER BY created_ms DESC",
            vec![json!(req.caller_tenant)],
        )?;
        let pats: Vec<PatRow> = rows
            .iter()
            .map(|row| {
                PatRow::new(
                    col_opt_str(row, "pat_id").unwrap_or_default(),
                    // Pre-0063 rows have no name: honest empty string.
                    col_opt_str(row, "name").unwrap_or_default(),
                    scope_to_list(&col_opt_str(row, "scope").unwrap_or_default()),
                    col_opt_i64(row, "created_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    // last-used is not tracked yet: honest None.
                    None,
                    col_opt_i64(row, "revoked_at_ms").map(ms_to_iso8601),
                )
            })
            .collect();

        let tenant = self.tenant_row(&req.caller_tenant)?;
        let byok = self.byok_status(&req.caller_tenant, tenant.as_ref())?;

        self.emit_sli(false);
        Ok(KeysListResponse::new(pats, byok))
    }

    fn create(&self, req: KeyCreateRequest) -> Result<KeyCreateResponse, CustomerHandlerError> {
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyCreateAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
        )?;

        let Some((signing_key, signing_key_id)) = self.signing.as_ref() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::Internal(
                "customer_d1: PAT signing key not configured; key creation unavailable".to_owned(),
            ));
        };

        // FROZEN scope map ('admin' NEVER grantable).
        let scope = map_requested_scopes(&req.scopes).inspect_err(|_| self.emit_sli(true))?;
        let scope_bits = if scope == "read-write" {
            PatScopes::from_u64(SCOPE_CACHE_RW)
        } else {
            PatScopes::from_u64(SCOPE_CACHE_R)
        };

        let tenant_uuid = Uuid::parse_str(&req.caller_tenant).map_err(|_| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!(
                "customer_d1: tenant id is not a UUID: {}",
                req.caller_tenant
            ))
        })?;

        let (plaintext, pat) = mint(
            PatEnv::Pat,
            TenantId(tenant_uuid),
            // The dashboard principal is the token-prefix string (not a
            // UUID); the PAT's embedded principal is the owning tenant —
            // the same identity the first-PAT signup mint binds.
            PrincipalId(tenant_uuid),
            scope_bits,
            Some(SELF_SERVE_PAT_TTL),
            signing_key,
            *signing_key_id,
        )
        .map_err(|e| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!("customer_d1: PAT mint failed: {e}"))
        })?;

        let now_ms = self.clock.now_ms();
        let expires_ms: u64 = pat
            .expires_at
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        // Durable INSERT. `shown_once_token` (NOT NULL UNIQUE, 0037) is
        // a fresh UUID immediately marked consumed: the dashboard
        // returns the plaintext in THIS response (shown once) and the
        // reveal-endpoint path is never used for self-serve keys.
        self.run(
            "INSERT INTO pat \
             (pat_id, tenant_id, pat_hash, scope, expires_ms, \
              shown_once_token, shown_once_consumed, created_ms, token_id, name) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)",
            vec![
                json!(pat.id.to_string()),
                json!(req.caller_tenant),
                json!(pat.hash.as_str()),
                json!(scope),
                json!(i64::try_from(expires_ms).unwrap_or(i64::MAX)),
                json!(Uuid::now_v7().to_string()),
                json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
                json!(pat.token_id.as_str()),
                json!(req.name),
            ],
        )?;

        let row = PatRow::new(
            pat.id.to_string(),
            req.name.clone(),
            scope_to_list(scope),
            ms_to_iso8601(i64::try_from(now_ms).unwrap_or(i64::MAX)),
            None,
            None,
        );

        self.emit_audit(
            AuditEventKind::KeyCreateCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
        )?;
        self.emit_sli(false);
        // The plaintext is returned ONCE here and is NEVER logged.
        Ok(KeyCreateResponse::new(row, plaintext.into_string()))
    }

    fn revoke(&self, req: KeyRevokeRequest) -> Result<KeyRevokeResponse, CustomerHandlerError> {
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyRevokeAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
        )?;

        // Tenant-scoped SELECT: a PAT owned by another tenant is simply
        // not visible → NotFound (cross-tenant safe by construction).
        let rows = self.run(
            "SELECT pat_id, name, scope, created_ms, revoked_at_ms \
             FROM pat WHERE pat_id = ?1 AND tenant_id = ?2 LIMIT 1",
            vec![json!(req.pat_id), json!(req.caller_tenant)],
        )?;
        let Some(row) = rows.into_iter().next() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("pat_id={} for tenant={}", req.pat_id, req.caller_tenant),
            });
        };

        let existing_revoked = col_opt_i64(&row, "revoked_at_ms");
        let revoked_at_ms = if let Some(already) = existing_revoked {
            // Idempotent: keep the original revocation timestamp.
            already
        } else {
            let now = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
            self.run(
                "UPDATE pat SET revoked_at_ms = ?1 \
                 WHERE pat_id = ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL",
                vec![json!(now), json!(req.pat_id), json!(req.caller_tenant)],
            )?;
            now
        };

        let pat = PatRow::new(
            col_opt_str(&row, "pat_id").unwrap_or_else(|| req.pat_id.clone()),
            col_opt_str(&row, "name").unwrap_or_default(),
            scope_to_list(&col_opt_str(&row, "scope").unwrap_or_default()),
            col_opt_i64(&row, "created_ms")
                .map(ms_to_iso8601)
                .unwrap_or_default(),
            None,
            Some(ms_to_iso8601(revoked_at_ms)),
        );

        self.emit_audit(
            AuditEventKind::KeyRevokeCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
        )?;
        self.emit_sli(false);
        Ok(KeyRevokeResponse::new(pat))
    }
}

impl CustomerTeamHandler for D1CustomerHandler {
    fn list(&self, req: TeamListRequest) -> Result<TeamListResponse, CustomerHandlerError> {
        // The tenant's Clerk binding is the canonical OWNER (synthesized — the
        // email is NOT stored in D1, only `email_hash` per CTRL-PRIV-001, so it
        // is shown as "—"). Additional seats are real `team_member` rows
        // (ADR-S33-001, migration 0074), appended below.
        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;

        let owner_id =
            col_opt_str(&tenant, "clerk_user_id").unwrap_or_else(|| req.caller_tenant.clone());
        let joined_at = col_opt_i64(&tenant, "created_at_ms")
            .map(ms_to_iso8601)
            .unwrap_or_default();
        let mut members = vec![TeamMemberRow::new(
            owner_id.clone(),
            "—",
            "Owner",
            joined_at,
            "active",
        )];

        // Seats from `team_member` (active + invited; `removed` rows are audit
        // tombstones and not listed). Skip any duplicate of the synthesized owner.
        let rows = self.run(
            "SELECT user_id, role, status, joined_at_ms, invited_at_ms \
             FROM team_member \
             WHERE tenant_id = ?1 AND status IN ('active','invited') \
             ORDER BY invited_at_ms ASC",
            vec![json!(req.caller_tenant)],
        )?;
        for row in &rows {
            let user_id = col_opt_str(row, "user_id").unwrap_or_default();
            if user_id.is_empty() || user_id == owner_id {
                continue;
            }
            let role = col_opt_str(row, "role").unwrap_or_else(|| "member".to_owned());
            let status = col_opt_str(row, "status").unwrap_or_else(|| "invited".to_owned());
            let joined = col_opt_i64(row, "joined_at_ms")
                .or_else(|| col_opt_i64(row, "invited_at_ms"))
                .map(ms_to_iso8601)
                .unwrap_or_default();
            members.push(TeamMemberRow::new(user_id, "—", role, joined, status));
        }

        self.emit_sli(false);
        Ok(TeamListResponse::new(members))
    }

    fn invite(&self, req: TeamInviteRequest) -> Result<TeamInviteResponse, CustomerHandlerError> {
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering) — mirrors
        // `remove()` / `create()`.
        self.emit_audit(
            AuditEventKind::TeamInviteAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
        )?;

        // ADR-S33-001 WP-T2: durably record the invite as a `team_member` row
        // (status `invited`, migration 0074). NO synchronous Clerk call (OB-1) —
        // the invitation EMAIL round-trip is an owner follow-up; acceptance is
        // bound later by the signup-worker `acceptTeamInvitation` helper (C-ACCEPT),
        // which matches the row by `email_hash` and rebinds `user_id` + flips it to
        // `active`.
        //
        // CTRL-PRIV-001: only the pseudonymized SHA-256 email hash is stored, never
        // the raw invitee email (mirrors the rest of the D1 schema + 0074's header).
        // NORMALIZE (trim + lowercase) BEFORE hashing — this is the join key the
        // signup-worker `acceptTeamInvitation` (C-ACCEPT, `emailHashFor`) matches on,
        // and it normalizes identically; without it an invite to `Alice@Example.com`
        // would never flip to `active` when Clerk delivers `alice@example.com`.
        let email_hash = hex::encode(Sha256::digest(req.email.trim().to_lowercase().as_bytes()));
        // The role is collapsed onto the FROZEN 0074 CHECK domain (CHECK-safe).
        let role = normalize_invite_role(&req.role);
        // No real Clerk user_id exists yet (OB-1) — `team_member.user_id` is NOT
        // NULL (PK), so a fresh UUID is the invitation-id placeholder 0074 expects
        // ("carries the Clerk invitation id until acceptance binds the real user").
        let invitation_id = Uuid::now_v7().to_string();
        let invited_at_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);

        // `joined_at_ms` is left NULL until acceptance flips the seat to `active`.
        self.run(
            "INSERT INTO team_member \
             (tenant_id, user_id, email_hash, role, status, invited_by, invited_at_ms) \
             VALUES (?1, ?2, ?3, ?4, 'invited', ?5, ?6)",
            vec![
                json!(req.caller_tenant),
                json!(invitation_id),
                json!(email_hash),
                json!(role),
                json!(req.principal),
                json!(invited_at_ms),
            ],
        )?;

        self.emit_audit(
            AuditEventKind::TeamInviteCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
        )?;

        // Response mirrors the `InMemoryCustomerHandler::invite` shape (echoes the
        // caller-supplied email + requested role; status `invited`). The raw email
        // is reflected back to the caller that supplied it — it is NOT persisted
        // (only `email_hash` is).
        let member = TeamMemberRow::new(
            invitation_id,
            req.email.clone(),
            req.role.clone(),
            String::new(),
            "invited",
        );
        self.emit_sli(false);
        Ok(TeamInviteResponse::new(member))
    }

    fn remove(&self, req: TeamRemoveRequest) -> Result<TeamRemoveResponse, CustomerHandlerError> {
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::TeamRemoveAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
        )?;

        // The member must exist under THIS tenant and not be the owner.
        let rows = self.run(
            "SELECT role, status FROM team_member WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;
        let Some(row) = rows.into_iter().next() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("team member={}", req.target_user_id),
            });
        };
        let role = col_opt_str(&row, "role").unwrap_or_default();
        if role.eq_ignore_ascii_case("owner") {
            self.emit_sli(true);
            return Err(CustomerHandlerError::Unauthorized(
                "cannot remove the tenant owner".to_owned(),
            ));
        }
        // Count the member's live PATs BEFORE revoking (the D1 query bridge
        // returns rows, not an UPDATE changes-count) — this is the revoked total.
        let live = self.run(
            "SELECT pat_id FROM pat \
             WHERE tenant_id = ?1 AND principal_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;
        let revoked_pats = u32::try_from(live.len()).unwrap_or(u32::MAX);

        let now = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);

        // Revoke every live PAT the member holds for this tenant — the
        // load-bearing security effect of seat removal.
        self.run(
            "UPDATE pat SET revoked_at_ms = ?3 \
             WHERE tenant_id = ?1 AND principal_id = ?2 AND revoked_at_ms IS NULL",
            vec![
                json!(req.caller_tenant),
                json!(req.target_user_id),
                json!(now),
            ],
        )?;

        // Flip the seat to `removed` (retain as an audit tombstone).
        self.run(
            "UPDATE team_member SET status = 'removed', joined_at_ms = joined_at_ms \
             WHERE tenant_id = ?1 AND user_id = ?2",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;

        self.emit_audit(
            AuditEventKind::TeamRemoveCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
        )?;

        let member = TeamMemberRow::new(
            req.target_user_id.clone(),
            "—",
            if role.is_empty() { "member".to_owned() } else { role },
            String::new(),
            "removed",
        );
        self.emit_sli(false);
        Ok(TeamRemoveResponse::new(member, revoked_pats))
    }
}

impl CustomerAuditHandler for D1CustomerHandler {
    fn query(&self, req: AuditQueryRequest) -> Result<AuditQueryResponse, CustomerHandlerError> {
        self.emit_audit(
            AuditEventKind::AuditQueryAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        // HONEST v1: no customer-queryable audit table is deployed (the
        // canonical chain lives in the R2 NDJSON archive, served by
        // /v1/audit/export) — explicit empty rows, never synthesized
        // events.
        let resp = AuditQueryResponse::new(Vec::<CustomerAuditEventRow>::new());

        self.emit_audit(
            AuditEventKind::AuditQueryServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use std::sync::Mutex;

    use corelink_handler_customer::{InMemoryAuditSink, InMemorySliObserver};

    use super::*;
    use crate::wall_clock::InMemoryFakeWallClock;

    /// 2023-11-14T22:13:20Z — a fixed, verifiable instant.
    const NOW_MS: u64 = 1_700_000_000_000;
    const TENANT: &str = "0192f0c1-2345-7890-abcd-ef0123456789";
    const OTHER_TENANT: &str = "0192f0c1-9999-7890-abcd-ef0123456789";

    // ── Mock D1 ──────────────────────────────────────────────────────────────

    /// Hermetic mock row source: canned result sets keyed by an SQL
    /// fragment, every executed (sql, binds) recorded for assertions.
    /// Mirrors the billing_d1_http "no live D1 in unit tests" posture.
    #[derive(Debug, Default)]
    struct MockD1 {
        canned: Vec<(&'static str, Vec<D1Row>)>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
        fail: bool,
    }

    impl MockD1 {
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
            self.calls.lock().unwrap().clone()
        }
    }

    impl CustomerD1 for MockD1 {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            self.calls.lock().unwrap().push((sql.to_owned(), binds));
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

    #[derive(Debug)]
    struct MockPortal {
        url: &'static str,
        fail: bool,
    }

    impl PortalSessions for MockPortal {
        fn create(
            &self,
            stripe_customer_id: &str,
            return_url: &str,
            _idempotency_key: &str,
        ) -> Result<String, String> {
            if self.fail {
                return Err("stripe down".to_owned());
            }
            assert!(!stripe_customer_id.is_empty());
            assert!(!return_url.is_empty());
            Ok(self.url.to_owned())
        }
    }

    fn row(pairs: &[(&str, Value)]) -> D1Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn tenant_row_fixture() -> D1Row {
        row(&[
            ("tenant_id", json!(TENANT)),
            ("tier", json!("pro")),
            ("clerk_user_id", json!("user_2abc")),
            ("byok_status", json!("active")),
            ("created_at_ms", json!(1_690_000_000_000_i64)),
        ])
    }

    struct Fixture {
        handler: D1CustomerHandler,
        db: Arc<MockD1>,
        audit: Arc<InMemoryAuditSink>,
        sli: Arc<InMemorySliObserver>,
    }

    fn fixture_with(db: MockD1, portal: Option<Arc<dyn PortalSessions>>) -> Fixture {
        let db = Arc::new(db);
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let signing = PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap();
        let handler = D1CustomerHandler::new(
            db.clone(),
            portal,
            "https://corelink-app.humangr.com/en/customer/billing".to_owned(),
            Some((Arc::new(signing), 1)),
            audit.clone(),
            sli.clone(),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
        );
        Fixture {
            handler,
            db,
            audit,
            sli,
        }
    }

    // ── Frozen maps ──────────────────────────────────────────────────────────

    #[test]
    fn billing_status_map_is_frozen() {
        // The FROZEN table from the WP-3 design.
        assert_eq!(map_billing_status(Some("paid")), "active");
        assert_eq!(map_billing_status(Some("past_due")), "past_due");
        assert_eq!(map_billing_status(Some("canceled")), "canceled");
        assert_eq!(map_billing_status(Some("incomplete")), "past_due");
        assert_eq!(map_billing_status(Some("inactive")), "inactive");
        assert_eq!(map_billing_status(None), "inactive"); // no-row
        assert_eq!(map_billing_status(Some("weird")), "inactive");
    }

    /// AUDIT REV-S5 (known-limitation guard): the dashboard status the map
    /// emits MUST stay inside the frozen cross-team union
    /// (`apps/admin-ui/src/lib/customer-types.ts`:
    /// `"trialing" | "active" | "past_due" | "canceled" | "inactive"`) and
    /// the `tenant_billing.status` CHECK in
    /// `specs/03_architecture/data_model.md` (`active`/`past_due`/`canceled`).
    /// In particular `incomplete` (pending FIRST payment) is collapsed onto
    /// `past_due` ON PURPOSE — there is no `pending`/`awaiting_payment` value
    /// in the frozen contract yet. If a future change starts emitting a new
    /// string from this map, it MUST widen that union + the spec CHECK in the
    /// SAME change; this test is the tripwire that forces that coordination.
    #[test]
    fn billing_status_map_stays_in_frozen_dashboard_union() {
        const FROZEN_DASHBOARD_STATUSES: &[&str] =
            &["trialing", "active", "past_due", "canceled", "inactive"];
        for d1_status in [
            Some("paid"),
            Some("past_due"),
            Some("incomplete"),
            Some("canceled"),
            Some("inactive"),
            Some("unrecognized_future_value"),
            None,
        ] {
            let mapped = map_billing_status(d1_status);
            assert!(
                FROZEN_DASHBOARD_STATUSES.contains(&mapped),
                "map_billing_status({d1_status:?}) = {mapped:?} is OUTSIDE the \
                 frozen dashboard status union {FROZEN_DASHBOARD_STATUSES:?} \
                 (REV-S5): widen apps/admin-ui customer-types.ts + the \
                 data_model.md CHECK in the same change before emitting it",
            );
        }
        // The specific REV-S5 collapse is intentional and asserted here so
        // the deferral is explicit, not accidental.
        assert_eq!(
            map_billing_status(Some("incomplete")),
            "past_due",
            "REV-S5: 'incomplete' deliberately collapses onto 'past_due' until \
             a distinct 'pending' status is added to the frozen contract",
        );
    }

    #[test]
    fn scope_map_is_frozen() {
        assert_eq!(
            map_requested_scopes(&["cache:read".to_owned()]).unwrap(),
            "read-only"
        );
        assert_eq!(
            map_requested_scopes(&["cache:read".to_owned(), "cache:write".to_owned()]).unwrap(),
            "read-write"
        );
        assert_eq!(
            map_requested_scopes(&["cas:rw".to_owned()]).unwrap(),
            "read-write"
        );
        // Least privilege for an empty request.
        assert_eq!(map_requested_scopes(&[]).unwrap(), "read-only");
        // 'admin' is NEVER grantable.
        let err = map_requested_scopes(&["admin".to_owned()]).unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::Unauthorized(_)),
            "{err:?}"
        );
        // rt-nuclear #15 REGRESSION: a substring-y token like "writes" must NOT
        // silently persist `read-write` (the old `s.contains("write")` mapper bug
        // that let a read-only PAT self-escalate). It is unrecognized ⇒ REJECTED
        // (fail-CLOSED) — this is where the escalation is actually closed, since
        // the mint gate intentionally treats unknown tokens as non-write.
        for evil in ["writes", "cache:write-x", "my-write"] {
            let err = map_requested_scopes(&[evil.to_owned()]).unwrap_err();
            assert!(
                matches!(err, CustomerHandlerError::Unauthorized(_)),
                "{evil:?} must be rejected, not mapped to read-write; got {err:?}"
            );
        }
    }

    // ── Calendar helpers ─────────────────────────────────────────────────────

    #[test]
    fn ms_to_iso8601_known_instants() {
        assert_eq!(ms_to_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            ms_to_iso8601(i64::try_from(NOW_MS).unwrap()),
            "2023-11-14T22:13:20Z"
        );
    }

    #[test]
    fn period_from_ms_known_instants() {
        assert_eq!(period_from_ms(0), "1970-01");
        assert_eq!(period_from_ms(NOW_MS), "2023-11");
    }

    // ── Overview ─────────────────────────────────────────────────────────────

    #[test]
    fn overview_happy_path_real_data() {
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                (
                    "FROM tenant_storage_state",
                    vec![row(&[
                        ("bytes_used", json!(123_456_i64)),
                        ("bytes_quota", json!(10_000_000_i64)),
                    ])],
                ),
                (
                    "FROM tenant_billing",
                    vec![row(&[
                        ("status", json!("paid")),
                        ("plan", json!("price_123")),
                        ("stripe_customer_id", json!("cus_42")),
                        ("current_period_end_ms", json!(1_700_500_000_000_i64)),
                    ])],
                ),
                // byok_envelope: no rows → "none".
            ]),
            None,
        );
        let resp = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(resp.tenant_id, TENANT);
        assert_eq!(
            resp.tenant_name, TENANT,
            "tenant_name = tenant_id (HONEST v1)"
        );
        assert_eq!(resp.plan, "pro");
        assert_eq!(resp.usage.cas_bytes, 123_456);
        assert_eq!(resp.usage.quota_bytes, 10_000_000);
        assert_eq!(resp.usage.period, "2023-11");
        assert_eq!(resp.usage.reads, 0, "reads not tracked: honest 0");
        assert_eq!(resp.usage.writes, 0, "writes not tracked: honest 0");
        assert_eq!(resp.billing.status, "active", "paid → active (frozen map)");
        assert_eq!(
            resp.billing.next_invoice_at,
            ms_to_iso8601(1_700_500_000_000)
        );
        assert_eq!(resp.byok.status, "none", "no envelope row → none");
        assert!(
            resp.recent_activity.is_empty(),
            "no activity table: honest []"
        );
        // Audit ordering: Attempted then Served.
        let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AuditEventKind::OverviewAttempted,
                AuditEventKind::OverviewServed
            ]
        );
        // SLI emitted, success.
        let obs = f.sli.snapshot().unwrap();
        assert!(obs.iter().any(|o| !o.is_error));
    }

    #[test]
    fn overview_unknown_tenant_is_not_found() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let err = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::NotFound { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn overview_byok_envelope_present_surfaces_kill_switch_state() {
        let mut tenant = tenant_row_fixture();
        tenant.insert("byok_status".to_owned(), json!("degraded_read_only"));
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant]),
                ("FROM byok_envelope", vec![row(&[("present", json!(1))])]),
            ]),
            None,
        );
        let resp = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(resp.byok.status, "degraded_read_only");
    }

    #[test]
    fn overview_fails_closed_on_d1_transport_error() {
        let f = fixture_with(MockD1::failing(), None);
        let err = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
        // Error SLI emitted (fail-CLOSED observation).
        assert!(f.sli.snapshot().unwrap().iter().any(|o| o.is_error));
    }

    #[test]
    fn overview_audit_failure_aborts_before_any_d1_query() {
        let f = fixture_with(MockD1::with(vec![]), None);
        f.audit.inject_failure("sink down").unwrap();
        let err = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::AuditFailed(_)),
            "{err:?}"
        );
        assert!(
            f.db.calls().is_empty(),
            "fail-CLOSED: audit emit failure must abort BEFORE any lookup"
        );
    }

    // ── Usage ────────────────────────────────────────────────────────────────

    #[test]
    fn usage_happy_path_real_bytes_zero_ops_empty_daily() {
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                (
                    "FROM tenant_storage_state",
                    vec![row(&[
                        ("bytes_used", json!(512_i64)),
                        ("bytes_quota", json!(1_000_000_i64)),
                    ])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
            .unwrap();
        assert_eq!(resp.period, "2023-11", "current period from the wall clock");
        assert_eq!(resp.cas_bytes, 512);
        assert_eq!(resp.quota_bytes, 1_000_000);
        assert_eq!(resp.reads, 0);
        assert_eq!(resp.writes, 0);
        assert!(resp.daily.is_empty(), "no per-day table: honest []");
    }

    #[test]
    fn usage_historical_period_reports_honest_zero_bytes() {
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                (
                    "FROM tenant_storage_state",
                    vec![row(&[
                        ("bytes_used", json!(512_i64)),
                        ("bytes_quota", json!(1_000_000_i64)),
                    ])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .usage(UsageRequest::new(
                TENANT,
                "clpat_x",
                Some("2023-01".to_owned()),
                0,
            ))
            .unwrap();
        assert_eq!(resp.period, "2023-01");
        assert_eq!(
            resp.cas_bytes, 0,
            "historical data is not retained — never relabel today's counter"
        );
        assert_eq!(
            resp.quota_bytes, 1_000_000,
            "the quota ceiling is still real"
        );
    }

    // ── Billing ──────────────────────────────────────────────────────────────

    #[test]
    fn billing_happy_path_maps_status_and_tier() {
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                (
                    "FROM tenant_billing",
                    vec![row(&[
                        ("status", json!("past_due")),
                        ("plan", json!("price_123")),
                        ("stripe_customer_id", json!("cus_42")),
                        ("current_period_end_ms", json!(1_700_500_000_000_i64)),
                    ])],
                ),
                (
                    "FROM tier_selections",
                    vec![row(&[("tier", json!("starter"))])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .billing(BillingRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(resp.status, "past_due");
        assert_eq!(
            resp.plan, "starter",
            "tier_selections is the canonical tier"
        );
        assert_eq!(
            resp.current_period_start, "",
            "period start not tracked: honest empty"
        );
        assert_eq!(resp.current_period_end, ms_to_iso8601(1_700_500_000_000));
        assert_eq!(
            resp.amount_due_cents, 0,
            "amount due not materialized: honest 0"
        );
        assert!(
            resp.invoices.is_empty(),
            "no invoice surface yet: honest []"
        );
    }

    #[test]
    fn billing_no_rows_is_inactive_free() {
        let f = fixture_with(
            MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
            None,
        );
        let resp = f
            .handler
            .billing(BillingRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(
            resp.status, "inactive",
            "no tenant_billing row → inactive (frozen)"
        );
        assert_eq!(
            resp.plan, "pro",
            "tenant.tier fallback when no tier_selections row"
        );
    }

    // ── Billing portal ───────────────────────────────────────────────────────

    #[test]
    fn portal_happy_path_returns_stripe_url() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM tenant_billing",
                vec![row(&[
                    ("status", json!("paid")),
                    ("stripe_customer_id", json!("cus_42")),
                ])],
            )]),
            Some(Arc::new(MockPortal {
                url: "https://billing.stripe.com/p/session/live_123",
                fail: false,
            })),
        );
        let resp = f
            .handler
            .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(
            resp.portal_url,
            "https://billing.stripe.com/p/session/live_123"
        );
    }

    #[test]
    fn portal_without_billing_account_is_404() {
        // No tenant_billing row at all.
        let f = fixture_with(
            MockD1::with(vec![]),
            Some(Arc::new(MockPortal {
                url: "https://unused",
                fail: false,
            })),
        );
        let err = f
            .handler
            .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(
            matches!(&err, CustomerHandlerError::NotFound { what } if what.contains("no billing account")),
            "{err:?}"
        );

        // A row whose stripe_customer_id is NULL → same 404.
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM tenant_billing",
                vec![row(&[
                    ("status", json!("inactive")),
                    ("stripe_customer_id", Value::Null),
                ])],
            )]),
            Some(Arc::new(MockPortal {
                url: "https://unused",
                fail: false,
            })),
        );
        let err = f
            .handler
            .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::NotFound { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn portal_without_stripe_client_fails_closed() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM tenant_billing",
                vec![row(&[("stripe_customer_id", json!("cus_42"))])],
            )]),
            None,
        );
        let err = f
            .handler
            .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
    }

    #[test]
    fn portal_stripe_error_fails_closed() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM tenant_billing",
                vec![row(&[("stripe_customer_id", json!("cus_42"))])],
            )]),
            Some(Arc::new(MockPortal {
                url: "https://unused",
                fail: true,
            })),
        );
        let err = f
            .handler
            .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
    }

    // ── Keys ─────────────────────────────────────────────────────────────────

    #[test]
    fn keys_list_maps_pat_rows() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM pat WHERE tenant_id",
                vec![
                    row(&[
                        ("pat_id", json!("pat_1")),
                        ("name", json!("ci-key")),
                        ("scope", json!("read-write")),
                        ("created_ms", json!(1_690_000_000_000_i64)),
                        ("revoked_at_ms", Value::Null),
                    ]),
                    row(&[
                        ("pat_id", json!("pat_0")),
                        // Pre-0063 row: NULL name.
                        ("name", Value::Null),
                        ("scope", json!("read-only")),
                        ("created_ms", json!(1_680_000_000_000_i64)),
                        ("revoked_at_ms", json!(1_695_000_000_000_i64)),
                    ]),
                ],
            )]),
            None,
        );
        let resp =
            CustomerKeysHandler::list(&f.handler, KeysListRequest::new(TENANT, "clpat_x", 0))
                .unwrap();
        assert_eq!(resp.pats.len(), 2);
        assert_eq!(resp.pats[0].pat_id, "pat_1");
        assert_eq!(resp.pats[0].name, "ci-key");
        assert_eq!(
            resp.pats[0].scopes,
            vec!["cache:read".to_owned(), "cache:write".to_owned()]
        );
        assert_eq!(resp.pats[0].created_at, ms_to_iso8601(1_690_000_000_000));
        assert_eq!(
            resp.pats[0].last_used_at, None,
            "last-used not tracked: honest None"
        );
        assert_eq!(resp.pats[0].revoked_at, None);
        assert_eq!(
            resp.pats[1].name, "",
            "NULL name (pre-0063 row) → honest empty"
        );
        assert_eq!(
            resp.pats[1].revoked_at,
            Some(ms_to_iso8601(1_695_000_000_000))
        );
        assert_eq!(resp.byok.status, "none");
    }

    #[test]
    fn keys_create_mints_inserts_and_returns_token_once() {
        let f = fixture_with(
            MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
            None,
        );
        let resp = f
            .handler
            .create(KeyCreateRequest::new(
                TENANT,
                "clpat_x",
                "deploy-key",
                vec!["cache:read".to_owned(), "cache:write".to_owned()],
                0,
            ))
            .unwrap();
        assert!(
            resp.token.starts_with("corelink_pat_"),
            "real minted plaintext"
        );
        assert_eq!(resp.pat.name, "deploy-key");
        assert_eq!(
            resp.pat.scopes,
            vec!["cache:read".to_owned(), "cache:write".to_owned()]
        );
        assert_eq!(
            resp.pat.created_at,
            ms_to_iso8601(i64::try_from(NOW_MS).unwrap())
        );
        assert_eq!(resp.pat.revoked_at, None);

        // The INSERT carried the frozen 'read-write' scope + the name,
        // and the plaintext was NEVER bound into SQL.
        let calls = f.db.calls();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT INTO pat"))
            .expect("INSERT executed");
        assert_eq!(insert.1[3], json!("read-write"));
        assert_eq!(insert.1[8], json!("deploy-key"));
        assert!(
            !insert
                .1
                .iter()
                .any(|b| b.as_str().is_some_and(|s| s.contains(resp.token.as_str()))),
            "the PAT plaintext must never reach D1"
        );
        // Audit ordering: Attempted BEFORE the INSERT, then Committed.
        let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AuditEventKind::KeyCreateAttempted,
                AuditEventKind::KeyCreateCommitted
            ]
        );
    }

    #[test]
    fn keys_create_read_only_scope_for_cache_read() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let resp = f
            .handler
            .create(KeyCreateRequest::new(
                TENANT,
                "clpat_x",
                "ro-key",
                vec!["cache:read".to_owned()],
                0,
            ))
            .unwrap();
        assert_eq!(resp.pat.scopes, vec!["cache:read".to_owned()]);
        let calls = f.db.calls();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT INTO pat"))
            .expect("INSERT executed");
        assert_eq!(insert.1[3], json!("read-only"));
    }

    #[test]
    fn keys_create_admin_scope_is_never_grantable() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let err = f
            .handler
            .create(KeyCreateRequest::new(
                TENANT,
                "clpat_x",
                "evil",
                vec!["admin".to_owned()],
                0,
            ))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::Unauthorized(_)),
            "{err:?}"
        );
        assert!(
            !f.db.calls().iter().any(|(sql, _)| sql.contains("INSERT")),
            "no INSERT may run for a rejected scope"
        );
    }

    #[test]
    fn keys_create_without_signing_key_fails_closed() {
        let db = Arc::new(MockD1::with(vec![]));
        let handler = D1CustomerHandler::new(
            db,
            None,
            String::new(),
            None, // no signing key
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
        );
        let err = handler
            .create(KeyCreateRequest::new(TENANT, "clpat_x", "k", vec![], 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
    }

    #[test]
    fn keys_revoke_happy_path_updates_and_returns_revoked_at() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM pat WHERE pat_id",
                vec![row(&[
                    ("pat_id", json!("pat_1")),
                    ("name", json!("ci-key")),
                    ("scope", json!("read-only")),
                    ("created_ms", json!(1_690_000_000_000_i64)),
                    ("revoked_at_ms", Value::Null),
                ])],
            )]),
            None,
        );
        let resp = f
            .handler
            .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
            .unwrap();
        assert_eq!(
            resp.pat.revoked_at,
            Some(ms_to_iso8601(i64::try_from(NOW_MS).unwrap()))
        );
        // The UPDATE ran, tenant-scoped + only-if-unrevoked.
        let calls = f.db.calls();
        let update = calls
            .iter()
            .find(|(sql, _)| sql.contains("UPDATE pat SET revoked_at_ms"))
            .expect("UPDATE executed");
        assert!(update.0.contains("tenant_id = ?3"));
        assert!(update.0.contains("revoked_at_ms IS NULL"));
        assert_eq!(update.1[1], json!("pat_1"));
        assert_eq!(update.1[2], json!(TENANT));
    }

    #[test]
    fn keys_revoke_is_idempotent_no_second_update() {
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM pat WHERE pat_id",
                vec![row(&[
                    ("pat_id", json!("pat_1")),
                    ("name", json!("ci-key")),
                    ("scope", json!("read-only")),
                    ("created_ms", json!(1_690_000_000_000_i64)),
                    ("revoked_at_ms", json!(1_695_000_000_000_i64)),
                ])],
            )]),
            None,
        );
        let resp = f
            .handler
            .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
            .unwrap();
        // Original timestamp preserved.
        assert_eq!(resp.pat.revoked_at, Some(ms_to_iso8601(1_695_000_000_000)));
        assert!(
            !f.db
                .calls()
                .iter()
                .any(|(sql, _)| sql.contains("UPDATE pat")),
            "idempotent re-revoke must not re-UPDATE"
        );
    }

    #[test]
    fn keys_revoke_cross_tenant_is_not_found() {
        // The tenant-scoped SELECT finds nothing for another tenant's
        // PAT (the mock returns rows only when binds match TENANT).
        #[derive(Debug, Default)]
        struct TenantScopedMock {
            calls: Mutex<Vec<(String, Vec<Value>)>>,
        }
        impl CustomerD1 for TenantScopedMock {
            fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
                self.calls
                    .lock()
                    .unwrap()
                    .push((sql.to_owned(), binds.clone()));
                if sql.contains("FROM pat WHERE pat_id") && binds.get(1) == Some(&json!(TENANT)) {
                    return Ok(vec![row(&[
                        ("pat_id", json!("pat_1")),
                        ("name", json!("ci-key")),
                        ("scope", json!("read-only")),
                        ("created_ms", json!(1_690_000_000_000_i64)),
                        ("revoked_at_ms", Value::Null),
                    ])]);
                }
                Ok(Vec::new())
            }
        }
        let db = Arc::new(TenantScopedMock::default());
        let handler = D1CustomerHandler::new(
            db.clone(),
            None,
            String::new(),
            None,
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
        );
        // The OWNING tenant revokes fine…
        assert!(handler
            .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
            .is_ok());
        // …another tenant naming the same pat_id gets NotFound, and no
        // UPDATE ever ran under the foreign tenant.
        let err = handler
            .revoke(KeyRevokeRequest::new(OTHER_TENANT, "clpat_y", "pat_1", 0))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::NotFound { .. }),
            "{err:?}"
        );
        assert!(
            !db.calls
                .lock()
                .unwrap()
                .iter()
                .any(|(sql, binds)| sql.contains("UPDATE pat")
                    && binds.contains(&json!(OTHER_TENANT))),
            "cross-tenant revoke must never UPDATE"
        );
    }

    // ── Team ─────────────────────────────────────────────────────────────────

    #[test]
    fn team_list_synthesizes_single_owner_row() {
        let f = fixture_with(
            MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
            None,
        );
        let resp =
            CustomerTeamHandler::list(&f.handler, TeamListRequest::new(TENANT, "clpat_x", 0))
                .unwrap();
        assert_eq!(resp.members.len(), 1);
        assert_eq!(resp.members[0].user_id, "user_2abc");
        assert_eq!(
            resp.members[0].email, "—",
            "email is hashed in D1: honest dash"
        );
        assert_eq!(resp.members[0].role, "Owner");
        assert_eq!(resp.members[0].status, "active");
        assert_eq!(resp.members[0].joined_at, ms_to_iso8601(1_690_000_000_000));
    }

    #[test]
    fn team_invite_inserts_invited_member_row() {
        // WP-T2: invite() is now D1-PURE — it INSERTs an `invited` team_member row
        // (status `invited`, pseudonymized email_hash, CHECK-safe role) and returns
        // the new member; NO synchronous Clerk call (OB-1).
        let f = fixture_with(MockD1::with(vec![]), None);
        let resp = f
            .handler
            .invite(TeamInviteRequest::new(
                TENANT,
                "clpat_x",
                "Alice@Example.com",
                "Developer",
                0,
            ))
            .expect("invite must succeed");
        assert_eq!(resp.member.status, "invited");
        // Response echoes the caller-supplied email + requested role (InMemory shape).
        assert_eq!(resp.member.email, "Alice@Example.com");
        assert_eq!(resp.member.role, "Developer");

        // Exactly one D1 write: the team_member INSERT (status='invited').
        let calls = f.db.calls();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT INTO team_member"))
            .expect("an INSERT INTO team_member must have run");
        assert!(insert.0.contains("'invited'"), "row must be status=invited");
        // binds: tenant, user_id(placeholder UUID), email_hash, role, invited_by, invited_at_ms.
        assert_eq!(insert.1[0], json!(TENANT));
        // CTRL-PRIV-001: the raw email is NEVER a bind value — only its SHA-256 hash
        // of the NORMALIZED (trim+lowercase) email — the exact join key the
        // signup-worker accept side matches on (C-ACCEPT parity).
        let expected_hash = hex::encode(Sha256::digest(b"alice@example.com"));
        assert_eq!(insert.1[2], json!(expected_hash));
        for bind in &insert.1 {
            assert_ne!(
                bind.as_str(),
                Some("Alice@Example.com"),
                "raw invitee email must never be persisted (CTRL-PRIV-001)"
            );
        }
        // `Developer` collapses onto the CHECK domain → `member`.
        assert_eq!(insert.1[3], json!("member"));
        assert_eq!(insert.1[4], json!("clpat_x"), "invited_by = caller principal");

        // Audit: Attempted BEFORE the write, Committed AFTER.
        let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AuditEventKind::TeamInviteAttempted,
                AuditEventKind::TeamInviteCommitted
            ]
        );
    }

    #[test]
    fn normalize_invite_role_maps_to_check_domain() {
        assert_eq!(normalize_invite_role("Owner"), "owner");
        assert_eq!(normalize_invite_role("ADMIN"), "admin");
        assert_eq!(normalize_invite_role("Viewer"), "viewer");
        assert_eq!(normalize_invite_role("Developer"), "member");
        assert_eq!(normalize_invite_role("  member "), "member");
        assert_eq!(normalize_invite_role("anything-else"), "member");
    }

    #[test]
    fn team_list_includes_active_members() {
        // Owner (synthesized from tenant) + one active `team_member` seat.
        let member = row(&[
            ("user_id", json!("user_member1")),
            ("role", json!("member")),
            ("status", json!("active")),
            ("joined_at_ms", json!(1_690_000_500_000_i64)),
            ("invited_at_ms", json!(1_690_000_400_000_i64)),
        ]);
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                ("status IN ('active','invited')", vec![member]),
            ]),
            None,
        );
        let resp =
            CustomerTeamHandler::list(&f.handler, TeamListRequest::new(TENANT, "clpat_x", 0))
                .unwrap();
        assert_eq!(resp.members.len(), 2, "owner + 1 member");
        assert_eq!(resp.members[0].role, "Owner");
        assert_eq!(resp.members[1].user_id, "user_member1");
        assert_eq!(resp.members[1].status, "active");
    }

    #[test]
    fn team_remove_flips_seat_and_revokes_member_pats() {
        // Member exists (not owner) + holds 2 live PATs → removal revokes both.
        let f = fixture_with(
            MockD1::with(vec![
                (
                    "role, status FROM team_member",
                    vec![row(&[("role", json!("member")), ("status", json!("active"))])],
                ),
                (
                    "pat_id FROM pat",
                    vec![
                        row(&[("pat_id", json!("pat_a"))]),
                        row(&[("pat_id", json!("pat_b"))]),
                    ],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .remove(TeamRemoveRequest::new(TENANT, "clpat_owner", "user_member1", 0))
            .unwrap();
        assert_eq!(resp.revoked_pats, 2, "both of the member's live PATs revoked");
        assert_eq!(resp.member.status, "removed");

        // The load-bearing effects must both have been issued to D1.
        let sqls: Vec<String> = f.db.calls().into_iter().map(|(s, _)| s).collect();
        assert!(
            sqls.iter().any(|s| s.contains("UPDATE pat SET revoked_at_ms")),
            "must revoke the member's PATs"
        );
        assert!(
            sqls.iter()
                .any(|s| s.contains("UPDATE team_member SET status = 'removed'")),
            "must flip the seat to removed"
        );
        let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AuditEventKind::TeamRemoveAttempted,
                AuditEventKind::TeamRemoveCommitted
            ]
        );
    }

    #[test]
    fn team_remove_owner_is_rejected() {
        let f = fixture_with(
            MockD1::with(vec![(
                "role, status FROM team_member",
                vec![row(&[("role", json!("owner")), ("status", json!("active"))])],
            )]),
            None,
        );
        let err = f
            .handler
            .remove(TeamRemoveRequest::new(TENANT, "clpat_x", "user_owner", 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::Unauthorized(_)), "{err:?}");
        // No PAT revocation must have been attempted for an owner-removal reject.
        let sqls: Vec<String> = f.db.calls().into_iter().map(|(s, _)| s).collect();
        assert!(!sqls.iter().any(|s| s.contains("UPDATE pat")));
    }

    #[test]
    fn team_remove_absent_member_is_not_found() {
        // No canned team_member row → lookup returns empty → NotFound.
        let f = fixture_with(MockD1::with(vec![]), None);
        let err = f
            .handler
            .remove(TeamRemoveRequest::new(TENANT, "clpat_x", "user_ghost", 0))
            .unwrap_err();
        assert!(matches!(err, CustomerHandlerError::NotFound { .. }), "{err:?}");
    }

    // ── Audit query ──────────────────────────────────────────────────────────

    #[test]
    fn audit_query_returns_honest_empty_rows() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let resp = f
            .handler
            .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
            .unwrap();
        assert!(resp.rows.is_empty(), "no customer-audit table: honest []");
    }

    // ── Misc plumbing ────────────────────────────────────────────────────────

    #[test]
    fn debug_is_redacted() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let dbg = format!("{:?}", f.handler);
        assert!(dbg.contains("[CustomerD1]"), "{dbg}");
        assert!(!dbg.contains("0x42"), "no key material in Debug: {dbg}");
    }

    #[test]
    fn audit_events_use_wall_clock_not_request_timestamp() {
        let f = fixture_with(MockD1::with(vec![]), None);
        // Request carries the routes' pinned at_unix_ms = 0.
        let _ = f
            .handler
            .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0));
        let rows = f.audit.snapshot().unwrap();
        assert!(!rows.is_empty());
        assert!(
            rows.iter().all(|e| e.at_unix_ms == NOW_MS),
            "audit timestamps must come from the wall clock, never the request"
        );
    }
}
