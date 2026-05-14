//! Error taxonomy for sub-processor emit operations.

use thiserror::Error;

/// Canonical error taxonomy for sub-processor emit operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SubProcessorEmitError {
    /// Audit sink emit failed (fail-CLOSED: operation aborted).
    #[error("sub-processor audit emit failed: {0}")]
    Audit(#[from] SubProcessorAuditSinkError),

    /// Broadcast store write failed.
    #[error("sub-processor broadcast store failed: {0}")]
    BroadcastStore(#[from] SubProcessorBroadcastStoreError),

    /// Objection store write failed.
    #[error("sub-processor objection store failed: {0}")]
    ObjectionStore(#[from] SubProcessorObjectionStoreError),

    /// DKIM key derivation failed.
    #[error("sub-processor DKIM key derivation failed: {0}")]
    DkimDerivation(String),

    /// Invalid state transition in objection ticket state machine.
    #[error("invalid objection ticket state transition from {from} to {to}")]
    InvalidStateTransition {
        /// Current state name.
        from: String,
        /// Target state name.
        to: String,
    },

    /// Idempotency constraint violation (UNIQUE key conflict).
    #[error("sub-processor broadcast idempotency conflict: {0}")]
    IdempotencyConflict(String),

    /// Internal / unexpected error.
    #[error("sub-processor emit internal error: {0}")]
    Internal(String),
}

/// Error returned by [`SubProcessorAuditSink`][crate::audit::SubProcessorAuditSink].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SubProcessorAuditSinkError {
    /// Sink is unavailable or refused the record.
    #[error("audit sink unavailable: {0}")]
    Unavailable(String),
}

/// Error returned by [`BroadcastStore`][crate::broadcast::BroadcastStore].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SubProcessorBroadcastStoreError {
    /// Store is unavailable.
    #[error("broadcast store unavailable: {0}")]
    Unavailable(String),
}

/// Error returned by [`ObjectionStore`][crate::objection::ObjectionStore].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SubProcessorObjectionStoreError {
    /// Store is unavailable.
    #[error("objection store unavailable: {0}")]
    Unavailable(String),
    /// Duplicate objection (UNIQUE constraint).
    #[error("duplicate objection: tenant={tenant_id} subject={subject_id} sp={sub_processor_id} version={version}")]
    Duplicate {
        /// Tenant ID.
        tenant_id: String,
        /// Subject ID.
        subject_id: String,
        /// Sub-processor ID.
        sub_processor_id: String,
        /// Sub-processors version.
        version: String,
    },
}
