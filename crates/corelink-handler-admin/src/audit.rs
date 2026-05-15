//! Admin handler audit surface.

use std::sync::Mutex;

/// Admin audit event taxonomy. `#[non_exhaustive]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuditEventKind {
    /// `corelink.admin.read.attempted` — emitted on read entry.
    ReadAttempted,
    /// `corelink.admin.read.served` — emitted on successful read.
    ReadServed,
    /// `corelink.admin.read.denied` — emitted on RBAC rejection.
    ReadDenied,
    /// `corelink.admin.mutate.attempted` — emitted on mutate entry
    /// BEFORE the dual-approval check.
    MutateAttempted,
    /// `corelink.admin.mutate.dual_approval_rejected` — emitted when
    /// the dual-approval token is missing or self-approved.
    MutateDualApprovalRejected,
    /// `corelink.admin.mutate.committed` — emitted on successful
    /// mutation AFTER state change.
    MutateCommitted,
    /// `corelink.admin.mutate.denied` — emitted on RBAC rejection.
    MutateDenied,
}

impl AuditEventKind {
    /// Canonical lower-case dotted slug.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ReadAttempted => "corelink.admin.read.attempted",
            Self::ReadServed => "corelink.admin.read.served",
            Self::ReadDenied => "corelink.admin.read.denied",
            Self::MutateAttempted => "corelink.admin.mutate.attempted",
            Self::MutateDualApprovalRejected => "corelink.admin.mutate.dual_approval_rejected",
            Self::MutateCommitted => "corelink.admin.mutate.committed",
            Self::MutateDenied => "corelink.admin.mutate.denied",
        }
    }
}

/// Admin audit row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    /// Event kind.
    pub kind: AuditEventKind,
    /// Principal performing the action.
    pub principal: String,
    /// Free-form resource ID (tenant ID, quota ID, etc.).
    pub resource: String,
    /// Optional second-approver principal (set on mutate paths).
    pub approver: Option<String>,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
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

    /// Snapshot of recorded rows.
    ///
    /// # Errors
    ///
    /// Returns an error string on lock poisoning.
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
    /// Returns an error string on lock poisoning.
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
