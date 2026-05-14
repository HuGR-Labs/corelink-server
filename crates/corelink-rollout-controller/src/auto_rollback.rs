//! Auto-rollback driver (WI-S13-005 §6.1.3).
//!
//! Implements 3-trigger auto-rollback with 5-minute sustained threshold
//! (filters transient variance — INV-ROLLOUT-AUTO-ROLLBACK-TRIGGERS).
//!
//! ## Triggers (any fires rollback)
//!
//! 1. Error rate > baseline + 3σ, sustained 5 min.
//! 2. SLO burn-rate > 14.4 (1h window, Google SRE Workbook Ch 16).
//! 3. p99 latency > baseline + 50%, sustained 5 min.
//!
//! ## Sustained threshold
//!
//! Each trigger is only confirmed after it has been continuously
//! observed for ≥ [`SUSTAINED_THRESHOLD_SECS`] (300 s = 5 min).
//! Transient spikes that resolve before the threshold do not fire.
//!
//! ## Detection SLO
//!
//! Probe interval 60s → detection p99 ≤ 360s (6 probes) ≤ 10 min.

use crate::types::{AutoRollbackTrigger, GateMetrics};

/// Minimum duration (seconds) a trigger must be continuously observed
/// before auto-rollback fires. Default: 300 s (5 min).
pub const SUSTAINED_THRESHOLD_SECS: u64 = 300;

/// State tracking how long each trigger has been continuously active.
///
/// Reset to 0 when the trigger clears (transient). When any counter
/// reaches [`SUSTAINED_THRESHOLD_SECS`], auto-rollback fires.
#[derive(Debug, Clone, Default)]
pub struct SustainedTrigger {
    /// Elapsed seconds error rate trigger has been continuously active.
    pub error_rate_secs: u64,
    /// Elapsed seconds SLO burn trigger has been continuously active.
    pub slo_burn_secs: u64,
    /// Elapsed seconds p99 latency trigger has been continuously active.
    pub p99_latency_secs: u64,
}

impl SustainedTrigger {
    /// Create a new zeroed sustained trigger state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the sustained trigger counters given the latest
    /// [`GateMetrics`] and elapsed probe interval.
    ///
    /// Returns the first [`AutoRollbackTrigger`] that has breached the
    /// sustained threshold, or `None` if no trigger is sustained.
    pub fn advance(
        &mut self,
        metrics: &GateMetrics,
        elapsed_secs: u64,
    ) -> Option<AutoRollbackTrigger> {
        // Error rate trigger
        if metrics.error_rate_trigger() {
            self.error_rate_secs = self.error_rate_secs.saturating_add(elapsed_secs);
        } else {
            self.error_rate_secs = 0;
        }

        // SLO burn trigger
        if metrics.slo_burn_trigger() {
            self.slo_burn_secs = self.slo_burn_secs.saturating_add(elapsed_secs);
        } else {
            self.slo_burn_secs = 0;
        }

        // p99 latency trigger
        if metrics.p99_latency_trigger() {
            self.p99_latency_secs = self.p99_latency_secs.saturating_add(elapsed_secs);
        } else {
            self.p99_latency_secs = 0;
        }

        self.any_sustained()
    }

    /// Returns the first trigger that has reached the sustained
    /// threshold, or `None`.
    #[must_use]
    pub fn any_sustained(&self) -> Option<AutoRollbackTrigger> {
        if self.error_rate_secs >= SUSTAINED_THRESHOLD_SECS {
            Some(AutoRollbackTrigger::ErrorRateExceedsBaseline3Sigma)
        } else if self.slo_burn_secs >= SUSTAINED_THRESHOLD_SECS {
            Some(AutoRollbackTrigger::SloBurnRateExceeds14_4)
        } else if self.p99_latency_secs >= SUSTAINED_THRESHOLD_SECS {
            Some(AutoRollbackTrigger::P99LatencyExceedsBaseline50Pct)
        } else {
            None
        }
    }

    /// Reset all counters (called after rollback completes).
    pub fn reset(&mut self) {
        self.error_rate_secs = 0;
        self.slo_burn_secs = 0;
        self.p99_latency_secs = 0;
    }
}

/// Auto-rollback driver: stateful accumulator that tracks sustained
/// trigger windows across multiple probe invocations.
///
/// Instantiate one `AutoRollbackDriver` per active rollout session.
/// Call [`AutoRollbackDriver::observe`] at each 60-second probe
/// interval.
#[derive(Debug, Clone)]
pub struct AutoRollbackDriver {
    sustained: SustainedTrigger,
    /// Elapsed seconds since rollout started (for detection SLO
    /// measurement: metric `corelink_admin_rollout_detection_duration_seconds`).
    pub elapsed_since_start_secs: u64,
}

impl AutoRollbackDriver {
    /// Create a new driver for a fresh rollout session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sustained: SustainedTrigger::new(),
            elapsed_since_start_secs: 0,
        }
    }

    /// Observe a new metrics snapshot. `probe_interval_secs` is the
    /// interval since the last observation (typically 60s in production).
    ///
    /// Returns the sustained [`AutoRollbackTrigger`] that fired, or
    /// `None` if no trigger is sustained yet.
    pub fn observe(
        &mut self,
        metrics: &GateMetrics,
        probe_interval_secs: u64,
    ) -> Option<AutoRollbackTrigger> {
        self.elapsed_since_start_secs =
            self.elapsed_since_start_secs.saturating_add(probe_interval_secs);
        self.sustained.advance(metrics, probe_interval_secs)
    }

    /// Reset the driver after rollback (reuse for post-rollout 24h
    /// monitoring).
    pub fn reset(&mut self) {
        self.sustained.reset();
    }
}

impl Default for AutoRollbackDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests use panics as assertions"
)]
mod tests {
    use super::*;

    fn healthy_metrics() -> GateMetrics {
        GateMetrics {
            error_rate: 0.001,
            error_rate_baseline: 0.005,
            error_rate_sigma: 0.001,
            slo_burn_rate_1h: 0.5,
            p99_latency_ms: 50.0,
            p99_baseline_ms: 60.0,
            stage_elapsed_secs: 1800,
        }
    }

    fn error_rate_spike() -> GateMetrics {
        GateMetrics {
            error_rate: 0.10,       // baseline 0.005 + 3×0.001 = 0.008; spike > threshold
            error_rate_baseline: 0.005,
            error_rate_sigma: 0.001,
            slo_burn_rate_1h: 0.5,
            p99_latency_ms: 50.0,
            p99_baseline_ms: 60.0,
            stage_elapsed_secs: 1800,
        }
    }

    fn slo_burn_spike() -> GateMetrics {
        GateMetrics {
            error_rate: 0.001,
            error_rate_baseline: 0.005,
            error_rate_sigma: 0.001,
            slo_burn_rate_1h: 15.0, // > 14.4
            p99_latency_ms: 50.0,
            p99_baseline_ms: 60.0,
            stage_elapsed_secs: 1800,
        }
    }

    fn p99_spike() -> GateMetrics {
        GateMetrics {
            error_rate: 0.001,
            error_rate_baseline: 0.005,
            error_rate_sigma: 0.001,
            slo_burn_rate_1h: 0.5,
            p99_latency_ms: 150.0, // > 60 × 1.5 = 90 ms baseline + 50%
            p99_baseline_ms: 60.0,
            stage_elapsed_secs: 1800,
        }
    }

    #[test]
    fn healthy_metrics_no_trigger() {
        let mut d = AutoRollbackDriver::new();
        for _ in 0..10 {
            assert!(d.observe(&healthy_metrics(), 60).is_none());
        }
    }

    #[test]
    fn transient_spike_clears_before_threshold() {
        let mut d = AutoRollbackDriver::new();
        // Spike for 4 min (240 s < 300 s threshold)
        d.observe(&error_rate_spike(), 60);
        d.observe(&error_rate_spike(), 60);
        d.observe(&error_rate_spike(), 60);
        d.observe(&error_rate_spike(), 60);
        // Clears
        d.observe(&healthy_metrics(), 60);
        assert!(d.sustained.error_rate_secs == 0);
        // No rollback triggered
        assert!(d.sustained.any_sustained().is_none());
    }

    #[test]
    fn error_rate_sustained_fires_rollback() {
        let mut d = AutoRollbackDriver::new();
        // 5 × 60s = 300s = exactly threshold
        for i in 0..5 {
            let result = d.observe(&error_rate_spike(), 60);
            if i < 4 {
                assert!(result.is_none(), "should not fire before 300s");
            } else {
                assert_eq!(
                    result,
                    Some(AutoRollbackTrigger::ErrorRateExceedsBaseline3Sigma)
                );
            }
        }
    }

    #[test]
    fn slo_burn_sustained_fires_rollback() {
        let mut d = AutoRollbackDriver::new();
        for i in 0..5 {
            let result = d.observe(&slo_burn_spike(), 60);
            if i < 4 {
                assert!(result.is_none());
            } else {
                assert_eq!(result, Some(AutoRollbackTrigger::SloBurnRateExceeds14_4));
            }
        }
    }

    #[test]
    fn p99_latency_sustained_fires_rollback() {
        let mut d = AutoRollbackDriver::new();
        for i in 0..5 {
            let result = d.observe(&p99_spike(), 60);
            if i < 4 {
                assert!(result.is_none());
            } else {
                assert_eq!(
                    result,
                    Some(AutoRollbackTrigger::P99LatencyExceedsBaseline50Pct)
                );
            }
        }
    }

    #[test]
    fn reset_clears_counters() {
        let mut d = AutoRollbackDriver::new();
        d.observe(&error_rate_spike(), 300);
        assert!(d.sustained.error_rate_secs >= 300);
        d.reset();
        assert_eq!(d.sustained.error_rate_secs, 0);
    }
}
