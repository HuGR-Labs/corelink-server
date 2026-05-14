//! Email broadcast sink surface (SES mirror).
//!
//! Production wiring (Cloudflare Email / SES with retry queue + bounce
//! handling) is deferred to the PRR ship gate per the
//! `trait-abstraction-defer` charter pattern.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::error::DpaVersioningError;
use crate::version::SemverVersion;

/// Kind of message a broadcast emits. Used by the in-memory sink to
/// distinguish Major-bump notifications from grace-period reminders
/// from post-grace degrade warnings.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastKind {
    /// "DPA updated to v\<new\>; please re-accept by \<grace_end\>".
    MajorBumpNotice,
    /// Daily nudge while within [`crate::schema::REMINDER_WINDOW_SECONDS`]
    /// of grace expiry.
    GraceReminder,
    /// "Grace period expired; tenant is read-only until re-acceptance".
    DegradeNotice,
}

/// An envelope captured by the in-memory broadcast sink for assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastEnvelope {
    /// Tenant the message targeted.
    pub tenant_id: Uuid,
    /// DPA version the message references.
    pub version: SemverVersion,
    /// Kind discriminator.
    pub kind: BroadcastKind,
}

/// Sink primitive a production wiring satisfies. The trait is
/// **synchronous** by design — the production CF Worker bridges
/// async ↔ sync at the worker boundary, per the `no tokio in src`
/// charter constraint.
pub trait BroadcastSink: std::fmt::Debug + Send + Sync {
    /// Enqueue / dispatch a single broadcast envelope.
    ///
    /// Implementations MUST be retry-safe at the trait boundary —
    /// callers may invoke `send` twice for the same logical envelope
    /// on cron re-runs.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Broadcast`] when the underlying
    /// transport fails.
    fn send(&self, envelope: BroadcastEnvelope) -> Result<(), DpaVersioningError>;
}

/// In-memory capture sink for assertion-based testing.
#[derive(Debug, Default)]
pub struct InMemoryBroadcastSink {
    inner: Arc<Mutex<Vec<BroadcastEnvelope>>>,
}

impl InMemoryBroadcastSink {
    /// Construct an empty capture sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every envelope captured so far.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::LockPoisoned`] if a panic
    /// poisoned the inner mutex during a concurrent insert.
    pub fn captured(&self) -> Result<Vec<BroadcastEnvelope>, DpaVersioningError> {
        self.inner
            .lock()
            .map(|g| g.clone())
            .map_err(|_| DpaVersioningError::LockPoisoned)
    }
}

impl BroadcastSink for InMemoryBroadcastSink {
    fn send(&self, envelope: BroadcastEnvelope) -> Result<(), DpaVersioningError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DpaVersioningError::LockPoisoned)?;
        guard.push(envelope);
        Ok(())
    }
}

/// Always-failing sink for fail-CLOSED unit tests.
#[derive(Debug, Default)]
pub struct FailingBroadcastSink;

impl BroadcastSink for FailingBroadcastSink {
    fn send(&self, _envelope: BroadcastEnvelope) -> Result<(), DpaVersioningError> {
        Err(DpaVersioningError::Broadcast(
            "simulated transport failure".into(),
        ))
    }
}
