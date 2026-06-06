//! CloudEvent types for region provisioning + migration — WI-S14-001 §6.1.8.
//!
//! CloudEvent types emitted:
//! - `corelink.region.provisioned`
//! - `corelink.region.migration.started`
//! - `corelink.region.migration.tenant_completed`
//! - `corelink.region.migration.completed`
//! - `corelink.region.outage.detected`
//! - `corelink.region.outage.resolved`

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::region::Region;

/// CloudEvent type strings — WI-S14-001 §6.1.8.
pub const EVT_REGION_PROVISIONED: &str = "corelink.region.provisioned";
/// Migration started event.
pub const EVT_MIGRATION_STARTED: &str = "corelink.region.migration.started";
/// Per-tenant migration completed event.
pub const EVT_MIGRATION_TENANT_COMPLETED: &str = "corelink.region.migration.tenant_completed";
/// Full migration completed event.
pub const EVT_MIGRATION_COMPLETED: &str = "corelink.region.migration.completed";
/// Region outage detected event.
pub const EVT_OUTAGE_DETECTED: &str = "corelink.region.outage.detected";
/// Region outage resolved event.
pub const EVT_OUTAGE_RESOLVED: &str = "corelink.region.outage.resolved";
/// Terraform drift findings event.
pub const EVT_TERRAFORM_DRIFT: &str = "corelink.region.terraform_drift.detected";

/// Audit event type taxonomy — WI-S14-001 §6.1.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RegionAuditEventType {
    /// Terraform apply succeeded; region provisioned.
    Provisioned,
    /// Migration run started.
    MigrationStarted,
    /// Per-tenant migration step completed (with hash verification).
    MigrationTenantCompleted,
    /// Full migration run completed.
    MigrationCompleted,
    /// Region outage detected (chaos test or real).
    OutageDetected,
    /// Region outage resolved.
    OutageResolved,
    /// Terraform drift detected (per-region).
    TerraformDrift,
}

impl RegionAuditEventType {
    /// CloudEvent type string.
    #[must_use]
    pub fn as_cloud_event_type(&self) -> &'static str {
        match self {
            Self::Provisioned => EVT_REGION_PROVISIONED,
            Self::MigrationStarted => EVT_MIGRATION_STARTED,
            Self::MigrationTenantCompleted => EVT_MIGRATION_TENANT_COMPLETED,
            Self::MigrationCompleted => EVT_MIGRATION_COMPLETED,
            Self::OutageDetected => EVT_OUTAGE_DETECTED,
            Self::OutageResolved => EVT_OUTAGE_RESOLVED,
            Self::TerraformDrift => EVT_TERRAFORM_DRIFT,
        }
    }
}

/// Audit record for region provisioning events.
///
/// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: audit fires BEFORE state mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAuditRecord {
    /// CloudEvent spec version.
    pub specversion: String,
    /// CloudEvent type.
    pub event_type: String,
    /// Event source.
    pub source: String,
    /// Unique event ID (UUIDv7).
    pub id: Uuid,
    /// Epoch ms.
    pub time_ms: u64,
    /// Region this record covers.
    pub region: String,
    /// R2 bucket name (for provisioning events).
    pub r2_bucket_name: Option<String>,
    /// D1 instance ID (for provisioning events).
    pub d1_instance_id: Option<String>,
    /// DO namespace ID (for provisioning events).
    pub do_namespace_id: Option<String>,
    /// DO jurisdiction (for provisioning events).
    pub do_jurisdiction: Option<String>,
    /// Result of the operation.
    pub result: String,
    /// Tenant ID (hashed) for migration events — cardinality budget.
    pub tenant_id_hash: Option<String>,
}

impl RegionAuditRecord {
    /// Construct a provisioning audit record.
    #[must_use]
    pub fn provisioning(
        region: Region,
        r2_bucket_name: String,
        d1_instance_id: String,
        do_namespace_id: String,
        do_jurisdiction: String,
        time_ms: u64,
    ) -> Self {
        Self {
            specversion: "1.0".to_owned(),
            event_type: EVT_REGION_PROVISIONED.to_owned(),
            source: "corelink/region/terraform".to_owned(),
            id: Uuid::now_v7(),
            time_ms,
            region: region.as_str().to_owned(),
            r2_bucket_name: Some(r2_bucket_name),
            d1_instance_id: Some(d1_instance_id),
            do_namespace_id: Some(do_namespace_id),
            do_jurisdiction: Some(do_jurisdiction),
            result: "success".to_owned(),
            tenant_id_hash: None,
        }
    }

    /// Construct a migration tenant-completed audit record.
    #[must_use]
    pub fn migration_tenant_completed(
        region: Region,
        tenant_id_hash: String,
        time_ms: u64,
    ) -> Self {
        Self {
            specversion: "1.0".to_owned(),
            event_type: EVT_MIGRATION_TENANT_COMPLETED.to_owned(),
            source: "corelink/region/migration-script".to_owned(),
            id: Uuid::now_v7(),
            time_ms,
            region: region.as_str().to_owned(),
            r2_bucket_name: None,
            d1_instance_id: None,
            do_namespace_id: None,
            do_jurisdiction: None,
            result: "success".to_owned(),
            tenant_id_hash: Some(tenant_id_hash),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_all_event_types_have_cloud_event_string() {
        let types = [
            RegionAuditEventType::Provisioned,
            RegionAuditEventType::MigrationStarted,
            RegionAuditEventType::MigrationTenantCompleted,
            RegionAuditEventType::MigrationCompleted,
            RegionAuditEventType::OutageDetected,
            RegionAuditEventType::OutageResolved,
            RegionAuditEventType::TerraformDrift,
        ];
        for t in &types {
            let s = t.as_cloud_event_type();
            assert!(
                s.starts_with("corelink.region."),
                "expected corelink.region. prefix: {}",
                s
            );
        }
    }

    #[test]
    fn test_provisioning_record_fields() {
        let rec = RegionAuditRecord::provisioning(
            Region::Weur,
            "corelink-cas-weur".to_owned(),
            "d1-id-123".to_owned(),
            "do-weur".to_owned(),
            "eu".to_owned(),
            1_700_000_000_000,
        );
        assert_eq!(rec.region, "weur");
        assert_eq!(rec.do_jurisdiction.as_deref(), Some("eu"));
        assert_eq!(rec.specversion, "1.0");
        assert_eq!(rec.event_type, EVT_REGION_PROVISIONED);
    }
}
