//! Dead-letter queue (DLQ) for failed webhook delivery attempts.
//!
//! # Invariant DLQ_BOUNDED
//!
//! The DLQ MUST NOT exceed [`crate::metrics::DLQ_CAP`] (1 000) events.
//! Attempting to push when at capacity returns
//! [`crate::types::DtWebhookError::DlqCapacityExceeded`].
//!
//! # Production note
//!
//! In production the DLQ is backed by CF KV (`dt:webhook:dlq:<event_id>`).
//! This module provides an in-memory implementation used in tests and staging.
//! The reconciliation job (`corelink-dt-reconcile`) drains the DLQ daily by
//! replaying events against the handler.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::metrics::DLQ_CAP;
use crate::types::{DtWebhookError, DtWebhookEvent};

/// An in-memory dead-letter queue of failed webhook events.
///
/// Thread-safe via `Arc<Mutex<>>` (F-001 closure pattern).
#[derive(Debug, Clone)]
pub struct InMemoryDlq {
    inner: Arc<Mutex<VecDeque<DlqEntry>>>,
}

/// A single entry in the DLQ.
#[derive(Debug, Clone)]
pub struct DlqEntry {
    /// The original event that failed delivery.
    pub event: DtWebhookEvent,
    /// Number of delivery attempts so far (max 3).
    pub attempt_count: u32,
    /// The last error message recorded.
    pub last_error: String,
}

impl Default for InMemoryDlq {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDlq {
    /// Create a new empty DLQ.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Push a failed event into the DLQ.
    ///
    /// # Errors
    ///
    /// Returns [`DtWebhookError::DlqCapacityExceeded`] if the queue already
    /// holds [`DLQ_CAP`] events.
    pub fn push(&self, entry: DlqEntry) -> Result<(), DtWebhookError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DtWebhookError::DtApiUnreachable("DLQ mutex poisoned".into()))?;

        if guard.len() >= DLQ_CAP {
            return Err(DtWebhookError::DlqCapacityExceeded { size: guard.len() });
        }
        guard.push_back(entry);
        Ok(())
    }

    /// Pop the oldest entry from the DLQ for replay.
    ///
    /// Returns `None` when the queue is empty.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the mutex is poisoned.
    pub fn pop(&self) -> Result<Option<DlqEntry>, DtWebhookError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DtWebhookError::DtApiUnreachable("DLQ mutex poisoned".into()))?;
        Ok(guard.pop_front())
    }

    /// Return the current number of events in the DLQ.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the mutex is poisoned.
    pub fn len(&self) -> Result<usize, DtWebhookError> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| DtWebhookError::DtApiUnreachable("DLQ mutex poisoned".into()))?;
        Ok(guard.len())
    }

    /// Returns `true` if the DLQ contains no events.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the mutex is poisoned.
    pub fn is_empty(&self) -> Result<bool, DtWebhookError> {
        Ok(self.len()? == 0)
    }

    /// Drain all entries into a `Vec` (used by the reconciliation job).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the mutex is poisoned.
    pub fn drain_all(&self) -> Result<Vec<DlqEntry>, DtWebhookError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DtWebhookError::DtApiUnreachable("DLQ mutex poisoned".into()))?;
        Ok(guard.drain(..).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ComponentMetadata, DtEventType, DtProjectUuid, DtWebhookEvent, VulnerabilityMetadata,
    };
    use std::time::SystemTime;

    fn dummy_event() -> DtWebhookEvent {
        DtWebhookEvent {
            event_type: DtEventType::NewVulnerability,
            project_uuid: DtProjectUuid::new("proj-uuid-001").expect("valid"),
            component: ComponentMetadata {
                purl: "pkg:cargo/ring@0.16.0".into(),
                name: "ring".into(),
                version: "0.16.0".into(),
                patched_locally: false,
                patched_locally_adr: None,
                adr_ratified_date: None,
            },
            vulnerability: VulnerabilityMetadata {
                cve_id: "CVE-2024-00001".into(),
                cvss_score: 9.5,
                severity_label: Some("CRITICAL".into()),
                description: None,
                sources: vec!["NVD".into()],
            },
            timestamp: SystemTime::now(),
        }
    }

    #[test]
    fn push_and_pop() {
        let dlq = InMemoryDlq::new();
        let entry = DlqEntry {
            event: dummy_event(),
            attempt_count: 1,
            last_error: "Slack 500".into(),
        };
        dlq.push(entry).expect("push");
        assert_eq!(dlq.len().expect("len"), 1);
        let popped = dlq.pop().expect("pop").expect("some");
        assert_eq!(popped.attempt_count, 1);
        assert!(dlq.is_empty().expect("empty"));
    }

    #[test]
    fn capacity_exceeded_returns_error() {
        let dlq = InMemoryDlq::new();
        for i in 0..DLQ_CAP {
            let mut e = dummy_event();
            e.vulnerability.cve_id = format!("CVE-2024-{i:05}");
            dlq.push(DlqEntry {
                event: e,
                attempt_count: 1,
                last_error: "test".into(),
            })
            .expect("push within cap");
        }
        let overflow = DlqEntry {
            event: dummy_event(),
            attempt_count: 1,
            last_error: "overflow".into(),
        };
        let result = dlq.push(overflow);
        assert!(matches!(
            result,
            Err(DtWebhookError::DlqCapacityExceeded { .. })
        ));
    }

    #[test]
    fn drain_all() {
        let dlq = InMemoryDlq::new();
        for _ in 0..3 {
            dlq.push(DlqEntry {
                event: dummy_event(),
                attempt_count: 1,
                last_error: "err".into(),
            })
            .expect("push");
        }
        let drained = dlq.drain_all().expect("drain");
        assert_eq!(drained.len(), 3);
        assert!(dlq.is_empty().expect("empty after drain"));
    }
}
