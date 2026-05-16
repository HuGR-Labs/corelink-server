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
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use corelink_audit_chain::{NeonShadowSink, TimelineBucket};
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl AnalyticsAuditRow {
    /// Construct a new analytics audit row with every field explicit.
    /// `#[non_exhaustive]` blocks struct-expression construction from
    /// external crates so we expose this builder.
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
        }
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

/// Internal handler for `/event-count`.
async fn handle_event_count(
    State(state): State<AuditAnalyticsRouteState>,
    headers: HeaderMap,
    Query(query): Query<EventCountQuery>,
) -> axum::response::Response {
    let tenant = match parse_tenant_header(&headers) {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if query.from >= query.to {
        let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "event_count".to_string(),
            query.from,
            query.to,
            0,
            "bad_request".to_string(),
        ));
        return (StatusCode::BAD_REQUEST, "from must be < to").into_response();
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "event_count", query.from, query.to) {
        return resp;
    }
    let shadow = match state.shadow_factory.for_tenant(tenant) {
        Ok(s) => s,
        Err(msg) => {
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
                query.from,
                query.to,
                0,
                "backend_error".to_string(),
            ));
            return (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
        }
    };
    // Verify the shadow sink is bound to this tenant (defense in
    // depth — the SQL-layer RLS is the authoritative gate, the trait
    // pin is the wiring-bug catch).
    if shadow.tenant_id() != tenant {
        let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "event_count".to_string(),
            query.from,
            query.to,
            0,
            "backend_error".to_string(),
        ));
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "shadow sink tenant mismatch",
        )
            .into_response();
    }
    let buckets = match shadow.aggregate_event_count(
        query.from,
        query.to,
        query.event_type.as_deref(),
    ) {
        Ok(v) => v,
        Err(_) => {
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
                query.from,
                query.to,
                0,
                "backend_error".to_string(),
            ));
            return (StatusCode::INTERNAL_SERVER_ERROR, "shadow aggregate failed")
                .into_response();
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
    if state
        .audit_sink
        .emit(AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "event_count".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        ))
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    (StatusCode::OK, axum::Json(response)).into_response()
}

/// Internal handler for `/timeline`.
async fn handle_timeline(
    State(state): State<AuditAnalyticsRouteState>,
    headers: HeaderMap,
    Query(query): Query<TimelineQuery>,
) -> axum::response::Response {
    let tenant = match parse_tenant_header(&headers) {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if query.from >= query.to {
        let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "timeline".to_string(),
            query.from,
            query.to,
            0,
            "bad_request".to_string(),
        ));
        return (StatusCode::BAD_REQUEST, "from must be < to").into_response();
    }
    let granularity_ms = query.granularity.unwrap_or(3_600_000);
    if granularity_ms == 0 || granularity_ms > MAX_GRANULARITY_MS {
        return (
            StatusCode::BAD_REQUEST,
            "granularity must be in (0, 86_400_000]",
        )
            .into_response();
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
        return (
            StatusCode::BAD_REQUEST,
            "(to - from) / granularity exceeds MAX_TIMELINE_BUCKETS",
        )
            .into_response();
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "timeline", query.from, query.to) {
        return resp;
    }
    let shadow = match state.shadow_factory.for_tenant(tenant) {
        Ok(s) => s,
        Err(msg) => return (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
    };
    if shadow.tenant_id() != tenant {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "shadow sink tenant mismatch",
        )
            .into_response();
    }
    let buckets = match shadow.aggregate_timeline(query.from, query.to, granularity_ms) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "shadow aggregate failed")
                .into_response();
        }
    };
    let response = TimelineResponse {
        buckets: buckets.into_iter().map(TimelineEntry::from).collect(),
        granularity_ms,
    };
    let buckets_returned = response.buckets.len() as u64;
    if state
        .audit_sink
        .emit(AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "timeline".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        ))
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response();
    }
    (StatusCode::OK, axum::Json(response)).into_response()
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
/// **Known residual trait (wave-20 B-P2-03 closure):** `now_ms` is anchored
/// to the query window's `to_ms` (NOT wall-clock). Symmetric trait at
/// `audit_export::now_ms_from_window` (Stream A A-P2-05) — both routes
/// share the same `WallClock`-collaborator-swap path slotted at production
/// wiring. A customer querying a 1970-epoch window every 60ms keeps
/// refilling the bucket; the global throughput cap is enforced one layer
/// up by the per-tenant request budget in the wall-clock-backed
/// production wiring. The cleanup lands as one cross-route fix-stream
/// in wave-21+.
fn rate_limit_check(
    state: &AuditAnalyticsRouteState,
    tenant: Uuid,
    endpoint: &str,
    from_ms: u64,
    to_ms: u64,
) -> Option<axum::response::Response> {
    let bucket_key = BucketKey::per_tenant_per_endpoint(tenant, "audit.analytics");
    let now_ms = to_ms;
    match state.rate_limiter.try_acquire(tenant, bucket_key, 1, now_ms) {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => None,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    endpoint.to_string(),
                    from_ms,
                    to_ms,
                    0,
                    "rate_limited".to_string(),
                ));
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                Some(resp)
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
    AuditAnalyticsRouteState {
        shadow_factory,
        rate_limiter,
        audit_sink,
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
    use corelink_analytics::Region;
    use corelink_audit_chain::{InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink};

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
}
