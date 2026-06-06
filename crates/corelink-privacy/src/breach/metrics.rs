//! Canonical Prometheus metric names for breach notification observability.
//!
//! 4 canonical metrics per WI-S11-006 §10.4 + §21.
//! Production wiring in the CF Worker / monitoring exporter registers
//! these with the actual `prometheus` or `metrics` crate. This module
//! ships the canonical name constants + label key constants so every
//! call site uses the same strings.
//!
//! ## Metric taxonomy
//!
//! | Metric | Type | Labels | Description |
//! |---|---|---|---|
//! | `corelink_breach_time_to_notification_seconds` | Histogram | `{jurisdiction, severity}` | Elapsed from `breach_detected_at` to dispatch per jurisdiction. SLA threshold: 72h = 259200s. |
//! | `corelink_breach_time_to_decision_seconds` | Histogram | `{severity, scenario}` | Elapsed from triage start to decision tree completion. SLO target: ≤ 4h = 14400s. |
//! | `corelink_breach_dry_run_completion_total` | Counter | `{outcome, scenario}` | Dry-run completions. `outcome` ∈ {passed, passed_with_concerns}. |
//! | `corelink_breach_customer_notification_delivery_total` | Counter | `{outcome, locale, severity}` | Customer notification delivery outcomes. `outcome` ∈ {delivered, failed}. |

/// Prometheus metric name: time from `breach_detected_at` to regulatory
/// notification dispatch per jurisdiction.
///
/// Labels: `{jurisdiction, severity}`
/// SLA threshold: 72h = 259200 seconds.
/// Histogram buckets recommended: [3600, 7200, 14400, 28800, 57600, 86400, 172800, 259200].
pub const METRIC_BREACH_TIME_TO_NOTIFICATION_SECONDS: &str =
    "corelink_breach_time_to_notification_seconds";

/// Prometheus metric name: time from triage start to decision tree
/// completion (includes template fill-in).
///
/// Labels: `{severity, scenario}`
/// SLO target: ≤ 4h = 14400 seconds.
/// Histogram buckets recommended: [1800, 3600, 7200, 14400, 21600, 28800].
pub const METRIC_BREACH_TIME_TO_DECISION_SECONDS: &str = "corelink_breach_time_to_decision_seconds";

/// Prometheus counter: dry-run tabletop completions.
///
/// Labels: `{outcome, scenario}`
/// `outcome` ∈ {"passed", "passed_with_concerns"}
/// `scenario` ∈ {"scenario-1-pii-leak-via-log", "scenario-2-r2-cross-tenant", "scenario-3-audit-chain"}
pub const METRIC_BREACH_DRY_RUN_COMPLETION_TOTAL: &str = "corelink_breach_dry_run_completion_total";

/// Prometheus counter: customer notification delivery outcomes.
///
/// Labels: `{outcome, locale, severity}`
/// `outcome` ∈ {"delivered", "failed"}
/// `locale` ∈ {"pt-BR", "en-US", "es-MX"}
/// `severity` ∈ {"SEV-1", "SEV-2", "SEV-3"}
pub const METRIC_BREACH_CUSTOMER_NOTIFICATION_DELIVERY_TOTAL: &str =
    "corelink_breach_customer_notification_delivery_total";

/// Label key: regulatory jurisdiction (ANPD / Irish DPC / California AG).
pub const LABEL_JURISDICTION: &str = "jurisdiction";

/// Label key: breach severity tier (SEV-1 / SEV-2 / SEV-3).
pub const LABEL_SEVERITY: &str = "severity";

/// Label key: dry-run / delivery outcome (passed / passed_with_concerns / delivered / failed).
pub const LABEL_OUTCOME: &str = "outcome";

/// Label key: dry-run scenario identifier.
pub const LABEL_SCENARIO: &str = "scenario";

/// Label key: customer notification locale (pt-BR / en-US / es-MX).
pub const LABEL_LOCALE: &str = "locale";

/// SLA threshold for `corelink_breach_time_to_notification_seconds`:
/// 72 hours in seconds (GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82).
pub const SLA_NOTIFICATION_SECONDS: u64 = 72 * 3600;

/// SLO target for `corelink_breach_time_to_decision_seconds`:
/// 4 hours in seconds (RB-BREACH-NOTIF §1 + DD-005).
pub const SLO_DECISION_SECONDS: u64 = 4 * 3600;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn metric_names_are_unique_and_prefixed() {
        let names = [
            METRIC_BREACH_TIME_TO_NOTIFICATION_SECONDS,
            METRIC_BREACH_TIME_TO_DECISION_SECONDS,
            METRIC_BREACH_DRY_RUN_COMPLETION_TOTAL,
            METRIC_BREACH_CUSTOMER_NOTIFICATION_DELIVERY_TOTAL,
        ];
        let mut seen = std::collections::HashSet::new();
        for name in names {
            assert!(
                name.starts_with("corelink_breach_"),
                "metric {name} must start with corelink_breach_"
            );
            assert!(seen.insert(name), "duplicate metric name: {name}");
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn sla_notification_seconds_equals_72h() {
        assert_eq!(SLA_NOTIFICATION_SECONDS, 72 * 3600);
        assert_eq!(SLA_NOTIFICATION_SECONDS, 259_200);
    }

    #[test]
    fn slo_decision_seconds_equals_4h() {
        assert_eq!(SLO_DECISION_SECONDS, 4 * 3600);
        assert_eq!(SLO_DECISION_SECONDS, 14_400);
    }

    #[test]
    fn label_keys_are_non_empty() {
        let keys = [
            LABEL_JURISDICTION,
            LABEL_SEVERITY,
            LABEL_OUTCOME,
            LABEL_SCENARIO,
            LABEL_LOCALE,
        ];
        for k in keys {
            assert!(!k.is_empty(), "label key must not be empty");
        }
    }
}
