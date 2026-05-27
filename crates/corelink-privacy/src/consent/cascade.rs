//! Cascade unsubscribe sink — Cloudflare Queue fanout ≤24h SLA
//! (CTRL-PRIV-CONSENT-002).
//!
//! Production wiring (Cloudflare Queue producer, downstream consumers for
//! S-13 admin plane, S-09 Loki label dropper, S-10 Stripe metadata updater)
//! is deferred to WI-S11-008 PRR ship gate.
//!
//! # SLA
//!
//! Cascade MUST complete within 24h p99 (CTRL-PRIV-CONSENT-002). A SEV-2
//! alert fires if `cascade_status != 'completed'` after 24h. The cascade
//! is **asynchronous** (DD-005) to avoid blocking the revoke endpoint p95
//! latency target (≤5min CTRL-PRIV-CONSENT-002).

use std::sync::{Arc, Mutex};

use super::error::ConsentLedgerError;

/// Cascade task payload enqueued after consent revocation.
#[derive(Debug, Clone)]
pub struct CascadeTask {
    /// Revocation record ID.
    pub revocation_id: String,
    /// Tenant identifier.
    pub tenant_id: String,
    /// Subject SHA-256 hash (CTRL-PRIV-014 — no raw subject_id).
    pub subject_id_hash: String,
    /// Purpose string.
    pub purpose: String,
    /// ISO 8601 UTC timestamp when cascade was triggered.
    pub cascade_started_at: String,
    /// ISO 8601 UTC cascade SLA deadline (started_at + 24h).
    pub cascade_eta: String,
}

/// Trait for enqueuing cascade unsubscribe tasks.
///
/// Production wiring: Cloudflare Queue producer.
pub trait CascadeSink: Send + Sync {
    /// Enqueue a cascade task.
    ///
    /// The caller MUST call this AFTER the revocation record is stored.
    /// Failure here does NOT roll back the revocation — cascade status
    /// is tracked separately in `consent_revocation.cascade_status`.
    fn enqueue(&self, task: CascadeTask) -> Result<(), ConsentLedgerError>;
}

/// In-memory cascade sink that captures enqueued tasks.
///
/// Uses `Arc<Mutex<Vec<_>>>` F-001 closure (per-instance).
#[derive(Debug, Clone)]
pub struct InMemoryCascadeSink {
    tasks: Arc<Mutex<Vec<CascadeTask>>>,
}

impl InMemoryCascadeSink {
    /// Create a new empty sink.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return a snapshot of all enqueued tasks.
    pub fn captured(&self) -> Vec<CascadeTask> {
        self.tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Default for InMemoryCascadeSink {
    fn default() -> Self {
        Self::new()
    }
}

impl CascadeSink for InMemoryCascadeSink {
    fn enqueue(&self, task: CascadeTask) -> Result<(), ConsentLedgerError> {
        self.tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(task);
        Ok(())
    }
}
