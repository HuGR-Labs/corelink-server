//! Audit emit (fail-CLOSED) for deploy events.
//!
//! Implements CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY.  If audit emit fails,
//! the deploy is blocked and an SEV-1 alert fires — there is **no** fallback
//! to best-effort (§7 anti-patterns).
//!
//! # Trait contract
//!
//! Implementors must:
//! 1. Emit the event before returning `Ok(())`.
//! 2. Return `Err(DeployVerifyError::AuditEmitFailed(_))` on any failure.
//! 3. Never silently drop events.

use std::sync::{Arc, Mutex};

use tracing::{error, info, warn};

use super::error::DeployVerifyError;
use super::types::DeployAuditEvent;

// ── Trait ──────────────────────────────────────────────────────────────────

/// Audit sink for deploy events.  Must be fail-CLOSED.
pub trait DeployAuditSink: Send + Sync {
    /// Emit a deploy audit event.
    ///
    /// **Fail-CLOSED contract**: returning `Err` causes the caller to block
    /// the deploy and emit an SEV-1 alert.  Never return `Ok` unless the
    /// event has been durably persisted.
    fn emit(&self, event: DeployAuditEvent) -> Result<(), DeployVerifyError>;
}

// ── In-memory sink (test / staging) ───────────────────────────────────────

/// In-memory `DeployAuditSink` for testing.
///
/// Stores all emitted events in an append-only `Vec`.  Thread-safe via
/// `Arc<Mutex<>>` per F-001.
#[derive(Debug, Default)]
pub struct InMemoryDeployAuditSink {
    events: Arc<Mutex<Vec<DeployAuditEvent>>>,
}

impl InMemoryDeployAuditSink {
    /// Construct a new empty in-memory sink.
    pub fn new() -> Self {
        Self { events: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Return a snapshot of all emitted events (cloned).
    pub fn events(&self) -> Vec<DeployAuditEvent> {
        self.events
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Return the number of emitted events.
    pub fn len(&self) -> usize {
        self.events
            .lock()
            .map(|g| g.len())
            .unwrap_or(0)
    }

    /// Returns `true` if no events have been emitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl DeployAuditSink for InMemoryDeployAuditSink {
    fn emit(&self, event: DeployAuditEvent) -> Result<(), DeployVerifyError> {
        info!(
            event_type = %event.event_type,
            release_tag = %event.release_tag,
            outcome = ?event.outcome,
            "deploy audit event emitted"
        );
        self.events
            .lock()
            .map_err(|e| DeployVerifyError::AuditEmitFailed(format!("mutex poisoned: {e}")))?
            .push(event);
        Ok(())
    }
}

// ── Failing sink (chaos / adversarial tests) ───────────────────────────────

/// `DeployAuditSink` that always returns an error — used to validate the
/// fail-CLOSED behaviour: when audit emit fails, deploy must be blocked.
#[derive(Debug)]
pub struct FailingDeployAuditSink {
    message: String,
}

impl FailingDeployAuditSink {
    /// Construct with a custom error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Default for FailingDeployAuditSink {
    fn default() -> Self {
        Self::new("S-09 audit chain endpoint unavailable (simulated 503)")
    }
}

impl DeployAuditSink for FailingDeployAuditSink {
    fn emit(&self, event: DeployAuditEvent) -> Result<(), DeployVerifyError> {
        error!(
            event_type = %event.event_type,
            release_tag = %event.release_tag,
            outcome = ?event.outcome,
            error = %self.message,
            "AUDIT EMIT FAILED — SEV-1: deploy will be blocked (fail-CLOSED)"
        );
        Err(DeployVerifyError::AuditEmitFailed(self.message.clone()))
    }
}

// ── Alert helper ──────────────────────────────────────────────────────────

/// Emit a structured log entry for an SEV-2 deploy blocked alert.
///
/// In production this log line is picked up by the CF Logpush → alerting
/// pipeline.  During S-12 a staging stub is used; full S-09 integration
/// lands in a later sprint.
pub fn alert_sev2_deploy_blocked(
    release_tag: &str,
    reason: &str,
    error: &DeployVerifyError,
) {
    warn!(
        sev = "SEV-2",
        alert_name = "deploy_blocked",
        release_tag = %release_tag,
        reason = %reason,
        error = %error,
        "ALERT SEV-2: deploy blocked — {reason}"
    );
}

/// Emit a structured log entry for an SEV-1 audit emit failure alert.
pub fn alert_sev1_audit_emit_failed(release_tag: &str, error: &DeployVerifyError) {
    error!(
        sev = "SEV-1",
        alert_name = "audit_emit_failed",
        release_tag = %release_tag,
        error = %error,
        "ALERT SEV-1: audit emit failed — deploy blocked (fail-CLOSED)"
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use super::super::types::VerifyOutcome;

    #[test]
    fn in_memory_sink_appends_events() {
        let sink = InMemoryDeployAuditSink::new();
        assert!(sink.is_empty());
        let ev = DeployAuditEvent::verified("v0.1.0", "trace-000");
        sink.emit(ev).expect("emit must succeed");
        assert_eq!(sink.len(), 1);
        let ev2 = DeployAuditEvent::blocked("v0.1.0", VerifyOutcome::SigInvalid, "trace-001");
        sink.emit(ev2).expect("emit must succeed");
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn failing_sink_returns_audit_emit_failed() {
        let sink = FailingDeployAuditSink::default();
        let ev = DeployAuditEvent::verified("v0.1.0", "trace-000");
        let result = sink.emit(ev);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DeployVerifyError::AuditEmitFailed(_)));
    }

    #[test]
    fn in_memory_sink_thread_safe() {
        use std::thread;
        let sink = Arc::new(InMemoryDeployAuditSink::new());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let s = Arc::clone(&sink);
                thread::spawn(move || {
                    let ev = DeployAuditEvent::verified(format!("v0.{i}.0"), "t");
                    s.emit(ev).expect("thread emit");
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread join");
        }
        assert_eq!(sink.len(), 10);
    }
}
