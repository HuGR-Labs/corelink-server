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
    ///
    /// The `approver` here is the **ledger-recorded, authenticated**
    /// approver identity — NOT the free-text `approver` field from the
    /// client request body (which is advisory only and never trusted for
    /// the authorization decision).
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

    /// Mutation carried an `approval_id` that maps to **no record** in
    /// the approval ledger. This is the fail-CLOSED reject for a
    /// forged / absent / already-garbage-collected approval — the client
    /// cannot conjure authorization by supplying an arbitrary id +
    /// free-text approver.
    #[error("admin dual-approval unknown: no ledger record for approval_id={approval_id}")]
    DualApprovalUnknown {
        /// The unrecognised approval id.
        approval_id: String,
    },

    /// The ledger record for `approval_id` was created for a **different**
    /// resource than the one being mutated. An approval is scope-bound:
    /// an approval minted for tenant A cannot authorize a mutation of
    /// tenant B.
    #[error(
        "admin dual-approval scope mismatch: approval_id={approval_id} \
         was not recorded for resource={resource}"
    )]
    DualApprovalScopeMismatch {
        /// The approval id whose recorded scope did not match.
        approval_id: String,
        /// The resource the mutation targeted.
        resource: String,
    },

    /// The ledger record for `approval_id` has already been consumed by a
    /// prior mutation. Approvals are **single-use**: this is the
    /// anti-replay reject.
    #[error("admin dual-approval already consumed (replay): approval_id={approval_id}")]
    DualApprovalConsumed {
        /// The approval id that was already spent.
        approval_id: String,
    },

    /// The approval ledger backend was unreachable / errored. Fail-CLOSED:
    /// a mutation is NEVER committed when the ledger cannot be
    /// authoritatively consulted + consumed.
    #[error("admin dual-approval ledger unavailable: {0}")]
    ApprovalLedgerUnavailable(String),

    /// Audit emit failed BEFORE mutation; state unchanged.
    #[error("admin audit emit failed: {0}")]
    AuditFailed(String),

    /// Internal state inconsistency.
    #[error("internal admin-handler error: {0}")]
    Internal(String),
}
