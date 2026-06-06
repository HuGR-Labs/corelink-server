//! Drift consumer orchestrator — WI-S13-004.
//!
//! Pipeline: receive plan event → classify → audit emit (BEFORE store) →
//! store insert → metrics increment.
//!
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: audit fires before state mutation.
//! If audit fails → error propagated; no store write occurs.
//!
//! Auto-apply FORBIDDEN: this orchestrator has NO `terraform apply` surface.

use crate::audit::{DriftAuditEventType, DriftAuditRecord, DriftAuditSink};
use crate::classifier::DriftClassifier;
use crate::error::DriftConsumerError;
use crate::event::{DriftFinding, DriftPlanEvent};
use crate::metrics::{DriftMetricOutcome, DriftMetrics};
use crate::store::DriftFindingStore;

/// Drift consumer orchestrator.
#[derive(Debug)]
pub struct DriftConsumer<C, A, S>
where
    C: DriftClassifier,
    A: DriftAuditSink,
    S: DriftFindingStore,
{
    classifier: C,
    audit_sink: A,
    store: S,
    metrics: DriftMetrics,
}

impl<C, A, S> DriftConsumer<C, A, S>
where
    C: DriftClassifier,
    A: DriftAuditSink,
    S: DriftFindingStore,
{
    /// Construct a new consumer.
    #[must_use]
    pub fn new(classifier: C, audit_sink: A, store: S) -> Self {
        Self {
            classifier,
            audit_sink,
            store,
            metrics: DriftMetrics::default(),
        }
    }

    /// Process a plan event (main pipeline entry point).
    ///
    /// Returns the created [`DriftFinding`] on success.
    ///
    /// # Errors
    /// - [`DriftConsumerError::InvalidRegion`] — unknown region.
    /// - [`DriftConsumerError::UnrecognisedExitCode`] — bad exit code.
    /// - [`DriftConsumerError::AuditFailed`] — audit emit error (blocks store).
    /// - [`DriftConsumerError::StoreFailed`] — D1 insert error.
    pub fn process_plan_event(
        &mut self,
        event: &DriftPlanEvent,
    ) -> Result<DriftFinding, DriftConsumerError> {
        // Step 1: classify
        let severity = self.classifier.classify(event)?;

        // Step 2: build finding
        let finding = DriftFinding::from_event(event, severity);

        // Step 3: determine audit event type
        let audit_type = if finding.plan_diff_count > 0 {
            DriftAuditEventType::Detected
        } else {
            DriftAuditEventType::CleanRun
        };

        // Step 4: audit emit BEFORE state mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)
        let audit_record = DriftAuditRecord::from_finding(&finding, audit_type);
        self.audit_sink.emit(audit_record).map_err(|e| {
            DriftConsumerError::AuditFailed(format!("pre-insert audit failed: {e}"))
        })?;

        // Step 5: store insert
        self.store.insert(finding.clone())?;

        // Step 6: metrics
        let outcome = match event.tf_exit_code {
            0 => DriftMetricOutcome::Ok,
            2 => DriftMetricOutcome::DriftDetected,
            1 => DriftMetricOutcome::TerraformError,
            _ => DriftMetricOutcome::WorkflowFailed,
        };
        self.metrics.record_cron_run(outcome);
        self.metrics
            .record_finding(&finding.region, severity.as_label());

        Ok(finding)
    }

    /// Access the underlying metrics (for inspection in tests).
    #[must_use]
    pub fn metrics(&self) -> &DriftMetrics {
        &self.metrics
    }

    /// Access the underlying store (for inspection in tests).
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Access the audit sink (for inspection in tests).
    #[must_use]
    pub fn audit_sink(&self) -> &A {
        &self.audit_sink
    }

    /// Mutable access to the store (for test-only remediation updates).
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Check: this consumer MUST NOT have any terraform apply surface.
    /// This is a compile-time documentation invariant; called in tests.
    #[must_use]
    pub const fn auto_apply_forbidden() -> bool {
        // WI-S13-004 §7 anti-scope: "Auto-apply via CI (security property; never)"
        // No terraform apply step anywhere in this crate.
        true
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
    use crate::audit::InMemoryDriftAuditSink;
    use crate::classifier::DefaultDriftClassifier;
    use crate::event::DriftSeverity;
    use crate::store::InMemoryDriftFindingStore;

    fn make_event(region: &str, exit_code: i32, diff_count: u32) -> DriftPlanEvent {
        DriftPlanEvent {
            region: region.to_owned(),
            detected_at_ms: 1_715_600_000_000,
            tf_exit_code: exit_code,
            plan_diff_count: diff_count,
            plan_summary: "test summary".to_owned(),
            plan_full_artifact_url: Some("https://example.com/artifact".to_owned()),
            github_run_id: "run-42".to_owned(),
        }
    }

    fn make_consumer(
    ) -> DriftConsumer<DefaultDriftClassifier, InMemoryDriftAuditSink, InMemoryDriftFindingStore>
    {
        DriftConsumer::new(
            DefaultDriftClassifier,
            InMemoryDriftAuditSink::default(),
            InMemoryDriftFindingStore::default(),
        )
    }

    #[test]
    fn auto_apply_invariant() {
        assert!(DriftConsumer::<
            DefaultDriftClassifier,
            InMemoryDriftAuditSink,
            InMemoryDriftFindingStore,
        >::auto_apply_forbidden());
    }

    #[test]
    fn clean_run_inserts_none_severity_row() {
        let mut consumer = make_consumer();
        let finding = consumer
            .process_plan_event(&make_event("us-east", 0, 0))
            .unwrap();
        assert_eq!(finding.severity, DriftSeverity::None);
        assert_eq!(consumer.store().open_findings().len(), 1);
        assert_eq!(consumer.metrics().total_cron_runs_count(), 1);
        assert_eq!(consumer.audit_sink().records.len(), 1);
    }

    #[test]
    fn drift_detected_inserts_medium_severity() {
        let mut consumer = make_consumer();
        let finding = consumer
            .process_plan_event(&make_event("eu-west", 2, 5))
            .unwrap();
        assert_eq!(finding.severity, DriftSeverity::Medium);
        assert_eq!(
            consumer.metrics().total_findings_count(),
            1,
            "findings_total incremented"
        );
    }

    #[test]
    fn audit_emit_before_store_insert_invariant() {
        // Use FailingDriftAuditSink — store must NOT be written
        use crate::audit::FailingDriftAuditSink;
        let mut consumer = DriftConsumer::new(
            DefaultDriftClassifier,
            FailingDriftAuditSink,
            InMemoryDriftFindingStore::default(),
        );
        let result = consumer.process_plan_event(&make_event("us-west", 2, 3));
        assert!(
            matches!(result, Err(DriftConsumerError::AuditFailed(_))),
            "audit failure must propagate"
        );
        assert!(
            consumer.store().all_findings().is_empty(),
            "store must NOT be written if audit failed (fail-CLOSED)"
        );
    }

    #[test]
    fn invalid_region_rejected_before_audit() {
        let mut consumer = make_consumer();
        let result = consumer.process_plan_event(&make_event("moon-base", 2, 5));
        assert!(matches!(result, Err(DriftConsumerError::InvalidRegion(_))));
        assert!(consumer.audit_sink().records.is_empty(), "no audit emitted");
    }
}
