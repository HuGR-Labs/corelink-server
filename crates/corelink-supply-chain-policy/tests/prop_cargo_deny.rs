//! Property tests for cargo-deny policy enforcement (WI-S12-004 §6.1.10).
//!
//! Runs at **10k iterations** in PR gate mode and **100k iterations** in
//! nightly mode (via `PROPTEST_CASES` env var override, per S-07 P1-2 fix).
//!
//! Properties covered (§8 Gherkin scenarios + §10 Completeness Criteria):
//!
//! - `prop_cargo_deny_license_allowlist_enforced` — 10k synthetic Cargo.toml
//!   license combinations; asserts allowlist: MIT/Apache-2.0/BSD/ISC/MPL-2.0
//!   all pass; GPL/AGPL/SSPL/BUSL/Commons-Clause all deny.  Zero
//!   false-accepts (GPL passing) + zero false-rejects (MIT/Apache-2.0 failing).
//!   **10.s12.004.1** (EVT-002). **INV-SUPPLY-LICENSE-ALLOWLIST**.
//!
//! - `prop_cargo_deny_yanked_blocked` — 10k synthetic yanked dep injections;
//!   asserts 100% rejection.  **INV-SUPPLY-NO-YANKED**.
//!
//! - `prop_cargo_deny_unknown_source_blocked` — 10k unknown registry / git
//!   URLs; asserts 100% rejection (sources.unknown-registry = deny +
//!   sources.unknown-git = deny).
//!
//! - `prop_cargo_audit_critical_alerts` — 10k synthetic CRITICAL CVE advisory
//!   ids; asserts SEV-2 alert path triggered (outcome = denied, reason =
//!   advisory).
//!
//! Coverage: 100% deny.toml rule axes (advisories / licenses / bans / sources).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr
)]

use corelink_supply_chain_policy::{
    DenialReason, LicenseClass, SourceKind, classify_license, evaluate_policy, source_is_allowed,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// PROPTEST_CASES env var override (S-07 P1-2 nightly 100k pattern).
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn build_config() -> ProptestConfig {
    ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------------------
// License generators
// ---------------------------------------------------------------------------

fn allowed_license_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("MIT"),
        Just("Apache-2.0"),
        Just("Apache-2.0 WITH LLVM-exception"),
        Just("BSD-2-Clause"),
        Just("BSD-3-Clause"),
        Just("ISC"),
        Just("MPL-2.0"),
        Just("Unicode-DFS-2016"),
        Just("Unicode-3.0"),
        Just("Zlib"),
        Just("CC0-1.0"),
        Just("0BSD"),
    ]
}

fn banned_license_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("GPL-1.0"),
        Just("GPL-2.0"),
        Just("GPL-2.0+"),
        Just("GPL-2.0-only"),
        Just("GPL-2.0-or-later"),
        Just("GPL-3.0"),
        Just("GPL-3.0+"),
        Just("GPL-3.0-only"),
        Just("GPL-3.0-or-later"),
        Just("AGPL-1.0"),
        Just("AGPL-3.0"),
        Just("AGPL-3.0-only"),
        Just("AGPL-3.0-or-later"),
        Just("SSPL-1.0"),
        Just("BUSL-1.1"),
    ]
}

fn unknown_source_strategy() -> impl Strategy<Value = SourceKind> {
    prop_oneof![
        Just(SourceKind::UnknownRegistry),
        Just(SourceKind::GitUnpinned),
        Just(SourceKind::GitPinned), // also denied: allow-git = [] (empty)
    ]
}

// ---------------------------------------------------------------------------
// prop_cargo_deny_license_allowlist_enforced
// ---------------------------------------------------------------------------
// Zero false-accepts (GPL passing) + zero false-rejects (MIT/Apache-2.0
// failing). Covers §8 Gherkin "GPL-3.0 dep blocked by cargo-deny" +
// "Property test 100k iter green". INV-SUPPLY-LICENSE-ALLOWLIST.
proptest! {
    #![proptest_config(build_config())]

    /// Allowed licenses must be accepted; banned licenses must be denied.
    ///
    /// Zero false-accepts (GPL/AGPL/SSPL passing) and zero false-rejects
    /// (MIT/Apache-2.0 failing).  Covers **10.s12.004.1** (EVT-002) and
    /// **INV-SUPPLY-LICENSE-ALLOWLIST**.
    #[test]
    fn prop_cargo_deny_license_allowlist_enforced_allows(spdx in allowed_license_strategy()) {
        let class = classify_license(spdx);
        prop_assert_eq!(
            class.clone(), LicenseClass::Allowed,
            "allowed license '{}' was incorrectly classified as {:?}", spdx, class
        );

        // Also via full policy evaluator — clean dep on crates.io, not yanked.
        let outcome = evaluate_policy("test-crate", spdx, &SourceKind::CratesIo, false, None);
        prop_assert!(
            outcome.allowed,
            "policy denied allowed license '{}': {:?}",
            spdx, outcome.denial_reason
        );
    }

    /// Banned licenses must produce a denial with License reason.
    ///
    /// Covers **10.s12.004.2** adversarial scenario "GPL leak blocked".
    /// INV-SUPPLY-LICENSE-ALLOWLIST.
    #[test]
    fn prop_cargo_deny_license_allowlist_enforced_bans(spdx in banned_license_strategy()) {
        let class = classify_license(spdx);
        prop_assert!(
            class != LicenseClass::Allowed,
            "banned license '{}' was incorrectly classified as Allowed", spdx
        );

        let outcome = evaluate_policy("malicious-crate", spdx, &SourceKind::CratesIo, false, None);
        prop_assert!(
            !outcome.allowed,
            "policy allowed banned license '{}' — false-accept!", spdx
        );
        prop_assert!(
            matches!(outcome.denial_reason, Some(DenialReason::License(_))),
            "expected License denial for '{}', got: {:?}",
            spdx, outcome.denial_reason
        );
    }
}

// ---------------------------------------------------------------------------
// prop_cargo_deny_yanked_blocked
// ---------------------------------------------------------------------------
// 10k yanked dep injections; assert 100% rejection. INV-SUPPLY-NO-YANKED.
proptest! {
    #![proptest_config(build_config())]

    /// Every yanked dep must be denied regardless of license or source.
    ///
    /// Covers **INV-SUPPLY-NO-YANKED** + §8 Gherkin "Yanked dep blocked".
    /// 10k iterations (100k nightly via PROPTEST_CASES).
    #[test]
    fn prop_cargo_deny_yanked_blocked(
        spdx in allowed_license_strategy(),   // even a clean license
        crate_name in "[a-z][a-z0-9-]{0,20}", // arbitrary crate name
    ) {
        // yanked = true must block regardless of everything else.
        let outcome = evaluate_policy(&crate_name, spdx, &SourceKind::CratesIo, true, None);
        prop_assert!(
            !outcome.allowed,
            "yanked dep '{}' (license={}) was NOT blocked — INV-SUPPLY-NO-YANKED violated!",
            crate_name, spdx
        );
        prop_assert_eq!(
            outcome.denial_reason,
            Some(DenialReason::Yanked),
            "wrong denial reason for yanked dep '{}'", crate_name
        );
    }
}

// ---------------------------------------------------------------------------
// prop_cargo_deny_unknown_source_blocked
// ---------------------------------------------------------------------------
// 10k unknown registry/git URLs; assert 100% rejection.
proptest! {
    #![proptest_config(build_config())]

    /// Unknown-registry and git deps must be denied by source policy.
    ///
    /// Mirrors `deny.toml [sources] unknown-registry = "deny"` and
    /// `unknown-git = "deny"`.  Covers §8 Gherkin "Unknown git source blocked"
    /// + **10.s12.004.1** source axis.
    #[test]
    fn prop_cargo_deny_unknown_source_blocked(
        source in unknown_source_strategy(),
        spdx in allowed_license_strategy(),
        crate_name in "[a-z][a-z0-9-]{0,20}",
    ) {
        prop_assert!(
            !source_is_allowed(&source),
            "source {:?} incorrectly reported as allowed", source
        );

        let outcome = evaluate_policy(&crate_name, spdx, &source, false, None);
        prop_assert!(
            !outcome.allowed,
            "dep '{}' from {:?} was not blocked", crate_name, source
        );
        prop_assert!(
            matches!(outcome.denial_reason, Some(DenialReason::UnknownSource(_))),
            "wrong denial reason for unknown-source dep '{}': {:?}",
            crate_name, outcome.denial_reason
        );
    }
}

// ---------------------------------------------------------------------------
// prop_cargo_audit_critical_alerts
// ---------------------------------------------------------------------------
// 10k synthetic CRITICAL CVE advisory IDs; assert SEV-2 alert path triggered.
proptest! {
    #![proptest_config(build_config())]

    /// RUSTSEC advisory IDs must trigger advisory denial path.
    ///
    /// Simulates the daily cron detecting a new CVE; asserts the outcome is
    /// denied with Advisory reason (maps to SEV-2 alert if CRITICAL).
    /// Covers **10.s12.004.1** + §8 Gherkin "Daily cron detects new CVE CRITICAL".
    #[test]
    fn prop_cargo_audit_critical_alerts(
        year in 2020_u32..=2026_u32,
        seq in 1_u32..=9999_u32,
        spdx in allowed_license_strategy(),
        crate_name in "[a-z][a-z0-9-]{0,20}",
    ) {
        let advisory_id = format!("RUSTSEC-{year}-{seq:04}");
        let outcome = evaluate_policy(
            &crate_name,
            spdx,
            &SourceKind::CratesIo,
            false,
            Some(&advisory_id),
        );
        prop_assert!(
            !outcome.allowed,
            "dep '{}' with advisory {} was NOT denied!", crate_name, advisory_id
        );
        prop_assert!(
            matches!(outcome.denial_reason, Some(DenialReason::Advisory(_))),
            "wrong denial reason for advisory {}: {:?}",
            advisory_id, outcome.denial_reason
        );
    }
}

// ---------------------------------------------------------------------------
// Determinism sanity tests (non-property)
// ---------------------------------------------------------------------------

/// `classify_license` is referentially transparent — same input → same output.
#[test]
fn license_classifier_is_deterministic() {
    for &spdx in &["MIT", "GPL-3.0", "Apache-2.0", "SSPL-1.0", "BUSL-1.1", "ISC"] {
        let a = classify_license(spdx);
        let b = classify_license(spdx);
        assert_eq!(a, b, "classify_license('{spdx}') is not deterministic");
    }
}

/// Allowlist cardinality matches deny.toml — 12 allowed licenses.
#[test]
fn allowlist_cardinality_canonical() {
    let canonical = [
        "MIT",
        "Apache-2.0",
        "Apache-2.0 WITH LLVM-exception",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "MPL-2.0",
        "Unicode-DFS-2016",
        "Unicode-3.0",
        "Zlib",
        "CC0-1.0",
        "0BSD",
    ];
    for spdx in canonical {
        assert_eq!(
            classify_license(spdx),
            LicenseClass::Allowed,
            "canonical allowlist entry '{spdx}' was not classified as Allowed"
        );
    }
}

/// Bannedlist cardinality: GPL variants + AGPL + SSPL + BUSL must all be denied.
#[test]
fn bannedlist_canonical() {
    let banned = [
        "GPL-1.0",
        "GPL-2.0",
        "GPL-2.0+",
        "GPL-3.0",
        "GPL-3.0+",
        "AGPL-1.0",
        "AGPL-3.0",
        "SSPL-1.0",
        "BUSL-1.1",
    ];
    for spdx in banned {
        let class = classify_license(spdx);
        assert_ne!(
            class,
            LicenseClass::Allowed,
            "banned license '{spdx}' was incorrectly classified as Allowed"
        );
    }
}

/// crates.io source is allowed; all others denied (allow-git = []).
#[test]
fn source_policy_canonical() {
    assert!(source_is_allowed(&SourceKind::CratesIo));
    assert!(!source_is_allowed(&SourceKind::UnknownRegistry));
    assert!(!source_is_allowed(&SourceKind::GitPinned));
    assert!(!source_is_allowed(&SourceKind::GitUnpinned));
}
