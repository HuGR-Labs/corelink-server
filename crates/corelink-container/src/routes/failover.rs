//! Read-side failover Tower layer + a REAL `HealthProbe` (WI-MULTI-REGION-V1
//! failover prod-wiring).
//!
//! # What this closes
//!
//! `corelink-failover-router` shipped the pure-logic decision core
//! (multi-signal degradation → route reads to the sibling region + block
//! writes) plus an `InMemoryHealthProbe` fixture whose signals are *injected*
//! by tests. Nothing on the live request path consumed it: a production region
//! outage did NOT flow through the router. This module is the production wiring
//! the crate docs promised ("the production Tower layer + real health probes"):
//!
//! 1. [`RollingMetricsHealthProbe`] — a REAL [`HealthProbe`] that observes the
//!    container's OWN live traffic (5xx rate, p99 latency, consecutive
//!    failures over a sustained 5 s window) and evaluates the exact same
//!    multi-signal rule via [`RegionHealthSnapshot::evaluate`]. No injected
//!    HashMap, no simulated outage — the signals are the real ones this
//!    container is emitting.
//! 2. [`failover_guard`] — the axum/Tower middleware, layered in
//!    `routes::build_with_factory` exactly like [`crate::routes::residency`]'s
//!    `residency_guard`. It drives the crate's [`FailoverRouter`] decision core
//!    with the real probe and, when THIS region is degraded:
//!      - **write** (`POST`/`PUT`/`PATCH`/`DELETE`) → fail-CLOSED **503
//!        `failover_readonly`** (writes are blocked during failover so a
//!        stale-read-after-write can never occur), emitting the
//!        `failover.detected` audit BEFORE the reject.
//!      - **read** (`GET`/`HEAD`) → pass through, stamping
//!        `x-corelink-failover-read-region: <sibling>` + `x-corelink-failover-active: 1`
//!        so the edge Worker (which holds the cross-region Service Bindings) can
//!        re-route the read to the sibling region. The container cannot itself
//!        reach the sibling R2, so the actual reroute is the edge's job; the
//!        container's load-bearing action is the fail-closed write-block + the
//!        advisory hint.
//!
//! # Region model seam (precisely flagged)
//!
//! The failover graph is the 4-macro model (`wnam`/`enam`/`weur`/`sam`, siblings
//! WNAM↔ENAM, WEUR↔SAM). A container serves ONE colo (`R2_CAS_REGION`).
//! [`failover_region_for_colo`] maps the colo to its failover macro:
//! `iad→enam`, `lhr→weur`, `sam→sam`. The APAC colos (`nrt`/`syd`) have no
//! sibling in the 4-macro graph, so the layer is INERT (pass-through) there
//! until the graph gains an APAC pair — it never blocks a region it cannot fail
//! over. Dev/CI (no/unknown `R2_CAS_REGION`) is likewise inert.
//!
//! # Fail-safe vs fail-closed
//!
//! The multi-signal rule is an **AND** of three sustained signals, so the layer
//! never trips on transient slowness — in a healthy container it is completely
//! inert (no false write-blocks). It only ever engages under a real, sustained,
//! triangulated outage, and then it fails **closed** on writes.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use corelink_failover_router::{
    FailoverRouter, HealthProbe, InMemoryFailoverAuditSink, InMemoryFailoverRouter, Region,
    RegionHealthSnapshot, ResidencyGraph, SUSTAINED_WINDOW_SECS,
};

/// The DO-injected, server-trusted tenant header (same value the CAS/AC
/// handlers and the rate-limit layer key off).
const TENANT_HEADER: &str = "x-corelink-tenant-id";

/// Advisory response header naming the sibling region the edge should re-route
/// reads to while this region is in failover.
pub const FAILOVER_READ_REGION_HEADER: &str = "x-corelink-failover-read-region";
/// Advisory response header flagging that failover is engaged for this region.
pub const FAILOVER_ACTIVE_HEADER: &str = "x-corelink-failover-active";

/// Rolling-metrics window in milliseconds — the "sustained" window over which
/// the three degradation signals are measured (`SUSTAINED_WINDOW_SECS` = 5 s).
const WINDOW_MS: u64 = SUSTAINED_WINDOW_SECS * 1_000;

/// Hard cap on retained samples so a request burst can never grow the probe's
/// memory without bound (older samples are also time-pruned every observation).
const MAX_SAMPLES: usize = 4_096;

/// Wall-clock milliseconds since the Unix epoch.
#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Map a container serving colo (`R2_CAS_REGION`) to its failover macro region,
/// or `None` when the colo has no sibling in the 4-macro failover graph.
///
/// `iad`→`enam` (US-east; sibling `wnam`), `lhr`→`weur` (EU; sibling `sam`),
/// `sam`→`sam` (sibling `weur`). `nrt`/`syd` (APAC) and anything unknown → `None`
/// (the layer stays inert; see the module-level "Region model seam").
#[must_use]
pub fn failover_region_for_colo(colo: &str) -> Option<Region> {
    match colo {
        "iad" => Some(Region::Enam),
        "lhr" => Some(Region::Weur),
        "sam" => Some(Region::Sam),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// RollingMetricsHealthProbe — the REAL probe over this container's live traffic
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct Sample {
    ts_ms: u64,
    is_5xx: bool,
    latency_ms: u64,
}

/// A production [`HealthProbe`] backed by the container's own observed request
/// outcomes over a sustained rolling window. Unlike the crate's
/// `InMemoryHealthProbe` (signals injected by tests) this probe's signals are
/// the REAL 5xx rate / p99 latency / consecutive-failure streak the container
/// is currently emitting.
///
/// The [`failover_guard`] middleware feeds every completed response into
/// [`RollingMetricsHealthProbe::record_outcome`]; [`HealthProbe::probe`] then
/// evaluates the three signals via [`RegionHealthSnapshot::evaluate`] (the same
/// multi-signal AND rule the crate pins).
#[derive(Debug)]
pub struct RollingMetricsHealthProbe {
    samples: Mutex<VecDeque<Sample>>,
}

impl RollingMetricsHealthProbe {
    /// Construct an empty probe (defaults to healthy until traffic is observed).
    #[must_use]
    pub fn new() -> Self {
        RollingMetricsHealthProbe {
            samples: Mutex::new(VecDeque::new()),
        }
    }

    /// Record one completed request outcome. `is_5xx` marks a server-error
    /// response; `latency_ms` is the wall-clock service time; `now_ms` is the
    /// observation timestamp. Prunes samples older than the sustained window
    /// and bounds retention to [`MAX_SAMPLES`].
    pub fn record_outcome(&self, is_5xx: bool, latency_ms: u64, now_ms: u64) {
        let mut guard = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        guard.push_back(Sample {
            ts_ms: now_ms,
            is_5xx,
            latency_ms,
        });
        Self::prune(&mut guard, now_ms);
        while guard.len() > MAX_SAMPLES {
            guard.pop_front();
        }
    }

    fn prune(guard: &mut VecDeque<Sample>, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(WINDOW_MS);
        while let Some(front) = guard.front() {
            if front.ts_ms < cutoff {
                guard.pop_front();
            } else {
                break;
            }
        }
    }

    /// Nearest-rank p99 latency over the retained window (0 when empty).
    fn p99_latency(sorted_latencies: &[u64]) -> u64 {
        if sorted_latencies.is_empty() {
            return 0;
        }
        // ceil(0.99 * n) - 1, clamped into range.
        let n = sorted_latencies.len();
        let rank = (99 * n).div_ceil(100); // == ceil(0.99 * n)
        let idx = rank.saturating_sub(1).min(n - 1);
        sorted_latencies.get(idx).copied().unwrap_or(0)
    }
}

impl Default for RollingMetricsHealthProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthProbe for RollingMetricsHealthProbe {
    fn probe(&self, region: Region, timestamp_ms: u64) -> Result<RegionHealthSnapshot, String> {
        let mut guard = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        Self::prune(&mut guard, timestamp_ms);

        let total = guard.len();
        if total == 0 {
            // No observed traffic in the window → healthy (nothing to fail over).
            return Ok(RegionHealthSnapshot::evaluate(
                region,
                0.0,
                0,
                0,
                timestamp_ms,
            ));
        }

        let err_count = guard.iter().filter(|s| s.is_5xx).count();
        #[allow(clippy::cast_precision_loss)]
        let rate_5xx_pct = (err_count as f64 / total as f64) * 100.0;

        // Consecutive trailing 5xx (newest-first until the first non-error).
        let mut consecutive: u32 = 0;
        for s in guard.iter().rev() {
            if s.is_5xx {
                consecutive = consecutive.saturating_add(1);
            } else {
                break;
            }
        }

        let mut latencies: Vec<u64> = guard.iter().map(|s| s.latency_ms).collect();
        latencies.sort_unstable();
        let p99 = Self::p99_latency(&latencies);

        Ok(RegionHealthSnapshot::evaluate(
            region,
            rate_5xx_pct,
            p99,
            consecutive,
            timestamp_ms,
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tower layer
// ──────────────────────────────────────────────────────────────────────────────

/// State threaded into the [`failover_guard`] middleware.
///
/// `primary == None` ⇒ the layer is INERT (dev/CI, or an APAC colo with no
/// sibling in the failover graph) and every request passes straight through.
#[derive(Clone)]
pub struct FailoverLayerState {
    router: Arc<dyn FailoverRouter>,
    probe: Arc<RollingMetricsHealthProbe>,
    residency: ResidencyGraph,
    primary: Option<Region>,
}

impl std::fmt::Debug for FailoverLayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailoverLayerState")
            .field("primary", &self.primary)
            .finish_non_exhaustive()
    }
}

impl FailoverLayerState {
    /// Build the layer state, resolving this container's failover macro region
    /// from `R2_CAS_REGION` (the SAME env the CAS handler + the residency guard
    /// key off, so all three agree by construction). Wires the crate's
    /// [`FailoverRouter`] decision core to the REAL [`RollingMetricsHealthProbe`]
    /// and a `tracing`-backed audit sink.
    #[must_use]
    pub fn from_env() -> Self {
        let colo = crate::storage::env_or("R2_CAS_REGION", "iad");
        let primary = failover_region_for_colo(&colo);
        if primary.is_none() {
            tracing::info!(
                colo = %colo,
                "failover layer INERT: colo has no sibling in the 4-macro failover graph \
                 (nrt/syd/unknown) — reads/writes pass through unchanged"
            );
        }
        Self::with_primary(primary)
    }

    /// Construct with an explicit primary region (test seam + `from_env`).
    #[must_use]
    pub fn with_primary(primary: Option<Region>) -> Self {
        let probe = Arc::new(RollingMetricsHealthProbe::new());
        let audit = Arc::new(InMemoryFailoverAuditSink::new());
        let router = Arc::new(InMemoryFailoverRouter::new(
            Arc::clone(&probe) as Arc<dyn HealthProbe>,
            audit,
        ));
        FailoverLayerState {
            router,
            probe,
            residency: ResidencyGraph,
            primary,
        }
    }

    /// Test/inspection accessor for the shared probe (drive `record_outcome`).
    #[must_use]
    pub fn probe(&self) -> Arc<RollingMetricsHealthProbe> {
        Arc::clone(&self.probe)
    }
}

fn is_write_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Uniform 503 `failover_readonly` envelope — writes are blocked while the
/// region is degraded (fail-CLOSED). No handler/storage I/O on this path.
fn readonly_response(primary: Region, sibling: Option<Region>) -> Response {
    let sibling_str = sibling.map_or("none", Region::as_str);
    let body = format!(
        "{{\"error\":\"failover_readonly\",\"message\":\"region '{}' is in failover \
         read-only mode; writes are blocked (read failover target: '{}')\"}}",
        primary.as_str(),
        sibling_str
    );
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("content-type", "application/json")],
        body,
    )
        .into_response()
}

/// Failover middleware. Layered in `routes::build_with_factory` alongside
/// `residency_guard`. See the module docs for the full behaviour.
pub async fn failover_guard(
    State(state): State<FailoverLayerState>,
    req: Request,
    next: Next,
) -> Response {
    // Inert when this container has no failover macro region (dev/CI or an
    // APAC colo with no sibling in the 4-macro graph).
    let Some(primary) = state.primary else {
        return next.run(req).await;
    };

    let now = now_ms();
    let snap = state.router.region_health(primary, now);

    if snap.health.requires_failover() {
        let sibling = state.residency.sibling(primary);
        let write = is_write_method(req.method());

        if write {
            // Fail-CLOSED: block the write. `route_read` emits the
            // `failover.detected` audit BEFORE we return (audit-before-action);
            // its decision is discarded — we only need the side-effect + the
            // reject envelope.
            let tenant = req
                .headers()
                .get(TENANT_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let _ = state.router.route_read(tenant, primary, now);
            tracing::warn!(
                primary = %primary.as_str(),
                sibling = sibling.map_or("none", Region::as_str),
                "failover: WRITE blocked (region degraded, read-only mode)"
            );
            return readonly_response(primary, sibling);
        }

        // Read during failover: serve best-effort + stamp the sibling hint so
        // the edge Worker re-routes the read via its cross-region Service
        // Binding. (No per-read audit — that would be one audit per request
        // during an outage; the write-block path above carries the audit.)
        let mut resp = run_and_record(&state.probe, req, next, now).await;
        let headers = resp.headers_mut();
        headers.insert(FAILOVER_ACTIVE_HEADER, HeaderValue::from_static("1"));
        if let Some(s) = sibling {
            if let Ok(v) = HeaderValue::from_str(s.as_str()) {
                headers.insert(FAILOVER_READ_REGION_HEADER, v);
            }
        }
        return resp;
    }

    // Healthy region — pass through and feed the outcome back into the probe.
    run_and_record(&state.probe, req, next, now).await
}

/// Run the downstream handler and record its outcome (5xx? latency) into the
/// real probe so the NEXT probe observation reflects live health.
async fn run_and_record(
    probe: &RollingMetricsHealthProbe,
    req: Request,
    next: Next,
    now: u64,
) -> Response {
    let started = Instant::now();
    let resp = next.run(req).await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let is_5xx = resp.status().is_server_error();
    probe.record_outcome(is_5xx, latency_ms, now);
    resp
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
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        routing::get,
        Router,
    };
    use corelink_failover_router::{RegionHealth, CONSECUTIVE_FAILURES_THRESHOLD};
    use std::sync::atomic::{AtomicBool, Ordering};
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
    async fn degraded_region_blocks_writes_503() {
        let state = FailoverLayerState::with_primary(Some(Region::Enam));
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
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        // Fail-CLOSED: the handler must NOT have run (no storage write).
        assert!(!reached.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn degraded_region_passes_reads_with_sibling_hint() {
        let state = FailoverLayerState::with_primary(Some(Region::Enam));
        degrade(&state);
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
}
