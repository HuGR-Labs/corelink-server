//! Broadcast store trait + implementations for `sub_processor_broadcast_log`.
//!
//! D1 table `sub_processor_broadcast_log` tracks per-recipient delivery
//! state with a UNIQUE constraint `(broadcast_id, tenant_id,
//! recipient_email_hash, notification_type)` per INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::SubProcessorBroadcastStoreError;
use crate::event::{BroadcastLogEntry, DeliveryStatus, NotificationType};

/// Key for UNIQUE constraint lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BroadcastUniqueKey {
    /// Broadcast ULID.
    pub broadcast_id: String,
    /// Tenant ID.
    pub tenant_id: String,
    /// SHA-256 of recipient email.
    pub recipient_email_hash: String,
    /// Notification type.
    pub notification_type: NotificationType,
}

/// Trait for broadcast log persistence (D1 `sub_processor_broadcast_log`).
pub trait BroadcastStore: std::fmt::Debug + Send + Sync {
    /// Insert a new broadcast log entry (fail if UNIQUE constraint violated).
    fn insert(
        &self,
        entry: BroadcastLogEntry,
    ) -> Result<(), SubProcessorBroadcastStoreError>;

    /// Update delivery status for an existing entry (webhook callback path).
    fn update_delivery_status(
        &self,
        log_id: &str,
        status: DeliveryStatus,
        delivered_at: Option<String>,
        error_class: Option<String>,
    ) -> Result<(), SubProcessorBroadcastStoreError>;

    /// Query delivery rate for a broadcast: returns (delivered_count, total_count).
    fn delivery_rate(
        &self,
        broadcast_id: &str,
    ) -> Result<(usize, usize), SubProcessorBroadcastStoreError>;

    /// List all entries for a tenant (sorted by enqueued_at DESC).
    fn list_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<BroadcastLogEntry>, SubProcessorBroadcastStoreError>;
}

/// In-memory broadcast store for testing.
///
/// Per-instance `Arc<Mutex<>>` — NEVER `static LazyLock<Mutex<>>` (F-001).
#[derive(Debug, Clone)]
pub struct InMemoryBroadcastStore {
    entries: Arc<Mutex<HashMap<String, BroadcastLogEntry>>>,
    unique_keys: Arc<Mutex<HashMap<BroadcastUniqueKey, String>>>,
}

impl InMemoryBroadcastStore {
    /// Construct a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            unique_keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return the total number of entries in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    /// Return true if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return all entries as a snapshot.
    #[must_use]
    pub fn all_entries(&self) -> Vec<BroadcastLogEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

impl Default for InMemoryBroadcastStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BroadcastStore for InMemoryBroadcastStore {
    fn insert(
        &self,
        entry: BroadcastLogEntry,
    ) -> Result<(), SubProcessorBroadcastStoreError> {
        let uk = BroadcastUniqueKey {
            broadcast_id: entry.broadcast_id.clone(),
            tenant_id: entry.tenant_id.clone(),
            recipient_email_hash: entry.recipient_email_hash.clone(),
            notification_type: entry.notification_type,
        };

        let mut keys = self.unique_keys.lock().unwrap_or_else(|p| p.into_inner());
        if keys.contains_key(&uk) {
            return Err(SubProcessorBroadcastStoreError::Unavailable(format!(
                "idempotency conflict: broadcast_id={} tenant_id={} recipient_hash={} notification_type={:?}",
                entry.broadcast_id, entry.tenant_id, entry.recipient_email_hash, entry.notification_type
            )));
        }
        keys.insert(uk, entry.log_id.clone());
        drop(keys);

        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(entry.log_id.clone(), entry);
        Ok(())
    }

    fn update_delivery_status(
        &self,
        log_id: &str,
        status: DeliveryStatus,
        delivered_at: Option<String>,
        error_class: Option<String>,
    ) -> Result<(), SubProcessorBroadcastStoreError> {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        match entries.get_mut(log_id) {
            Some(entry) => {
                entry.delivery_status = status;
                entry.delivered_at = delivered_at;
                entry.delivery_error_class = error_class;
                Ok(())
            }
            None => Err(SubProcessorBroadcastStoreError::Unavailable(format!(
                "broadcast log entry not found: log_id={log_id}"
            ))),
        }
    }

    fn delivery_rate(
        &self,
        broadcast_id: &str,
    ) -> Result<(usize, usize), SubProcessorBroadcastStoreError> {
        let entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let all: Vec<_> = entries
            .values()
            .filter(|e| e.broadcast_id == broadcast_id)
            .collect();
        let total = all.len();
        let delivered = all
            .iter()
            .filter(|e| e.delivery_status == DeliveryStatus::Delivered)
            .count();
        Ok((delivered, total))
    }

    fn list_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<BroadcastLogEntry>, SubProcessorBroadcastStoreError> {
        let entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let mut result: Vec<_> = entries
            .values()
            .filter(|e| e.tenant_id == tenant_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.enqueued_at.cmp(&a.enqueued_at));
        Ok(result)
    }
}

/// Always-failing broadcast store for fail-CLOSED envelope testing.
#[derive(Debug, Clone)]
pub struct FailingBroadcastStore;

impl BroadcastStore for FailingBroadcastStore {
    fn insert(
        &self,
        _entry: BroadcastLogEntry,
    ) -> Result<(), SubProcessorBroadcastStoreError> {
        Err(SubProcessorBroadcastStoreError::Unavailable(
            "FailingBroadcastStore always fails".into(),
        ))
    }

    fn update_delivery_status(
        &self,
        _log_id: &str,
        _status: DeliveryStatus,
        _delivered_at: Option<String>,
        _error_class: Option<String>,
    ) -> Result<(), SubProcessorBroadcastStoreError> {
        Err(SubProcessorBroadcastStoreError::Unavailable(
            "FailingBroadcastStore always fails".into(),
        ))
    }

    fn delivery_rate(
        &self,
        _broadcast_id: &str,
    ) -> Result<(usize, usize), SubProcessorBroadcastStoreError> {
        Err(SubProcessorBroadcastStoreError::Unavailable(
            "FailingBroadcastStore always fails".into(),
        ))
    }

    fn list_by_tenant(
        &self,
        _tenant_id: &str,
    ) -> Result<Vec<BroadcastLogEntry>, SubProcessorBroadcastStoreError> {
        Err(SubProcessorBroadcastStoreError::Unavailable(
            "FailingBroadcastStore always fails".into(),
        ))
    }
}
