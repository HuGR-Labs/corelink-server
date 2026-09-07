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
//! The multi-signal rule is an **AND** of three signals, gated by TWO
//! anti-false-positive layers before it can block a write:
//!
//! 1. **Sample floor** (`FAILOVER_MIN_SAMPLES`, default 50): below 50 observed
//!    requests in the 5s window a clean/empty window is Healthy, but a window
//!    containing any 5xx is indeterminate and fails closed through the router's
//!    probe-error path. The independent hysteresis still protects writes from
//!    a single transient observation.
//! 2. **Trip/recover hysteresis**: the guard latches only after 3 consecutive
//!    DEGRADED probe observations and releases only after 5 consecutive
//!    HEALTHY ones (no flapping).
//!
//! In a healthy container the layer is completely inert (no false
//! write-blocks). It only ever engages under a real, sustained, triangulated
//! outage, and then it fails **closed** on writes.
//!
//! The audit sink wired here is a BOUNDED ring buffer (`FAILOVER_AUDIT_SINK_CAP`,
//! 10k records, oldest dropped) — process-lifetime memory stays flat.
//!
//! An authenticated internal heartbeat endpoint refreshes [`FailoverHeartbeat`].
//! The public `/_health` readiness endpoint never refreshes this signal. A stale
//! heartbeat is treated as degraded through the same hysteresis gate, so a
//! silent blackhole cannot hide behind an empty data-plane sample window;
//! data-plane responses never refresh it.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use subtle::ConstantTimeEq;

use corelink_failover_router::{
    FailoverAuditSink, FailoverRouter, HealthProbe, InMemoryFailoverAuditSink,
    InMemoryFailoverRouter, Region, RegionHealthSnapshot, ResidencyGraph, SUSTAINED_WINDOW_SECS,
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

/// Statistical floor on the rolling window: below this many observed requests
/// in the 5s window a clean sample is NOT significant and no degradation signal
/// fires. A low-traffic window containing a 5xx is instead indeterminate, so
/// callers can fail closed rather than claim positive health. Without the floor,
/// 3 consecutive slow 5xx (rate = 100% > 1%, p99 > 300ms, streak >= 3) trip all
/// three triggers at once and freeze EVERY write region-wide — a brownout
/// amplifier, since the probe counts the container's own responses. Override
/// with `FAILOVER_MIN_SAMPLES` (0/invalid → default).
const MIN_SAMPLES_FOR_FAILOVER_DEFAULT: usize = 50;

/// The external liveness signal must be refreshed by the authenticated internal
/// heartbeat route; data-plane traffic and public readiness are deliberately
/// not allowed to make a silent blackhole look alive.
const FAILOVER_HEARTBEAT_STALE_DEFAULT_MS: u64 = 15_000;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Hysteresis: consecutive DEGRADED probe observations required before the
/// guard latches into read-only mode. A single bad observation no longer
/// blocks writes.
pub(crate) const FAILOVER_TRIP_PROBES: u32 = 3;

/// Hysteresis grace: consecutive HEALTHY probe observations required before a
/// latched guard releases. Prevents flapping around the threshold.
pub(crate) const FAILOVER_RECOVER_PROBES: u32 = 5;

static EXTERNAL_HEARTBEAT: OnceLock<Arc<FailoverHeartbeat>> = OnceLock::new();

/// External liveness signal shared by the authenticated internal heartbeat
/// endpoint and the data-plane failover layer. A quiet data plane therefore
/// cannot hide a blackhole: only an authenticated internal scheduler refreshes
/// this timestamp.
#[derive(Debug)]
#[non_exhaustive]
pub struct FailoverHeartbeat {
    last_seen_ms: AtomicU64,
    stale_after_ms: u64,
}

impl FailoverHeartbeat {
    #[must_use]
    fn new(now_ms: u64, stale_after_ms: u64) -> Self {
        Self {
            last_seen_ms: AtomicU64::new(now_ms),
            stale_after_ms,
        }
    }

    /// Refresh from the external health checker, never from a data request.
    fn beat(&self, now_ms: u64) {
        self.last_seen_ms.store(now_ms, Ordering::Release);
    }

    #[must_use]
    fn is_stale(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_seen_ms.load(Ordering::Acquire)) > self.stale_after_ms
    }

    #[cfg(test)]
    #[must_use]
    fn stale_after_ms(&self) -> u64 {
        self.stale_after_ms
    }
}

fn heartbeat_stale_after_ms() -> u64 {
    crate::storage::env_or(
        "FAILOVER_HEARTBEAT_STALE_MS",
        &FAILOVER_HEARTBEAT_STALE_DEFAULT_MS.to_string(),
    )
    .parse::<u64>()
    .ok()
    .filter(|value| *value > 0)
    .unwrap_or(FAILOVER_HEARTBEAT_STALE_DEFAULT_MS)
}

fn register_external_heartbeat(heartbeat: Arc<FailoverHeartbeat>) {
    let _ = EXTERNAL_HEARTBEAT.set(heartbeat);
}

fn internal_auth_ok(expected: Option<&Arc<str>>, headers: &HeaderMap) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    let provided_padded = if provided_bytes.len() >= expected_bytes.len() {
        provided_bytes
            .get(..expected_bytes.len())
            .unwrap_or(&[])
            .to_vec()
    } else {
        let mut padded = provided_bytes.to_vec();
        padded.resize(expected_bytes.len(), 0);
        padded
    };
    let content_ok = expected_bytes.ct_eq(&provided_padded).unwrap_u8();
    let length_ok = u8::from(expected_bytes.len() == provided_bytes.len());
    (content_ok & length_ok) == 1
}

fn apply_authenticated_heartbeat(
    heartbeat: &FailoverHeartbeat,
    expected: Option<&Arc<str>>,
    headers: &HeaderMap,
    now_ms: u64,
) -> StatusCode {
    if !internal_auth_ok(expected, headers) {
        return StatusCode::UNAUTHORIZED;
    }
    heartbeat.beat(now_ms);
    StatusCode::OK
}

async fn internal_heartbeat_handler(headers: HeaderMap) -> StatusCode {
    let Some(heartbeat) = EXTERNAL_HEARTBEAT.get() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let expected = crate::routes::admin::internal_auth_key_from_env();
    apply_authenticated_heartbeat(heartbeat, expected.as_ref(), &headers, now_ms())
}

/// Build the authenticated internal failover-heartbeat route. It is merged
/// outside the public readiness and data-plane middleware stacks. Without the
/// configured internal key, the handler fails closed and cannot refresh state.
pub fn internal_heartbeat_router() -> Router {
    Router::new().route(
        "/_internal/failover/heartbeat",
        post(internal_heartbeat_handler),
    )
}

/// Cap for the production failover audit sink (`with_capacity` ring buffer):
/// region-failover events are exactly the ones worth auditing, but an
/// unbounded `Mutex<Vec>` wired for the life of the process grows without
/// limit. 10k records is ample burst headroom; oldest records are dropped.
const FAILOVER_AUDIT_SINK_CAP: usize = 10_000;

/// Trip/recover hysteresis over the raw per-request health verdict.
///
/// The raw signal ([`RegionHealthSnapshot::evaluate`] via the router) is
/// INSTANTANEOUS — one degraded observation used to block writes immediately.
/// This gate latches only after [`FAILOVER_TRIP_PROBES`] consecutive degraded
/// observations and releases only after [`FAILOVER_RECOVER_PROBES`]
/// consecutive healthy observations (any degraded blip resets the recovery
/// grace). Counters are approximate under contention (fetch-then-load), which
/// is acceptable: the cost of being off by one observation here is one extra
/// request's delay, not a correctness break.
#[derive(Debug, Default)]
pub struct HysteresisGate {
    engaged: AtomicBool,
    degraded_streak: AtomicU32,
    healthy_streak: AtomicU32,
}

impl HysteresisGate {
    /// Feed one raw observation; returns whether failover should be treated
    /// as ACTIVE for this request (latched with hysteresis).
    pub(crate) fn observe(&self, raw_degraded: bool) -> bool {
        if raw_degraded {
            self.degraded_streak.fetch_add(1, Ordering::Relaxed);
            self.healthy_streak.store(0, Ordering::Relaxed);
            if !self.engaged.load(Ordering::Relaxed)
                && self.degraded_streak.load(Ordering::Relaxed) >= FAILOVER_TRIP_PROBES
            {
                self.engaged.store(true, Ordering::Relaxed);
                self.healthy_streak.store(0, Ordering::Relaxed);
            }
        } else if self.engaged.load(Ordering::Relaxed) {
            self.healthy_streak.fetch_add(1, Ordering::Relaxed);
            self.degraded_streak.store(0, Ordering::Relaxed);
            if self.healthy_streak.load(Ordering::Relaxed) >= FAILOVER_RECOVER_PROBES {
                self.engaged.store(false, Ordering::Relaxed);
                self.degraded_streak.store(0, Ordering::Relaxed);
            }
        } else {
            self.degraded_streak.store(0, Ordering::Relaxed);
        }
        self.engaged.load(Ordering::Relaxed)
    }
}

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
    /// Monotonic event identity for each 5xx. Probe observations consume this
    /// identity once, so a single transient cannot be counted repeatedly while
    /// clean probes arrive and the sample remains in the rolling window.
    error_event: Option<u64>,
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
    /// Statistical floor: below this many in-window samples the probe refuses
    /// to make a positive health claim when any 5xx is present; the router
    /// treats that indeterminate result as degraded (fail-closed).
    min_samples: usize,
    next_error_event: AtomicU64,
    last_reported_error_event: AtomicU64,
}

impl RollingMetricsHealthProbe {
    /// Construct an empty probe (defaults to healthy until traffic is observed).
    ///
    /// Reads `FAILOVER_MIN_SAMPLES` once for the sample floor; an unset,
    /// non-numeric, or zero value falls back to
    /// [`MIN_SAMPLES_FOR_FAILOVER_DEFAULT`].
    #[must_use]
    pub fn new() -> Self {
        let min_samples = crate::storage::env_or("FAILOVER_MIN_SAMPLES", "50")
            .parse::<usize>()
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or(MIN_SAMPLES_FOR_FAILOVER_DEFAULT);
        RollingMetricsHealthProbe {
            samples: Mutex::new(VecDeque::new()),
            min_samples,
            next_error_event: AtomicU64::new(0),
            last_reported_error_event: AtomicU64::new(0),
        }
    }

    /// Record one completed request outcome. `is_5xx` marks a server-error
    /// response; `latency_ms` is the wall-clock service time; `now_ms` is the
    /// observation timestamp. Prunes samples older than the sustained window
    /// and bounds retention to [`MAX_SAMPLES`].
    pub fn record_outcome(&self, is_5xx: bool, latency_ms: u64, now_ms: u64) {
        let mut guard = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        let error_event = is_5xx.then(|| {
            self.next_error_event
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1)
        });
        guard.push_back(Sample {
            ts_ms: now_ms,
            is_5xx,
            latency_ms,
            error_event,
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
        if total < self.min_samples {
            // The floor still suppresses tiny clean/transient windows, but it
            // must not turn observed server errors into a positive health
            // claim. Returning Err is deliberate: InMemoryFailoverRouter maps
            // probe errors to a conservative degraded snapshot, while
            // HysteresisGate still requires three consecutive degraded
            // observations before writes are blocked. Empty/clean low-traffic
            // windows remain healthy.
            let newest_error_event = guard
                .iter()
                .filter_map(|sample| sample.error_event)
                .max()
                .unwrap_or_else(|| self.last_reported_error_event.load(Ordering::Acquire));
            let last_reported_error_event = self.last_reported_error_event.load(Ordering::Acquire);
            if newest_error_event > last_reported_error_event
                && self
                    .last_reported_error_event
                    .compare_exchange(
                        last_reported_error_event,
                        newest_error_event,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            {
                return Err(
                    "insufficient samples with observed 5xx; health indeterminate".to_owned(),
                );
            }
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
    heartbeat: Arc<FailoverHeartbeat>,
    residency: ResidencyGraph,
    primary: Option<Region>,
    /// Trip/recover hysteresis over the raw health verdict (B.2).
    hysteresis: Arc<HysteresisGate>,
    /// The production audit sink — a BOUNDED ring buffer so the process
    /// cannot accumulate unbounded failover-audit memory (B.3).
    audit: Arc<InMemoryFailoverAuditSink>,
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
        let state = Self::with_primary(primary);
        register_external_heartbeat(Arc::clone(&state.heartbeat));
        state
    }

    /// Construct with an explicit primary region (test seam + `from_env`).
    #[must_use]
    pub fn with_primary(primary: Option<Region>) -> Self {
        let probe = Arc::new(RollingMetricsHealthProbe::new());
        let heartbeat = Arc::new(FailoverHeartbeat::new(now_ms(), heartbeat_stale_after_ms()));
        // Bounded ring buffer: failover events are the most audit-worthy, but
        // an unbounded in-memory sink grows for the whole process lifetime
        // and never reaches D1 — cap retention instead of leaking.
        let audit = Arc::new(InMemoryFailoverAuditSink::with_capacity(
            FAILOVER_AUDIT_SINK_CAP,
        ));
        let router = Arc::new(InMemoryFailoverRouter::new(
            Arc::clone(&probe) as Arc<dyn HealthProbe>,
            Arc::clone(&audit) as Arc<dyn FailoverAuditSink>,
        ));
        FailoverLayerState {
            router,
            probe,
            heartbeat,
            residency: ResidencyGraph,
            primary,
            hysteresis: Arc::new(HysteresisGate::default()),
            audit,
        }
    }

    /// Test/inspection accessor for the shared probe (drive `record_outcome`).
    #[must_use]
    pub fn probe(&self) -> Arc<RollingMetricsHealthProbe> {
        Arc::clone(&self.probe)
    }

    /// Test/inspection accessor for the external liveness signal. Production
    /// callers must use the authenticated internal heartbeat route.
    #[cfg(test)]
    #[must_use]
    fn heartbeat(&self) -> Arc<FailoverHeartbeat> {
        Arc::clone(&self.heartbeat)
    }

    /// Test/inspection accessor for the hysteresis gate (drive `observe`).
    #[must_use]
    pub fn failover_gate(&self) -> Arc<HysteresisGate> {
        Arc::clone(&self.hysteresis)
    }

    /// Test/inspection accessor for the bounded production audit sink.
    #[must_use]
    pub fn audit_sink(&self) -> Arc<InMemoryFailoverAuditSink> {
        Arc::clone(&self.audit)
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
    // Hysteresis: the raw verdict is instantaneous; the gate latches only
    // after FAILOVER_TRIP_PROBES consecutive degraded observations and
    // releases after FAILOVER_RECOVER_PROBES consecutive healthy ones.
    let heartbeat_stale = state.heartbeat.is_stale(now);
    let degraded = state
        .hysteresis
        .observe(heartbeat_stale || snap.health.requires_failover());

    if degraded {
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
#[path = "failover_tests.rs"]
mod tests;
