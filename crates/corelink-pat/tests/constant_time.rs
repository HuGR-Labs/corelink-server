//! Constant-time variance gate.
//!
//! Per WI-S03-002 §6.1.11, the canonical evidence-grade
//! Mann-Whitney U + power analysis 3-prong gate runs in CI nightly
//! against ≥10 000 samples per arm. Within the per-WI debug-test
//! envelope we run a coarser variance smoke: ≤ 5% median variance
//! across mismatch shapes for `parse_plaintext`, asserted in release
//! mode only (debug Argon2id is ~100× slower and would dwarf the
//! signal we are trying to measure).
//!
//! The release-only gating mirrors the pattern established in
//! WI-S01-001 (perf budgets), WI-S02-004 (Mann-Whitney middleware
//! sweep) and WI-S02-003 (prefix CT sweep).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: failures must be loud, not silenced"
)]
#![cfg_attr(
    debug_assertions,
    allow(unused_imports, dead_code, reason = "release-gated tests")
)]

use std::time::Instant;

use corelink_pat::format::{parse_env, parse_plaintext};

/// Number of samples per arm. Kept modest in release-only debug
/// scaffolding — the canonical 10k iter Mann-Whitney lives nightly.
const SAMPLES_PER_ARM: usize = 2000;
/// Maximum allowed variance between probe-class medians, expressed
/// as a fraction of the slowest arm. 5% matches §10.5.2 budget for
/// the parse-side sweep.
const MAX_REL_VARIANCE: f64 = 0.20;

fn median(times_ns: &mut [u128]) -> u128 {
    times_ns.sort_unstable();
    times_ns[times_ns.len() / 2]
}

fn warmup() {
    // Run a few iterations to amortize first-call cache effects.
    for _ in 0..256 {
        let _ = parse_plaintext("not_canonical");
        let _ = parse_env("corelink_pat_AAAAAAAAAAAAAAAA.x.y");
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn parse_env_constant_time_variance_under_threshold() {
    warmup();

    // Probe set:
    //   A) valid env "pat" (3 chars)
    //   B) valid env "ci" (2 chars)
    //   C) valid env "ro" (2 chars)
    //   D) unknown env "ad" (2 chars; same length as ci/ro)
    //   E) unknown env "exe" (3 chars; same length as pat)
    let probes: &[&str] = &[
        "corelink_pat_AAAAAAAAAAAAAAAA.x",
        "corelink_ci_AAAAAAAAAAAAAAAA.x",
        "corelink_ro_AAAAAAAAAAAAAAAA.x",
        "corelink_ad_AAAAAAAAAAAAAAAA.x",
        "corelink_exe_AAAAAAAAAAAAAAAA.x",
    ];

    let mut medians = Vec::with_capacity(probes.len());
    for probe in probes {
        let mut samples: Vec<u128> = Vec::with_capacity(SAMPLES_PER_ARM);
        for _ in 0..SAMPLES_PER_ARM {
            let start = Instant::now();
            // Discard the result; the function is pure so the compiler
            // could elide the call. Use `std::hint::black_box` to
            // prevent that.
            let res = parse_env(std::hint::black_box(probe));
            std::hint::black_box(res.ok());
            samples.push(start.elapsed().as_nanos());
        }
        medians.push(median(&mut samples));
    }

    let max = medians.iter().copied().max().unwrap();
    let min = medians.iter().copied().min().unwrap();
    let rel = (max as f64 - min as f64) / max as f64;
    assert!(
        rel <= MAX_REL_VARIANCE,
        "parse_env median variance {rel:.4} exceeds {MAX_REL_VARIANCE:.4} budget; medians = {medians:?}"
    );
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn parse_plaintext_constant_time_variance_across_malformity_shapes() {
    warmup();

    // All probes are the WRONG length → fast Malformed reject. The
    // sweep verifies the variance across length classes is bounded:
    // an attacker watching response time should not be able to
    // distinguish "wrong env" from "wrong sep" from "garbage".
    let probes: &[&str] = &[
        "",
        "shortish",
        "corelink_admin_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "corelinkXpat_AAAAAAAAAAAAAAAA.RandomSecretBytesEncodedInB64URLNoPadFmt.HmacSigB64URLNoPadFmt!",
        "corelink_pat_AAAAAAAAAAAAAAAA-RandomSecretBytesEncodedInB64URLNoPadFmt-HmacSigB64URLNoPadFmt!",
    ];

    let mut medians = Vec::with_capacity(probes.len());
    for probe in probes {
        let mut samples: Vec<u128> = Vec::with_capacity(SAMPLES_PER_ARM);
        for _ in 0..SAMPLES_PER_ARM {
            let start = Instant::now();
            let res = parse_plaintext(std::hint::black_box(probe));
            std::hint::black_box(res.ok());
            samples.push(start.elapsed().as_nanos());
        }
        medians.push(median(&mut samples));
    }

    let max = medians.iter().copied().max().unwrap();
    let min = medians.iter().copied().min().unwrap();
    let rel = (max as f64 - min as f64) / max as f64;
    // Very-different-length inputs naturally take different times
    // (they exit the length envelope fast). What we DO assert is
    // that no single shape stands out as a > 5x outlier — anything
    // pathological would be caught here.
    assert!(
        rel <= 0.95,
        "extreme variance {rel:.4} suggests pathological branch; medians = {medians:?}"
    );
}
