//! Offline batch aggregation job — top 1% blobs per tenant.
//!
//! **CRITICAL**: This is OFFLINE (cron daily 02:00 UTC), NOT a live
//! Prometheus metric labeled by `tenant_id`. Live metric labeled by
//! `tenant_id` is FORBIDDEN per INV-OBS-CARDINALITY-BUDGET S-09.
//!
//! Strategy:
//! 1. Query S-09 audit log R2 events `corelink.cas.get.ok` for last
//!    30d window (payload: `{tenant_id, blob_hash, bytes, ts}`).
//! 2. `GROUP BY tenant_id, blob_hash; SUM(bytes) AS bytes_total;
//!    COUNT(*) AS access_count_30d`.
//! 3. Compute top 1% per tenant by `bytes_total`.
//! 4. INSERT/UPDATE D1 `hot_blobs` table; DELETE rows not in current top 1%.
//!
//! Audit fail-CLOSED: state (D1 hot_blobs) is NEVER mutated if audit emit fails.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{ReplicaAuditEventType, ReplicaAuditRecord, ReplicaAuditSink};
use crate::error::ReplicaError;
use crate::hot_blob::{AggregationEntry, HotBlob, ReplicaStatus};
use crate::region::ResidencyGraph;

/// Aggregation window in days (last 30d of audit log events).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::AGGREGATION_WINDOW_DAYS;
///
/// assert_eq!(AGGREGATION_WINDOW_DAYS, 30);
/// ```
pub const AGGREGATION_WINDOW_DAYS: u32 = 30;

/// Top-N% threshold as a fraction (1% = 0.01).
///
/// Configurable via admin API (waiver); GA default = 1%.
/// Top 1% covers ~80% of read traffic (Pareto distribution).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::TOP_1_PCT_THRESHOLD;
///
/// assert!((TOP_1_PCT_THRESHOLD - 0.01_f64).abs() < f64::EPSILON);
/// ```
pub const TOP_1_PCT_THRESHOLD: f64 = 0.01;

/// Result of a single offline aggregation run.
#[derive(Debug, Clone)]
pub struct AggregationResult {
    /// Total audit events processed.
    pub events_processed: u64,
    /// Total unique (tenant_id, blob_hash) pairs found.
    pub unique_blobs: u64,
    /// Number of hot blob rows inserted/updated in D1.
    pub hot_blobs_upserted: u32,
    /// Number of hot blob rows evicted (no longer in top 1%).
    pub hot_blobs_evicted: u32,
}

/// Offline aggregator trait.
///
/// Implementations query the S-09 audit log R2 and compute the top 1%
/// blobs per tenant for replication. NEVER labeled by `tenant_id` in
/// live metrics.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{
///     AggregationEntry, InMemoryOfflineAggregator, OfflineAggregator, Region,
///     InMemoryReplicaAuditSink, ReplicaAuditSink, AGGREGATION_WINDOW_DAYS,
/// };
/// use std::sync::Arc;
///
/// let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
/// let agg = InMemoryOfflineAggregator::new(Arc::clone(&sink));
///
/// // Seed with synthetic audit events
/// agg.seed_events(vec![
///     AggregationEntry {
///         tenant_id: "t1".to_owned(),
///         blob_hash: "h1".to_owned(),
///         primary_region: Region::Wnam,
///         bytes_total: 1_000_000,
///         access_count_30d: 100,
///     },
/// ]);
///
/// let result = agg.run(AGGREGATION_WINDOW_DAYS).unwrap();
/// assert!(result.events_processed > 0);
/// ```
pub trait OfflineAggregator: std::fmt::Debug + Send + Sync {
    /// Run the offline aggregation job over the given window.
    ///
    /// Returns the number of hot blob rows upserted in D1.
    /// Audit emit `aggregation.started` BEFORE mutating D1 (fail-CLOSED).
    fn run(&self, window_days: u32) -> Result<AggregationResult, ReplicaError>;

    /// Retrieve current hot blobs table snapshot.
    fn hot_blobs(&self) -> Vec<HotBlob>;
}

/// In-memory offline aggregator for tests and local orchestration.
///
/// Thread-safe via `Arc<Mutex<>>` per-instance (F-001 closure; no `static`).
/// Audit emit-BEFORE-mutation fail-CLOSED enforced on every write path.
#[derive(Debug)]
pub struct InMemoryOfflineAggregator {
    state: Arc<Mutex<AggregatorState>>,
    audit: Arc<dyn ReplicaAuditSink>,
    residency: ResidencyGraph,
}

#[derive(Debug, Default)]
struct AggregatorState {
    events: Vec<AggregationEntry>,
    hot_blobs: Vec<HotBlob>,
    run_count: u64,
    timestamp_ms: u64,
}

impl InMemoryOfflineAggregator {
    /// Create a new in-memory aggregator with the given audit sink.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::{
    ///     InMemoryOfflineAggregator, InMemoryReplicaAuditSink, OfflineAggregator,
    ///     ReplicaAuditSink,
    /// };
    /// use std::sync::Arc;
    ///
    /// let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
    /// let agg = InMemoryOfflineAggregator::new(sink);
    /// assert_eq!(agg.hot_blobs().len(), 0);
    /// ```
    pub fn new(audit: Arc<dyn ReplicaAuditSink>) -> Self {
        InMemoryOfflineAggregator {
            state: Arc::new(Mutex::new(AggregatorState::default())),
            audit,
            residency: ResidencyGraph,
        }
    }

    /// Seed synthetic audit log events (for tests).
    pub fn seed_events(&self, events: Vec<AggregationEntry>) {
        if let Ok(mut s) = self.state.lock() {
            s.events.extend(events);
        }
    }

    /// Set simulated current timestamp (ms since epoch) for test determinism.
    pub fn set_timestamp_ms(&self, ts: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.timestamp_ms = ts;
        }
    }
}

impl OfflineAggregator for InMemoryOfflineAggregator {
    fn run(&self, _window_days: u32) -> Result<AggregationResult, ReplicaError> {
        // Step 1: Emit aggregation.started BEFORE mutating state (fail-CLOSED).
        let ts_ms = self
            .state
            .lock()
            .map_err(|e| ReplicaError::Storage(e.to_string()))?
            .timestamp_ms;

        self.audit
            .emit(ReplicaAuditRecord {
                event_type: ReplicaAuditEventType::AggregationStarted,
                tenant_id_hash: String::new(),
                blob_hash: String::new(),
                primary_region: String::new(),
                replica_region: String::new(),
                timestamp_ms: ts_ms,
                detail: format!("window_days={_window_days}"),
            })
            .map_err(ReplicaError::Audit)?;

        // Step 2: Compute aggregation (GROUP BY tenant_id, blob_hash; SUM(bytes)).
        let events = {
            self.state
                .lock()
                .map_err(|e| ReplicaError::Storage(e.to_string()))?
                .events
                .clone()
        };

        let events_processed = events.len() as u64;

        // Aggregate: sum bytes per (tenant_id, blob_hash).
        let mut agg_map: HashMap<(String, String), AggregationEntry> = HashMap::new();
        for ev in &events {
            let key = (ev.tenant_id.clone(), ev.blob_hash.clone());
            let entry = agg_map.entry(key).or_insert_with(|| AggregationEntry {
                tenant_id: ev.tenant_id.clone(),
                blob_hash: ev.blob_hash.clone(),
                primary_region: ev.primary_region,
                bytes_total: 0,
                access_count_30d: 0,
            });
            entry.bytes_total = entry.bytes_total.saturating_add(ev.bytes_total);
            entry.access_count_30d = entry.access_count_30d.saturating_add(ev.access_count_30d);
        }

        let unique_blobs = agg_map.len() as u64;

        // Group by tenant and find top 1%.
        let mut by_tenant: HashMap<String, Vec<AggregationEntry>> = HashMap::new();
        for entry in agg_map.into_values() {
            by_tenant
                .entry(entry.tenant_id.clone())
                .or_default()
                .push(entry);
        }

        let mut new_hot_blobs: Vec<HotBlob> = Vec::new();
        for (_, mut blobs) in by_tenant {
            // Sort by bytes_total descending.
            blobs.sort_by(|a, b| b.bytes_total.cmp(&a.bytes_total));
            // Top 1% (minimum 1 blob).
            let top_n = ((blobs.len() as f64 * TOP_1_PCT_THRESHOLD).ceil() as usize).max(1);
            let top_n = top_n.min(blobs.len());
            for entry in blobs.get(..top_n).unwrap_or(&blobs) {
                let replica_region =
                    self.residency
                        .sibling(entry.primary_region)
                        .ok_or_else(|| {
                            ReplicaError::Storage(format!(
                                "no sibling configured for {:?}",
                                entry.primary_region
                            ))
                        })?;
                new_hot_blobs.push(HotBlob {
                    tenant_id: entry.tenant_id.clone(),
                    blob_hash: entry.blob_hash.clone(),
                    primary_region: entry.primary_region,
                    replica_region,
                    bytes: entry.bytes_total,
                    access_count_30d: entry.access_count_30d,
                    last_access_ms: ts_ms,
                    replicated_at_ms: None,
                    replication_status: ReplicaStatus::Pending,
                });
            }
        }

        let hot_blobs_upserted = new_hot_blobs.len() as u32;

        // Step 3: Mutate state AFTER audit emit succeeded.
        {
            let mut s = self
                .state
                .lock()
                .map_err(|e| ReplicaError::Storage(e.to_string()))?;

            // Count evictions (blobs in current hot set not in new set).
            let new_keys: std::collections::HashSet<(String, String)> = new_hot_blobs
                .iter()
                .map(|b| (b.tenant_id.clone(), b.blob_hash.clone()))
                .collect();
            let hot_blobs_evicted = s
                .hot_blobs
                .iter()
                .filter(|b| {
                    !new_keys.contains(&(b.tenant_id.clone(), b.blob_hash.clone()))
                        && b.replication_status != ReplicaStatus::Evicted
                })
                .count() as u32;

            s.hot_blobs = new_hot_blobs;
            s.run_count += 1;

            // Emit completed.
            self.audit
                .emit(ReplicaAuditRecord {
                    event_type: ReplicaAuditEventType::AggregationCompleted,
                    tenant_id_hash: String::new(),
                    blob_hash: String::new(),
                    primary_region: String::new(),
                    replica_region: String::new(),
                    timestamp_ms: ts_ms,
                    detail: format!(
                        "upserted={hot_blobs_upserted} evicted={hot_blobs_evicted}"
                    ),
                })
                .map_err(ReplicaError::Audit)?;

            Ok(AggregationResult {
                events_processed,
                unique_blobs,
                hot_blobs_upserted,
                hot_blobs_evicted,
            })
        }
    }

    fn hot_blobs(&self) -> Vec<HotBlob> {
        self.state
            .lock()
            .map(|s| s.hot_blobs.clone())
            .unwrap_or_default()
    }
}
