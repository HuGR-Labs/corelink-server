//! Audit envelope — every publish attempt emits a fail-CLOSED event.
//!
//! Per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (Lote 10.6bis) audit fires
//! BEFORE the final outcome is reported. The sink is fail-CLOSED: if
//! the audit emit returns an error the caller MUST propagate it
//! instead of treating the Statuspage publish as successful.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Canonical audit event taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum StatuspageAuditOutcome {
    /// Statuspage accepted the data-point (2xx).
    Published,
    /// Statuspage rejected the API key (401 / 403).
    AuthFailed,
    /// Local rate-limiter denied the publish (1-per-5-min quota).
    RateLimited,
    /// Publish failed after retry exhaustion OR on a permanent 4xx
    /// other than auth.
    Failed,
}

impl StatuspageAuditOutcome {
    /// Canonical event-type string emitted into the audit chain.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Published => "corelink.privacy.statuspage_published.v1",
            Self::AuthFailed => "corelink.privacy.statuspage_auth_failed.v1",
            Self::RateLimited => "corelink.privacy.statuspage_rate_limited.v1",
            Self::Failed => "corelink.privacy.statuspage_failed.v1",
        }
    }
}

/// Audit envelope for a single publish attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StatuspageAuditEvent {
    /// Outcome.
    pub outcome: StatuspageAuditOutcome,
    /// Statuspage page ID this publish targeted.
    pub page_id: String,
    /// Statuspage metric ID this publish targeted.
    pub metric_id: String,
    /// Redacted API key string (`OAuth ***<last4>` — NEVER plaintext).
    pub api_key_redacted: String,
    /// Final HTTP status (None on transport error / local reject).
    pub final_status: Option<u16>,
    /// Attempts made (1 = first-shot).
    pub attempts: u32,
    /// Optional reason string (filled on `Failed` / `AuthFailed` /
    /// `RateLimited`).
    pub reason: Option<String>,
    /// Canonical p95 hours observation that was published (audit
    /// trail of telemetry that left the system).
    pub p95_hours_observed: u64,
    /// Canonical window-end timestamp (Unix seconds) of the published
    /// data-point. Useful for forensic correlation with the SLO
    /// histogram observations.
    pub window_end_unix_s: u64,
}

/// Audit sink trait.
pub trait StatuspageAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single event.
    ///
    /// # Errors
    ///
    /// Returns [`StatuspageAuditError::EmitFailed`] when the audit
    /// chain rejects the event (fail-CLOSED).
    fn emit(&self, event: &StatuspageAuditEvent) -> Result<(), StatuspageAuditError>;
}

/// Audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatuspageAuditError {
    /// Audit chain rejected (storage failure, signature mismatch).
    #[error("statuspage audit emit failed: {0}")]
    EmitFailed(String),
}

/// In-memory sink — records every emitted event.
#[derive(Clone, Debug, Default)]
pub struct InMemoryStatuspageAuditSink {
    events: Arc<Mutex<Vec<StatuspageAuditEvent>>>,
}

impl InMemoryStatuspageAuditSink {
    /// Construct empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<StatuspageAuditEvent> {
        match self.events.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.events.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when no events recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl StatuspageAuditSink for InMemoryStatuspageAuditSink {
    fn emit(&self, event: &StatuspageAuditEvent) -> Result<(), StatuspageAuditError> {
        let mut g = self
            .events
            .lock()
            .map_err(|e| StatuspageAuditError::EmitFailed(format!("mutex poisoned: {e}")))?;
        g.push(event.clone());
        Ok(())
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

    #[test]
    fn event_types_are_canonical() {
        assert_eq!(
            StatuspageAuditOutcome::Published.event_type(),
            "corelink.privacy.statuspage_published.v1"
        );
        assert_eq!(
            StatuspageAuditOutcome::AuthFailed.event_type(),
            "corelink.privacy.statuspage_auth_failed.v1"
        );
        assert_eq!(
            StatuspageAuditOutcome::RateLimited.event_type(),
            "corelink.privacy.statuspage_rate_limited.v1"
        );
        assert_eq!(
            StatuspageAuditOutcome::Failed.event_type(),
            "corelink.privacy.statuspage_failed.v1"
        );
    }

    #[test]
    fn in_memory_sink_records() {
        let sink = InMemoryStatuspageAuditSink::new();
        let evt = StatuspageAuditEvent {
            outcome: StatuspageAuditOutcome::Published,
            page_id: "p1".to_string(),
            metric_id: "m1".to_string(),
            api_key_redacted: "OAuth ***1234".to_string(),
            final_status: Some(201),
            attempts: 1,
            reason: None,
            p95_hours_observed: 12,
            window_end_unix_s: 86_400,
        };
        sink.emit(&evt).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], evt);
    }
}
