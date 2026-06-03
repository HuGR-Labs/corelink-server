//! Property tests on sccache key translation (spec §8 row 2).
//!
//! Invariants verified:
//!
//! 1. Any 64-char lowercase hex string roundtrips through `normalize_key`.
//! 2. `normalize_key` lowercases uppercase hex.
//! 3. `key_from_path` strips the leading `/` correctly.
//! 4. `normalize_key` is idempotent.
//! 5. Keys shorter / longer than 64 chars are rejected.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items
)]

#[path = "cargo_common.rs"]
mod common;

use corelink_adapter_host::cargo::translate::{key_from_path, normalize_key, DIGEST_HEX_LEN};
use proptest::prelude::*;

/// Generate a random 64-char lowercase hex string.
fn hex_key_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(b"0123456789abcdef"),
        DIGEST_HEX_LEN,
    )
    .prop_map(|v| v.iter().map(|b| *b as char).collect())
}

/// Generate a random 64-char mixed-case hex string.
fn mixed_case_hex_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(b"0123456789abcdefABCDEF"),
        DIGEST_HEX_LEN,
    )
    .prop_map(|v| v.iter().map(|b| *b as char).collect())
}

proptest! {
    #[test]
    fn prop_normalize_roundtrips_lowercase_hex(key in hex_key_strategy()) {
        let normalized = normalize_key(&key).expect("valid lowercase hex must normalize");
        prop_assert_eq!(&normalized, &key);
    }

    #[test]
    fn prop_normalize_lowercases_mixed_case(key in mixed_case_hex_strategy()) {
        let normalized = normalize_key(&key).expect("valid mixed-case hex must normalize");
        let expected = key.to_ascii_lowercase();
        prop_assert_eq!(normalized, expected);
    }

    #[test]
    fn prop_normalize_is_idempotent(key in hex_key_strategy()) {
        let once = normalize_key(&key).expect("first normalize");
        let twice = normalize_key(&once).expect("second normalize");
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn prop_key_from_path_with_slash(key in hex_key_strategy()) {
        let path = format!("/{key}");
        let result = key_from_path(&path).expect("key_from_path with leading slash");
        prop_assert_eq!(result, key);
    }

    #[test]
    fn prop_key_from_path_without_slash(key in hex_key_strategy()) {
        let result = key_from_path(&key).expect("key_from_path without leading slash");
        prop_assert_eq!(result, key);
    }

    #[test]
    fn prop_short_key_rejected(len in 0usize..64) {
        let short: String = "a".repeat(len);
        prop_assert!(normalize_key(&short).is_none());
    }

    #[test]
    fn prop_long_key_rejected(extra in 1usize..32) {
        let long: String = "a".repeat(DIGEST_HEX_LEN + extra);
        prop_assert!(normalize_key(&long).is_none());
    }
}
