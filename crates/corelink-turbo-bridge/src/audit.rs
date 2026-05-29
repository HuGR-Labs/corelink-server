//! Audit surface for `corelink-turbo-bridge`.
//!
//! Five audit event kinds covering the full PUT/GET lifecycle:
//!
//! | Kind | When emitted |
//! |------|--------------|
//! | `PutAttempted` | Before any storage write (intent record). |
//! | `PutCommitted` | After a durable store (outcome record). |
//! | `PutDenied` | Cross-tenant PUT rejected — emitted BEFORE returning denial. |
//! | `GetAttempted` | Before any storage read. |
//! | `GetServed` | After a successful read. |
//! | `GetDenied` | Cross-tenant GET rejected — emitted BEFORE returning denial. |

use std::sync::Mutex;

use crate::error::TurboBridgeError;

/// The kind of audit event.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurboAuditEventKind {
    /// A PUT was received and is about to be attempted (intent record).
    PutAttempted,
    /// A PUT completed and the artifact is durably stored.
    PutCommitted,
    /// A PUT was denied because `team_id` ≠ `caller_tenant`.
    PutDenied,
    /// A GET was received and the store is about to be queried.
    GetAttempted,
    /// A GET completed and the artifact bytes are being returned.
    GetServed,
    /// A GET was denied because `team_id` ≠ `caller_tenant`.
    GetDenied,
}

/// A single audit event emitted by the turbo bridge.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TurboAuditEvent {
    /// Event kind.
    pub kind: TurboAuditEventKind,
    /// CoreLink tenant (equals `team_id` on authorised paths).
    pub tenant: String,
    /// Turbo artifact hash (opaque key).
    pub hash: String,
    /// Authenticated principal identity.
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
    /// Informational slug supplied by the Turbo client (not used for
    /// partitioning; logged for debugging / audit).
    pub slug: String,
}

impl TurboAuditEvent {
    /// Construct a [`TurboAuditEvent`].
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: TurboAuditEventKind,
        tenant: impl Into<String>,
        hash: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
        slug: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            tenant: tenant.into(),
            hash: hash.into(),
            principal: principal.into(),
            at_unix_ms,
            slug: slug.into(),
        }
    }
}

/// Trait for receiving turbo-bridge audit events.
pub trait TurboAuditSink: Send + Sync + core::fmt::Debug {
    /// Emit one audit event.
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::AuditFailed`] if the sink is unavailable.
    /// Callers must treat this as a hard error and abort the operation
    /// (fail-CLOSED ordering).
    fn emit(&self, event: TurboAuditEvent) -> Result<(), TurboBridgeError>;
}

/// In-memory audit sink that captures all events.  Intended for unit tests.
pub struct InMemoryTurboAuditSink {
    events: Mutex<Vec<TurboAuditEvent>>,
    inject_failure: Mutex<Option<String>>,
}

impl core::fmt::Debug for InMemoryTurboAuditSink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryTurboAuditSink").finish_non_exhaustive()
    }
}

impl InMemoryTurboAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            inject_failure: Mutex::new(None),
        }
    }

    /// Snapshot all captured events.
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] if the storage lock is poisoned.
    pub fn snapshot(&self) -> Result<Vec<TurboAuditEvent>, TurboBridgeError> {
        self.events
            .lock()
            .map(|g| g.clone())
            .map_err(|_| TurboBridgeError::Internal("audit lock poisoned".into()))
    }

    /// Inject a failure so the next emit returns [`TurboBridgeError::AuditFailed`].
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] if the storage lock is poisoned.
    pub fn inject_failure(&self, msg: impl Into<String>) -> Result<(), TurboBridgeError> {
        let mut g = self
            .inject_failure
            .lock()
            .map_err(|_| TurboBridgeError::Internal("inject lock poisoned".into()))?;
        *g = Some(msg.into());
        Ok(())
    }
}

impl Default for InMemoryTurboAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl TurboAuditSink for InMemoryTurboAuditSink {
    fn emit(&self, event: TurboAuditEvent) -> Result<(), TurboBridgeError> {
        {
            let g = self
                .inject_failure
                .lock()
                .map_err(|_| TurboBridgeError::Internal("inject lock poisoned".into()))?;
            if let Some(msg) = g.as_ref() {
                return Err(TurboBridgeError::AuditFailed(msg.clone()));
            }
        }
        self.events
            .lock()
            .map_err(|_| TurboBridgeError::Internal("audit lock poisoned".into()))?
            .push(event);
        Ok(())
    }
}
