//! Wave-20 / wave-21 / wave-24 handler-level emit-discipline tests.
//!
//! Pins:
//! - B-P1-02 / B-P1-03 — tenant-isolation violation MUST fail-CLOSED 503.
//! - handle_timeline shadow-aggregate-error MUST emit + 503.
//! - B-P2-03 — rate-limit `now_ms` driven by injected `WallClock`.
//! - Symmetric W21-R-P2-01 — saturating wall clock MUST fail-CLOSED.
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
    body::to_bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use corelink_analytics::Region;
use corelink_audit_chain::{InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink, NeonShadowSink};
use uuid::Uuid;

use crate::wall_clock::WallClock;

use super::audit_sink::{AnalyticsAuditSink, InMemoryAnalyticsAuditSink};
use super::handler_event_count::handle_event_count;
use super::handler_timeline::handle_timeline;
use super::shadow_factory::ShadowSinkFactory;
use super::state::build_state;
use super::tests_common::{
    AggregateTimelineFailsShadow, FailingAnalyticsAuditSink, OneTenantFactory,
    state_with_tenant_mismatch_and_failing_audit,
};
use super::types::{
    EventCountQuery, TimelineQuery, EVENT_TYPE_ANALYTICS_QUERY, TENANT_ID_HEADER,
};

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
/// `specs/_audits/sealed/2026-05-16-wave23-cleanup.md` §1.1 CAVEAT). This is
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
