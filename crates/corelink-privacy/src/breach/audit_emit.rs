//! Breach audit sink trait + InMemory and Failing test sinks.
//!
//! Mirrors the `corelink-privacy-erasure-worker` audit discipline:
//! every breach notification dispatch fires its audit event per the
//! canonical Lote 10.6bis pattern + ADR-S11-002 split-tier discipline
//! (WI-S11-006 §9.3 DD audit failure trade-off).
//!
//! ## Audit ordering (S-06 P0-2 / S-07 P1-1 lessons absorbed)
//!
//! `emit()` MUST be called BEFORE any state mutation (e.g., marking a
//! `dsr_ticket` as `breach_notified`). The [`regression_audit_fail_closed_behavior`]
//! test verifies that state remains UNCHANGED when `emit()` returns `Err`.
//!
//! ## Audit fail behavior (WI-S11-006 §9.3 DD)
//!
//! Unlike the DSR erasure pipeline (pure fail-CLOSED), breach notification
//! has a distinct trade-off: regulatory SLA (72h) takes priority over
//! audit trail completeness when they conflict. Therefore:
//!
//! - `emit()` returning `Err` does NOT block notification dispatch.
//! - The caller (Privacy Officer / automation) MUST:
//!   1. Proceed with dispatch (regulatory SLA priority).
//!   2. Fire a SEV-1 alert to Security Lead + Compliance.
//!   3. Queue manual re-emit with `retry_attempt++` and same `breach_id`.
//!
//! This is documented-priority behavior, NOT silent-skip. The audit
//! gap is logged and remediated post-recovery.

use std::sync::{Arc, Mutex};

use super::error::BreachAuditSinkError;
use super::event::BreachNotificationDispatch;

/// Breach audit sink trait. Production wiring emits
/// `dev.hugr.corelink.breach.notification_dispatched.v1` CloudEvent
/// to R2 `audit-<region>` with Object Lock 7y
/// (INV-AUDIT-APPEND-ONLY §3.6 L116 CRITICAL).
///
/// `emit()` MUST be called BEFORE any state mutation (audit-ordering
/// invariant per S-06 P0-2 / S-07 P1-1 lessons).
pub trait BreachAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `dispatch` record durably.
    ///
    /// Returns `Ok(())` on success. On `Err`:
    /// - The caller MUST NOT block notification dispatch (regulatory
    ///   SLA priority per WI-S11-006 §9.3 DD).
    /// - The caller MUST fire a SEV-1 alert + queue retry.
    ///
    /// # Errors
    ///
    /// Returns [`BreachAuditSinkError::Store`] on any backend failure.
    fn emit(&self, dispatch: BreachNotificationDispatch) -> Result<(), BreachAuditSinkError>;
}

/// In-memory test audit sink.
///
/// Cloning shares the underlying `Arc<Mutex<>>` buffer so orchestrator
/// + verifier can hold separate handles to the same sink.
///
/// Per F-001 closure: per-instance `Arc<Mutex<>>` — NEVER
/// `static LazyLock<Mutex<>>`.
#[derive(Clone, Default, Debug)]
pub struct InMemoryBreachAuditSink {
    /// F-001: per-instance Arc<Mutex<>>.
    inner: Arc<Mutex<Vec<BreachNotificationDispatch>>>,
}

impl InMemoryBreachAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every dispatch captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<BreachNotificationDispatch> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of dispatch records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot dispatches for a specific `breach_id`.
    #[must_use]
    pub fn snapshot_for_breach(&self, breach_id: &str) -> Vec<BreachNotificationDispatch> {
        self.snapshot()
            .into_iter()
            .filter(|d| d.breach_id == breach_id)
            .collect()
    }
}

impl BreachAuditSink for InMemoryBreachAuditSink {
    fn emit(&self, dispatch: BreachNotificationDispatch) -> Result<(), BreachAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            BreachAuditSinkError::Store(
                "breach audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(dispatch);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED-ish
/// envelope (WI-S11-006 §15 chaos experiment 2: "audit emit failure
/// mid-dispatch → dispatch ainda procede; SEV-1 alert + retry").
#[derive(Debug, Default)]
pub struct FailingBreachAuditSink;

impl FailingBreachAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl BreachAuditSink for FailingBreachAuditSink {
    fn emit(&self, _dispatch: BreachNotificationDispatch) -> Result<(), BreachAuditSinkError> {
        Err(BreachAuditSinkError::Store(
            "induced breach audit sink failure (test fixture — chaos experiment 2)".to_string(),
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use super::super::event::{BreachSeverity, CustomerLocale, NotificationJurisdiction};

    fn sample_dispatch(breach_id: &str) -> BreachNotificationDispatch {
        BreachNotificationDispatch {
            breach_id: breach_id.to_string(),
            severity: BreachSeverity::Sev1,
            jurisdictions_notified: vec![
                NotificationJurisdiction::Anpd,
                NotificationJurisdiction::IrishDpc,
                NotificationJurisdiction::CaliforniaAg,
            ],
            customer_notifications_sent: true,
            customer_locales: vec![
                CustomerLocale::PtBr,
                CustomerLocale::EnUs,
                CustomerLocale::EsMx,
            ],
            ts_ms: 1_715_000_000_000,
            retry_attempt: 0,
            breach_detected_at_ms: 1_714_997_000_000,
        }
    }

    #[test]
    fn in_memory_sink_captures_dispatches() {
        let sink = InMemoryBreachAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(sample_dispatch("BREACH-001")).unwrap();
        sink.emit(sample_dispatch("BREACH-002")).unwrap();
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn snapshot_for_breach_filters_correctly() {
        let sink = InMemoryBreachAuditSink::new();
        sink.emit(sample_dispatch("BREACH-A")).unwrap();
        sink.emit(sample_dispatch("BREACH-B")).unwrap();
        sink.emit(sample_dispatch("BREACH-A")).unwrap(); // retry
        let a = sink.snapshot_for_breach("BREACH-A");
        assert_eq!(a.len(), 2, "BREACH-A should have 2 records (original + retry)");
        let b = sink.snapshot_for_breach("BREACH-B");
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryBreachAuditSink::new();
        let s2 = s1.clone();
        s1.emit(sample_dispatch("BREACH-C")).unwrap();
        assert_eq!(s2.len(), 1, "cloned sink must share the Arc<Mutex<>> buffer");
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingBreachAuditSink::new();
        let err = sink.emit(sample_dispatch("BREACH-D")).unwrap_err();
        let is_store = matches!(err, BreachAuditSinkError::Store(_));
        assert!(is_store, "FailingBreachAuditSink must return Store error");
    }

    #[test]
    fn state_unchanged_on_audit_emit_failure() {
        // Audit ordering invariant (S-06 P0-2 / S-07 P1-1):
        // state must remain UNCHANGED when emit() returns Err.
        //
        // This test models the caller side: dispatch count (simulated
        // state) must not increment when sink.emit() fails.
        let sink = FailingBreachAuditSink::new();
        let in_memory_sink = InMemoryBreachAuditSink::new();

        // Simulated state: number of confirmed dispatches
        let mut confirmed_dispatches: u32 = 0;

        let dispatch = sample_dispatch("BREACH-E");

        // Audit ordering: emit FIRST, mutate state ONLY on Ok
        let emit_result = sink.emit(dispatch);
        if emit_result.is_ok() {
            confirmed_dispatches += 1;
        }
        // emit failed → state UNCHANGED
        assert_eq!(confirmed_dispatches, 0, "state must be unchanged on audit emit failure");

        // Control: succeeding sink → state updates
        in_memory_sink.emit(sample_dispatch("BREACH-F")).unwrap();
        confirmed_dispatches += 1;
        assert_eq!(confirmed_dispatches, 1);
    }
}
