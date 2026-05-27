#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
//! Property tests for the PEP 691 JSON ↔ PEP 503 HTML conversion.
//!
//! Per spec §8 row 2: round-trip + yank propagation invariants over
//! 1k generated cases.

use corelink_adapter_host::pip::pep503_html::{
    encode_html, is_valid_sha256_hex, normalise_project_name, parse_html, IndexFile,
    ProjectIndex,
};
use proptest::prelude::*;

fn sha256_hex_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<u8>(), 32).prop_map(hex::encode)
}

fn filename_strategy() -> impl Strategy<Value = String> {
    // PEP 491 wheel-name grammar uses `[a-zA-Z0-9_-]` for the
    // distribution + `.` + digits for the version. Generate
    // alphanumeric with `-_.` mixed in.
    "[a-zA-Z][a-zA-Z0-9_.-]{0,30}".prop_map(|s| s)
}

fn project_name_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_.-]{0,15}".prop_map(|s| s)
}

fn index_file_strategy() -> impl Strategy<Value = IndexFile> {
    (
        filename_strategy(),
        sha256_hex_strategy(),
        proptest::option::of("[<>=!,. 0-9a-zA-Z]{1,15}".prop_map(|s| s)),
        any::<bool>(),
    )
        .prop_map(|(filename, sha256, requires_python, yanked)| {
            let url = format!(
                "https://files.pythonhosted.org/packages/{filename}#sha256={sha256}"
            );
            IndexFile::new(filename, url, sha256, requires_python, yanked)
        })
}

fn project_index_strategy() -> impl Strategy<Value = ProjectIndex> {
    (
        project_name_strategy(),
        proptest::collection::vec(index_file_strategy(), 1..=6),
    )
        .prop_map(|(name, files)| ProjectIndex::new(normalise_project_name(&name), files))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, .. ProptestConfig::default() })]

    /// HTML encode → HTML parse must yield an equivalent
    /// `ProjectIndex` (after normalising the project name; we don't
    /// transport it in the HTML, so the parser takes the name as a
    /// caller arg).
    #[test]
    fn html_roundtrip(index in project_index_strategy()) {
        let html = encode_html(&index);
        let parsed = parse_html(&html, &index.name).expect("parse must succeed");
        prop_assert_eq!(&parsed.name, &index.name);
        prop_assert_eq!(parsed.files.len(), index.files.len());
        for (a, b) in parsed.files.iter().zip(index.files.iter()) {
            prop_assert_eq!(&a.filename, &b.filename);
            prop_assert_eq!(&a.url, &b.url);
            prop_assert_eq!(a.sha256.to_ascii_lowercase(), b.sha256.to_ascii_lowercase());
            prop_assert_eq!(a.yanked, b.yanked);
        }
    }

    /// PEP 503 project-name normalisation is idempotent.
    #[test]
    fn normalisation_idempotent(name in project_name_strategy()) {
        let once = normalise_project_name(&name);
        let twice = normalise_project_name(&once);
        prop_assert_eq!(once, twice);
    }

    /// SHA256 hex validator accepts every output of
    /// [`sha256_hex_strategy`].
    #[test]
    fn sha256_validator_accepts_strategy_output(sha in sha256_hex_strategy()) {
        prop_assert!(is_valid_sha256_hex(&sha));
    }

    /// Yank status survives HTML round-trip per file.
    #[test]
    fn yank_status_propagates(yanked in any::<bool>(), sha in sha256_hex_strategy()) {
        let index = ProjectIndex::new(
            "x".into(),
            vec![IndexFile::new(
                "x-1.tar.gz".into(),
                format!("https://files.pythonhosted.org/x-1.tar.gz#sha256={sha}"),
                sha.clone(),
                None,
                yanked,
            )],
        );
        let html = encode_html(&index);
        let parsed = parse_html(&html, "x").expect("parse");
        prop_assert_eq!(parsed.files[0].yanked, yanked);
    }
}
