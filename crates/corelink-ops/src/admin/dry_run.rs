//! `corelink-admin-dry-run` — shared logic for the WI-S13-006 dry-run
//! harnesses (RB-FM-201 / RB-FM-205 / RB-FM-206).
//!
//! The three binaries under `src/bin/` orchestrate cadence-based dry
//! runs of the dual-approval defense-in-depth. This library exposes the
//! load-bearing **pure predicates** that the binaries (and a property
//! test suite under `tests/`) consume, so the INV-ADMIN-DUAL-APPROVAL
//! invariant can be exercised by proptest without spinning up a full
//! `DualApprovalGateImpl`.
//!
//! # Invariants enforced
//!
//! - `INV-ADMIN-DUAL-APPROVAL` (CRITICAL): a single approver MUST never
//!   be sufficient; an approver MUST NOT be the same actor as the
//!   requester; two distinct approvers are required for high-risk admin
//!   operations.
//!
//! The full enforcement lives in `corelink-dual-approval`; this crate
//! exposes a thin shim that the dry-run binaries import to validate
//! candidate (caller, approver) tuples ahead of constructing a
//! `DualApprovalGateImpl`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use uuid::Uuid;

/// Outcome of pre-flight dual-approval validation.
///
/// Pure predicate over `(requester, approvers)`. The downstream
/// `DualApprovalGateImpl::verify` performs the full pipeline (signature,
/// nonce, MFA freshness, collusion rotation); this predicate is the
/// **structural** gate exercised by the proptests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DualApprovalPreflight {
    /// Two distinct approvers, neither equal to the requester.
    Accepted,
    /// Fewer than 2 approvers were provided.
    InsufficientApprovers {
        /// Number of approvers seen.
        n: usize,
    },
    /// A single approver appeared twice (`approver_a == approver_b`).
    DuplicateApprovers,
    /// One of the approvers is the requester (self-approval).
    SelfApproval,
}

/// Structural dual-approval check.
///
/// Returns [`DualApprovalPreflight::Accepted`] iff:
/// 1. Exactly 2 approver UUIDs are present (no fewer).
/// 2. The two approvers are distinct from each other.
/// 3. Neither approver equals the requester.
///
/// This is the structural backstop the dry-run harness applies BEFORE
/// constructing an `AdminOpRequest`. It exists so the property test
/// suite can pin INV-ADMIN-DUAL-APPROVAL without depending on the full
/// dual-approval pipeline (HMAC + nonce + collusion).
#[must_use]
pub fn dual_approval_preflight(requester: Uuid, approvers: &[Uuid]) -> DualApprovalPreflight {
    if approvers.len() < 2 {
        return DualApprovalPreflight::InsufficientApprovers { n: approvers.len() };
    }
    // We canonicalize on the first two approvers — additional entries
    // are ignored by the dry-run harness (it only ever passes 0..=2).
    let Some((approver_a, rest)) = approvers.split_first() else {
        return DualApprovalPreflight::InsufficientApprovers { n: 0 };
    };
    let Some(approver_b) = rest.first() else {
        return DualApprovalPreflight::InsufficientApprovers { n: 1 };
    };
    if approver_a == approver_b {
        return DualApprovalPreflight::DuplicateApprovers;
    }
    if approver_a == &requester || approver_b == &requester {
        return DualApprovalPreflight::SelfApproval;
    }
    DualApprovalPreflight::Accepted
}

/// True iff a dry-run plan should remain read-only — i.e. the dual
/// approval pre-flight rejected the candidate request.
///
/// The "read-only invariant" of the dry-run harness is that ANY rejection
/// outcome MUST short-circuit before any mutation could be attempted.
/// This helper is the canonical predicate that pins it.
#[must_use]
pub fn is_dry_run_short_circuit(p: DualApprovalPreflight) -> bool {
    !matches!(p, DualApprovalPreflight::Accepted)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    fn u(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn accepted_when_two_distinct_non_self() {
        let req = u(1);
        let a = u(2);
        let b = u(3);
        let p = dual_approval_preflight(req, &[a, b]);
        assert_eq!(p, DualApprovalPreflight::Accepted);
        assert!(!is_dry_run_short_circuit(p));
    }

    #[test]
    fn rejected_on_single_approver() {
        let req = u(1);
        let a = u(2);
        let p = dual_approval_preflight(req, &[a]);
        assert_eq!(p, DualApprovalPreflight::InsufficientApprovers { n: 1 });
        assert!(is_dry_run_short_circuit(p));
    }

    #[test]
    fn rejected_on_zero_approvers() {
        let req = u(1);
        let p = dual_approval_preflight(req, &[]);
        assert_eq!(p, DualApprovalPreflight::InsufficientApprovers { n: 0 });
    }

    #[test]
    fn rejected_on_duplicate_approvers() {
        let req = u(1);
        let a = u(2);
        let p = dual_approval_preflight(req, &[a, a]);
        assert_eq!(p, DualApprovalPreflight::DuplicateApprovers);
    }

    #[test]
    fn rejected_on_self_approval() {
        let req = u(1);
        let other = u(2);
        let p = dual_approval_preflight(req, &[req, other]);
        assert_eq!(p, DualApprovalPreflight::SelfApproval);
        let q = dual_approval_preflight(req, &[other, req]);
        assert_eq!(q, DualApprovalPreflight::SelfApproval);
    }
}
