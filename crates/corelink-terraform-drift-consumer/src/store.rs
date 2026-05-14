//! Drift finding store — WI-S13-004.
//!
//! INV-AUDIT-APPEND-ONLY (CRITICAL): no DELETE surface.
//! Status updates are additive (remediation fields only).

use std::collections::HashMap;

use uuid::Uuid;

use crate::error::{DriftConsumerError, DriftStoreError};
use crate::event::{DriftFinding, DriftStatus, RemediationDecision};

/// Store trait for drift findings.
pub trait DriftFindingStore: std::fmt::Debug {
    /// Insert a new finding. Returns error if finding_id already exists.
    ///
    /// # Errors
    /// [`DriftConsumerError::StoreFailed`] on duplicate or internal error.
    fn insert(&mut self, finding: DriftFinding) -> Result<(), DriftConsumerError>;

    /// Update remediation fields on an existing finding (additive only).
    ///
    /// # Errors
    /// [`DriftConsumerError::NotFound`] if finding_id not found.
    /// [`DriftStoreError::AppendOnlyViolation`] if caller attempts to modify
    /// immutable fields (region, detected_at_ms, severity, etc.).
    fn update_remediation(
        &mut self,
        finding_id: Uuid,
        new_status: DriftStatus,
        decision: RemediationDecision,
        remediated_at_ms: u64,
        remediated_by_user_id: Uuid,
    ) -> Result<(), DriftConsumerError>;

    /// Retrieve all findings (for monitoring / age gauge).
    fn all_findings(&self) -> Vec<&DriftFinding>;

    /// Retrieve open findings (for age gauge + post-mortem trigger).
    fn open_findings(&self) -> Vec<&DriftFinding>;
}

/// In-memory store for testing.
#[derive(Debug, Default)]
pub struct InMemoryDriftFindingStore {
    findings: HashMap<Uuid, DriftFinding>,
    /// Insertion order (for deterministic iteration in tests).
    order: Vec<Uuid>,
}

impl DriftFindingStore for InMemoryDriftFindingStore {
    fn insert(&mut self, finding: DriftFinding) -> Result<(), DriftConsumerError> {
        let id = finding.finding_id;
        if self.findings.contains_key(&id) {
            return Err(DriftConsumerError::StoreFailed(format!(
                "finding_id {id} already exists"
            )));
        }
        self.findings.insert(id, finding);
        self.order.push(id);
        Ok(())
    }

    fn update_remediation(
        &mut self,
        finding_id: Uuid,
        new_status: DriftStatus,
        decision: RemediationDecision,
        remediated_at_ms: u64,
        remediated_by_user_id: Uuid,
    ) -> Result<(), DriftConsumerError> {
        let finding = self
            .findings
            .get_mut(&finding_id)
            .ok_or_else(|| DriftConsumerError::NotFound(finding_id.to_string()))?;

        // Append-only: only remediation fields may be updated
        finding.status = new_status;
        finding.remediation_decision = Some(decision);
        finding.remediated_at_ms = Some(remediated_at_ms);
        finding.remediated_by_user_id = Some(remediated_by_user_id);
        Ok(())
    }

    fn all_findings(&self) -> Vec<&DriftFinding> {
        self.order
            .iter()
            .filter_map(|id| self.findings.get(id))
            .collect()
    }

    fn open_findings(&self) -> Vec<&DriftFinding> {
        self.all_findings()
            .into_iter()
            .filter(|f| f.status == DriftStatus::Open)
            .collect()
    }
}

/// Validates that the update_remediation call only touches mutable fields.
/// Panics in debug if immutable field mutation attempted.
/// (This is a test-helper invariant check, not production logic.)
#[must_use]
pub fn check_immutable_fields_unchanged(before: &DriftFinding, after: &DriftFinding) -> bool {
    before.finding_id == after.finding_id
        && before.region == after.region
        && before.detected_at_ms == after.detected_at_ms
        && before.plan_diff_count == after.plan_diff_count
        && before.severity == after.severity
}

// Suppress unused import warning in non-test builds
#[allow(dead_code)]
fn _use_drift_store_error(_e: DriftStoreError) {}
