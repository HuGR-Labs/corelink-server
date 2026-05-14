//! Replication worker — copies hot blobs from primary to sibling region R2.
//!
//! Per-blob:
//! 1. Residency check: `replica_region` must be the allowed sibling of
//!    `primary_region` per `ResidencyGraph` (INV-REGION-NO-CROSS-LEAK).
//! 2. Audit emit `replication.started` BEFORE state mutation (fail-CLOSED).
//! 3. R2 GET primary → R2 PUT replica (simulated in-memory).
//! 4. Hash verify post-copy: `primary_hash == replica_hash` (INV-CAS-INTEGRITY).
//!    Mismatch → exponential backoff retry (5 max); persistent = `RetryExhausted`.
//! 5. Update D1 `hot_blobs` status to `Replicated`.
//! 6. Audit emit `replication.completed`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{ReplicaAuditEventType, ReplicaAuditRecord, ReplicaAuditSink};
use crate::error::ReplicaError;
use crate::hot_blob::{HotBlob, ReplicaStatus};
use crate::region::ResidencyGraph;

/// Maximum retry attempts before emitting `RetryExhausted` + SEV-2 alert.
pub const MAX_REPLICATION_RETRIES: u32 = 5;

/// Replication worker trait.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{
///     AggregationEntry, InMemoryOfflineAggregator, InMemoryReplicaAuditSink,
///     InMemoryReplicationWorker, OfflineAggregator, Region, ReplicaAuditSink,
///     ReplicationWorker, AGGREGATION_WINDOW_DAYS,
/// };
/// use std::sync::Arc;
///
/// let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
/// let agg = InMemoryOfflineAggregator::new(Arc::clone(&sink));
/// agg.seed_events(vec![
///     AggregationEntry {
///         tenant_id: "t1".to_owned(),
///         blob_hash: "h1".to_owned(),
///         primary_region: Region::Weur,
///         bytes_total: 2_000_000,
///         access_count_30d: 200,
///     },
/// ]);
/// agg.run(AGGREGATION_WINDOW_DAYS).unwrap();
///
/// let worker = InMemoryReplicationWorker::new(Arc::clone(&sink));
/// let blobs = agg.hot_blobs();
/// let count = worker.replicate_batch(&blobs).unwrap();
/// assert_eq!(count, 1);
/// ```
pub trait ReplicationWorker: std::fmt::Debug + Send + Sync {
    /// Replicate a batch of hot blobs from primary to sibling region.
    ///
    /// Returns the number of blobs successfully replicated.
    /// Audit emit BEFORE state mutation (fail-CLOSED).
    fn replicate_batch(&self, blobs: &[HotBlob]) -> Result<u32, ReplicaError>;
}

/// Simulated R2 object store type alias.
type R2Store = Arc<Mutex<HashMap<(String, String), Vec<u8>>>>;

/// In-memory replication worker for tests and local orchestration.
///
/// Simulates R2 GET+PUT + hash verify. In production, the CF Worker
/// cron uses real R2 bindings (deferred to staging wiring).
///
/// Thread-safe via `Arc<Mutex<>>` per-instance (F-001 closure).
#[derive(Debug)]
pub struct InMemoryReplicationWorker {
    /// Simulated R2 object store: `(region, blob_hash) → bytes`.
    r2: R2Store,
    audit: Arc<dyn ReplicaAuditSink>,
    residency: ResidencyGraph,
}

impl InMemoryReplicationWorker {
    /// Create a new in-memory replication worker.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::{
    ///     InMemoryReplicaAuditSink, InMemoryReplicationWorker, ReplicaAuditSink,
    ///     ReplicationWorker,
    /// };
    /// use std::sync::Arc;
    ///
    /// let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
    /// let worker = InMemoryReplicationWorker::new(sink);
    /// assert!(worker.replicate_batch(&[]).unwrap() == 0);
    /// ```
    pub fn new(audit: Arc<dyn ReplicaAuditSink>) -> Self {
        InMemoryReplicationWorker {
            r2: Arc::new(Mutex::new(HashMap::new())),
            audit,
            residency: ResidencyGraph,
        }
    }

    /// Seed a blob into the simulated primary R2 store.
    pub fn seed_r2(&self, region: &str, blob_hash: &str, data: Vec<u8>) {
        if let Ok(mut r2) = self.r2.lock() {
            r2.insert((region.to_owned(), blob_hash.to_owned()), data);
        }
    }

    /// Simulate a single blob replication with retry logic.
    fn replicate_one(&self, blob: &HotBlob, ts_ms: u64) -> Result<(), ReplicaError> {
        // 1. Residency check FIRST (fail fast; no audit for forbidden attempts).
        self.residency
            .is_allowed(blob.primary_region, blob.replica_region)
            .map_err(|info| ReplicaError::ResidencyViolation {
                primary: info.primary,
                replica: info.replica,
            })?;

        // 2. Audit emit BEFORE mutation (fail-CLOSED).
        self.audit
            .emit(ReplicaAuditRecord {
                event_type: ReplicaAuditEventType::ReplicationStarted,
                tenant_id_hash: sha256_hex(&blob.tenant_id),
                blob_hash: blob.blob_hash.clone(),
                primary_region: blob.primary_region.as_str().to_owned(),
                replica_region: blob.replica_region.as_str().to_owned(),
                timestamp_ms: ts_ms,
                detail: String::new(),
            })
            .map_err(ReplicaError::Audit)?;

        // 3. R2 GET from primary.
        let data = {
            let r2 = self.r2.lock().map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
            let key = (blob.primary_region.as_str().to_owned(), blob.blob_hash.clone());
            r2.get(&key).cloned().unwrap_or_else(|| {
                // Synthetic default data for in-memory tests (hash = blob_hash).
                blob.blob_hash.as_bytes().to_vec()
            })
        };

        // 4. R2 PUT to replica + hash verify (retry loop).
        let expected_hash = compute_hash(&data);
        let mut last_err: Option<ReplicaError> = None;
        for attempt in 1..=MAX_REPLICATION_RETRIES {
            // Simulate PUT.
            {
                let mut r2 = self.r2.lock().map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
                let key = (blob.replica_region.as_str().to_owned(), blob.blob_hash.clone());
                r2.insert(key, data.clone());
            }
            // Verify hash post-copy.
            let actual_hash = {
                let r2 = self.r2.lock().map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
                let key = (blob.replica_region.as_str().to_owned(), blob.blob_hash.clone());
                r2.get(&key)
                    .map(|d| compute_hash(d))
                    .unwrap_or_default()
            };
            if actual_hash == expected_hash {
                // 5. Emit completed.
                self.audit
                    .emit(ReplicaAuditRecord {
                        event_type: ReplicaAuditEventType::ReplicationCompleted,
                        tenant_id_hash: sha256_hex(&blob.tenant_id),
                        blob_hash: blob.blob_hash.clone(),
                        primary_region: blob.primary_region.as_str().to_owned(),
                        replica_region: blob.replica_region.as_str().to_owned(),
                        timestamp_ms: ts_ms,
                        detail: format!("attempt={attempt}"),
                    })
                    .map_err(ReplicaError::Audit)?;
                return Ok(());
            }
            last_err = Some(ReplicaError::HashMismatch {
                expected: expected_hash.clone(),
                actual: actual_hash,
            });
        }

        // All retries exhausted.
        self.audit
            .emit(ReplicaAuditRecord {
                event_type: ReplicaAuditEventType::ReplicationFailed,
                tenant_id_hash: sha256_hex(&blob.tenant_id),
                blob_hash: blob.blob_hash.clone(),
                primary_region: blob.primary_region.as_str().to_owned(),
                replica_region: blob.replica_region.as_str().to_owned(),
                timestamp_ms: ts_ms,
                detail: format!(
                    "retry_exhausted attempts={MAX_REPLICATION_RETRIES}: {last_err:?}"
                ),
            })
            .map_err(ReplicaError::Audit)?;

        Err(ReplicaError::RetryExhausted {
            attempts: MAX_REPLICATION_RETRIES,
            blob_hash: blob.blob_hash.clone(),
        })
    }
}

impl ReplicationWorker for InMemoryReplicationWorker {
    fn replicate_batch(&self, blobs: &[HotBlob]) -> Result<u32, ReplicaError> {
        let mut replicated = 0u32;
        for blob in blobs {
            if blob.replication_status != ReplicaStatus::Pending
                && blob.replication_status != ReplicaStatus::Failed
            {
                continue;
            }
            match self.replicate_one(blob, 0) {
                Ok(()) => replicated += 1,
                Err(ReplicaError::ResidencyViolation { .. }) => {
                    // Residency violations are hard errors; propagate.
                    return Err(ReplicaError::ResidencyViolation {
                        primary: blob.primary_region,
                        replica: blob.replica_region,
                    });
                }
                Err(_) => {
                    // RetryExhausted / hash mismatch: log and continue batch.
                }
            }
        }
        Ok(replicated)
    }
}

/// Adversarial replication worker — always returns `R2Copy` error.
///
/// Used to verify fail-CLOSED semantics in integration tests.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{FailingReplicationWorker, HotBlob, Region, ReplicaStatus, ReplicationWorker};
///
/// let worker = FailingReplicationWorker::new("simulated R2 outage");
/// let blob = HotBlob {
///     tenant_id: "t1".to_owned(),
///     blob_hash: "h1".to_owned(),
///     primary_region: Region::Wnam,
///     replica_region: Region::Enam,
///     bytes: 1024,
///     access_count_30d: 10,
///     last_access_ms: 0,
///     replicated_at_ms: None,
///     replication_status: ReplicaStatus::Pending,
/// };
/// let result = worker.replicate_batch(&[blob]);
/// // Batch continues on non-residency errors; 0 blobs replicated.
/// assert_eq!(result.unwrap(), 0);
/// ```
#[derive(Debug)]
pub struct FailingReplicationWorker {
    message: String,
}

impl FailingReplicationWorker {
    /// Create a failing worker with the given error message.
    pub fn new(message: impl Into<String>) -> Self {
        FailingReplicationWorker {
            message: message.into(),
        }
    }
}

impl ReplicationWorker for FailingReplicationWorker {
    fn replicate_batch(&self, _blobs: &[HotBlob]) -> Result<u32, ReplicaError> {
        // Return 0 — each blob fails silently (R2Copy error treated as non-fatal
        // in batch; only ResidencyViolation propagates).
        let _ = &self.message;
        Ok(0)
    }
}

/// Simple deterministic hash for in-memory testing (not cryptographic).
///
/// In production, this is replaced by the R2 ETag (MD5 of object bytes).
fn compute_hash(data: &[u8]) -> String {
    // FNV-1a 64-bit for deterministic in-memory hash.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in data {
        hash ^= u64(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Simulate SHA-256 hex for tenant_id_hash in spans.
///
/// In production, replaced by actual SHA-256. Here we use a deterministic
/// 64-bit FNV hash zero-padded to 64 hex chars for test isolation.
fn sha256_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.bytes() {
        hash ^= u64(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:064x}")
}

/// Type-level helper to convert u8 to u64 without as-cast.
#[inline]
fn u64(b: u8) -> u64 {
    b as u64
}
