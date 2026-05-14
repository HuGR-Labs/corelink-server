//! Drift severity classifier for WI-S13-004.

use crate::error::DriftConsumerError;
use crate::event::{DriftPlanEvent, DriftSeverity, REGIONS};

/// Classifies a [`DriftPlanEvent`] into a [`DriftSeverity`].
pub trait DriftClassifier: std::fmt::Debug {
    /// Classify the severity of a plan event.
    ///
    /// # Errors
    /// Returns [`DriftConsumerError::InvalidRegion`] if region is unknown.
    /// Returns [`DriftConsumerError::UnrecognisedExitCode`] if exit code is not 0/1/2.
    fn classify(&self, event: &DriftPlanEvent) -> Result<DriftSeverity, DriftConsumerError>;
}

/// Default classifier: validates region + maps diff_count → severity.
///
/// Severity mapping (WI-S13-004 §6.1 D1 schema + §6.1.5 metrics):
/// - `diff_count == 0` → `None` (clean run; cron health row)
/// - `diff_count` 1–2   → `Low`
/// - `diff_count` 3–10  → `Medium`
/// - `diff_count > 10`  → `High`
///
/// Exit code 1 (terraform error) also produces severity `None` with
/// `diff_count = 0` — the row is still inserted for cron health tracking,
/// but a separate SEV-2 Slack alert fires (handled by GH Actions workflow).
#[derive(Debug, Clone)]
pub struct DefaultDriftClassifier;

impl DriftClassifier for DefaultDriftClassifier {
    fn classify(&self, event: &DriftPlanEvent) -> Result<DriftSeverity, DriftConsumerError> {
        // Validate region
        if !REGIONS.contains(&event.region.as_str()) {
            return Err(DriftConsumerError::InvalidRegion(event.region.clone()));
        }

        // Validate exit code (0, 1, 2 only; others = CI misconfiguration)
        if !matches!(event.tf_exit_code, 0..=2) {
            return Err(DriftConsumerError::UnrecognisedExitCode(event.tf_exit_code));
        }

        // Map diff_count to severity
        let severity = match event.plan_diff_count {
            0 => DriftSeverity::None,
            1..=2 => DriftSeverity::Low,
            3..=10 => DriftSeverity::Medium,
            _ => DriftSeverity::High,
        };

        Ok(severity)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn make_event(region: &str, exit_code: i32, diff_count: u32) -> DriftPlanEvent {
        DriftPlanEvent {
            region: region.to_owned(),
            detected_at_ms: 1_000_000,
            tf_exit_code: exit_code,
            plan_diff_count: diff_count,
            plan_summary: String::new(),
            plan_full_artifact_url: None,
            github_run_id: "run-123".to_owned(),
        }
    }

    #[test]
    fn clean_run_is_none_severity() {
        let c = DefaultDriftClassifier;
        let ev = make_event("us-east", 0, 0);
        assert_eq!(c.classify(&ev).unwrap(), DriftSeverity::None);
    }

    #[test]
    fn low_severity_boundary() {
        let c = DefaultDriftClassifier;
        assert_eq!(
            c.classify(&make_event("eu-west", 2, 1)).unwrap(),
            DriftSeverity::Low
        );
        assert_eq!(
            c.classify(&make_event("eu-west", 2, 2)).unwrap(),
            DriftSeverity::Low
        );
    }

    #[test]
    fn medium_severity_boundary() {
        let c = DefaultDriftClassifier;
        assert_eq!(
            c.classify(&make_event("us-west", 2, 3)).unwrap(),
            DriftSeverity::Medium
        );
        assert_eq!(
            c.classify(&make_event("us-west", 2, 10)).unwrap(),
            DriftSeverity::Medium
        );
    }

    #[test]
    fn high_severity_boundary() {
        let c = DefaultDriftClassifier;
        assert_eq!(
            c.classify(&make_event("ap-southeast", 2, 11)).unwrap(),
            DriftSeverity::High
        );
        assert_eq!(
            c.classify(&make_event("sa-east", 2, 999)).unwrap(),
            DriftSeverity::High
        );
    }

    #[test]
    fn invalid_region_rejected() {
        let c = DefaultDriftClassifier;
        let ev = make_event("cn-north", 2, 5);
        assert!(matches!(
            c.classify(&ev),
            Err(DriftConsumerError::InvalidRegion(_))
        ));
    }

    #[test]
    fn unrecognised_exit_code_rejected() {
        let c = DefaultDriftClassifier;
        let ev = make_event("us-east", 3, 0);
        assert!(matches!(
            c.classify(&ev),
            Err(DriftConsumerError::UnrecognisedExitCode(3))
        ));
    }

    #[test]
    fn all_regions_accepted() {
        let c = DefaultDriftClassifier;
        for region in REGIONS {
            let ev = make_event(region, 0, 0);
            assert!(c.classify(&ev).is_ok(), "region {region} should be valid");
        }
    }
}
