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
//!
//! # Module layout (wave-33 stage 2.PRE-B.2.c decomposition)
//!
//! The monolithic `audit_analytics.rs` was decomposed into the
//! per-responsibility submodules below. Every pre-split public
//! symbol is re-exported here verbatim so external consumers
//! (`routes.rs`, integration tests, the production
//! `neon_shadow_factory` wiring) see the SAME
//! `crate::routes::audit_analytics::*` surface.
//!
//! - [`types`] — constants + [`AnalyticsAuditRow`] +
//!   [`RequestPrelude`] + query / response types.
//! - [`audit_sink`] — [`AnalyticsAuditSink`] trait +
//!   [`InMemoryAnalyticsAuditSink`] capture fake + fail-CLOSED
//!   emit helper.
//! - [`shadow_factory`] — [`ShadowSinkFactory`] trait + the
//!   `resolve_shadow_via_prelude` wave-27 helper.
//! - [`state`] — [`AuditAnalyticsRouteState`] + [`build_state`] +
//!   [`audit_analytics_rate_limit_config`] + [`router`].
//! - [`rate_limit`] — shared rate-limit gate + tenant-header parser.
//! - [`handler_event_count`] — axum handler for
//!   `/v1/audit/analytics/event-count`.
//! - [`handler_timeline`] — axum handler for
//!   `/v1/audit/analytics/timeline`.

#![forbid(unsafe_code)]
// W35-P2: module-level `//!` docs reference items absorbed from a
// former sibling crate via short paths; under the umbrella crate's
// scope they would require full prefixes to keep working. Suppressing
// the lint preserves the original reference text without churn.
#![allow(rustdoc::broken_intra_doc_links)]

pub mod audit_sink;
pub mod d1_sink;
pub mod handler_event_count;
pub mod handler_timeline;
pub mod rate_limit;
pub mod shadow_factory;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_common;
#[cfg(test)]
mod tests_handlers;
#[cfg(test)]
mod tests_prelude;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split
// `crate::routes::audit_analytics::*` public surface verbatim.
// ---------------------------------------------------------------------------

pub use audit_sink::{AnalyticsAuditSink, InMemoryAnalyticsAuditSink};
pub use d1_sink::D1ShadowSinkFactory;
pub use shadow_factory::ShadowSinkFactory;
pub use state::{audit_analytics_rate_limit_config, build_state, router, AuditAnalyticsRouteState};
pub use types::{
    AnalyticsAuditRow, EventCountEntry, EventCountQuery, EventCountResponse, RequestPrelude,
    TimelineEntry, TimelineQuery, TimelineResponse, EVENT_TYPE_ANALYTICS_QUERY, MAX_GRANULARITY_MS,
    MAX_TIMELINE_BUCKETS, REGION_SOURCE_FALLBACK, REGION_SOURCE_PRELUDE,
    REQUEST_PRELUDE_MISSING_EXIT, ROUTE_EVENT_COUNT, ROUTE_TIMELINE, TENANT_ID_HEADER,
};

/// Run the native PAT possession gate (rt-nuclear #17) when it is wired on the
/// analytics route state. Reads the bearer PAT from the `Authorization` header
/// and re-verifies it (Argon2id, full Option-B pipeline) against the claimed
/// `tenant`. `Some(resp)` ⇒ REJECT (401 forged/wrong-tenant / 503 verifier
/// fault); `None` ⇒ proceed (or when the gate is absent in dev/CI). Called at the
/// TOP of each analytics handler, AFTER the scope+tenant gate, BEFORE any data
/// access. Mirrors `cas::pat_gate_reject`.
pub(super) async fn pat_gate_reject(
    state: &AuditAnalyticsRouteState,
    tenant: &str,
    headers: &axum::http::HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}
