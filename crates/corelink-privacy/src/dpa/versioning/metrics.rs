//! Prometheus metric name scaffolds (snake_case underscored).
//!
//! Production wiring exports via `corelink-tracing` Prometheus
//! exporter; this module only ships the **canonical name constants**
//! so the wiring layer + dashboards + alerting rules share a single
//! source of truth.

/// Counter — `corelink_onboarding_dpa_re_acceptance_total{from_version,
/// to_version, outcome}`. `outcome` ∈ `accepted` | `grace_expired_degraded`
/// | `grace_expired_blocked`.
pub const RE_ACCEPTANCE_TOTAL: &str = "corelink_onboarding_dpa_re_acceptance_total";

/// Gauge — `corelink_onboarding_dpa_grace_period_remaining_days{tenant_tier,
/// plan}`. Aggregated p50 / p99 in dashboards.
pub const GRACE_PERIOD_REMAINING_DAYS: &str = "corelink_onboarding_dpa_grace_period_remaining_days";

/// Gauge — `corelink_onboarding_dpa_degraded_tenants_count{plan}`.
/// Alert at >5% of existing tenants in production.
pub const DEGRADED_TENANTS_COUNT: &str = "corelink_onboarding_dpa_degraded_tenants_count";

/// Outcome label values for [`RE_ACCEPTANCE_TOTAL`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReAcceptanceOutcome {
    /// Tenant re-accepted within grace.
    Accepted,
    /// Cron pass found tenant past grace — first-time degrade.
    GraceExpiredDegraded,
    /// Subsequent write attempt blocked while still degraded.
    GraceExpiredBlocked,
}

impl ReAcceptanceOutcome {
    /// Wire-safe label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::GraceExpiredDegraded => "grace_expired_degraded",
            Self::GraceExpiredBlocked => "grace_expired_blocked",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_names_are_snake_case_and_canonical() {
        assert!(RE_ACCEPTANCE_TOTAL
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
        assert!(GRACE_PERIOD_REMAINING_DAYS
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
        assert!(DEGRADED_TENANTS_COUNT
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
    }

    #[test]
    fn outcome_labels_canonical() {
        assert_eq!(ReAcceptanceOutcome::Accepted.label(), "accepted");
        assert_eq!(
            ReAcceptanceOutcome::GraceExpiredDegraded.label(),
            "grace_expired_degraded"
        );
        assert_eq!(
            ReAcceptanceOutcome::GraceExpiredBlocked.label(),
            "grace_expired_blocked"
        );
    }
}
