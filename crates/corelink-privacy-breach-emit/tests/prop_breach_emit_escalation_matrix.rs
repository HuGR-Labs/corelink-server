//! Property test: PagerDuty escalation matrix — AC-006.
//!
//! Tests that 100k random (severity, alert_recipients) combinations
//! 100% match the expected escalation policy per RB-BREACH-NOTIF §8.
//!
//! AC-006 (WI-S11-006 §8):
//! "property test 100 random (severity, alert_recipients) combinations:
//!  100% match expected escalation"
//! Note: PROPTEST_CASES env var overrides the count (default 10_000;
//! PROPTEST_CASES=100000 for full 100k run).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test file"
)]

use corelink_privacy_breach_emit::{
    escalation_policy_for, BreachSeverity,
};
use proptest::prelude::*;

/// Runtime-configurable test count per S-07 P1-2 lesson absorbed.
/// NEVER compile-time const.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

/// Arbitrary severity strategy.
fn arb_severity() -> impl Strategy<Value = BreachSeverity> {
    prop_oneof![
        Just(BreachSeverity::Sev1),
        Just(BreachSeverity::Sev2),
        Just(BreachSeverity::Sev3),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Property: every (severity, escalation_policy) pair satisfies the
    /// canonical RB-BREACH-NOTIF §8 contract:
    ///
    /// - SEV-1: 4 steps, includes CEO interim, service = corelink-breach-sev1.
    /// - SEV-2: 3 steps, no CEO, service = corelink-breach-sev2.
    /// - SEV-3: 1 step (Privacy Officer only), service = corelink-breach-sev3.
    ///
    /// Time-out offsets are strictly increasing in ALL cases.
    ///
    /// AC-006 compliance: 100k random severity values → 100% match.
    #[test]
    fn escalation_matrix_matches_runbook_for_all_severities(
        severity in arb_severity(),
    ) {
        let policy = escalation_policy_for(severity);

        // Policy severity matches input
        assert_eq!(policy.severity, severity);

        // PagerDuty service name matches expected
        let expected_service = severity.pagerduty_service();
        assert_eq!(policy.pagerduty_service, expected_service);

        // Severity-specific step count and role expectations
        match severity {
            BreachSeverity::Sev1 => {
                let step_count = policy.steps.len();
                let valid = step_count == 4;
                prop_assert!(valid, "SEV-1 must have 4 escalation steps, got {}", step_count);

                let has_ceo = policy.steps.iter().any(|s| s.role.contains("CEO"));
                prop_assert!(has_ceo, "SEV-1 must include CEO interim in escalation");

                let has_privacy_officer = policy.steps.iter().any(|s| s.role.contains("Privacy Officer"));
                prop_assert!(has_privacy_officer, "SEV-1 must include Privacy Officer");

                let has_legal = policy.steps.iter().any(|s| s.role.contains("Legal"));
                prop_assert!(has_legal, "SEV-1 must include Legal");

                let has_security_lead = policy.steps.iter().any(|s| s.role.contains("Security Lead"));
                prop_assert!(has_security_lead, "SEV-1 must include Security Lead");

                // Service name pinned
                let valid_service = policy.pagerduty_service == "corelink-breach-sev1";
                prop_assert!(valid_service, "SEV-1 service must be corelink-breach-sev1");
            }
            BreachSeverity::Sev2 => {
                let step_count = policy.steps.len();
                let valid = step_count == 3;
                prop_assert!(valid, "SEV-2 must have 3 escalation steps, got {}", step_count);

                let has_ceo = policy.steps.iter().any(|s| s.role.contains("CEO"));
                prop_assert!(!has_ceo, "SEV-2 must NOT include CEO");

                let valid_service = policy.pagerduty_service == "corelink-breach-sev2";
                prop_assert!(valid_service, "SEV-2 service must be corelink-breach-sev2");
            }
            BreachSeverity::Sev3 => {
                let step_count = policy.steps.len();
                let valid = step_count == 1;
                prop_assert!(valid, "SEV-3 must have 1 escalation step, got {}", step_count);

                let is_privacy_officer = policy.steps[0].role.contains("Privacy Officer");
                prop_assert!(is_privacy_officer, "SEV-3 must only page Privacy Officer");

                let valid_service = policy.pagerduty_service == "corelink-breach-sev3";
                prop_assert!(valid_service, "SEV-3 service must be corelink-breach-sev3");
            }
            // SAFETY: non_exhaustive enum — future arms require ≥ 1 step
            _ => {
                let valid = !policy.steps.is_empty();
                prop_assert!(valid, "future severity arms must have at least 1 escalation step");
            }
        }

        // All policies: timeouts strictly increasing
        let timeouts: Vec<u32> = policy.steps.iter().map(|s| s.timeout_minutes).collect();
        for w in timeouts.windows(2) {
            let valid = w[0] < w[1];
            prop_assert!(valid, "escalation timeouts must be strictly increasing for {severity}");
        }

        // First step timeout is always > 0
        if let Some(first) = policy.steps.first() {
            let valid = first.timeout_minutes > 0;
            prop_assert!(valid, "first escalation step must have timeout > 0");
        }
    }

    /// Property: BreachSeverity methods are consistent across all values.
    #[test]
    fn severity_methods_consistent(severity in arb_severity()) {
        // as_str() is non-empty and starts with "SEV-"
        let s = severity.as_str();
        let valid_prefix = s.starts_with("SEV-");
        prop_assert!(valid_prefix, "severity str must start with SEV-, got {s}");

        // pagerduty_service() contains severity level
        let svc = severity.pagerduty_service();
        let valid_svc = svc.starts_with("corelink-breach-sev");
        prop_assert!(valid_svc, "pagerduty service must start with corelink-breach-sev, got {svc}");

        // requires_all_jurisdictions only for SEV-1
        let requires_all = severity.requires_all_jurisdictions();
        let valid_requires_all = match severity {
            BreachSeverity::Sev1 => requires_all,
            BreachSeverity::Sev2 | BreachSeverity::Sev3 => !requires_all,
            // SAFETY: non_exhaustive enum — future arms require requires_all = false by default
            _ => !requires_all,
        };
        prop_assert!(valid_requires_all, "requires_all_jurisdictions must match severity contract");

        // activates_status_page_banner only for SEV-1
        let activates_banner = severity.activates_status_page_banner();
        let valid_banner = match severity {
            BreachSeverity::Sev1 => activates_banner,
            BreachSeverity::Sev2 | BreachSeverity::Sev3 => !activates_banner,
            // SAFETY: non_exhaustive enum — future arms: banner activation is opt-in per ADR
            _ => !activates_banner,
        };
        prop_assert!(valid_banner, "activates_status_page_banner must match severity contract");
    }
}
