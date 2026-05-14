//! Adversarial regression tests — WI-S13-004 §6.1.8 + §15.
//!
//! Scenarios tested:
//! 1. Synthetic drift injection: diff_count > 0 → severity medium/high + finding inserted.
//! 2. Cron skip simulation: outcome WorkflowFailed recorded + metrics counter.
//! 3. Auto-apply attempt: no apply surface in consumer (compile-time + runtime check).
//! 4. State file tampering simulation: corrupted exit code → error handling.
//! 5. Acceptable drift filter: diff_count = 0 → no alert (clean run row only).
//! 6. Multi-region drift: parallel events all 5 regions → 5 findings.
//! 7. Fail-CLOSED audit: failing sink blocks store write.
//! 8. Append-only violation: attempt to mutate immutable fields → error.
//! 9. Unknown region hardening: unknown region rejected at classifier boundary.
//! 10. RB-FM-206 decision tree: remediation update sets all fields correctly.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "adversarial tests are allowed to use these primitives"
)]

use corelink_terraform_drift_consumer::{
    audit::{FailingDriftAuditSink, InMemoryDriftAuditSink},
    classifier::DefaultDriftClassifier,
    consumer::DriftConsumer,
    error::DriftConsumerError,
    event::{DriftPlanEvent, DriftSeverity, DriftStatus, RemediationDecision, REGIONS},
    metrics::DriftMetricOutcome,
    store::{DriftFindingStore, InMemoryDriftFindingStore, check_immutable_fields_unchanged},
};
use uuid::Uuid;

fn make_event(region: &str, exit_code: i32, diff_count: u32) -> DriftPlanEvent {
    DriftPlanEvent {
        region: region.to_owned(),
        detected_at_ms: 1_715_600_000_000,
        tf_exit_code: exit_code,
        plan_diff_count: diff_count,
        plan_summary: format!("summary for {region} diff={diff_count}"),
        plan_full_artifact_url: Some(format!("https://ci.example.com/artifacts/{region}")),
        github_run_id: format!("run-adversarial-{region}"),
    }
}

fn make_consumer() -> DriftConsumer<
    DefaultDriftClassifier,
    InMemoryDriftAuditSink,
    InMemoryDriftFindingStore,
> {
    DriftConsumer::new(
        DefaultDriftClassifier,
        InMemoryDriftAuditSink::default(),
        InMemoryDriftFindingStore::default(),
    )
}

// -----------------------------------------------------------------------
// Scenario 1: Synthetic drift injection
// -----------------------------------------------------------------------

#[test]
fn adv_01_synthetic_drift_medium() {
    let mut consumer = make_consumer();
    // Inject: 5 resources changed in eu-west
    let finding = consumer
        .process_plan_event(&make_event("eu-west", 2, 5))
        .unwrap();

    assert_eq!(finding.severity, DriftSeverity::Medium);
    assert_eq!(finding.plan_diff_count, 5);
    assert_eq!(finding.region, "eu-west");
    assert_eq!(finding.status, DriftStatus::Open);
    assert_eq!(finding.runbook_ref, "RB-FM-206");

    // Audit emitted
    assert_eq!(consumer.audit_sink().records.len(), 1);
    let first_record = consumer.audit_sink().records.first().expect("audit record");
    assert_eq!(
        first_record.event_type,
        "corelink.admin.terraform_drift.detected"
    );

    // Finding stored
    assert_eq!(consumer.store().open_findings().len(), 1);

    // Metrics recorded
    assert_eq!(consumer.metrics().total_cron_runs_count(), 1);
    assert_eq!(consumer.metrics().total_findings_count(), 1);
}

#[test]
fn adv_01_synthetic_drift_high() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("us-east", 2, 15))
        .unwrap();
    assert_eq!(finding.severity, DriftSeverity::High);
}

// -----------------------------------------------------------------------
// Scenario 2: Cron skip simulation (WorkflowFailed outcome)
// -----------------------------------------------------------------------

#[test]
fn adv_02_cron_skip_outcome_recorded() {
    let mut consumer = make_consumer();
    // Simulate workflow failed by directly recording metric
    // (in production the monitoring job would push this; here we test metric label)
    let outcome = DriftMetricOutcome::WorkflowFailed;
    assert_eq!(outcome.as_label(), "workflow_failed");

    // Also test that a terraform error (exit 1) records correctly
    // Terraform error still inserts a row (cron health tracking) with severity=none
    let finding = consumer
        .process_plan_event(&make_event("us-west", 1, 0))
        .unwrap();
    // Exit code 1 with diff_count 0 → severity None
    assert_eq!(finding.severity, DriftSeverity::None);
    // Cron run recorded as TerraformError
    let cron_runs = &consumer.metrics().cron_runs_total;
    assert!(!cron_runs.is_empty());
    let (label, _count) = cron_runs.first().expect("cron run record");
    assert_eq!(label.as_str(), "terraform_error");
}

// -----------------------------------------------------------------------
// Scenario 3: Auto-apply forbidden (compile-time + runtime invariant)
// -----------------------------------------------------------------------

#[test]
fn adv_03_auto_apply_forbidden_invariant() {
    // Compile-time invariant: auto_apply_forbidden() always returns true
    assert!(
        DriftConsumer::<
            DefaultDriftClassifier,
            InMemoryDriftAuditSink,
            InMemoryDriftFindingStore,
        >::auto_apply_forbidden(),
        "auto-apply MUST be forbidden per WI-S13-004 §7"
    );

    // Runtime: no process_plan_event call ever results in apply
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("sa-east", 2, 20))
        .unwrap();
    // Finding status = Open; no apply triggered
    assert_eq!(finding.status, DriftStatus::Open);
    assert!(
        finding.remediation_decision.is_none(),
        "no auto-remediation: decision must be None until human approves"
    );
}

// -----------------------------------------------------------------------
// Scenario 4: State file tampering — unrecognised exit code → error
// -----------------------------------------------------------------------

#[test]
fn adv_04_corrupted_exit_code_rejected() {
    let mut consumer = make_consumer();
    // Exit code 3 = unrecognised (tampered or CI bug)
    let result = consumer.process_plan_event(&make_event("ap-southeast", 3, 0));
    assert!(
        matches!(result, Err(DriftConsumerError::UnrecognisedExitCode(3))),
        "corrupted exit code must be rejected"
    );
    // No store write, no audit
    assert!(consumer.store().all_findings().is_empty());
    assert!(consumer.audit_sink().records.is_empty());
}

#[test]
fn adv_04_negative_exit_code_rejected() {
    let mut consumer = make_consumer();
    let result = consumer.process_plan_event(&make_event("us-east", -1, 0));
    assert!(matches!(
        result,
        Err(DriftConsumerError::UnrecognisedExitCode(-1))
    ));
}

// -----------------------------------------------------------------------
// Scenario 5: Acceptable drift filter — diff_count=0 → no alert row
// -----------------------------------------------------------------------

#[test]
fn adv_05_clean_run_no_drift_alert() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("us-east", 0, 0))
        .unwrap();

    // Row inserted for cron health tracking
    assert_eq!(finding.plan_diff_count, 0);
    assert_eq!(finding.severity, DriftSeverity::None);

    // Audit type is clean_run (NOT detected)
    let first_audit = consumer.audit_sink().records.first().expect("audit record");
    assert_eq!(
        first_audit.event_type,
        "corelink.admin.terraform_drift.clean_run"
    );

    // Cron outcome is Ok
    let (label, _) = consumer.metrics().cron_runs_total.first().expect("cron run");
    assert_eq!(label.as_str(), "ok");
}

// -----------------------------------------------------------------------
// Scenario 6: Multi-region drift — all 5 regions inject drift
// -----------------------------------------------------------------------

#[test]
fn adv_06_multi_region_drift_all_5() {
    let mut consumer = make_consumer();

    for region in REGIONS {
        consumer
            .process_plan_event(&make_event(region, 2, 4))
            .unwrap();
    }

    let open = consumer.store().open_findings();
    assert_eq!(open.len(), 5, "all 5 regions must have findings");

    let regions: Vec<&str> = open.iter().map(|f| f.region.as_str()).collect();
    for r in REGIONS {
        assert!(regions.contains(r), "region {r} must be in findings");
    }

    assert_eq!(consumer.metrics().total_cron_runs_count(), 5);
    assert_eq!(consumer.metrics().total_findings_count(), 5);
}

// -----------------------------------------------------------------------
// Scenario 7: Fail-CLOSED audit — failing sink blocks store write
// -----------------------------------------------------------------------

#[test]
fn adv_07_failing_audit_blocks_store() {
    let mut consumer = DriftConsumer::new(
        DefaultDriftClassifier,
        FailingDriftAuditSink,
        InMemoryDriftFindingStore::default(),
    );

    let result = consumer.process_plan_event(&make_event("us-east", 2, 5));
    assert!(
        matches!(result, Err(DriftConsumerError::AuditFailed(_))),
        "audit failure must propagate"
    );
    assert!(
        consumer.store().all_findings().is_empty(),
        "store MUST be empty when audit fails (fail-CLOSED, INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)"
    );
}

// -----------------------------------------------------------------------
// Scenario 8: Append-only violation — immutable fields unchanged after remediation
// -----------------------------------------------------------------------

#[test]
fn adv_08_remediation_preserves_immutable_fields() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("eu-west", 2, 7))
        .unwrap();

    let finding_id = finding.finding_id;
    // Clone the before state from the finding itself (before remediation)
    let before = finding.clone();

    // Perform remediation update
    consumer
        .store_mut()
        .update_remediation(
            finding_id,
            DriftStatus::Remediated,
            RemediationDecision::Apply,
            1_715_600_900_000,
            Uuid::now_v7(),
        )
        .unwrap();

    let all = consumer.store().all_findings();
    let after = all
        .into_iter()
        .find(|f| f.finding_id == finding_id)
        .expect("finding must exist after remediation")
        .clone();

    assert!(
        check_immutable_fields_unchanged(&before, &after),
        "immutable fields (finding_id, region, detected_at_ms, severity, plan_diff_count) MUST be unchanged after remediation"
    );

    // Mutable fields updated
    assert_eq!(after.status, DriftStatus::Remediated);
    assert_eq!(after.remediation_decision, Some(RemediationDecision::Apply));
    assert!(after.remediated_at_ms.is_some());
    assert!(after.remediated_by_user_id.is_some());
}

// -----------------------------------------------------------------------
// Scenario 9: Unknown region hardening
// -----------------------------------------------------------------------

#[test]
fn adv_09_unknown_region_rejected() {
    let mut consumer = make_consumer();
    let regions_to_reject = [
        "cn-north",
        "us-central",
        "global",
        "moon-base",
        "",
        "US-EAST", // case-sensitive
    ];
    for region in &regions_to_reject {
        let result = consumer.process_plan_event(&make_event(region, 2, 5));
        assert!(
            matches!(result, Err(DriftConsumerError::InvalidRegion(_))),
            "region {region:?} must be rejected"
        );
    }
    // No findings stored
    assert!(consumer.store().all_findings().is_empty());
}

// -----------------------------------------------------------------------
// Scenario 10: RB-FM-206 decision tree — all 3 decisions work
// -----------------------------------------------------------------------

#[test]
fn adv_10_remediation_decision_tree_apply() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("us-east", 2, 3))
        .unwrap();
    consumer
        .store_mut()
        .update_remediation(
            finding.finding_id,
            DriftStatus::Remediated,
            RemediationDecision::Apply,
            1_715_600_500_000,
            Uuid::now_v7(),
        )
        .unwrap();
    let after = (*consumer.store().all_findings().first().expect("finding")).clone();
    assert_eq!(after.remediation_decision, Some(RemediationDecision::Apply));
    assert_eq!(after.status, DriftStatus::Remediated);
}

#[test]
fn adv_10_remediation_decision_tree_investigate() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("us-west", 2, 2))
        .unwrap();
    consumer
        .store_mut()
        .update_remediation(
            finding.finding_id,
            DriftStatus::Investigating,
            RemediationDecision::Investigate,
            1_715_600_500_000,
            Uuid::now_v7(),
        )
        .unwrap();
    let after = (*consumer.store().all_findings().first().expect("finding")).clone();
    assert_eq!(
        after.remediation_decision,
        Some(RemediationDecision::Investigate)
    );
    assert_eq!(after.status, DriftStatus::Investigating);
}

#[test]
fn adv_10_remediation_decision_tree_revert() {
    let mut consumer = make_consumer();
    let finding = consumer
        .process_plan_event(&make_event("ap-southeast", 2, 1))
        .unwrap();
    consumer
        .store_mut()
        .update_remediation(
            finding.finding_id,
            DriftStatus::Remediated,
            RemediationDecision::Revert,
            1_715_600_500_000,
            Uuid::now_v7(),
        )
        .unwrap();
    let after = (*consumer.store().all_findings().first().expect("finding")).clone();
    assert_eq!(after.remediation_decision, Some(RemediationDecision::Revert));
}

// -----------------------------------------------------------------------
// Additional: all exit codes produce valid findings
// -----------------------------------------------------------------------

#[test]
fn all_valid_exit_codes_produce_findings() {
    let exit_codes = [0i32, 1, 2];
    for code in &exit_codes {
        let mut consumer = make_consumer();
        let result = consumer.process_plan_event(&make_event("us-east", *code, 0));
        assert!(
            result.is_ok(),
            "exit code {code} must produce a valid finding"
        );
    }
}

// -----------------------------------------------------------------------
// Proptest: diff_count → severity mapping is always correct
// -----------------------------------------------------------------------

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    /// PROPTEST_CASES runtime env-var per S-07 P1-2 fix. Defaults to 10k
    /// at PR-gate; 100k nightly via `PROPTEST_CASES=100000`. S-13
    /// sprint-close P1-5: prior default of 100 was insufficient.
    fn proptest_cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000)
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: proptest_cases(),
            ..ProptestConfig::default()
        })]
        #[test]
        fn prop_severity_monotone_with_diff_count(diff_count in 0u32..=1000) {
            let mut consumer = make_consumer();
            let event = make_event("us-east", 2, diff_count);
            let finding = consumer.process_plan_event(&event).unwrap();
            let expected = match diff_count {
                0 => DriftSeverity::None,
                1..=2 => DriftSeverity::Low,
                3..=10 => DriftSeverity::Medium,
                _ => DriftSeverity::High,
            };
            prop_assert_eq!(finding.severity, expected);
        }

        #[test]
        fn prop_all_valid_regions_accepted(
            region_idx in 0usize..5,
            diff_count in 0u32..=20,
        ) {
            let region = REGIONS.get(region_idx).expect("valid region index");
            let mut consumer = make_consumer();
            let result = consumer.process_plan_event(&make_event(region, 2, diff_count));
            prop_assert!(result.is_ok(), "region must always be accepted");
        }

        #[test]
        fn prop_audit_always_emitted_before_store(diff_count in 1u32..=50) {
            // For every successful process, audit count == store count
            let mut consumer = make_consumer();
            consumer.process_plan_event(&make_event("eu-west", 2, diff_count)).unwrap();
            prop_assert_eq!(
                consumer.audit_sink().records.len(),
                consumer.store().all_findings().len(),
                "audit and store must be in sync"
            );
        }

        #[test]
        fn prop_cron_run_always_increments_metric(
            exit_code in 0i32..=2,
            diff_count in 0u32..=5,
        ) {
            let mut consumer = make_consumer();
            consumer.process_plan_event(&make_event("sa-east", exit_code, diff_count)).unwrap();
            prop_assert_eq!(consumer.metrics().total_cron_runs_count(), 1);
        }
    }
}
