//! Canonical Prometheus metric name constants for the rollout
//! controller (WI-S13-005 §6.1.6).
//!
//! 7 metrics per spec, all prefixed `corelink_admin_rollout_`.
//! Labels follow `observability_model.md §3.1 + §4.1`.

/// Gauge: current rollout stage + outcome.
/// Labels: `stage` ∈ {stage_1pct, stage_10pct, stage_50pct,
/// stage_100pct}, `outcome` ∈ {active, complete, rolled_back}.
pub const METRIC_ROLLOUT_STAGE_GAUGE: &str = "corelink_admin_rollout_stage_gauge";

/// Counter: auto-rollback events.
/// Labels: `trigger` ∈ {error_rate, slo_burn, p99_latency},
/// `plan` (tenant plan tier).
pub const METRIC_ROLLOUT_AUTO_ROLLBACK_TOTAL: &str =
    "corelink_admin_rollout_auto_rollback_total";

/// Gauge: monthly budget consumed ratio (0.0–1.0+).
/// Alert threshold: > 0.30.
/// Labels: `plan`.
pub const METRIC_BUDGET_CONSUMED_RATIO: &str =
    "corelink_admin_rollout_budget_consumed_ratio";

/// Histogram: detection-to-rollback latency (seconds).
/// SLO: ≤ 10 min p99 (600 s).
pub const METRIC_DETECTION_DURATION_SECONDS: &str =
    "corelink_admin_rollout_detection_duration_seconds_bucket";

/// Histogram: rollback completion duration (seconds).
/// SLO: ≤ 5 min p99 (300 s).
pub const METRIC_ROLLBACK_DURATION_SECONDS: &str =
    "corelink_admin_rollout_rollback_duration_seconds_bucket";

/// Histogram: per-stage dwell time (seconds).
/// Labels: `stage`.
pub const METRIC_DWELL_SECONDS: &str = "corelink_admin_rollout_dwell_seconds";

/// Counter: deploy-freeze events.
/// Labels: `reason` ∈ {budget_exceeded, manual_override, chaos_test}.
pub const METRIC_FREEZE_TOTAL: &str = "corelink_admin_rollout_freeze_total";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn seven_metrics_all_distinct() {
        let metrics = [
            METRIC_ROLLOUT_STAGE_GAUGE,
            METRIC_ROLLOUT_AUTO_ROLLBACK_TOTAL,
            METRIC_BUDGET_CONSUMED_RATIO,
            METRIC_DETECTION_DURATION_SECONDS,
            METRIC_ROLLBACK_DURATION_SECONDS,
            METRIC_DWELL_SECONDS,
            METRIC_FREEZE_TOTAL,
        ];
        assert_eq!(metrics.len(), 7, "expected 7 metrics per §6.1.6");
        let unique: HashSet<_> = metrics.iter().collect();
        assert_eq!(unique.len(), 7, "duplicate metric names");
    }

    #[test]
    fn all_metrics_prefixed_correctly() {
        let metrics = [
            METRIC_ROLLOUT_STAGE_GAUGE,
            METRIC_ROLLOUT_AUTO_ROLLBACK_TOTAL,
            METRIC_BUDGET_CONSUMED_RATIO,
            METRIC_DETECTION_DURATION_SECONDS,
            METRIC_ROLLBACK_DURATION_SECONDS,
            METRIC_DWELL_SECONDS,
            METRIC_FREEZE_TOTAL,
        ];
        for m in &metrics {
            assert!(
                m.starts_with("corelink_admin_rollout_"),
                "metric {m} missing prefix"
            );
        }
    }
}
