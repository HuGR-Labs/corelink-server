//! Property tests for the OCI ↔ CAS digest bridge.
//!
//! Two invariants:
//!
//! 1. `parse(to_wire(d)) == d` — wire form round-trips losslessly.
//! 2. `compute(bytes).verify_against_bytes(bytes) == Ok` — every
//!    computed digest verifies against its source bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]

use corelink_adapter_host::oci::digest::{OciDigest, OciDigestAlgo};
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_to_wire_roundtrip_sha256(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let d = OciDigest::compute(OciDigestAlgo::Sha256, &bytes).expect("compute");
        let wire = d.to_wire();
        let parsed = OciDigest::parse(&wire).expect("parse");
        prop_assert_eq!(parsed.to_wire(), wire);
    }

    #[test]
    fn compute_then_verify_always_succeeds(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let d = OciDigest::compute(OciDigestAlgo::Sha256, &bytes).expect("compute");
        d.verify_against_bytes(&bytes).expect("verify ok");
    }

    #[test]
    fn verify_against_other_bytes_always_fails(
        a in proptest::collection::vec(any::<u8>(), 1..256),
        b in proptest::collection::vec(any::<u8>(), 1..256),
    ) {
        prop_assume!(a != b);
        let d = OciDigest::compute(OciDigestAlgo::Sha256, &a).expect("compute");
        prop_assert!(d.verify_against_bytes(&b).is_err());
    }
}
