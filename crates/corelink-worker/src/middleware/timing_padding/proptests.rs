//! Property tests — padding-window invariant, determinism per seed,
//! Mann-Whitney bounds, Šidák monotonicity, server-secret entropy
//! property (INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE).
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptest harness: panics surface as test failures by design"
)]

use core::time::Duration;
use std::sync::atomic::AtomicU64;

use http::Request;
use proptest::prelude::*;

use super::config::{
    TimingPaddingConfig, JITTER_PCT_MAX, PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_MAX,
};
use super::padding::{canonical_pad_target, compute_request_seed};
use super::policy::JitterPolicy;
use super::stats::{mann_whitney_u_p_value, sidak_per_test_alpha};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn padding_window_holds_for_seeded_jitter(
        target_ms in PADDING_GRANULARITY_MS_MIN..=TARGET_P99_MS_MAX,
        jitter_pct in 0u8..=JITTER_PCT_MAX,
        seed in any::<u64>(),
        elapsed_ms in 0u64..1000,
    ) {
        let cfg = TimingPaddingConfig::new(target_ms, jitter_pct).unwrap();
        let policy = JitterPolicy::Seeded;
        let pad = canonical_pad_target(
            cfg,
            &policy,
            seed,
            Duration::from_millis(elapsed_ms),
        );
        let pad_ms = pad.as_millis() as i128;
        let target_i = target_ms as i128;
        let jitter_i = jitter_pct as i128;
        let min_padded = (target_i * (100 - jitter_i)) / 100;
        let lower_bound = core::cmp::max(min_padded, elapsed_ms as i128);
        let max_padded = (target_i * (100 + jitter_i)) / 100;
        let upper_bound = core::cmp::max(max_padded, elapsed_ms as i128);
        prop_assert!(
            pad_ms >= lower_bound && pad_ms <= upper_bound,
            "pad {pad_ms} ms not in [{lower_bound}, {upper_bound}] (target {target_ms}, jitter {jitter_pct}%, elapsed {elapsed_ms})"
        );
    }

    #[test]
    fn padding_deterministic_per_seed(
        target_ms in PADDING_GRANULARITY_MS_MIN..=TARGET_P99_MS_MAX,
        jitter_pct in 0u8..=JITTER_PCT_MAX,
        seed in any::<u64>(),
    ) {
        let cfg = TimingPaddingConfig::new(target_ms, jitter_pct).unwrap();
        let policy = JitterPolicy::Seeded;
        let elapsed = Duration::from_millis(0);
        let a = canonical_pad_target(cfg, &policy, seed, elapsed);
        let b = canonical_pad_target(cfg, &policy, seed, elapsed);
        prop_assert_eq!(a, b);
    }

    #[test]
    fn mwu_bounds_hold(
        xs in proptest::collection::vec(0.0_f64..1000.0, 5..200),
        ys in proptest::collection::vec(0.0_f64..1000.0, 5..200),
    ) {
        let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
        prop_assert!(p.is_finite());
        prop_assert!((0.0..=1.0).contains(&p));
    }

    #[test]
    fn sidak_monotone_in_k(alpha in 0.001f64..=0.5, k in 1usize..50) {
        let alpha_k = sidak_per_test_alpha(alpha, k).unwrap();
        prop_assert!(alpha_k > 0.0);
        prop_assert!(alpha_k <= alpha + 1e-9);
        if k > 1 {
            let alpha_one = sidak_per_test_alpha(alpha, 1).unwrap();
            prop_assert!(alpha_k <= alpha_one + 1e-9);
        }
    }

    // INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE: server-side secret
    // entropy property — given DIFFERENT server secrets, the same
    // client-controlled `x-request-id` MUST produce DIFFERENT
    // seeds. Codex round-1 P1 fix; client must not be able to
    // pre-compute the pad.
    #[test]
    fn server_secret_dominates_client_id(
        secret_a in any::<u64>(),
        secret_b in any::<u64>(),
        id in "[a-zA-Z0-9_-]{1,64}",
    ) {
        prop_assume!(secret_a != secret_b);
        let counter_a = AtomicU64::new(0);
        let counter_b = AtomicU64::new(0);
        let req = || Request::builder().header("x-request-id", id.as_str()).body(()).unwrap();
        let sa = compute_request_seed(&req(), secret_a, &counter_a);
        let sb = compute_request_seed(&req(), secret_b, &counter_b);
        prop_assert_ne!(sa, sb);
    }
}
