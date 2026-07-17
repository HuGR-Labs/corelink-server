//! `corelink-handler-admin` — Admin API HTTP handler skeleton (R-prep).
//!
//! Two trait halves:
//!
//! - [`handler::AdminReadHandler`] — read-only inspections of the
//!   admin surface (tenant lookup, quota inspection, audit-row
//!   pagination, isolation-assertion probe). Ships first because
//!   readonly handlers can be safely exposed without dual-approval
//!   gating.
//! - [`handler::AdminMutateHandler`] — admin mutations that **MUST**
//!   carry a dual-approval token. The token's second approver must
//!   be a distinct principal from the initiator; the handler
//!   rejects with [`error::AdminHandlerError::DualApprovalMissing`]
//!   otherwise, fail-CLOSED + audit BEFORE the rejection.
//!
//! # SLI emit
//!
//! Every admin handler entry emits one [`observer::SliObservation`]
//! tagged `Sli::AvailControlPlane` (the audit P0-1 closure SLI).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod handler;
pub mod ledger;
pub mod observer;

pub use audit::{AuditEvent, AuditEventKind, AuditSink, InMemoryAuditSink};
pub use error::AdminHandlerError;
pub use handler::{
    AdminMutateHandler, AdminMutateRequest, AdminMutateResponse, AdminReadHandler,
    AdminReadRequest, AdminReadResponse, DualApprovalToken, InMemoryAdminHandler, MutateOp,
};
pub use ledger::{
    ApprovalLedger, ApprovalLedgerWriter, ApprovalRejection, InMemoryApprovalLedger,
    VerifiedApproval,
};
pub use observer::{InMemorySliObserver, Sli, SliObservation, SliObserver};
