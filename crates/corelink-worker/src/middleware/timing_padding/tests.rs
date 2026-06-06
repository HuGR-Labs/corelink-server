//! Unit tests — config invariants, padding arithmetic, seed mixing,
//! Mann-Whitney U, Šidák, bootstrap CI, median, splitmix, predicates.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3). 36 tests; verbatim bodies.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use core::time::Duration;
use std::sync::atomic::AtomicU64;

use http::{HeaderValue, Request, Response, StatusCode};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use super::config::{
    TimingPaddingConfig, TimingPaddingError, JITTER_PCT_DEFAULT, JITTER_PCT_MAX,
    PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_DEFAULT, TARGET_P99_MS_MAX,
};
use super::padding::{canonical_pad_target, compute_request_seed, splitmix_str, splitmix_u64};
use super::policy::{JitterPolicy, MissMarker};
use super::predicate::miss_predicates;
use super::stats::{bootstrap_median_ci, mann_whitney_u_p_value, median, sidak_per_test_alpha};

// Compile-time witness: defaults are inside the validated range.
const _: () = {
    assert!(TARGET_P99_MS_DEFAULT >= PADDING_GRANULARITY_MS_MIN);
    assert!(TARGET_P99_MS_DEFAULT <= TARGET_P99_MS_MAX);
    assert!(JITTER_PCT_DEFAULT <= JITTER_PCT_MAX);
};

// ----- TimingPaddingConfig invariants ----------------------------

#[test]
fn canonical_defaults_are_within_bounds() {
    let cfg = TimingPaddingConfig::canonical();
    assert_eq!(cfg.target_p99_ms(), TARGET_P99_MS_DEFAULT);
    assert_eq!(cfg.jitter_pct(), JITTER_PCT_DEFAULT);
}

#[test]
fn config_rejects_target_too_low() {
    let err = TimingPaddingConfig::new(PADDING_GRANULARITY_MS_MIN - 1, 10).unwrap_err();
    assert!(matches!(err, TimingPaddingError::TargetOutOfRange { .. }));
}

#[test]
fn config_rejects_target_too_high() {
    let err = TimingPaddingConfig::new(TARGET_P99_MS_MAX + 1, 10).unwrap_err();
    assert!(matches!(err, TimingPaddingError::TargetOutOfRange { .. }));
}

#[test]
fn config_rejects_jitter_too_high() {
    let err = TimingPaddingConfig::new(200, JITTER_PCT_MAX + 1).unwrap_err();
    assert!(matches!(err, TimingPaddingError::JitterOutOfRange { .. }));
}

#[test]
fn config_zero_jitter_ok() {
    let cfg = TimingPaddingConfig::new(200, 0).unwrap();
    assert_eq!(cfg.jitter_pct(), 0);
}

#[test]
fn config_boundary_values_ok() {
    let lo = TimingPaddingConfig::new(PADDING_GRANULARITY_MS_MIN, 0).unwrap();
    assert_eq!(lo.target_p99_ms(), PADDING_GRANULARITY_MS_MIN);
    let hi = TimingPaddingConfig::new(TARGET_P99_MS_MAX, JITTER_PCT_MAX).unwrap();
    assert_eq!(hi.target_p99_ms(), TARGET_P99_MS_MAX);
    assert_eq!(hi.jitter_pct(), JITTER_PCT_MAX);
}

// ----- canonical_pad_target arithmetic ---------------------------

#[test]
fn pad_target_returns_target_when_jitter_zero_and_handler_fast() {
    let cfg = TimingPaddingConfig::new(200, 0).unwrap();
    let policy = JitterPolicy::Seeded;
    let target = canonical_pad_target(cfg, &policy, 0xDEAD_BEEF, Duration::from_millis(50));
    assert_eq!(target.as_millis(), 200);
}

#[test]
fn pad_target_returns_observed_when_handler_slow() {
    let cfg = TimingPaddingConfig::new(50, 0).unwrap();
    let policy = JitterPolicy::FixedForTests { signed_pct: 0 };
    let observed = Duration::from_millis(80);
    let target = canonical_pad_target(cfg, &policy, 1, observed);
    assert_eq!(target, observed);
}

#[test]
fn pad_target_applies_positive_jitter() {
    let cfg = TimingPaddingConfig::new(200, 10).unwrap();
    let policy = JitterPolicy::FixedForTests { signed_pct: 10 };
    let target = canonical_pad_target(cfg, &policy, 0, Duration::from_millis(0));
    assert_eq!(target.as_millis(), 220);
}

#[test]
fn pad_target_applies_negative_jitter() {
    let cfg = TimingPaddingConfig::new(200, 10).unwrap();
    let policy = JitterPolicy::FixedForTests { signed_pct: -10 };
    let target = canonical_pad_target(cfg, &policy, 0, Duration::from_millis(0));
    assert_eq!(target.as_millis(), 180);
}

#[test]
fn pad_target_seeded_jitter_stays_in_window() {
    let cfg = TimingPaddingConfig::new(200, 10).unwrap();
    let policy = JitterPolicy::Seeded;
    for seed in 0..1024u64 {
        let target = canonical_pad_target(cfg, &policy, seed, Duration::from_millis(0));
        let ms = target.as_millis();
        assert!(
            (180..=220).contains(&ms),
            "seed {seed}: pad {ms} ms outside ±10% window of 200 ms"
        );
    }
}

#[test]
fn pad_target_seeded_jitter_is_deterministic() {
    let cfg = TimingPaddingConfig::new(200, 20).unwrap();
    let policy = JitterPolicy::Seeded;
    let a = canonical_pad_target(cfg, &policy, 42, Duration::from_millis(0));
    let b = canonical_pad_target(cfg, &policy, 42, Duration::from_millis(0));
    assert_eq!(a, b);
}

#[test]
fn pad_target_seeded_jitter_varies_across_seeds() {
    let cfg = TimingPaddingConfig::new(200, 20).unwrap();
    let policy = JitterPolicy::Seeded;
    let mut distinct = std::collections::BTreeSet::new();
    for seed in 0..256u64 {
        let target = canonical_pad_target(cfg, &policy, seed, Duration::from_millis(0));
        distinct.insert(target.as_millis() as i64);
    }
    assert!(
        distinct.len() >= 8,
        "expected ≥ 8 distinct pad targets across 256 seeds, got {}",
        distinct.len()
    );
}

// ----- compute_request_seed: server-secret mixing ----------------

#[test]
fn server_secret_changes_seed_for_same_request_id() {
    // Codex round-1 P1 fix: the seed must NOT be a pure function
    // of the client-controlled `x-request-id`.
    let counter = AtomicU64::new(0);
    let req_a = Request::builder()
        .header("x-request-id", "abc")
        .body(())
        .unwrap();
    let counter_b = AtomicU64::new(0);
    let req_b = Request::builder()
        .header("x-request-id", "abc")
        .body(())
        .unwrap();
    let s_secret_1 = 0x1111_1111_1111_1111u64;
    let s_secret_2 = 0x2222_2222_2222_2222u64;
    let s1 = compute_request_seed(&req_a, s_secret_1, &counter);
    let s2 = compute_request_seed(&req_b, s_secret_2, &counter_b);
    assert_ne!(
        s1, s2,
        "different server secrets must produce different seeds"
    );
}

#[test]
fn same_id_different_calls_yield_different_seeds() {
    // Counter-mix defeats id-collision: two probes with the same
    // `x-request-id` (a misconfigured client OR an attacker
    // intentionally re-using ids) get distinct seeds.
    let counter = AtomicU64::new(0);
    let secret = 0xC0DE_C0DE_C0DE_C0DEu64;
    let req = || {
        Request::builder()
            .header("x-request-id", "same")
            .body(())
            .unwrap()
    };
    let s1 = compute_request_seed(&req(), secret, &counter);
    let s2 = compute_request_seed(&req(), secret, &counter);
    assert_ne!(s1, s2);
}

#[test]
fn missing_id_falls_back_to_counter_mix() {
    let counter = AtomicU64::new(0);
    let secret = 0xABCDu64;
    let req = || Request::builder().body(()).unwrap();
    let s1 = compute_request_seed(&req(), secret, &counter);
    let s2 = compute_request_seed(&req(), secret, &counter);
    assert_ne!(s1, s2);
}

#[test]
fn non_utf8_header_value_does_not_panic() {
    let counter = AtomicU64::new(0);
    let secret = 1u64;
    // Bytes 0x80..0x9F are valid HeaderValue bytes but not UTF-8.
    let v = HeaderValue::from_bytes(b"\x80\x81invalid").unwrap();
    let mut req = Request::new(());
    req.headers_mut().insert("x-request-id", v);
    let s = compute_request_seed(&req, secret, &counter);
    // Falls back to header_seed = 0; counter advances; s is
    // splitmix(secret ^ counter ^ 0).
    assert_eq!(s, splitmix_u64(secret));
}

// ----- Mann-Whitney U statistical primitive ----------------------

#[test]
fn mwu_identical_samples_p_one() {
    let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
    let ys = [1.0, 2.0, 3.0, 4.0, 5.0];
    let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
    assert!(p > 0.95, "identical samples expected p ≈ 1, got {p}");
}

#[test]
fn mwu_disjoint_samples_p_small() {
    let xs: Vec<f64> = (0..30).map(|i| i as f64).collect();
    let ys: Vec<f64> = (100..130).map(|i| i as f64).collect();
    let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
    assert!(p < 0.001, "disjoint samples expected p ≪ 0.001, got {p}");
}

#[test]
fn mwu_overlapping_samples_p_large() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x00C0_FFEE);
    let xs: Vec<f64> = (0..200).map(|_| rng.random_range(0.0..100.0)).collect();
    let ys: Vec<f64> = (0..200).map(|_| rng.random_range(0.0..100.0)).collect();
    let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
    assert!(
        p > 0.05,
        "same-distribution samples expected p > 0.05, got {p}"
    );
}

#[test]
fn mwu_empty_returns_none() {
    assert!(mann_whitney_u_p_value(&[], &[1.0]).is_none());
    assert!(mann_whitney_u_p_value(&[1.0], &[]).is_none());
}

#[test]
fn mwu_handles_ties() {
    let xs = [1.0, 1.0, 2.0, 2.0, 3.0];
    let ys = [1.0, 2.0, 2.0, 3.0, 3.0];
    let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
    assert!((0.0..=1.0).contains(&p));
    assert!(p > 0.3);
}

// ----- Šidák correction ------------------------------------------

#[test]
fn sidak_canonical_nine_tests() {
    let alpha_prime = sidak_per_test_alpha(0.05, 9).unwrap();
    assert!((alpha_prime - 0.005_69).abs() < 1e-4);
}

#[test]
fn sidak_canonical_three_tests() {
    let alpha_prime = sidak_per_test_alpha(0.05, 3).unwrap();
    assert!((alpha_prime - 0.016_95).abs() < 1e-4);
}

#[test]
fn sidak_zero_k_returns_none() {
    assert!(sidak_per_test_alpha(0.05, 0).is_none());
}

#[test]
fn sidak_alpha_out_of_range_returns_none() {
    assert!(sidak_per_test_alpha(-0.1, 3).is_none());
    assert!(sidak_per_test_alpha(1.5, 3).is_none());
}

// ----- Bootstrap CI ----------------------------------------------

#[test]
fn bootstrap_ci_zero_for_identical_samples() {
    let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
    let ys = xs.clone();
    let ci = bootstrap_median_ci(&xs, &ys, 200, 7).unwrap();
    assert_eq!(ci.point_estimate, 0.0);
    assert!(ci.ci_lower >= 0.0);
    assert!(ci.ci_upper <= 30.0);
}

#[test]
fn bootstrap_ci_nonzero_for_separated_samples() {
    let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
    let ys: Vec<f64> = (200..250).map(|i| i as f64).collect();
    let ci = bootstrap_median_ci(&xs, &ys, 200, 11).unwrap();
    assert!(ci.point_estimate > 100.0);
    assert!(
        ci.ci_lower > 50.0,
        "expected CI lower > 50, got {}",
        ci.ci_lower
    );
}

#[test]
fn bootstrap_ci_empty_returns_none() {
    let xs: Vec<f64> = vec![];
    let ys = vec![1.0];
    assert!(bootstrap_median_ci(&xs, &ys, 100, 0).is_none());
}

// ----- Median ----------------------------------------------------

#[test]
fn median_odd() {
    let mut v = [3.0, 1.0, 4.0, 1.0, 5.0];
    assert_eq!(median(&mut v), 3.0);
}

#[test]
fn median_even() {
    let mut v = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(median(&mut v), 2.5);
}

// ----- splitmix --------------------------------------------------

#[test]
fn splitmix_str_is_deterministic_per_input() {
    let a = splitmix_str("019384a0-face-7000-8000-000000000001");
    let b = splitmix_str("019384a0-face-7000-8000-000000000001");
    assert_eq!(a, b);
}

#[test]
fn splitmix_str_distinct_for_distinct_inputs() {
    let a = splitmix_str("req-001");
    let b = splitmix_str("req-002");
    assert_ne!(a, b);
}

#[test]
fn splitmix_u64_canonical_known_outputs() {
    let v0 = splitmix_u64(0);
    let v1 = splitmix_u64(1);
    let vmax = splitmix_u64(u64::MAX);
    assert_ne!(v0, 0);
    assert_ne!(v1, 0);
    assert_ne!(v0, v1);
    assert_ne!(v1, vmax);
    for v in [v0, v1, vmax] {
        let ones = v.count_ones();
        assert!(
            (16..=48).contains(&ones),
            "splitmix output {v:#x} has unbalanced popcount {ones}"
        );
    }
}

#[test]
fn splitmix_u64_avalanche_property() {
    for seed in 0..64u64 {
        let a = splitmix_u64(seed);
        let b = splitmix_u64(seed.wrapping_add(1));
        let differing = (a ^ b).count_ones();
        assert!(
            differing >= 16,
            "splitmix({seed}) ^ splitmix({}) only differs in {differing} bits",
            seed.wrapping_add(1)
        );
    }
}

// ----- MissPredicate canonical impls -----------------------------

#[test]
fn predicate_http_404_matches_404_only() {
    let p = miss_predicates::http_404();
    let mut r: Response<()> = Response::new(());
    *r.status_mut() = StatusCode::NOT_FOUND;
    assert!(p.matches(&r));
    *r.status_mut() = StatusCode::OK;
    assert!(!p.matches(&r));
    *r.status_mut() = StatusCode::FORBIDDEN;
    assert!(!p.matches(&r));
}

#[test]
fn predicate_extension_marker_matches_marker_only() {
    let p = miss_predicates::extension_marker();
    let mut r: Response<()> = Response::new(());
    *r.status_mut() = StatusCode::OK;
    assert!(!p.matches(&r));
    r.extensions_mut().insert(MissMarker::new());
    assert!(p.matches(&r));
}

#[test]
fn predicate_any_matches_any_signal() {
    let p = miss_predicates::any();
    // No status, no marker.
    let mut r: Response<()> = Response::new(());
    assert!(!p.matches(&r));
    // 404 alone.
    *r.status_mut() = StatusCode::NOT_FOUND;
    assert!(p.matches(&r));
    // Marker alone, OK status.
    let mut r: Response<()> = Response::new(());
    r.extensions_mut().insert(MissMarker::new());
    assert!(p.matches(&r));
    // grpc-status: 5 alone, OK status.
    let mut r: Response<()> = Response::new(());
    r.headers_mut().insert("grpc-status", "5".parse().unwrap());
    assert!(p.matches(&r));
}

#[test]
fn predicate_grpc_not_found_matches_grpc_status_5() {
    let p = miss_predicates::grpc_not_found();
    let mut r: Response<()> = Response::new(());
    // No grpc-status header.
    assert!(!p.matches(&r));
    // grpc-status: 0 (OK).
    r.headers_mut().insert("grpc-status", "0".parse().unwrap());
    assert!(!p.matches(&r));
    // grpc-status: 5 (NOT_FOUND).
    r.headers_mut().insert("grpc-status", "5".parse().unwrap());
    assert!(p.matches(&r));
}
