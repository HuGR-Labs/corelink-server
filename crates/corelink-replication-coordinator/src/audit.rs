//! Coordinator audit sink — canonical 4-event taxonomy.
//!
//! Audit-emit-BEFORE-mutation fail-CLOSED per S-06 P0-2: every role
//! flip in [`crate::InMemoryReplicationCoordinator`] first emits the
//! corresponding audit record; if the audit emit fails, the role map
//! is NOT mutated and the caller observes
//! [`crate::CoordinatorError::Audit`].

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Canonical coordinator audit event types.
///
/// `#[non_exhaustive]` — adding a new event type does NOT break
/// downstream consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorAuditEventType {
    /// A replica was promoted to primary.
    /// Canonical CloudEvent type: `corelink.failover.region_promoted.v1`.
    RegionPromoted,
    /// A primary was demoted to hot-standby.
    /// Canonical CloudEvent type: `corelink.failover.region_demoted.v1`.
    RegionDemoted,
    /// A failback attempt was blocked (cool-down not elapsed,
    /// audit outbox dirty, etc.).
    /// Canonical CloudEvent type: `corelink.failover.failback_blocked.v1`.
    FailbackBlocked,
    /// A hot-standby region was failed back to primary after cool-down.
    /// Canonical CloudEvent type: `corelink.failover.failback_committed.v1`.
    FailbackCommitted,
}

impl CoordinatorAuditEventType {
    /// Canonical CloudEvent type string for the audit bus.
    #[must_use]
    pub fn cloudevent_type(self) -> &'static str {
        match self {
            CoordinatorAuditEventType::RegionPromoted => {
                "corelink.failover.region_promoted.v1"
            }
            CoordinatorAuditEventType::RegionDemoted => {
                "corelink.failover.region_demoted.v1"
            }
            CoordinatorAuditEventType::FailbackBlocked => {
                "corelink.failover.failback_blocked.v1"
            }
            CoordinatorAuditEventType::FailbackCommitted => {
                "corelink.failover.failback_committed.v1"
            }
        }
    }
}

/// A single coordinator audit record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorAuditRecord {
    /// Event type.
    pub event_type: CoordinatorAuditEventType,
    /// Lowercase canonical string of the **subject** region (the region
    /// being promoted / demoted / blocked).
    pub region: String,
    /// Lowercase canonical string of the previous primary (when applicable).
    pub previous_primary: String,
    /// Wall-clock timestamp of the audit emit, milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Free-form detail line (trigger summary, reason, etc.).
    pub detail: String,
}

/// Audit sink trait.
///
/// Implementations MUST be `Send + Sync`. A failing emit returns `Err`
/// and the coordinator does NOT mutate state (audit fail-CLOSED).
pub trait CoordinatorAuditSink: std::fmt::Debug + Send + Sync {
    /// Emit one audit record. Returns `Err(detail)` on failure; the
    /// detail string is propagated into
    /// [`crate::CoordinatorError::Audit`].
    fn emit(&self, record: CoordinatorAuditRecord) -> Result<(), String>;

    /// All records observed so far (test-only convenience; may be a
    /// snapshot in production).
    fn records(&self) -> Vec<CoordinatorAuditRecord>;
}

/// In-memory audit sink.
#[derive(Debug, Default)]
pub struct InMemoryCoordinatorAuditSink {
    inner: Arc<Mutex<Vec<CoordinatorAuditRecord>>>,
}

impl InMemoryCoordinatorAuditSink {
    /// Create an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<CoordinatorAuditRecord> {
        self.records()
    }
}

impl CoordinatorAuditSink for InMemoryCoordinatorAuditSink {
    fn emit(&self, record: CoordinatorAuditRecord) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| format!("audit sink lock poisoned: {e}"))?;
        guard.push(record);
        Ok(())
    }

    fn records(&self) -> Vec<CoordinatorAuditRecord> {
        self.inner
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

/// Adversarial audit sink that always fails — pins the fail-CLOSED
/// pattern (state MUST NOT mutate if audit emit fails).
#[derive(Debug, Default)]
pub struct FailingCoordinatorAuditSink;

impl FailingCoordinatorAuditSink {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        FailingCoordinatorAuditSink
    }
}

impl CoordinatorAuditSink for FailingCoordinatorAuditSink {
    fn emit(&self, _record: CoordinatorAuditRecord) -> Result<(), String> {
        Err("FailingCoordinatorAuditSink: forced failure for fail-CLOSED test".to_owned())
    }

    fn records(&self) -> Vec<CoordinatorAuditRecord> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudevent_types_canonical() {
        assert_eq!(
            CoordinatorAuditEventType::RegionPromoted.cloudevent_type(),
            "corelink.failover.region_promoted.v1"
        );
        assert_eq!(
            CoordinatorAuditEventType::RegionDemoted.cloudevent_type(),
            "corelink.failover.region_demoted.v1"
        );
        assert_eq!(
            CoordinatorAuditEventType::FailbackBlocked.cloudevent_type(),
            "corelink.failover.failback_blocked.v1"
        );
        assert_eq!(
            CoordinatorAuditEventType::FailbackCommitted.cloudevent_type(),
            "corelink.failover.failback_committed.v1"
        );
    }

    #[test]
    fn in_memory_sink_records_emit() -> Result<(), String> {
        let sink = InMemoryCoordinatorAuditSink::new();
        let rec = CoordinatorAuditRecord {
            event_type: CoordinatorAuditEventType::RegionPromoted,
            region: "wnam".to_owned(),
            previous_primary: "enam".to_owned(),
            timestamp_ms: 1,
            detail: "test".to_owned(),
        };
        sink.emit(rec.clone())?;
        assert_eq!(sink.snapshot(), vec![rec]);
        Ok(())
    }

    #[test]
    fn failing_sink_always_errors() {
        let sink = FailingCoordinatorAuditSink::new();
        let rec = CoordinatorAuditRecord {
            event_type: CoordinatorAuditEventType::RegionPromoted,
            region: "wnam".to_owned(),
            previous_primary: "enam".to_owned(),
            timestamp_ms: 1,
            detail: String::new(),
        };
        assert!(sink.emit(rec).is_err());
        assert!(sink.records().is_empty());
    }
}
