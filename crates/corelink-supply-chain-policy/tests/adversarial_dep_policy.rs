//! Adversarial regression tests for dep policy (WI-S12-004 §6.1.11 + §15).
//!
//! These tests encode the 5 adversarial scenarios required by
//! **10.s12.004.2** (EVT-040) and verify that the deny.toml policy
//! (modelled by `corelink-supply-chain-policy`) blocks each attack vector:
//!
//! 1. **GPL-3.0 dep introduced** → `cargo-deny` blocks (INV-SUPPLY-LICENSE-ALLOWLIST).
//! 2. **Yanked dep introduced via Dependabot auto-merge** → `cargo-deny` blocks
//!    via CI gate (INV-SUPPLY-NO-YANKED).
//! 3. **Typosquat `corelink-fake` em crates.io** → lockfile diff comment catches;
//!    unknown-registry (non-crates.io) variant blocked by sources policy.
//! 4. **Unmaintained dep** → cargo-deny `unmaintained = "warn"` + quarterly review
//!    trigger; policy outcome is WARN, not auto-deny (documented intentional).
//! 5. **`[patch.crates-io]` sem ADR** → pre-merge check fails; vendored patch
//!    without explicit allowlist treated as unknown-git source.
//!
//! Each test is a regression guard: if the policy evaluator allows an
//! adversarial input, the test fails with an actionable message.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr
)]

use corelink_supply_chain_policy::{DenialReason, SourceKind, evaluate_policy};

// ---------------------------------------------------------------------------
// Scenario 1: GPL-3.0 dep introduced
// ---------------------------------------------------------------------------

/// Red-team scenario: developer adds a dep with `license = "GPL-3.0"`.
/// cargo-deny MUST block the PR. INV-SUPPLY-LICENSE-ALLOWLIST.
///
/// Ref: §15 Chaos Experiment 1 + §8 Gherkin "GPL-3.0 dep blocked".
#[test]
fn scenario_01_gpl3_dep_introduced_is_blocked() {
    let gpl_licenses = [
        "GPL-1.0",
        "GPL-2.0",
        "GPL-2.0+",
        "GPL-2.0-only",
        "GPL-2.0-or-later",
        "GPL-3.0",
        "GPL-3.0+",
        "GPL-3.0-only",
        "GPL-3.0-or-later",
    ];
    for spdx in gpl_licenses {
        let outcome = evaluate_policy("adversarial-gpl-crate", spdx, &SourceKind::CratesIo, false, None);
        assert!(
            !outcome.allowed,
            "GPL license '{spdx}' was NOT blocked — INV-SUPPLY-LICENSE-ALLOWLIST violated! \
             cargo-deny would have allowed a GPL dep into the workspace."
        );
        assert!(
            matches!(outcome.denial_reason, Some(DenialReason::License(_))),
            "Wrong denial reason for GPL '{spdx}': expected License, got {:?}",
            outcome.denial_reason
        );
    }
}

/// AGPL and SSPL (network copyleft) are also blocked.
///
/// Particularly important for SaaS: AGPL-3.0 triggers copyleft at network use.
#[test]
fn scenario_01_agpl_sspl_blocked() {
    for spdx in ["AGPL-1.0", "AGPL-3.0", "AGPL-3.0-only", "AGPL-3.0-or-later", "SSPL-1.0"] {
        let outcome = evaluate_policy("adversarial-agpl-crate", spdx, &SourceKind::CratesIo, false, None);
        assert!(
            !outcome.allowed,
            "'{spdx}' was NOT blocked — network copyleft dep slipped through!"
        );
    }
}

/// Commons-Clause + BUSL-1.1 (commercial restriction) are blocked.
#[test]
fn scenario_01_commercial_restriction_blocked() {
    for spdx in ["BUSL-1.1", "Commons-Clause"] {
        let outcome = evaluate_policy("adversarial-commercial-crate", spdx, &SourceKind::CratesIo, false, None);
        assert!(
            !outcome.allowed,
            "'{spdx}' (commercial restriction) was NOT blocked — legal exposure!"
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 2: Yanked dep introduced via Dependabot auto-merge
// ---------------------------------------------------------------------------

/// Red-team scenario: Dependabot proposes a minor patch, CI runs cargo-deny,
/// dep is yanked on crates.io. CI MUST fail. INV-SUPPLY-NO-YANKED.
///
/// Ref: §15 Chaos Experiment 2 + §28 R-007 risk register.
#[test]
fn scenario_02_yanked_dep_blocks_auto_merge() {
    // Even a fully allowed license with a clean advisory record must be blocked
    // when the dep is yanked.
    let outcome = evaluate_policy(
        "yanked-patch-crate",
        "MIT",         // allowed license
        &SourceKind::CratesIo,
        true,          // yanked!
        None,          // no RUSTSEC advisory
    );

    assert!(
        !outcome.allowed,
        "Yanked dep was NOT blocked — Dependabot auto-merge would have introduced \
         a yanked dep! INV-SUPPLY-NO-YANKED violated."
    );
    assert_eq!(
        outcome.denial_reason,
        Some(DenialReason::Yanked),
        "Expected Yanked denial reason, got {:?}",
        outcome.denial_reason
    );
}

/// Yanked takes precedence over advisory — both are blockers but yanked
/// fires first in the evaluation order (consistent with cargo-deny behaviour).
#[test]
fn scenario_02_yanked_precedence_over_advisory() {
    let outcome = evaluate_policy(
        "yanked-and-vulnerable",
        "MIT",
        &SourceKind::CratesIo,
        true,                          // yanked
        Some("RUSTSEC-2024-0001"),     // also has an advisory
    );
    assert!(!outcome.allowed);
    assert_eq!(outcome.denial_reason, Some(DenialReason::Yanked));
}

// ---------------------------------------------------------------------------
// Scenario 3: Typosquat `corelink-fake`
// ---------------------------------------------------------------------------

/// Red-team scenario: attacker publishes `corelink-fake` on a non-crates.io
/// registry; developer adds it manually.  cargo-deny blocks via sources policy.
///
/// Note: if published on crates.io proper, detection falls to lockfile diff
/// PR comment (mandatory Action) + pair review on new dep additions (§2 §6.1.6).
/// The registry-level block is the automated gate.
///
/// Ref: §2 + §15 Chaos Experiment 3.
#[test]
fn scenario_03_typosquat_unknown_registry_blocked() {
    let outcome = evaluate_policy(
        "corelink-fake",
        "MIT",
        &SourceKind::UnknownRegistry, // non-crates.io registry
        false,
        None,
    );
    assert!(
        !outcome.allowed,
        "Typosquat crate from unknown registry was NOT blocked — sources policy failure!"
    );
    assert!(
        matches!(outcome.denial_reason, Some(DenialReason::UnknownSource(SourceKind::UnknownRegistry))),
        "Expected UnknownSource(UnknownRegistry), got {:?}",
        outcome.denial_reason
    );
}

/// Git dep to an unknown URL (non-allowlisted) is also blocked.
/// Covers git-based typosquat vector.
#[test]
fn scenario_03_typosquat_git_unpinned_blocked() {
    let outcome = evaluate_policy(
        "corelink-server-utils",
        "MIT",
        &SourceKind::GitUnpinned, // git without commit hash
        false,
        None,
    );
    assert!(!outcome.allowed, "Git-unpinned dep was not blocked");
    assert!(
        matches!(outcome.denial_reason, Some(DenialReason::UnknownSource(SourceKind::GitUnpinned))),
        "Expected UnknownSource(GitUnpinned), got {:?}",
        outcome.denial_reason
    );
}

/// Even a SHA-pinned git dep is blocked (allow-git = [] in canonical config).
///
/// To allowlist a git dep, an ADR must add the URL to `allow-git`.
#[test]
fn scenario_03_git_pinned_blocked_when_not_allowlisted() {
    let outcome = evaluate_policy(
        "some-git-dep",
        "MIT",
        &SourceKind::GitPinned, // SHA-pinned, but URL not in allow-git = []
        false,
        None,
    );
    assert!(!outcome.allowed, "SHA-pinned git dep (not in allow-git) was not blocked");
}

// ---------------------------------------------------------------------------
// Scenario 4: Unmaintained dep (warn, not auto-deny — intentional)
// ---------------------------------------------------------------------------

/// Red-team scenario: dep marked `unmaintained` in RUSTSEC advisory db.
///
/// Policy decision: `unmaintained = "warn"` in deny.toml (not "deny").
/// This is intentional — auto-deny would cause false positives on widely-used
/// but stable crates.  Quarterly review process handles unmaintained deps.
///
/// This test documents the INTENTIONAL warn behaviour.  If this test fails,
/// the policy has changed and the quarterly review process must be updated.
///
/// Ref: §2 §9.2 + §14.s12.004.11 (quarterly review documented).
///
/// Note: The `evaluate_policy` function models "deny" for any advisory —
/// unmaintained advisories in the real cargo-deny config use `unmaintained = "warn"`.
/// This test therefore checks that the LIBRARY correctly models the advisory path
/// (which includes unmaintained RUSTSEC IDs) and documents the intentional gap.
#[test]
fn scenario_04_unmaintained_dep_advisory_path_documented() {
    // Unmaintained deps have RUSTSEC advisory IDs like RUSTSEC-2024-XXXX.
    // In the real cargo-deny config, unmaintained = "warn" (not "deny").
    // Our library's evaluate_policy models "deny" for any advisory for
    // property test coverage of the alert path.
    // This test documents that the library is intentionally conservative.
    let outcome = evaluate_policy(
        "unmaintained-serde-old",
        "MIT",
        &SourceKind::CratesIo,
        false,
        Some("RUSTSEC-2023-0071"), // a known unmaintained advisory
    );
    // The evaluator denies any advisory — library is intentionally conservative.
    // In production, unmaintained = "warn" in cargo-deny; this test serves as
    // the quarterly review trigger documentation (see dep-policy.md §5).
    assert!(
        !outcome.allowed,
        "Advisory path for unmaintained dep returned allowed — evaluator is broken"
    );
    assert!(
        matches!(outcome.denial_reason, Some(DenialReason::Advisory(_))),
        "Expected Advisory denial, got: {:?}",
        outcome.denial_reason
    );
    // NOTE: In production cargo-deny, unmaintained = "warn" means this would
    // NOT block CI; it would produce a warning requiring quarterly review action.
    // The deny.toml [advisories] section documents the intentional separation.
}

// ---------------------------------------------------------------------------
// Scenario 5: [patch.crates-io] without ADR
// ---------------------------------------------------------------------------

/// Red-team scenario: developer applies a vendor patch via `[patch.crates-io]`
/// without a mandatory ADR + Security review.
///
/// Mitigation: `[patch.crates-io]` patches introduce a git dep; without the
/// URL being in `allow-git`, cargo-deny blocks it.  The ADR + Security review
/// requirement is enforced at the PR review level (pre-merge check).
///
/// This test validates the source-level block (cargo-deny) as the automated
/// gate. The ADR requirement is the human-level gate (both required).
///
/// Ref: §7 anti-scope + §9.6 + §15 Chaos Experiment 7 + §28 R-008.
#[test]
fn scenario_05_vendor_patch_without_adr_blocked_at_source_level() {
    // A [patch.crates-io] entry that points to a local path or git URL
    // without being in the allow-git list is treated as git-pinned (if SHA
    // is provided) or git-unpinned (if branch/tag).
    let outcome_unpinned = evaluate_policy(
        "serde",                           // legitimate crate, patched version
        "MIT",                             // correct license
        &SourceKind::GitUnpinned,          // [patch.crates-io] git without commit
        false,
        None,
    );
    assert!(
        !outcome_unpinned.allowed,
        "Vendor patch (git-unpinned) was NOT blocked — [patch.crates-io] without ADR slipped through!"
    );

    let outcome_pinned = evaluate_policy(
        "tokio",
        "MIT",
        &SourceKind::GitPinned,            // [patch.crates-io] with SHA, not in allow-git
        false,
        None,
    );
    assert!(
        !outcome_pinned.allowed,
        "Vendor patch (git-pinned, not in allow-git) was NOT blocked!"
    );
}

// ---------------------------------------------------------------------------
// Composite adversarial: all 5 scenarios in one sweep
// ---------------------------------------------------------------------------

/// Composite regression: 5 adversarial vectors × canonical inputs.
///
/// This test provides a single CI-visible gate that fires if any of the
/// 5 adversarial scenarios is broken, without requiring running all tests
/// individually.  **10.s12.004.2** (EVT-040): 5 scenarios × 100% mitigated.
#[test]
fn regression_all_5_adversarial_scenarios_blocked() {
    let adversarial_cases = [
        // (label, crate_name, spdx, source, yanked, advisory)
        ("GPL-3.0 dep", "evil-gpl", "GPL-3.0", SourceKind::CratesIo, false, None),
        ("Yanked dep", "yanked-dep", "MIT", SourceKind::CratesIo, true, None),
        ("Typosquat unknown-registry", "corelink-fake", "MIT", SourceKind::UnknownRegistry, false, None),
        ("Vendor patch git-unpinned", "serde-patched", "MIT", SourceKind::GitUnpinned, false, None),
        ("Advisory dep", "vuln-crate", "MIT", SourceKind::CratesIo, false, Some("RUSTSEC-2024-0001")),
    ];

    for (label, crate_name, spdx, source, yanked, advisory) in adversarial_cases {
        let outcome = evaluate_policy(crate_name, spdx, &source, yanked, advisory);
        assert!(
            !outcome.allowed,
            "ADVERSARIAL REGRESSION: scenario '{label}' was NOT blocked! \
             Denial reason: {:?}",
            outcome.denial_reason
        );
    }
}
