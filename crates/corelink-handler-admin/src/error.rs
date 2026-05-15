//! `AdminHandlerError` taxonomy.

use thiserror::Error;

/// Error taxonomy for the admin handler surface. `#[non_exhaustive]`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdminHandlerError {
    /// Requested tenant / resource not found.
    #[error("admin lookup not found: {what}")]
    NotFound {
        /// Free-form descriptor of the missing resource.
        what: String,
    },

    /// Caller lacks the required admin role.
    #[error("admin forbidden: principal={principal}")]
    Forbidden {
        /// Principal that attempted the action.
        principal: String,
    },

    /// Mutation lacked a dual-approval token. Fail-CLOSED + audit
    /// BEFORE the rejection.
    #[error("admin mutation requires dual-approval; got initiator-only")]
    DualApprovalMissing,

    /// Mutation carried a dual-approval token whose second approver
    /// is the same principal as the initiator (self-approval
    /// rejected per `dual_approval.md §3`).
    #[error(
        "admin dual-approval self-approval rejected: \
         initiator={initiator} approver={approver}"
    )]
    DualApprovalSelfApproval {
        /// Initiator principal.
        initiator: String,
        /// (Second) approver principal — must differ from initiator.
        approver: String,
    },

    /// Audit emit failed BEFORE mutation; state unchanged.
    #[error("admin audit emit failed: {0}")]
    AuditFailed(String),

    /// Internal state inconsistency.
    #[error("internal admin-handler error: {0}")]
    Internal(String),
}
