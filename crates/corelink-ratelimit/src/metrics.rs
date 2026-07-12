//! Rate-limit metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S08-001 §6.1.10 freezes the canonical 7 metrics every emission
//! path uses (CloudEvent dotted naming spec → underscored Prometheus
//! exposed name per `prometheus.io/docs/practices/naming/`):
//!
//! - `corelink.ratelimit.check_total{tenant_id, dimension, result=allowed|denied}` (counter)
//! - `corelink.ratelimit.tokens_remaining{tenant_id, dimension}` (gauge)
//! - `corelink.ratelimit.refill_rate{tenant_id, dimension}` (gauge)
//! - `corelink.ratelimit.plan_sync_lag_ms{tenant_id}` (gauge; alert SEV-2 if > 5min)
//! - `corelink.ratelimit.do_cold_start_total{region}` (counter)
//! - `corelink.ratelimit.middleware_duration_us{result}` (histogram-like)
//! - `corelink.ratelimit.cross_tenant_violation_total` (counter; **alert SEV-1 if > 0**)

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::key::KeyDimension;

/// Canonical metric names per WI §6.1.10 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 7] {
    &[
        "corelink.ratelimit.check_total",
        "corelink.ratelimit.tokens_remaining",
        "corelink.ratelimit.refill_rate",
        "corelink.ratelimit.plan_sync_lag_ms",
        "corelink.ratelimit.do_cold_start_total",
        "corelink.ratelimit.middleware_duration_us",
        "corelink.ratelimit.cross_tenant_violation_total",
    ]
}

/// Errors surfaced by [`RateLimitMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RateLimitMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("ratelimit metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink and
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RateLimitMetricKind {
    /// `corelink.ratelimit.check_total`.
    CheckTotal,
    /// `corelink.ratelimit.tokens_remaining` — gauge.
    TokensRemaining,
    /// `corelink.ratelimit.refill_rate` — gauge (current tier).
    RefillRate,
    /// `corelink.ratelimit.plan_sync_lag_ms` — gauge (alert SEV-2 if
    /// > 300_000 ms; INV-RATE-LIMIT-PROPORTIONALITY canary).
    PlanSyncLagMs,
    /// `corelink.ratelimit.do_cold_start_total` — counter (informational).
    DoColdStartTotal,
    /// `corelink.ratelimit.middleware_duration_us` — histogram-like
    /// (SLO target p99 < 3000 us per spec_contract §10.s08.2).
    MiddlewareDurationUs,
    /// `corelink.ratelimit.cross_tenant_violation_total` — counter;
    /// alert SEV-1 if > 0 (INV-AVAIL-ISOLATION canary).
    CrossTenantViolationTotal,
}

impl RateLimitMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckTotal => "corelink.ratelimit.check_total",
            Self::TokensRemaining => "corelink.ratelimit.tokens_remaining",
            Self::RefillRate => "corelink.ratelimit.refill_rate",
            Self::PlanSyncLagMs => "corelink.ratelimit.plan_sync_lag_ms",
            Self::DoColdStartTotal => "corelink.ratelimit.do_cold_start_total",
            Self::MiddlewareDurationUs => "corelink.ratelimit.middleware_duration_us",
            Self::CrossTenantViolationTotal => "corelink.ratelimit.cross_tenant_violation_total",
        }
    }
}

/// Result label for `check_total` + `middleware_duration_us`. Closed
/// canonical 2-set per WI §6.1.10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RateLimitResultLabel {
    /// `result=allowed`.
    Allowed,
    /// `result=denied`.
    Denied,
}

impl RateLimitResultLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }
}

/// Rate-limit metrics observer trait every backend (statsd /
/// prometheus / CloudWatch / in-memory) implements.
pub trait RateLimitMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.ratelimit.check_total{tenant, dimension,
    /// result=...}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitMetricsObserverError::Backend`] on backend failure.
    fn record_check(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        result: RateLimitResultLabel,
    ) -> Result<(), RateLimitMetricsObserverError>;

    /// Set the gauge `corelink.ratelimit.tokens_remaining{tenant,
    /// dimension}` to `count`.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitMetricsObserverError::Backend`] on backend failure.
    fn observe_tokens_remaining(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        count: u64,
    ) -> Result<(), RateLimitMetricsObserverError>;

    /// Set the gauge `corelink.ratelimit.refill_rate{tenant,
    /// dimension}` to the current refill rate (tokens / second).
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitMetricsObserverError::Backend`] on backend failure.
    fn observe_refill_rate(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        rate_per_sec: u64,
    ) -> Result<(), RateLimitMetricsObserverError>;

    /// Observe `corelink.ratelimit.middleware_duration_us{result=...}`
    /// — histogram-like (SLO target p99 < 3000 us).
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitMetricsObserverError::Backend`] on backend failure.
    fn record_middleware_duration_us(
        &self,
        result: RateLimitResultLabel,
        duration_us: u64,
    ) -> Result<(), RateLimitMetricsObserverError>;

    /// Increment `corelink.ratelimit.cross_tenant_violation_total` by 1.
    /// **MUST fire SEV-1 alert** at the production observability layer.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitMetricsObserverError::Backend`] on backend failure.
    fn record_cross_tenant_violation(&self) -> Result<(), RateLimitMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryRateLimitMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    gauges: HashMap<String, u64>,
    last_duration_per_result: HashMap<&'static str, u64>,
    duration_samples_per_result: HashMap<&'static str, Vec<u64>>,
}

impl InMemoryRateLimitMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a fully-qualified label string.
    #[must_use]
    pub fn counter(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.counters.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .counters
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Per-metric aggregate counter snapshot summing every label
    /// fragment under the canonical metric name.
    #[must_use]
    pub fn counter_total(&self, kind: RateLimitMetricKind) -> u64 {
        let prefix = kind.as_str();
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .counters
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(_, v)| *v)
            .sum()
    }

    /// Gauge snapshot.
    #[must_use]
    pub fn gauge(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.gauges.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .gauges
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Most-recently observed duration_us for a result label.
    #[must_use]
    pub fn last_duration_us(&self, result: RateLimitResultLabel) -> Option<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.last_duration_per_result.get(result.as_str()).copied()
    }

    /// Snapshot of every duration sample observed for the given result
    /// label (informational; the property test `prop_check_duration_under_3ms_p99`
    /// asserts the SLO bound).
    #[must_use]
    pub fn duration_samples(&self, result: RateLimitResultLabel) -> Vec<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .duration_samples_per_result
            .get(result.as_str())
            .cloned()
            .unwrap_or_default()
    }

    fn bump_counter(&self, label: String, by: u64) -> Result<(), RateLimitMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            RateLimitMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }
}

impl RateLimitMetricsObserver for InMemoryRateLimitMetrics {
    fn record_check(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        result: RateLimitResultLabel,
    ) -> Result<(), RateLimitMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},dimension={},result={}}}",
            RateLimitMetricKind::CheckTotal.as_str(),
            dimension.as_str(),
            result.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn observe_tokens_remaining(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        count: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},dimension={}}}",
            RateLimitMetricKind::TokensRemaining.as_str(),
            dimension.as_str(),
        );
        let mut guard = self.inner.lock().map_err(|_| {
            RateLimitMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        guard.gauges.insert(label, count);
        Ok(())
    }

    fn observe_refill_rate(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        rate_per_sec: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},dimension={}}}",
            RateLimitMetricKind::RefillRate.as_str(),
            dimension.as_str(),
        );
        let mut guard = self.inner.lock().map_err(|_| {
            RateLimitMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        guard.gauges.insert(label, rate_per_sec);
        Ok(())
    }

    fn record_middleware_duration_us(
        &self,
        result: RateLimitResultLabel,
        duration_us: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        let label = format!(
            "{}{{result={}}}",
            RateLimitMetricKind::MiddlewareDurationUs.as_str(),
            result.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            RateLimitMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(duration_us);
        guard
            .last_duration_per_result
            .insert(result.as_str(), duration_us);
        guard
            .duration_samples_per_result
            .entry(result.as_str())
            .or_default()
            .push(duration_us);
        Ok(())
    }

    fn record_cross_tenant_violation(&self) -> Result<(), RateLimitMetricsObserverError> {
        let label = RateLimitMetricKind::CrossTenantViolationTotal
            .as_str()
            .to_string();
        self.bump_counter(label, 1)
    }
}

/// Production-safe **bounded** metrics observer: accepts every metric
/// and drops it (constant memory, zero heap growth, zero label
/// cardinality).
///
/// ## Why this exists (F-022 closure)
///
/// [`InMemoryRateLimitMetrics`] is a **test capture** observer — every
/// `record_*` / `observe_*` call inserts a `format!`-built per-tenant
/// label string into an unbounded `HashMap` (counters/gauges grow with
/// tenant × dimension cardinality) and pushes onto an unbounded
/// `duration_samples_per_result` `Vec`. Wired into the production
/// data-plane layer it grows heap on ordinary traffic without bound. This
/// observer is the bounded production default: it satisfies the
/// [`RateLimitMetricsObserver`] contract with **O(1)** memory, **no
/// allocation**, and **no per-tenant label retention**.
///
/// ## Forward path
///
/// The live exporter (statsd / Prometheus / CloudWatch) increments an
/// external time-series store with bounded local state; it lands with the
/// observability wiring (WI-S08-006). Until then this NoOp observer is the
/// correct production posture — the cross-tenant-violation SEV-1 canary is
/// also routed here, so a future exporter must re-surface it; dropping the
/// (informational) per-tenant gauges in the interim is strictly preferable
/// to the unbounded test observer's leak.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpRateLimitMetrics;

impl NoOpRateLimitMetrics {
    /// Construct a fresh bounded no-op observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimitMetricsObserver for NoOpRateLimitMetrics {
    #[inline]
    fn record_check(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _result: RateLimitResultLabel,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Ok(())
    }

    #[inline]
    fn observe_tokens_remaining(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _count: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Ok(())
    }

    #[inline]
    fn observe_refill_rate(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _rate_per_sec: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Ok(())
    }

    #[inline]
    fn record_middleware_duration_us(
        &self,
        _result: RateLimitResultLabel,
        _duration_us: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Ok(())
    }

    #[inline]
    fn record_cross_tenant_violation(&self) -> Result<(), RateLimitMetricsObserverError> {
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingRateLimitMetrics;

impl FailingRateLimitMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimitMetricsObserver for FailingRateLimitMetrics {
    fn record_check(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _result: RateLimitResultLabel,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Err(RateLimitMetricsObserverError::Backend(
            "induced ratelimit metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_tokens_remaining(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _count: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Err(RateLimitMetricsObserverError::Backend(
            "induced ratelimit metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_refill_rate(
        &self,
        _tenant_id: Uuid,
        _dimension: KeyDimension,
        _rate_per_sec: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Err(RateLimitMetricsObserverError::Backend(
            "induced ratelimit metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_middleware_duration_us(
        &self,
        _result: RateLimitResultLabel,
        _duration_us: u64,
    ) -> Result<(), RateLimitMetricsObserverError> {
        Err(RateLimitMetricsObserverError::Backend(
            "induced ratelimit metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_cross_tenant_violation(&self) -> Result<(), RateLimitMetricsObserverError> {
        Err(RateLimitMetricsObserverError::Backend(
            "induced ratelimit metrics failure (test fixture)".to_string(),
        ))
    }
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

    fn ten() -> Uuid {
        Uuid::from_u128(0x1)
    }

    #[test]
    fn canonical_metric_names_align_with_enum() {
        let names = canonical_metric_names();
        assert_eq!(names.len(), 7);
        assert!(names.contains(&RateLimitMetricKind::CheckTotal.as_str()));
        assert!(names.contains(&RateLimitMetricKind::TokensRemaining.as_str()));
        assert!(names.contains(&RateLimitMetricKind::RefillRate.as_str()));
        assert!(names.contains(&RateLimitMetricKind::PlanSyncLagMs.as_str()));
        assert!(names.contains(&RateLimitMetricKind::DoColdStartTotal.as_str()));
        assert!(names.contains(&RateLimitMetricKind::MiddlewareDurationUs.as_str()));
        assert!(names.contains(&RateLimitMetricKind::CrossTenantViolationTotal.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            RateLimitMetricKind::CheckTotal,
            RateLimitMetricKind::TokensRemaining,
            RateLimitMetricKind::RefillRate,
            RateLimitMetricKind::PlanSyncLagMs,
            RateLimitMetricKind::DoColdStartTotal,
            RateLimitMetricKind::MiddlewareDurationUs,
            RateLimitMetricKind::CrossTenantViolationTotal,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.ratelimit."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 7);
    }

    #[test]
    fn check_total_increments_per_label() {
        let m = InMemoryRateLimitMetrics::new();
        m.record_check(
            ten(),
            KeyDimension::PerTenant,
            RateLimitResultLabel::Allowed,
        )
        .unwrap();
        m.record_check(
            ten(),
            KeyDimension::PerTenant,
            RateLimitResultLabel::Allowed,
        )
        .unwrap();
        m.record_check(ten(), KeyDimension::PerIp, RateLimitResultLabel::Denied)
            .unwrap();
        assert_eq!(m.counter_total(RateLimitMetricKind::CheckTotal), 3);
    }

    #[test]
    fn tokens_remaining_overwrites_gauge() {
        let m = InMemoryRateLimitMetrics::new();
        m.observe_tokens_remaining(ten(), KeyDimension::PerTenant, 100)
            .unwrap();
        let label = format!(
            "{}{{tenant={},dimension=per_tenant}}",
            RateLimitMetricKind::TokensRemaining.as_str(),
            ten()
        );
        assert_eq!(m.gauge(&label), 100);
        // Overwrite with a smaller value (the bucket emptied).
        m.observe_tokens_remaining(ten(), KeyDimension::PerTenant, 10)
            .unwrap();
        assert_eq!(m.gauge(&label), 10);
    }

    #[test]
    fn refill_rate_overwrites_gauge() {
        let m = InMemoryRateLimitMetrics::new();
        m.observe_refill_rate(ten(), KeyDimension::PerTenant, 200)
            .unwrap();
        let label = format!(
            "{}{{tenant={},dimension=per_tenant}}",
            RateLimitMetricKind::RefillRate.as_str(),
            ten()
        );
        assert_eq!(m.gauge(&label), 200);
    }

    #[test]
    fn middleware_duration_records_per_result() {
        let m = InMemoryRateLimitMetrics::new();
        m.record_middleware_duration_us(RateLimitResultLabel::Allowed, 100)
            .unwrap();
        m.record_middleware_duration_us(RateLimitResultLabel::Allowed, 200)
            .unwrap();
        assert_eq!(m.last_duration_us(RateLimitResultLabel::Allowed), Some(200));
        let samples = m.duration_samples(RateLimitResultLabel::Allowed);
        assert_eq!(samples, vec![100, 200]);
    }

    #[test]
    fn cross_tenant_violation_increments_canary() {
        let m = InMemoryRateLimitMetrics::new();
        m.record_cross_tenant_violation().unwrap();
        m.record_cross_tenant_violation().unwrap();
        assert_eq!(
            m.counter_total(RateLimitMetricKind::CrossTenantViolationTotal),
            2
        );
    }

    #[test]
    fn noop_metrics_accepts_everything_and_retains_no_state() {
        // F-022 regression: the bounded prod observer must accept every
        // metric (never erroring the hot path) and retain NO per-tenant
        // label state (no unbounded HashMap / sample Vec growth).
        let m = NoOpRateLimitMetrics::new();
        for i in 0..10_000u128 {
            let t = Uuid::from_u128(i); // distinct tenants → would blow up label cardinality on the in-mem sink
            m.record_check(t, KeyDimension::PerTenant, RateLimitResultLabel::Allowed)
                .unwrap();
            m.observe_tokens_remaining(t, KeyDimension::PerTenant, 1)
                .unwrap();
            m.observe_refill_rate(t, KeyDimension::PerTenant, 1)
                .unwrap();
            m.record_middleware_duration_us(RateLimitResultLabel::Allowed, 1)
                .unwrap();
            m.record_cross_tenant_violation().unwrap();
        }
        // Zero-sized type → no state, no growth, by construction.
        assert_eq!(std::mem::size_of::<NoOpRateLimitMetrics>(), 0);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingRateLimitMetrics::new();
        assert!(matches!(
            m.record_check(
                ten(),
                KeyDimension::PerTenant,
                RateLimitResultLabel::Allowed
            )
            .unwrap_err(),
            RateLimitMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_tokens_remaining(ten(), KeyDimension::PerTenant, 0)
                .unwrap_err(),
            RateLimitMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_refill_rate(ten(), KeyDimension::PerTenant, 0)
                .unwrap_err(),
            RateLimitMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_middleware_duration_us(RateLimitResultLabel::Allowed, 1)
                .unwrap_err(),
            RateLimitMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_cross_tenant_violation().unwrap_err(),
            RateLimitMetricsObserverError::Backend(_)
        ));
    }

    #[test]
    fn result_label_canonical_strings() {
        assert_eq!(RateLimitResultLabel::Allowed.as_str(), "allowed");
        assert_eq!(RateLimitResultLabel::Denied.as_str(), "denied");
    }
}
