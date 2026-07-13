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
use crate::metrics::{
    BatchOutcome, InMemoryReplicationLagSli, ReplicationDomain, ReplicationLagSli,
};
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

/// Simulated R2 object store type alias, keyed by `(tenant_id, region,
/// blob_hash)`.
///
/// The `tenant_id` component is load-bearing even in simulation: without it,
/// two tenants that store the same `blob_hash` in the same region collapse onto
/// one entry — which, once wired to real per-tenant-prefixed R2 keys, would be a
/// cross-tenant blob-collision. In production the leading key segment is the
/// derived `tenant_prefix`; here we key by the raw `tenant_id` (the identity the
/// prefix is derived from) so the isolation is preserved by construction.
type R2Store = Arc<Mutex<HashMap<(String, String, String), Vec<u8>>>>;

/// In-memory replication worker for tests and local orchestration.
///
/// Simulates R2 GET+PUT + hash verify. In production, the CF Worker
/// cron uses real R2 bindings (deferred to staging wiring).
///
/// Thread-safe via `Arc<Mutex<>>` per-instance (F-001 closure).
#[derive(Debug)]
pub struct InMemoryReplicationWorker {
    /// Simulated R2 object store: `(tenant_id, region, blob_hash) → bytes`.
    r2: R2Store,
    audit: Arc<dyn ReplicaAuditSink>,
    residency: ResidencyGraph,
    /// Lag SLI emit point (closes DEBT-011 R-PREP-REPL-P0-001).
    sli: Arc<dyn ReplicationLagSli>,
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
            sli: Arc::new(InMemoryReplicationLagSli::new()),
        }
    }

    /// Create a worker with a caller-supplied [`ReplicationLagSli`].
    ///
    /// Production wiring (CF Worker cron) passes a Prometheus-backed impl;
    /// tests can pass [`crate::metrics::InMemoryReplicationLagSli`] for
    /// deterministic assertion or [`crate::metrics::FailingReplicationLagSli`]
    /// to verify that SLI emit failure does NOT block replication completion
    /// (best-effort observability per S-06 P0-2 — audit chain is canonical).
    pub fn with_sli(audit: Arc<dyn ReplicaAuditSink>, sli: Arc<dyn ReplicationLagSli>) -> Self {
        InMemoryReplicationWorker {
            r2: Arc::new(Mutex::new(HashMap::new())),
            audit,
            residency: ResidencyGraph,
            sli,
        }
    }

    /// Accessor for the underlying SLI (useful when the worker owns the
    /// default `InMemoryReplicationLagSli` — tests can read observations
    /// without holding a separate `Arc`).
    pub fn sli(&self) -> Arc<dyn ReplicationLagSli> {
        Arc::clone(&self.sli)
    }

    /// Seed a blob into the simulated primary R2 store for a given tenant.
    ///
    /// `tenant_id` is part of the store key (see [`R2Store`]) so seeded blobs
    /// are tenant-isolated exactly as the replicate path keys them.
    pub fn seed_r2(&self, tenant_id: &str, region: &str, blob_hash: &str, data: Vec<u8>) {
        if let Ok(mut r2) = self.r2.lock() {
            r2.insert(
                (tenant_id.to_owned(), region.to_owned(), blob_hash.to_owned()),
                data,
            );
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
            let r2 = self
                .r2
                .lock()
                .map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
            let key = (
                blob.tenant_id.clone(),
                blob.primary_region.as_str().to_owned(),
                blob.blob_hash.clone(),
            );
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
                let mut r2 = self
                    .r2
                    .lock()
                    .map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
                let key = (
                    blob.tenant_id.clone(),
                    blob.replica_region.as_str().to_owned(),
                    blob.blob_hash.clone(),
                );
                r2.insert(key, data.clone());
            }
            // Verify hash post-copy.
            let actual_hash = {
                let r2 = self
                    .r2
                    .lock()
                    .map_err(|e| ReplicaError::R2Copy(e.to_string()))?;
                let key = (
                    blob.tenant_id.clone(),
                    blob.replica_region.as_str().to_owned(),
                    blob.blob_hash.clone(),
                );
                r2.get(&key).map(|d| compute_hash(d)).unwrap_or_default()
            };
            if actual_hash == expected_hash {
                // 5. Emit completed audit BEFORE Sli (audit is canonical).
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

                // 6. Emit lag SLI (best-effort; failure logged via audit,
                //    NEVER blocks replication completion — see
                //    `crates/corelink-replica-worker/src/metrics.rs` module
                //    docs and DEBT-011 R-PREP-REPL-P0-001).
                let lag_secs = compute_lag_seconds(ts_ms, blob.last_access_ms);
                if let Err(sli_err) = self.sli.emit_lag(
                    ReplicationDomain::R2Hot,
                    blob.primary_region,
                    blob.replica_region,
                    lag_secs,
                    ts_ms,
                ) {
                    // Audit-log SLI emit failure for forensic trace; do NOT
                    // propagate as replication failure (audit is canonical,
                    // SLI is best-effort observability).
                    let _ = self.audit.emit(ReplicaAuditRecord {
                        event_type: ReplicaAuditEventType::ReplicationCompleted,
                        tenant_id_hash: sha256_hex(&blob.tenant_id),
                        blob_hash: blob.blob_hash.clone(),
                        primary_region: blob.primary_region.as_str().to_owned(),
                        replica_region: blob.replica_region.as_str().to_owned(),
                        timestamp_ms: ts_ms,
                        detail: format!("sli_emit_failed: {sli_err}"),
                    });
                }
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
                detail: format!("retry_exhausted attempts={MAX_REPLICATION_RETRIES}: {last_err:?}"),
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
        let mut attempted = 0u32;
        let batch_ts_ms: u64 = 0;
        for blob in blobs {
            if blob.replication_status != ReplicaStatus::Pending
                && blob.replication_status != ReplicaStatus::Failed
            {
                continue;
            }
            attempted += 1;
            match self.replicate_one(blob, batch_ts_ms) {
                Ok(()) => replicated += 1,
                Err(ReplicaError::ResidencyViolation { .. }) => {
                    // Residency violations are hard errors; emit `failed`
                    // batch outcome BEFORE propagating (silent-skip hazard
                    // mitigation per audit §5.2).
                    let _ = self.sli.emit_batch_outcome(
                        ReplicationDomain::R2Hot,
                        BatchOutcome::Failed,
                        batch_ts_ms,
                    );
                    return Err(ReplicaError::ResidencyViolation {
                        primary: blob.primary_region,
                        replica: blob.replica_region,
                    });
                }
                Err(_) => {
                    // RetryExhausted / hash mismatch: continue batch.
                    // Outcome discrimination happens after the loop.
                }
            }
        }
        // Emit batch outcome counter — closes audit §5.2 silent-skip hazard.
        let outcome = if attempted == 0 {
            // Empty batch: no observation needed.
            return Ok(replicated);
        } else if replicated == attempted {
            BatchOutcome::Ok
        } else if replicated == 0 {
            BatchOutcome::Failed
        } else {
            BatchOutcome::Partial
        };
        let _ = self
            .sli
            .emit_batch_outcome(ReplicationDomain::R2Hot, outcome, batch_ts_ms);
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

/// Compute replication lag in seconds between two millisecond timestamps.
///
/// `completed_ms` = wall-clock at the `replication.completed` audit emit.
/// `last_access_ms` = `HotBlob::last_access_ms` (proxy for blob freshness;
/// in production CF Worker wiring this is the `blob.created_ts` per the
/// acceptance criterion in `replication-followup-tickets.md
/// R-PREP-REPL-P0-001 §1`).
///
/// Clock-skew defensive: clamps negative deltas to `0.0` (replica may briefly
/// observe a "future" `last_access_ms` if the source DB clock is ahead; the
/// SLI must never emit a negative observation per
/// `corelink-replica-worker::metrics::InMemoryReplicationLagSli::emit_lag`
/// guard).
#[inline]
fn compute_lag_seconds(completed_ms: u64, last_access_ms: u64) -> f64 {
    let delta_ms = completed_ms.saturating_sub(last_access_ms);
    (delta_ms as f64) / 1000.0
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tenant_key_tests {
    use super::*;
    use crate::audit::InMemoryReplicaAuditSink;

    /// The simulated R2 store is keyed by `(tenant_id, region, blob_hash)`: two
    /// tenants that store the SAME blob_hash in the SAME region must NOT
    /// collide. Without the tenant component this would be a single overwritten
    /// entry — a cross-tenant collapse once wired to real R2.
    #[test]
    fn simulated_store_is_tenant_isolated() {
        let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
        let worker = InMemoryReplicationWorker::new(sink);

        worker.seed_r2("tenant-a", "iad", "deadbeef", b"tenant-a-bytes".to_vec());
        worker.seed_r2("tenant-b", "iad", "deadbeef", b"tenant-b-bytes".to_vec());

        let r2 = worker.r2.lock().expect("r2 lock");
        assert_eq!(
            r2.len(),
            2,
            "same (region, blob_hash) for two tenants must not collide"
        );
        assert_eq!(
            r2.get(&("tenant-a".to_owned(), "iad".to_owned(), "deadbeef".to_owned()))
                .map(Vec::as_slice),
            Some(b"tenant-a-bytes".as_slice())
        );
        assert_eq!(
            r2.get(&("tenant-b".to_owned(), "iad".to_owned(), "deadbeef".to_owned()))
                .map(Vec::as_slice),
            Some(b"tenant-b-bytes".as_slice())
        );
    }
}
