//! Property tests for the transparency-log submission seam.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "property tests may use unwrap/expect/panic"
)]

use corelink_transparency_log::{witness_or_degrade, InMemoryRekor, SignedEntry, WitnessOutcome};
use proptest::prelude::*;

fn entry_strategy() -> impl Strategy<Value = SignedEntry> {
    (
        prop::collection::vec(any::<u8>(), 0..512),
        prop::collection::vec(any::<u8>(), 1..128),
        prop::collection::vec(any::<u8>(), 1..64),
    )
        .prop_map(|(payload, pubkey, sig)| SignedEntry::new(payload, "ed25519", pubkey, sig))
}

proptest! {
    /// The content digest is always a 64-char lowercase hex SHA-256.
    #[test]
    fn digest_is_always_64_char_lowercase_hex(entry in entry_strategy()) {
        let h = entry.content_sha256_hex();
        prop_assert_eq!(h.len(), 64);
        prop_assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    /// The Rekor body always serializes and never leaks raw payload bytes that
    /// differ from the digest representation.
    #[test]
    fn rekor_body_always_serializes_with_canonical_kind(entry in entry_strategy()) {
        let body = entry.to_rekor_hashedrekord();
        prop_assert_eq!(body.kind.as_str(), "hashedrekord");
        prop_assert_eq!(body.api_version.as_str(), "0.0.1");
        let json = body.to_wire_json().unwrap();
        prop_assert!(json.contains(&entry.content_sha256_hex()));
    }

    /// Witnessing N distinct entries against a fresh fake yields strictly
    /// increasing log indices 0..N and N submissions.
    #[test]
    fn witness_indices_are_monotonic(n in 1usize..16) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let rekor = InMemoryRekor::new();
            for i in 0..n {
                let e = SignedEntry::new(
                    format!("{i}").into_bytes(),
                    "ed25519",
                    vec![1],
                    vec![2],
                );
                let out = witness_or_degrade(&rekor, &e).await;
                match out {
                    WitnessOutcome::Witnessed(rec) => {
                        prop_assert_eq!(rec.log_index, i as u64);
                    }
                    WitnessOutcome::Degraded(r) => {
                        prop_assert!(false, "unexpected degrade: {}", r);
                    }
                }
            }
            prop_assert_eq!(rekor.submitted_count().unwrap(), n);
            Ok(())
        })?;
    }
}
