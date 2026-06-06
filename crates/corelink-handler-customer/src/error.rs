//! `CustomerHandlerError` taxonomy.

use thiserror::Error;

/// Error taxonomy for the customer handler surface. `#[non_exhaustive]`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CustomerHandlerError {
    /// Cross-tenant access denied — the caller's authenticated tenant
    /// does not match the requested tenant. Audit row emitted BEFORE
    /// this error is returned (fail-CLOSED ordering).
    #[error("cross-tenant denied: caller_tenant={caller} requested_tenant={requested_tenant}")]
    CrossTenantDenied {
        /// Caller's authenticated tenant.
        caller: String,
        /// Tenant from the request that was denied.
        requested_tenant: String,
    },

    /// Caller is not authenticated / session invalid.
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Requested resource not found.
    #[error("not found: {what}")]
    NotFound {
        /// Free-form descriptor of the missing resource.
        what: String,
    },

    /// Audit emit failed BEFORE mutation; state unchanged (fail-CLOSED).
    #[error("customer audit emit failed: {0}")]
    AuditFailed(String),

    /// Internal state inconsistency.
    #[error("internal customer-handler error: {0}")]
    Internal(String),
}
