//! Property-based tests: URL canonicalization is idempotent and stable.
//!
//! Per `specs/_proposals/adapters/brew.md` §8 Property row — 1k cases:
//! trailing slash / case / query-param stripping all produce the same
//! CAS digest. This file realizes that contract with `proptest`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items
)]

use corelink_adapter_host::brew::bottle::{canonical_bottle_path, cas_key_for};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 1024
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024)
}

/// Generate a "bottle-ish" path segment.
///
/// Lowercase ASCII alphanumeric plus a few path separators. Kept tiny
/// so 1k cases stays fast.
fn base_path_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("a".to_owned()),
            Just("b".to_owned()),
            Just("9".to_owned()),
            Just("homebrew".to_owned()),
            Just("core".to_owned()),
            Just("curl".to_owned()),
            Just("wget".to_owned()),
        ],
        1..6,
    )
    .prop_map(|parts| format!("/{}", parts.join("/")))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Trailing slash MUST NOT change the canonical key.
    #[test]
    fn trailing_slash_invariance(p in base_path_strategy()) {
        let with_slash = format!("{p}/");
        prop_assert_eq!(
            cas_key_for(&canonical_bottle_path(&p)),
            cas_key_for(&canonical_bottle_path(&with_slash))
        );
    }

    /// Case folding MUST NOT change the canonical key.
    #[test]
    fn case_invariance(p in base_path_strategy()) {
        let upper = p.to_ascii_uppercase();
        prop_assert_eq!(
            cas_key_for(&canonical_bottle_path(&p)),
            cas_key_for(&canonical_bottle_path(&upper))
        );
    }

    /// Trailing query-string MUST NOT change the canonical key.
    #[test]
    fn query_string_invariance(p in base_path_strategy()) {
        let with_query = format!("{p}?cdn=us-east&t=42");
        prop_assert_eq!(
            cas_key_for(&canonical_bottle_path(&p)),
            cas_key_for(&canonical_bottle_path(&with_query))
        );
    }

    /// Canonicalization MUST be idempotent.
    #[test]
    fn canonical_idempotent(p in base_path_strategy()) {
        let once = canonical_bottle_path(&p);
        let twice = canonical_bottle_path(&format!("/{once}"));
        prop_assert_eq!(once, twice);
    }

    /// Different inputs (when their canonical forms differ) MUST
    /// produce different CAS keys.
    #[test]
    fn distinct_canonical_paths_distinct_keys(
        p1 in base_path_strategy(),
        p2 in base_path_strategy(),
    ) {
        let c1 = canonical_bottle_path(&p1);
        let c2 = canonical_bottle_path(&p2);
        if c1 != c2 {
            prop_assert_ne!(cas_key_for(&c1), cas_key_for(&c2));
        }
    }
}
