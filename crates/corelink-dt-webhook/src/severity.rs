//! CVSS-to-[`DtSeverity`] classification and alert routing rules.
//!
//! # Routing Rules
//!
//! | Severity  | CVSS range | Channels                          |
//! |-----------|------------|-----------------------------------|
//! | Critical  | 9.0–10.0   | Slack + Email + PagerDuty SEV-2   |
//! | High      | 7.0–8.9    | Slack + Email                     |
//! | Medium    | 4.0–6.9    | Slack only                        |
//! | Low       | 0.1–3.9    | Log only (no channel delivery)    |
//! | Info      | 0.0        | Log only (no channel delivery)    |

use crate::types::{AlertChannel, DtSeverity};

/// Classify a CVSS v3.x base score into a [`DtSeverity`].
///
/// Score must be in the range `[0.0, 10.0]`. Scores outside that range are
/// clamped to the nearest boundary before classification.
pub fn classify_cvss(score: f64) -> DtSeverity {
    let score = score.clamp(0.0_f64, 10.0_f64);
    if score >= 9.0 {
        DtSeverity::Critical
    } else if score >= 7.0 {
        DtSeverity::High
    } else if score >= 4.0 {
        DtSeverity::Medium
    } else if score > 0.0 {
        DtSeverity::Low
    } else {
        DtSeverity::Info
    }
}

/// Return the alert channels for a given severity.
///
/// Returns an empty slice for `Low` and `Info` (log-only; no channel delivery).
pub fn routing_channels(severity: &DtSeverity) -> Vec<AlertChannel> {
    match severity {
        DtSeverity::Critical => vec![AlertChannel::Slack, AlertChannel::Email, AlertChannel::PagerDuty],
        DtSeverity::High => vec![AlertChannel::Slack, AlertChannel::Email],
        DtSeverity::Medium => vec![AlertChannel::Slack],
        DtSeverity::Low | DtSeverity::Info => vec![],
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn boundary_critical_low() {
        assert_eq!(classify_cvss(9.0), DtSeverity::Critical);
    }

    #[test]
    fn boundary_critical_high() {
        assert_eq!(classify_cvss(10.0), DtSeverity::Critical);
    }

    #[test]
    fn boundary_high_low() {
        assert_eq!(classify_cvss(7.0), DtSeverity::High);
    }

    #[test]
    fn boundary_high_high() {
        assert_eq!(classify_cvss(8.9), DtSeverity::High);
    }

    #[test]
    fn boundary_medium_low() {
        assert_eq!(classify_cvss(4.0), DtSeverity::Medium);
    }

    #[test]
    fn boundary_medium_high() {
        assert_eq!(classify_cvss(6.9), DtSeverity::Medium);
    }

    #[test]
    fn boundary_low() {
        assert_eq!(classify_cvss(0.1), DtSeverity::Low);
        assert_eq!(classify_cvss(3.9), DtSeverity::Low);
    }

    #[test]
    fn boundary_info() {
        assert_eq!(classify_cvss(0.0), DtSeverity::Info);
    }

    #[test]
    fn clamp_above_10() {
        // NaN or very high value should be treated as Critical
        assert_eq!(classify_cvss(11.0), DtSeverity::Critical);
    }

    #[test]
    fn clamp_below_0() {
        assert_eq!(classify_cvss(-1.0), DtSeverity::Info);
    }

    #[test]
    fn routing_critical_has_three_channels() {
        let channels = routing_channels(&DtSeverity::Critical);
        assert_eq!(channels.len(), 3);
        assert!(channels.contains(&AlertChannel::PagerDuty));
    }

    #[test]
    fn routing_high_no_pagerduty() {
        let channels = routing_channels(&DtSeverity::High);
        assert_eq!(channels.len(), 2);
        assert!(!channels.contains(&AlertChannel::PagerDuty));
    }

    #[test]
    fn routing_medium_slack_only() {
        let channels = routing_channels(&DtSeverity::Medium);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0], AlertChannel::Slack);
    }

    #[test]
    fn routing_low_empty() {
        assert!(routing_channels(&DtSeverity::Low).is_empty());
    }

    #[test]
    fn routing_info_empty() {
        assert!(routing_channels(&DtSeverity::Info).is_empty());
    }
}
