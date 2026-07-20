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
//! | overview            | `tenant` (0023/0031/0056/0057) + `tenant_storage_state` (0008) `SUM(bytes_used)` / `MAX(bytes_quota)` + `tenant_billing` (0055) + `byok_envelope` (0030) existence; `tenant_name` = `tenant_id`; `recent_activity` = newest 8 `customer_audit_events` (0077) via [`D1CustomerHandler::recent_activity`] (BE-3), honestly empty for a new tenant |
//! | usage               | real `cas_bytes`/`quota_bytes` from `tenant_storage_state` + real `request_count` from `monthly_request_counts` (0071) via [`D1CustomerHandler::monthly_request_count`] (BE-1a); `reads`/`writes` = 0 + `daily` = `[]` (no per-op table) — only the CURRENT period retains bytes, an earlier period honestly reports 0 bytes |
//! | audit               | `customer_audit_events` (migration 0077): newest-first, tenant-scoped, bounded; rows written UNSKIPPABLE / fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER) by `keys create` (`pat.created`) + `team invite` (`team.invited`) |
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
    mint::mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_FIND,
    SCOPE_CACHE_R, SCOPE_CACHE_RW,
};
use serde_json::{json, Value};
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

    /// Build the row source over a fresh D1-over-HTTP client from `StorageEnv`.
    /// `None` when the storage env is unset/invalid (dev/CI) — mirrors
    /// `D1CustomerHandler::from_env` and `routes::dsr::build_d1_worker`. Used by
    /// `routes::customer::account_deletion_from_env` to give the self-serve
    /// account-delete requester its own D1 query seam.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env).ok()?;
        Some(Self::new(Arc::new(d1)))
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
        // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only`
        // (`pat.scope` CHECK forbids a 4th value, migration 0037) and is narrowed
        // to find-missing via the additive `find_only` marker (migration 0093) —
        // NOT a new `pat.scope` string. See `mint_is_find_only`.
        Ok(crate::scope::RequestedScopeClass::FindMissing)
        | Ok(crate::scope::RequestedScopeClass::ReadOnly) => Ok("read-only"),
        Ok(crate::scope::RequestedScopeClass::ReadWrite) => Ok("read-write"),
        Ok(crate::scope::RequestedScopeClass::Admin) => Err(CustomerHandlerError::Unauthorized(
            "the admin scope is not grantable via self-serve key creation".to_owned(),
        )),
        Err(token) => Err(CustomerHandlerError::Unauthorized(format!(
            "unrecognized scope token {token:?}; valid self-serve scopes: cache:read, cache:write, cache:find-missing"
        ))),
    }
}

/// True when the requested scopes classify as FIND-ONLY (the true least-privilege
/// scope, ADR-0071) — a `find-missing` grant with NO read/write. The mint stores
/// `pat.scope = 'read-only'` (CHECK-safe) PLUS the `find_only = 1` marker; the
/// Worker then narrows the forwarded `x-corelink-scope` to `find-missing`. Any
/// unrecognized/admin token classifies elsewhere and is rejected by
/// [`map_requested_scopes`], so this is only ever consulted after that succeeds.
fn mint_is_find_only(requested: &[String]) -> bool {
    matches!(
        crate::scope::classify_requested_scopes(requested),
        Ok(crate::scope::RequestedScopeClass::FindMissing)
    )
}

/// D1 `pat.scope` string → dashboard scopes list (inverse of
/// [`map_requested_scopes`] for the canonical values; unknown legacy
/// values are surfaced verbatim rather than guessed at).
fn scope_to_list(scope: &str, find_only: bool) -> Vec<String> {
    // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only` + the
    // `find_only` marker, so surface it as the find-missing capability it
    // actually grants (NOT `cache:read`, which it cannot do).
    if find_only {
        return vec!["cache:find-missing".to_owned()];
    }
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
        // `owner` is NEVER mintable via a self-serve invite (there is exactly one
        // owner — the tenant creator). The route handler rejects an owner invite
        // outright (`handle_team_invite`); this maps it to `admin` as
        // defense-in-depth so NO code path can ever persist a second owner seat
        // (the member→owner escalation closed at the persistence layer too).
        "owner" | "admin" => "admin",
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

/// Civil (y, m, d) → days-since-epoch. Howard Hinnant's `days_from_civil`
/// algorithm (public domain), the exact inverse of [`civil_from_days`]. Used to
/// turn an ISO-8601 `since` filter back into the integer `ts_ms` domain.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let m = i64::from(m);
    let d = i64::from(d);
    let y = if m <= 2 { y - 1 } else { y };
    // `div_euclid` floors (matching `civil_from_days`), so no truncating-
    // division `y - 399` adjustment is needed — that would double-correct.
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
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

/// ISO-8601 / RFC3339 UTC timestamp → unix-ms. The inverse of
/// [`ms_to_iso8601`], used to honor the `GET /v1/customer/audit?from=` filter
/// (`AuditQueryRequest::since`), whose contract type is an ISO-8601 string while
/// the `customer_audit_events.ts_ms` column is integer millis.
///
/// Accepts the canonical `ms_to_iso8601` output (`YYYY-MM-DDTHH:MM:SSZ`) plus
/// common variants: a bare date (`YYYY-MM-DD`), a space date/time separator, an
/// optional fractional-second part, and an optional trailing `Z`. Returns
/// `None` for anything it cannot parse — the caller then applies NO `since`
/// filter (lenient: a malformed param never silently drops the customer's rows,
/// nor errors their whole activity read). UTC-only, mirroring `ms_to_iso8601`.
#[must_use]
fn iso8601_to_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    // Split date from the optional time component on 'T' or ' '.
    let (date, time) = s
        .split_once(['T', ' '])
        .map_or((s, None), |(d, t)| (d, Some(t)));
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (mut hh, mut mi, mut ss): (i64, i64, i64) = (0, 0, 0);
    if let Some(t) = time {
        // Strip a trailing 'Z' and any fractional-second suffix.
        let t = t.trim_end_matches('Z');
        let t = t.split_once('.').map_or(t, |(whole, _)| whole);
        let mut tp = t.split(':');
        hh = tp.next()?.parse().ok()?;
        mi = tp.next().map_or(Ok(0), str::parse).ok()?;
        ss = tp.next().map_or(Ok(0), str::parse).ok()?;
        if !(0..=23).contains(&hh) || !(0..=59).contains(&mi) || !(0..=60).contains(&ss) {
            return None;
        }
    }
    Some((days_from_civil(y, m, d) * 86_400 + hh * 3_600 + mi * 60 + ss) * 1_000)
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

/// Extract a monotonic counter column as `u64` (`0` for NULL / absent /
/// negative — the `usage_daily` CHECK constraints keep these non-negative).
fn col_u64(row: &D1Row, key: &str) -> u64 {
    col_opt_i64(row, key)
        .and_then(|c| u64::try_from(c).ok())
        .unwrap_or(0)
}

// ─── D1CustomerHandler ────────────────────────────────────────────────────────

/// Self-serve PAT TTL: 90 days (the canonical rotation cadence from
/// migration 0037's `pat.expires_ms` contract).
const SELF_SERVE_PAT_TTL: Duration = Duration::from_secs(90 * 86_400);

/// Max customer-facing audit rows returned by `GET /v1/customer/audit`
/// (newest-first). Bounds the row source (migration 0077) so a long-lived
/// tenant's activity log can never return an unbounded page.
const AUDIT_QUERY_LIMIT: i64 = 100;

/// Estimated wall-clock seconds a single cache hit saves — a hit avoids
/// re-executing ~one build action. Feeds the DISPLAYED build-time-saved
/// estimate on the customer usage surface; NEVER used to bill or gate.
const SECONDS_SAVED_PER_HIT: f64 = 15.0;

/// Estimated USD per compute-second (~$0.04/vCPU-hour) — deliberately
/// conservative. Feeds the DISPLAYED $-saved estimate; NEVER used to bill.
const USD_PER_COMPUTE_SECOND: f64 = 0.000_011_1;

/// Aggregate of one period's `usage_daily` rows (migration 0089): the summed
/// counters plus the per-day `{day, reads, writes}` buckets. DISPLAY telemetry
/// only.
#[derive(Debug, Default)]
struct UsageRollup {
    /// Summed cache reads across the period.
    reads: u64,
    /// Summed cache writes across the period.
    writes: u64,
    /// Summed cache read HITS across the period.
    hits: u64,
    /// Summed cache read MISSES across the period.
    misses: u64,
    /// Per-day buckets, oldest-first (`cas_bytes` always 0 — no per-day byte
    /// history exists).
    daily: Vec<DailyUsageBucket>,
}

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
            .unwrap_or_else(|| "https://humangr.com/corelink/en/customer/billing".to_owned());

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

    /// Write one customer-facing audit row (migration 0077) — the WRITE half of
    /// `GET /v1/customer/audit`. **Fail-CLOSED / unskippable** (INV-AUDIT-EMIT-
    /// ATOMIC-WITH-HANDLER): a failed insert propagates as
    /// [`CustomerHandlerError::AuditFailed`] (→ 503 "audit closed") so the
    /// customer-visible control-plane audit trail can NEVER be silently skipped.
    ///
    /// Callers MUST invoke this BEFORE the primary state mutation (emit-before-
    /// mutate, the canonical audit-fail-CLOSED ordering the DSR endpoint + the
    /// audit-chain sink use): if the audit row cannot be persisted the mutation
    /// never happens, so a committed op is never left without its audit row (and
    /// the non-idempotent mints/invites are never double-applied by a client that
    /// retries a committed-then-500 response). The row is wall-clock timestamped,
    /// tenant-scoped + fully parameterised (INV-TENANT-ISOLATION). `target` /
    /// `detail` MUST be PII-free (e.g. a PAT id / invitation id + role, never a
    /// raw email — CTRL-PRIV-001).
    ///
    /// # Errors
    ///
    /// [`CustomerHandlerError::AuditFailed`] on any D1 transport/decode failure
    /// (the caller MUST propagate it — never `.ok()`/`let _ =`).
    fn insert_audit_event(
        &self,
        tenant_id: &str,
        event_type: &str,
        actor: &str,
        target: &str,
        detail: &str,
    ) -> Result<(), CustomerHandlerError> {
        let ts_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        self.db
            .query(
                "INSERT INTO customer_audit_events \
                 (tenant_id, event_type, actor, target, ts_ms, detail) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                vec![
                    json!(tenant_id),
                    json!(event_type),
                    json!(actor),
                    json!(target),
                    json!(ts_ms),
                    json!(detail),
                ],
            )
            .map(|_rows| ())
            .map_err(|e| {
                // Fail-CLOSED: mark the op errored + surface AuditFailed so the
                // caller aborts BEFORE the primary mutation. Never a silent drop.
                self.emit_sli(true);
                tracing::error!(error = %e, event_type, "customer_d1: unskippable audit insert failed (fail-CLOSED)");
                CustomerHandlerError::AuditFailed(format!(
                    "customer audit row insert failed for {event_type}: {e}"
                ))
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

    /// Newest-first customer activity for the overview snapshot (dashboard-revival
    /// BE-3). Reads the same `customer_audit_events` (0077) surface the audit
    /// endpoint serves — written UNSKIPPABLE / fail-CLOSED by the control-plane
    /// mutations (`keys create` → `pat.created`, `team invite` → `team.invited`) — bounded
    /// to `limit`, tenant-scoped, newest-first. Fail-CLOSED on transport (never a
    /// fabricated row; an honestly-empty feed stays empty).
    fn recent_activity(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomerAuditEventRow>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT id, event_type, actor, target, ts_ms, detail \
             FROM customer_audit_events WHERE tenant_id = ?1 \
             ORDER BY ts_ms DESC LIMIT ?2",
            vec![json!(tenant_id), json!(limit)],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                CustomerAuditEventRow::new(
                    col_opt_i64(row, "id")
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    col_opt_i64(row, "ts_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    col_opt_str(row, "event_type").unwrap_or_default(),
                    // No per-event severity column — informational activity only.
                    "info",
                    col_opt_str(row, "actor").unwrap_or_default(),
                    col_opt_str(row, "detail").unwrap_or_default(),
                )
            })
            .collect())
    }

    /// The billable request count for `(tenant, year_month)` from
    /// `monthly_request_counts` (migration 0071) — the running counter the quota
    /// gate already increments per request. READ-ONLY (never writes the hot-path
    /// counter); `0` when no row exists (a period with no requests yet). The
    /// `year_month` key format (`YYYY-MM`, UTC) matches [`period_from_ms`] and
    /// the writer's `request_count::year_month_utc`, so a period lookup aligns.
    fn monthly_request_count(
        &self,
        tenant_id: &str,
        year_month: &str,
    ) -> Result<u64, CustomerHandlerError> {
        let rows = self.run(
            "SELECT request_count FROM monthly_request_counts \
             WHERE tenant_id = ?1 AND year_month = ?2 LIMIT 1",
            vec![json!(tenant_id), json!(year_month)],
        )?;
        Ok(rows
            .first()
            .and_then(|r| col_opt_i64(r, "request_count"))
            .and_then(|c| u64::try_from(c).ok())
            .unwrap_or(0))
    }

    /// Per-period usage rollup for `(tenant, year_month)` from `usage_daily`
    /// (migration 0089) — the DISPLAY telemetry feeding the dashboard ROI
    /// surface (BE-1 reads/writes/daily + BE-2 hit-rate / $-saved). A READ-ONLY
    /// period scan (`day LIKE 'YYYY-MM-%'`, `ORDER BY day`); it NEVER writes the
    /// hot-path counters. Fail-CLOSED on transport like
    /// [`Self::monthly_request_count`]: a transport/decode error propagates
    /// (→ 500) rather than degrading to a fabricated empty rollup. An honestly
    /// empty result set (no rows for the period) is a zeroed rollup with an
    /// empty daily series. `cas_bytes` is 0 in every bucket — no per-day byte
    /// history exists.
    fn usage_daily_rollup(
        &self,
        tenant_id: &str,
        year_month: &str,
    ) -> Result<UsageRollup, CustomerHandlerError> {
        let rows = self.run(
            "SELECT day, reads, writes, hits, misses FROM usage_daily \
             WHERE tenant_id = ?1 AND day LIKE ?2 ORDER BY day",
            vec![json!(tenant_id), json!(format!("{year_month}-%"))],
        )?;
        let mut roll = UsageRollup::default();
        for row in &rows {
            let reads = col_u64(row, "reads");
            let writes = col_u64(row, "writes");
            roll.reads = roll.reads.saturating_add(reads);
            roll.writes = roll.writes.saturating_add(writes);
            roll.hits = roll.hits.saturating_add(col_u64(row, "hits"));
            roll.misses = roll.misses.saturating_add(col_u64(row, "misses"));
            let day = col_opt_str(row, "day").unwrap_or_default();
            // cas_bytes = 0: no per-day byte history exists (0089 has no byte col).
            roll.daily
                .push(DailyUsageBucket::new(day, reads, writes, 0));
        }
        Ok(roll)
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
        // BE-3: the newest few control-plane events for the snapshot feed
        // (honestly empty for a brand-new tenant — never a fabricated row).
        let recent = self.recent_activity(&req.caller_tenant, 8)?;

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
            recent,
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

        // Billable request count for the period (monthly_request_counts, 0071):
        // a real usage-vs-quota signal. The quota gate already increments this
        // per request, so surfacing it is a READ — no hot-path write added.
        let request_count = self.monthly_request_count(&req.caller_tenant, &requested)?;

        // BE-1 + BE-2: real reads/writes/daily + cache hit-rate + estimated
        // build-time / $ saved from usage_daily (0089). DISPLAY telemetry — a
        // READ-ONLY period scan, never a hot-path write, fail-CLOSED on transport.
        let rollup = self.usage_daily_rollup(&req.caller_tenant, &requested)?;

        // hit_rate: fraction 0.0..=1.0; None when there were no cache reads at
        // all (hits + misses == 0) — an honest "no data", NEVER a fabricated rate.
        let cache_reads = rollup.hits.saturating_add(rollup.misses);
        let hit_rate = if cache_reads == 0 {
            None
        } else {
            Some(rollup.hits as f64 / cache_reads as f64)
        };

        // Estimated build-time / compute-cost saved (DISPLAYED AS AN ESTIMATE):
        //   time_saved_seconds   = hits * SECONDS_SAVED_PER_HIT
        //   dollars_saved_cents  = round(time_saved_seconds * USD_PER_COMPUTE_SECOND * 100)
        let seconds_saved = rollup.hits as f64 * SECONDS_SAVED_PER_HIT;
        let time_saved_seconds = seconds_saved as u64;
        let dollars_saved_cents = (seconds_saved * USD_PER_COMPUTE_SECOND * 100.0).round() as u64;

        let resp = UsageResponse::new(
            requested,
            period_bytes,
            rollup.reads,
            rollup.writes,
            quota_bytes,
            rollup.daily,
            request_count,
            hit_rate,
            time_saved_seconds,
            dollars_saved_cents,
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
            "SELECT pat_id, name, scope, created_ms, revoked_at_ms, find_only \
             FROM pat WHERE tenant_id = ?1 ORDER BY created_ms DESC",
            vec![json!(req.caller_tenant)],
        )?;
        let pats: Vec<PatRow> = rows
            .iter()
            .map(|row| {
                // `find_only` (0093): NULL/0 = normal PAT; 1 = find-missing only.
                let find_only = col_opt_i64(row, "find_only").unwrap_or(0) == 1;
                PatRow::new(
                    col_opt_str(row, "pat_id").unwrap_or_default(),
                    // Pre-0063 rows have no name: honest empty string.
                    col_opt_str(row, "name").unwrap_or_default(),
                    scope_to_list(&col_opt_str(row, "scope").unwrap_or_default(), find_only),
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

        // FROZEN scope map ('admin' NEVER grantable). `scope` is the CHECK-safe
        // D1 value ('read-only'/'read-write'); a FIND-ONLY request maps to
        // 'read-only' + the `find_only` marker (ADR-0071, migration 0093).
        let scope = map_requested_scopes(&req.scopes).inspect_err(|_| self.emit_sli(true))?;
        let find_only = mint_is_find_only(&req.scopes);
        // Bitset mirrors the effective grant (ADR-0071). Read is a superset of
        // find-missing, so read/read-write also carry the FIND bit; a find-only
        // token carries ONLY `SCOPE_CACHE_FIND` (no read/write). (The authoritative
        // enforcement input is the scope the Worker forwards as `x-corelink-scope`
        // — `find-missing` for a find-only PAT; these bits mirror it.)
        let scope_bits = if find_only {
            PatScopes::from_u64(SCOPE_CACHE_FIND)
        } else if scope == "read-write" {
            PatScopes::from_u64(SCOPE_CACHE_RW | SCOPE_CACHE_FIND)
        } else {
            PatScopes::from_u64(SCOPE_CACHE_R | SCOPE_CACHE_FIND)
        };
        // Human-readable scope for the audit summary (the stored `scope` is the
        // CHECK-safe base, so a find-only PAT would otherwise read "read-only").
        let scope_label = if find_only { "find-missing" } else { scope };

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

        // Customer-facing audit row (migration 0077, write half). UNSKIPPABLE /
        // fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER): emitted BEFORE the pat
        // INSERT so a self-serve key mint can NEVER commit without its customer-
        // visible audit row. `target` is the PAT id (no PII, minted above); the
        // summary names the key + granted scope. A failed insert → 503 and the
        // key is never created (the non-idempotent mint is not left half-applied).
        self.insert_audit_event(
            &req.caller_tenant,
            "pat.created",
            &req.principal,
            &pat.id.to_string(),
            &format!("Created API key {:?} ({scope_label})", req.name),
        )?;

        // Durable INSERT. `shown_once_token` (NOT NULL UNIQUE, 0037) is
        // a fresh UUID immediately marked consumed: the dashboard
        // returns the plaintext in THIS response (shown once) and the
        // reveal-endpoint path is never used for self-serve keys.
        self.run(
            "INSERT INTO pat \
             (pat_id, tenant_id, pat_hash, scope, expires_ms, \
              shown_once_token, shown_once_consumed, created_ms, token_id, name, find_only) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10)",
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
                json!(i32::from(find_only)),
            ],
        )?;

        let row = PatRow::new(
            pat.id.to_string(),
            req.name.clone(),
            scope_to_list(scope, find_only),
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
            "SELECT pat_id, name, scope, created_ms, revoked_at_ms, find_only \
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
            scope_to_list(
                &col_opt_str(&row, "scope").unwrap_or_default(),
                col_opt_i64(&row, "find_only").unwrap_or(0) == 1,
            ),
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
        // ONE canonical scheme (CTRL-PRIV-001): HMAC-SHA256 under `EMAIL_HASH_SALT`
        // when set, else unsalted SHA-256 (pre-salt parity). The DSR rectification
        // and the signup-worker accept-match MUST use the SAME helper, or the join
        // key diverges. Normalization (trim+lowercase) lives inside the helper.
        let email_hash = crate::email_hash::hash_email(&req.email);
        // The role is collapsed onto the FROZEN 0074 CHECK domain (CHECK-safe).
        let role = normalize_invite_role(&req.role);
        // No real Clerk user_id exists yet (OB-1) — `team_member.user_id` is NOT
        // NULL (PK), so a fresh UUID is the invitation-id placeholder 0074 expects
        // ("carries the Clerk invitation id until acceptance binds the real user").
        let invitation_id = Uuid::now_v7().to_string();
        let invited_at_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);

        // Customer-facing audit row (migration 0077, write half). UNSKIPPABLE /
        // fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER): emitted BEFORE the
        // team_member INSERT so an invite can NEVER commit without its customer-
        // visible audit row. PII-safe — `target` is the invitation id and the
        // summary names only the role, NEVER the raw invitee email (CTRL-PRIV-001;
        // only `email_hash` is persisted below). A failed insert → 503 and no seat
        // is written.
        self.insert_audit_event(
            &req.caller_tenant,
            "team.invited",
            &req.principal,
            &invitation_id,
            &format!("Invited a team member with role {role}"),
        )?;

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
            if role.is_empty() {
                "member".to_owned()
            } else {
                role
            },
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

        // Real customer-facing activity log (migration 0077): newest-first,
        // tenant-scoped, bounded. Written UNSKIPPABLE / fail-CLOSED by the
        // control-plane mutations (`create` / `invite`). Fail-CLOSED on transport error
        // (`self.run`), never degraded to fabricated empty data.
        //
        // Contract: honor the `?from=`/`?kind=` filters (`req.since` /
        // `req.event_types`). The WHERE clause is BUILT with generated
        // positional placeholders (`?N`) and every value is BOUND through
        // `self.run` — no value is ever string-interpolated, so the dynamic
        // shape carries NO injection surface. An absent filter is omitted
        // (behaves as before — no narrowing). Tenant scope stays fail-CLOSED
        // (always `WHERE tenant_id = ?1`).
        let mut sql = String::from(
            "SELECT id, event_type, actor, target, ts_ms, detail \
             FROM customer_audit_events WHERE tenant_id = ?1",
        );
        let mut binds: Vec<Value> = vec![json!(req.caller_tenant)];

        // `?from=` → `AND ts_ms >= ?` (ISO-8601 parsed to the integer ts_ms
        // domain). A present-but-unparseable `since` applies NO filter
        // (lenient — never silently drops the customer's rows on a bad param).
        if let Some(since_ms) = req.since.as_deref().and_then(iso8601_to_ms) {
            binds.push(json!(since_ms));
            sql.push_str(&format!(" AND ts_ms >= ?{}", binds.len()));
        }

        // `?kind=` → `AND event_type IN (?, ?, …)`. Placeholders are GENERATED
        // (positional `?N`); each event-type value is BOUND, never interpolated.
        if !req.event_types.is_empty() {
            let first = binds.len() + 1;
            let placeholders: Vec<String> = (first..first + req.event_types.len())
                .map(|n| format!("?{n}"))
                .collect();
            for et in &req.event_types {
                binds.push(json!(et));
            }
            sql.push_str(&format!(" AND event_type IN ({})", placeholders.join(", ")));
        }

        // Newest-first, bounded (preserved).
        binds.push(json!(AUDIT_QUERY_LIMIT));
        sql.push_str(&format!(" ORDER BY ts_ms DESC LIMIT ?{}", binds.len()));

        let rows = self.run(&sql, binds)?;
        let event_rows: Vec<CustomerAuditEventRow> = rows
            .iter()
            .map(|row| {
                CustomerAuditEventRow::new(
                    col_opt_i64(row, "id")
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    col_opt_i64(row, "ts_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    col_opt_str(row, "event_type").unwrap_or_default(),
                    // No per-event severity column; the customer-facing surface
                    // carries only informational activity rows.
                    "info",
                    col_opt_str(row, "actor").unwrap_or_default(),
                    col_opt_str(row, "detail").unwrap_or_default(),
                )
            })
            .collect();
        let resp = AuditQueryResponse::new(event_rows);

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

// ─── BYOK config read model (Wave 2 — read seam only, NOT yet wired) ──────────
//
// The per-tenant key-custody + crypto-mode read model over migration 0081
// (`tenant_byok_config`). This is a READ seam landed for Wave 3 (data-plane
// wiring) to consume — NO handler / hot-path calls it yet, and nothing here
// writes the config tables (the signup-worker / control-plane is the sole
// writer, a later wave — audit finding H5). Placed AFTER the handler impls so
// it shifts no OKF-cited line range in this file.
//
// FAIL-CLOSED invariant: an unknown / unparseable mode, crypto_mode, or state
// is an ERROR, never a permissive default. An unrecognised custody rung must
// NEVER silently degrade to "encryption off".

/// Failure reading or parsing the BYOK config read model. Fail-CLOSED:
/// callers MUST treat any variant as "custody undetermined" and refuse to
/// downgrade to plaintext — never coerce an error into "encryption off".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByokConfigError {
    /// D1 transport / decode failure (the underlying row source errored).
    Transport(String),
    /// A column held an unknown / unparseable enum value, or a `NOT NULL`
    /// column was absent from the row.
    Parse(String),
}

impl core::fmt::Display for ByokConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "byok config transport error: {e}"),
            Self::Parse(e) => write!(f, "byok config parse error: {e}"),
        }
    }
}

impl std::error::Error for ByokConfigError {}

/// Key-custody rung of a tenant's BYOK configuration
/// (`tenant_byok_config.mode`, plan §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByokMode {
    /// CoreLink-held key (no customer KMS).
    Managed,
    /// Customer CMK wraps the per-tenant Tenant Convergence Secret (Tcs).
    Byok,
    /// Hold-your-own-key — strongest custody.
    Hyok,
}

impl ByokMode {
    /// Canonical D1 string (matches the `mode` CHECK constraint).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Byok => "byok",
            Self::Hyok => "hyok",
        }
    }
}

impl core::str::FromStr for ByokMode {
    type Err = ByokConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "managed" => Ok(Self::Managed),
            "byok" => Ok(Self::Byok),
            "hyok" => Ok(Self::Hyok),
            other => Err(ByokConfigError::Parse(format!(
                "unknown byok mode: {other:?}"
            ))),
        }
    }
}

/// Crypto mode — Mode A vs Mode B (`tenant_byok_config.crypto_mode`, plan §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByokCryptoMode {
    /// Mode A — convergent encryption keyed by the Tcs; preserves cross-blob
    /// dedup.
    Convergent,
    /// Mode B — per-write random keys; maximises isolation, no dedup.
    Random,
}

impl ByokCryptoMode {
    /// Canonical D1 string (matches the `crypto_mode` CHECK constraint).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Convergent => "convergent",
            Self::Random => "random",
        }
    }
}

impl core::str::FromStr for ByokCryptoMode {
    type Err = ByokConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "convergent" => Ok(Self::Convergent),
            "random" => Ok(Self::Random),
            other => Err(ByokConfigError::Parse(format!(
                "unknown byok crypto_mode: {other:?}"
            ))),
        }
    }
}

/// Onboarding state machine (`tenant_byok_config.state`). Monotonic +
/// audited; written only by the control-plane authority (signup-worker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByokState {
    /// Not configured (no encryption).
    Inactive,
    /// Onboarding in progress (key referenced, not yet active).
    Pending,
    /// Fully active — all writes encrypted.
    Active,
    /// Active for NEW writes while a backfill re-encrypts historical blobs.
    Partial,
    /// Crypto-shredded — CMK/Tcs destroyed; ciphertext unrecoverable.
    Shredded,
}

impl ByokState {
    /// Canonical D1 string (matches the `state` CHECK constraint).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Partial => "partial",
            Self::Shredded => "shredded",
        }
    }
}

impl core::str::FromStr for ByokState {
    type Err = ByokConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inactive" => Ok(Self::Inactive),
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "partial" => Ok(Self::Partial),
            "shredded" => Ok(Self::Shredded),
            other => Err(ByokConfigError::Parse(format!(
                "unknown byok state: {other:?}"
            ))),
        }
    }
}

/// The per-tenant BYOK configuration read model (one `tenant_byok_config`
/// row, migration 0081). `cmk_*` are `None` until a BYOK/HYOK tenant is
/// onboarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantByokConfig {
    /// Tenant id (PK).
    pub tenant_id: String,
    /// Key-custody rung.
    pub mode: ByokMode,
    /// Convergent (dedup-preserving) vs random (max-isolation).
    pub crypto_mode: ByokCryptoMode,
    /// CMK provider (`aws` / `gcp` / `azure` / `vault` / `corelink_managed`).
    pub cmk_provider: Option<String>,
    /// CMK identity: ARN / GCP resource name / Azure URI / Vault path.
    pub cmk_key_id: Option<String>,
    /// CMK region.
    pub cmk_region: Option<String>,
    /// Onboarding state.
    pub state: ByokState,
}

/// `true` when the tenant's at-rest encryption is LIVE. `Partial` counts as
/// active because it still encrypts NEW writes while a backfill re-encrypts
/// historical blobs (only `Inactive`/`Pending`/`Shredded` are not "encrypt
/// new writes").
#[must_use]
pub fn is_encryption_active(cfg: &TenantByokConfig) -> bool {
    matches!(cfg.state, ByokState::Active | ByokState::Partial)
}

/// Parse one `tenant_byok_config` D1 row into the read model. Fail-CLOSED: a
/// missing `NOT NULL` column or an unparseable enum is a
/// [`ByokConfigError::Parse`], never a permissive default.
fn parse_byok_config_row(row: &D1Row) -> Result<TenantByokConfig, ByokConfigError> {
    let tenant_id = col_opt_str(row, "tenant_id")
        .ok_or_else(|| ByokConfigError::Parse("tenant_byok_config.tenant_id missing".to_owned()))?;
    let mode = col_opt_str(row, "mode")
        .ok_or_else(|| ByokConfigError::Parse("tenant_byok_config.mode missing".to_owned()))?
        .parse::<ByokMode>()?;
    let crypto_mode = col_opt_str(row, "crypto_mode")
        .ok_or_else(|| ByokConfigError::Parse("tenant_byok_config.crypto_mode missing".to_owned()))?
        .parse::<ByokCryptoMode>()?;
    let state = col_opt_str(row, "state")
        .ok_or_else(|| ByokConfigError::Parse("tenant_byok_config.state missing".to_owned()))?
        .parse::<ByokState>()?;
    Ok(TenantByokConfig {
        tenant_id,
        mode,
        crypto_mode,
        cmk_provider: col_opt_str(row, "cmk_provider"),
        cmk_key_id: col_opt_str(row, "cmk_key_id"),
        cmk_region: col_opt_str(row, "cmk_region"),
        state,
    })
}

/// Async D1 row-source seam for the BYOK config read model. The production
/// impl is [`D1HttpClient`]; tests supply a hermetic mock. Generic (not
/// `dyn`) so the native `async fn` needs no boxing on the data-plane read.
pub trait ByokConfigRows: Send + Sync {
    /// Run one parameterised statement; return the result rows.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport / HTTP / decode failure.
    fn query_rows(
        &self,
        sql: &str,
        binds: Vec<Value>,
    ) -> impl core::future::Future<Output = Result<Vec<D1Row>, String>> + Send;
}

impl ByokConfigRows for D1HttpClient {
    async fn query_rows(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        self.query(sql, &binds).await
    }
}

/// Read-only loader for the per-tenant BYOK configuration (migration 0081).
///
/// **Wave 2 read seam** — landed for Wave 3 (data-plane wiring) to consume;
/// NO handler / hot-path calls it yet. Read-only by construction: it never
/// writes the config tables (the control-plane authority is the sole writer).
#[derive(Debug)]
pub struct D1ByokConfigReader<R = D1HttpClient> {
    /// Async row source (production: [`D1HttpClient`]).
    rows: Arc<R>,
}

impl<R: ByokConfigRows> D1ByokConfigReader<R> {
    /// Wire the reader over an async row source.
    #[must_use]
    pub fn new(rows: Arc<R>) -> Self {
        Self { rows }
    }

    /// Load the tenant's BYOK config.
    ///
    /// `Ok(None)` when no row exists — BYOK is NOT configured, i.e. today's
    /// (unencrypted) behaviour. Fully parameterised + tenant-scoped
    /// (INV-TENANT-ISOLATION).
    ///
    /// # Errors
    ///
    /// Fail-CLOSED: a transport failure is [`ByokConfigError::Transport`] and
    /// an unparseable enum / missing `NOT NULL` column is
    /// [`ByokConfigError::Parse`] — NEVER a silent "encryption off".
    pub async fn get_byok_config(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
        let rows = self
            .rows
            .query_rows(
                "SELECT tenant_id, mode, crypto_mode, cmk_provider, cmk_key_id, \
                        cmk_region, state \
                 FROM tenant_byok_config WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant_id)],
            )
            .await
            .map_err(ByokConfigError::Transport)?;
        match rows.first() {
            None => Ok(None),
            Some(row) => parse_byok_config_row(row).map(Some),
        }
    }
}

// ─── BYOK config WRITER (activation seam — control-plane authority) ───────────
//
// The activation WRITE path over migration 0081, closing the H5 gap: the read
// model above + the r2_s3 CAS engagement gate are inert until SOMETHING flips a
// tenant's `tenant_byok_config.state` to `active` AND persists its CMK-wrapped
// Tcs in `tenant_byok_secret`. This writer is that authority. It is operator-
// gated at the route boundary (`routes::byok_admin`, mirrors `/v1/admin/*`);
// nothing tenant-reachable calls it.
//
// SECURITY / fail-CLOSED discipline:
//   * The plaintext Tcs is NEVER handled here — only the CMK-WRAPPED ciphertext
//     (`tcs_wrapped`), mirroring 0081's INV-BYOK-CRYPTO-SOVEREIGNTY note. No
//     key material is ever logged (the audit event records tenant + provider +
//     CMK *identity* + state, never secret bytes).
//   * Parameterised SQL only; every statement is tenant-scoped by PK
//     (INV-TENANT-ISOLATION).
//   * Write ORDER is Tcs-secret FIRST, then flip config→active — so the read
//     path never observes `state=='active'` pointing at a missing Tcs (the
//     r2_s3 resolve fails CLOSED on that combination; the ordering keeps the
//     activation atomic-enough that a crash between the two writes leaves the
//     tenant NON-active, i.e. plaintext, never a half-active fail-closed brick).
//   * Invalid activation parameters are rejected BEFORE any D1 write; the
//     monotonic state machine refuses to re-activate a crypto-shredded tenant.

/// CMK providers accepted for a BYOK activation. A subset of migration 0081's
/// `cmk_provider` CHECK — `corelink_managed` is excluded because activation
/// implies a customer-held CMK (the whole point of BYOK).
const BYOK_ACTIVATION_PROVIDERS: [&str; 4] = ["aws", "gcp", "azure", "vault"];

/// A failure writing the BYOK activation / deactivation state. Fail-CLOSED:
/// the caller MUST surface any variant as an error — a partial or unvalidated
/// custody record is never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByokWriteError {
    /// Caller-supplied activation parameters are invalid (rejected BEFORE any
    /// D1 write — never persist a half-valid custody record).
    Invalid(String),
    /// The requested transition is not permitted by the monotonic state
    /// machine (e.g. re-activating a crypto-shredded tenant, or deactivating a
    /// tenant that was never active).
    IllegalTransition {
        /// Current persisted state.
        from: ByokState,
        /// Requested target state.
        to: ByokState,
    },
    /// D1 transport / write failure.
    Transport(String),
}

impl core::fmt::Display for ByokWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "byok activation invalid: {e}"),
            Self::IllegalTransition { from, to } => write!(
                f,
                "byok illegal state transition: {} → {}",
                from.as_str(),
                to.as_str()
            ),
            Self::Transport(e) => write!(f, "byok write transport error: {e}"),
        }
    }
}

impl std::error::Error for ByokWriteError {}

/// The activation parameters that flip a tenant to BYOK `active` with its CMK
/// identity + CMK-wrapped Tenant Convergence Secret.
#[derive(Debug, Clone)]
pub struct ByokActivation {
    /// Tenant id (PK of both `tenant_byok_config` and `tenant_byok_secret`).
    pub tenant_id: String,
    /// Key-custody rung — MUST be `byok` or `hyok` (not `managed`).
    pub mode: ByokMode,
    /// Convergent (Mode A) vs random (Mode B).
    pub crypto_mode: ByokCryptoMode,
    /// CMK provider — one of [`BYOK_ACTIVATION_PROVIDERS`].
    pub cmk_provider: String,
    /// CMK identity: ARN / GCP resource name / Azure URI / Vault path. Public
    /// key IDENTITY, not secret material.
    pub cmk_key_id: String,
    /// CMK region (bound for the latency SLO; optional).
    pub cmk_region: Option<String>,
    /// CMK-WRAPPED Tenant Convergence Secret ciphertext. The plaintext Tcs is
    /// NEVER carried here — only the provider-opaque wrapped bytes.
    pub tcs_wrapped: Vec<u8>,
}

impl ByokActivation {
    /// Validate fail-CLOSED. Rejects `managed` custody, unknown providers,
    /// empty CMK identity, and an empty wrapped-Tcs BEFORE any D1 write.
    fn validate(&self) -> Result<(), ByokWriteError> {
        if self.tenant_id.trim().is_empty() {
            return Err(ByokWriteError::Invalid("tenant_id is empty".to_owned()));
        }
        if !matches!(self.mode, ByokMode::Byok | ByokMode::Hyok) {
            return Err(ByokWriteError::Invalid(format!(
                "mode must be byok or hyok for an activation, got {}",
                self.mode.as_str()
            )));
        }
        if !BYOK_ACTIVATION_PROVIDERS.contains(&self.cmk_provider.as_str()) {
            return Err(ByokWriteError::Invalid(format!(
                "cmk_provider must be one of {BYOK_ACTIVATION_PROVIDERS:?}, got {:?}",
                self.cmk_provider
            )));
        }
        if self.cmk_key_id.trim().is_empty() {
            return Err(ByokWriteError::Invalid("cmk_key_id is empty".to_owned()));
        }
        if self.tcs_wrapped.is_empty() {
            return Err(ByokWriteError::Invalid(
                "tcs_wrapped is empty (refusing to activate without a wrapped Tcs)".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Control-plane WRITER for the per-tenant BYOK configuration (migration 0081).
///
/// The counterpart of [`D1ByokConfigReader`] — the sole authority that flips a
/// tenant to BYOK `active` (engaging the r2_s3 encryption gate) and the
/// crypto-shred kill switch that flips it back off. Generic over the same
/// [`ByokConfigRows`] async seam so unit tests drive it hermetically.
#[derive(Debug)]
pub struct D1ByokConfigWriter<R = D1HttpClient> {
    /// Async row source (production: [`D1HttpClient`]). `query_rows` carries
    /// both reads and writes (a write returns an empty result set).
    rows: Arc<R>,
}

impl<R: ByokConfigRows> D1ByokConfigWriter<R> {
    /// Wire the writer over an async row source.
    #[must_use]
    pub fn new(rows: Arc<R>) -> Self {
        Self { rows }
    }

    /// Read the tenant's current `state` (fail-CLOSED on an unparseable value).
    /// `Ok(None)` ⇒ no config row yet.
    async fn current_state(&self, tenant_id: &str) -> Result<Option<ByokState>, ByokWriteError> {
        let rows = self
            .rows
            .query_rows(
                "SELECT state FROM tenant_byok_config WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant_id)],
            )
            .await
            .map_err(ByokWriteError::Transport)?;
        match rows.first() {
            None => Ok(None),
            Some(row) => {
                let s = col_opt_str(row, "state")
                    .ok_or_else(|| {
                        ByokWriteError::Transport("tenant_byok_config.state missing".to_owned())
                    })?
                    .parse::<ByokState>()
                    .map_err(|e| ByokWriteError::Transport(e.to_string()))?;
                Ok(Some(s))
            }
        }
    }

    /// Flip a tenant to BYOK `active` with its CMK identity + wrapped Tcs.
    ///
    /// Idempotent + safe to call on an already-active tenant (a re-activation
    /// re-wraps the Tcs and bumps `tcs_version`). Refuses to re-activate a
    /// crypto-shredded tenant (the monotonic terminal state).
    ///
    /// # Errors
    ///
    /// - [`ByokWriteError::Invalid`] when the parameters fail validation.
    /// - [`ByokWriteError::IllegalTransition`] when the tenant is `shredded`.
    /// - [`ByokWriteError::Transport`] on any D1 write failure.
    pub async fn activate(&self, act: &ByokActivation, now_ms: i64) -> Result<(), ByokWriteError> {
        act.validate()?;
        // Monotonic guard: crypto-shred is terminal — a shredded tenant's
        // ciphertext is unrecoverable, so re-activation would be a lie.
        if let Some(ByokState::Shredded) = self.current_state(&act.tenant_id).await? {
            return Err(ByokWriteError::IllegalTransition {
                from: ByokState::Shredded,
                to: ByokState::Active,
            });
        }

        // BLOB over D1-HTTP: the wrapped Tcs is written as a base64 STRING; the
        // read seam (`storage::byok_cas::decode_blob`) accepts both a base64
        // string and a byte-array, so this round-trips to the exact bytes.
        let tcs_b64 = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(&act.tcs_wrapped)
        };

        // 1) Persist the wrapped Tcs FIRST (so an active config never dangles
        //    over a missing secret). UPSERT: a re-activation bumps tcs_version.
        self.rows
            .query_rows(
                "INSERT INTO tenant_byok_secret \
                   (tenant_id, tcs_wrapped, cmk_key_id, tcs_version, wrapped_at_ms) \
                 VALUES (?1, ?2, ?3, 1, ?4) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   tcs_wrapped   = excluded.tcs_wrapped, \
                   cmk_key_id    = excluded.cmk_key_id, \
                   tcs_version   = tenant_byok_secret.tcs_version + 1, \
                   wrapped_at_ms = excluded.wrapped_at_ms",
                vec![
                    json!(act.tenant_id),
                    json!(tcs_b64),
                    json!(act.cmk_key_id),
                    json!(now_ms),
                ],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        // 2) Flip the config row to `active`. UPSERT preserves created_at_ms on
        //    conflict (only set on first insert).
        self.rows
            .query_rows(
                "INSERT INTO tenant_byok_config \
                   (tenant_id, mode, crypto_mode, cmk_provider, cmk_key_id, \
                    cmk_region, state, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   mode          = excluded.mode, \
                   crypto_mode   = excluded.crypto_mode, \
                   cmk_provider  = excluded.cmk_provider, \
                   cmk_key_id    = excluded.cmk_key_id, \
                   cmk_region    = excluded.cmk_region, \
                   state         = 'active', \
                   updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(act.tenant_id),
                    json!(act.mode.as_str()),
                    json!(act.crypto_mode.as_str()),
                    json!(act.cmk_provider),
                    json!(act.cmk_key_id),
                    json!(act.cmk_region),
                    json!(now_ms),
                ],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        // Audit: identity + provider + state only — NEVER key/secret material.
        tracing::info!(
            target: "corelink.byok.activation.audit",
            audit = true,
            op = "activate",
            tenant = %act.tenant_id,
            provider = %act.cmk_provider,
            cmk_key_id = %act.cmk_key_id,
            crypto_mode = act.crypto_mode.as_str(),
            state = "active",
            "BYOK activation: tenant flipped to active"
        );
        Ok(())
    }

    /// Crypto-shred KILL SWITCH — flip an active/partial tenant to `shredded`.
    ///
    /// The deliberate control-plane complement of the always-on CMK-revocation
    /// detector (`corelink_byok::revocation`): an operator (or a downstream
    /// erasure flow) can hard-stop BYOK for a tenant. Monotonic + idempotent:
    /// `shredded` is terminal (a second call is a no-op `Ok`); a tenant that
    /// was never active has nothing to shred and is rejected fail-CLOSED.
    ///
    /// # Errors
    ///
    /// - [`ByokWriteError::IllegalTransition`] when the tenant is not
    ///   active/partial/shredded (i.e. no active BYOK config to kill).
    /// - [`ByokWriteError::Transport`] on any D1 read/write failure.
    pub async fn deactivate(&self, tenant_id: &str, now_ms: i64) -> Result<(), ByokWriteError> {
        match self.current_state(tenant_id).await? {
            // Nothing active to kill — refuse rather than write a spurious
            // shredded record over a fresh/managed tenant.
            None | Some(ByokState::Inactive) | Some(ByokState::Pending) => {
                return Err(ByokWriteError::IllegalTransition {
                    from: ByokState::Inactive,
                    to: ByokState::Shredded,
                })
            }
            // Already shredded — idempotent success.
            Some(ByokState::Shredded) => return Ok(()),
            Some(ByokState::Active | ByokState::Partial) => {}
        }

        self.rows
            .query_rows(
                "UPDATE tenant_byok_config \
                 SET state = 'shredded', updated_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND state IN ('active', 'partial')",
                vec![json!(now_ms), json!(tenant_id)],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        tracing::warn!(
            target: "corelink.byok.activation.audit",
            audit = true,
            op = "deactivate",
            tenant = %tenant_id,
            state = "shredded",
            "BYOK kill switch: tenant crypto-shredded (state → shredded)"
        );
        Ok(())
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
        /// When `Some(fragment)`, ONLY the queries whose SQL contains
        /// `fragment` fail (every other query behaves normally). Lets a test
        /// fault a specific statement — e.g. the `customer_audit_events`
        /// insert — to prove the audit-fail-CLOSED / emit-before-mutate ordering.
        fail_on: Option<&'static str>,
    }

    impl MockD1 {
        fn with(canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
            Self {
                canned,
                calls: Mutex::new(Vec::new()),
                fail: false,
                fail_on: None,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::default()
            }
        }

        /// A mock that faults ONLY on queries whose SQL contains `fragment`.
        fn failing_on(fragment: &'static str, canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
            Self {
                canned,
                calls: Mutex::new(Vec::new()),
                fail: false,
                fail_on: Some(fragment),
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
            if let Some(fragment) = self.fail_on {
                if sql.contains(fragment) {
                    return Err(format!("D1 HTTP 500: induced failure on {fragment}"));
                }
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
            "https://humangr.com/corelink/en/customer/billing".to_owned(),
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
        // ADR-0071: find-ONLY stores the CHECK-safe base "read-only" + the
        // `find_only` marker (NOT a 4th `pat.scope` value — the 0037 CHECK forbids
        // it); combined with read/write it folds into the superset (read ⊇ find).
        assert_eq!(
            map_requested_scopes(&["cache:find-missing".to_owned()]).unwrap(),
            "read-only"
        );
        assert!(mint_is_find_only(&["cache:find-missing".to_owned()]));
        assert_eq!(
            map_requested_scopes(&["cache:read".to_owned(), "cache:find-missing".to_owned()])
                .unwrap(),
            "read-only"
        );
        // find + read is NOT find-only (it's a full read grant).
        assert!(!mint_is_find_only(&["cache:read".to_owned(), "cache:find-missing".to_owned()]));
        assert!(!mint_is_find_only(&["cache:read".to_owned()]));
        // Display: a find-only PAT surfaces as cache:find-missing (via the marker),
        // a normal read PAT as cache:read.
        assert_eq!(scope_to_list("read-only", true), vec!["cache:find-missing".to_owned()]);
        assert_eq!(scope_to_list("read-only", false), vec!["cache:read".to_owned()]);
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
            "no seeded customer_audit_events → honestly empty feed (BE-3)"
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
    fn overview_recent_activity_reads_seeded_audit_events() {
        // BE-3: the overview snapshot feed reads the same `customer_audit_events`
        // (0077) surface the audit endpoint serves — newest-first, mapped to
        // `CustomerAuditEventRow`, `severity` = constant `info`. A seeded event
        // surfaces on the snapshot; it is NOT fabricated and NOT hard-empty.
        let f = fixture_with(
            MockD1::with(vec![
                ("FROM tenant WHERE", vec![tenant_row_fixture()]),
                (
                    "FROM tenant_storage_state",
                    vec![row(&[
                        ("bytes_used", json!(1_i64)),
                        ("bytes_quota", json!(10_000_000_i64)),
                    ])],
                ),
                (
                    "FROM customer_audit_events",
                    vec![row(&[
                        ("id", json!(7)),
                        ("event_type", json!("pat.created")),
                        ("actor", json!("clpat_admin")),
                        ("target", json!("pat-7")),
                        ("ts_ms", json!(1_700_000_000_000_i64)),
                        ("detail", json!("Created API key \"ci\" (read-only)")),
                    ])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
            .unwrap();
        assert_eq!(
            resp.recent_activity.len(),
            1,
            "seeded customer_audit_events row surfaces on the snapshot (BE-3)"
        );
        assert_eq!(resp.recent_activity[0].event_id, "7");
        assert_eq!(resp.recent_activity[0].event_type, "pat.created");
        assert_eq!(resp.recent_activity[0].severity, "info");
        assert_eq!(resp.recent_activity[0].actor, "clpat_admin");
        assert!(
            resp.recent_activity[0].ts.ends_with('Z'),
            "ISO-8601 ts: {}",
            resp.recent_activity[0].ts
        );
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
        assert_eq!(
            resp.request_count, 0,
            "no monthly_request_counts row → honest 0"
        );
        assert!(resp.daily.is_empty(), "no per-day table: honest []");
    }

    #[test]
    fn usage_request_count_reads_monthly_counter() {
        // BE-1a: the running request counter the quota gate maintains
        // (monthly_request_counts, 0071) surfaces as a real usage-vs-quota
        // signal — a READ, never a hot-path write. Keyed on the same `YYYY-MM`
        // period as the storage read.
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
                (
                    "FROM monthly_request_counts",
                    vec![row(&[("request_count", json!(4_211_i64))])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
            .unwrap();
        assert_eq!(
            resp.request_count, 4_211,
            "seeded monthly_request_counts row surfaces (BE-1a)"
        );
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

    /// BE-1 + BE-2: `usage_daily` (0089) rollup — reads/writes/daily are the
    /// real period sums, hit_rate = hits/(hits+misses), and the time/$ estimates
    /// follow the frozen formulas.
    #[test]
    fn usage_daily_rollup_sums_days_and_computes_roi() {
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
                (
                    "FROM usage_daily",
                    vec![
                        row(&[
                            ("day", json!("2023-11-01")),
                            ("reads", json!(5_000_i64)),
                            ("writes", json!(1_000_i64)),
                            ("hits", json!(4_000_i64)),
                            ("misses", json!(1_000_i64)),
                        ]),
                        row(&[
                            ("day", json!("2023-11-02")),
                            ("reads", json!(3_000_i64)),
                            ("writes", json!(500_i64)),
                            ("hits", json!(2_000_i64)),
                            ("misses", json!(1_000_i64)),
                        ]),
                    ],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
            .unwrap();
        // Real period sums (were hardcoded 0).
        assert_eq!(resp.reads, 8_000, "reads summed across days");
        assert_eq!(resp.writes, 1_500, "writes summed across days");
        // Daily series, oldest-first, cas_bytes always 0 (no per-day byte history).
        assert_eq!(resp.daily.len(), 2);
        assert_eq!(
            resp.daily[0],
            DailyUsageBucket::new("2023-11-01", 5_000, 1_000, 0)
        );
        assert_eq!(
            resp.daily[1],
            DailyUsageBucket::new("2023-11-02", 3_000, 500, 0)
        );
        // hit_rate = hits/(hits+misses) = 6000/8000 = 0.75.
        assert_eq!(resp.hit_rate, Some(0.75));
        // time_saved_seconds = hits * 15 = 6000 * 15 = 90_000.
        assert_eq!(resp.time_saved_seconds, 90_000);
        // dollars_saved_cents = round(90_000 * 0.0000111 * 100) = round(99.9) = 100.
        assert_eq!(resp.dollars_saved_cents, 100);
    }

    /// hit_rate is `None` (not a fabricated 0.0/1.0) when there were no cache
    /// reads at all (hits + misses == 0) — even if writes happened. With no
    /// hits, the time/$ estimates are honest 0.
    #[test]
    fn usage_hit_rate_none_when_no_cache_reads() {
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
                (
                    "FROM usage_daily",
                    vec![row(&[
                        ("day", json!("2023-11-01")),
                        ("reads", json!(0_i64)),
                        ("writes", json!(42_i64)),
                        ("hits", json!(0_i64)),
                        ("misses", json!(0_i64)),
                    ])],
                ),
            ]),
            None,
        );
        let resp = f
            .handler
            .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
            .unwrap();
        assert_eq!(resp.writes, 42, "writes still surface");
        assert_eq!(
            resp.hit_rate, None,
            "no cache reads → honest null, never a fabricated rate"
        );
        assert_eq!(resp.time_saved_seconds, 0);
        assert_eq!(resp.dollars_saved_cents, 0);
    }

    /// The rollup is a READ-ONLY period `SELECT` bound to the tenant + period
    /// (`day LIKE 'YYYY-MM-%'`) — never a hot-path write.
    #[test]
    fn usage_daily_rollup_is_read_only_and_period_scoped() {
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
        f.handler
            .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
            .unwrap();
        let rollup_call =
            f.db.calls()
                .into_iter()
                .find(|(sql, _)| sql.contains("FROM usage_daily"))
                .expect("usage_daily rollup query issued");
        let (sql, binds) = rollup_call;
        assert!(sql.trim_start().starts_with("SELECT"), "READ-ONLY select");
        assert!(!sql.contains("INSERT") && !sql.contains("UPDATE"));
        assert!(sql.contains("day LIKE ?2"));
        assert_eq!(binds[0], json!(TENANT), "bound to the caller tenant");
        assert_eq!(
            binds[1],
            json!("2023-11-%"),
            "period-scoped LIKE for the current period"
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

    /// Item-4 fail-CLOSED: when the UNSKIPPABLE `customer_audit_events` insert
    /// faults, `create` MUST surface `AuditFailed` (→ 503) AND must NOT have run
    /// the `INSERT INTO pat` mutation (emit-before-mutate: the key is never
    /// minted without its customer-visible audit row). This is the marketing
    /// "unskippable audit trail" made true for the key-create control-plane op.
    #[test]
    fn keys_create_fails_closed_when_customer_audit_insert_faults() {
        let f = fixture_with(
            MockD1::failing_on(
                "customer_audit_events",
                vec![("FROM tenant WHERE", vec![tenant_row_fixture()])],
            ),
            None,
        );
        let err = f
            .handler
            .create(KeyCreateRequest::new(
                TENANT,
                "clpat_x",
                "deploy-key",
                vec!["cache:read".to_owned()],
                0,
            ))
            .expect_err("audit-insert fault must fail the create CLOSED");
        assert!(
            matches!(err, CustomerHandlerError::AuditFailed(_)),
            "expected AuditFailed, got {err:?}"
        );
        // Emit-before-mutate: the pat INSERT must NEVER have run.
        let calls = f.db.calls();
        assert!(
            !calls.iter().any(|(sql, _)| sql.contains("INSERT INTO pat")),
            "the pat mint must not commit when the unskippable audit row fails"
        );
        // The audit row WAS attempted (proving it is on the fail-CLOSED path).
        assert!(
            calls
                .iter()
                .any(|(sql, _)| sql.contains("customer_audit_events")),
            "the customer audit insert must be attempted before the mutation"
        );
    }

    /// Item-4 fail-CLOSED sibling for team invite: an audit-insert fault must
    /// surface `AuditFailed` and leave NO `team_member` seat behind.
    #[test]
    fn team_invite_fails_closed_when_customer_audit_insert_faults() {
        let _env = crate::email_hash::EnvGuard::acquire();
        let f = fixture_with(MockD1::failing_on("customer_audit_events", vec![]), None);
        let err = f
            .handler
            .invite(TeamInviteRequest::new(
                TENANT,
                "clpat_x",
                "Alice@Example.com",
                "Developer",
                0,
            ))
            .expect_err("audit-insert fault must fail the invite CLOSED");
        assert!(
            matches!(err, CustomerHandlerError::AuditFailed(_)),
            "expected AuditFailed, got {err:?}"
        );
        let calls = f.db.calls();
        assert!(
            !calls
                .iter()
                .any(|(sql, _)| sql.contains("INSERT INTO team_member")),
            "no seat may be written when the unskippable audit row fails"
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
        // A normal read PAT is NOT find-only (?10 = find_only = 0).
        assert_eq!(insert.1[9], json!(0));
    }

    /// ADR-0071: a find-only request stores the CHECK-safe base `read-only` +
    /// `find_only = 1` (NOT a 4th `pat.scope` value the 0037 CHECK would reject),
    /// and surfaces as the `cache:find-missing` capability it actually grants.
    #[test]
    fn keys_create_find_only_stores_read_only_base_plus_marker() {
        let f = fixture_with(MockD1::with(vec![]), None);
        let resp = f
            .handler
            .create(KeyCreateRequest::new(
                TENANT,
                "clpat_x",
                "find-key",
                vec!["cache:find-missing".to_owned()],
                0,
            ))
            .unwrap();
        // Displayed as find-missing (via the marker), never cache:read.
        assert_eq!(resp.pat.scopes, vec!["cache:find-missing".to_owned()]);
        let calls = f.db.calls();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT INTO pat"))
            .expect("INSERT executed");
        // Base scope is CHECK-safe `read-only` (?4); the `find_only` marker (?10) = 1.
        assert_eq!(insert.1[3], json!("read-only"));
        assert_eq!(insert.1[9], json!(1));
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
        //
        // EMAIL_HASH_SALT is process-global: hold the shared lock (forces salt
        // UNSET) so a concurrent salted test can't perturb the bind below.
        let _env = crate::email_hash::EnvGuard::acquire();
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
        // CTRL-PRIV-001: the raw email is NEVER a bind value — only the canonical
        // pseudonymized hash of the NORMALIZED (trim+lowercase) email, computed by
        // the ONE shared helper (the exact join key the rectification + signup-worker
        // accept side match on — C-ACCEPT parity). Asserting against the helper proves
        // the WRITE site routes through the single source of truth (matching invariant).
        let expected_hash = crate::email_hash::hash_email("alice@example.com");
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
        assert_eq!(
            insert.1[4],
            json!("clpat_x"),
            "invited_by = caller principal"
        );

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
        // RBAC hardening: `owner` is NEVER mintable via a self-serve invite — it
        // maps to `admin` (defense-in-depth so no code path persists a 2nd owner).
        assert_eq!(normalize_invite_role("Owner"), "admin");
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
                    vec![row(&[
                        ("role", json!("member")),
                        ("status", json!("active")),
                    ])],
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
            .remove(TeamRemoveRequest::new(
                TENANT,
                "clpat_owner",
                "user_member1",
                0,
            ))
            .unwrap();
        assert_eq!(
            resp.revoked_pats, 2,
            "both of the member's live PATs revoked"
        );
        assert_eq!(resp.member.status, "removed");

        // The load-bearing effects must both have been issued to D1.
        let sqls: Vec<String> = f.db.calls().into_iter().map(|(s, _)| s).collect();
        assert!(
            sqls.iter()
                .any(|s| s.contains("UPDATE pat SET revoked_at_ms")),
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
                vec![row(&[
                    ("role", json!("owner")),
                    ("status", json!("active")),
                ])],
            )]),
            None,
        );
        let err = f
            .handler
            .remove(TeamRemoveRequest::new(TENANT, "clpat_x", "user_owner", 0))
            .unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::Unauthorized(_)),
            "{err:?}"
        );
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
        assert!(
            matches!(err, CustomerHandlerError::NotFound { .. }),
            "{err:?}"
        );
    }

    // ── Audit query ──────────────────────────────────────────────────────────

    #[test]
    fn audit_query_empty_source_returns_empty_rows() {
        // No rows in customer_audit_events → honest empty page (never fabricated).
        let f = fixture_with(MockD1::with(vec![]), None);
        let resp = f
            .handler
            .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
            .unwrap();
        assert!(resp.rows.is_empty());
    }

    #[test]
    fn audit_query_maps_customer_audit_events_rows() {
        // Two seeded events (migration 0077). The read maps each row to a
        // `CustomerAuditEventRow` and preserves the SQL's newest-first order;
        // `ts` is rendered ISO-8601 and `severity` is the constant `info`.
        let f = fixture_with(
            MockD1::with(vec![(
                "FROM customer_audit_events",
                vec![
                    row(&[
                        ("id", json!(2)),
                        ("event_type", json!("team.invited")),
                        ("actor", json!("clpat_admin")),
                        ("target", json!("inv-2")),
                        ("ts_ms", json!(1_700_000_000_000_i64)),
                        ("detail", json!("Invited a team member with role member")),
                    ]),
                    row(&[
                        ("id", json!(1)),
                        ("event_type", json!("pat.created")),
                        ("actor", json!("clpat_admin")),
                        ("target", json!("pat-1")),
                        ("ts_ms", json!(1_699_999_999_000_i64)),
                        ("detail", json!("Created API key \"ci\" (read-only)")),
                    ]),
                ],
            )]),
            None,
        );
        let resp = f
            .handler
            .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
            .unwrap();
        assert_eq!(resp.rows.len(), 2);
        assert_eq!(resp.rows[0].event_id, "2");
        assert_eq!(resp.rows[0].event_type, "team.invited");
        assert_eq!(resp.rows[0].severity, "info");
        assert_eq!(resp.rows[0].actor, "clpat_admin");
        assert!(
            resp.rows[0].ts.ends_with('Z'),
            "ISO-8601 ts: {}",
            resp.rows[0].ts
        );
        assert_eq!(resp.rows[1].event_id, "1");
        assert_eq!(resp.rows[1].event_type, "pat.created");
    }

    #[test]
    fn iso8601_to_ms_round_trips_and_rejects_garbage() {
        // Exact inverse of `ms_to_iso8601` on the canonical (second-precision)
        // form, plus the lenient variants the contract may receive.
        assert_eq!(iso8601_to_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            iso8601_to_ms("2023-11-14T22:13:20Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(iso8601_to_ms("2023-11-14"), Some(1_699_920_000_000)); // bare date → 00:00:00Z
        assert_eq!(
            iso8601_to_ms("2023-11-14T22:13:20.999Z"), // fractional dropped
            Some(1_700_000_000_000)
        );
        assert_eq!(
            iso8601_to_ms("2023-11-14 22:13:20"),
            Some(1_700_000_000_000)
        ); // space sep, no Z
           // Garbage → None (caller then applies no filter).
        assert_eq!(iso8601_to_ms("not-a-date"), None);
        assert_eq!(iso8601_to_ms("2023-13-01"), None); // month out of range
        assert_eq!(iso8601_to_ms("2023-11-14T25:00:00Z"), None); // hour out of range
        assert_eq!(iso8601_to_ms(""), None);
    }

    #[test]
    fn audit_query_applies_since_filter_in_sql_and_binds() {
        // `?from=` must reach the SQL as `ts_ms >= ?` with the ISO-8601 value
        // parsed into the integer ts_ms domain — not silently ignored.
        let f = fixture_with(MockD1::with(vec![]), None);
        let _ = f
            .handler
            .query(AuditQueryRequest::new(
                TENANT,
                "clpat_x",
                Some("2023-11-14T22:13:20Z".to_owned()),
                vec![],
                0,
            ))
            .unwrap();
        let calls = f.db.calls();
        let (sql, binds) = calls
            .iter()
            .find(|(s, _)| s.contains("FROM customer_audit_events"))
            .expect("the audit SELECT must have run");
        assert!(sql.contains("ts_ms >= ?2"), "since filter missing: {sql}");
        // tenant (?1), since-ms (?2), LIMIT (?3) — parameterized, no interpolation.
        assert_eq!(binds.len(), 3, "binds: {binds:?}");
        assert_eq!(binds[0], json!(TENANT));
        assert_eq!(binds[1], json!(1_700_000_000_000_i64));
        assert_eq!(binds[2], json!(AUDIT_QUERY_LIMIT));
        assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?3"), "{sql}");
    }

    #[test]
    fn audit_query_applies_event_types_filter_in_sql_and_binds() {
        // `?kind=` must reach the SQL as a parameterized `event_type IN (…)`
        // with one BOUND placeholder per type (never string-interpolated).
        let f = fixture_with(MockD1::with(vec![]), None);
        let _ = f
            .handler
            .query(AuditQueryRequest::new(
                TENANT,
                "clpat_x",
                None,
                vec!["pat.created".to_owned(), "team.invited".to_owned()],
                0,
            ))
            .unwrap();
        let calls = f.db.calls();
        let (sql, binds) = calls
            .iter()
            .find(|(s, _)| s.contains("FROM customer_audit_events"))
            .expect("the audit SELECT must have run");
        assert!(
            sql.contains("event_type IN (?2, ?3)"),
            "event_types filter missing/not parameterized: {sql}"
        );
        // The values are BOUND, not embedded in the SQL string (no injection).
        assert!(!sql.contains("pat.created"), "value interpolated: {sql}");
        // tenant (?1), two event types (?2,?3), LIMIT (?4).
        assert_eq!(binds.len(), 4, "binds: {binds:?}");
        assert_eq!(binds[0], json!(TENANT));
        assert_eq!(binds[1], json!("pat.created"));
        assert_eq!(binds[2], json!("team.invited"));
        assert_eq!(binds[3], json!(AUDIT_QUERY_LIMIT));
        assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?4"), "{sql}");
    }

    #[test]
    fn audit_query_combines_since_and_event_types_filters() {
        // Both filters together: placeholders stay correctly numbered and the
        // tenant scope stays fail-CLOSED at ?1.
        let f = fixture_with(MockD1::with(vec![]), None);
        let _ = f
            .handler
            .query(AuditQueryRequest::new(
                TENANT,
                "clpat_x",
                Some("2023-11-14T22:13:20Z".to_owned()),
                vec!["pat.created".to_owned()],
                0,
            ))
            .unwrap();
        let calls = f.db.calls();
        let (sql, binds) = calls
            .iter()
            .find(|(s, _)| s.contains("FROM customer_audit_events"))
            .expect("the audit SELECT must have run");
        assert!(sql.contains("WHERE tenant_id = ?1"), "{sql}");
        assert!(sql.contains("ts_ms >= ?2"), "{sql}");
        assert!(sql.contains("event_type IN (?3)"), "{sql}");
        assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?4"), "{sql}");
        assert_eq!(
            binds,
            &vec![
                json!(TENANT),
                json!(1_700_000_000_000_i64),
                json!("pat.created"),
                json!(AUDIT_QUERY_LIMIT),
            ]
        );
    }

    #[test]
    fn audit_query_unparseable_since_applies_no_filter() {
        // A malformed `from=` is lenient: NO `ts_ms` clause, behaves as before.
        let f = fixture_with(MockD1::with(vec![]), None);
        let _ = f
            .handler
            .query(AuditQueryRequest::new(
                TENANT,
                "clpat_x",
                Some("garbage".to_owned()),
                vec![],
                0,
            ))
            .unwrap();
        let calls = f.db.calls();
        let (sql, binds) = calls
            .iter()
            .find(|(s, _)| s.contains("FROM customer_audit_events"))
            .expect("the audit SELECT must have run");
        assert!(
            !sql.contains("ts_ms >="),
            "bad since must not filter: {sql}"
        );
        assert_eq!(binds.len(), 2, "tenant + LIMIT only: {binds:?}");
        assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?2"), "{sql}");
    }

    #[test]
    fn audit_query_no_filters_matches_prior_shape() {
        // Regression guard: with neither param the SQL is the original
        // tenant-only, bounded, newest-first form.
        let f = fixture_with(MockD1::with(vec![]), None);
        let _ = f
            .handler
            .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
            .unwrap();
        let calls = f.db.calls();
        let (sql, binds) = calls
            .iter()
            .find(|(s, _)| s.contains("FROM customer_audit_events"))
            .expect("the audit SELECT must have run");
        assert!(!sql.contains("ts_ms >="), "{sql}");
        assert!(!sql.contains("event_type IN"), "{sql}");
        assert_eq!(binds, &vec![json!(TENANT), json!(AUDIT_QUERY_LIMIT)]);
        assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?2"), "{sql}");
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

    // ── BYOK config read model (Wave 2) ────────────────────────────────────────

    /// Hermetic async mock for the [`ByokConfigRows`] seam.
    #[derive(Debug, Default)]
    struct MockByokRows {
        rows: Vec<D1Row>,
        fail: Option<String>,
    }

    impl ByokConfigRows for MockByokRows {
        async fn query_rows(&self, _sql: &str, _binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            match &self.fail {
                Some(e) => Err(e.clone()),
                None => Ok(self.rows.clone()),
            }
        }
    }

    fn byok_config_row_fixture() -> D1Row {
        row(&[
            ("tenant_id", json!(TENANT)),
            ("mode", json!("byok")),
            ("crypto_mode", json!("convergent")),
            ("cmk_provider", json!("aws")),
            ("cmk_key_id", json!("arn:aws:kms:us-east-1:1:key/abc")),
            ("cmk_region", json!("us-east-1")),
            ("state", json!("active")),
        ])
    }

    #[test]
    fn byok_mode_parse_round_trip_every_variant() {
        for m in [ByokMode::Managed, ByokMode::Byok, ByokMode::Hyok] {
            assert_eq!(m.as_str().parse::<ByokMode>().unwrap(), m);
        }
    }

    #[test]
    fn byok_crypto_mode_parse_round_trip_every_variant() {
        for m in [ByokCryptoMode::Convergent, ByokCryptoMode::Random] {
            assert_eq!(m.as_str().parse::<ByokCryptoMode>().unwrap(), m);
        }
    }

    #[test]
    fn byok_state_parse_round_trip_every_variant() {
        for s in [
            ByokState::Inactive,
            ByokState::Pending,
            ByokState::Active,
            ByokState::Partial,
            ByokState::Shredded,
        ] {
            assert_eq!(s.as_str().parse::<ByokState>().unwrap(), s);
        }
    }

    #[test]
    fn byok_enums_fail_closed_on_unknown_string() {
        // Fail-CLOSED: an unknown value is an Err, NEVER a permissive default
        // (an unrecognised custody rung must not silently mean "encryption off").
        assert!(matches!(
            "rot13".parse::<ByokMode>(),
            Err(ByokConfigError::Parse(_))
        ));
        assert!(matches!(
            "".parse::<ByokMode>(),
            Err(ByokConfigError::Parse(_))
        ));
        assert!(matches!(
            "homomorphic".parse::<ByokCryptoMode>(),
            Err(ByokConfigError::Parse(_))
        ));
        assert!(matches!(
            "rotated".parse::<ByokState>(),
            Err(ByokConfigError::Parse(_))
        ));
        // Case sensitivity is enforced (the D1 CHECK is lowercase-only).
        assert!("BYOK".parse::<ByokMode>().is_err());
        assert!("Active".parse::<ByokState>().is_err());
    }

    #[test]
    fn is_encryption_active_truth_table() {
        let cfg = |state| TenantByokConfig {
            tenant_id: TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode: ByokCryptoMode::Convergent,
            cmk_provider: None,
            cmk_key_id: None,
            cmk_region: None,
            state,
        };
        // Active + Partial encrypt new writes → active.
        assert!(is_encryption_active(&cfg(ByokState::Active)));
        assert!(is_encryption_active(&cfg(ByokState::Partial)));
        // The rest do not.
        assert!(!is_encryption_active(&cfg(ByokState::Inactive)));
        assert!(!is_encryption_active(&cfg(ByokState::Pending)));
        assert!(!is_encryption_active(&cfg(ByokState::Shredded)));
    }

    #[tokio::test]
    async fn get_byok_config_none_when_absent() {
        let reader = D1ByokConfigReader::new(Arc::new(MockByokRows::default()));
        let got = reader.get_byok_config(TENANT).await.unwrap();
        assert!(
            got.is_none(),
            "no row = BYOK not configured = today's behaviour"
        );
    }

    #[tokio::test]
    async fn get_byok_config_parses_present_row() {
        let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
            rows: vec![byok_config_row_fixture()],
            fail: None,
        }));
        let cfg = reader.get_byok_config(TENANT).await.unwrap().unwrap();
        assert_eq!(cfg.tenant_id, TENANT);
        assert_eq!(cfg.mode, ByokMode::Byok);
        assert_eq!(cfg.crypto_mode, ByokCryptoMode::Convergent);
        assert_eq!(cfg.cmk_provider.as_deref(), Some("aws"));
        assert_eq!(cfg.state, ByokState::Active);
        assert!(is_encryption_active(&cfg));
    }

    #[tokio::test]
    async fn get_byok_config_fail_closed_on_unparseable_enum() {
        // A corrupt/unknown `state` must be an Err, not a silent default.
        let mut bad = byok_config_row_fixture();
        bad.insert("state".to_owned(), json!("frobnicated"));
        let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
            rows: vec![bad],
            fail: None,
        }));
        assert!(matches!(
            reader.get_byok_config(TENANT).await,
            Err(ByokConfigError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn get_byok_config_fail_closed_on_transport_error() {
        let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
            rows: vec![],
            fail: Some("D1 HTTP 500: transport down".to_owned()),
        }));
        assert!(matches!(
            reader.get_byok_config(TENANT).await,
            Err(ByokConfigError::Transport(_))
        ));
    }

    // ── BYOK config WRITER (activation seam) ───────────────────────────────────

    /// A STATEFUL hermetic mock of the two 0081 tables over the async
    /// [`ByokConfigRows`] seam. It interprets the exact SQL the writer + readers
    /// emit so a WRITE is observable by a later READ (proving read-after-write).
    #[derive(Debug, Default)]
    struct StatefulByokDb {
        config: Mutex<std::collections::HashMap<String, D1Row>>,
        secret: Mutex<std::collections::HashMap<String, D1Row>>,
        fail: Option<String>,
    }

    impl StatefulByokDb {
        fn tenant_of(binds: &[Value], idx: usize) -> String {
            binds
                .get(idx)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        }
    }

    impl ByokConfigRows for StatefulByokDb {
        async fn query_rows(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            if sql.contains("INSERT INTO tenant_byok_secret") {
                let tenant = Self::tenant_of(&binds, 0);
                self.secret.lock().unwrap().insert(
                    tenant.clone(),
                    row(&[
                        ("tenant_id", json!(tenant)),
                        ("tcs_wrapped", binds[1].clone()),
                        ("cmk_key_id", binds[2].clone()),
                        ("tcs_version", json!(1)),
                    ]),
                );
                return Ok(Vec::new());
            }
            if sql.contains("INSERT INTO tenant_byok_config") {
                let tenant = Self::tenant_of(&binds, 0);
                self.config.lock().unwrap().insert(
                    tenant.clone(),
                    row(&[
                        ("tenant_id", json!(tenant)),
                        ("mode", binds[1].clone()),
                        ("crypto_mode", binds[2].clone()),
                        ("cmk_provider", binds[3].clone()),
                        ("cmk_key_id", binds[4].clone()),
                        ("cmk_region", binds[5].clone()),
                        ("state", json!("active")),
                    ]),
                );
                return Ok(Vec::new());
            }
            if sql.contains("UPDATE tenant_byok_config") {
                // deactivate: binds = [now_ms, tenant]
                let tenant = Self::tenant_of(&binds, 1);
                if let Some(r) = self.config.lock().unwrap().get_mut(&tenant) {
                    r.insert("state".to_owned(), json!("shredded"));
                }
                return Ok(Vec::new());
            }
            if sql.contains("FROM tenant_byok_secret") {
                let tenant = Self::tenant_of(&binds, 0);
                return Ok(self
                    .secret
                    .lock()
                    .unwrap()
                    .get(&tenant)
                    .cloned()
                    .into_iter()
                    .collect());
            }
            if sql.contains("FROM tenant_byok_config") {
                let tenant = Self::tenant_of(&binds, 0);
                return Ok(self
                    .config
                    .lock()
                    .unwrap()
                    .get(&tenant)
                    .cloned()
                    .into_iter()
                    .collect());
            }
            Ok(Vec::new())
        }
    }

    const NOW_ACT_MS: i64 = 1_700_000_000_000;

    fn activation_fixture() -> ByokActivation {
        ByokActivation {
            tenant_id: TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode: ByokCryptoMode::Convergent,
            cmk_provider: "aws".to_owned(),
            cmk_key_id: "arn:aws:kms:us-east-1:1:key/abc".to_owned(),
            cmk_region: Some("us-east-1".to_owned()),
            tcs_wrapped: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03],
        }
    }

    /// Read-after-write: activate() flips the tenant to `active`, and a
    /// subsequent read observes encryption ENGAGED (engagement_for → Encrypt)
    /// with the wrapped Tcs round-tripping to the exact bytes.
    #[tokio::test]
    async fn activate_then_read_engages_encryption() {
        use crate::storage::byok_cas::{
            engagement_for, ByokEngagement, ByokSecretSource, D1ByokSecretReader,
        };

        let db = Arc::new(StatefulByokDb::default());
        let writer = D1ByokConfigWriter::new(db.clone());
        let reader = D1ByokConfigReader::new(db.clone());

        let act = activation_fixture();
        writer.activate(&act, NOW_ACT_MS).await.expect("activate");

        // Config read observes the active state → encryption engaged.
        let cfg = reader
            .get_byok_config(TENANT)
            .await
            .expect("read")
            .expect("row present after activation");
        assert_eq!(cfg.state, ByokState::Active);
        assert_eq!(cfg.mode, ByokMode::Byok);
        assert_eq!(cfg.crypto_mode, ByokCryptoMode::Convergent);
        assert_eq!(cfg.cmk_provider.as_deref(), Some("aws"));
        assert!(is_encryption_active(&cfg));
        assert!(
            matches!(
                engagement_for(&cfg),
                ByokEngagement::Encrypt(ByokCryptoMode::Convergent)
            ),
            "an active tenant must ENGAGE convergent encryption on the CAS path"
        );

        // The wrapped Tcs round-trips through the base64 BLOB encoding to the
        // EXACT bytes the caller supplied — so the Tcs is resolvable (a missing
        // Tcs under an active state would fail CLOSED on the read path).
        let secret_reader = D1ByokSecretReader::new(db.clone());
        let wrapped = secret_reader
            .get_wrapped_tcs(TENANT)
            .await
            .expect("secret read")
            .expect("wrapped Tcs present after activation");
        assert_eq!(wrapped.tcs_wrapped, act.tcs_wrapped);
        assert_eq!(
            wrapped.cmk_key_id.as_deref(),
            Some("arn:aws:kms:us-east-1:1:key/abc")
        );
    }

    /// Kill switch: deactivate() flips active → shredded, and a subsequent read
    /// observes encryption DISENGAGED (engagement_for → Plaintext,
    /// is_encryption_active false).
    #[tokio::test]
    async fn deactivate_kill_switch_disengages_encryption() {
        use crate::storage::byok_cas::{engagement_for, ByokEngagement};

        let db = Arc::new(StatefulByokDb::default());
        let writer = D1ByokConfigWriter::new(db.clone());
        let reader = D1ByokConfigReader::new(db.clone());

        writer
            .activate(&activation_fixture(), NOW_ACT_MS)
            .await
            .expect("activate");
        writer
            .deactivate(TENANT, NOW_ACT_MS + 1)
            .await
            .expect("deactivate");

        let cfg = reader
            .get_byok_config(TENANT)
            .await
            .expect("read")
            .expect("row still present");
        assert_eq!(cfg.state, ByokState::Shredded);
        assert!(!is_encryption_active(&cfg));
        assert!(matches!(engagement_for(&cfg), ByokEngagement::Plaintext));

        // Idempotent: a second kill switch on an already-shredded tenant is Ok.
        writer
            .deactivate(TENANT, NOW_ACT_MS + 2)
            .await
            .expect("idempotent shred");
    }

    /// Re-activating a crypto-shredded tenant is refused (monotonic terminal).
    #[tokio::test]
    async fn activate_refuses_reactivation_of_shredded() {
        let db = Arc::new(StatefulByokDb::default());
        let writer = D1ByokConfigWriter::new(db.clone());
        writer
            .activate(&activation_fixture(), NOW_ACT_MS)
            .await
            .expect("activate");
        writer
            .deactivate(TENANT, NOW_ACT_MS + 1)
            .await
            .expect("shred");
        let err = writer
            .activate(&activation_fixture(), NOW_ACT_MS + 2)
            .await
            .expect_err("re-activation of a shredded tenant must fail");
        assert!(matches!(
            err,
            ByokWriteError::IllegalTransition {
                from: ByokState::Shredded,
                to: ByokState::Active
            }
        ));
    }

    /// Deactivating a tenant that was never active is refused fail-CLOSED.
    #[tokio::test]
    async fn deactivate_fresh_tenant_is_illegal() {
        let db = Arc::new(StatefulByokDb::default());
        let writer = D1ByokConfigWriter::new(db);
        let err = writer
            .deactivate(TENANT, NOW_ACT_MS)
            .await
            .expect_err("nothing to shred");
        assert!(matches!(err, ByokWriteError::IllegalTransition { .. }));
    }

    /// Validation fail-CLOSED: managed custody, unknown provider, empty CMK
    /// identity, and an empty wrapped-Tcs are all rejected BEFORE any D1 write.
    #[tokio::test]
    async fn activate_validation_rejects_bad_params() {
        let db = Arc::new(StatefulByokDb::default());
        let writer = D1ByokConfigWriter::new(db.clone());

        let mut managed = activation_fixture();
        managed.mode = ByokMode::Managed;
        assert!(matches!(
            writer.activate(&managed, NOW_ACT_MS).await,
            Err(ByokWriteError::Invalid(_))
        ));

        let mut bad_provider = activation_fixture();
        bad_provider.cmk_provider = "corelink_managed".to_owned();
        assert!(matches!(
            writer.activate(&bad_provider, NOW_ACT_MS).await,
            Err(ByokWriteError::Invalid(_))
        ));

        let mut empty_key = activation_fixture();
        empty_key.cmk_key_id = "  ".to_owned();
        assert!(matches!(
            writer.activate(&empty_key, NOW_ACT_MS).await,
            Err(ByokWriteError::Invalid(_))
        ));

        let mut empty_tcs = activation_fixture();
        empty_tcs.tcs_wrapped = Vec::new();
        assert!(matches!(
            writer.activate(&empty_tcs, NOW_ACT_MS).await,
            Err(ByokWriteError::Invalid(_))
        ));

        // Nothing was persisted — a rejected activation is invisible to a read.
        assert!(db.config.lock().unwrap().is_empty());
        assert!(db.secret.lock().unwrap().is_empty());
    }

    /// A D1 transport failure surfaces as `Transport`, never a silent success.
    #[tokio::test]
    async fn activate_fails_closed_on_transport_error() {
        let db = Arc::new(StatefulByokDb {
            fail: Some("D1 HTTP 500: transport down".to_owned()),
            ..StatefulByokDb::default()
        });
        let writer = D1ByokConfigWriter::new(db);
        assert!(matches!(
            writer.activate(&activation_fixture(), NOW_ACT_MS).await,
            Err(ByokWriteError::Transport(_))
        ));
    }
}
