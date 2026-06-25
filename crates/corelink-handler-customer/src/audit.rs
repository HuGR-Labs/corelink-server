//! Customer handler audit event surface.
//!
//! Light-weight audit row + sink **local to this crate** so the
//! handler can satisfy `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` without
//! pulling the full `corelink-audit` CloudEvents envelope into the
//! handler's transitive dependency closure. Production wiring will
//! adapt this sink to forward into `corelink-audit::Emitter` at the
//! `apps/server` integration point.

use std::sync::Mutex;

/// Canonical customer handler audit event taxonomy.
/// `#[non_exhaustive]` — callers MUST use a wildcard arm.
///
/// 14 variants covering all 6 endpoint groups: overview (read only),
/// usage (read only), billing (read + portal action), keys (list + create
/// + revoke), team (list + invite), audit (query only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuditEventKind {
    // --- Overview ---
    /// `corelink.customer.overview.attempted` — emitted on every
    /// overview entry BEFORE the data lookup.
    OverviewAttempted,
    /// `corelink.customer.overview.served` — emitted after a
    /// successful overview response.
    OverviewServed,
    /// `corelink.customer.overview.denied` — emitted on cross-tenant
    /// or auth rejection BEFORE the rejection response.
    OverviewDenied,

    // --- Usage ---
    /// `corelink.customer.usage.attempted` — emitted on every usage
    /// entry BEFORE the data lookup.
    UsageAttempted,
    /// `corelink.customer.usage.served` — emitted after a successful
    /// usage response.
    UsageServed,
    /// `corelink.customer.usage.denied` — emitted on cross-tenant or
    /// auth rejection BEFORE the rejection response.
    UsageDenied,

    // --- Billing ---
    /// `corelink.customer.billing.attempted` — emitted on every
    /// billing entry BEFORE the data lookup.
    BillingAttempted,
    /// `corelink.customer.billing.served` — emitted after a
    /// successful billing response.
    BillingServed,
    /// `corelink.customer.billing.denied` — emitted on cross-tenant
    /// or auth rejection BEFORE the rejection response.
    BillingDenied,

    // --- Keys (PAT) ---
    /// `corelink.customer.keys.create.attempted` — emitted BEFORE the
    /// PAT creation mutation.
    KeyCreateAttempted,
    /// `corelink.customer.keys.create.committed` — emitted AFTER a
    /// PAT is durably created.
    KeyCreateCommitted,
    /// `corelink.customer.keys.revoke.attempted` — emitted BEFORE the
    /// PAT revocation mutation.
    KeyRevokeAttempted,
    /// `corelink.customer.keys.revoke.committed` — emitted AFTER a
    /// PAT is durably revoked (idempotent; re-revoke still emits this).
    KeyRevokeCommitted,
    /// `corelink.customer.keys.denied` — emitted on cross-tenant or
    /// auth rejection BEFORE the rejection response (covers list +
    /// create + revoke).
    KeysDenied,

    // --- Team ---
    /// `corelink.customer.team.invite.attempted` — emitted BEFORE the
    /// invite mutation.
    TeamInviteAttempted,
    /// `corelink.customer.team.invite.committed` — emitted AFTER the
    /// invite is durably recorded.
    TeamInviteCommitted,
    /// `corelink.customer.team.denied` — emitted on cross-tenant or
    /// auth rejection BEFORE the rejection response (covers list +
    /// invite + remove).
    TeamDenied,
    /// `corelink.customer.team.remove.attempted` — emitted BEFORE the
    /// seat-removal mutation.
    TeamRemoveAttempted,
    /// `corelink.customer.team.remove.committed` — emitted AFTER the seat
    /// is durably removed and the member's PATs revoked.
    TeamRemoveCommitted,

    // --- Audit query ---
    /// `corelink.customer.audit.query.attempted` — emitted on every
    /// audit query entry BEFORE the data lookup.
    AuditQueryAttempted,
    /// `corelink.customer.audit.query.served` — emitted after a
    /// successful audit query response.
    AuditQueryServed,
    /// `corelink.customer.audit.query.denied` — emitted on cross-tenant
    /// or auth rejection BEFORE the rejection response.
    AuditQueryDenied,
}

impl AuditEventKind {
    /// Canonical lower-case dotted slug used in the audit row +
    /// downstream `corelink-audit` envelope `type` field.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::OverviewAttempted => "corelink.customer.overview.attempted",
            Self::OverviewServed => "corelink.customer.overview.served",
            Self::OverviewDenied => "corelink.customer.overview.denied",
            Self::UsageAttempted => "corelink.customer.usage.attempted",
            Self::UsageServed => "corelink.customer.usage.served",
            Self::UsageDenied => "corelink.customer.usage.denied",
            Self::BillingAttempted => "corelink.customer.billing.attempted",
            Self::BillingServed => "corelink.customer.billing.served",
            Self::BillingDenied => "corelink.customer.billing.denied",
            Self::KeyCreateAttempted => "corelink.customer.keys.create.attempted",
            Self::KeyCreateCommitted => "corelink.customer.keys.create.committed",
            Self::KeyRevokeAttempted => "corelink.customer.keys.revoke.attempted",
            Self::KeyRevokeCommitted => "corelink.customer.keys.revoke.committed",
            Self::KeysDenied => "corelink.customer.keys.denied",
            Self::TeamInviteAttempted => "corelink.customer.team.invite.attempted",
            Self::TeamInviteCommitted => "corelink.customer.team.invite.committed",
            Self::TeamDenied => "corelink.customer.team.denied",
            Self::TeamRemoveAttempted => "corelink.customer.team.remove.attempted",
            Self::TeamRemoveCommitted => "corelink.customer.team.remove.committed",
            Self::AuditQueryAttempted => "corelink.customer.audit.query.attempted",
            Self::AuditQueryServed => "corelink.customer.audit.query.served",
            Self::AuditQueryDenied => "corelink.customer.audit.query.denied",
        }
    }
}

/// Minimal customer audit event row. Fields chosen so a downstream
/// adapter can populate a full `corelink-audit::AuthEvent` envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    /// Event kind (canonical taxonomy).
    pub kind: AuditEventKind,
    /// Tenant whose data is being operated on (session-derived).
    pub tenant: String,
    /// Caller principal (opaque hash; production wiring routes a
    /// `PrincipalIdHash`).
    pub principal: String,
    /// Optional resource identifier (PAT id, team member email, etc.).
    /// Empty string when not applicable.
    pub resource: String,
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
        principal: impl Into<String>,
        resource: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            kind,
            tenant: tenant.into(),
            principal: principal.into(),
            resource: resource.into(),
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

    /// Clear a previously injected failure, restoring normal behavior.
    ///
    /// # Errors
    ///
    /// Returns an `"audit sink poisoned"` string if the internal lock
    /// is poisoned.
    pub fn clear_failure(&self) -> Result<(), String> {
        let mut g = self
            .fail_with
            .lock()
            .map_err(|_| "audit sink poisoned".to_string())?;
        *g = None;
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
