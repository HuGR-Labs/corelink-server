//! `corelink-signup` canonical error taxonomy.

use thiserror::Error;

use crate::audit::SignupAuditEmitError;
use crate::billing::BillingError;
use crate::store::StorageError;

/// Canonical orchestrator error taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs (e.g. WI-S19-002
/// DPA receipt JWT signing failure, WI-S19-004 Stripe Checkout
/// activation race).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OrchestrationError {
    /// Caller-supplied input failed validation (empty `IdempotencyKey`,
    /// missing `clerk_event_id`, etc.). The orchestrator did NOT open a
    /// D1 transaction; outcome reports `SignupOutcome::Rejected`.
    #[error("invalid signup input: {0}")]
    InvalidInput(String),
    /// Audit emit failed; per INV-AUDIT-APPEND-ONLY + Lote 10.6bis
    /// pattern the orchestrator aborts the mutation. Caller MUST NOT
    /// retry without first resolving the audit-sink failure.
    #[error("signup audit emit failed: {0}")]
    Audit(#[from] SignupAuditEmitError),
    /// Atomic store mutation failed; the orchestrator triggered the
    /// `rollback` arm of the transaction. Caller may retry (idempotent
    /// per `IdempotencyKey`).
    #[error("signup storage failure (step {step}): {source}")]
    Storage {
        /// The orchestration step inside which the failure occurred.
        step: crate::outcome::OrchestrationStep,
        /// Underlying storage error cause.
        #[source]
        source: StorageError,
    },
    /// Billing client returned a non-retryable error. The atomic D1 tx
    /// is **still committed** (per Lote 10.19 codex P0 canonical scope:
    /// Stripe is NOT inside the atomic boundary); the caller receives
    /// `SignupOutcome::Deferred { billing: BillingIntent::Deferred }`.
    #[error("billing client failure: {0}")]
    Billing(#[from] BillingError),
    /// Internal invariant violation (e.g. `Mutex` poisoning, state
    /// corruption). Treat as non-recoverable; caller MUST tear down the
    /// orchestrator instance + reconstruct from the durable D1 mirror.
    #[error("internal orchestrator fault: {0}")]
    Internal(String),
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
    use crate::outcome::OrchestrationStep;

    #[test]
    fn invalid_input_display() {
        let e = OrchestrationError::InvalidInput("empty idempotency key".to_string());
        assert!(e.to_string().contains("empty idempotency key"));
    }

    #[test]
    fn audit_error_converts() {
        let e: OrchestrationError = SignupAuditEmitError::Rejected("sink fail".to_string()).into();
        assert!(matches!(e, OrchestrationError::Audit(_)));
    }

    #[test]
    fn storage_error_display_carries_step() {
        let e = OrchestrationError::Storage {
            step: OrchestrationStep::InsertTenant,
            source: StorageError::Aborted("FK violation".to_string()),
        };
        assert!(e.to_string().contains("insert_tenant"));
    }
}
