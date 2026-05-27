#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
//! Property tests on metadata cache TTL logic.
//!
//! Exercises [`corelink_adapter_host::npm::metadata::is_fresh`] across the
//! freshness boundary and verifies that the cache key normalisation
//! is stable.

use corelink_adapter_host::npm::metadata::{is_fresh, kv_key_for_pkg, normalise_pkg_name};
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_is_fresh_within_ttl(
        // inserted somewhere in 0..1_000_000 ms ago
        inserted_ms in 0u64..1_000_000u64,
        ttl_s in 1u64..3600u64,
    ) {
        let now_ms = inserted_ms + ttl_s * 1000 / 2; // halfway through TTL
        prop_assert!(is_fresh(inserted_ms, now_ms, ttl_s));
    }

    #[test]
    fn prop_is_stale_past_ttl(
        inserted_ms in 0u64..1_000_000u64,
        ttl_s in 1u64..3600u64,
    ) {
        let now_ms = inserted_ms + ttl_s * 1000 + 1; // 1ms past expiry
        prop_assert!(!is_fresh(inserted_ms, now_ms, ttl_s));
    }

    #[test]
    fn prop_kv_key_is_stable(pkg in "[a-z][a-z0-9-]{0,30}") {
        let k1 = kv_key_for_pkg(&pkg);
        let k2 = kv_key_for_pkg(&pkg);
        prop_assert_eq!(k1, k2);
    }

    #[test]
    fn prop_normalise_pkg_name_is_idempotent(pkg in "[a-z][a-z0-9-]{0,30}") {
        let n1 = normalise_pkg_name(&pkg);
        let n2 = normalise_pkg_name(&n1);
        prop_assert_eq!(n1, n2);
    }
}

// Unit sanity tests for edge cases proptest won't necessarily cover.

#[test]
fn is_fresh_zero_ttl_always_stale() {
    // ttl_seconds=0 means any positive elapsed time is stale.
    assert!(!is_fresh(0, 1, 0));
}

#[test]
fn kv_key_prefix_is_npm_meta() {
    let key = kv_key_for_pkg("lodash");
    assert!(key.starts_with("npm:meta:"));
}
