//! Constants + audit-row data shape + request-prelude carrier + query
//! / response types for the `/v1/audit/analytics/*` routes.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move; no behavioural change.

#![forbid(unsafe_code)]

use corelink_analytics::Region;
use corelink_audit_chain::TimelineBucket;
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
