//! Global circuit-breaker state machine + multi-signal trip evaluator
//! + hysteresis recovery + HalfOpen 10% deterministic sampler.
//!
//! ## Canonical 3-state lifecycle (sprint contract §5 R-S08-4)
//!
//! - `Closed` — normal; all requests pass through to camadas 1-3.
//! - `Open` — emergency; all requests render 429 GlobalCircuitOpen.
//! - `HalfOpen` — hysteresis recovery; 10% deterministic sample
//!   (observation_id mod 10) probes the system; remaining 90% reject.
//!
//! ## Multi-signal trip evaluator (sprint contract §15 R-S08-004)
//!
//! Per the canonical Netflix Hystrix / Resilience4j circuit-breaker
//! pattern absorbed at WI-S08-005, single-signal trips are a
//! false-positive vulnerability (e.g. an S-09 metrics glitch causing
//! 5xx for ALL request types simultaneously). Trip requires **≥ 2 of 3
//! signals concurrently breached**:
//!
//! - **Signal A**: `error_5xx_rate > 0.5` over the rolling window.
//! - **Signal B**: `p99_latency_us > 5×SLO_threshold_us`.
//! - **Signal C**: `do_error_rate > 0.3` (per-DO actor failure rate).
//!
//! When exactly **one** signal breaches, the orchestrator emits
//! `corelink.global_circuit.single_signal_alarm_total` (SEV-3) and
//! leaves the circuit Closed (investigation-only; not a trip). When ≥ 2
//! signals breach, the canonical `MultiSignalCombined` reason fires.
//!
//! Per Lote 10.8bis P1-2 R5 fix, the `do_error_rate` signal uses the
//! **most-recent** observation's `do_error_rate_5m` (NOT averaged across
//! the window) — averaging produces a moving-average-of-moving-average
//! smoothing that biases toward LATE detection.
//!
//! ## Hysteresis recovery (sprint contract §15 R-S08-004 + canonical
//! Resilience4j absorption)
//!
//! Single-threshold transitions cause flapping. The canonical pattern:
//!
//! - **Open → HalfOpen**: all signals < 90% of trip threshold sustained
//!   ≥ 2min dwell. (`HALFOPEN_DWELL_MS`)
//! - **HalfOpen → Closed**: all signals < 50% of trip threshold AND
//!   ≥ 90% of HalfOpen sample requests succeed AND ≥ 2min dwell.
//! - **HalfOpen → Open**: ANY signal trips again (immediate revert; the
//!   probe surfaced real failure).
//!
//! Sample rate during HalfOpen is fixed at 10% via the
//! [`allow_halfopen_request`] deterministic mod-10 selector — this
//! prevents thundering-herd at full re-engage and sources the
//! `half_open_sample_success_rate` recovery signal.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::audit::{
    CircuitAuditRecord, CircuitAuditSink, CircuitEventType,
};
use crate::error::CircuitError;
use crate::metrics::CircuitMetricsObserver;

/// Canonical 3-state lifecycle per sprint contract §5 R-S08-4.
///
/// `#[non_exhaustive]` reserves additive growth (e.g. forensic-only
/// "drained" state for graceful shutdown) for follow-on WIs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CircuitState {
    /// Normal operation; all requests pass.
    Closed,
    /// Emergency; all requests render 429 GlobalCircuitOpen.
    Open {
        /// Wall-clock instant the trip fired (Unix ms).
        tripped_at_ms: u64,
        /// Why this trip happened.
        reason: TripReason,
    },
    /// Hysteresis recovery; 10% sample probe.
    HalfOpen {
        /// Wall-clock instant of the Open → HalfOpen transition.
        since_ms: u64,
        /// Latest trip reason (carried forward from Open; production
        /// wiring uses for forensic correlation).
        reason: TripReason,
        /// Rolling probe outcome counters (incremented by
        /// [`InMemoryGlobalCircuitBreaker::record_probe_outcome`]).
        probe_success: u32,
        /// Rolling probe-failure counter.
        probe_failure: u32,
    },
}

impl CircuitState {
    /// Discriminator string for the `global_circuit.state` gauge.
    #[must_use]
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::HalfOpen { .. } => "half_open",
            Self::Open { .. } => "open",
        }
    }

    /// Numeric gauge value (0 = closed, 1 = half_open, 2 = open).
    #[must_use]
    pub const fn as_gauge(&self) -> u8 {
        match self {
            Self::Closed => 0,
            Self::HalfOpen { .. } => 1,
            Self::Open { .. } => 2,
        }
    }
}

/// Canonical trip-reason discriminator (sprint contract §15 R-S08-004
/// + WI §6.1.6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TripReason {
    /// Multi-signal combined: ≥ 2 of 3 signals (A=5xx, B=p99, C=DO)
    /// breached simultaneously. The CANONICAL trip path per sprint
    /// contract §15 R-S08-004 (single-signal is investigation-only,
    /// NOT a trip).
    MultiSignalCombined,
    /// Manual admin force-open (S-13 planned drill / load shed). Per
    /// Lote 10.8bis P1-3 R5 fix, this trip reason is the SLI-exclusion
    /// canary — `XRateLimitTypeKind::counts_against_sli(true)` returns
    /// `false` so the planned drill does NOT exhaust error budget.
    ManualOverride {
        /// Admin id (extracted from `AdminCtx` per S-13).
        admin_id: String,
        /// Free-text human-readable reason.
        reason: String,
    },
}

impl TripReason {
    /// Discriminator label for the `trips_total` counter + audit
    /// serialisation.
    #[must_use]
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::MultiSignalCombined => "MultiSignalCombined",
            Self::ManualOverride { .. } => "ManualOverride",
        }
    }
}

/// One health observation fed to the orchestrator (per request OR per
/// 60s cron-tick aggregate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthObservation {
    /// Wall-clock instant of the observation (Unix ms).
    pub timestamp_ms: u64,
    /// Status discriminator.
    pub status: ObservationStatus,
    /// Observed p99 latency in microseconds.
    pub latency_p99_us: u64,
    /// Observed 5min DO error rate ([0.0, 1.0]).
    pub do_error_rate_5m: f64,
}

/// Observation status discriminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObservationStatus {
    /// 2xx / 3xx successful response.
    Success,
    /// 4xx client error.
    ClientError4xx,
    /// 5xx server error.
    ServerError5xx,
}

/// Multi-signal trip thresholds (sprint contract §15 R-S08-004
/// canonical defaults).
#[derive(Clone, Copy, Debug)]
pub struct CircuitThresholds {
    /// Signal A threshold: 5xx rate over the rolling window.
    pub error_5xx_rate: f64,
    /// Signal B threshold: p99 latency in microseconds (5× SLO; SLO
    /// canonical 200ms p99 per slo_catalog).
    pub p99_latency_us: u64,
    /// Signal C threshold: DO error rate ([0.0, 1.0]).
    pub do_error_rate: f64,
    /// Minimum observations in the rolling window to permit a trip
    /// (sprint contract §6.1.6 "insufficient data; do not trip").
    pub min_observations: usize,
}

impl CircuitThresholds {
    /// Canonical defaults per sprint contract §15 R-S08-004.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            error_5xx_rate: 0.5,
            p99_latency_us: 1_000_000,
            do_error_rate: 0.3,
            min_observations: 100,
        }
    }
}

impl Default for CircuitThresholds {
    fn default() -> Self {
        Self::canonical()
    }
}

/// Canonical hysteresis dwell (Open → HalfOpen → Closed). Per sprint
/// contract §15 R-S08-004 absorbed: 2 min sustained signal floor before
/// transitioning.
pub const HALFOPEN_DWELL_MS: u64 = 2 * 60 * 1000;

/// Canonical HalfOpen sample rate (10%).
pub const HALFOPEN_SAMPLE_PCT: u8 = 10;

/// Canonical rolling-window duration (5 min sustained; sprint contract
/// §5 R-S08-4).
pub const ROLLING_WINDOW_MS: u64 = 5 * 60 * 1000;

/// Canonical recovery threshold ratio (90% of trip threshold; Open →
/// HalfOpen).
pub const RECOVERY_RATIO_OPEN_TO_HALFOPEN: f64 = 0.9;

/// Canonical recovery threshold ratio (50% of trip threshold; HalfOpen
/// → Closed).
pub const RECOVERY_RATIO_HALFOPEN_TO_CLOSED: f64 = 0.5;

/// Canonical HalfOpen sample success-rate floor (90%; HalfOpen →
/// Closed).
pub const RECOVERY_SAMPLE_SUCCESS_FLOOR: f64 = 0.9;

/// Deterministic 10% HalfOpen sampler per WI §6.1.8.
///
/// Returns `true` for exactly 10% of `observation_id` values: the first
/// `sample_pct` integers in each contiguous block of 10. The check is
/// deterministic (no randomness) so adversarial clients cannot game
/// the probe schedule per request — the production wiring sources
/// `observation_id` from a monotonic per-instance counter.
#[must_use]
pub const fn allow_halfopen_request(
    observation_id: u64,
    sample_pct: u8,
) -> bool {
    let cap = if sample_pct > 100 { 100 } else { sample_pct };
    (observation_id % 10) < cap as u64 / 10
}

/// Pure-logic multi-signal trip evaluator. Returns `Some(reason)` if
/// the canonical `≥ 2 of 3 signals breached` predicate holds; `None`
/// otherwise. Single-signal-only callers MUST emit
/// `single_signal_alarm_total` SEV-3 (NOT a trip) per sprint contract
/// §15 R-S08-004.
///
/// Per Lote 10.8bis P1-2 R5 fix, `do_error_rate` is sourced from the
/// **most-recent** observation (NOT averaged across overlapping rolling
/// rates which produces moving-average-of-moving-average smoothing
/// biased toward LATE detection).
#[must_use]
pub fn evaluate_signals(
    observations: &VecDeque<HealthObservation>,
    thresholds: &CircuitThresholds,
    now_ms: u64,
) -> SignalEvaluation {
    let cutoff = now_ms.saturating_sub(ROLLING_WINDOW_MS);
    let window: Vec<&HealthObservation> = observations
        .iter()
        .filter(|o| o.timestamp_ms >= cutoff)
        .collect();
    let total = window.len();

    if total < thresholds.min_observations {
        return SignalEvaluation {
            error_5xx_rate: 0.0,
            p99_latency_us: 0,
            do_error_rate: 0.0,
            signal_a: false,
            signal_b: false,
            signal_c: false,
            signals_tripped: 0,
            trip_reason: None,
            insufficient_data: true,
        };
    }

    let total_f = total as f64;
    let error_5xx_count = window
        .iter()
        .filter(|o| matches!(o.status, ObservationStatus::ServerError5xx))
        .count();
    let error_5xx_rate = error_5xx_count as f64 / total_f;

    let p99_latency_us = window
        .iter()
        .map(|o| o.latency_p99_us)
        .max()
        .unwrap_or(0);

    let do_error_rate = window
        .last()
        .map(|o| o.do_error_rate_5m)
        .unwrap_or(0.0);

    let signal_a = error_5xx_rate > thresholds.error_5xx_rate;
    let signal_b = p99_latency_us > thresholds.p99_latency_us;
    let signal_c = do_error_rate > thresholds.do_error_rate;

    let signals_tripped =
        u8::from(signal_a) + u8::from(signal_b) + u8::from(signal_c);

    let trip_reason = if signals_tripped >= 2 {
        Some(TripReason::MultiSignalCombined)
    } else {
        None
    };

    SignalEvaluation {
        error_5xx_rate,
        p99_latency_us,
        do_error_rate,
        signal_a,
        signal_b,
        signal_c,
        signals_tripped,
        trip_reason,
        insufficient_data: false,
    }
}

/// Output of [`evaluate_signals`] — tripping verdict + per-signal
/// diagnostics + raw values for the audit trail.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalEvaluation {
    /// Computed 5xx rate.
    pub error_5xx_rate: f64,
    /// Observed p99 latency (max across the window).
    pub p99_latency_us: u64,
    /// Most-recent DO error rate (per Lote 10.8bis P1-2 R5).
    pub do_error_rate: f64,
    /// Signal A breached (5xx > threshold).
    pub signal_a: bool,
    /// Signal B breached (p99 > threshold).
    pub signal_b: bool,
    /// Signal C breached (DO error > threshold).
    pub signal_c: bool,
    /// Total signals breached (0..=3).
    pub signals_tripped: u8,
    /// Canonical trip reason if `signals_tripped >= 2`; else `None`.
    pub trip_reason: Option<TripReason>,
    /// Window had fewer than `min_observations` samples; trip
    /// evaluation skipped (NOT a trip; investigation noop).
    pub insufficient_data: bool,
}

/// Snapshot of the circuit state + recent observations for diagnostic /
/// dashboard surfacing.
#[derive(Clone, Debug)]
pub struct CircuitStateSnapshot {
    /// Current state.
    pub state: CircuitState,
    /// Region scope.
    pub region: String,
    /// Recent rolling-window signal evaluation.
    pub last_evaluation: Option<SignalEvaluation>,
    /// Number of trips recorded so far.
    pub trips_count: u64,
    /// Number of recoveries recorded so far.
    pub recoveries_count: u64,
}

/// Decision returned to a caller per [`GlobalCircuitBreaker::check`]
/// invocation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CircuitDecision {
    /// Caller MAY proceed (Closed; OR HalfOpen and the observation_id
    /// landed in the 10% sample).
    Allow {
        /// Discriminator label snapshot at decision time.
        state_label: &'static str,
    },
    /// Caller MUST render 429 GlobalCircuitOpen (Open; OR HalfOpen and
    /// the observation_id was outside the 10% sample).
    Reject {
        /// Trip reason carried forward from the active Open / HalfOpen
        /// state.
        reason: TripReason,
        /// `true` if `reason` is a `ManualOverride` (sourced from
        /// [`TripReason::ManualOverride`]); pinning this here lets the
        /// caller skip the SLI-distinction match and pass directly to
        /// [`crate::headers::XRateLimitTypeKind::counts_against_sli`].
        is_manual_override: bool,
    },
}

/// Trait surface for the global circuit breaker. Production wiring
/// composes the CF DO singleton `GlobalRateLimiter-<region>`; tests use
/// [`InMemoryGlobalCircuitBreaker`] for deterministic assertions.
pub trait GlobalCircuitBreaker: Send + Sync + core::fmt::Debug {
    /// Render the per-request circuit decision.
    ///
    /// `observation_id` is the per-instance monotonic counter sourced
    /// from the production wiring (canonical
    /// `corelink_time::request_id_seq()` — out-of-scope here). The
    /// decision factors:
    ///
    /// - State Closed → `Allow`.
    /// - State Open → `Reject` (429 GlobalCircuitOpen).
    /// - State HalfOpen + observation_id in 10% sample → `Allow`.
    /// - State HalfOpen + observation_id outside sample → `Reject`.
    ///
    /// Emits `corelink.circuit.request_rejected` audit on every
    /// `Reject` (informational lineage; SLI distinction critical).
    /// Audit emit is fail-closed: returns `Err(Audit)` → caller maps
    /// to 5xx (NOT a 429; preserves
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Surface as [`CircuitError`].
    fn check(
        &self,
        observation_id: u64,
        now_ms: u64,
    ) -> Result<CircuitDecision, CircuitError>;

    /// Record a fresh health observation + re-evaluate trip predicate.
    /// Drives the canonical state machine:
    ///
    /// - Closed + multi-signal → Open (audit `tripped`; metrics).
    /// - Closed + single-signal → SEV-3 alarm metric only.
    /// - Open + recovery threshold + dwell → HalfOpen (audit
    ///   `half_open_probe`; metrics).
    /// - HalfOpen + signal re-trip → Open (immediate revert).
    /// - HalfOpen + recovery + sample success → Closed (audit
    ///   `closed_recovery`; metrics).
    ///
    /// # Errors
    ///
    /// Surface as [`CircuitError`].
    fn record_observation(
        &self,
        observation: HealthObservation,
        now_ms: u64,
    ) -> Result<CircuitState, CircuitError>;

    /// Record a HalfOpen probe outcome (success / failure of the 10%
    /// sample request that was allowed through). Drives the
    /// `half_open_sample_success_rate` recovery signal. No-op when not
    /// in HalfOpen.
    ///
    /// # Errors
    ///
    /// Surface as [`CircuitError`].
    fn record_probe_outcome(
        &self,
        success: bool,
    ) -> Result<(), CircuitError>;

    /// Manual override (admin S-13): force the circuit Open or Closed.
    /// Audit + metrics fail-closed; SEV-2 alert per sprint contract
    /// §15 R-S08-004.
    ///
    /// # Errors
    ///
    /// Surface as [`CircuitError`]. Production wiring's AdminCtx
    /// extraction maps an unauthorized request to
    /// [`CircuitError::AdminAuthFailed`] before reaching this method.
    fn manual_override(
        &self,
        target: ManualOverrideTarget,
        admin_id: String,
        reason: String,
        now_ms: u64,
    ) -> Result<CircuitState, CircuitError>;

    /// Snapshot the current state + summary stats (for DASH-RATE
    /// widget; admin diagnostic surface).
    ///
    /// # Errors
    ///
    /// Surface as [`CircuitError`].
    fn snapshot(&self) -> Result<CircuitStateSnapshot, CircuitError>;
}

/// Manual-override target state (open → force trip; closed → force
/// recovery).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ManualOverrideTarget {
    /// Force the circuit Open (planned drill / emergency load shed).
    Open,
    /// Force the circuit Closed (override a false-positive trip).
    Closed,
}

impl ManualOverrideTarget {
    /// Discriminator label.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

/// Canonical observation buffer cap. Per WI §6.1.5 the DO alarm 60s
/// sweep trims observations older than the rolling window; the
/// in-memory orchestrator caps the buffer to 10× the rolling window's
/// theoretical maximum (1 obs/req × 10 req/sec × 300s = 3000) to avoid
/// unbounded growth on a hot region.
pub const OBSERVATION_BUFFER_CAP: usize = 3_000;

/// In-memory global circuit breaker (the canonical pure-logic
/// orchestrator). Mirrors the production CF DO actor model: per-instance
/// `Mutex` serialises concurrent calls so the state-machine cannot be
/// torn.
///
/// Per the autonomous execution charter, the orchestrator state is held
/// in a per-instance `Arc<Mutex<CircuitInner>>` (NOT a process-global
/// `static LazyLock<Mutex<>>`). Tests instantiate fresh orchestrators
/// per case so the harness cannot leak state.
pub struct InMemoryGlobalCircuitBreaker<A, M>
where
    A: CircuitAuditSink,
    M: CircuitMetricsObserver,
{
    audit: Arc<A>,
    metrics: Arc<M>,
    region: String,
    thresholds: CircuitThresholds,
    inner: Arc<Mutex<CircuitInner>>,
}

struct CircuitInner {
    state: CircuitState,
    observations: VecDeque<HealthObservation>,
    last_evaluation: Option<SignalEvaluation>,
    trips_count: u64,
    recoveries_count: u64,
}

impl<A, M> core::fmt::Debug for InMemoryGlobalCircuitBreaker<A, M>
where
    A: CircuitAuditSink,
    M: CircuitMetricsObserver,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryGlobalCircuitBreaker")
            .field("region", &self.region)
            .field("thresholds", &self.thresholds)
            .finish_non_exhaustive()
    }
}

impl<A, M> InMemoryGlobalCircuitBreaker<A, M>
where
    A: CircuitAuditSink,
    M: CircuitMetricsObserver,
{
    /// Construct with the canonical default thresholds.
    #[must_use]
    pub fn with_defaults(
        audit: Arc<A>,
        metrics: Arc<M>,
        region: impl Into<String>,
    ) -> Self {
        Self::new(audit, metrics, region, CircuitThresholds::canonical())
    }

    /// Construct with explicit thresholds.
    #[must_use]
    pub fn new(
        audit: Arc<A>,
        metrics: Arc<M>,
        region: impl Into<String>,
        thresholds: CircuitThresholds,
    ) -> Self {
        Self {
            audit,
            metrics,
            region: region.into(),
            thresholds,
            inner: Arc::new(Mutex::new(CircuitInner {
                state: CircuitState::Closed,
                observations: VecDeque::new(),
                last_evaluation: None,
                trips_count: 0,
                recoveries_count: 0,
            })),
        }
    }

    /// Region scope.
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Active thresholds.
    #[must_use]
    pub const fn thresholds(&self) -> &CircuitThresholds {
        &self.thresholds
    }

    fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, CircuitInner>, CircuitError> {
        self.inner.lock().map_err(|_| {
            CircuitError::Backend("circuit inner mutex poisoned".to_string())
        })
    }

    fn audit_fail_closed(
        &self,
        record: CircuitAuditRecord,
    ) -> Result<(), CircuitError> {
        self.audit.emit(record).map_err(CircuitError::Audit)
    }
}

fn signals_below_ratio(
    eval: &SignalEvaluation,
    thresholds: &CircuitThresholds,
    ratio: f64,
) -> bool {
    eval.error_5xx_rate < thresholds.error_5xx_rate * ratio
        && (eval.p99_latency_us as f64)
            < (thresholds.p99_latency_us as f64) * ratio
        && eval.do_error_rate < thresholds.do_error_rate * ratio
}

fn any_signal_breaches(
    eval: &SignalEvaluation,
    thresholds: &CircuitThresholds,
) -> bool {
    eval.error_5xx_rate > thresholds.error_5xx_rate
        || eval.p99_latency_us > thresholds.p99_latency_us
        || eval.do_error_rate > thresholds.do_error_rate
}

impl<A, M> GlobalCircuitBreaker for InMemoryGlobalCircuitBreaker<A, M>
where
    A: CircuitAuditSink,
    M: CircuitMetricsObserver,
{
    fn check(
        &self,
        observation_id: u64,
        now_ms: u64,
    ) -> Result<CircuitDecision, CircuitError> {
        let inner = self.lock_inner()?;

        match inner.state.clone() {
            CircuitState::Closed => Ok(CircuitDecision::Allow {
                state_label: "closed",
            }),
            CircuitState::Open { reason, .. } => {
                let is_manual_override =
                    matches!(reason, TripReason::ManualOverride { .. });
                self.audit_fail_closed(CircuitAuditRecord {
                    event_type: CircuitEventType::RequestRejected,
                    region: self.region.clone(),
                    trip_reason: Some(reason.as_label().to_string()),
                    now_ms,
                    created_by_request_id: format!("obs-{observation_id}"),
                    manual_override_admin_id: None,
                })?;
                drop(inner);
                let _ = self.metrics.record_within_quota(
                    "global_circuit_open",
                    &self.region,
                );
                Ok(CircuitDecision::Reject {
                    reason,
                    is_manual_override,
                })
            }
            CircuitState::HalfOpen { reason, .. } => {
                let allowed = allow_halfopen_request(
                    observation_id,
                    HALFOPEN_SAMPLE_PCT,
                );
                if allowed {
                    Ok(CircuitDecision::Allow {
                        state_label: "half_open",
                    })
                } else {
                    let is_manual_override = matches!(
                        reason,
                        TripReason::ManualOverride { .. }
                    );
                    self.audit_fail_closed(CircuitAuditRecord {
                        event_type: CircuitEventType::RequestRejected,
                        region: self.region.clone(),
                        trip_reason: Some(reason.as_label().to_string()),
                        now_ms,
                        created_by_request_id: format!(
                            "obs-{observation_id}"
                        ),
                        manual_override_admin_id: None,
                    })?;
                    drop(inner);
                    let _ = self.metrics.record_within_quota(
                        "global_circuit_open",
                        &self.region,
                    );
                    Ok(CircuitDecision::Reject {
                        reason,
                        is_manual_override,
                    })
                }
            }
        }
    }

    fn record_observation(
        &self,
        observation: HealthObservation,
        now_ms: u64,
    ) -> Result<CircuitState, CircuitError> {
        let mut inner = self.lock_inner()?;
        inner.observations.push_back(observation);
        if inner.observations.len() > OBSERVATION_BUFFER_CAP {
            inner.observations.pop_front();
        }
        let cutoff = now_ms.saturating_sub(ROLLING_WINDOW_MS);
        while let Some(front) = inner.observations.front() {
            if front.timestamp_ms < cutoff {
                inner.observations.pop_front();
            } else {
                break;
            }
        }
        let eval = evaluate_signals(
            &inner.observations,
            &self.thresholds,
            now_ms,
        );

        let new_state = match inner.state.clone() {
            CircuitState::Closed => {
                if let Some(reason) = eval.trip_reason.clone() {
                    self.audit_fail_closed(CircuitAuditRecord {
                        event_type: CircuitEventType::Tripped,
                        region: self.region.clone(),
                        trip_reason: Some(reason.as_label().to_string()),
                        now_ms,
                        created_by_request_id: format!("obs-{now_ms}"),
                        manual_override_admin_id: None,
                    })?;
                    inner.trips_count = inner.trips_count.saturating_add(1);
                    let _ = self.metrics.record_trip(
                        &self.region,
                        reason.as_label(),
                    );
                    CircuitState::Open {
                        tripped_at_ms: now_ms,
                        reason,
                    }
                } else {
                    if !eval.insufficient_data && eval.signals_tripped == 1 {
                        let signal_label = if eval.signal_a {
                            "error_5xx"
                        } else if eval.signal_b {
                            "p99_latency"
                        } else {
                            "do_error_rate"
                        };
                        let _ = self.metrics.record_single_signal_alarm(
                            &self.region,
                            signal_label,
                        );
                    }
                    CircuitState::Closed
                }
            }
            CircuitState::Open {
                tripped_at_ms,
                reason,
            } => {
                let dwell_met = now_ms.saturating_sub(tripped_at_ms)
                    >= HALFOPEN_DWELL_MS;
                let signals_recovered = !eval.insufficient_data
                    && signals_below_ratio(
                        &eval,
                        &self.thresholds,
                        RECOVERY_RATIO_OPEN_TO_HALFOPEN,
                    );
                if dwell_met
                    && signals_recovered
                    && !matches!(reason, TripReason::ManualOverride { .. })
                {
                    self.audit_fail_closed(CircuitAuditRecord {
                        event_type: CircuitEventType::HalfOpenProbe,
                        region: self.region.clone(),
                        trip_reason: Some(reason.as_label().to_string()),
                        now_ms,
                        created_by_request_id: format!("obs-{now_ms}"),
                        manual_override_admin_id: None,
                    })?;
                    CircuitState::HalfOpen {
                        since_ms: now_ms,
                        reason,
                        probe_success: 0,
                        probe_failure: 0,
                    }
                } else {
                    CircuitState::Open {
                        tripped_at_ms,
                        reason,
                    }
                }
            }
            CircuitState::HalfOpen {
                since_ms,
                reason,
                probe_success,
                probe_failure,
            } => {
                if !eval.insufficient_data
                    && any_signal_breaches(&eval, &self.thresholds)
                {
                    let revert_reason =
                        eval.trip_reason.clone().unwrap_or(reason);
                    self.audit_fail_closed(CircuitAuditRecord {
                        event_type: CircuitEventType::Tripped,
                        region: self.region.clone(),
                        trip_reason: Some(
                            revert_reason.as_label().to_string(),
                        ),
                        now_ms,
                        created_by_request_id: format!("obs-{now_ms}"),
                        manual_override_admin_id: None,
                    })?;
                    inner.trips_count = inner.trips_count.saturating_add(1);
                    let _ = self.metrics.record_trip(
                        &self.region,
                        revert_reason.as_label(),
                    );
                    CircuitState::Open {
                        tripped_at_ms: now_ms,
                        reason: revert_reason,
                    }
                } else {
                    let dwell_met = now_ms.saturating_sub(since_ms)
                        >= HALFOPEN_DWELL_MS;
                    let signals_recovered = !eval.insufficient_data
                        && signals_below_ratio(
                            &eval,
                            &self.thresholds,
                            RECOVERY_RATIO_HALFOPEN_TO_CLOSED,
                        );
                    let total_probes = probe_success + probe_failure;
                    let success_rate = if total_probes == 0 {
                        1.0
                    } else {
                        probe_success as f64 / total_probes as f64
                    };
                    let probe_floor_met =
                        success_rate >= RECOVERY_SAMPLE_SUCCESS_FLOOR;
                    if dwell_met && signals_recovered && probe_floor_met {
                        self.audit_fail_closed(CircuitAuditRecord {
                            event_type: CircuitEventType::ClosedRecovery,
                            region: self.region.clone(),
                            trip_reason: None,
                            now_ms,
                            created_by_request_id: format!("obs-{now_ms}"),
                            manual_override_admin_id: None,
                        })?;
                        inner.recoveries_count =
                            inner.recoveries_count.saturating_add(1);
                        let _ = self.metrics.record_recovery(&self.region);
                        CircuitState::Closed
                    } else {
                        CircuitState::HalfOpen {
                            since_ms,
                            reason,
                            probe_success,
                            probe_failure,
                        }
                    }
                }
            }
        };

        inner.state = new_state.clone();
        inner.last_evaluation = Some(eval);
        Ok(new_state)
    }

    fn record_probe_outcome(
        &self,
        success: bool,
    ) -> Result<(), CircuitError> {
        let mut inner = self.lock_inner()?;
        if let CircuitState::HalfOpen {
            since_ms,
            reason,
            probe_success,
            probe_failure,
        } = inner.state.clone()
        {
            let new_success =
                if success { probe_success.saturating_add(1) } else { probe_success };
            let new_failure =
                if success { probe_failure } else { probe_failure.saturating_add(1) };
            inner.state = CircuitState::HalfOpen {
                since_ms,
                reason,
                probe_success: new_success,
                probe_failure: new_failure,
            };
        }
        Ok(())
    }

    fn manual_override(
        &self,
        target: ManualOverrideTarget,
        admin_id: String,
        reason: String,
        now_ms: u64,
    ) -> Result<CircuitState, CircuitError> {
        if admin_id.is_empty() {
            return Err(CircuitError::AdminAuthFailed(
                "admin_id is empty".to_string(),
            ));
        }

        self.audit_fail_closed(CircuitAuditRecord {
            event_type: CircuitEventType::ManualOverride,
            region: self.region.clone(),
            trip_reason: Some("ManualOverride".to_string()),
            now_ms,
            created_by_request_id: format!("admin-{admin_id}"),
            manual_override_admin_id: Some(admin_id.clone()),
        })?;

        let mut inner = self.lock_inner()?;
        let new_state = match target {
            ManualOverrideTarget::Open => {
                inner.trips_count = inner.trips_count.saturating_add(1);
                CircuitState::Open {
                    tripped_at_ms: now_ms,
                    reason: TripReason::ManualOverride { admin_id, reason },
                }
            }
            ManualOverrideTarget::Closed => {
                if matches!(inner.state, CircuitState::Open { .. } | CircuitState::HalfOpen { .. }) {
                    inner.recoveries_count =
                        inner.recoveries_count.saturating_add(1);
                }
                CircuitState::Closed
            }
        };

        inner.state = new_state.clone();
        drop(inner);

        let _ = self
            .metrics
            .record_manual_override(&self.region, target.as_label());
        Ok(new_state)
    }

    fn snapshot(&self) -> Result<CircuitStateSnapshot, CircuitError> {
        let inner = self.lock_inner()?;
        Ok(CircuitStateSnapshot {
            state: inner.state.clone(),
            region: self.region.clone(),
            last_evaluation: inner.last_evaluation.clone(),
            trips_count: inner.trips_count,
            recoveries_count: inner.recoveries_count,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{
        FailingCircuitAuditSink, InMemoryCircuitAuditSink,
    };
    use crate::metrics::{CircuitMetricKind, InMemoryCircuitMetrics};

    type Brk = InMemoryGlobalCircuitBreaker<
        InMemoryCircuitAuditSink,
        InMemoryCircuitMetrics,
    >;

    fn fresh() -> (
        Brk,
        Arc<InMemoryCircuitAuditSink>,
        Arc<InMemoryCircuitMetrics>,
    ) {
        let audit = Arc::new(InMemoryCircuitAuditSink::new());
        let metrics = Arc::new(InMemoryCircuitMetrics::new());
        let breaker = InMemoryGlobalCircuitBreaker::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            "iad",
        );
        (breaker, audit, metrics)
    }

    fn obs(now: u64, status: ObservationStatus) -> HealthObservation {
        HealthObservation {
            timestamp_ms: now,
            status,
            latency_p99_us: 100_000,
            do_error_rate_5m: 0.01,
        }
    }

    fn flood(
        breaker: &Brk,
        count: usize,
        status_5xx: usize,
        latency_us: u64,
        do_err: f64,
        base_now: u64,
    ) -> u64 {
        let mut now = base_now;
        for i in 0..count {
            let status = if i < status_5xx {
                ObservationStatus::ServerError5xx
            } else {
                ObservationStatus::Success
            };
            let mut o = obs(now, status);
            o.latency_p99_us = latency_us;
            o.do_error_rate_5m = do_err;
            breaker.record_observation(o, now).unwrap();
            now += 1;
        }
        now
    }

    // Base timestamp larger than ROLLING_WINDOW_MS so old observations
    // can age out of the rolling window via the cutoff (saturating_sub
    // returns 0 when now < ROLLING_WINDOW_MS, preventing aging).
    const BASE_NOW: u64 = 1_000_000;

    // ---- Closed → Open transitions -----------------------------------

    #[test]
    fn closed_state_allows_all_requests() {
        let (breaker, _, _) = fresh();
        for i in 0..50 {
            let d = breaker.check(i, BASE_NOW).unwrap();
            assert!(matches!(d, CircuitDecision::Allow { .. }));
        }
    }

    #[test]
    fn multi_signal_combined_trips_to_open() {
        let (breaker, audit, metrics) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.01, BASE_NOW);
        let state = breaker
            .record_observation(
                HealthObservation {
                    timestamp_ms: now,
                    status: ObservationStatus::ServerError5xx,
                    latency_p99_us: 6_000_000,
                    do_error_rate_5m: 0.5,
                },
                now,
            )
            .unwrap();
        assert!(matches!(state, CircuitState::Open { .. }));
        assert_eq!(audit.snapshot_of(CircuitEventType::Tripped).len(), 1);
        assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 1);
    }

    #[test]
    fn single_signal_only_does_not_trip_emits_sev3_alarm() {
        let (breaker, audit, metrics) = fresh();
        let _ = flood(&breaker, 200, 150, 100_000, 0.01, BASE_NOW);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Closed));
        assert_eq!(audit.snapshot_of(CircuitEventType::Tripped).len(), 0);
        assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 0);
        assert!(
            metrics.counter_total(
                CircuitMetricKind::SingleSignalAlarmTotal
            ) > 0
        );
    }

    #[test]
    fn insufficient_data_does_not_trip() {
        let (breaker, _, metrics) = fresh();
        let _ = flood(&breaker, 50, 50, 6_000_000, 0.5, BASE_NOW);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Closed));
        assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 0);
        assert_eq!(
            metrics.counter_total(
                CircuitMetricKind::SingleSignalAlarmTotal
            ),
            0
        );
    }

    // ---- Open behaviour ---------------------------------------------

    #[test]
    fn open_state_rejects_all_requests_with_audit_per_check() {
        let (breaker, audit, metrics) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        for i in 0..10 {
            let d = breaker.check(i, now + i).unwrap();
            assert!(matches!(d, CircuitDecision::Reject { .. }));
        }
        assert_eq!(
            audit.snapshot_of(CircuitEventType::RequestRejected).len(),
            10
        );
        assert_eq!(
            metrics.counter_total(CircuitMetricKind::WithinQuotaTotal),
            10
        );
    }

    #[test]
    fn open_reject_carries_trip_reason() {
        let (breaker, _, _) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        let d = breaker.check(7, now).unwrap();
        match d {
            CircuitDecision::Reject {
                reason,
                is_manual_override,
            } => {
                assert!(matches!(reason, TripReason::MultiSignalCombined));
                assert!(!is_manual_override);
            }
            CircuitDecision::Allow { .. } => panic!("expected reject"),
        }
    }

    // ---- Open → HalfOpen → Closed recovery --------------------------

    #[test]
    fn open_to_halfopen_after_dwell_with_recovered_signals() {
        let (breaker, audit, _) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        // Advance past ROLLING_WINDOW_MS so the old bad observations
        // age out of the window before we feed clean ones (otherwise
        // the max-latency old obs persistently breaches Signal B).
        let later = now + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
        let snap = breaker.snapshot().unwrap();
        assert!(
            matches!(snap.state, CircuitState::HalfOpen { .. }),
            "expected HalfOpen got {:?}",
            snap.state
        );
        assert!(
            !audit
                .snapshot_of(CircuitEventType::HalfOpenProbe)
                .is_empty()
        );
    }

    #[test]
    fn halfopen_immediate_revert_on_signal_re_trip() {
        let (breaker, audit, _) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        let later = now + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
        let later2 = later + 200 + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 150, 6_000_000, 0.5, later2);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Open { .. }));
        let trip_count =
            audit.snapshot_of(CircuitEventType::Tripped).len();
        assert!(trip_count >= 2, "expected ≥ 2 trips; got {trip_count}");
    }

    #[test]
    fn halfopen_to_closed_after_dwell_recovered_signals_and_probe_floor() {
        let (breaker, audit, metrics) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        let mid = now + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 0, 100_000, 0.01, mid);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
        for _ in 0..100 {
            breaker.record_probe_outcome(true).unwrap();
        }
        let later = mid + 200 + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 0, 50_000, 0.01, later);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Closed));
        assert_eq!(
            audit.snapshot_of(CircuitEventType::ClosedRecovery).len(),
            1
        );
        assert!(metrics.counter_total(CircuitMetricKind::RecoveriesTotal) >= 1);
    }

    // ---- HalfOpen 10% sample ----------------------------------------

    #[test]
    fn halfopen_sample_exactly_10_percent_deterministic() {
        let mut allowed = 0_u64;
        for i in 0..1000 {
            if allow_halfopen_request(i, HALFOPEN_SAMPLE_PCT) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 100);
    }

    #[test]
    fn halfopen_request_decision_uses_10_pct_sampler() {
        let (breaker, _, _) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        let later = now + ROLLING_WINDOW_MS + 1;
        let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
        let mut allowed = 0;
        let mut rejected = 0;
        for i in 0..1000 {
            match breaker.check(i, later + 1).unwrap() {
                CircuitDecision::Allow { .. } => allowed += 1,
                CircuitDecision::Reject { .. } => rejected += 1,
            }
        }
        assert_eq!(allowed, 100);
        assert_eq!(rejected, 900);
    }

    // ---- Manual override --------------------------------------------

    #[test]
    fn manual_override_force_open_carries_admin_id() {
        let (breaker, audit, metrics) = fresh();
        let new_state = breaker
            .manual_override(
                ManualOverrideTarget::Open,
                "admin-7".to_string(),
                "planned drill".to_string(),
                100,
            )
            .unwrap();
        match new_state {
            CircuitState::Open { reason, .. } => {
                assert!(matches!(
                    reason,
                    TripReason::ManualOverride { .. }
                ));
            }
            _ => panic!("expected Open"),
        }
        assert_eq!(
            audit.snapshot_of(CircuitEventType::ManualOverride).len(),
            1
        );
        assert_eq!(
            metrics.counter_for_labels(
                CircuitMetricKind::ManualOverrideTotal,
                "iad",
                "open"
            ),
            1
        );
    }

    #[test]
    fn manual_override_force_closed_recovery() {
        let (breaker, audit, _) = fresh();
        breaker
            .manual_override(
                ManualOverrideTarget::Open,
                "admin-1".to_string(),
                "drill".to_string(),
                100,
            )
            .unwrap();
        let new_state = breaker
            .manual_override(
                ManualOverrideTarget::Closed,
                "admin-1".to_string(),
                "drill done".to_string(),
                200,
            )
            .unwrap();
        assert!(matches!(new_state, CircuitState::Closed));
        assert_eq!(
            audit.snapshot_of(CircuitEventType::ManualOverride).len(),
            2
        );
    }

    #[test]
    fn manual_override_empty_admin_returns_auth_failed() {
        let (breaker, _, _) = fresh();
        let err = breaker
            .manual_override(
                ManualOverrideTarget::Open,
                String::new(),
                "x".to_string(),
                100,
            )
            .unwrap_err();
        assert!(matches!(err, CircuitError::AdminAuthFailed(_)));
    }

    #[test]
    fn manual_override_open_then_check_carries_is_manual_override_true() {
        let (breaker, _, _) = fresh();
        breaker
            .manual_override(
                ManualOverrideTarget::Open,
                "admin-1".to_string(),
                "drill".to_string(),
                100,
            )
            .unwrap();
        let d = breaker.check(0, 200).unwrap();
        match d {
            CircuitDecision::Reject {
                is_manual_override, ..
            } => {
                assert!(is_manual_override);
            }
            _ => panic!("expected reject"),
        }
    }

    // ---- Audit fail-closed -------------------------------------------

    #[test]
    fn audit_failure_aborts_check_decision() {
        let audit = Arc::new(FailingCircuitAuditSink::new());
        let metrics = Arc::new(InMemoryCircuitMetrics::new());
        let breaker = InMemoryGlobalCircuitBreaker::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            "iad",
        );
        // Closed → no audit emit on `check` → no failure.
        let d = breaker.check(0, 100).unwrap();
        assert!(matches!(d, CircuitDecision::Allow { .. }));
        // Force Open via manual override; this audit-emits → fails.
        let err = breaker
            .manual_override(
                ManualOverrideTarget::Open,
                "admin-1".to_string(),
                "x".to_string(),
                100,
            )
            .unwrap_err();
        assert!(matches!(err, CircuitError::Audit(_)));
        // State should NOT have transitioned (audit emit BEFORE state mutation).
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Closed));
    }

    // ---- F-001 closure -----------------------------------------------

    #[test]
    fn separate_breaker_instances_have_independent_state() {
        let (b1, _, _) = fresh();
        let (b2, _, _) = fresh();
        b1.manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "x".to_string(),
            100,
        )
        .unwrap();
        let s1 = b1.snapshot().unwrap();
        let s2 = b2.snapshot().unwrap();
        assert!(matches!(s1.state, CircuitState::Open { .. }));
        assert!(matches!(s2.state, CircuitState::Closed));
    }

    #[test]
    fn record_probe_outcome_is_noop_when_not_halfopen() {
        let (breaker, _, _) = fresh();
        breaker.record_probe_outcome(true).unwrap();
        breaker.record_probe_outcome(false).unwrap();
        let snap = breaker.snapshot().unwrap();
        assert!(matches!(snap.state, CircuitState::Closed));
    }

    #[test]
    fn snapshot_carries_trips_and_recoveries_counts() {
        let (breaker, _, _) = fresh();
        let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
        let snap = breaker.snapshot().unwrap();
        assert_eq!(snap.trips_count, 1);
        assert_eq!(snap.recoveries_count, 0);
        assert_eq!(snap.region, "iad");
        let _ = now;
    }

    // ---- evaluate_signals pure logic --------------------------------

    #[test]
    fn evaluate_signals_pure_below_min_observations_returns_insufficient() {
        let mut buf = VecDeque::new();
        for t in 0..50 {
            buf.push_back(obs(t, ObservationStatus::ServerError5xx));
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            100,
        );
        assert!(eval.insufficient_data);
        assert!(eval.trip_reason.is_none());
    }

    #[test]
    fn evaluate_signals_two_signal_breach_returns_multi_signal() {
        let mut buf = VecDeque::new();
        for t in 0..200 {
            let mut o = obs(
                t,
                if t < 150 {
                    ObservationStatus::ServerError5xx
                } else {
                    ObservationStatus::Success
                },
            );
            o.latency_p99_us = 6_000_000;
            buf.push_back(o);
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        assert!(eval.signal_a);
        assert!(eval.signal_b);
        assert!(matches!(
            eval.trip_reason,
            Some(TripReason::MultiSignalCombined)
        ));
    }

    #[test]
    fn evaluate_signals_three_signal_breach_returns_multi_signal() {
        let mut buf = VecDeque::new();
        for t in 0..200 {
            let mut o = obs(
                t,
                if t < 150 {
                    ObservationStatus::ServerError5xx
                } else {
                    ObservationStatus::Success
                },
            );
            o.latency_p99_us = 6_000_000;
            o.do_error_rate_5m = 0.5;
            buf.push_back(o);
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        assert!(eval.signal_a);
        assert!(eval.signal_b);
        assert!(eval.signal_c);
        assert_eq!(eval.signals_tripped, 3);
    }

    #[test]
    fn evaluate_signals_zero_signal_returns_none() {
        let mut buf = VecDeque::new();
        for t in 0..200 {
            buf.push_back(obs(t, ObservationStatus::Success));
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        assert!(!eval.signal_a);
        assert!(!eval.signal_b);
        assert!(!eval.signal_c);
        assert_eq!(eval.signals_tripped, 0);
        assert!(eval.trip_reason.is_none());
    }

    #[test]
    fn evaluate_signals_uses_most_recent_do_error_rate() {
        // Per Lote 10.8bis P1-2 R5 fix: do_error_rate from the LAST
        // observation only (NOT averaged across the window).
        let mut buf = VecDeque::new();
        for t in 0..200 {
            let mut o = obs(t, ObservationStatus::Success);
            o.do_error_rate_5m = if t == 199 { 0.9 } else { 0.0 };
            buf.push_back(o);
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        assert_eq!(eval.do_error_rate, 0.9);
        assert!(eval.signal_c);
    }

    #[test]
    fn allow_halfopen_request_clamps_sample_pct_at_100() {
        for i in 0..100_u64 {
            assert!(allow_halfopen_request(i, 200));
        }
    }
}
