//! Region health state + failover trigger signals + SLO thresholds.

use serde::{Deserialize, Serialize};

/// Failover overhead SLO ceiling in milliseconds.
///
/// Failover routing overhead must be ≤ 50ms p99 (WI-S14-003 §6.1.4).
///
/// # Examples
///
/// ```
/// use corelink_failover_router::SLO_FAILOVER_OVERHEAD_MS;
///
/// assert_eq!(SLO_FAILOVER_OVERHEAD_MS, 50);
/// ```
pub const SLO_FAILOVER_OVERHEAD_MS: u64 = 50;

/// 5xx rate threshold for region degradation signal.
///
/// If 5xx rate > 1% sustained 5s, this signal is active.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::RATE_5XX_THRESHOLD_PCT;
///
/// assert!((RATE_5XX_THRESHOLD_PCT - 1.0_f64).abs() < f64::EPSILON);
/// ```
pub const RATE_5XX_THRESHOLD_PCT: f64 = 1.0;

/// Latency SLO ceiling for CAS GET p99 in milliseconds.
///
/// SLO-LAT-CAS-GET p99 < 300ms. If exceeded sustained 5s → degraded signal.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::LATENCY_SLO_CEIL_MS;
///
/// assert_eq!(LATENCY_SLO_CEIL_MS, 300);
/// ```
pub const LATENCY_SLO_CEIL_MS: u64 = 300;

/// Consecutive failures threshold for degradation signal.
///
/// ≥ 3 consecutive failures within the 5s sustained window.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::CONSECUTIVE_FAILURES_THRESHOLD;
///
/// assert_eq!(CONSECUTIVE_FAILURES_THRESHOLD, 3);
/// ```
pub const CONSECUTIVE_FAILURES_THRESHOLD: u32 = 3;

/// Sustained window in seconds. All signals must be active within this window.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::SUSTAINED_WINDOW_SECS;
///
/// assert_eq!(SUSTAINED_WINDOW_SECS, 5);
/// ```
pub const SUSTAINED_WINDOW_SECS: u64 = 5;

/// Region health status.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::RegionHealth;
///
/// assert_eq!(RegionHealth::Healthy.as_str(), "healthy");
/// assert!(RegionHealth::Healthy.is_healthy());
/// assert!(!RegionHealth::Degraded.is_healthy());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RegionHealth {
    /// Region fully operational; all signals nominal.
    Healthy,
    /// Multi-signal degradation detected; reads routed to sibling.
    Degraded,
    /// Region completely unavailable.
    Down,
}

impl RegionHealth {
    /// Canonical string for Prometheus gauge `corelink_region_health_status{region}`.
    ///
    /// `0 = Down / 1 = Degraded / 2 = Healthy` (numeric in metric; string in label).
    pub fn as_str(self) -> &'static str {
        match self {
            RegionHealth::Healthy => "healthy",
            RegionHealth::Degraded => "degraded",
            RegionHealth::Down => "down",
        }
    }

    /// Returns the Prometheus gauge value (0=Down, 1=Degraded, 2=Healthy).
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::RegionHealth;
    ///
    /// assert_eq!(RegionHealth::Healthy.prometheus_value(), 2);
    /// assert_eq!(RegionHealth::Down.prometheus_value(), 0);
    /// ```
    pub fn prometheus_value(self) -> u8 {
        match self {
            RegionHealth::Down => 0,
            RegionHealth::Degraded => 1,
            RegionHealth::Healthy => 2,
        }
    }

    /// Returns `true` if the region is fully operational.
    pub fn is_healthy(self) -> bool {
        matches!(self, RegionHealth::Healthy)
    }

    /// Returns `true` if the region requires failover (Degraded or Down).
    pub fn requires_failover(self) -> bool {
        !self.is_healthy()
    }
}

impl std::fmt::Display for RegionHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failover trigger signal type.
///
/// All 3 signals must be simultaneously active within the 5s sustained window
/// to trigger failover (multi-signal triangulation; prevents false-positive).
///
/// # Examples
///
/// ```
/// use corelink_failover_router::FailoverTrigger;
///
/// let trigger = FailoverTrigger::Rate5xx;
/// assert_eq!(trigger.as_str(), "5xx_rate");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum FailoverTrigger {
    /// 5xx error rate > 1% sustained.
    Rate5xx,
    /// Latency p99 > SLO ceiling (300ms) sustained.
    LatencySloViolation,
    /// ≥ 3 consecutive failures within 5s.
    ConsecutiveFailures,
}

impl FailoverTrigger {
    /// Canonical string for `failover.trigger` span attribute.
    pub fn as_str(self) -> &'static str {
        match self {
            FailoverTrigger::Rate5xx => "5xx_rate",
            FailoverTrigger::LatencySloViolation => "latency_slo",
            FailoverTrigger::ConsecutiveFailures => "consecutive_failures",
        }
    }
}

impl std::fmt::Display for FailoverTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A snapshot of region health at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionHealthSnapshot {
    /// Region identifier.
    pub region: crate::Region,
    /// Current health status.
    pub health: RegionHealth,
    /// 5xx rate (percentage, 0.0–100.0).
    pub rate_5xx_pct: f64,
    /// Observed latency p99 in milliseconds.
    pub latency_p99_ms: u64,
    /// Number of consecutive failures in current window.
    pub consecutive_failures: u32,
    /// Timestamp (ms since epoch) of this snapshot.
    pub timestamp_ms: u64,
    /// Active trigger signals (empty if Healthy).
    pub active_triggers: Vec<FailoverTrigger>,
}

impl RegionHealthSnapshot {
    /// Evaluate multi-signal degradation: all 3 signals active = Degraded.
    ///
    /// Must be sustained (`active_triggers.len() == 3`) to avoid false-positives.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::{
    ///     FailoverTrigger, Region, RegionHealth, RegionHealthSnapshot,
    ///     CONSECUTIVE_FAILURES_THRESHOLD, LATENCY_SLO_CEIL_MS, RATE_5XX_THRESHOLD_PCT,
    /// };
    ///
    /// let snap = RegionHealthSnapshot::evaluate(
    ///     Region::Weur,
    ///     2.0,          // rate_5xx > 1% ✓
    ///     350,          // latency > 300ms ✓
    ///     4,            // consecutive failures ≥ 3 ✓
    ///     1_700_000_000_000,
    /// );
    /// assert_eq!(snap.health, RegionHealth::Degraded);
    /// assert_eq!(snap.active_triggers.len(), 3);
    /// ```
    pub fn evaluate(
        region: crate::Region,
        rate_5xx_pct: f64,
        latency_p99_ms: u64,
        consecutive_failures: u32,
        timestamp_ms: u64,
    ) -> Self {
        let mut triggers = Vec::new();

        if rate_5xx_pct > RATE_5XX_THRESHOLD_PCT {
            triggers.push(FailoverTrigger::Rate5xx);
        }
        if latency_p99_ms > LATENCY_SLO_CEIL_MS {
            triggers.push(FailoverTrigger::LatencySloViolation);
        }
        if consecutive_failures >= CONSECUTIVE_FAILURES_THRESHOLD {
            triggers.push(FailoverTrigger::ConsecutiveFailures);
        }

        let health = if triggers.len() >= 3 {
            RegionHealth::Degraded
        } else {
            RegionHealth::Healthy
        };

        RegionHealthSnapshot {
            region,
            health,
            rate_5xx_pct,
            latency_p99_ms,
            consecutive_failures,
            timestamp_ms,
            active_triggers: triggers,
        }
    }
}
