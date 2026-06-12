//! `AcHandlerError` taxonomy.

use thiserror::Error;

/// Error taxonomy for the AC handler surface. `#[non_exhaustive]`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AcHandlerError {
    /// No AC entry under the requested `(tenant, action_digest)`.
    /// In AC semantics a miss is **not** an error from the
    /// SLO-availability standpoint, but the trait surfaces it so
    /// the caller can branch.
    #[error("ac miss: tenant={tenant} action_digest={action_digest}")]
    Miss {
        /// Tenant the lookup was scoped to.
        tenant: String,
        /// Canonical action digest.
        action_digest: String,
    },

    /// Caller attempted to access another tenant's AC entry.
    /// Fail-CLOSED + audit emit BEFORE the response.
    #[error("ac cross-tenant denied: caller={caller} requested_tenant={requested_tenant}")]
    CrossTenantDenied {
        /// Caller's authenticated tenant.
        caller: String,
        /// Tenant in the URL/path.
        requested_tenant: String,
    },

    /// Caller PUT a divergent body for an existing `(tenant, action_digest)`.
    /// AC entries are immutable-once-proven; the handler refuses to overwrite a
    /// stored result with different bytes (a proven cache result must never be
    /// silently replaced). Maps to HTTP 409 Conflict. A byte-identical re-PUT is
    /// an idempotent no-op (`durable=false`), NOT this error.
    #[error("ac divergent body: tenant={tenant} action_digest={action_digest}")]
    DivergentBody {
        /// Tenant the update was scoped to.
        tenant: String,
        /// Canonical action digest.
        action_digest: String,
    },

    /// Audit emit failed BEFORE mutation; state unchanged.
    #[error("ac audit emit failed: {0}")]
    AuditFailed(String),

    /// Internal state inconsistency (lock poisoning, etc.).
    #[error("internal ac-handler error: {0}")]
    Internal(String),
}
