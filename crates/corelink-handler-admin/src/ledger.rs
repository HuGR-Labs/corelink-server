//! Approval ledger — the persisted, independently-recorded second-approver
//! store that makes admin dual-approval a REAL two-person control instead of
//! a free-text string compare.
//!
//! # Why this exists (the theater it replaces)
//!
//! The admin mutate path used to "enforce" dual-approval with a single
//! comparison: `if token.approver == initiator { reject }`. Because the
//! `approver` was free-text supplied by the client request body — and the
//! initiator on the operator-gated path is a hardcoded constant — a single
//! key-holder could self-grant any tier simply by putting a *different*
//! string in `approver`. No independent record of a second approver was ever
//! consulted.
//!
//! The [`ApprovalLedger`] port fixes that: an approval must have been
//! **independently recorded** (behind its own authenticated "create approval"
//! step) BEFORE it can authorize a mutation. At mutate time the handler
//! consults the ledger and it, not the request body, is the authority for:
//!
//! - **existence** — an unknown `approval_id` is rejected
//!   ([`AdminHandlerError::DualApprovalUnknown`]);
//! - **distinct approver** — the ledger-recorded, authenticated approver
//!   MUST differ from the initiator
//!   ([`AdminHandlerError::DualApprovalSelfApproval`]);
//! - **scope** — the approval must have been recorded for THIS resource
//!   ([`AdminHandlerError::DualApprovalScopeMismatch`]);
//! - **single use** — a consumed approval cannot be replayed
//!   ([`AdminHandlerError::DualApprovalConsumed`]).
//!
//! The verification and consumption are one atomic step
//! ([`ApprovalLedger::verify_and_consume`]) so a concurrent replay of the
//! same approval cannot double-spend.

use std::collections::HashMap;
use std::sync::Mutex;

/// The outcome of a successful [`ApprovalLedger::verify_and_consume`]: the
/// **ledger-recorded, authenticated** approver identity that authorized the
/// mutation. Callers record THIS in the audit trail — never the free-text
/// request-body `approver`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VerifiedApproval {
    /// The authenticated approver principal recorded in the ledger.
    pub approver: String,
}

impl VerifiedApproval {
    /// Construct from the recorded approver. The struct is `#[non_exhaustive]`,
    /// so out-of-crate ledger impls (e.g. the container's `D1ApprovalLedger`)
    /// use this rather than the struct-literal syntax.
    #[must_use]
    pub fn new(approver: impl Into<String>) -> Self {
        Self {
            approver: approver.into(),
        }
    }
}

/// Why an approval failed verification. Each variant maps to a distinct
/// [`crate::error::AdminHandlerError`] reject at the handler boundary; every
/// variant is fail-CLOSED (no mutation is committed).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApprovalRejection {
    /// No ledger record exists for the supplied `approval_id` (forged /
    /// absent). This is the reject that kills the free-text bypass.
    Unknown,
    /// The record's recorded resource differs from the mutation's resource.
    ScopeMismatch,
    /// The record's authenticated approver equals the initiator.
    SelfApproval {
        /// The ledger-recorded approver identity that collided with the
        /// initiator.
        approver: String,
    },
    /// The approval was already consumed (single-use replay).
    Consumed,
    /// The ledger backend errored / was unreachable — fail-CLOSED.
    Backend(String),
}

/// Persisted second-approver ledger. Implementations MUST make
/// [`verify_and_consume`](ApprovalLedger::verify_and_consume) an atomic
/// lookup-verify-consume so a concurrent double-spend of one approval is
/// impossible.
///
/// The in-process [`InMemoryApprovalLedger`] is the reference implementation
/// (tests + dev/CI). The production container wires a D1-backed
/// implementation over the durable `admin_approvals` table.
pub trait ApprovalLedger: Send + Sync + core::fmt::Debug {
    /// Verify that `approval_id` names a recorded, unconsumed approval that
    /// (1) was recorded for `resource` and (2) whose recorded, authenticated
    /// approver differs from `initiator`; then **consume** it (single-use).
    ///
    /// Returns the recorded approver on success, or an [`ApprovalRejection`]
    /// describing the fail-CLOSED reason. The implementation MUST NOT consume
    /// the approval on any reject path.
    fn verify_and_consume(
        &self,
        approval_id: &str,
        initiator: &str,
        resource: &str,
    ) -> Result<VerifiedApproval, ApprovalRejection>;
}

/// The write ("create approval") half of the ledger, kept separate from
/// [`ApprovalLedger`] (the verify+consume half) so the two capabilities can be
/// handed to different route surfaces: the approve endpoint gets a writer, the
/// mutate handler gets a verifier, and in production both are backed by the
/// SAME durable object.
///
/// `approver` is always the INDEPENDENTLY-AUTHENTICATED approver identity
/// derived from the approve endpoint's own auth gate — never a value taken
/// from a client request body.
pub trait ApprovalLedgerWriter: Send + Sync + core::fmt::Debug {
    /// Record ("create") an approval so a later mutation can verify + consume
    /// it. Idempotent on an unconsumed `approval_id`; refuses to resurrect a
    /// consumed one.
    ///
    /// # Errors
    ///
    /// Returns `Err` on a backend failure.
    fn record_approval(
        &self,
        approval_id: &str,
        approver: &str,
        resource: &str,
    ) -> Result<(), String>;
}

impl ApprovalLedgerWriter for InMemoryApprovalLedger {
    fn record_approval(
        &self,
        approval_id: &str,
        approver: &str,
        resource: &str,
    ) -> Result<(), String> {
        self.record(approval_id, approver, resource)
    }
}

/// A single recorded approval.
#[derive(Clone, Debug)]
struct Entry {
    approver: String,
    resource: String,
    consumed: bool,
}

/// In-memory reference [`ApprovalLedger`]. Approvals are recorded via
/// [`record`](Self::record) (the "create approval" step) and spent by
/// [`verify_and_consume`](ApprovalLedger::verify_and_consume).
///
/// This is the dev/CI/test backend and the fail-CLOSED default: a freshly
/// constructed ledger has NO approvals, so every mutation is rejected until an
/// approval is independently recorded.
#[derive(Debug, Default)]
pub struct InMemoryApprovalLedger {
    entries: Mutex<HashMap<String, Entry>>,
}

impl InMemoryApprovalLedger {
    /// Construct an empty ledger (no approvals recorded ⇒ every mutation
    /// fails CLOSED until [`record`](Self::record) is called).
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record an approval: the "create approval" step. In production this is
    /// performed by an independently-authenticated approver endpoint; the
    /// `approver` here is the approver's authenticated identity, NEVER a
    /// client-body free-text value.
    ///
    /// Recording is idempotent on `approval_id` for an *unconsumed* entry
    /// (re-recording the same id with the same fields is a no-op). Recording
    /// over a **consumed** id is refused (`Err`) so a spent approval cannot be
    /// silently resurrected.
    ///
    /// # Errors
    ///
    /// Returns `Err` on lock poisoning or an attempt to re-record a consumed
    /// approval id.
    pub fn record(
        &self,
        approval_id: impl Into<String>,
        approver: impl Into<String>,
        resource: impl Into<String>,
    ) -> Result<(), String> {
        let approval_id = approval_id.into();
        let approver = approver.into();
        let resource = resource.into();
        let mut g = self
            .entries
            .lock()
            .map_err(|_| "approval ledger lock poisoned".to_owned())?;
        if let Some(existing) = g.get(&approval_id) {
            if existing.consumed {
                return Err(format!(
                    "approval_id={approval_id} already consumed; cannot re-record"
                ));
            }
        }
        g.insert(
            approval_id,
            Entry {
                approver,
                resource,
                consumed: false,
            },
        );
        Ok(())
    }
}

impl ApprovalLedger for InMemoryApprovalLedger {
    fn verify_and_consume(
        &self,
        approval_id: &str,
        initiator: &str,
        resource: &str,
    ) -> Result<VerifiedApproval, ApprovalRejection> {
        let mut g = self
            .entries
            .lock()
            .map_err(|_| ApprovalRejection::Backend("approval ledger lock poisoned".to_owned()))?;
        let entry = g.get_mut(approval_id).ok_or(ApprovalRejection::Unknown)?;
        // The authoritative approver is the LEDGER record, not the request
        // body. Self-approval is checked against it first (primary invariant).
        if entry.approver == initiator {
            return Err(ApprovalRejection::SelfApproval {
                approver: entry.approver.clone(),
            });
        }
        if entry.resource != resource {
            return Err(ApprovalRejection::ScopeMismatch);
        }
        if entry.consumed {
            return Err(ApprovalRejection::Consumed);
        }
        entry.consumed = true;
        Ok(VerifiedApproval {
            approver: entry.approver.clone(),
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn unknown_approval_is_rejected() {
        let l = InMemoryApprovalLedger::new();
        let r = l.verify_and_consume("ghost", "operator", "tenant:t1");
        assert_eq!(r, Err(ApprovalRejection::Unknown));
    }

    #[test]
    fn distinct_recorded_approval_is_accepted_and_consumed_once() {
        let l = InMemoryApprovalLedger::new();
        l.record("a1", "approver", "tenant:t1").expect("record");
        let ok = l
            .verify_and_consume("a1", "operator", "tenant:t1")
            .expect("accept");
        assert_eq!(ok.approver, "approver");
        // Replay is rejected — single use.
        let replay = l.verify_and_consume("a1", "operator", "tenant:t1");
        assert_eq!(replay, Err(ApprovalRejection::Consumed));
    }

    #[test]
    fn self_approval_by_recorded_identity_is_rejected() {
        let l = InMemoryApprovalLedger::new();
        // Recorded approver == the initiator that will try to spend it.
        l.record("a1", "operator", "tenant:t1").expect("record");
        let r = l.verify_and_consume("a1", "operator", "tenant:t1");
        assert_eq!(
            r,
            Err(ApprovalRejection::SelfApproval {
                approver: "operator".to_owned()
            })
        );
        // A reject must NOT consume: re-check still sees it unconsumed and
        // still rejects (never silently spent).
        let again = l.verify_and_consume("a1", "operator", "tenant:t1");
        assert_eq!(
            again,
            Err(ApprovalRejection::SelfApproval {
                approver: "operator".to_owned()
            })
        );
    }

    #[test]
    fn scope_mismatch_is_rejected() {
        let l = InMemoryApprovalLedger::new();
        l.record("a1", "approver", "tenant:t1").expect("record");
        let r = l.verify_and_consume("a1", "operator", "tenant:OTHER");
        assert_eq!(r, Err(ApprovalRejection::ScopeMismatch));
    }

    #[test]
    fn re_recording_a_consumed_approval_is_refused() {
        let l = InMemoryApprovalLedger::new();
        l.record("a1", "approver", "tenant:t1").expect("record");
        l.verify_and_consume("a1", "operator", "tenant:t1")
            .expect("accept");
        let err = l
            .record("a1", "approver", "tenant:t1")
            .expect_err("refused");
        assert!(err.contains("already consumed"));
    }
}
