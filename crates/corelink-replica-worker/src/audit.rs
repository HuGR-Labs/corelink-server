//! Audit event taxonomy + sink trait for the replica worker.
//!
//! **Audit fail-CLOSED**: state is NEVER mutated if `emit` returns `Err`.
//! Ordering: `lookup → emit_audit → mutate_state`.

use serde::{Deserialize, Serialize};

/// 8-canonical audit event taxonomy for the replica worker.
///
/// CloudEvents type strings per `observability_model.md §3.1` +
/// Lote 10.9bis P0-G prefix (`dev.hugr.corelink.*`).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::ReplicaAuditEventType;
///
/// assert_eq!(
///     ReplicaAuditEventType::ReplicationStarted.as_str(),
///     "corelink.region.replication.started"
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReplicaAuditEventType {
    /// Replication of a single blob started.
    ReplicationStarted,
    /// Replication of a single blob completed (hash verified).
    ReplicationCompleted,
    /// Replication of a single blob failed (all retries exhausted).
    ReplicationFailed,
    /// Offline aggregation daily job started.
    AggregationStarted,
    /// Offline aggregation daily job completed successfully.
    AggregationCompleted,
    /// Offline aggregation daily job failed.
    AggregationFailed,
    /// Region failover detected (primary degraded → replica reads).
    FailoverDetected,
    /// Region failover resolved (primary healthy → normal reads).
    FailoverResolved,
}

impl ReplicaAuditEventType {
    /// CloudEvents type string.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::ReplicaAuditEventType;
    ///
    /// assert!(ReplicaAuditEventType::FailoverDetected.as_str().contains("failover"));
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            ReplicaAuditEventType::ReplicationStarted => "corelink.region.replication.started",
            ReplicaAuditEventType::ReplicationCompleted => "corelink.region.replication.completed",
            ReplicaAuditEventType::ReplicationFailed => "corelink.region.replication.failed",
            ReplicaAuditEventType::AggregationStarted => "corelink.hot_blob.aggregation.started",
            ReplicaAuditEventType::AggregationCompleted => {
                "corelink.hot_blob.aggregation.completed"
            }
            ReplicaAuditEventType::AggregationFailed => "corelink.hot_blob.aggregation.failed",
            ReplicaAuditEventType::FailoverDetected => "corelink.region.failover.detected",
            ReplicaAuditEventType::FailoverResolved => "corelink.region.failover.resolved",
        }
    }
}

impl std::fmt::Display for ReplicaAuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Audit record emitted by the replica worker.
///
/// `tenant_id_hash` is the hashed tenant identifier (SHA-256 of raw ID)
/// for observability spans; plain `tenant_id` stays in D1 only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaAuditRecord {
    /// CloudEvents type.
    pub event_type: ReplicaAuditEventType,
    /// Hashed tenant identifier (SHA-256 hex; NOT raw tenant_id in spans).
    pub tenant_id_hash: String,
    /// BLAKE3 hash of the blob (or empty string for aggregation events).
    pub blob_hash: String,
    /// Primary region identifier.
    pub primary_region: String,
    /// Replica region identifier (or empty string for aggregation events).
    pub replica_region: String,
    /// Milliseconds since epoch when this audit record was created.
    pub timestamp_ms: u64,
    /// Human-readable detail (e.g., error message for failure events).
    pub detail: String,
}

/// Audit sink trait for the replica worker.
///
/// Implementations must be **fail-CLOSED**: if `emit` fails, the caller
/// must NOT mutate state (S-06 P0-2 lesson).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{
///     InMemoryReplicaAuditSink, ReplicaAuditEventType, ReplicaAuditRecord, ReplicaAuditSink,
/// };
/// use std::sync::Arc;
///
/// let sink = Arc::new(InMemoryReplicaAuditSink::new());
/// let record = ReplicaAuditRecord {
///     event_type: ReplicaAuditEventType::ReplicationStarted,
///     tenant_id_hash: "hash-01".to_owned(),
///     blob_hash: "blake3-abc".to_owned(),
///     primary_region: "weur".to_owned(),
///     replica_region: "sam".to_owned(),
///     timestamp_ms: 0,
///     detail: String::new(),
/// };
/// sink.emit(record).unwrap();
/// assert_eq!(sink.records().len(), 1);
/// ```
pub trait ReplicaAuditSink: std::fmt::Debug + Send + Sync {
    /// Emit an audit record. Must be called BEFORE state mutation (fail-CLOSED).
    fn emit(&self, record: ReplicaAuditRecord) -> Result<(), String>;
}

/// In-memory audit sink for tests and local orchestration.
///
/// Thread-safe via `std::sync::Mutex`.
#[derive(Debug)]
pub struct InMemoryReplicaAuditSink {
    records: std::sync::Mutex<Vec<ReplicaAuditRecord>>,
}

impl InMemoryReplicaAuditSink {
    /// Create a new empty in-memory audit sink.
    pub fn new() -> Self {
        InMemoryReplicaAuditSink {
            records: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns a snapshot of all emitted records.
    pub fn records(&self) -> Vec<ReplicaAuditRecord> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Default for InMemoryReplicaAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicaAuditSink for InMemoryReplicaAuditSink {
    fn emit(&self, record: ReplicaAuditRecord) -> Result<(), String> {
        self.records.lock().map_err(|e| e.to_string())?.push(record);
        Ok(())
    }
}

/// Adversarial audit sink that always fails.
///
/// Used to verify fail-CLOSED semantics: state must NOT be mutated
/// when `emit` returns `Err`.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{FailingReplicaAuditSink, ReplicaAuditEventType, ReplicaAuditRecord, ReplicaAuditSink};
///
/// let sink = FailingReplicaAuditSink::new("forced failure");
/// let record = ReplicaAuditRecord {
///     event_type: ReplicaAuditEventType::ReplicationStarted,
///     tenant_id_hash: String::new(),
///     blob_hash: String::new(),
///     primary_region: String::new(),
///     replica_region: String::new(),
///     timestamp_ms: 0,
///     detail: String::new(),
/// };
/// assert!(sink.emit(record).is_err());
/// ```
#[derive(Debug)]
pub struct FailingReplicaAuditSink {
    message: String,
}

impl FailingReplicaAuditSink {
    /// Create a failing sink with the given error message.
    pub fn new(message: impl Into<String>) -> Self {
        FailingReplicaAuditSink {
            message: message.into(),
        }
    }
}

impl ReplicaAuditSink for FailingReplicaAuditSink {
    fn emit(&self, _record: ReplicaAuditRecord) -> Result<(), String> {
        Err(self.message.clone())
    }
}
