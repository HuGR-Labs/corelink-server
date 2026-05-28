//! AC handler audit surface.

use std::sync::Mutex;

/// AC audit event taxonomy. `#[non_exhaustive]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuditEventKind {
    /// `corelink.ac.lookup.attempted` — emitted on lookup entry.
    LookupAttempted,
    /// `corelink.ac.lookup.hit` — emitted on cache hit.
    LookupHit,
    /// `corelink.ac.lookup.miss` — emitted on cache miss.
    LookupMiss,
    /// `corelink.ac.lookup.denied` — emitted on cross-tenant rejection.
    LookupDenied,
    /// `corelink.ac.update.attempted` — emitted on update entry.
    UpdateAttempted,
    /// `corelink.ac.update.committed` — emitted on successful update.
    UpdateCommitted,
    /// `corelink.ac.update.denied` — emitted on cross-tenant rejection.
    UpdateDenied,
}

impl AuditEventKind {
    /// Canonical lower-case dotted slug.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::LookupAttempted => "corelink.ac.lookup.attempted",
            Self::LookupHit => "corelink.ac.lookup.hit",
            Self::LookupMiss => "corelink.ac.lookup.miss",
            Self::LookupDenied => "corelink.ac.lookup.denied",
            Self::UpdateAttempted => "corelink.ac.update.attempted",
            Self::UpdateCommitted => "corelink.ac.update.committed",
            Self::UpdateDenied => "corelink.ac.update.denied",
        }
    }
}

/// Minimal AC audit row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    /// Event kind.
    pub kind: AuditEventKind,
    /// Tenant operated on.
    pub tenant: String,
    /// Action digest.
    pub action_digest: String,
    /// Caller principal hash.
    pub principal: String,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AuditEvent {
    /// Construct from fields. The struct is `#[non_exhaustive]` so this
    /// is the only out-of-crate construction path (required by out-of-crate
    /// AC handler impls like the R2-backed one in `corelink-container`).
    #[must_use]
    pub fn new(
        kind: AuditEventKind,
        tenant: impl Into<String>,
        action_digest: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            kind,
            tenant: tenant.into(),
            action_digest: action_digest.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Fail-CLOSED audit sink trait.
pub trait AuditSink: Send + Sync + core::fmt::Debug {
    /// Persist a single row.
    ///
    /// # Errors
    ///
    /// Returns a sink-specific error string on durable write failure.
    fn emit(&self, event: AuditEvent) -> Result<(), String>;
}

/// Capture-everything in-process audit sink.
#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    rows: Mutex<Vec<AuditEvent>>,
    fail_with: Mutex<Option<String>>,
}

impl InMemoryAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all rows recorded so far.
    ///
    /// # Errors
    ///
    /// Returns an error string if the internal lock is poisoned.
    pub fn snapshot(&self) -> Result<Vec<AuditEvent>, String> {
        self.rows
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "audit sink poisoned".to_string())
    }

    /// Cause every subsequent emit to fail with `msg`.
    ///
    /// # Errors
    ///
    /// Returns an error string if the internal lock is poisoned.
    pub fn inject_failure(&self, msg: impl Into<String>) -> Result<(), String> {
        let mut g = self
            .fail_with
            .lock()
            .map_err(|_| "audit sink poisoned".to_string())?;
        *g = Some(msg.into());
        Ok(())
    }
}

impl AuditSink for InMemoryAuditSink {
    fn emit(&self, event: AuditEvent) -> Result<(), String> {
        let fail = self
            .fail_with
            .lock()
            .map_err(|_| "audit sink poisoned".to_string())?
            .clone();
        if let Some(msg) = fail {
            return Err(msg);
        }
        let mut g = self
            .rows
            .lock()
            .map_err(|_| "audit sink poisoned".to_string())?;
        g.push(event);
        Ok(())
    }
}
