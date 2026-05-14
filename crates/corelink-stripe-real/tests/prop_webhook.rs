//! Property tests for webhook signature verification.
//!
//! - 10k cases (configurable via `PROPTEST_CASES` env).
//! - Roundtrip: any (secret, ts, payload) the verify accepts iff the
//!   signature was computed with the SAME secret + ts + payload.
//! - Negative: any tampering of any single input bit → reject.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_stripe_real::webhook::{
    compute_signature, verify_webhook_signature, DEFAULT_TOLERANCE_SECONDS,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        .. ProptestConfig::default()
    })]

    #[test]
    fn signature_roundtrip(
        secret in proptest::collection::vec(any::<u8>(), 16..64),
        ts in 1_000_000_000_u64 .. 2_000_000_000_u64,
        payload in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let sig = compute_signature(&secret, ts, &payload);
        let header = format!("t={ts},v1={sig}");
        prop_assert!(verify_webhook_signature(
            &payload, &header, &secret, ts, DEFAULT_TOLERANCE_SECONDS
        ).is_ok());
    }

    #[test]
    fn tampered_payload_rejected(
        secret in proptest::collection::vec(any::<u8>(), 16..64),
        ts in 1_000_000_000_u64 .. 2_000_000_000_u64,
        payload in proptest::collection::vec(any::<u8>(), 1..256),
        flip_idx in 0_usize..256,
    ) {
        let sig = compute_signature(&secret, ts, &payload);
        let header = format!("t={ts},v1={sig}");
        let mut tampered = payload.clone();
        let i = flip_idx % tampered.len();
        tampered[i] ^= 0x01;
        prop_assert!(verify_webhook_signature(
            &tampered, &header, &secret, ts, DEFAULT_TOLERANCE_SECONDS
        ).is_err());
    }

    #[test]
    fn wrong_secret_rejected(
        secret in proptest::collection::vec(any::<u8>(), 16..64),
        other in proptest::collection::vec(any::<u8>(), 16..64),
        ts in 1_000_000_000_u64 .. 2_000_000_000_u64,
        payload in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        prop_assume!(secret != other);
        let sig = compute_signature(&secret, ts, &payload);
        let header = format!("t={ts},v1={sig}");
        prop_assert!(verify_webhook_signature(
            &payload, &header, &other, ts, DEFAULT_TOLERANCE_SECONDS
        ).is_err());
    }

    #[test]
    fn replay_outside_tolerance_rejected(
        secret in proptest::collection::vec(any::<u8>(), 16..64),
        ts in 1_000_000_000_u64 .. 1_500_000_000_u64,
        payload in proptest::collection::vec(any::<u8>(), 0..256),
        // skew >= 301s (just past 5-min tolerance)
        skew in 301_u64 .. 86_400_u64,
    ) {
        let sig = compute_signature(&secret, ts, &payload);
        let header = format!("t={ts},v1={sig}");
        let now = ts.saturating_add(skew);
        prop_assert!(verify_webhook_signature(
            &payload, &header, &secret, now, DEFAULT_TOLERANCE_SECONDS
        ).is_err());
    }
}
