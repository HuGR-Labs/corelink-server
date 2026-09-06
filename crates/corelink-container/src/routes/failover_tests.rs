#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use super::*;
use axum::{
    body::Body,
    http::{Request as HttpRequest, StatusCode},
    routing::get,
    Router,
};
use corelink_failover_router::{RegionHealth, CONSECUTIVE_FAILURES_THRESHOLD};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tower::ServiceExt; // `.oneshot()`

// ── colo → failover macro map ────────────────────────────────────────────

#[test]
fn colo_map_is_exact() {
    assert_eq!(failover_region_for_colo("iad"), Some(Region::Enam));
    assert_eq!(failover_region_for_colo("lhr"), Some(Region::Weur));
    assert_eq!(failover_region_for_colo("sam"), Some(Region::Sam));
    // APAC colos have no sibling in the 4-macro graph → inert.
    assert_eq!(failover_region_for_colo("nrt"), None);
    assert_eq!(failover_region_for_colo("syd"), None);
    assert_eq!(failover_region_for_colo("zzz"), None);
}

// ── RollingMetricsHealthProbe (the REAL probe) ────────────────────────────

#[test]
fn empty_probe_is_healthy() {
    let p = RollingMetricsHealthProbe::new();
    let snap = p.probe(Region::Enam, 1_000).unwrap();
    assert_eq!(snap.health, RegionHealth::Healthy);
}

#[test]
fn all_three_signals_degrade() {
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    // A sustained streak of high-latency 5xx trips all three signals:
    // 5xx rate = 100% (> 1%), p99 = 500ms (> 300ms), consecutive >= 3.
    // Seeded above the sample floor (50) so the window is significant.
    for _ in 0..MIN_SAMPLES_FOR_FAILOVER_DEFAULT {
        p.record_outcome(false, 10, now);
    }
    for _ in 0..(CONSECUTIVE_FAILURES_THRESHOLD + 2) {
        p.record_outcome(true, 500, now);
    }
    let snap = p.probe(Region::Enam, now).unwrap();
    assert_eq!(snap.health, RegionHealth::Degraded);
    assert_eq!(snap.active_triggers.len(), 3);
}

#[test]
fn latency_alone_does_not_degrade() {
    // Multi-signal AND: high latency but ZERO 5xx and no failure streak
    // must stay Healthy (suppresses transient-slowness false positives).
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    for _ in 0..10 {
        p.record_outcome(false, 900, now);
    }
    let snap = p.probe(Region::Enam, now).unwrap();
    assert_eq!(snap.health, RegionHealth::Healthy);
}

#[test]
fn stale_samples_are_pruned_out_of_window() {
    let p = RollingMetricsHealthProbe::new();
    // Old outage samples...
    for _ in 0..5 {
        p.record_outcome(true, 500, 1_000);
    }
    // ...are entirely outside the 5s window at a much later observation.
    let later = 1_000 + WINDOW_MS + 10_000;
    let snap = p.probe(Region::Enam, later).unwrap();
    assert_eq!(snap.health, RegionHealth::Healthy);
}

// ── Tower layer integration (oneshot) ─────────────────────────────────────

fn app(state: FailoverLayerState, reached: Arc<AtomicBool>) -> Router {
    let r_get = reached.clone();
    let r_post = reached;
    Router::new()
        .route(
            "/v1/cas/{tenant}/{hash}",
            get(move || {
                let r = r_get.clone();
                async move {
                    r.store(true, Ordering::SeqCst);
                    "ok"
                }
            })
            .put(move || {
                let r = r_post.clone();
                async move {
                    r.store(true, Ordering::SeqCst);
                    "written"
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(state, failover_guard))
}

fn degrade(state: &FailoverLayerState) {
    let probe = state.probe();
    let now = now_ms();
    for _ in 0..(CONSECUTIVE_FAILURES_THRESHOLD + 2) {
        probe.record_outcome(true, 500, now);
    }
}

/// Seed degraded samples AND pump the hysteresis gate through its trip
/// threshold so the guard is latched (post-B.2 the raw snapshot alone no
/// longer blocks).
fn degrade_and_trip(state: &FailoverLayerState) {
    degrade(state);
    let gate = state.failover_gate();
    for _ in 0..FAILOVER_TRIP_PROBES {
        gate.observe(true);
    }
}

#[tokio::test]
async fn healthy_region_passes_reads_and_writes() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let reached = Arc::new(AtomicBool::new(false));
    let resp = app(state, reached.clone())
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/v1/cas/t1/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(reached.load(Ordering::SeqCst));
    assert!(resp.headers().get(FAILOVER_ACTIVE_HEADER).is_none());
}

#[tokio::test]
async fn integrated_transient_error_then_clean_requests_do_not_trip_hysteresis() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let calls = Arc::new(AtomicU32::new(0));
    let route_calls = Arc::clone(&calls);
    let app = Router::new()
        .route(
            "/v1/cas/t1/abc",
            get(move || {
                let call = route_calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if call == 0 {
                        StatusCode::INTERNAL_SERVER_ERROR
                    } else {
                        StatusCode::OK
                    }
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            failover_guard,
        ));

    let mut statuses = Vec::new();
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri("/v1/cas/t1/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        statuses.push(response.status());
    }
    assert_eq!(
        statuses,
        vec![
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::OK,
            StatusCode::OK
        ],
        "one 5xx followed by clean requests must not be counted repeatedly"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn degraded_region_blocks_writes_503() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    degrade_and_trip(&state);
    let reached = Arc::new(AtomicBool::new(false));
    let resp = app(state, reached.clone())
        .oneshot(
            HttpRequest::builder()
                .method("PUT")
                .uri("/v1/cas/t1/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    // Fail-CLOSED: the handler must NOT have run (no storage write).
    assert!(!reached.load(Ordering::SeqCst));
}

#[tokio::test]
async fn degraded_region_passes_reads_with_sibling_hint() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    degrade_and_trip(&state);
    let reached = Arc::new(AtomicBool::new(false));
    let resp = app(state, reached.clone())
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/v1/cas/t1/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(reached.load(Ordering::SeqCst));
    assert_eq!(resp.headers().get(FAILOVER_ACTIVE_HEADER).unwrap(), "1");
    // Enam's failover sibling is Wnam (WNAM↔ENAM).
    assert_eq!(
        resp.headers().get(FAILOVER_READ_REGION_HEADER).unwrap(),
        Region::Wnam.as_str()
    );
}

#[tokio::test]
async fn inert_when_no_failover_region() {
    // primary = None (e.g. nrt/syd colo) → layer passes writes through even
    // if degradation were recorded (no macro region to fail over).
    let state = FailoverLayerState::with_primary(None);
    degrade(&state);
    let reached = Arc::new(AtomicBool::new(false));
    let resp = app(state, reached.clone())
        .oneshot(
            HttpRequest::builder()
                .method("PUT")
                .uri("/v1/cas/t1/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(reached.load(Ordering::SeqCst));
}

// ── B.1 sample floor + B.2 hysteresis ────────────────────────────────────

#[test]
fn tiny_clean_sample_burst_stays_healthy() {
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    for _ in 0..3 {
        p.record_outcome(false, 10, now);
    }
    let snap = p.probe(Region::Enam, now).unwrap();
    assert_eq!(snap.health, RegionHealth::Healthy);
}

#[test]
fn tiny_error_sample_is_indeterminate_not_healthy() {
    // The floor remains an anti-blip guard, but a low-traffic window with
    // observed server errors must not claim that the region is healthy.
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    for _ in 0..3 {
        p.record_outcome(true, 500, now);
    }
    let err = p
        .probe(Region::Enam, now)
        .expect_err("low-traffic errors are indeterminate");
    assert!(err.contains("insufficient samples"));
}

#[test]
fn one_transient_5xx_then_clean_probe_is_not_counted_repeatedly() {
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    p.record_outcome(true, 500, now);
    assert!(p.probe(Region::Enam, now).is_err());

    // A clean event closes the transient bucket. The old 5xx remains in
    // the bounded window for rate/p99 accounting, but it cannot feed the
    // same degraded observation into hysteresis on every clean probe.
    p.record_outcome(false, 10, now + 1);
    assert_eq!(
        p.probe(Region::Enam, now + 1).unwrap().health,
        RegionHealth::Healthy
    );
    assert_eq!(
        p.probe(Region::Enam, now + 2).unwrap().health,
        RegionHealth::Healthy
    );
}

#[test]
fn router_maps_low_traffic_error_uncertainty_to_degraded() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let probe = state.probe();
    let now = 1_000_000;
    for _ in 0..3 {
        probe.record_outcome(true, 500, now);
    }
    let snap = state.router.region_health(Region::Enam, now);
    assert_eq!(snap.health, RegionHealth::Degraded);
}

#[test]
fn outage_above_sample_floor_still_degrades() {
    // Non-regression: a REAL outage (statistically significant window)
    // still trips. 60 samples, trailing streak of 5 slow 5xx:
    // rate ≈ 8.3% > 1%, p99 = 500ms > 300ms, consecutive = 5 ≥ 3.
    let p = RollingMetricsHealthProbe::new();
    let now = 1_000_000;
    for _ in 0..55 {
        p.record_outcome(false, 10, now);
    }
    for _ in 0..5 {
        p.record_outcome(true, 500, now);
    }
    let snap = p.probe(Region::Enam, now).unwrap();
    assert_eq!(snap.health, RegionHealth::Degraded);
    assert_eq!(snap.active_triggers.len(), 3);
}

#[test]
fn hysteresis_single_degraded_probe_does_not_trip_guard() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    degrade(&state); // probe WOULD read Degraded (above floor not required here)
    let gate = state.failover_gate();
    assert!(!gate.observe(true), "one degraded probe must not trip");
}

#[test]
fn hysteresis_three_consecutive_degraded_probes_trip_guard() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    degrade(&state);
    let gate = state.failover_gate();
    assert!(!gate.observe(true));
    assert!(!gate.observe(true));
    assert!(gate.observe(true), "three consecutive degraded probes trip");
}

#[test]
fn hysteresis_recovers_only_after_grace_of_healthy_probes() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let gate = state.failover_gate();
    for _ in 0..FAILOVER_TRIP_PROBES {
        gate.observe(true);
    }
    // Recovery grace: FAILOVER_RECOVER_PROBES consecutive healthy probes;
    // release happens ON that Nth healthy observation.
    for _ in 0..(FAILOVER_RECOVER_PROBES - 1) {
        assert!(gate.observe(false), "engaged while grace incomplete");
    }
    gate.observe(true); // degraded blip resets the healthy streak...
    for _ in 0..(FAILOVER_RECOVER_PROBES - 1) {
        assert!(gate.observe(false), "grace restarted after blip");
    }
    assert!(
        !gate.observe(false),
        "recovers exactly on the Nth consecutive healthy probe"
    );
}

#[tokio::test]
async fn guard_requires_hysteresis_before_blocking_writes() {
    // Integration: raw-degraded samples present, but only ONE probe
    // observation so far → the write must PASS (was: instant 503).
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    degrade(&state);
    let reached = Arc::new(AtomicBool::new(false));
    let resp = app(state.clone(), reached.clone())
        .oneshot(
            HttpRequest::builder()
                .method("PUT")
                .uri("/v1/cas/t1/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "single degraded observation must not block writes"
    );
    assert!(reached.load(Ordering::SeqCst));
}

#[test]
fn anonymous_caller_cannot_refresh_heartbeat() {
    let heartbeat = FailoverHeartbeat::new(0, 10);
    let expected: Arc<str> = Arc::from("internal-heartbeat-secret");
    let anonymous = HeaderMap::new();

    assert!(heartbeat.is_stale(100));
    assert_eq!(
        apply_authenticated_heartbeat(&heartbeat, Some(&expected), &anonymous, 100),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        heartbeat.is_stale(100),
        "a public/anonymous request must not renew failover liveness"
    );

    let mut authenticated = HeaderMap::new();
    authenticated.insert(
        INTERNAL_AUTH_HEADER,
        HeaderValue::from_static("internal-heartbeat-secret"),
    );
    assert_eq!(
        apply_authenticated_heartbeat(&heartbeat, Some(&expected), &authenticated, 100),
        StatusCode::OK
    );
    assert!(!heartbeat.is_stale(100));
}

#[tokio::test]
async fn stale_external_heartbeat_blocks_without_error_traffic() {
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let now = now_ms();
    let heartbeat = state.heartbeat();
    heartbeat.beat(now.saturating_sub(heartbeat.stale_after_ms() + 1));
    assert!(heartbeat.is_stale(now));

    // No 5xx samples are injected. The first two clean requests only
    // advance hysteresis; the third is fail-CLOSED solely because the
    // independent heartbeat has gone stale.
    let reached = Arc::new(AtomicBool::new(false));
    for attempt in 0..FAILOVER_TRIP_PROBES {
        let resp = app(state.clone(), reached.clone())
            .oneshot(
                HttpRequest::builder()
                    .method("PUT")
                    .uri("/v1/cas/t1/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if attempt + 1 == FAILOVER_TRIP_PROBES {
            assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        } else {
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }
    assert!(reached.load(Ordering::SeqCst));
}

// ── B.3 bounded audit sink ───────────────────────────────────────────────

#[test]
fn ring_buffer_sink_keeps_newest_cap_records() {
    use corelink_failover_router::{
        FailoverAuditEventType as ReplicaAuditEventType, FailoverAuditRecord as ReplicaAuditRecord,
        FailoverAuditSink as ReplicaAuditSink,
    };
    let mk = |i: u32| ReplicaAuditRecord {
        event_type: ReplicaAuditEventType::ReplicationStarted,
        tenant_id_hash: format!("t{i}"),
        blob_hash: String::new(),
        primary_region: "enam".to_owned(),
        replica_region: "wnam".to_owned(),
        timestamp_ms: 1_000 + u64::from(i),
        detail: String::new(),
    };
    // The production wiring's cap (same constant `with_primary` uses).
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let sink = state.audit_sink();
    for i in 0..(FAILOVER_AUDIT_SINK_CAP as u32 + 10) {
        sink.emit(mk(i)).unwrap();
    }
    let records = sink.records();
    assert_eq!(records.len(), FAILOVER_AUDIT_SINK_CAP, "cap enforced");
    assert_eq!(
        records.last().unwrap().tenant_id_hash,
        format!("t{}", FAILOVER_AUDIT_SINK_CAP as u32 + 9),
        "newest preserved"
    );
    assert_eq!(
        records.first().unwrap().tenant_id_hash,
        "t10",
        "oldest dropped"
    );
}
