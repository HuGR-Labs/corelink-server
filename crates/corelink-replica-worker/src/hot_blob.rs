//! `HotBlob` + `ReplicaStatus` + `AggregationEntry` — D1 `hot_blobs` table row types.

use crate::region::Region;
use serde::{Deserialize, Serialize};

/// Replication status of a hot blob row in D1 `hot_blobs` table.
///
/// Maps to `replication_status TEXT CHECK (... IN ('pending', 'in_progress',
/// 'replicated', 'failed', 'evicted'))` constraint in migration `0027_hot_blobs.sql`.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::ReplicaStatus;
///
/// assert_eq!(ReplicaStatus::Pending.as_str(), "pending");
/// assert_eq!(ReplicaStatus::Replicated.as_str(), "replicated");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ReplicaStatus {
    /// Waiting for replication worker to pick up.
    Pending,
    /// Replication in progress (worker claimed this row).
    InProgress,
    /// Successfully replicated and hash verified.
    Replicated,
    /// Replication failed (all retries exhausted); SEV-2 alert raised.
    Failed,
    /// Evicted from hot set (no longer in top 1% per latest aggregation).
    Evicted,
}

impl ReplicaStatus {
    /// Canonical SQL CHECK string value.
    pub fn as_str(self) -> &'static str {
        match self {
            ReplicaStatus::Pending => "pending",
            ReplicaStatus::InProgress => "in_progress",
            ReplicaStatus::Replicated => "replicated",
            ReplicaStatus::Failed => "failed",
            ReplicaStatus::Evicted => "evicted",
        }
    }
}

impl std::fmt::Display for ReplicaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A hot blob row from D1 `hot_blobs` table.
///
/// Populated by the offline aggregation daily job; consumed by the
/// replication worker cron (every 10 min).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{HotBlob, Region, ReplicaStatus};
///
/// let blob = HotBlob {
///     tenant_id: "tenant-eu-01".to_owned(),
///     blob_hash: "blake3-abc123".to_owned(),
///     primary_region: Region::Weur,
///     replica_region: Region::Sam,
///     bytes: 1_048_576,
///     access_count_30d: 4200,
///     last_access_ms: 1_700_000_000_000,
///     replicated_at_ms: None,
///     replication_status: ReplicaStatus::Pending,
/// };
/// assert_eq!(blob.primary_region, Region::Weur);
/// assert_eq!(blob.replica_region, Region::Sam);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotBlob {
    /// Tenant identifier (hashed in observability spans; plain in D1).
    pub tenant_id: String,
    /// BLAKE3 content-addressable hash of the blob.
    pub blob_hash: String,
    /// Tenant's primary pinned region (source of truth).
    pub primary_region: Region,
    /// Allowed sibling region for replication (residency-restricted).
    pub replica_region: Region,
    /// Total bytes for this blob.
    pub bytes: u64,
    /// Access count over last 30d window (from offline aggregation).
    pub access_count_30d: u64,
    /// Last access timestamp in milliseconds since epoch.
    pub last_access_ms: u64,
    /// Timestamp (ms since epoch) when replication completed; `None` if pending.
    pub replicated_at_ms: Option<u64>,
    /// Current replication lifecycle status.
    pub replication_status: ReplicaStatus,
}

/// A single tenant+blob aggregation result from the offline daily job.
///
/// Produced by `GROUP BY tenant_id, blob_hash; SUM(bytes) AS bytes_total;
/// COUNT(*) AS access_count_30d` over 30d audit log window.
///
/// **CRITICAL**: This is OFFLINE, NOT a live Prometheus metric.
/// DO NOT add `tenant_id` label to any live metric — use this struct
/// for per-tenant analysis. (INV-OBS-CARDINALITY-BUDGET S-09.)
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::AggregationEntry;
/// use corelink_replica_worker::Region;
///
/// let entry = AggregationEntry {
///     tenant_id: "tenant-us-01".to_owned(),
///     blob_hash: "blake3-xyz789".to_owned(),
///     primary_region: Region::Wnam,
///     bytes_total: 52_428_800,
///     access_count_30d: 9800,
/// };
/// assert!(entry.bytes_total > 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationEntry {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Blob hash.
    pub blob_hash: String,
    /// Primary region of this tenant.
    pub primary_region: Region,
    /// Total bytes transferred for this blob over the 30d window.
    pub bytes_total: u64,
    /// Access count over the 30d window.
    pub access_count_30d: u64,
}
