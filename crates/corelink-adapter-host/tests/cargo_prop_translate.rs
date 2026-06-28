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

    // NOTE (corrected vs the pre-#507 contract): a non-64-hex key is NOT rejected.
    // sccache sends NON-hex CONTROL keys (e.g. `.sccache_check`), so normalize_key
    // ACCEPTS any single safe path segment of `[alnum._-]`, length 1..=256, that is
    // not `.`/`..`/contains `/`. The real rejection axes are: empty, >256, traversal,
    // and illegal characters — pinned below. (The old prop_short/long_key_rejected
    // asserted the pre-#507 "exactly 64 hex or reject" behaviour and were stale —
    // the flaky tests gate never caught the contradiction.)

    #[test]
    fn prop_short_alnum_key_accepted_as_control_key(len in 1usize..64) {
        // A short all-alnum key (a control-key shape) is ACCEPTED verbatim.
        let short: String = "a".repeat(len);
        let got = normalize_key(&short);
        prop_assert_eq!(got.as_deref(), Some(short.as_str()));
    }

    #[test]
    fn prop_midsize_alnum_key_accepted(extra in 1usize..(256 - DIGEST_HEX_LEN)) {
        // 65..=256 chars of `[alnum._-]` is a valid control key (not a 64-hex
        // object key) → accepted verbatim, no longer rejected for being "too long".
        let mid: String = "a".repeat(DIGEST_HEX_LEN + extra);
        let got = normalize_key(&mid);
        prop_assert_eq!(got.as_deref(), Some(mid.as_str()));
    }

    #[test]
    fn prop_oversize_key_rejected(extra in 1usize..64) {
        // >256 chars is the real length-rejection boundary.
        let long: String = "a".repeat(256 + extra);
        prop_assert!(normalize_key(&long).is_none());
    }

    #[test]
    fn prop_traversal_key_rejected(seg in "[a-z]{1,20}") {
        // Any key containing a path separator is rejected (no traversal).
        let with_slash = format!("{seg}/{seg}");
        prop_assert!(normalize_key(&with_slash).is_none());
        // The exact `.` and `..` segments are rejected too.
        prop_assert!(normalize_key(".").is_none());
        prop_assert!(normalize_key("..").is_none());
    }

    #[test]
    fn prop_illegal_char_key_rejected(prefix in "[a-z]{1,10}", bad in "[!@#$%^&*()+=:;,?<>|]", suffix in "[a-z]{1,10}") {
        // A non-whitespace character outside [alnum._-], embedded in the middle
        // (so it can't be trimmed away), rejects the whole key. (Whitespace is
        // excluded from `bad` because normalize_key trims it first.)
        let key = format!("{prefix}{bad}{suffix}");
        prop_assert!(normalize_key(&key).is_none());
    }
}
