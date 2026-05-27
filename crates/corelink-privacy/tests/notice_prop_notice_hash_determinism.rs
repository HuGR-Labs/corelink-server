//! Property test: notice_text_hash determinism across CRLF/LF/CR variants.
//!
//! AC-006: "property test: 100 random content samples × 3 platforms → 100%
//! hash parity".  In CI: `PROPTEST_CASES=10000 cargo test -p
//! corelink-privacy-notice-emit --test prop_notice_hash_determinism`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use corelink_privacy::notice::notice_text_hash;
use proptest::prelude::*;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// AC-006 property: hash of LF content == hash of CRLF content.
    #[test]
    fn hash_lf_eq_crlf(
        // Generate printable ASCII lines (avoid embedded \r\n in generator).
        lines in prop::collection::vec(
            "[a-zA-Z0-9 ,.:;!?()'\"-]{0,80}",
            1..=30,
        )
    ) {
        let lf_content = lines.join("\n");
        // Simulate Windows CRLF.
        let crlf_content = lines.join("\r\n");
        // Simulate old-Mac CR.
        let cr_content = lines.join("\r");

        let hash_lf = notice_text_hash(&lf_content);
        let hash_crlf = notice_text_hash(&crlf_content);
        let hash_cr = notice_text_hash(&cr_content);

        prop_assert_eq!(&hash_lf, &hash_crlf, "LF vs CRLF hash mismatch");
        prop_assert_eq!(&hash_lf, &hash_cr, "LF vs CR hash mismatch");
    }

    /// Trailing whitespace stripped: "line   \n" == "line\n".
    #[test]
    fn hash_trailing_whitespace_invariant(
        lines in prop::collection::vec(
            "[a-zA-Z0-9]{1,40}",
            1..=20,
        ),
        trailing_spaces in prop::collection::vec(0_usize..=6, 1..=20),
    ) {
        // Build a content with trailing spaces on each line.
        let with_trailing: String = lines.iter().enumerate().map(|(i, line)| {
            let spaces = trailing_spaces.get(i).copied().unwrap_or(0);
            format!("{}{}", line, " ".repeat(spaces))
        }).collect::<Vec<_>>().join("\n");

        let clean: String = lines.join("\n");

        let hash_with_trailing = notice_text_hash(&with_trailing);
        let hash_clean = notice_text_hash(&clean);

        prop_assert_eq!(hash_with_trailing, hash_clean, "trailing whitespace should not affect hash");
    }

    /// Hash is always 64 hex chars (SHA-256).
    #[test]
    fn hash_always_64_hex_chars(
        content in "[a-zA-Z0-9 \n\r.,!?-]{0,500}"
    ) {
        let h = notice_text_hash(&content);
        prop_assert_eq!(h.len(), 64, "expected 64 hex chars (SHA-256)");
        let all_hex = h.chars().all(|c| c.is_ascii_hexdigit());
        prop_assert!(all_hex, "hash must be lowercase hex");
    }

    /// Idempotent: same input → same hash every time.
    #[test]
    fn hash_idempotent(
        content in "[a-zA-Z0-9 \n.,!?-]{0,200}"
    ) {
        let h1 = notice_text_hash(&content);
        let h2 = notice_text_hash(&content);
        prop_assert_eq!(h1, h2, "hash must be idempotent");
    }
}
