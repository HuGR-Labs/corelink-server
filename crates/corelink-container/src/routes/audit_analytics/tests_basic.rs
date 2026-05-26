//! Basic unit tests for the audit-analytics routes: route constants,
//! rate-limit config, router-builder, audit-row builder, and bucket-
//! cardinality constants.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Test bodies are verbatim copies of the original inline `mod tests`
//! block.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::{InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink, NeonShadowSink};
use uuid::Uuid;

use super::shadow_factory::ShadowSinkFactory;
use super::state::{audit_analytics_rate_limit_config, build_state, router};
use super::tests_common::OneTenantFactory;
use super::types::{
    AnalyticsAuditRow, EVENT_TYPE_ANALYTICS_QUERY, MAX_GRANULARITY_MS, MAX_TIMELINE_BUCKETS,
    ROUTE_EVENT_COUNT, ROUTE_TIMELINE,
};

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
