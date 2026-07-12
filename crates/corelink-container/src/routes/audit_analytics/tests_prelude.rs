//! Wave-27 + wave-29 `RequestPrelude` consumer-adoption tests.
//!
//! Pins the wave-27 closure of the wave-26 caveat:
//! "ShadowSinkFactory consumer call sites are not yet shadow-aware".
//! After wave-27 the analytics handlers prefer the pre-resolved
//! `RequestPrelude.region` (sourced from
//! `corelink_clerk_cf::prod_wiring::prefetch_request_prelude`) over
//! re-resolving via `state.shadow_factory.for_tenant(tenant_id)` and
//! fall back to the legacy path with a WARN + audit emit ONLY when
//! the extension is missing.
//!
//! Wave-29 telemetry: the canonical `corelink.audit.analytics_query.v1`
//! emit threads the `region_source` field (`"prelude"` vs
//! `"fallback"`) so operators can observe which dispatch path each
//! request took.
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

use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use corelink_analytics::Region;
use uuid::Uuid;

use super::audit_sink::{AnalyticsAuditSink, InMemoryAnalyticsAuditSink};
use super::handler_event_count::handle_event_count;
use super::handler_timeline::handle_timeline;
use super::shadow_factory::ShadowSinkFactory;
use super::state::build_state;
use super::tests_common::RecordingShadowFactory;
use super::types::{
    AnalyticsAuditRow, EventCountQuery, RequestPrelude, TimelineQuery, EVENT_TYPE_ANALYTICS_QUERY,
    REGION_SOURCE_FALLBACK, REGION_SOURCE_PRELUDE, REQUEST_PRELUDE_MISSING_EXIT,
};
use crate::auth_tenant::AuthTenant;

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

    let auth = AuthTenant(tenant.to_string());
    let query = EventCountQuery {
        from: 0,
        to: 1_000,
        event_type: None,
    };
    let prelude = RequestPrelude::new(tenant, Region::Fra);

    let resp = handle_event_count(
        State(state),
        auth,
        Some(Extension(prelude)),
        Query(query),
        axum::http::HeaderMap::new(),
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
    let capture: Arc<InMemoryAnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

    let auth = AuthTenant(tenant.to_string());
    let query = EventCountQuery {
        from: 0,
        to: 1_000,
        event_type: None,
    };

    let resp = handle_event_count(
        State(state),
        auth,
        None,
        Query(query),
        axum::http::HeaderMap::new(),
    )
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
/// is attached BUT bound to a DIFFERENT tenant than the authenticated
/// tenant (the `AuthTenant` / `x-corelink-tenant-id` binding — a
/// wiring bug where the CF Worker
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

    let capture: Arc<InMemoryAnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

    let auth = AuthTenant(tenant.to_string());
    let tq = TimelineQuery {
        from: 0,
        to: 1_000,
        granularity: Some(100),
    };
    // Stale prelude — attached but bound to a DIFFERENT tenant.
    let stale_prelude = RequestPrelude::new(stale_tenant, Region::Fra);

    let resp = handle_timeline(
        State(state),
        auth,
        Some(Extension(stale_prelude)),
        Query(tq),
        axum::http::HeaderMap::new(),
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
        rows.iter()
            .any(|r| { r.exit_status == REQUEST_PRELUDE_MISSING_EXIT && r.endpoint == "timeline" }),
        "wave-27: timeline route MUST emit `request_prelude_missing` on stale-prelude fallback; \
         saw {:?}",
        rows.iter()
            .map(|r| (&r.endpoint, &r.exit_status))
            .collect::<Vec<_>>()
    );
}

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
    let capture: Arc<InMemoryAnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

    let auth = AuthTenant(tenant.to_string());
    let query = EventCountQuery {
        from: 0,
        to: 1_000,
        event_type: None,
    };
    let prelude = RequestPrelude::new(tenant, Region::Fra);

    let resp = handle_event_count(
        State(state),
        auth,
        Some(Extension(prelude)),
        Query(query),
        axum::http::HeaderMap::new(),
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
    let ok_rows: Vec<&AnalyticsAuditRow> = rows.iter().filter(|r| r.exit_status == "ok").collect();
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

    let capture: Arc<InMemoryAnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    state.audit_sink = capture.clone() as Arc<dyn AnalyticsAuditSink>;

    let auth = AuthTenant(tenant.to_string());
    let query = EventCountQuery {
        from: 0,
        to: 1_000,
        event_type: None,
    };

    let resp = handle_event_count(
        State(state),
        auth,
        None,
        Query(query),
        axum::http::HeaderMap::new(),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::OK);

    let rows = capture.snapshot().expect("snapshot");
    let ok_rows: Vec<&AnalyticsAuditRow> = rows.iter().filter(|r| r.exit_status == "ok").collect();
    assert_eq!(ok_rows.len(), 1);
    assert_eq!(
        ok_rows[0].region_source.as_deref(),
        Some(REGION_SOURCE_FALLBACK),
        "wave-29: fallback path MUST tag region_source = \"fallback\""
    );
}
