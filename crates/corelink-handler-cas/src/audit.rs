//! CAS handler audit event surface.
//!
//! Light-weight audit row + sink **local to this crate** so the
//! handler can satisfy `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` without
//! pulling the full `corelink-audit` CloudEvents envelope into the
//! handler's transitive dependency closure. Production wiring will
//! adapt this sink to forward into `corelink-audit::Emitter` at the
//! `apps/server` integration point.

use std::sync::Mutex;

/// Canonical CAS audit event taxonomy. `#[non_exhaustive]` — callers
/// MUST use a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuditEventKind {
    /// `corelink.cas.read.attempted` — emitted on every read entry
    /// BEFORE the storage lookup.
    ReadAttempted,
    /// `corelink.cas.read.served` — emitted after a successful read
    /// (bytes returned to caller).
    ReadServed,
    /// `corelink.cas.read.denied` — emitted on cross-tenant or auth
    /// rejection BEFORE the rejection response.
    ReadDenied,
    /// `corelink.cas.write.attempted` — emitted on every write entry
    /// BEFORE the storage mutation.
    WriteAttempted,
    /// `corelink.cas.write.committed` — emitted after a successful
    /// write (bytes durably stored).
    WriteCommitted,
    /// `corelink.cas.write.denied` — emitted on cross-tenant or
    /// auth rejection BEFORE the rejection response.
    WriteDenied,
    /// `corelink.cas.delete.attempted` — emitted on every delete entry
    /// BEFORE the storage delete.
    DeleteAttempted,
    /// `corelink.cas.delete.committed` — emitted after a delete
    /// completes (idempotent: fires whether or not the blob existed).
    DeleteCommitted,
    /// `corelink.cas.delete.denied` — emitted on cross-tenant or auth
    /// rejection BEFORE the rejection response.
    DeleteDenied,
    /// `corelink.cas.list.attempted` — emitted on every list entry
    /// BEFORE the storage enumeration.
    ListAttempted,
    /// `corelink.cas.list.denied` — emitted on cross-tenant or auth
    /// rejection BEFORE the rejection response.
    ListDenied,
    /// `corelink.cas.correctness.violation` — emitted whenever a
    /// hash mismatch is observed (zero-budget SLO-CORRECT-CAS).
    CorrectnessViolation,
}

impl AuditEventKind {
    /// Canonical lower-case dotted slug used in the audit row +
    /// downstream `corelink-audit` envelope `type` field.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ReadAttempted => "corelink.cas.read.attempted",
            Self::ReadServed => "corelink.cas.read.served",
            Self::ReadDenied => "corelink.cas.read.denied",
            Self::WriteAttempted => "corelink.cas.write.attempted",
            Self::WriteCommitted => "corelink.cas.write.committed",
            Self::WriteDenied => "corelink.cas.write.denied",
            Self::DeleteAttempted => "corelink.cas.delete.attempted",
            Self::DeleteCommitted => "corelink.cas.delete.committed",
            Self::DeleteDenied => "corelink.cas.delete.denied",
            Self::ListAttempted => "corelink.cas.list.attempted",
            Self::ListDenied => "corelink.cas.list.denied",
            Self::CorrectnessViolation => "corelink.cas.correctness.violation",
        }
    }
}

/// Minimal CAS audit event row. Fields chosen so a downstream adapter
/// can populate a full `corelink-audit::AuthEvent` envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    /// Event kind (canonical taxonomy).
    pub kind: AuditEventKind,
    /// Tenant whose data is being operated on (path-derived).
    pub tenant: String,
    /// Hex content hash referenced by the operation (empty for
    /// pre-write events where the hash is supplied by the caller).
    pub hash: String,
    /// Caller principal (opaque hash; production wiring routes a
    /// `PrincipalIdHash`).
    pub principal: String,
    /// Monotonic timestamp in unix-millis (logical clock in tests).
    pub at_unix_ms: u64,
}

impl AuditEvent {
    /// Construct an [`AuditEvent`] from its fields.
    ///
    /// Provided because the struct is `#[non_exhaustive]`, which
    /// prevents struct-literal construction from outside this crate.
    #[must_use]
    pub fn new(
        kind: AuditEventKind,
        tenant: impl Into<String>,
        hash: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            kind,
            tenant: tenant.into(),
            hash: hash.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Sink the handler emits audit rows to. Fail-CLOSED — every emit
/// returns `Err` if the sink cannot durably accept the event; the
/// handler aborts the mutation/response on that error.
pub trait AuditSink: Send + Sync + core::fmt::Debug {
    /// Persist a single audit row.
    ///
    /// # Errors
    ///
    /// Returns a sink-specific error string when the row cannot be
    /// durably written. The handler treats any error as fail-CLOSED.
    fn emit(&self, event: AuditEvent) -> Result<(), String>;
}

/// Capture-everything in-process audit sink for tests + apps/server
/// wire-up.
#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    rows: Mutex<Vec<AuditEvent>>,
    /// If set, every `emit` call returns this error string instead
    /// of recording the row — used to drive fail-CLOSED proptests.
    fail_with: Mutex<Option<String>>,
}

impl InMemoryAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all rows recorded so far (in emit order).
    ///
    /// # Errors
    ///
    /// Returns an `"audit sink poisoned"` string if the internal lock
    /// is poisoned.
    pub fn snapshot(&self) -> Result<Vec<AuditEvent>, String> {
        self.rows
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "audit sink poisoned".to_string())
    }

    /// Cause every subsequent `emit` to return `Err(msg)` without
    /// recording the row. Used to drive fail-CLOSED audit-failure
    /// proptests.
    ///
    /// # Errors
    ///
    /// Returns an `"audit sink poisoned"` string if the internal lock
    /// is poisoned.
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
