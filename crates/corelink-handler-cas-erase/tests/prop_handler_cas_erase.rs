//! Property tests for `corelink-handler-cas-erase` pure logic.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_handler_cas_erase::{
    erase_outcome, prepare_erase, read_gate, validate_digest, CasEraseError, CasEraseRequest,
    EraseOutcome, ReadGate,
};
use proptest::prelude::*;

proptest! {
    /// A digest built only from the hash-safe alphabet (within the length
    /// bound) always validates; the validator never panics on any input.
    #[test]
    fn safe_digests_validate(s in "[A-Za-z0-9_-]{1,128}") {
        prop_assert!(validate_digest(&s).is_ok());
    }

    /// Any digest containing a '/' is rejected — the property that guarantees
    /// an erase key can never widen an R2 LIST/DELETE prefix.
    #[test]
    fn slash_digests_always_rejected(prefix in ".{0,40}", suffix in ".{0,40}") {
        let d = format!("{prefix}/{suffix}");
        prop_assert!(
            matches!(validate_digest(&d), Err(CasEraseError::InvalidDigest { .. })),
            "slash digest must be rejected as InvalidDigest"
        );
    }

    /// `prepare_erase` denies cross-tenant whenever the two tenants differ,
    /// regardless of digest validity (cross-tenant is the first gate).
    #[test]
    fn cross_tenant_always_denied(a in "[a-z]{1,8}", b in "[a-z]{1,8}", d in ".{0,20}") {
        prop_assume!(a != b);
        let req = CasEraseRequest::new(a, b, d);
        prop_assert!(
            matches!(prepare_erase(&req, "reason"), Err(CasEraseError::CrossTenantDenied { .. })),
            "differing tenants must be CrossTenantDenied"
        );
    }

    /// The read gate is `Gone` iff a tombstone is present — total + exact.
    #[test]
    fn read_gate_total(present: bool) {
        let g = read_gate(present);
        if present {
            prop_assert_eq!(g, ReadGate::Gone);
        } else {
            prop_assert_eq!(g, ReadGate::Proceed);
        }
    }

    /// Re-erase idempotency: an already-tombstoned hash reports `AlreadyErased`.
    #[test]
    fn outcome_tracks_prior_tombstone(prior: bool) {
        let o = erase_outcome(prior);
        if prior {
            prop_assert_eq!(o, EraseOutcome::AlreadyErased);
        } else {
            prop_assert_eq!(o, EraseOutcome::Erased);
        }
    }
}
