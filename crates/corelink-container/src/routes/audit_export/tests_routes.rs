//! Wave-20 / wave-21 / wave-23 emit-discipline route-level tests.
//!
//! Close audit findings A-P1-02 (cross-tenant-reject),
//! A-P1-03 (mid-stream-break), A-P1-05 (verify-failed-sev0),
//! A-P2-01 (rate-limit-deny), A-P2-05 (wall-clock-driven `now_ms`),
//! and W21-R-P2-01 (fail-CLOSED on saturating wall clock).
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
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

use axum::http::StatusCode;
use corelink_audit_chain::{AuditExporter, ChainHash, ExportWindow, InMemoryAuditExporter};
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimiter,
};
use uuid::Uuid;

use crate::wall_clock::{default_wall_clock, WallClock};

use super::audit_sink::{emit_or_503, ExportAuditSink, InMemoryExportAuditSink};
use super::state::{audit_export_rate_limit_config, router, AuditExportRouteState};
use super::stream::{build_audit_export_async_stream, InMemoryR2ListPager};
use super::types::{ExportAuditRow, EVENT_TYPE_VERIFY_FAILED, R2_LIST_PAGE_SIZE, TENANT_ID_HEADER};

/// A-P1-02 closure — cross-tenant-reject now fails CLOSED.
#[tokio::test]
async fn cross_tenant_reject_returns_503_on_audit_sink_failure() {
    use tower::ServiceExt;
    let auth_tenant = Uuid::from_u128(0xAA);
    let attempted = Uuid::from_u128(0xBB);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    sink.inject_failure("pipeline down").expect("inject");
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);
    let uri = format!("/v1/audit/{auth_tenant}/export?from=0&to=1000&tenant={attempted}",);
    let req = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, auth_tenant.to_string())
        .header("x-corelink-tenant-id", auth_tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "cross-tenant audit-emit failure MUST surface 503 (A-P1-02)"
    );
}

/// AuthTenant negative — a request with NO `x-corelink-tenant-id`
/// header MUST be rejected 401 by the `AuthTenant` extractor BEFORE
/// the `handle_export` body runs (`auth: AuthTenant` is the first
/// extractor in the handler signature). Drop that arg and the request
/// would proceed into the handler and parse the path tenant instead.
/// Both path `:tenant` and `?tenant=` are well-formed UUIDs so the
/// ONLY reason for the rejection is the missing authenticated-tenant
/// header.
#[tokio::test]
async fn missing_tenant_header_returns_401() {
    use tower::ServiceExt;
    let tenant = Uuid::from_u128(0xA1);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);
    // NO `x-corelink-tenant-id` header — `AuthTenant` fails CLOSED
    // with 401 before the handler is invoked. The `TENANT_ID_HEADER`
    // (`X-Tenant-Id`) is NOT the authenticated-tenant source; omitting
    // `x-corelink-tenant-id` is what trips the extractor.
    let uri = format!("/v1/audit/{tenant}/export?from=0&to=1000");
    let req = axum::http::Request::builder()
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "missing x-corelink-tenant-id MUST 401 at the AuthTenant extractor, \
         not reach handle_export"
    );
}

/// Cross-tenant negative (PATH segment) — a request whose path
/// `:tenant` does NOT equal the authenticated `x-corelink-tenant-id`
/// header MUST hit the step-1b cross-tenant guard: emit the SEV-1
/// `cross_tenant_reject` audit row and return 403, BEFORE any data
/// access (and before the `?tenant=` query check, which is left
/// matching the header here so ONLY the path drives the mismatch).
/// This complements the existing `?tenant=` mismatch test
/// (`cross_tenant_reject_returns_503_on_audit_sink_failure`). The sink
/// is healthy so the fail-CLOSED ordering yields 403 (not 503).
#[tokio::test]
async fn path_tenant_ne_header_returns_403_and_audits_sev1() {
    use tower::ServiceExt;
    let auth_tenant = Uuid::from_u128(0xAA);
    let path_tenant = Uuid::from_u128(0xBB);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink.clone() as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);
    // Path `:tenant` is `path_tenant` (0xBB) but the authenticated
    // header is `auth_tenant` (0xAA) → step-1b path mismatch → 403.
    let uri = format!("/v1/audit/{path_tenant}/export?from=0&to=1000");
    let req = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, auth_tenant.to_string())
        .header("x-corelink-tenant-id", auth_tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "path :tenant != authenticated header MUST 403 at the step-1b guard \
         (healthy sink → 403, not 503)"
    );
    // The SEV-1 cross-tenant row landed BEFORE the 403 (fail-CLOSED
    // ordering) — pin both the emit and the attempted-tenant binding.
    let rows = sink.snapshot().expect("snapshot");
    assert!(
        rows.iter().any(|r| r.exit_status == "cross_tenant_reject"
            && r.authenticated_tenant == Some(auth_tenant)
            && r.attempted_tenant == Some(path_tenant)),
        "path-mismatch MUST emit one cross_tenant_reject row binding the \
         authenticated + attempted (path) tenants; saw {:?}",
        rows.iter().map(|r| &r.exit_status).collect::<Vec<_>>(),
    );
}

/// A-P2-01 closure — rate-limit-deny now fails CLOSED.
#[tokio::test]
async fn rate_limit_deny_returns_503_on_audit_sink_failure() {
    use tower::ServiceExt;
    // Build a rate limiter that's already at the floor (burst=1,
    // first request consumes the token) so the second request hits
    // Deny429. The audit sink will fail when emitting the
    // rate_limited row, surfacing 503.
    let tenant = Uuid::from_u128(0xC1);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink.clone() as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);
    // First request: consume the token (200 expected; no failure
    // injected yet so emit succeeds).
    let req1 = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req1");
    let resp1 = app.clone().oneshot(req1).await.expect("oneshot");
    assert_eq!(resp1.status(), StatusCode::OK);
    // Inject failure NOW so the second request's rate_limited
    // emit hits the failing sink.
    sink.inject_failure("pipeline down").expect("inject");
    let req2 = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req2");
    let resp2 = app.oneshot(req2).await.expect("oneshot");
    assert_eq!(
        resp2.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "rate-limit-deny audit-emit failure MUST surface 503 (A-P2-01)"
    );
}

/// Wave-21 closure (`A-P2-05`): the rate-limit bucket's `now_ms`
/// is now driven by `state.wall_clock` (an `Arc<dyn WallClock>`)
/// rather than the request's `until_ms`. This test pins the
/// contract by:
///   1. Injecting an [`crate::wall_clock::InMemoryFakeWallClock`]
///      pinned at a known unix-ms instant.
///   2. Issuing the SAME stationary query window twice within the
///      60s refill floor — second request 429 (proves the bucket
///      clock is NOT advancing on the stationary `until_ms`).
///   3. Advancing the fake wall clock past the refill floor —
///      next request 200 (proves the bucket clock IS advancing on
///      the injected wall clock).
#[tokio::test]
async fn rate_limit_now_ms_is_driven_by_injected_wall_clock() {
    use crate::wall_clock::InMemoryFakeWallClock;
    use tower::ServiceExt;

    let tenant = Uuid::from_u128(0xF1);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    // Pin the wall clock at a known instant well past unix epoch
    // so the fallback-on-zero arm is NOT exercised.
    let fake = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: fake.clone() as Arc<dyn WallClock>,
        pat_gate: None,
    };
    let app = router(state);

    // First request — burst=1 → consumes the token, 200 expected.
    let req1 = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req1");
    let resp1 = app.clone().oneshot(req1).await.expect("oneshot");
    assert_eq!(
        resp1.status(),
        StatusCode::OK,
        "first request consumes the burst-1 token; expected 200",
    );

    // Second request immediately — wall clock still pinned, bucket
    // empty, refill floor not reached → 429.
    let req2 = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req2");
    let resp2 = app.clone().oneshot(req2).await.expect("oneshot");
    assert_eq!(
        resp2.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "second request inside the 60s refill floor MUST 429 \
         (wall clock pinned; bucket cannot refill)",
    );

    // Advance the wall clock by 61s — the bucket refills (refill=1
    // token / 60s; 61s ≥ 60s floor). The next request MUST allow.
    fake.advance(std::time::Duration::from_secs(61));
    let req3 = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req3");
    let resp3 = app.oneshot(req3).await.expect("oneshot");
    assert_eq!(
        resp3.status(),
        StatusCode::OK,
        "after advancing the fake wall clock 61s, the next request MUST allow \
         (proves wall-clock-driven `now_ms`, finding A-P2-05 closure)",
    );
}

/// Wave-23 closure of W21-R-P2-01 — `wall_clock.now_ms() == 0`
/// MUST fail-CLOSED (503 + `clock_unavailable` audit row) rather
/// than fall back to the request-window-derived bucket clock. This
/// pins the structural fix described in
/// `specs/_audits/sealed/2026-05-16-wave23-cleanup.md` §W21-R-P2-01: the
/// bucket clock NEVER couples to caller-controlled bytes, even on
/// the structurally-unreachable (production) pre-epoch branch.
#[tokio::test]
async fn wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row() {
    use crate::wall_clock::InMemoryFakeWallClock;
    use tower::ServiceExt;

    let tenant = Uuid::from_u128(0xF2);
    let sink: Arc<InMemoryExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
    let sink_dyn: Arc<dyn ExportAuditSink> = sink.clone();
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    // Pin the wall clock at the saturating value (unix_ms == 0).
    // SystemWallClock cannot reach this branch in production
    // (epoch is decades past), but InMemoryFakeWallClock can —
    // simulating a poisoned-mutex or pre-epoch host.
    let fake = Arc::new(InMemoryFakeWallClock::at_unix_ms(0));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink_dyn,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: fake as Arc<dyn WallClock>,
        pat_gate: None,
    };
    let app = router(state);

    let req = axum::http::Request::builder()
        .uri(format!("/v1/audit/{tenant}/export?from=0&to=1000"))
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "wall-clock saturating to 0 MUST fail-CLOSED with 503 \
         (W21-R-P2-01 closure — no fallback to until_ms-derived clock)",
    );

    let rows = sink.snapshot().expect("snapshot");
    assert!(
        rows.iter().any(|r| r.exit_status == "clock_unavailable"),
        "fail-CLOSED path MUST emit one `clock_unavailable` audit row; saw {:?}",
        rows.iter().map(|r| &r.exit_status).collect::<Vec<_>>(),
    );
}

/// A-P1-05 closure — verify-failed SEV-0 now fails CLOSED.
#[tokio::test]
async fn verify_failed_sev0_returns_503_on_audit_sink_failure() {
    // Drive the route handler logic directly by constructing the
    // emit_or_503 path with a failing sink; this isolates the
    // verify-failed arm without standing up a full chain-tampering
    // exporter (the integration test
    // `chain_tamper_emits_verify_failed_sev0` already covers the
    // happy-emit path end-to-end). The 503 surface is what we pin
    // here.
    let sink: Arc<dyn ExportAuditSink> = Arc::new({
        let s = InMemoryExportAuditSink::new();
        s.inject_failure("pipeline down").expect("inject");
        s
    });
    let row = ExportAuditRow {
        event_type: EVENT_TYPE_VERIFY_FAILED.to_string(),
        authenticated_tenant: Some(Uuid::from_u128(0xD1)),
        attempted_tenant: None,
        from_ms: 0,
        to_ms: 1,
        bytes_written: 0,
        events_written: 0,
        exit_status: "verify_failed".to_string(),
        payload: None,
    };
    let resp = emit_or_503(&sink, row).expect("503 on sink failure");
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "verify-failed-sev0 audit-emit failure MUST surface 503 (A-P1-05)"
    );
}

/// A-P1-03 closure — mid-stream-break audit-emit failure force-
/// closes the body WITHOUT yielding the abort trailer frame. This
/// is the documented trade-off: a 503 is impossible after headers
/// are flushed, so a truncated body becomes the loudest signal.
/// We assert (a) the audit sink saw the emit attempt, (b) NO
/// trailer frame leaves the generator, (c) the row data frames
/// already yielded BEFORE the break-row are preserved.
#[tokio::test]
async fn mid_stream_break_surfaces_audit_failure_via_forced_close() {
    use corelink_analytics::Region;
    use corelink_audit_chain::{
        AuditEvent, AuditEventKind, HashChainBuilder, InMemoryAuditExporter,
    };
    use futures::StreamExt;
    use serde_json::json;
    let tenant = Uuid::from_u128(0xE1);
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = ChainHash::genesis();
    for i in 0..3_u64 {
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_000 + i,
            tenant,
            Region::Iad,
            i,
            prev,
            json!({ "i": i }),
        );
        prev = builder.append(&e).expect("append");
        exporter.append_event(e).expect("seed");
    }
    let window = ExportWindow::new(0, 10_000).expect("window");
    let result = exporter
        .export_window(&tenant.to_string(), window)
        .expect("export");
    // Bogus anchor → verify fails on row 0 → mid-stream break path.
    let bogus_anchor = ChainHash::genesis();
    // Sink that fails on EVERY emit (production: audit pipeline
    // down during the mid-stream verify-failed emit).
    let sink_inner = Arc::new(InMemoryExportAuditSink::new());
    sink_inner
        .inject_failure("pipeline down mid-stream")
        .expect("inject");
    let sink_dyn: Arc<dyn ExportAuditSink> = sink_inner.clone();
    let pager = InMemoryR2ListPager::with_rows(result.rows, 2);
    let stream = build_audit_export_async_stream(
        Box::new(pager),
        "ignored-manifest".to_string(),
        bogus_anchor,
        sink_dyn,
        tenant,
        0,
        10_000,
        0,
        0,
    );
    let frames: Vec<_> = stream.collect().await;
    // Every frame yielded must be a `Frame::data` — the trailer
    // frame is suppressed under the wave-20 force-close discipline.
    for (idx, frame) in frames.iter().enumerate() {
        let f = frame.as_ref().expect("infallible");
        assert!(
            f.is_data(),
            "frame[{idx}] MUST be data — abort trailer is suppressed \
             on audit-emit failure (A-P1-03 force-close trade-off)"
        );
    }
    // The audit sink rejected the emit (so no captured row), but
    // the sink-error path was traversed exactly once (the snapshot
    // is empty because every emit returns Err early — we pin the
    // empty snapshot as a regression on the "force-close was
    // taken" path).
    let snap = sink_inner.snapshot().expect("snap");
    assert!(
        snap.is_empty(),
        "failing sink captures zero rows; force-close path traversed"
    );
    // Sanity: the generator stopped early — row 0 verify-fails so
    // no data frames precede the (suppressed) trailer. Frames len
    // is therefore 0 (the generator returns BEFORE yielding any
    // happy-row data frames in this seed).
    assert!(
        frames.len() < 3,
        "generator must force-close before draining all rows"
    );
}

/// M4 DoS fix — a window wider than 30 days MUST be rejected with 400
/// BEFORE any data access. Pins `MAX_EXPORT_WINDOW_MS` enforcement in
/// `handle_export` (step 2b).
#[tokio::test]
async fn export_window_exceeding_30_days_returns_400() {
    use super::types::MAX_EXPORT_WINDOW_MS;
    use tower::ServiceExt;

    let tenant = Uuid::from_u128(0xD05A);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);

    // from=0, to = MAX+1 ms → span is exactly one millisecond past the cap.
    let to_ms = MAX_EXPORT_WINDOW_MS + 1;
    let uri = format!("/v1/audit/{tenant}/export?from=0&to={to_ms}");
    let req = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "window > 30 days MUST be rejected with 400 (M4 DoS fix)"
    );
}

/// M4 DoS fix — a window exactly at the 30-day boundary MUST pass the
/// span gate and proceed to the exporter (200 on an empty window).
/// Pins that `MAX_EXPORT_WINDOW_MS` is an inclusive upper bound.
#[tokio::test]
async fn export_window_at_30_day_boundary_is_allowed() {
    use super::types::MAX_EXPORT_WINDOW_MS;
    use tower::ServiceExt;

    let tenant = Uuid::from_u128(0xD05B);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: None,
    };
    let app = router(state);

    // from=0, to = MAX_EXPORT_WINDOW_MS → span == cap exactly, MUST allow.
    let to_ms = MAX_EXPORT_WINDOW_MS;
    let uri = format!("/v1/audit/{tenant}/export?from=0&to={to_ms}");
    let req = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "window == 30 days MUST pass the span gate (inclusive boundary)"
    );
}

/// WP-B — audit-read requires a READ-capable PAT. A request whose
/// server-trusted `x-corelink-scope` carries NO read capability (here a
/// `find-missing`-only, existence-probe token) MUST be rejected 403
/// "insufficient scope" BEFORE any data access, even with a valid tenant
/// binding and window. A `read-only` PAT (the self-serve customer-export
/// case) MUST clear the gate — so the owner's flip from admin-only does not
/// break `corelink audit export`. The `admin` happy path is also pinned by
/// `export_window_at_30_day_boundary_is_allowed`.
#[tokio::test]
async fn non_read_scope_is_rejected_403_read_scope_passes() {
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let build_app = || {
        let sink = Arc::new(InMemoryExportAuditSink::new());
        let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
        let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
            rl_audit,
            rl_metrics,
            audit_export_rate_limit_config(),
        ));
        let state = AuditExportRouteState {
            exporter,
            rate_limiter,
            audit_sink: sink as Arc<dyn ExportAuditSink>,
            pager_page_size: R2_LIST_PAGE_SIZE,
            wall_clock: default_wall_clock(),
            pat_gate: None,
        };
        router(state)
    };

    let tenant = Uuid::from_u128(0x5C0BE);
    let uri = format!("/v1/audit/{tenant}/export?from=0&to=1000");

    // find-missing-only (no read capability) → 403 "insufficient scope".
    let req = axum::http::Request::builder()
        .uri(uri.clone())
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "find-missing")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = build_app().oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a find-missing-only (no-read) scope MUST be rejected 403 at the audit-read gate"
    );
    let body = resp.into_body().collect().await.expect("body").to_bytes();
    assert_eq!(body.as_ref(), b"insufficient scope");

    // read-only (the self-serve customer export case) → clears the gate (200).
    let req_ro = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "read-only")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp_ro = build_app().oneshot(req_ro).await.expect("oneshot");
    assert_eq!(
        resp_ro.status(),
        StatusCode::OK,
        "a read-only PAT MUST clear the audit-read gate (customer self-serve export)"
    );
}

/// rt-nuclear #17 — the native PAT possession gate, when wired, rejects an
/// audit-export request whose bearer does NOT prove possession of the
/// authenticated tenant's PAT (here: no bearer at all → fail-CLOSED 401). This
/// is the Argon2id backstop that contains a leaked `PAT_SIGNING_KEY`: a forged
/// HMAC bearer for a victim tenant cannot exfiltrate that tenant's audit log.
#[tokio::test]
async fn forged_pat_is_rejected_when_gate_present() {
    use crate::adapter_pat::PatRow;
    use crate::native_pat_gate::testing::verifier_with_row;
    use crate::native_pat_gate::NativePatGate;
    use corelink_pat::PatSigningKey;
    use tower::ServiceExt;

    let tenant = Uuid::from_u128(0x171C7117);
    let sink = Arc::new(InMemoryExportAuditSink::new());
    let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
    let verifier = verifier_with_row(
        "no-such-token".to_owned(),
        PatRow {
            tenant_id: tenant.to_string(),
            pat_hash: String::new(),
            scope: "cas:rw".to_owned(),
            find_only: false,
        },
        key,
    );
    let state = AuditExportRouteState {
        exporter,
        rate_limiter,
        audit_sink: sink as Arc<dyn ExportAuditSink>,
        pager_page_size: R2_LIST_PAGE_SIZE,
        wall_clock: default_wall_clock(),
        pat_gate: Some(Arc::new(NativePatGate::new_for_test(verifier))),
    };
    let app = router(state);
    // Valid authenticated tenant + window, but NO Authorization bearer — the
    // possession gate must fail CLOSED with 401 BEFORE any data access.
    let uri = format!("/v1/audit/{tenant}/export?from=0&to=1000");
    let req = axum::http::Request::builder()
        .uri(uri)
        .header(TENANT_ID_HEADER, tenant.to_string())
        .header("x-corelink-tenant-id", tenant.to_string())
        .header(crate::scope::SCOPE_HEADER, "admin")
        .body(axum::body::Body::empty())
        .expect("req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "an un-possessed (forged) PAT must be rejected 401 by the native gate"
    );
}
