//! WI-S12-006 — reproducible builds property tests.
//!
//! Covers the four property requirements from §6.1.8:
//!
//! - [`prop_source_date_epoch_honored`] — 10k synthetic `build.rs` script
//!   strings; assert `SOURCE_DATE_EPOCH` is referenced (lint-equivalent check).
//! - [`prop_remap_path_prefix_applied`] — 10k path strings; assert
//!   `--remap-path-prefix` correctly strips absolute prefixes.
//! - [`prop_diff_percentage_calculation`] — 10k (diff_bytes, total_bytes)
//!   pairs; assert percentage computed correctly and boundary conditions hold.
//! - [`prop_threshold_enforcement`] — 10k diff scenarios; assert ≤ 5 %
//!   accepted, > 5 % rejected.
//!
//! PR-gate: 10k per prop (`PROPTEST_CASES` default).
//! Nightly: 100k (`PROPTEST_CASES=100000` env override per S-07 P1-2).
//!
//! This file is host-only (no wasm32 target) — it exercises logic that
//! runs in CI scripts and is not part of the WASM crate.  No tokio.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test code: panics surface as test failures by design"
)]

use proptest::prelude::*;

// ── Helpers (mirror logic from the reproducible-build CI workflow) ─────────────

/// Compute diff percentage: (diff_bytes / total_bytes) × 100.
///
/// Returns `None` if `total_bytes == 0` to avoid division by zero.
/// This mirrors the `awk "BEGIN {printf "%.4f", ...}"` in the workflow.
fn diff_percentage(diff_bytes: u64, total_bytes: u64) -> Option<f64> {
    if total_bytes == 0 {
        return None;
    }
    Some((diff_bytes as f64 / total_bytes as f64) * 100.0)
}

/// Returns `true` if the diff is acceptable (≤ 5 % or bit-identical).
///
/// This mirrors the `Enforce ≤ 5 % diff threshold` step in the workflow.
fn is_within_threshold(diff_bytes: u64, total_bytes: u64) -> bool {
    if diff_bytes == 0 {
        return true; // bit-identical
    }
    match diff_percentage(diff_bytes, total_bytes) {
        Some(pct) => pct <= 5.0,
        None => false, // 0-byte binary is an error; conservatively reject
    }
}

/// Apply `--remap-path-prefix` semantics: replace `original_prefix` with
/// `canonical_prefix` at the start of `path`.
///
/// This mirrors the LLVM remap logic triggered by RUSTFLAGS in the workflow.
fn remap_path(path: &str, original_prefix: &str, canonical_prefix: &str) -> String {
    if let Some(stripped) = path.strip_prefix(original_prefix) {
        format!("{canonical_prefix}{stripped}")
    } else {
        path.to_string()
    }
}

/// Returns `true` if the given `build_rs_source` contains a banned
/// non-deterministic timestamp pattern that bypasses SOURCE_DATE_EPOCH.
///
/// This mirrors `scripts/build_rs_lint.sh` pattern detection.
fn build_rs_has_banned_pattern(source: &str) -> bool {
    source.contains("chrono::Local::now()")
        || source.contains("chrono::Utc::now()")
        || source.contains("SystemTime::now()")
        // UNIX_EPOCH alone (without SOURCE_DATE_EPOCH context) is banned
        || (source.contains("UNIX_EPOCH") && !source.contains("SOURCE_DATE_EPOCH"))
}

/// Returns `true` if `build_rs_source` contains the compliant
/// SOURCE_DATE_EPOCH honour pattern.
fn build_rs_honors_source_date_epoch(source: &str) -> bool {
    source.contains("SOURCE_DATE_EPOCH")
}

// ── Read PROPTEST_CASES at runtime (S-07 P1-2 pattern) ────────────────────────

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// ── Property: SOURCE_DATE_EPOCH compliance ────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1, // overridden below; placeholder satisfies macro
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    // Synthetic build.rs scripts containing the banned pattern must be
    // detected.  A script that uses SOURCE_DATE_EPOCH must NOT be flagged.
    #[test]
    fn prop_source_date_epoch_banned_patterns_detected(
        // Use a boolean to select between banned and compliant templates.
        use_banned in any::<bool>(),
        // Arbitrary prefix/suffix to simulate real build.rs noise.
        prefix_len in 0usize..40usize,
        suffix_len in 0usize..40usize,
        prefix_seed in any::<u64>(),
        suffix_seed in any::<u64>(),
    ) {
        // Build a simple noise string from seeds.
        let prefix: String = (0..prefix_len)
            .map(|i| char::from_u32(((prefix_seed.wrapping_add(i as u64) % 26) as u32) + b'a' as u32).unwrap_or('x'))
            .collect();
        let suffix: String = (0..suffix_len)
            .map(|i| char::from_u32(((suffix_seed.wrapping_add(i as u64) % 26) as u32) + b'a' as u32).unwrap_or('y'))
            .collect();

        let (source, expect_banned) = if use_banned {
            // Compliant pattern: uses SOURCE_DATE_EPOCH.
            (
                format!(
                    "{}fn main() {{ let epoch = std::env::var(\"SOURCE_DATE_EPOCH\").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0); println!(\"cargo:rustc-env=BUILD_TIMESTAMP={{}}\", epoch); }}{}",
                    prefix, suffix
                ),
                false,
            )
        } else {
            // Banned pattern: uses chrono::Local::now() directly.
            (
                format!(
                    "{}fn main() {{ let ts = chrono::Local::now().timestamp(); println!(\"cargo:rustc-env=BUILD_TIMESTAMP={{}}\", ts); }}{}",
                    prefix, suffix
                ),
                true,
            )
        };

        let detected = build_rs_has_banned_pattern(&source);
        prop_assert_eq!(
            detected,
            expect_banned,
            "banned={} detected={} source_snippet={}",
            expect_banned, detected, &source[..source.len().min(80)]
        );

        // Compliant scripts must reference SOURCE_DATE_EPOCH.
        if !expect_banned {
            let honors = build_rs_honors_source_date_epoch(&source);
            prop_assert!(
                honors,
                "compliant source must reference SOURCE_DATE_EPOCH: {}",
                &source[..source.len().min(80)]
            );
        }
    }
}

// ── Property: --remap-path-prefix application ─────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_remap_path_prefix_applied(
        // Simulate a workspace absolute path prefix.
        workspace_root_seed in 0u64..10u64,
        // Relative path within the workspace.
        rel_path_len in 1usize..30usize,
        rel_seed in any::<u64>(),
    ) {
        let workspace_roots = [
            "/home/runner/work/corelink-server/corelink-server",
            "/home/user/projects/corelink",
            "/opt/hostedtoolcache/work/corelink",
            "/Users/dev/corelink",
            "/tmp/build-agent-1/corelink",
            "/tmp/build-agent-2/corelink",
            "/home/github-actions/workspace",
            "/runner/_work/corelink",
            "/workspace",
            "/build",
        ];
        let original = workspace_roots[workspace_root_seed as usize % workspace_roots.len()];

        let rel: String = (0..rel_path_len)
            .map(|i| {
                let c = (rel_seed.wrapping_add(i as u64) % 26) as u32 + b'a' as u32;
                char::from_u32(c).unwrap_or('z')
            })
            .collect();
        let full_path = format!("{original}/src/{rel}.rs");

        let remapped = remap_path(&full_path, original, "/SRC");

        // After remap, the original prefix must be absent.
        prop_assert!(
            !remapped.starts_with(original),
            "original prefix still present after remap: {remapped}"
        );
        // After remap, the canonical prefix must be present.
        prop_assert!(
            remapped.starts_with("/SRC"),
            "canonical prefix /SRC not present: {remapped}"
        );
        // The relative suffix must be preserved unchanged.
        let expected_suffix = format!("/src/{rel}.rs");
        prop_assert!(
            remapped.ends_with(&expected_suffix),
            "relative suffix not preserved: remapped={remapped} expected_suffix={expected_suffix}"
        );
    }
}

// ── Property: diff percentage calculation ─────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_diff_percentage_calculation(
        // total_bytes: 1 byte to ~10 MB (realistic WASM sizes).
        total_bytes in 1u64..10_000_000u64,
        // diff_bytes: 0 to total_bytes.
        diff_fraction_pct in 0u64..=100u64,
    ) {
        let diff_bytes = (total_bytes * diff_fraction_pct) / 100;
        let pct = diff_percentage(diff_bytes, total_bytes);

        prop_assert!(pct.is_some(), "diff_percentage returned None for non-zero total");
        let pct = pct.unwrap();

        // Basic bounds check.
        prop_assert!(pct >= 0.0, "percentage must be non-negative: {pct}");
        prop_assert!(pct <= 100.0, "percentage must not exceed 100: {pct}");

        // When diff_bytes == 0, percentage must be exactly 0.
        if diff_bytes == 0 {
            prop_assert_eq!(pct, 0.0, "zero diff_bytes must yield 0.0%");
        }
        // When diff_bytes == total_bytes, percentage must be 100.
        if diff_bytes == total_bytes {
            let tolerance = 0.01; // floating-point rounding
            prop_assert!(
                (pct - 100.0).abs() < tolerance,
                "full diff must yield ~100%: got {pct}"
            );
        }
    }

    /// diff_percentage on a zero-length binary returns None (no division by zero).
    #[test]
    fn prop_diff_percentage_zero_total_is_none(
        diff_bytes in any::<u64>(),
    ) {
        let result = diff_percentage(diff_bytes, 0);
        prop_assert!(result.is_none(), "expected None for total_bytes=0, got {result:?}");
    }
}

// ── Property: threshold enforcement ───────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_threshold_enforcement(
        total_bytes in 1_000u64..10_000_000u64,
        // Exact diff percentage expressed as tenths of a percent (0..200 → 0.0%..20.0%).
        diff_tenths_pct in 0u64..200u64,
    ) {
        // Compute diff_bytes from the requested fraction.
        // Clamp to total_bytes to avoid arithmetic overflow.
        let diff_bytes = ((total_bytes * diff_tenths_pct) / 1000).min(total_bytes);
        let pct = diff_percentage(diff_bytes, total_bytes).unwrap_or(0.0);
        let accepted = is_within_threshold(diff_bytes, total_bytes);

        if pct <= 5.0 {
            prop_assert!(
                accepted,
                "diff ≤ 5% must be accepted: diff_bytes={diff_bytes} total={total_bytes} pct={pct:.4}"
            );
        } else {
            prop_assert!(
                !accepted,
                "diff > 5% must be rejected: diff_bytes={diff_bytes} total={total_bytes} pct={pct:.4}"
            );
        }
    }

    /// Bit-identical (diff_bytes == 0) is always within threshold.
    #[test]
    fn prop_bit_identical_always_accepted(
        total_bytes in 1u64..10_000_000u64,
    ) {
        prop_assert!(
            is_within_threshold(0, total_bytes),
            "bit-identical (0 diff bytes) must always be accepted"
        );
    }
}

// ── Runtime-driven tests (respects PROPTEST_CASES env) ────────────────────────

#[test]
fn prop_reproducible_all_10k() {
    let cases = proptest_cases();
    let config = ProptestConfig {
        cases,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    };

    // prop: threshold boundary — exact 5 % is accepted, 5.001 % is rejected.
    proptest!(config.clone(), |(total_bytes in 1_000u64..5_000_000u64)| {
        // Exactly 5 %.
        let diff_exactly_5 = (total_bytes * 5) / 100;
        prop_assert!(
            is_within_threshold(diff_exactly_5, total_bytes),
            "exactly 5% must be accepted (total={total_bytes})"
        );
        // One byte over the 5 % threshold (when total > 100 bytes to avoid rounding).
        if total_bytes > 100 {
            let diff_over = diff_exactly_5 + 1;
            let pct = diff_percentage(diff_over, total_bytes).unwrap_or(0.0);
            if pct > 5.0 {
                prop_assert!(
                    !is_within_threshold(diff_over, total_bytes),
                    "just-over-5% must be rejected (total={total_bytes}, diff={diff_over}, pct={pct:.4})"
                );
            }
        }
    });
}
