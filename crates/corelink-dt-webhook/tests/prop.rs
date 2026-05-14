//! Property tests for the DT webhook handler (WI-S12-005).
//!
//! # Coverage
//!
//! - `prop_dt_severity_classification`: 10k random CVSS scores → correct `DtSeverity` mapping.
//! - `prop_dt_hmac_verification`: 10k tampered payloads → 100% rejected.
//! - `prop_dt_alert_routing`: 10k events → routing matches severity rules.
//! - `prop_dt_dlq_replay`: 10k DLQ push/pop cycles → idempotent.
//!
//! Nightly CI overrides `PROPTEST_CASES=100000`.

use corelink_dt_webhook::{
    dlq::{DlqEntry, InMemoryDlq},
    hmac::{sign, verify_signature},
    severity::{classify_cvss, routing_channels},
    types::{
        AlertChannel, ComponentMetadata, DtEventType, DtProjectUuid, DtSeverity, DtWebhookError,
        DtWebhookEvent, VulnerabilityMetadata,
    },
};
use proptest::prelude::*;
use std::time::SystemTime;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

fn dummy_event(cvss: f64) -> DtWebhookEvent {
    DtWebhookEvent {
        event_type: DtEventType::NewVulnerability,
        project_uuid: DtProjectUuid::new("prop-test-project").expect("valid"),
        component: ComponentMetadata {
            purl: format!("pkg:cargo/ring@0.{:.0}.0", cvss * 10.0),
            name: "ring".into(),
            version: "0.16.0".into(),
            patched_locally: false,
            patched_locally_adr: None,
            adr_ratified_date: None,
        },
        vulnerability: VulnerabilityMetadata {
            cve_id: "CVE-2024-99999".into(),
            cvss_score: cvss,
            severity_label: None,
            description: None,
            sources: vec!["NVD".into()],
        },
        timestamp: SystemTime::now(),
    }
}

/// Expected severity for a given CVSS score (mirrors `classify_cvss`).
fn expected_severity(score: f64) -> DtSeverity {
    let score = score.max(0.0).min(10.0);
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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// prop_dt_severity_classification: random CVSS scores → correct DtSeverity.
    #[test]
    fn prop_dt_severity_classification(score in 0.0f64..=10.0f64) {
        let got = classify_cvss(score);
        let want = expected_severity(score);
        prop_assert_eq!(got, want.clone(), "CVSS {}: expected {:?}", score, want);
    }

    /// prop_dt_hmac_verification: tampered payloads rejected 100%.
    #[test]
    fn prop_dt_hmac_verification(
        secret in proptest::collection::vec(0u8..=255, 1..64),
        body in proptest::collection::vec(0u8..=255, 1..512),
        tamper_byte in 0u8..=255u8,
        tamper_pos in 0usize..512,
    ) {
        let sig = sign(&secret, &body).expect("sign");

        // Tamper body at a position within bounds.
        let mut tampered = body.clone();
        if !tampered.is_empty() {
            let pos = tamper_pos % tampered.len();
            // Flip the byte; if it doesn't change (unlikely), change to tamper_byte.
            if tampered[pos] == tamper_byte {
                tampered[pos] = tamper_byte.wrapping_add(1);
            } else {
                tampered[pos] = tamper_byte;
            }
            prop_assert_ne!(&tampered, &body, "tampered body must differ from original");
            let result = verify_signature(&secret, &tampered, &sig);
            prop_assert!(
                matches!(result, Err(DtWebhookError::HmacInvalid)),
                "Tampered body MUST be rejected"
            );
        }

        // Original body must verify.
        let result = verify_signature(&secret, &body, &sig);
        prop_assert!(result.is_ok(), "Valid HMAC MUST verify");
    }

    /// prop_dt_alert_routing: routing matches severity rules.
    #[test]
    fn prop_dt_alert_routing(score in 0.0f64..=10.0f64) {
        let severity = classify_cvss(score);
        let channels = routing_channels(&severity);
        match severity {
            DtSeverity::Critical => {
                prop_assert_eq!(channels.len(), 3);
                prop_assert!(channels.contains(&AlertChannel::PagerDuty));
                prop_assert!(channels.contains(&AlertChannel::Slack));
                prop_assert!(channels.contains(&AlertChannel::Email));
            }
            DtSeverity::High => {
                prop_assert_eq!(channels.len(), 2);
                prop_assert!(!channels.contains(&AlertChannel::PagerDuty));
                prop_assert!(channels.contains(&AlertChannel::Slack));
                prop_assert!(channels.contains(&AlertChannel::Email));
            }
            DtSeverity::Medium => {
                prop_assert_eq!(channels.len(), 1);
                prop_assert_eq!(&channels[0], &AlertChannel::Slack);
            }
            DtSeverity::Low | DtSeverity::Info => {
                prop_assert!(channels.is_empty());
            }
            _ => {}
        }
    }

    /// prop_dt_dlq_replay: push N events, drain all, assert idempotent.
    #[test]
    fn prop_dt_dlq_replay(
        n in 1usize..=50,
        cvss_values in proptest::collection::vec(0.0f64..=10.0f64, 1..=50),
    ) {
        let dlq = InMemoryDlq::new();
        let limit = n.min(cvss_values.len());

        for cvss in cvss_values.iter().take(limit) {
            let entry = DlqEntry {
                event: dummy_event(*cvss),
                attempt_count: 1,
                last_error: "test".into(),
            };
            dlq.push(entry).expect("push within cap");
        }

        prop_assert_eq!(dlq.len().expect("len"), limit);

        // Drain all.
        let drained = dlq.drain_all().expect("drain");
        prop_assert_eq!(drained.len(), limit);
        prop_assert!(dlq.is_empty().expect("empty"));

        // Re-push drained events (idempotency of replay).
        for entry in drained {
            dlq.push(DlqEntry {
                event: entry.event,
                attempt_count: entry.attempt_count + 1,
                last_error: entry.last_error,
            })
            .expect("re-push");
        }
        prop_assert_eq!(dlq.len().expect("len after replay"), limit);
    }
}
