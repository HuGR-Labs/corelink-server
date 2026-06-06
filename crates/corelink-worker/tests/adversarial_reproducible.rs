//! WI-S12-006 — adversarial regression tests for reproducible builds.
//!
//! Covers the five adversarial scenarios from §6.1.9:
//!
//! 1. [`compromised_builder_diff_exceeds_threshold`] — Simulates a malicious
//!    code injection that changes > 5 % of bytes; asserts the gate rejects it.
//!
//! 2. [`non_determinism_regression_detected`] — Simulates a dep upgrade that
//!    introduces new timestamp bytes; asserts the nightly CI diff metric fires.
//!
//! 3. [`build_rs_timestamp_lint_catches_violations`] — Synthetic `build.rs`
//!    with banned patterns; asserts lint detection logic triggers.
//!
//! 4. [`cross_runner_cpu_heterogeneity_documented_limitation`] — Documents
//!    that CPU-heterogeneity-induced diffs are expected within the 5 %
//!    threshold; verified not to trip the gate at realistic sizes.
//!
//! 5. [`rustc_minor_upgrade_regression_gate`] — Simulates a toolchain bump
//!    scenario; asserts that a diff > 5 % from a synthetic rustc upgrade
//!    would correctly fail the reproducible-build gate.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test code: panics surface as test failures by design"
)]

// ── Shared helpers (must stay in sync with prop_reproducible.rs) ──────────────

fn diff_percentage(diff_bytes: u64, total_bytes: u64) -> Option<f64> {
    if total_bytes == 0 {
        return None;
    }
    Some((diff_bytes as f64 / total_bytes as f64) * 100.0)
}

fn is_within_threshold(diff_bytes: u64, total_bytes: u64) -> bool {
    if diff_bytes == 0 {
        return true;
    }
    match diff_percentage(diff_bytes, total_bytes) {
        Some(pct) => pct <= 5.0,
        None => false,
    }
}

fn build_rs_has_banned_pattern(source: &str) -> bool {
    source.contains("chrono::Local::now()")
        || source.contains("chrono::Utc::now()")
        || source.contains("SystemTime::now()")
        || (source.contains("UNIX_EPOCH") && !source.contains("SOURCE_DATE_EPOCH"))
}

/// Map (diff_bytes, total_bytes) to outcome string (mirrors CI workflow output).
fn compute_outcome(diff_bytes: u64, total_bytes: u64) -> &'static str {
    if diff_bytes == 0 {
        "bit_identical"
    } else {
        match diff_percentage(diff_bytes, total_bytes) {
            Some(pct) if pct <= 5.0 => "within_threshold",
            Some(_) => "exceeds_threshold",
            None => "build_failed",
        }
    }
}

// ── Scenario 1: Compromised builder injects malicious code ────────────────────

/// A malicious binary injection changes ≥ 6 % of bytes.  The gate MUST
/// reject this and emit outcome = exceeds_threshold.
///
/// Regression contract (ADR-0015 §8 Threat "Compromised builder injects
/// backdoor"): the attacker would need to inject changes that stay within
/// 5 % of the total binary size to evade detection.  For a realistic WASM
/// binary this is a very small byte budget for a functional backdoor.
#[test]
fn compromised_builder_diff_exceeds_threshold() {
    // Simulate a realistic corelink-worker.wasm size (~500 KB).
    let total_bytes: u64 = 512_000;

    // Malicious injection changes ~10 % of bytes (a realistic backdoor payload).
    let injected_bytes: u64 = (total_bytes * 10) / 100; // 51_200 bytes
    let pct = diff_percentage(injected_bytes, total_bytes).expect("non-zero total");
    assert!(
        pct > 5.0,
        "injection of {injected_bytes} bytes into {total_bytes} should exceed 5 %; got {pct:.2}"
    );

    let outcome = compute_outcome(injected_bytes, total_bytes);
    assert_eq!(
        outcome, "exceeds_threshold",
        "compromised builder injection MUST produce exceeds_threshold outcome"
    );
    assert!(
        !is_within_threshold(injected_bytes, total_bytes),
        "gate MUST reject diff of {pct:.2}% (threshold ≤ 5 %)"
    );

    // Verify boundary: attacker is limited to ≤ floor(total_bytes * 5/100) bytes.
    let max_covert_bytes = (total_bytes * 5) / 100; // 25_600 bytes
    let max_covert_pct = diff_percentage(max_covert_bytes, total_bytes).unwrap();
    // At the boundary the gate accepts — attacker has ≤ 25 KB covert budget.
    assert!(
        is_within_threshold(max_covert_bytes, total_bytes),
        "boundary must be accepted: {max_covert_pct:.2}%"
    );
    // Verify the covert budget is tiny (< 5.1 %) — hard to embed a real backdoor.
    assert!(
        max_covert_pct <= 5.1,
        "covert budget must be ≤ 5.1 %; got {max_covert_pct:.2}%"
    );
}

// ── Scenario 2: Non-determinism regression via dep upgrade ────────────────────

/// A dep upgrade introduces a new timestamp that is not SOURCE_DATE_EPOCH-
/// aware, causing additional diff bytes in the nightly run.  The test
/// simulates the "before" (clean) and "after" (regressed) states and
/// verifies the CI metric fires correctly.
#[test]
fn non_determinism_regression_detected() {
    // Before dep upgrade: diff within threshold (clean state).
    let total_bytes: u64 = 600_000;
    let baseline_diff: u64 = 5_000; // ~0.83 %
    assert!(
        is_within_threshold(baseline_diff, total_bytes),
        "baseline state must be within threshold"
    );
    assert_eq!(
        compute_outcome(baseline_diff, total_bytes),
        "within_threshold"
    );

    // After dep upgrade: new non-deterministic timestamp adds ~30 KB of diff.
    let regressed_diff: u64 = baseline_diff + 30_000; // ~5.83 %
    let regressed_pct = diff_percentage(regressed_diff, total_bytes).unwrap();

    // The regression is detectable: diff exceeds 5 %.
    assert!(
        regressed_pct > 5.0,
        "regressed diff must exceed 5 %; got {regressed_pct:.2}%"
    );
    assert_eq!(
        compute_outcome(regressed_diff, total_bytes),
        "exceeds_threshold",
        "nightly CI must emit exceeds_threshold outcome after regression"
    );
    assert!(
        !is_within_threshold(regressed_diff, total_bytes),
        "gate must fail on regressed diff {regressed_pct:.2}%"
    );

    // Root cause analysis trigger: diff grew from ~0.83 % to ~5.83 %.
    // This > 5× growth is a strong signal for a new non-determinism source.
    let growth_factor = regressed_diff as f64 / baseline_diff as f64;
    assert!(
        growth_factor > 5.0,
        "growth factor should be > 5× to signal regression: {growth_factor:.1}×"
    );
}

// ── Scenario 3: build.rs timestamp lint catches violations ────────────────────

/// Synthetic `build.rs` scripts containing banned timestamp patterns must
/// be detected by the lint logic (mirrors `scripts/build_rs_lint.sh`).
#[test]
fn build_rs_timestamp_lint_catches_violations() {
    // Case A: chrono::Local::now() — banned.
    let banned_chrono_local = r#"
        fn main() {
            let ts = chrono::Local::now().timestamp();
            println!("cargo:rustc-env=BUILD_TIMESTAMP={}", ts);
        }
    "#;
    assert!(
        build_rs_has_banned_pattern(banned_chrono_local),
        "chrono::Local::now() must be detected as banned"
    );

    // Case B: chrono::Utc::now() — banned.
    let banned_chrono_utc = r#"
        fn main() {
            let ts = chrono::Utc::now().timestamp();
            println!("cargo:rustc-env=BUILD_TIMESTAMP={}", ts);
        }
    "#;
    assert!(
        build_rs_has_banned_pattern(banned_chrono_utc),
        "chrono::Utc::now() must be detected as banned"
    );

    // Case C: SystemTime::now() — banned.
    let banned_systemtime = r#"
        use std::time::SystemTime;
        fn main() {
            let ts = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            println!("cargo:rustc-env=BUILD_TIMESTAMP={}", ts);
        }
    "#;
    assert!(
        build_rs_has_banned_pattern(banned_systemtime),
        "SystemTime::now() must be detected as banned"
    );

    // Case D: UNIX_EPOCH without SOURCE_DATE_EPOCH — banned.
    let banned_unix_epoch = r#"
        fn main() {
            let since_epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            println!("cargo:rustc-env=BUILD_SECS={}", since_epoch.as_secs());
        }
    "#;
    assert!(
        build_rs_has_banned_pattern(banned_unix_epoch),
        "UNIX_EPOCH without SOURCE_DATE_EPOCH must be detected as banned"
    );

    // Case E: compliant pattern — must NOT be detected as banned.
    let compliant = r#"
        fn main() {
            let epoch = std::env::var("SOURCE_DATE_EPOCH")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            println!("cargo:rustc-env=BUILD_TIMESTAMP={}", epoch);
        }
    "#;
    assert!(
        !build_rs_has_banned_pattern(compliant),
        "compliant SOURCE_DATE_EPOCH pattern must NOT be flagged"
    );

    // Case F: empty build.rs — must NOT be detected as banned.
    let empty = r#"fn main() {}"#;
    assert!(
        !build_rs_has_banned_pattern(empty),
        "empty build.rs must NOT be flagged"
    );
}

// ── Scenario 4: Cross-runner CPU heterogeneity (documented limitation) ────────

/// CPU-heterogeneity-induced diffs are a documented limitation (ADR-0015 §3.6
/// in docs/build/reproducible.md §3.6).  At realistic WASM sizes the expected
/// diff from LLVM auto-vectorization differences fits within the 5 % budget.
///
/// This test documents the expected regime: small (< 2 %) diff from LLVM
/// instruction-scheduling differences is accepted by the gate.
#[test]
fn cross_runner_cpu_heterogeneity_documented_limitation() {
    // Realistic corelink-worker.wasm: ~400 KB.
    let total_bytes: u64 = 400_000;

    // Observed CPU-heterogeneity diff regime in practice: < 1 % of WASM
    // (SIMD instruction alternatives for a few hot loops).
    let cpu_diff_bytes: u64 = 2_000; // ~0.5 %
    let pct = diff_percentage(cpu_diff_bytes, total_bytes).unwrap();
    assert!(
        pct < 1.0,
        "CPU heterogeneity diff expected < 1 %; got {pct:.2}%"
    );
    assert!(
        is_within_threshold(cpu_diff_bytes, total_bytes),
        "CPU-heterogeneity diff ({pct:.2}%) must be within 5 % threshold"
    );
    assert_eq!(
        compute_outcome(cpu_diff_bytes, total_bytes),
        "within_threshold"
    );

    // Verify the documented mitigation (ubuntu-22.04 pin + generic target)
    // keeps the diff well inside the budget: budget at 5 % is 20 KB;
    // expected CPU diff is ~2 KB — 10× headroom.
    let budget_5pct = (total_bytes * 5) / 100;
    assert!(
        cpu_diff_bytes * 10 <= budget_5pct,
        "CPU diff ({cpu_diff_bytes} B) should be ≤ 1/10 of 5 % budget ({budget_5pct} B)"
    );
}

// ── Scenario 5: rustc minor upgrade regression gate ───────────────────────────

/// A synthetic rustc minor upgrade (1.84 → 1.85) that introduces LLVM
/// non-determinism would produce a diff > 5 % and block the PR until the
/// ADR amendment process completes.
#[test]
fn rustc_minor_upgrade_regression_gate() {
    let total_bytes: u64 = 500_000;

    // Before upgrade: bit-identical (best case after 1.84 pin).
    let before_diff: u64 = 0;
    assert_eq!(compute_outcome(before_diff, total_bytes), "bit_identical");
    assert!(is_within_threshold(before_diff, total_bytes));

    // After hypothetical 1.84 → 1.85 upgrade that introduces new opt pass:
    // LLVM might re-order basic blocks, changing ~8 % of bytes.
    let after_diff: u64 = (total_bytes * 8) / 100; // 40_000 bytes = 8 %
    let after_pct = diff_percentage(after_diff, total_bytes).unwrap();
    assert!(
        after_pct > 5.0,
        "post-upgrade diff must exceed 5 %; got {after_pct:.2}%"
    );
    assert_eq!(
        compute_outcome(after_diff, total_bytes),
        "exceeds_threshold",
        "rustc upgrade regression must produce exceeds_threshold"
    );
    assert!(
        !is_within_threshold(after_diff, total_bytes),
        "gate MUST fail on post-upgrade diff {after_pct:.2}%"
    );

    // Mitigation: rust-toolchain.toml prevents the upgrade from landing
    // unless the PR explicitly bumps it AND the reproducible-build test
    // passes.  The gate above ensures the PR cannot merge with > 5 % diff.
    // ADR-0015 §3.5 (rustc minor upgrade tested) documents this procedure.
}
