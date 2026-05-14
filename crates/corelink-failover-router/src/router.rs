//! Failover router — multi-signal detection + read routing + write blocking.
//!
//! # Multi-signal detection
//!
//! Failover is triggered when ALL 3 signals are active simultaneously:
//! 1. 5xx rate > 1%
//! 2. Latency p99 > 300ms
//! 3. ≥ 3 consecutive failures
//!
//! All sustained within 5s window (prevents false-positive from transient
//! slowness misdetected as outage).
//!
//! # Read-only mode
//!
//! Writes are BLOCKED during failover (returns `WriteBlockedDuringFailover`).
//! Stale read post-write inconsistency is eliminated.
//! Customer notified after 30 min sustained outage (production wiring deferred).
//!
//! # Acyclic graph
//!
//! Uses `ResidencyGraph` (WNAM↔ENAM, WEUR↔SAM). No cycles possible.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{FailoverAuditEventType, FailoverAuditRecord, FailoverAuditSink};
use crate::error::FailoverError;
use crate::health::{RegionHealth, RegionHealthSnapshot};
use crate::probe::HealthProbe;
use crate::{Region, ResidencyGraph};

/// Whether a read request should use primary or replica region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    /// Read from primary region (nominal path).
    Primary,
    /// Read from sibling replica region (failover path).
    Replica,
}

/// Whether a write request is allowed in current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Writes allowed (primary region healthy).
    Allowed,
    /// Writes blocked; region in failover read-only mode.
    Blocked,
}

/// Failover routing decision for a request.
#[derive(Debug, Clone)]
pub struct FailoverDecision {
    /// The primary region for this tenant.
    pub primary: Region,
    /// The effective region to use for reads.
    pub read_region: Region,
    /// Read mode selected.
    pub read_mode: ReadMode,
    /// Write mode (blocked during failover).
    pub write_mode: WriteMode,
    /// Whether failover was engaged.
    pub failover_active: bool,
    /// Overhead in milliseconds (simulated).
    pub overhead_ms: u64,
}

/// Failover router trait.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::{
///     FailoverAuditSink, FailoverRouter, HealthProbe, InMemoryFailoverAuditSink,
///     InMemoryFailoverRouter, InMemoryHealthProbe, ReadMode, Region,
/// };
/// use std::sync::Arc;
///
/// let probe_inner = Arc::new(InMemoryHealthProbe::new());
/// let probe: Arc<dyn HealthProbe> = Arc::clone(&probe_inner) as Arc<dyn HealthProbe>;
/// let sink: Arc<dyn FailoverAuditSink> = Arc::new(InMemoryFailoverAuditSink::new());
/// let router = InMemoryFailoverRouter::new(probe, sink);
///
/// // Healthy — reads from primary.
/// let decision = router.route_read("tenant-01", Region::Weur, 0).unwrap();
/// assert_eq!(decision.read_mode, ReadMode::Primary);
/// assert_eq!(decision.read_region, Region::Weur);
///
/// // Inject outage.
/// probe_inner.inject_degraded(Region::Weur);
/// let decision = router.route_read("tenant-01", Region::Weur, 0).unwrap();
/// assert_eq!(decision.read_mode, ReadMode::Replica);
/// assert_eq!(decision.read_region, Region::Sam); // WEUR sibling
/// assert!(decision.failover_active);
/// ```
pub trait FailoverRouter: std::fmt::Debug + Send + Sync {
    /// Determine the routing decision for a read request.
    ///
    /// If primary region is degraded, routes to sibling replica.
    /// Audit emit `failover.detected` BEFORE routing state mutation (fail-CLOSED).
    fn route_read(
        &self,
        tenant_id: &str,
        primary: Region,
        timestamp_ms: u64,
    ) -> Result<FailoverDecision, FailoverError>;

    /// Check write mode for a tenant's region.
    ///
    /// Returns `WriteMode::Blocked` if region is in failover read-only mode.
    fn write_mode(&self, primary: Region) -> WriteMode;

    /// Get the current health snapshot for a region.
    fn region_health(&self, region: Region, timestamp_ms: u64) -> RegionHealthSnapshot;
}

/// In-memory failover router for tests and local orchestration.
///
/// Thread-safe via `Arc<Mutex<>>` per-instance (F-001 closure).
#[derive(Debug)]
pub struct InMemoryFailoverRouter {
    probe: Arc<dyn HealthProbe>,
    audit: Arc<dyn FailoverAuditSink>,
    residency: ResidencyGraph,
    /// Cached health state per region: region_str → health.
    health_cache: Arc<Mutex<HashMap<String, RegionHealthSnapshot>>>,
}

impl InMemoryFailoverRouter {
    /// Create a new in-memory failover router.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::{InMemoryFailoverRouter, InMemoryHealthProbe, InMemoryFailoverAuditSink};
    /// use std::sync::Arc;
    ///
    /// let probe = Arc::new(InMemoryHealthProbe::new());
    /// let sink = Arc::new(InMemoryFailoverAuditSink::new());
    /// let _router = InMemoryFailoverRouter::new(probe, sink);
    /// ```
    pub fn new(probe: Arc<dyn HealthProbe>, audit: Arc<dyn FailoverAuditSink>) -> Self {
        InMemoryFailoverRouter {
            probe,
            audit,
            residency: ResidencyGraph,
            health_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get_health(&self, region: Region, timestamp_ms: u64) -> RegionHealthSnapshot {
        self.probe
            .probe(region, timestamp_ms)
            .unwrap_or_else(|_| {
                // Probe failure = treat as degraded (fail-CLOSED / conservative).
                RegionHealthSnapshot::evaluate(region, 100.0, 10_000, 100, timestamp_ms)
            })
    }
}

impl FailoverRouter for InMemoryFailoverRouter {
    fn route_read(
        &self,
        tenant_id: &str,
        primary: Region,
        timestamp_ms: u64,
    ) -> Result<FailoverDecision, FailoverError> {
        let snap = self.get_health(primary, timestamp_ms);

        // Cache current health.
        if let Ok(mut cache) = self.health_cache.lock() {
            cache.insert(primary.as_str().to_owned(), snap.clone());
        }

        if snap.health == RegionHealth::Healthy {
            return Ok(FailoverDecision {
                primary,
                read_region: primary,
                read_mode: ReadMode::Primary,
                write_mode: WriteMode::Allowed,
                failover_active: false,
                overhead_ms: 0,
            });
        }

        // Region degraded — route to sibling.
        let sibling = self
            .residency
            .sibling(primary)
            .ok_or(FailoverError::NoReplicaAvailable { region: primary })?;

        // Emit failover.detected BEFORE routing (fail-CLOSED).
        let trigger_str = snap
            .active_triggers
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(",");
        self.audit
            .emit(FailoverAuditRecord {
                event_type: FailoverAuditEventType::FailoverDetected,
                tenant_id_hash: sha256_hex(tenant_id),
                blob_hash: String::new(),
                primary_region: primary.as_str().to_owned(),
                replica_region: sibling.as_str().to_owned(),
                timestamp_ms,
                detail: format!(
                    "triggers=[{trigger_str}] health={:?}",
                    snap.health
                ),
            })
            .map_err(FailoverError::Audit)?;

        Ok(FailoverDecision {
            primary,
            read_region: sibling,
            read_mode: ReadMode::Replica,
            write_mode: WriteMode::Blocked,
            failover_active: true,
            overhead_ms: 5, // simulated 5ms overhead (≤ 50ms p99 SLO)
        })
    }

    fn write_mode(&self, primary: Region) -> WriteMode {
        let snap = self.get_health(primary, 0);
        if snap.health.requires_failover() {
            WriteMode::Blocked
        } else {
            WriteMode::Allowed
        }
    }

    fn region_health(&self, region: Region, timestamp_ms: u64) -> RegionHealthSnapshot {
        self.get_health(region, timestamp_ms)
    }
}

/// Simulate SHA-256 hex for tenant_id_hash in spans (FNV-1a 64-bit, zero-padded).
fn sha256_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:064x}")
}
