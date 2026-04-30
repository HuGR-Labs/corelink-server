//! WI-S02-003 property tests for `corelink-client-verify`.
//!
//! Coverage map (cross-referenced with WI §8 / §10):
//!
//! - AC "Default config enables verify" → [`default_is_enabled`].
//! - AC "Sync verify happy path" → [`verify_match_basic`].
//! - AC "Sync verify mismatch detected" → [`verify_mismatch_basic`] +
//!   error variant carries hex digests of `expected` + `computed`.
//! - AC "Property test — 10k random body+digest pairs" →
//!   [`prop_verify_match_always_ok`] + [`prop_verify_mismatch_always_err`].
//! - AC "Property test — constant-time compare" →
//!   [`constant_time_variance_on_verify`] (release-only).
//! - AC "Opt-out emit warning" → [`disabled_increments_counter_and_errs`].
//! - AC "ABI stability for FFI" → [`tests/abi_smoke.rs`] (separate file
//!   because it pokes the `extern "C"` surface directly).
//! - AC "Stand-alone build (no Worker deps)" → enforced by
//!   `Cargo.toml` dep tree + nightly CI `cargo tree` job.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs,
    reason = "test code; panic on assertion failure is the contract"
)]

use std::sync::Mutex;
use std::time::Instant;

use corelink_client_verify::{
    opt_out_total, ClientVerifier, Digest, VerifyConfig, VerifyError, COR_CAS_DIGEST_MISMATCH,
    COR_CAS_VERIFY_DISABLED, DIGEST_LEN,
};
use proptest::prelude::*;

/// Process-wide test mutex for any test that asserts on the
/// `opt_out_total` counter. Tests run in parallel by default; the
/// counter is process-wide, so two opt-out tests racing would each
/// see the other's increment and fail their delta-assertion. Holding
/// this mutex serializes them. Tests that do NOT assert on the
/// counter delta (e.g. `prop_disabled_always_errs`) do not need the
/// mutex — they only ever observe the counter going up, never down.
static COUNTER_TEST_MUTEX: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Canonical regression vectors — cross-language smoke that BLAKE3 of the
// canonical bodies matches the same hex the server-side `corelink-hash`
// crate sees. Pinning these inside this crate's test corpus defends
// against a future split where the FFI consumer's view drifts from the
// server's view.
// ---------------------------------------------------------------------------

#[test]
fn canonical_vectors_verify_against_known_hex() {
    struct Case {
        body: Vec<u8>,
        expected_hex: &'static str,
    }
    let cases: Vec<Case> = vec![
        Case {
            body: b"".to_vec(),
            expected_hex: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        },
        Case {
            body: b"a".to_vec(),
            expected_hex: "17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f",
        },
        Case {
            body: b"hello world".to_vec(),
            expected_hex: "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
        },
        Case {
            body: b"The quick brown fox jumps over the lazy dog".to_vec(),
            expected_hex: "2f1514181aadccd913abd94cfa592701a5686ab23f8df1dff1b74710febc6d4a",
        },
    ];
    let v = ClientVerifier::default_on();
    for case in &cases {
        let claimed = Digest::from_hex(case.expected_hex).expect("static hex");
        v.verify(&case.body, &claimed)
            .expect("canonical vector must verify");
        // Mismatch surface: flip a byte and ensure error hex matches.
        let mut wrong = *claimed.as_bytes();
        wrong[0] ^= 0x01;
        let wrong_d = Digest::from_hex(&hex::encode(wrong)).expect("hex");
        let err = v
            .verify(&case.body, &wrong_d)
            .expect_err("flipped byte must mismatch");
        match err {
            VerifyError::DigestMismatch { expected, computed } => {
                assert_eq!(expected, wrong_d.to_hex());
                assert_eq!(computed, case.expected_hex);
            }
            VerifyError::VerifyDisabled => panic!("default verifier must verify"),
            _ => panic!("unexpected non-exhaustive variant"),
        }
    }
}

#[test]
fn default_is_enabled() {
    let cfg = VerifyConfig::default();
    assert!(cfg.enabled());
    assert!(cfg.warn_on_optout());
    let v = ClientVerifier::default();
    assert!(v.config().enabled());
}

#[test]
fn verify_match_basic() {
    let v = ClientVerifier::default_on();
    let body = b"hello world";
    let d = Digest::compute(body);
    v.verify(body, &d).expect("ok");
}

#[test]
fn verify_mismatch_basic() {
    let v = ClientVerifier::default_on();
    let body = b"hello world";
    let wrong = Digest::compute(b"goodbye");
    let err = v.verify(body, &wrong).expect_err("mismatch");
    assert_eq!(err.code(), COR_CAS_DIGEST_MISMATCH);
    match err {
        VerifyError::DigestMismatch { expected, computed } => {
            assert_eq!(expected, wrong.to_hex());
            assert_eq!(computed, Digest::compute(body).to_hex());
        }
        VerifyError::VerifyDisabled => panic!("default verifier must verify"),
        _ => panic!("unexpected non-exhaustive variant"),
    }
}

#[test]
fn disabled_increments_counter_and_errs() {
    // Lock to serialize against other counter-asserting tests.
    let _g = COUNTER_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = opt_out_total();
    let v_warn = ClientVerifier::new(VerifyConfig::disabled());
    let after = opt_out_total();
    assert_eq!(after, before + 1);

    let body = b"hello world";
    let d = Digest::compute(body);
    let err = v_warn.verify(body, &d).expect_err("disabled errors");
    assert!(matches!(err, VerifyError::VerifyDisabled));
    assert_eq!(err.code(), COR_CAS_VERIFY_DISABLED);
}

#[test]
fn opt_out_counter_monotonic_under_repeated_construction() {
    // Lock to serialize against other counter-asserting tests.
    let _g = COUNTER_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let start = opt_out_total();
    for _ in 0..10 {
        // Use `disabled()` (warn-on-optout=true) directly: the warn
        // log target is filtered out by default in tests so the
        // surface stays quiet, but the counter still ticks.
        let _ = ClientVerifier::new(VerifyConfig::disabled());
    }
    let end = opt_out_total();
    assert_eq!(end, start + 10);
}

/// Counter does NOT tick when constructing default-on verifiers.
/// Serialized via `COUNTER_TEST_MUTEX` to keep the delta clean.
#[test]
fn enabled_construction_does_not_tick_counter() {
    let _g = COUNTER_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let start = opt_out_total();
    for _ in 0..100 {
        let _ = ClientVerifier::default_on();
    }
    assert_eq!(opt_out_total(), start);
}

#[test]
fn digest_len_constant_matches_blake3() {
    assert_eq!(DIGEST_LEN, 32);
}

// ---------------------------------------------------------------------------
// Property tests — 10k iter per AC §8
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    /// Every (body, BLAKE3(body)) pair verifies OK. 10k iter.
    #[test]
    fn prop_verify_match_always_ok(body in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let v = ClientVerifier::default_on();
        let d = Digest::compute(&body);
        prop_assert!(v.verify(&body, &d).is_ok());
    }

    /// Every (body, wrong_digest) pair errors with DigestMismatch.
    /// We construct `wrong` by flipping a chosen byte of the truth so
    /// the wrong digest is guaranteed to differ. 10k iter.
    #[test]
    fn prop_verify_mismatch_always_err(
        body in proptest::collection::vec(any::<u8>(), 0..4096),
        flip_byte in 0u8..32,
        flip_mask in 1u8..=255,
    ) {
        let v = ClientVerifier::default_on();
        let truth = Digest::compute(&body);
        let mut wrong_bytes = *truth.as_bytes();
        wrong_bytes[flip_byte as usize] ^= flip_mask;
        let wrong = Digest::from_hex(&hex::encode(wrong_bytes)).expect("hex");
        let err = v.verify(&body, &wrong).expect_err("must mismatch");
        match err {
            VerifyError::DigestMismatch { expected, computed } => {
                prop_assert_eq!(expected, wrong.to_hex());
                prop_assert_eq!(computed, truth.to_hex());
            }
            VerifyError::VerifyDisabled => prop_assert!(false, "default verifier must verify"),
            _ => prop_assert!(false, "unexpected non-exhaustive variant"),
        }
    }

    /// Default verifier is always default-on regardless of repeated
    /// construction (no static or env knob can flip it).
    #[test]
    fn prop_default_always_enabled(_seed in 0u64..u64::MAX) {
        let v = ClientVerifier::default();
        prop_assert!(v.config().enabled());
        prop_assert!(v.config().warn_on_optout());
    }

    /// Disabled verifier always errs with VerifyDisabled regardless
    /// of whether the digest would otherwise match.
    #[test]
    fn prop_disabled_always_errs(body in proptest::collection::vec(any::<u8>(), 0..512)) {
        // Use the silent variant in the property loop so we don't
        // spam the tracing subscriber 10k times.
        let v = ClientVerifier::new(VerifyConfig::default());
        prop_assume!(v.config().enabled());

        let v2_cfg_silent_disabled = VerifyConfig::disabled();
        let v2 = ClientVerifier::new(v2_cfg_silent_disabled);
        let d = Digest::compute(&body);
        let err = v2.verify(&body, &d).expect_err("disabled");
        prop_assert!(matches!(err, VerifyError::VerifyDisabled));
    }
}

// ---------------------------------------------------------------------------
// Constant-time variance gate (release-only) — AC §8 partial-match
// timing-channel test, mirroring the corelink-hash gate but going
// through the public `ClientVerifier::verify` entry point so we cover
// the integration layer (subtle::ConstantTimeEq + Digest::verify_constant_time).
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only timing-channel gate; run via `cargo test --release`"
)]
fn constant_time_variance_on_verify() {
    // The CT gate measures the *constant-time-compare branch* —
    // mismatch probes vs each other. Four probe classes:
    // - zero_digest:        no shared bytes with the truth
    // - first_byte_diff:    differs at index 0 (early-exit attack)
    // - middle_byte_diff:   differs at index 15 (mid-buffer attack)
    // - last_byte_diff:     differs at index 31 (longest shared prefix)
    //
    // All four return Err(DigestMismatch) so the post-compare
    // control flow is identical (both legs allocate two hex
    // strings). A constant-time compare must produce statistically
    // indistinguishable timings across these four probes.
    //
    // We do NOT include an `exact_match` probe in the gate because
    // its post-compare path is structurally different (returns
    // Ok(()), no hex allocation). Comparing it against mismatch
    // probes would conflate the constant-time *compare* signal
    // with the *control-flow* difference between Ok and Err — a
    // separate contract documented in WI §10. The match-vs-mismatch
    // observability is not exploitable for prefix-recovery (there's
    // no graded leakage; an attacker only learns "match or not"
    // which they would learn from the boolean return anyway).
    const TRIALS_PER_PROBE: usize = 800;
    const ITERS_PER_TRIAL: usize = 256;
    let body = b"the canonical body for ct-variance through verify".to_vec();
    let truth = Digest::compute(&body);

    let make_probe = |flip_index: usize| -> Digest {
        let mut bytes = *truth.as_bytes();
        bytes[flip_index] ^= 0x01;
        Digest::from_hex(&hex::encode(bytes)).expect("hex")
    };
    let zero_digest = Digest::from_hex(&"0".repeat(64)).expect("hex");
    let first_byte_diff = make_probe(0);
    let middle_byte_diff = make_probe(15);
    let last_byte_diff = make_probe(31);

    let probes: [(&str, &Digest); 4] = [
        ("zero", &zero_digest),
        ("first_byte_diff", &first_byte_diff),
        ("middle_byte_diff", &middle_byte_diff),
        ("last_byte_diff", &last_byte_diff),
    ];

    let v = ClientVerifier::default_on();

    let mut samples: Vec<Vec<u128>> = vec![Vec::with_capacity(TRIALS_PER_PROBE); probes.len()];

    // Interleave probes so any external scheduler / thermal
    // perturbation hits every class equally.
    for trial in 0..TRIALS_PER_PROBE {
        for (i, (_label, probe)) in probes.iter().enumerate() {
            let start = Instant::now();
            for _ in 0..ITERS_PER_TRIAL {
                let _ = std::hint::black_box(v.verify(std::hint::black_box(&body), probe));
            }
            let elapsed = start.elapsed().as_nanos();
            #[allow(clippy::indexing_slicing, reason = "probes.len() == samples.len()")]
            samples[i].push(elapsed);
            // Avoid accidentally compiling out the trial counter
            // even at -O3.
            std::hint::black_box(trial);
        }
    }

    let mean = |xs: &[u128]| -> f64 {
        let n = xs.len() as f64;
        xs.iter().map(|&x| x as f64).sum::<f64>() / n
    };
    #[allow(clippy::indexing_slicing, reason = "fixed indices match the probes array")]
    let baseline_mean = mean(&samples[0]);

    for (i, (label, _)) in probes.iter().enumerate() {
        #[allow(clippy::indexing_slicing, reason = "iterating in lockstep with samples")]
        let m = mean(&samples[i]);
        let delta = (m - baseline_mean).abs() / baseline_mean.max(m);
        eprintln!("ct-variance(verify): {label}={m:.0}ns delta_vs_zero={:.3}%", delta * 100.0);
        // The 5% gate is a release-mode statistical check; in
        // practice subtle::ConstantTimeEq holds well below this
        // threshold on all amd64 / aarch64 hosts we have measured.
        assert!(
            delta < 0.05,
            "constant-time variance for probe {label} = {:.3}% exceeds 5% gate",
            delta * 100.0
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only structural-symmetry gate; run via `cargo test --release`"
)]
fn match_vs_mismatch_control_flow_documented() {
    // Documents the structural timing difference between match (Ok)
    // and mismatch (Err) paths. This is NOT a constant-time gate —
    // the difference is the post-compare control flow (no hex
    // allocation in the Ok arm). The test exists so a future
    // engineer who sees the asymmetry knows it is intentional and
    // documented, and a regression that reverses the relationship
    // (mismatch faster than match) would surface as a panic.
    const TRIALS: usize = 400;
    const ITERS_PER_TRIAL: usize = 256;
    let body = b"the canonical body for ct-variance documentation".to_vec();
    let truth = Digest::compute(&body);
    let mut wrong = *truth.as_bytes();
    wrong[31] ^= 0x01;
    let wrong_d = Digest::from_hex(&hex::encode(wrong)).expect("hex");

    let v = ClientVerifier::default_on();

    let measure = |probe: &Digest| -> f64 {
        let mut samples = Vec::with_capacity(TRIALS);
        for _ in 0..TRIALS {
            let start = Instant::now();
            for _ in 0..ITERS_PER_TRIAL {
                let _ = std::hint::black_box(v.verify(std::hint::black_box(&body), probe));
            }
            samples.push(start.elapsed().as_nanos());
        }
        let n = samples.len() as f64;
        samples.iter().map(|&x| x as f64).sum::<f64>() / n
    };

    let m_match = measure(&truth);
    let m_mismatch = measure(&wrong_d);
    eprintln!("match-vs-mismatch: match={m_match:.0}ns mismatch={m_mismatch:.0}ns");

    // Mismatch must be measurably slower (it allocates 2 hex
    // strings and constructs a structured error). If this ordering
    // ever inverts, something has changed about the verify path
    // and the documentation must be updated.
    assert!(
        m_mismatch > m_match,
        "regression: mismatch ({m_mismatch:.0}ns) is no longer slower than match ({m_match:.0}ns)"
    );
}
