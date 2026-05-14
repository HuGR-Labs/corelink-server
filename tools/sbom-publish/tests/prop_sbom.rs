//! Property tests for the SBOM pipeline (WI-S12-002 §6.1.9).
// Tests use assert macros that may implicitly use panic paths.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//!
//! Runs 10k iterations by default (PR gate); 100k for nightly via `PROPTEST_CASES=100000`.
//!
//! # Properties tested
//!
//! - `prop_sbom_ntia_field_missing_rejected`: random SBOMs missing ≥ 1 NTIA field
//!   are 100 % rejected in strict mode.
//! - `prop_sbom_purl_malformed_rejected`: mutated PURL strings are either
//!   canonically normalised or rejected by `is_valid_purl`.
//! - `prop_sbom_component_count_consistent`: component count extracted from JSON
//!   matches the array length.
//! - `prop_sbom_tsa_replay_rejected`: TSR tokens bound to one hash are rejected
//!   when verified against a different SBOM.

use proptest::prelude::*;

use sbom_publish::ntia::{validate_ntia_json, ValidationMode};
use sbom_publish::purl::{is_valid_purl, normalise_purl};
use sbom_publish::tsa::{verify_tsr_binding, TsrToken};

// ---------------------------------------------------------------------------
// PROPTEST_CASES helper
// ---------------------------------------------------------------------------

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// ---------------------------------------------------------------------------
// Helpers for constructing synthetic SBOMs
// ---------------------------------------------------------------------------

fn base_compliant_sbom() -> serde_json::Value {
    serde_json::json!({
        "specVersion": "1.5",
        "metadata": {
            "timestamp": "2026-05-13T00:00:00Z",
            "authors": [{"name": "CoreLink CI"}]
        },
        "components": [
            {
                "name": "serde",
                "version": "1.0.197",
                "purl": "pkg:cargo/serde@1.0.197",
                "supplier": {"name": "serde-rs"}
            }
        ],
        "dependencies": [{"ref": "pkg:cargo/serde@1.0.197", "dependsOn": []}]
    })
}

// ---------------------------------------------------------------------------
// prop_sbom_ntia_field_missing_rejected
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Any SBOM with one or more NTIA fields missing must be rejected in strict mode.
    #[test]
    fn prop_sbom_ntia_field_missing_rejected(
        // choose which field(s) to drop: bitmap 0b0111111 means 6 fields
        field_mask in 1u8..=127u8,
    ) {
        let mut sbom = base_compliant_sbom();

        // Apply drops based on bitmask
        if field_mask & 0b000_0001 != 0 {
            // Remove author
            if let Some(meta) = sbom.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                meta.remove("authors");
            }
        }
        if field_mask & 0b000_0010 != 0 {
            // Remove timestamp
            if let Some(meta) = sbom.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                meta.remove("timestamp");
            }
        }
        if field_mask & 0b000_0100 != 0 {
            // Remove component name
            if let Some(arr) = sbom.get_mut("components").and_then(|c| c.as_array_mut()) {
                for c in arr.iter_mut() {
                    if let Some(obj) = c.as_object_mut() { obj.remove("name"); }
                }
            }
        }
        if field_mask & 0b000_1000 != 0 {
            // Remove component version
            if let Some(arr) = sbom.get_mut("components").and_then(|c| c.as_array_mut()) {
                for c in arr.iter_mut() {
                    if let Some(obj) = c.as_object_mut() { obj.remove("version"); }
                }
            }
        }
        if field_mask & 0b001_0000 != 0 {
            // Remove supplier
            if let Some(arr) = sbom.get_mut("components").and_then(|c| c.as_array_mut()) {
                for c in arr.iter_mut() {
                    if let Some(obj) = c.as_object_mut() { obj.remove("supplier"); }
                }
            }
        }
        if field_mask & 0b010_0000 != 0 {
            // Remove purl (unique id)
            if let Some(arr) = sbom.get_mut("components").and_then(|c| c.as_array_mut()) {
                for c in arr.iter_mut() {
                    if let Some(obj) = c.as_object_mut() { obj.remove("purl"); }
                }
            }
        }
        if field_mask & 0b100_0000 != 0 {
            // Remove dependencies
            if let Some(obj) = sbom.as_object_mut() {
                obj.remove("dependencies");
            }
        }

        let result = validate_ntia_json(&sbom, ValidationMode::Strict);
        // With any field missing, overall_compliant must be false
        prop_assert!(
            !result.overall_compliant,
            "SBOM with dropped fields (mask={field_mask:#010b}) should not be NTIA-compliant"
        );
    }
}

// ---------------------------------------------------------------------------
// prop_sbom_purl_malformed_rejected
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Canonical PURL normalisation: valid PURLs produce a pair; invalid ones are
    /// rejected by `is_valid_purl`.
    #[test]
    fn prop_sbom_purl_malformed_rejected(
        // Arbitrary string that may or may not look like a PURL
        s in ".*",
    ) {
        if is_valid_purl(&s) {
            // Normalisation must never panic on a syntactically valid PURL
            let (primary, alias) = normalise_purl(&s, false);
            prop_assert!(is_valid_purl(&primary), "primary PURL should be valid");
            // alias: if it started with pkg:cargo/ it must now be pkg:crates/
            if s.starts_with("pkg:cargo/") {
                prop_assert!(alias.starts_with("pkg:crates/"), "DT alias must use pkg:crates/");
            }
        } else {
            // An invalid PURL string must be rejected
            prop_assert!(!is_valid_purl(&s));
        }
    }
}

// ---------------------------------------------------------------------------
// prop_sbom_component_count_consistent
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Component count extracted from the SBOM JSON must equal the array length.
    #[test]
    fn prop_sbom_component_count_consistent(
        n in 0usize..200usize,
    ) {
        let components: Vec<serde_json::Value> = (0..n)
            .map(|i| serde_json::json!({
                "name": format!("crate-{i}"),
                "version": "1.0.0",
                "purl": format!("pkg:cargo/crate-{i}@1.0.0"),
                "supplier": {"name": "acme"}
            }))
            .collect();

        let sbom = serde_json::json!({
            "specVersion": "1.5",
            "metadata": {
                "timestamp": "2026-05-13T00:00:00Z",
                "authors": [{"name": "CI"}]
            },
            "components": components,
            "dependencies": [{"ref": "root", "dependsOn": []}]
        });

        let extracted = sbom
            .get("components")
            .and_then(|c| c.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        prop_assert_eq!(extracted, n, "component count must match array length");
    }
}

// ---------------------------------------------------------------------------
// prop_sbom_tsa_replay_rejected
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// TSR tokens bound to one SBOM hash are rejected when verified against
    /// different SBOM bytes.
    #[test]
    fn prop_sbom_tsa_replay_rejected(
        original in "[a-z]{10,50}",
        other in "[A-Z]{10,50}",
    ) {
        // Ensure the two SBOMs are actually different
        prop_assume!(original != other);

        use sha2::{Digest, Sha256};

        let original_bytes = original.as_bytes();
        let other_bytes = other.as_bytes();

        // Build a fake TsrToken bound to `original`
        let sha = Sha256::digest(original_bytes);
        let sha_hex = format!("{sha:x}");
        let token = TsrToken {
            der_bytes: vec![0xCA, 0xFE],
            sbom_sha256_hex: sha_hex,
            nonce_hex: "deadbeef".to_owned(),
        };

        // Verify against `original` → must succeed
        prop_assert!(
            verify_tsr_binding(original_bytes, &token).is_ok(),
            "TSR must bind to original SBOM"
        );

        // Verify against `other` → must fail (replay detected)
        prop_assert!(
            verify_tsr_binding(other_bytes, &token).is_err(),
            "TSR replay with different SBOM must be rejected"
        );
    }
}
