//! Migration types for single-region → multi-region tenant migration.
//!
//! WI-S14-001 §6.1 — migration script dry-run / execute / rollback paths.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::region::Region;

/// Migration decision taxonomy — `#[non_exhaustive]` per project convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MigrationDecision {
    /// Tenant will be migrated (D1 row + R2 blob copy + hash verify).
    Migrate,
    /// Tenant skipped — already in target region (idempotent re-run safe).
    SkippedAlreadyMigrated,
    /// Tenant skipped — filtered out by `--tenant-id-filter` argument.
    SkippedFiltered,
    /// Tenant migration failed; rollback for this tenant initiated.
    Failed,
}

impl MigrationDecision {
    /// String label for audit / metrics.
    #[must_use]
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Migrate => "migrate",
            Self::SkippedAlreadyMigrated => "skipped_already_migrated",
            Self::SkippedFiltered => "skipped_filtered",
            Self::Failed => "failed",
        }
    }
}

/// Per-tenant migration result in a migration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantMigrationResult {
    /// Tenant ID.
    pub tenant_id: String,
    /// Source region (original single-region).
    pub source_region: String,
    /// Target region.
    pub target_region: String,
    /// Decision made for this tenant.
    pub decision: MigrationDecision,
    /// D1 row count migrated.
    pub d1_rows_migrated: u64,
    /// R2 blob count migrated.
    pub r2_blobs_migrated: u64,
    /// Hash verification passed.
    pub hash_verified: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Dry-run or execution migration report.
///
/// Produced by migration script in both `--dry-run` and `--execute` modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    /// Unique run ID (UUIDv7).
    pub run_id: Uuid,
    /// Whether this was a dry-run (no actual data moved).
    pub is_dry_run: bool,
    /// Target region for this migration run.
    pub target_region: String,
    /// Optional tenant ID filter applied.
    pub tenant_id_filter: Option<String>,
    /// Epoch ms when run started.
    pub started_at_ms: u64,
    /// Epoch ms when run completed (None if still in progress).
    pub completed_at_ms: Option<u64>,
    /// Per-tenant results.
    pub tenant_results: Vec<TenantMigrationResult>,
    /// Total D1 rows migrated.
    pub total_d1_rows: u64,
    /// Total R2 blobs migrated.
    pub total_r2_blobs: u64,
    /// Estimated duration for dry-run (ms).
    pub estimated_duration_ms: Option<u64>,
    /// Error count.
    pub error_count: u64,
    /// Whether rollback was triggered.
    pub rollback_triggered: bool,
}

impl MigrationReport {
    /// Create a new dry-run report scaffold.
    #[must_use]
    pub fn new_dry_run(
        target_region: Region,
        tenant_id_filter: Option<String>,
        started_at_ms: u64,
    ) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            is_dry_run: true,
            target_region: target_region.as_str().to_owned(),
            tenant_id_filter,
            started_at_ms,
            completed_at_ms: None,
            tenant_results: Vec::new(),
            total_d1_rows: 0,
            total_r2_blobs: 0,
            estimated_duration_ms: None,
            error_count: 0,
            rollback_triggered: false,
        }
    }

    /// Create a new execute report scaffold.
    #[must_use]
    pub fn new_execute(
        target_region: Region,
        tenant_id_filter: Option<String>,
        started_at_ms: u64,
    ) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            is_dry_run: false,
            target_region: target_region.as_str().to_owned(),
            tenant_id_filter,
            started_at_ms,
            completed_at_ms: None,
            tenant_results: Vec::new(),
            total_d1_rows: 0,
            total_r2_blobs: 0,
            estimated_duration_ms: None,
            error_count: 0,
            rollback_triggered: false,
        }
    }

    /// Add a tenant result and update counters.
    pub fn add_tenant_result(&mut self, result: TenantMigrationResult) {
        self.total_d1_rows += result.d1_rows_migrated;
        self.total_r2_blobs += result.r2_blobs_migrated;
        if result.decision == MigrationDecision::Failed {
            self.error_count += 1;
        }
        self.tenant_results.push(result);
    }

    /// Mark report as completed.
    pub fn complete(&mut self, completed_at_ms: u64) {
        self.completed_at_ms = Some(completed_at_ms);
    }

    /// Total tenants processed.
    #[must_use]
    pub fn tenant_count(&self) -> usize {
        self.tenant_results.len()
    }

    /// Count by decision.
    #[must_use]
    pub fn count_by_decision(&self, decision: MigrationDecision) -> usize {
        self.tenant_results
            .iter()
            .filter(|r| r.decision == decision)
            .count()
    }

    /// Whether this report has any failures.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.error_count > 0
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_decision_labels() {
        assert_eq!(MigrationDecision::Migrate.as_label(), "migrate");
        assert_eq!(
            MigrationDecision::SkippedAlreadyMigrated.as_label(),
            "skipped_already_migrated"
        );
        assert_eq!(MigrationDecision::Failed.as_label(), "failed");
    }

    #[test]
    fn test_migration_report_counters() {
        let mut report = MigrationReport::new_dry_run(Region::Weur, None, 1_000_000);
        assert!(report.is_dry_run);
        assert_eq!(report.target_region, "weur");

        report.add_tenant_result(TenantMigrationResult {
            tenant_id: "tenant-001".to_owned(),
            source_region: "wnam".to_owned(),
            target_region: "weur".to_owned(),
            decision: MigrationDecision::Migrate,
            d1_rows_migrated: 150,
            r2_blobs_migrated: 42,
            hash_verified: true,
            duration_ms: 5_000,
            error: None,
        });

        report.add_tenant_result(TenantMigrationResult {
            tenant_id: "tenant-002".to_owned(),
            source_region: "wnam".to_owned(),
            target_region: "weur".to_owned(),
            decision: MigrationDecision::Failed,
            d1_rows_migrated: 0,
            r2_blobs_migrated: 0,
            hash_verified: false,
            duration_ms: 1_000,
            error: Some("D1 write timeout".to_owned()),
        });

        assert_eq!(report.tenant_count(), 2);
        assert_eq!(report.total_d1_rows, 150);
        assert_eq!(report.total_r2_blobs, 42);
        assert_eq!(report.error_count, 1);
        assert!(report.has_failures());
        assert_eq!(report.count_by_decision(MigrationDecision::Migrate), 1);
        assert_eq!(report.count_by_decision(MigrationDecision::Failed), 1);
    }

    #[test]
    fn test_migration_report_idempotent_skip() {
        let mut report =
            MigrationReport::new_execute(Region::Sam, Some("eu_*".to_owned()), 2_000_000);
        assert!(!report.is_dry_run);

        report.add_tenant_result(TenantMigrationResult {
            tenant_id: "tenant-eu-001".to_owned(),
            source_region: "wnam".to_owned(),
            target_region: "sam".to_owned(),
            decision: MigrationDecision::SkippedAlreadyMigrated,
            d1_rows_migrated: 0,
            r2_blobs_migrated: 0,
            hash_verified: false,
            duration_ms: 10,
            error: None,
        });

        assert!(!report.has_failures());
        assert_eq!(
            report.count_by_decision(MigrationDecision::SkippedAlreadyMigrated),
            1
        );
    }
}
