//! `GET /v1/audit/analytics/{event-count, timeline}` — customer-facing
//! audit-analytics routes (Wave-18; Neon analytics shadow consumption).
//!
//! Wave 15 shipped `/v1/audit/export` (NDJSON over the canonical R2
//! archive). NDJSON is the right shape for forensic export, but
//! analytics queries (multi-event aggregation, time-bucketed counts)
//! are painful when implemented as a client-side post-process. Wave 18
//! exposes two SQL-backed aggregate endpoints over the Neon analytics
//! shadow (`audit_events_shadow`; see
//! `migrations/neon/0001_audit_events_shadow.sql`):
//!
//! - `GET /v1/audit/analytics/event-count?from=&to=&event_type=` —
//!   total event count per `event_type` over `[from, to)`.
//! - `GET /v1/audit/analytics/timeline?from=&to=&granularity=` —
//!   time-bucketed count over `[from, to)` with bucket size
//!   `granularity` ms.
//!
//! ## Source-of-truth discipline
//!
//! These endpoints answer ANALYTICS questions, not COMPLIANCE
//! questions. A compliance auditor verifying chain integrity MUST use
//! `corelink audit verify` against the canonical R2 NDJSON archive
//! (Wave 15) — the Neon shadow is a convenience read tier with a ≤
//! 5-min nominal lag. The OpenAPI doc explicitly tags this disclaimer.
//!
//! ## Auth + tenant isolation
//!
//! The route mirrors the `audit_export.rs` middleware pattern:
//! production wiring slots a JWT-validating tower middleware that
//! injects `X-Tenant-Id`. The handler:
//!
//! 1. Parses the authenticated tenant.
//! 2. Binds the `NeonShadowSink` to that tenant (the sink rejects
//!    cross-tenant rows at construction time; production wiring SETs
//!    `app.current_tenant` GUC so SQL RLS is the authoritative gate).
//! 3. Rate-limits per-tenant (10 analytics queries / minute / tenant).
//! 4. Emits `corelink.audit.analytics_query.v1` BEFORE serving bytes
//!    (fail-CLOSED ordering).
//!
//! ## Rate limit
//!
//! `corelink-ratelimit::BucketKey::per_tenant_per_endpoint(tenant, "audit.analytics")`.
//! 10 queries / 60s / tenant — looser than the export endpoint
//! (analytics is read-only over a pre-flushed shadow; the cost is a
//! Postgres aggregate query, not a full R2 list+walk).

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Extension, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use corelink_analytics::Region;
use corelink_audit_chain::{NeonShadowSink, TimelineBucket};
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::wall_clock::{default_wall_clock, WallClock};

/// Canonical route path for the event-count aggregate.
pub const ROUTE_EVENT_COUNT: &str = "/v1/audit/analytics/event-count";

/// Canonical route path for the timeline aggregate.
pub const ROUTE_TIMELINE: &str = "/v1/audit/analytics/timeline";

/// Header the production middleware uses to inject the JWT-validated
/// tenant id (mirrors `audit_export::TENANT_ID_HEADER`).
pub const TENANT_ID_HEADER: &str = "x-tenant-id";

/// Canonical CloudEvents `type` for the audit-analytics query emit.
pub const EVENT_TYPE_ANALYTICS_QUERY: &str = "corelink.audit.analytics_query.v1";

/// Maximum granularity bucket cardinality enforced by the timeline
/// endpoint. A `(to - from) / granularity` request exceeding this
/// cardinality is rejected with `400 Bad Request` so a single query
/// cannot exhaust analytics planner memory.
pub const MAX_TIMELINE_BUCKETS: u64 = 1_000;

/// Maximum granularity span (1 day in ms) enforced as a coarse upper
/// bound on the bucket width. Granularities above this are nonsensical
/// for an audit-event timeline and rejected at the route boundary.
pub const MAX_GRANULARITY_MS: u64 = 24 * 60 * 60 * 1_000;

/// Audit row captured per analytics request. Mirrors the `ExportAuditRow`
/// shape so the wave-17 dashboard widgets can union the two emit
/// streams on `event_type`.
///
/// ## Wave-29 — `region_source` field
///
/// Wave-27 wired the CF Worker `RequestPrelude` into the route. Wave-29
/// finishes the loop by adding the `region_source` field so operators
/// can observe (per emit) whether the shadow sink was resolved through
/// the prelude hot path (`Some("prelude")` — no D1 round-trip) or the
/// wave-21 resolver fallback (`Some("fallback")`). Non-resolver emit
/// arms (bad-request, rate-limit, clock-unavailable, audit-failed,
/// request-prelude-missing marker) leave the field as `None` because
/// no shadow-sink resolution was attempted.
///
/// The field is additive — `AnalyticsAuditRow` is `#[non_exhaustive]`
/// and the canonical [`AnalyticsAuditRow::new`] constructor defaults
/// `region_source` to `None`. Call sites on the resolver path
/// (`event_count` + `timeline` happy-path emits) decorate the row
/// via [`AnalyticsAuditRow::with_region_source`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalyticsAuditRow {
    /// Canonical CloudEvents `type`.
    pub event_type: String,
    /// Tenant id (from `X-Tenant-Id`).
    pub authenticated_tenant: Option<Uuid>,
    /// Canonical endpoint slug (`"event_count"` / `"timeline"`).
    pub endpoint: String,
    /// Window lower bound (Unix epoch ms; inclusive).
    pub from_ms: u64,
    /// Window upper bound (Unix epoch ms; exclusive).
    pub to_ms: u64,
    /// Bucket count returned to the caller (after filtering / aggregation).
    pub buckets_returned: u64,
    /// Exit status: `"ok"` / `"unauthorized"` / `"rate_limited"` /
    /// `"bad_request"` / `"backend_error"` / `"audit_failed"`.
    pub exit_status: String,
    /// Wave-29 telemetry — region resolution source on the rows whose
    /// emit path attempted a shadow-sink resolve. Canonical values:
    /// `"prelude"` (wave-26 `RequestPrelude` hot path — no D1 round-
    /// trip) or `"fallback"` (wave-21 `TenantRegionResolver` round-
    /// trip). `None` on pre-resolution exit arms (bad-request, rate-
    /// limit, clock-unavailable, request-prelude-missing marker).
    pub region_source: Option<String>,
}

/// Canonical `region_source` value for the wave-29 prelude hot path
/// (no resolver round-trip; the wave-26 `RequestPrelude` carried the
/// pre-resolved region).
pub const REGION_SOURCE_PRELUDE: &str = "prelude";

/// Canonical `region_source` value for the wave-21 resolver fallback
/// path (a `TenantRegionResolver::resolve_region` round-trip was
/// performed).
pub const REGION_SOURCE_FALLBACK: &str = "fallback";

impl AnalyticsAuditRow {
    /// Construct a new analytics audit row with every field explicit.
    /// `#[non_exhaustive]` blocks struct-expression construction from
    /// external crates so we expose this builder.
    ///
    /// `region_source` defaults to `None`; decorate the row via
    /// [`Self::with_region_source`] on the resolver-path emit arms.
    #[must_use]
    pub fn new(
        event_type: String,
        authenticated_tenant: Option<Uuid>,
        endpoint: String,
        from_ms: u64,
        to_ms: u64,
        buckets_returned: u64,
        exit_status: String,
    ) -> Self {
        Self {
            event_type,
            authenticated_tenant,
            endpoint,
            from_ms,
            to_ms,
            buckets_returned,
            exit_status,
            region_source: None,
        }
    }

    /// Wave-29: decorate this row with the `region_source` telemetry
    /// field (`"prelude"` on the hot path; `"fallback"` on the
    /// resolver round-trip arm). The constants
    /// [`REGION_SOURCE_PRELUDE`] / [`REGION_SOURCE_FALLBACK`] are the
    /// canonical values.
    #[must_use]
    pub fn with_region_source(mut self, region_source: &'static str) -> Self {
        self.region_source = Some(region_source.to_string());
        self
    }
}

/// Audit emit trait for the analytics routes.
pub trait AnalyticsAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`AnalyticsAuditRow`]. Fail-CLOSED on `Err` —
    /// the route returns 503.
    ///
    /// # Errors
    ///
    /// Static error string when the pipeline is closed.
    fn emit(&self, row: AnalyticsAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink for analytics audit emits.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAnalyticsAuditSink {
    inner: Arc<Mutex<Vec<AnalyticsAuditRow>>>,
}

impl InMemoryAnalyticsAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured row.
    ///
    /// # Errors
    ///
    /// Static error string when the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<AnalyticsAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "analytics audit sink mutex poisoned")?;
        Ok(g.clone())
    }
}

impl AnalyticsAuditSink for InMemoryAnalyticsAuditSink {
    fn emit(&self, row: AnalyticsAuditRow) -> Result<(), &'static str> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "analytics audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}

/// Per-tenant shadow-sink factory. Production wiring binds this to
/// the per-region Neon project (see
/// `crates/corelink-audit-chain/src/neon_shadow.rs` module docs).
///
/// The factory is the swap point that lets test harnesses inject an
/// `InMemoryNeonShadowSink` per (tenant, region) while production wires
/// a `RealNeonShadowSink` per tenant's pinned region.
pub trait ShadowSinkFactory: Send + Sync + core::fmt::Debug {
    /// Resolve a shadow sink for `tenant_id`. Production wiring returns
    /// the `RealNeonShadowSink` for the tenant's pinned region; tests
    /// return an in-memory fake.
    ///
    /// # Errors
    ///
    /// Returns a static error string when the tenant has no pinned
    /// region recorded (production) or the test fake doesn't carry one.
    fn for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str>;

    /// Wave-27 closure: resolve a shadow sink for `tenant_id` given a
    /// *pre-resolved* `region` — typically sourced from the
    /// [`RequestPrelude`] populated by the CF Worker request-prelude
    /// prefetch (wave-26). Production factories override this to skip
    /// the per-request `TenantRegionResolver::resolve_region` round-trip
    /// and dispatch straight to the per-region executor pool.
    ///
    /// The default implementation delegates back to [`Self::for_tenant`]
    /// so existing trait impls keep compiling without modification — the
    /// route layer falls back to this default only when the request was
    /// dispatched WITHOUT a `RequestPrelude` extension (dev/test boot
    /// paths or the CF Worker fetch-handler invoked the legacy path).
    ///
    /// # Errors
    ///
    /// Same error contract as [`Self::for_tenant`]: a static error
    /// string when the tenant has no pinned region recorded or the
    /// per-region executor pool is unavailable.
    fn for_tenant_in_region(
        &self,
        tenant_id: Uuid,
        _region: Region,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        self.for_tenant(tenant_id)
    }
}

/// Wave-27 closure: server-side request-prelude carrier.
///
/// Structural mirror of `corelink_clerk_cf::prod_wiring::RequestPrelude`
/// (the wasm32-only CF Worker request-prelude bundle landed in wave-26).
/// The CF Worker fetch handler populates this extension *before*
/// dispatching the axum router; downstream handler-chain code (this
/// route + future shadow-sink consumers) consumes the pre-resolved
/// region without re-issuing the wave-25 `D1TenantConfigStore` lookup.
///
/// The struct lives in this crate (not in `corelink-clerk-cf`) because
/// `corelink-clerk-cf` is wasm32-only and the analytics route is
/// native — the CF Worker boot path constructs a `RequestPrelude` from
/// its own `prod_wiring::RequestPrelude` at the handler boundary.
///
/// # Fields
///
/// - `tenant_id` — validated tenant uuid (matches `X-Tenant-Id` header).
/// - `region` — pre-resolved analytics-shadow region for `tenant_id`,
///   sourced from the CF Worker request-prelude prefetch (wave-26).
/// - `region_label` — `region.as_str()` cached so handler-chain logging
///   stays cheap (no `.to_string()` on every emit).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RequestPrelude {
    /// Validated tenant uuid.
    pub tenant_id: Uuid,
    /// Pre-resolved analytics-shadow region.
    pub region: Region,
    /// Stable region label (`region.as_str()`) for audit + logging.
    pub region_label: &'static str,
}

impl RequestPrelude {
    /// Construct a request-prelude carrier with explicit fields.
    /// `#[non_exhaustive]` blocks struct-expression construction from
    /// external crates so we expose this builder.
    #[must_use]
    pub fn new(tenant_id: Uuid, region: Region) -> Self {
        Self {
            tenant_id,
            region,
            region_label: region.as_str(),
        }
    }
}

/// Shared route state.
#[derive(Clone)]
pub struct AuditAnalyticsRouteState {
    /// Shadow-sink factory (resolves per-tenant Neon binding).
    pub shadow_factory: Arc<dyn ShadowSinkFactory>,
    /// Per-tenant rate limiter.
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Audit emit sink.
    pub audit_sink: Arc<dyn AnalyticsAuditSink>,
    /// Wave-21 — wall-clock collaborator. The rate-limit gate uses
    /// `wall_clock.now_ms()` as its bucket `now_ms` so the bucket clock
    /// advances on real time rather than the query window's `to_ms`
    /// (closes finding `B-P2-03`). Production wiring slots
    /// [`crate::wall_clock::SystemWallClock`] (the [`build_state`]
    /// default); tests inject
    /// [`crate::wall_clock::InMemoryFakeWallClock`] for deterministic
    /// rate-limit timing.
    pub wall_clock: Arc<dyn WallClock>,
}

impl core::fmt::Debug for AuditAnalyticsRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditAnalyticsRouteState")
            .finish_non_exhaustive()
    }
}

/// Canonical rate-limit config: 10 queries per tenant per minute,
/// 60s retry-after floor. Looser than the export endpoint (analytics
/// is cheaper — Postgres aggregate over the shadow vs. NDJSON walk
/// over R2).
#[must_use]
pub fn audit_analytics_rate_limit_config() -> RateLimitConfig {
    // Args: (refill_rate_per_sec, burst_capacity, retry_after_floor_secs,
    //        retry_after_hard_ceiling_secs, retry_after_canceled_tenant_secs)
    RateLimitConfig::with_overrides(
        1,
        10,
        60,
        ANALYTICS_HARD_CEILING_SECS,
        ANALYTICS_CANCELED_SECS,
    )
    .unwrap_or_else(RateLimitConfig::canonical)
}

const ANALYTICS_HARD_CEILING_SECS: u64 = 86_400;
const ANALYTICS_CANCELED_SECS: u64 = 7 * 86_400;

/// Build the axum router exposing both analytics endpoints.
pub fn router(state: AuditAnalyticsRouteState) -> Router {
    Router::new()
        .route(ROUTE_EVENT_COUNT, get(handle_event_count))
        .route(ROUTE_TIMELINE, get(handle_timeline))
        .with_state(state)
}

/// Query parameters for `/v1/audit/analytics/event-count`.
#[derive(Clone, Debug, Deserialize)]
pub struct EventCountQuery {
    /// Window lower bound (Unix epoch ms; inclusive).
    pub from: u64,
    /// Window upper bound (Unix epoch ms; exclusive).
    pub to: u64,
    /// Optional canonical CloudEvents `type` filter.
    pub event_type: Option<String>,
}

/// Query parameters for `/v1/audit/analytics/timeline`.
#[derive(Clone, Debug, Deserialize)]
pub struct TimelineQuery {
    /// Window lower bound (Unix epoch ms; inclusive).
    pub from: u64,
    /// Window upper bound (Unix epoch ms; exclusive).
    pub to: u64,
    /// Bucket width in milliseconds. Default `3_600_000` (1 h).
    pub granularity: Option<u64>,
}

/// Response body for `/v1/audit/analytics/event-count`.
#[derive(Clone, Debug, Serialize)]
pub struct EventCountResponse {
    /// One entry per distinct `event_type` seen in the window.
    pub buckets: Vec<EventCountEntry>,
}

/// One entry of the event-count response.
#[derive(Clone, Debug, Serialize)]
pub struct EventCountEntry {
    /// Canonical CloudEvents `type`.
    pub event_type: String,
    /// Row count for the bucket.
    pub count: u64,
}

/// Response body for `/v1/audit/analytics/timeline`.
#[derive(Clone, Debug, Serialize)]
pub struct TimelineResponse {
    /// Time-bucketed counts.
    pub buckets: Vec<TimelineEntry>,
    /// Echo of the granularity in ms.
    pub granularity_ms: u64,
}

/// One entry of the timeline response.
#[derive(Clone, Debug, Serialize)]
pub struct TimelineEntry {
    /// Bucket start (Unix epoch ms; inclusive).
    pub bucket_start_ms: u64,
    /// Row count in `[bucket_start, bucket_start + granularity_ms)`.
    pub count: u64,
}

impl From<TimelineBucket> for TimelineEntry {
    fn from(b: TimelineBucket) -> Self {
        Self {
            bucket_start_ms: b.bucket_start_ms,
            count: b.count,
        }
    }
}

/// Audit-row `exit_status` recorded on the fallback path when the
/// CF Worker request-prelude extension (wave-26 `RequestPrelude`) is
/// missing — the legacy `state.shadow_factory.for_tenant(tenant_id)`
/// resolver round-trip is exercised and a `tracing::warn!` line is
/// emitted so production operators can detect a CF Worker boot path
/// regression (the prelude extension SHOULD be present on every CF
/// Worker-dispatched request after wave-27).
///
/// The marker is non-terminal — the route still serves a successful
/// response on the fallback path; the audit row carries
/// `exit_status = "request_prelude_missing"` AS WELL AS the canonical
/// terminal `exit_status` (the helper emits the marker via a fresh
/// audit row before falling through to the legacy resolver).
pub const REQUEST_PRELUDE_MISSING_EXIT: &str = "request_prelude_missing";

/// Wave-27 closure: resolve the per-tenant shadow sink, preferring the
/// CF Worker request-prelude (wave-26) when present.
///
/// When the axum `Extension<RequestPrelude>` carries a pre-resolved
/// region for `tenant_id`, this helper dispatches through
/// [`ShadowSinkFactory::for_tenant_in_region`] — skipping the legacy
/// per-request `TenantRegionResolver::resolve_region` round-trip. The
/// CF Worker `prefetch_request_prelude` (wave-26
/// `corelink_clerk_cf::prod_wiring`) is the canonical populator: it
/// runs the D1 `SELECT region FROM tenant_config WHERE tenant_id = ?`
/// in the request-prelude and bundles the resolved [`Region`] into the
/// extension.
///
/// When the extension is missing (dev / test / native gRPC boot paths
/// without the CF Worker prefetch), this helper:
///
/// 1. Emits a `tracing::warn!` line so production operators can detect
///    a CF Worker boot path regression. NEVER a silent IAD fallback —
///    the WARN line is the operator-visible signal.
/// 2. Emits a `request_prelude_missing` audit row through
///    `state.audit_sink.emit` so the row survives in the analytics
///    pipeline (Logpush + dashboard widgets union on `exit_status`).
/// 3. Falls back to the legacy `state.shadow_factory.for_tenant(tenant_id)`
///    dispatch which routes through the wave-21 `TenantRegionResolver`
///    chain — the same path consumers exercised before wave-26.
///
/// The fallback path is intentionally NOT a hard failure: the analytics
/// route MUST stay functional on the native gRPC boot path (which never
/// constructs a CF Worker prelude). The WARN + audit emit is the SEV-3
/// observability hook; SEV-2 alerting is the rate of
/// `request_prelude_missing` rows in the dashboard.
///
/// # Errors
///
/// Returns a static error string when the underlying factory dispatch
/// fails — propagated unchanged so the calling handler emits the
/// canonical `backend_error` audit row + 500.
///
/// # Tenant binding
///
/// When the prelude extension's `tenant_id` does NOT match the header
/// `tenant_id`, the prelude is IGNORED and the fallback path runs. This
/// is a defense-in-depth catch for a wiring bug where the CF Worker
/// dispatches with a stale prelude attached to a different tenant's
/// request — the rejection emits the `request_prelude_missing` marker
/// (the prelude is *functionally* missing for this tenant).
fn resolve_shadow_via_prelude(
    state: &AuditAnalyticsRouteState,
    prelude: Option<&RequestPrelude>,
    tenant: Uuid,
    endpoint: &str,
    from_ms: u64,
    to_ms: u64,
) -> Result<(Arc<dyn NeonShadowSink>, &'static str), &'static str> {
    match prelude {
        Some(p) if p.tenant_id == tenant => {
            // Hot path — prelude present and bound to the same
            // tenant. Wave-29: the production `TokioPgShadowSinkFactory`
            // override skips the `TenantRegionResolver::resolve_region`
            // round-trip entirely and dispatches straight to the per-
            // region executor pool. The `region_source = "prelude"`
            // telemetry threads through to the canonical
            // `corelink.audit.analytics_query.v1` happy-path emit.
            let sink = state
                .shadow_factory
                .for_tenant_in_region(tenant, p.region)?;
            Ok((sink, REGION_SOURCE_PRELUDE))
        }
        Some(_) => {
            // Prelude attached BUT bound to a different tenant — the
            // CF Worker dispatched with a stale extension. Treat as
            // missing (emit the marker + fall back).
            tracing::warn!(
                endpoint,
                tenant = %tenant,
                "wave-27 audit-analytics: RequestPrelude tenant mismatch; falling back to factory.for_tenant — \
                 SEV-3 wiring observability"
            );
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                REQUEST_PRELUDE_MISSING_EXIT.to_string(),
            ));
            let sink = state.shadow_factory.for_tenant(tenant)?;
            Ok((sink, REGION_SOURCE_FALLBACK))
        }
        None => {
            // Prelude extension absent — dev/test/native gRPC dispatch.
            tracing::warn!(
                endpoint,
                tenant = %tenant,
                "wave-27 audit-analytics: RequestPrelude extension missing; falling back to factory.for_tenant — \
                 SEV-3 wiring observability"
            );
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                REQUEST_PRELUDE_MISSING_EXIT.to_string(),
            ));
            let sink = state.shadow_factory.for_tenant(tenant)?;
            Ok((sink, REGION_SOURCE_FALLBACK))
        }
    }
}

/// Emit an audit row and return a response. If the emit fails the
/// caller-supplied `success_resp` is dropped and a `503 Service
/// Unavailable` is returned instead — preserving the
/// `audit_emit ⇔ handler` atomicity contract (an emit failure on
/// ANY error arm — including tenant-isolation violations, bad-request,
/// backend-error, etc. — must surface as 503 so the security team's
/// SEV-2 anchor is never silently lost).
///
/// Mirrors the `audit_export` wave-20 fix-stream pattern; see
/// `specs/_audits/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md`
/// findings B-P1-02 + B-P1-03 for the discipline drift this closes.
fn emit_or_503(
    state: &AuditAnalyticsRouteState,
    row: AnalyticsAuditRow,
    success_resp: axum::response::Response,
) -> axum::response::Response {
    if state.audit_sink.emit(row).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    success_resp
}

/// Internal handler for `/event-count`.
async fn handle_event_count(
    State(state): State<AuditAnalyticsRouteState>,
    prelude: Option<Extension<RequestPrelude>>,
    headers: HeaderMap,
    Query(query): Query<EventCountQuery>,
) -> axum::response::Response {
    let tenant = match parse_tenant_header(&headers) {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if query.from >= query.to {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (StatusCode::BAD_REQUEST, "from must be < to").into_response(),
        );
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "event_count", query.from, query.to) {
        return resp;
    }
    let (shadow, region_source) = match resolve_shadow_via_prelude(
        &state,
        prelude.as_ref().map(|Extension(p)| p),
        tenant,
        "event_count",
        query.from,
        query.to,
    ) {
        Ok(pair) => pair,
        Err(msg) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "event_count".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                ),
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
            );
        }
    };
    // Verify the shadow sink is bound to this tenant (defense in
    // depth — the SQL-layer RLS is the authoritative gate, the trait
    // pin is the wiring-bug catch). Emit failure here surfaces as 503
    // per the fail-CLOSED contract — a silently-dropped tenant-isolation
    // violation row is a SEV-1 observability hole (B-P1-02 / B-P1-03).
    if shadow.tenant_id() != tenant {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
                query.from,
                query.to,
                0,
                "backend_error".to_string(),
            )
            .with_region_source(region_source),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "shadow sink tenant mismatch",
            )
                .into_response(),
        );
    }
    let buckets = match shadow.aggregate_event_count(
        query.from,
        query.to,
        query.event_type.as_deref(),
    ) {
        Ok(v) => v,
        Err(_) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "event_count".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                )
                .with_region_source(region_source),
                (StatusCode::INTERNAL_SERVER_ERROR, "shadow aggregate failed").into_response(),
            );
        }
    };
    let response = EventCountResponse {
        buckets: buckets
            .into_iter()
            .map(|b| EventCountEntry {
                event_type: b.event_type,
                count: b.count,
            })
            .collect(),
    };
    let buckets_returned = response.buckets.len() as u64;
    emit_or_503(
        &state,
        AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "event_count".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        )
        .with_region_source(region_source),
        (StatusCode::OK, axum::Json(response)).into_response(),
    )
}

/// Internal handler for `/timeline`.
async fn handle_timeline(
    State(state): State<AuditAnalyticsRouteState>,
    prelude: Option<Extension<RequestPrelude>>,
    headers: HeaderMap,
    Query(query): Query<TimelineQuery>,
) -> axum::response::Response {
    let tenant = match parse_tenant_header(&headers) {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if query.from >= query.to {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (StatusCode::BAD_REQUEST, "from must be < to").into_response(),
        );
    }
    let granularity_ms = query.granularity.unwrap_or(3_600_000);
    if granularity_ms == 0 || granularity_ms > MAX_GRANULARITY_MS {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (
                StatusCode::BAD_REQUEST,
                "granularity must be in (0, 86_400_000]",
            )
                .into_response(),
        );
    }
    let span = query.to.saturating_sub(query.from);
    let bucket_count_estimate = span.div_ceil(granularity_ms);
    // The bucket-cardinality gate is a defense against *resource
    // exhaustion* (a 7-year span at 1ms granularity would compute
    // 220-billion buckets), NOT a rate-limit bypass guard. Wave-20
    // (B-P2-04 closure): a burst of 11 requests with `to - from = 1,
    // granularity = 1` (each request returns 1 bucket, passing this
    // cardinality cap) would consume 10 rate-limit tokens immediately,
    // BUT the underlying Postgres aggregate over a 1ms span is bounded
    // O(1) and the per-tenant rate-limit gate one line down enforces
    // the throughput cap. The residual attack surface is one burst of
    // `rate_limit.capacity` cheap aggregates — bounded by design.
    if bucket_count_estimate > MAX_TIMELINE_BUCKETS {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (
                StatusCode::BAD_REQUEST,
                "(to - from) / granularity exceeds MAX_TIMELINE_BUCKETS",
            )
                .into_response(),
        );
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "timeline", query.from, query.to) {
        return resp;
    }
    let (shadow, region_source) = match resolve_shadow_via_prelude(
        &state,
        prelude.as_ref().map(|Extension(p)| p),
        tenant,
        "timeline",
        query.from,
        query.to,
    ) {
        Ok(pair) => pair,
        Err(msg) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "timeline".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                ),
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
            );
        }
    };
    if shadow.tenant_id() != tenant {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "backend_error".to_string(),
            )
            .with_region_source(region_source),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "shadow sink tenant mismatch",
            )
                .into_response(),
        );
    }
    let buckets = match shadow.aggregate_timeline(query.from, query.to, granularity_ms) {
        Ok(v) => v,
        Err(_) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "timeline".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                )
                .with_region_source(region_source),
                (StatusCode::INTERNAL_SERVER_ERROR, "shadow aggregate failed").into_response(),
            );
        }
    };
    let response = TimelineResponse {
        buckets: buckets.into_iter().map(TimelineEntry::from).collect(),
        granularity_ms,
    };
    let buckets_returned = response.buckets.len() as u64;
    emit_or_503(
        &state,
        AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "timeline".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        )
        .with_region_source(region_source),
        (StatusCode::OK, axum::Json(response)).into_response(),
    )
}

/// Extract + validate the `X-Tenant-Id` header.
///
/// Wave-20 (B-P3-03 closure): the `Err` variant carries a full
/// `axum::response::Response` (>100 bytes) which Clippy flags via
/// `result_large_err`. The waiver is deliberate — the caller chains
/// `let tenant = match parse_tenant_header(...) { Ok(t) => t, Err(r) =>
/// return r };` and the alternative (`ParseTenantError { resp: Response
/// }` newtype) would force every call site to unwrap the newtype before
/// returning. The size cost is bounded by `Response`'s stack-allocated
/// header map (one of the few large-on-stack types in axum), accepted
/// trade-off for call-site ergonomics. Tracked as cosmetic follow-on.
#[allow(clippy::result_large_err)]
fn parse_tenant_header(
    headers: &HeaderMap,
) -> Result<Uuid, axum::response::Response> {
    match headers
        .get(TENANT_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Uuid::parse_str)
    {
        Some(Ok(t)) => Ok(t),
        _ => Err(
            (StatusCode::UNAUTHORIZED, "missing or invalid X-Tenant-Id").into_response(),
        ),
    }
}

/// Common rate-limit + emit-on-deny path. Returns `Some(response)` on
/// deny; `None` if the request may proceed.
///
/// **Wave-21 closure (`B-P2-03`):** the bucket `now_ms` is now anchored
/// to [`AuditAnalyticsRouteState::wall_clock`] rather than the query
/// window's `to_ms`. The symmetric closure at
/// [`super::audit_export`] (`A-P2-05`) lands the same
/// [`crate::wall_clock::WallClock`] collaborator-swap surface — both
/// routes share the trait so production wiring stays uniform.
///
/// **Wave-24 closure (symmetric `W21-R-P2-01`):** the previous
/// implementation fell back to the query-window `to_ms` when
/// `wall_clock.now_ms() == 0`. That coupled the bucket clock to
/// caller-controlled bytes on the structurally-unreachable pre-epoch
/// branch — the exact pathology `audit_export.rs` closed at wave-23
/// (commit `5203e8b`, audit
/// `specs/_audits/2026-05-16-wave23-cleanup.md` §W21-R-P2-01). The
/// analytics route was scoped OUT of wave-23 and explicitly flagged
/// for a follow-on hygiene sweep — this is that sweep. The saturating
/// branch now fail-CLOSES with HTTP 503 + an
/// `exit_status = "clock_unavailable"` audit row, mirroring the
/// audit-export discipline. The bucket clock NEVER couples to
/// caller-controlled bytes, even on the structurally-unreachable
/// pre-epoch branch.
///
/// Production reachability: `SystemWallClock` saturates to 0 only on
/// pre-1970 wall-clock instants — structurally impossible on any
/// production host (epoch is decades past). The fail-CLOSED 503
/// affects only (a) test fakes deliberately pinned at `unix_ms == 0`
/// and (b) exotic hosts with a pre-epoch system clock, which is an
/// operational failure the audit pipeline SHOULD surface.
fn rate_limit_check(
    state: &AuditAnalyticsRouteState,
    tenant: Uuid,
    endpoint: &str,
    from_ms: u64,
    to_ms: u64,
) -> Option<axum::response::Response> {
    let bucket_key = BucketKey::per_tenant_per_endpoint(tenant, "audit.analytics");
    let wall_now_ms = state.wall_clock.now_ms();
    if wall_now_ms == 0 {
        // Fail-CLOSED: emit `clock_unavailable` audit row + 503.
        // The bucket clock MUST NEVER couple to caller-controlled
        // bytes (`to_ms`), even on the structurally-unreachable
        // pre-epoch branch. Mirrors the wave-23 closure of
        // `W21-R-P2-01` on `audit_export.rs`.
        let resp = (StatusCode::SERVICE_UNAVAILABLE, "wall clock unavailable")
            .into_response();
        return Some(emit_or_503(
            state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                "clock_unavailable".to_string(),
            ),
            resp,
        ));
    }
    let now_ms = wall_now_ms;
    match state.rate_limiter.try_acquire(tenant, bucket_key, 1, now_ms) {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => None,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                Some(emit_or_503(
                    state,
                    AnalyticsAuditRow::new(
                        EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                        Some(tenant),
                        endpoint.to_string(),
                        from_ms,
                        to_ms,
                        0,
                        "rate_limited".to_string(),
                    ),
                    resp,
                ))
            }
            // `RateLimitDecision` is `#[non_exhaustive]`; any future
            // non-Allow variant fail-CLOSED to 429.
            _ => Some(
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate-limit decision arm not handled",
                )
                    .into_response(),
            ),
        },
        Err(_) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response(),
        ),
    }
}

/// Build a native in-memory route state for tests + local dev.
///
/// Production wiring swaps `shadow_factory` for a per-region Neon
/// binding (deferred follow-on WI — needs `tokio-postgres` driver +
/// `app.current_tenant` GUC SET inside every transaction).
#[must_use]
pub fn build_state(shadow_factory: Arc<dyn ShadowSinkFactory>) -> AuditAnalyticsRouteState {
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_analytics_rate_limit_config(),
    ));
    let audit_sink: Arc<dyn AnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    let wall_clock = default_wall_clock();
    AuditAnalyticsRouteState {
        shadow_factory,
        rate_limiter,
        audit_sink,
        wall_clock,
    }
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
    use axum::body::to_bytes;
    use corelink_analytics::Region;
    use corelink_audit_chain::{
        ArchiveReceipt, EventCountBucket, InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink,
        NeonShadowError, ShadowEventRow, TimelineBucket,
    };

    /// Audit sink that always errors on `emit` — drives the
    /// `audit pipeline closed → 503` fail-CLOSED tests.
    #[derive(Debug, Default)]
    struct FailingAnalyticsAuditSink;

    impl AnalyticsAuditSink for FailingAnalyticsAuditSink {
        fn emit(&self, _row: AnalyticsAuditRow) -> Result<(), &'static str> {
            Err("injected analytics audit emit failure")
        }
    }

    /// Shadow sink that returns the bound tenant id BUT always errors
    /// on `aggregate_timeline` — drives the `handle_timeline` error
    /// arm test.
    #[derive(Debug)]
    struct AggregateTimelineFailsShadow {
        tenant_id: Uuid,
        region: Region,
    }

    impl NeonShadowSink for AggregateTimelineFailsShadow {
        fn tenant_id(&self) -> Uuid {
            self.tenant_id
        }
        fn region(&self) -> Region {
            self.region
        }
        fn sync_chunk(
            &self,
            _receipt: &ArchiveReceipt,
            _rows: &[ShadowEventRow],
            _now_ms: u64,
        ) -> Result<corelink_audit_chain::ShadowSyncReceipt, NeonShadowError> {
            Err(NeonShadowError::Backend("not used in test".to_string()))
        }
        fn aggregate_event_count(
            &self,
            _from_ms: u64,
            _to_ms: u64,
            _filter: Option<&str>,
        ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
            Err(NeonShadowError::Backend("injected".to_string()))
        }
        fn aggregate_timeline(
            &self,
            _from_ms: u64,
            _to_ms: u64,
            _granularity_ms: u64,
        ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
            Err(NeonShadowError::Backend("injected aggregate_timeline failure".to_string()))
        }
    }

    /// Build route state with a tenant-mismatch shadow sink (the
    /// factory returns a sink whose `tenant_id()` is DIFFERENT from
    /// the resolved tenant) + a `FailingAnalyticsAuditSink`. Used by
    /// `tenant_isolation_violation_returns_503_on_audit_sink_failure`.
    fn state_with_tenant_mismatch_and_failing_audit(
        requested_tenant: Uuid,
        bound_tenant: Uuid,
    ) -> AuditAnalyticsRouteState {
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let shadow: Arc<dyn NeonShadowSink> = Arc::new(InMemoryNeonShadowSink::new(
            bound_tenant,
            Region::Iad,
            audit,
        ));
        // Factory returns the wrongly-bound sink for the requested tenant.
        #[derive(Debug)]
        struct WrongBoundFactory {
            wanted: Uuid,
            shadow: Arc<dyn NeonShadowSink>,
        }
        impl ShadowSinkFactory for WrongBoundFactory {
            fn for_tenant(
                &self,
                tenant_id: Uuid,
            ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
                if tenant_id == self.wanted {
                    Ok(self.shadow.clone())
                } else {
                    Err("not bound")
                }
            }
        }
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(WrongBoundFactory {
            wanted: requested_tenant,
            shadow,
        });
        let mut state = build_state(factory);
        // Swap the audit sink for the failing one.
        state.audit_sink = Arc::new(FailingAnalyticsAuditSink);
        state
    }

    #[derive(Debug)]
    struct OneTenantFactory {
        tenant: Uuid,
        shadow: Arc<dyn NeonShadowSink>,
    }

    impl ShadowSinkFactory for OneTenantFactory {
        fn for_tenant(
            &self,
            tenant_id: Uuid,
        ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
            if tenant_id == self.tenant {
                Ok(self.shadow.clone())
            } else {
                Err("tenant not bound in test factory")
            }
        }
    }

    #[test]
    fn route_constants_match_spec() {
        assert_eq!(ROUTE_EVENT_COUNT, "/v1/audit/analytics/event-count");
        assert_eq!(ROUTE_TIMELINE, "/v1/audit/analytics/timeline");
        assert_eq!(EVENT_TYPE_ANALYTICS_QUERY, "corelink.audit.analytics_query.v1");
    }

    #[test]
    fn rate_limit_config_pins_10_per_minute() {
        let cfg = audit_analytics_rate_limit_config();
        assert_eq!(cfg.default_burst_capacity(), 10);
        assert_eq!(cfg.retry_after_floor_secs(), 60);
    }

    #[test]
    fn router_compiles_for_known_state() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let shadow: Arc<dyn NeonShadowSink> = Arc::new(InMemoryNeonShadowSink::new(
            tenant,
            Region::Iad,
            audit,
        ));
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(OneTenantFactory { tenant, shadow });
        let state = build_state(factory);
        let _router = router(state);
    }

    #[test]
    fn analytics_audit_row_builder_round_trips() {
        let t = Uuid::now_v7();
        let r = AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(t),
            "event_count".to_string(),
            10,
            20,
            3,
            "ok".to_string(),
        );
        assert_eq!(r.endpoint, "event_count");
        assert_eq!(r.authenticated_tenant, Some(t));
        assert_eq!(r.from_ms, 10);
        assert_eq!(r.to_ms, 20);
        assert_eq!(r.buckets_returned, 3);
    }

    #[test]
    fn max_timeline_buckets_pinned_to_canonical() {
        assert_eq!(MAX_TIMELINE_BUCKETS, 1_000);
        assert_eq!(MAX_GRANULARITY_MS, 24 * 60 * 60 * 1_000);
    }

    /// Wave-20 fix-stream (finding B-P1-02 + B-P1-03 closure).
    ///
    /// Drives the `handle_event_count` shadow-tenant-mismatch arm and
    /// verifies that, when the audit sink is wedged, the handler
    /// surfaces a 503 (NOT the original 500). The pre-fix code path
    /// emitted the SEV-1 audit row via `let _ = ...` and returned 500,
    /// silently dropping the security row. The `emit_or_503` helper
    /// now enforces the `audit_emit ⇔ handler` atomicity contract.
    #[tokio::test]
    async fn tenant_isolation_violation_returns_503_on_audit_sink_failure() {
        let requested = Uuid::now_v7();
        let mut bound = Uuid::now_v7();
        // Make sure bound ≠ requested (now_v7 is monotonic per-process
        // so two successive calls return distinct uuids, but pin
        // explicitly).
        while bound == requested {
            bound = Uuid::now_v7();
        }
        let state = state_with_tenant_mismatch_and_failing_audit(requested, bound);

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            requested.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };
        let resp = handle_event_count(State(state.clone()), None, headers.clone(), Query(query))
            .await
            .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "tenant-mismatch arm + failing audit sink MUST surface 503",
        );
        let body = to_bytes(resp.into_body(), 1024).await.expect("body");
        assert_eq!(&body[..], b"audit pipeline closed");

        // Same arm via the timeline handler — also surfaces 503.
        let tq = TimelineQuery {
            from: 0,
            to: 1_000,
            granularity: Some(100),
        };
        let resp2 = handle_timeline(State(state), None, headers, Query(tq))
            .await
            .into_response();
        assert_eq!(
            resp2.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "timeline tenant-mismatch arm + failing audit sink MUST surface 503",
        );
    }

    /// Wave-20 fix-stream (finding B-P1-03 closure).
    ///
    /// Drives the `handle_timeline` shadow-aggregate-error arm under
    /// a failing audit sink. The pre-fix code path did NOT emit any
    /// audit row on this arm AND swallowed any emit failure had it
    /// emitted — both modes regress the analytics-dashboard parity
    /// with `handle_event_count`. The fix now emits + surfaces 503.
    #[tokio::test]
    async fn handle_timeline_error_arm_returns_503_on_audit_sink_failure() {
        let tenant = Uuid::now_v7();
        let region = Region::Iad;
        let shadow: Arc<dyn NeonShadowSink> = Arc::new(AggregateTimelineFailsShadow {
            tenant_id: tenant,
            region,
        });
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(OneTenantFactory {
            tenant,
            shadow,
        });
        let mut state = build_state(factory);
        state.audit_sink = Arc::new(FailingAnalyticsAuditSink);

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let tq = TimelineQuery {
            from: 0,
            to: 1_000,
            granularity: Some(100),
        };
        let resp = handle_timeline(State(state), None, headers, Query(tq))
            .await
            .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "shadow aggregate_timeline failure + failing audit sink MUST surface 503",
        );
        let body = to_bytes(resp.into_body(), 1024).await.expect("body");
        assert_eq!(&body[..], b"audit pipeline closed");
    }

    /// Wave-21 closure (`B-P2-03`): the rate-limit bucket's `now_ms`
    /// is now driven by `state.wall_clock` (an `Arc<dyn WallClock>`)
    /// rather than the request's query-window `to_ms`. This test
    /// pins the contract by:
    ///   1. Injecting an [`InMemoryFakeWallClock`] pinned at a known
    ///      unix-ms instant.
    ///   2. Repeatedly issuing the SAME stationary query window
    ///      `[0, 1_000)` to exhaust the 10-token burst (rate-limit
    ///      config: burst=10, refill=1/s).
    ///   3. Asserting the 11th request denies (429) — proving the
    ///      bucket clock is NOT advancing on the stationary query
    ///      window.
    ///   4. Advancing the fake wall clock by 60s; asserting the next
    ///      request allows — proving the bucket clock IS advancing on
    ///      the injected wall-clock.
    #[tokio::test]
    async fn rate_limit_now_ms_is_driven_by_injected_wall_clock() {
        use crate::wall_clock::InMemoryFakeWallClock;

        let tenant = Uuid::now_v7();
        let shadow: Arc<dyn NeonShadowSink> = Arc::new(InMemoryNeonShadowSink::new(
            tenant,
            Region::Iad,
            Arc::new(InMemoryShadowSyncAuditSink::new()),
        ));
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(OneTenantFactory {
            tenant,
            shadow,
        });
        let mut state = build_state(factory);

        // Pin the wall clock at a known instant well past unix epoch
        // so the fallback-on-zero arm is NOT exercised.
        let fake = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        state.wall_clock = fake.clone() as Arc<dyn WallClock>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        // Stationary query window — the legacy `to_ms`-driven path
        // would let this loop forever; the wave-21 wall-clock-driven
        // path correctly exhausts after `burst=10`.
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };
        // Burst capacity = 10 per `audit_analytics_rate_limit_config`.
        // Drain the bucket — every request issues at the SAME pinned
        // wall-clock instant, so no refill happens.
        let mut allowed_count: u32 = 0;
        for _ in 0..10 {
            let resp = handle_event_count(
                State(state.clone()),
                None,
                headers.clone(),
                Query(query.clone()),
            )
            .await
            .into_response();
            if resp.status() == StatusCode::OK {
                allowed_count = allowed_count.saturating_add(1);
            }
        }
        assert_eq!(
            allowed_count, 10,
            "10 OKs expected within the burst window (wall clock pinned)",
        );

        // 11th request — bucket empty, wall clock still pinned → 429.
        let resp_denied = handle_event_count(
            State(state.clone()),
            None,
            headers.clone(),
            Query(query.clone()),
        )
        .await
        .into_response();
        assert_eq!(
            resp_denied.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "11th request MUST 429 — wall clock pinned so bucket cannot refill",
        );

        // Advance the wall clock by 60s — the bucket should now have
        // refilled (refill=1 token/s × 60s = 60 tokens, clamped to
        // burst=10). The next request MUST allow.
        fake.advance(std::time::Duration::from_secs(60));
        let resp_after_advance = handle_event_count(
            State(state),
            None,
            headers,
            Query(query),
        )
        .await
        .into_response();
        assert_eq!(
            resp_after_advance.status(),
            StatusCode::OK,
            "after advancing the fake wall clock 60s, the next request MUST allow \
             (proves wall-clock-driven `now_ms`, finding B-P2-03 closure)",
        );
    }

    /// Wave-24 closure of the symmetric `W21-R-P2-01` —
    /// `wall_clock.now_ms() == 0` on the analytics route MUST fail-CLOSED
    /// (503 + `clock_unavailable` audit row) rather than fall back to the
    /// caller-controlled query-window `to_ms`. Mirrors the wave-23
    /// `audit_export.rs` test
    /// `wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row`.
    ///
    /// The wave-23 cleanup explicitly scoped the analytics symmetric path
    /// OUT of `W21-R-P2-01` (see commit `5203e8b` +
    /// `specs/_audits/2026-05-16-wave23-cleanup.md` §1.1 CAVEAT). This is
    /// the follow-on hygiene sweep that closes the asymmetry: the bucket
    /// clock NEVER couples to caller-controlled bytes on EITHER route.
    #[tokio::test]
    async fn analytics_wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row() {
        use crate::wall_clock::InMemoryFakeWallClock;

        let tenant = Uuid::now_v7();
        let shadow: Arc<dyn NeonShadowSink> = Arc::new(InMemoryNeonShadowSink::new(
            tenant,
            Region::Iad,
            Arc::new(InMemoryShadowSyncAuditSink::new()),
        ));
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(OneTenantFactory {
            tenant,
            shadow,
        });
        let mut state = build_state(factory);

        // Capture-sink swap so we can snapshot the emitted row.
        let capture: Arc<InMemoryAnalyticsAuditSink> =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

        // Pin the wall clock at the saturating value (unix_ms == 0).
        // SystemWallClock cannot reach this branch in production
        // (epoch is decades past), but InMemoryFakeWallClock can —
        // simulating a poisoned-mutex or pre-epoch host.
        let fake = Arc::new(InMemoryFakeWallClock::at_unix_ms(0));
        state.wall_clock = fake as Arc<dyn WallClock>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };

        let resp = handle_event_count(
            State(state.clone()),
            None,
            headers.clone(),
            Query(query),
        )
        .await
        .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "analytics wall-clock saturating to 0 MUST fail-CLOSED with 503 \
             (wave-24 symmetric W21-R-P2-01 closure — no fallback to to_ms-derived clock)",
        );
        let body = to_bytes(resp.into_body(), 1024).await.expect("body");
        assert_eq!(
            &body[..],
            b"wall clock unavailable",
            "503 body MUST be the canonical `wall clock unavailable` marker",
        );

        let rows = capture.snapshot().expect("snapshot");
        assert!(
            rows.iter().any(|r| r.exit_status == "clock_unavailable"),
            "fail-CLOSED path MUST emit one `clock_unavailable` audit row; saw {:?}",
            rows.iter().map(|r| &r.exit_status).collect::<Vec<_>>(),
        );
        // Sanity — the emitted row carries the request tenant + endpoint,
        // matching the analytics audit-row schema.
        let row = rows
            .iter()
            .find(|r| r.exit_status == "clock_unavailable")
            .expect("at least one clock_unavailable row");
        assert_eq!(row.authenticated_tenant, Some(tenant));
        assert_eq!(row.endpoint, "event_count");
        assert_eq!(row.from_ms, 0);
        assert_eq!(row.to_ms, 1_000);
        assert_eq!(row.buckets_returned, 0);
        assert_eq!(row.event_type, EVENT_TYPE_ANALYTICS_QUERY);
    }

    // ------------------------------------------------------------------
    // Wave-27: RequestPrelude extension consumer adoption tests.
    //
    // These pin the wave-27 closure of the wave-26 caveat:
    // "ShadowSinkFactory consumer call sites are not yet shadow-aware".
    // After wave-27 the analytics handlers prefer the pre-resolved
    // `RequestPrelude.region` (sourced from
    // `corelink_clerk_cf::prod_wiring::prefetch_request_prelude`) over
    // re-resolving via `state.shadow_factory.for_tenant(tenant_id)` and
    // fall back to the legacy path with a WARN + audit emit ONLY when
    // the extension is missing.
    // ------------------------------------------------------------------

    /// Recording factory that captures which method was invoked
    /// (`for_tenant` legacy path vs. `for_tenant_in_region` wave-27
    /// shadow-aware path) so the tests can assert the consumer adoption
    /// without relying on observable side effects on the shadow sink.
    #[derive(Debug)]
    struct RecordingShadowFactory {
        tenant: Uuid,
        region: Region,
        audit: Arc<InMemoryShadowSyncAuditSink>,
        for_tenant_calls: Arc<Mutex<u32>>,
        for_tenant_in_region_calls: Arc<Mutex<(u32, Option<Region>)>>,
    }

    impl RecordingShadowFactory {
        fn new(tenant: Uuid, region: Region) -> Self {
            Self {
                tenant,
                region,
                audit: Arc::new(InMemoryShadowSyncAuditSink::new()),
                for_tenant_calls: Arc::new(Mutex::new(0)),
                for_tenant_in_region_calls: Arc::new(Mutex::new((0, None))),
            }
        }
    }

    impl ShadowSinkFactory for RecordingShadowFactory {
        fn for_tenant(
            &self,
            tenant_id: Uuid,
        ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
            *self.for_tenant_calls.lock().expect("legacy counter") += 1;
            if tenant_id == self.tenant {
                Ok(Arc::new(InMemoryNeonShadowSink::new(
                    self.tenant,
                    self.region,
                    self.audit.clone(),
                )))
            } else {
                Err("tenant not bound in recording factory")
            }
        }

        fn for_tenant_in_region(
            &self,
            tenant_id: Uuid,
            region: Region,
        ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
            let mut g = self
                .for_tenant_in_region_calls
                .lock()
                .expect("shadow-aware counter");
            g.0 += 1;
            g.1 = Some(region);
            drop(g);
            if tenant_id == self.tenant {
                Ok(Arc::new(InMemoryNeonShadowSink::new(
                    self.tenant,
                    region,
                    self.audit.clone(),
                )))
            } else {
                Err("tenant not bound in recording factory")
            }
        }
    }

    /// Wave-27 closure pin: when a `RequestPrelude` extension is attached
    /// to the request, the handler MUST dispatch through
    /// `for_tenant_in_region(tenant, prelude.region)` — skipping the
    /// per-request `TenantRegionResolver::resolve_region` round-trip the
    /// legacy `for_tenant` path runs internally.
    #[tokio::test]
    async fn request_prelude_consumed_dispatches_through_for_tenant_in_region() {
        let tenant = Uuid::now_v7();
        let recording = Arc::new(RecordingShadowFactory::new(tenant, Region::Fra));
        let factory: Arc<dyn ShadowSinkFactory> = recording.clone();
        let state = build_state(factory);

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };
        let prelude = RequestPrelude::new(tenant, Region::Fra);

        let resp = handle_event_count(
            State(state),
            Some(Extension(prelude)),
            headers,
            Query(query),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK, "happy path expected");

        let legacy = *recording.for_tenant_calls.lock().expect("legacy counter");
        let shadow_aware = *recording
            .for_tenant_in_region_calls
            .lock()
            .expect("shadow-aware counter");
        assert_eq!(
            legacy, 0,
            "wave-27: prelude-present requests MUST NOT touch the legacy for_tenant path"
        );
        assert_eq!(
            shadow_aware.0, 1,
            "wave-27: prelude-present requests MUST dispatch through for_tenant_in_region exactly once"
        );
        assert_eq!(
            shadow_aware.1,
            Some(Region::Fra),
            "wave-27: for_tenant_in_region MUST receive the prelude's pre-resolved region"
        );
    }

    /// Wave-27 fallback path pin: when the `RequestPrelude` extension is
    /// MISSING (dev / test / native gRPC dispatch), the handler MUST fall
    /// back to `for_tenant`, emit a `request_prelude_missing` audit row,
    /// AND still serve a successful response. NOT silent IAD — the
    /// audit row is the operator-visible signal.
    #[tokio::test]
    async fn request_prelude_missing_falls_back_with_warn_and_audit() {
        let tenant = Uuid::now_v7();
        let recording = Arc::new(RecordingShadowFactory::new(tenant, Region::Iad));
        let factory: Arc<dyn ShadowSinkFactory> = recording.clone();
        let mut state = build_state(factory);

        // Capture-sink swap so we can snapshot the emitted rows.
        let capture: Arc<InMemoryAnalyticsAuditSink> =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };

        let resp = handle_event_count(State(state), None, headers, Query(query))
            .await
            .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "wave-27: fallback path MUST stay functional (the prelude extension is OPTIONAL)"
        );

        let legacy = *recording.for_tenant_calls.lock().expect("legacy counter");
        let shadow_aware = *recording
            .for_tenant_in_region_calls
            .lock()
            .expect("shadow-aware counter");
        assert_eq!(
            legacy, 1,
            "wave-27: prelude-missing requests MUST fall back to the legacy for_tenant exactly once"
        );
        assert_eq!(
            shadow_aware.0, 0,
            "wave-27: prelude-missing requests MUST NOT touch the shadow-aware path"
        );

        let rows = capture.snapshot().expect("snapshot");
        let missing_rows: Vec<&AnalyticsAuditRow> = rows
            .iter()
            .filter(|r| r.exit_status == REQUEST_PRELUDE_MISSING_EXIT)
            .collect();
        assert_eq!(
            missing_rows.len(),
            1,
            "wave-27: fallback path MUST emit exactly one `request_prelude_missing` audit row; \
             saw {} rows total",
            rows.len()
        );
        let row = missing_rows[0];
        assert_eq!(row.authenticated_tenant, Some(tenant));
        assert_eq!(row.endpoint, "event_count");
        assert_eq!(row.from_ms, 0);
        assert_eq!(row.to_ms, 1_000);
        assert_eq!(row.event_type, EVENT_TYPE_ANALYTICS_QUERY);
    }

    /// Wave-27 defense-in-depth pin: when a `RequestPrelude` extension
    /// is attached BUT bound to a DIFFERENT tenant than the
    /// `X-Tenant-Id` header (a wiring bug where the CF Worker
    /// dispatched with a stale prelude), the handler MUST IGNORE the
    /// stale prelude and fall back through the legacy path with the
    /// `request_prelude_missing` marker emitted (the prelude is
    /// *functionally* missing for the current tenant).
    ///
    /// Exercises the symmetric `handle_timeline` arm so the fallback
    /// emit + WARN is pinned for both routes.
    #[tokio::test]
    async fn request_prelude_missing_emit_pinned_for_timeline_route() {
        let tenant = Uuid::now_v7();
        let mut stale_tenant = Uuid::now_v7();
        while stale_tenant == tenant {
            stale_tenant = Uuid::now_v7();
        }
        let recording = Arc::new(RecordingShadowFactory::new(tenant, Region::Iad));
        let factory: Arc<dyn ShadowSinkFactory> = recording.clone();
        let mut state = build_state(factory);

        let capture: Arc<InMemoryAnalyticsAuditSink> =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let tq = TimelineQuery {
            from: 0,
            to: 1_000,
            granularity: Some(100),
        };
        // Stale prelude — attached but bound to a DIFFERENT tenant.
        let stale_prelude = RequestPrelude::new(stale_tenant, Region::Fra);

        let resp = handle_timeline(
            State(state),
            Some(Extension(stale_prelude)),
            headers,
            Query(tq),
        )
        .await
        .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "wave-27: stale-prelude requests fall back to factory.for_tenant and still serve a response"
        );

        let legacy = *recording.for_tenant_calls.lock().expect("legacy counter");
        let shadow_aware = *recording
            .for_tenant_in_region_calls
            .lock()
            .expect("shadow-aware counter");
        assert_eq!(
            legacy, 1,
            "wave-27: stale-prelude MUST trigger the legacy fallback"
        );
        assert_eq!(
            shadow_aware.0, 0,
            "wave-27: stale-prelude MUST NOT dispatch through the shadow-aware path"
        );

        let rows = capture.snapshot().expect("snapshot");
        assert!(
            rows.iter().any(|r| {
                r.exit_status == REQUEST_PRELUDE_MISSING_EXIT && r.endpoint == "timeline"
            }),
            "wave-27: timeline route MUST emit `request_prelude_missing` on stale-prelude fallback; \
             saw {:?}",
            rows.iter()
                .map(|r| (&r.endpoint, &r.exit_status))
                .collect::<Vec<_>>()
        );
    }

    // ------------------------------------------------------------------
    // Wave-29: ShadowSinkFactory full-adoption tests.
    //
    // Closes the wave-27 caveat #2 ("Production `TokioPgShadowSinkFactory`
    // override — wave-21 impl continues to use the default
    // `for_tenant_in_region → for_tenant` delegate"). After wave-29:
    //
    // 1. The production factory's override (in
    //    `corelink_server::neon_shadow_factory`) skips the
    //    `TenantRegionResolver` round-trip entirely.
    // 2. The route's `corelink.audit.analytics_query.v1` emit threads
    //    the `region_source` telemetry field (`"prelude"` vs
    //    `"fallback"`) so operators can observe which dispatch path
    //    each request took.
    // ------------------------------------------------------------------

    /// Wave-29 closure pin: with the wave-26 `RequestPrelude`
    /// attached, the canonical `corelink.audit.analytics_query.v1`
    /// emit (success path) MUST carry `region_source = "prelude"` AND
    /// the recording factory MUST observe ZERO calls to the legacy
    /// `for_tenant` arm — i.e. the prelude region propagated end-to-
    /// end and no D1 round-trip was paid.
    #[tokio::test]
    async fn audit_analytics_consumes_prelude_region_without_extra_d1_round_trip() {
        let tenant = Uuid::now_v7();
        let recording = Arc::new(RecordingShadowFactory::new(tenant, Region::Fra));
        let factory: Arc<dyn ShadowSinkFactory> = recording.clone();
        let mut state = build_state(factory);

        // Capture-sink swap so we can introspect emitted rows.
        let capture: Arc<InMemoryAnalyticsAuditSink> =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };
        let prelude = RequestPrelude::new(tenant, Region::Fra);

        let resp = handle_event_count(
            State(state),
            Some(Extension(prelude)),
            headers,
            Query(query),
        )
        .await
        .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "wave-29 happy path: prelude-aware dispatch must serve 200"
        );

        // Wave-29: the legacy resolver round-trip is gone.
        let legacy = *recording.for_tenant_calls.lock().expect("legacy counter");
        assert_eq!(
            legacy, 0,
            "wave-29: prelude-attached requests MUST NOT touch for_tenant — no D1 round-trip"
        );
        let shadow_aware = *recording
            .for_tenant_in_region_calls
            .lock()
            .expect("shadow-aware counter");
        assert_eq!(
            shadow_aware.0, 1,
            "wave-29: prelude-attached requests MUST dispatch through for_tenant_in_region once"
        );
        assert_eq!(
            shadow_aware.1,
            Some(Region::Fra),
            "wave-29: the prelude region MUST be the value handed to for_tenant_in_region"
        );

        // Wave-29 telemetry: the canonical success emit carries
        // `region_source = "prelude"`. There should be exactly one
        // `ok` emit (no `request_prelude_missing` row since the
        // prelude was present and matched the tenant).
        let rows = capture.snapshot().expect("snapshot");
        let ok_rows: Vec<&AnalyticsAuditRow> = rows
            .iter()
            .filter(|r| r.exit_status == "ok")
            .collect();
        assert_eq!(
            ok_rows.len(),
            1,
            "wave-29: exactly one ok emit expected; saw {:?}",
            rows.iter()
                .map(|r| (&r.endpoint, &r.exit_status, &r.region_source))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            ok_rows[0].region_source.as_deref(),
            Some(REGION_SOURCE_PRELUDE),
            "wave-29: prelude hot path MUST tag region_source = \"prelude\""
        );
        let missing_rows: Vec<&AnalyticsAuditRow> = rows
            .iter()
            .filter(|r| r.exit_status == REQUEST_PRELUDE_MISSING_EXIT)
            .collect();
        assert!(
            missing_rows.is_empty(),
            "wave-29: prelude-attached requests MUST NOT emit request_prelude_missing"
        );
    }

    /// Wave-29 symmetric pin: the fallback path (no prelude attached)
    /// MUST tag the canonical success emit with `region_source =
    /// "fallback"` so dashboard widgets can split the rate of
    /// "prelude vs. fallback" dispatches — closing the wave-27
    /// observability caveat #3 (dashboard widget for the marker).
    #[tokio::test]
    async fn audit_analytics_fallback_path_tags_region_source_fallback() {
        let tenant = Uuid::now_v7();
        let recording = Arc::new(RecordingShadowFactory::new(tenant, Region::Iad));
        let factory: Arc<dyn ShadowSinkFactory> = recording.clone();
        let mut state = build_state(factory);

        let capture: Arc<InMemoryAnalyticsAuditSink> =
            Arc::new(InMemoryAnalyticsAuditSink::new());
        state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

        let mut headers = HeaderMap::new();
        headers.insert(
            TENANT_ID_HEADER,
            tenant.to_string().parse().expect("header parse"),
        );
        let query = EventCountQuery {
            from: 0,
            to: 1_000,
            event_type: None,
        };

        let resp = handle_event_count(State(state), None, headers, Query(query))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let rows = capture.snapshot().expect("snapshot");
        let ok_rows: Vec<&AnalyticsAuditRow> = rows
            .iter()
            .filter(|r| r.exit_status == "ok")
            .collect();
        assert_eq!(ok_rows.len(), 1);
        assert_eq!(
            ok_rows[0].region_source.as_deref(),
            Some(REGION_SOURCE_FALLBACK),
            "wave-29: fallback path MUST tag region_source = \"fallback\""
        );
    }
}
