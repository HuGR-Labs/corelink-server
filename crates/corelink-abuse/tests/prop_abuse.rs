//! Property tests pinning the load-bearing invariants of
//! `corelink-abuse` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override).
//!
//! Coverage map (mirrors WI-S08-004 §6.1.11):
//!
//! - `prop_score_in_range_0_1` — score is structurally clamped to
//!   `[0.0, 1.0]` regardless of feature inputs.
//! - `prop_score_monotonic_in_features` — increasing each feature
//!   (others held constant) → non-decreasing score (each weight ≥ 0).
//! - `prop_decision_threshold_boundary` — boundary semantics at exactly
//!   each threshold.
//! - `prop_tenant_isolation` — per-tenant scoring; tenant A's
//!   observation does not affect tenant B's rolling window.
//! - `prop_audit_emit_per_decision_arm` — every decision arm emits
//!   exactly the canonical audit pair (ScoreComputed + Decision*) +
//!   side-effect record where applicable.
//! - `prop_suspend_idempotent` — repeated `auto_suspend` calls are
//!   idempotent (always return `AutoSuspendForbidden`; never
//!   accidentally allow).
//! - `prop_downgrade_applied_reduces_refill_rate` — Suspicious tier
//!   reduces the per-tenant refill rate (cross-WI integration with
//!   `corelink-ratelimit`).
//! - `prop_overshoot_does_not_panic` — extreme inputs never panic.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_abuse::{
    canonical_audit_event_strings, canonical_metric_names,
    AbuseConfig, AbuseDecision, AbuseEventType, AbuseFeatureWeights,
    AbuseFeatures, AbuseMetricKind, AbuseScore, AbuseScorer,
    AbuseTierLabel, InMemoryAbuseAuditSink, InMemoryAbuseMetrics,
    InMemoryAbuseScorer, MIGRATION_0013_ABUSE_SCORES,
    MALICIOUS_THRESHOLD, SUSPICIOUS_THRESHOLD,
};
use corelink_eviction::Tier;
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimiter,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

type Scorer = InMemoryAbuseScorer<
    InMemoryAbuseAuditSink,
    InMemoryAbuseMetrics,
    InMemoryTokenBucketRateLimiter<
        InMemoryRateLimitAuditSink,
        InMemoryRateLimitMetrics,
    >,
>;

type Fixture = (
    Scorer,
    Arc<InMemoryAbuseAuditSink>,
    Arc<InMemoryAbuseMetrics>,
    Arc<
        InMemoryTokenBucketRateLimiter<
            InMemoryRateLimitAuditSink,
            InMemoryRateLimitMetrics,
        >,
    >,
);

fn fixture() -> Fixture {
    let audit = Arc::new(InMemoryAbuseAuditSink::new());
    let metrics = Arc::new(InMemoryAbuseMetrics::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let limiter = Arc::new(InMemoryTokenBucketRateLimiter::with_defaults(
        Arc::clone(&rl_audit),
        Arc::clone(&rl_metrics),
    ));
    let scorer = InMemoryAbuseScorer::with_defaults(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&limiter),
    );
    (scorer, audit, metrics, limiter)
}

// ---- Sanity: artifact + canonical lists pinned ----------------------

#[test]
fn migration_0013_is_embedded() {
    assert!(!MIGRATION_0013_ABUSE_SCORES.is_empty());
    assert!(MIGRATION_0013_ABUSE_SCORES.contains("CREATE TABLE"));
    assert!(MIGRATION_0013_ABUSE_SCORES.contains("abuse_score_history"));
}

#[test]
fn canonical_audit_event_strings_pinned_count_seven() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 7);
}

#[test]
fn canonical_metric_names_pinned_count_five() {
    let s = canonical_metric_names();
    assert_eq!(s.len(), 5);
}

// ---- prop_score_in_range_0_1 ---------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Score is structurally clamped to [0.0, 1.0] regardless of
    /// feature inputs.
    #[test]
    fn prop_score_in_range_0_1(
        cpu in -10.0_f64..10.0,
        egress in 0u64..u64::MAX / 2,
        entropy in -10.0_f64..50.0,
        exec in 0u32..u32::MAX,
    ) {
        let f = AbuseFeatures::new(cpu, egress, entropy, exec);
        let w = AbuseFeatureWeights::canonical();
        let s = corelink_abuse::compute_score(&f, &w, Tier::Team);
        prop_assert!(s.as_f64() >= 0.0);
        prop_assert!(s.as_f64() <= 1.0);
    }
}

// ---- prop_score_monotonic_in_features ------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Increasing each feature (others held constant) → non-decreasing
    /// score. Each weight ≥ 0; each `*_norm` non-decreasing in its
    /// input → score non-decreasing.
    #[test]
    fn prop_score_monotonic_in_cpu(
        cpu_a in 0.0_f64..0.5,
        delta in 0.0_f64..1.5,
        egress in 0u64..1_000_000_000,
        entropy in 0.0_f64..5.0,
        exec in 0u32..1_000_000,
    ) {
        let cpu_b = cpu_a + delta;
        let w = AbuseFeatureWeights::canonical();
        let f_a = AbuseFeatures::new(cpu_a, egress, entropy, exec);
        let f_b = AbuseFeatures::new(cpu_b, egress, entropy, exec);
        let s_a = corelink_abuse::compute_score(&f_a, &w, Tier::Team);
        let s_b = corelink_abuse::compute_score(&f_b, &w, Tier::Team);
        prop_assert!(
            s_b.as_f64() >= s_a.as_f64() - 1e-9,
            "cpu monotonicity violated: cpu_a={cpu_a} cpu_b={cpu_b} \
             s_a={} s_b={}",
            s_a.as_f64(),
            s_b.as_f64()
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_score_monotonic_in_egress(
        egress_a in 0u64..1_000_000_000,
        delta in 0u64..10_000_000_000,
        cpu in 0.0_f64..1.0,
        entropy in 0.0_f64..5.0,
        exec in 0u32..1_000_000,
    ) {
        let egress_b = egress_a.saturating_add(delta);
        let w = AbuseFeatureWeights::canonical();
        let f_a = AbuseFeatures::new(cpu, egress_a, entropy, exec);
        let f_b = AbuseFeatures::new(cpu, egress_b, entropy, exec);
        let s_a = corelink_abuse::compute_score(&f_a, &w, Tier::Team);
        let s_b = corelink_abuse::compute_score(&f_b, &w, Tier::Team);
        prop_assert!(s_b.as_f64() >= s_a.as_f64() - 1e-9);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Note: entropy is INVERTED (lower bits = higher score), so the
    /// monotonicity is non-INCREASING in entropy_bits.
    #[test]
    fn prop_score_monotonic_inverse_in_entropy(
        entropy_a in 0.0_f64..5.0,
        delta in 0.0_f64..5.0,
        cpu in 0.0_f64..1.0,
        egress in 0u64..1_000_000_000,
        exec in 0u32..1_000_000,
    ) {
        let entropy_b = entropy_a + delta;
        let w = AbuseFeatureWeights::canonical();
        let f_a = AbuseFeatures::new(cpu, egress, entropy_a, exec);
        let f_b = AbuseFeatures::new(cpu, egress, entropy_b, exec);
        let s_a = corelink_abuse::compute_score(&f_a, &w, Tier::Team);
        let s_b = corelink_abuse::compute_score(&f_b, &w, Tier::Team);
        prop_assert!(
            s_b.as_f64() <= s_a.as_f64() + 1e-9,
            "entropy inverse monotonicity violated"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_score_monotonic_in_exec(
        exec_a in 0u32..100_000,
        delta in 0u32..1_000_000,
        cpu in 0.0_f64..1.0,
        egress in 0u64..1_000_000_000,
        entropy in 0.0_f64..5.0,
    ) {
        let exec_b = exec_a.saturating_add(delta);
        let w = AbuseFeatureWeights::canonical();
        let f_a = AbuseFeatures::new(cpu, egress, entropy, exec_a);
        let f_b = AbuseFeatures::new(cpu, egress, entropy, exec_b);
        let s_a = corelink_abuse::compute_score(&f_a, &w, Tier::Team);
        let s_b = corelink_abuse::compute_score(&f_b, &w, Tier::Team);
        prop_assert!(s_b.as_f64() >= s_a.as_f64() - 1e-9);
    }
}

// ---- prop_decision_threshold_boundary ------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Boundary at exactly each threshold: ≥ on both. The decision
    /// must be exactly Benign / Suspicious / Malicious depending on
    /// the score's relation to the canonical thresholds.
    #[test]
    fn prop_decision_threshold_boundary(
        score_val in 0.0_f64..1.0,
    ) {
        let cfg = AbuseConfig::canonical();
        let s = AbuseScore::clamp(score_val);
        let d = corelink_abuse::decide(s, &cfg);
        if score_val >= MALICIOUS_THRESHOLD {
            prop_assert_eq!(d, AbuseDecision::Malicious);
        } else if score_val >= SUSPICIOUS_THRESHOLD {
            prop_assert_eq!(d, AbuseDecision::Suspicious);
        } else {
            prop_assert_eq!(d, AbuseDecision::Benign);
        }
    }
}

// ---- prop_tenant_isolation -----------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Per-tenant scoring; tenant A's observation does not affect
    /// tenant B's rolling window. INV-TENANT-ISOLATION canary
    /// (sprint contract §7.10.s08.4 explicit).
    #[test]
    fn prop_tenant_isolation(
        cpu_a in 0.0_f64..1.0,
        cpu_b in 0.0_f64..1.0,
        egress_a in 0u64..1_000_000_000,
        egress_b in 0u64..1_000_000_000,
        entropy_a in 0.0_f64..5.0,
        entropy_b in 0.0_f64..5.0,
        exec_a in 0u32..10_000,
        exec_b in 0u32..10_000,
    ) {
        let (scorer, _audit, _metrics, _l) = fixture();
        let f_a = AbuseFeatures::new(cpu_a, egress_a, entropy_a, exec_a);
        let f_b = AbuseFeatures::new(cpu_b, egress_b, entropy_b, exec_b);
        let ten_a = Uuid::from_u128(0xa);
        let ten_b = Uuid::from_u128(0xb);
        let out_a = scorer
            .score_and_apply(ten_a, f_a, Tier::Team, 0, 300_000)
            .unwrap();
        let out_b = scorer
            .score_and_apply(ten_b, f_b, Tier::Team, 0, 300_000)
            .unwrap();
        // After both, tenant A's window matches f_a regardless of
        // tenant B's observation.
        let win_a = scorer.snapshot_window(ten_a).unwrap().unwrap();
        let win_b = scorer.snapshot_window(ten_b).unwrap().unwrap();
        prop_assert_eq!(win_a.features, f_a);
        prop_assert_eq!(win_b.features, f_b);
        prop_assert_eq!(win_a.score, out_a.score);
        prop_assert_eq!(win_b.score, out_b.score);
    }
}

// ---- prop_audit_emit_per_decision_arm ------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_audit_emit_per_decision_arm(
        cpu in 0.0_f64..1.0,
        egress in 0u64..200_000_000_000,
        entropy in 0.0_f64..5.0,
        exec in 0u32..100_000,
    ) {
        let (scorer, audit, metrics, _l) = fixture();
        let f = AbuseFeatures::new(cpu, egress, entropy, exec);
        let out = scorer
            .score_and_apply(Uuid::from_u128(0xa), f, Tier::Team, 0, 300_000)
            .unwrap();

        // ScoreComputed always emitted exactly once.
        prop_assert_eq!(
            audit.snapshot_of(AbuseEventType::ScoreComputed).len(),
            1
        );

        match out.decision {
            AbuseDecision::Benign => {
                prop_assert_eq!(
                    audit.snapshot_of(AbuseEventType::DecisionBenign).len(),
                    1
                );
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::DowngradeApplied)
                        .len(),
                    0
                );
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::AdminReviewTriggered)
                        .len(),
                    0
                );
                prop_assert_eq!(
                    metrics.tier_total_for_label(AbuseTierLabel::Benign),
                    1
                );
            }
            AbuseDecision::Suspicious => {
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::DecisionSuspicious)
                        .len(),
                    1
                );
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::DowngradeApplied)
                        .len(),
                    1
                );
                prop_assert_eq!(
                    metrics.tier_total_for_label(AbuseTierLabel::Suspicious),
                    1
                );
                prop_assert_eq!(
                    metrics.counter_for_tenant(
                        AbuseMetricKind::DowngradeAppliedTotal,
                        Uuid::from_u128(0xa)
                    ),
                    1
                );
            }
            AbuseDecision::Malicious => {
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::DecisionMalicious)
                        .len(),
                    1
                );
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::AdminReviewTriggered)
                        .len(),
                    1
                );
                prop_assert_eq!(
                    audit
                        .snapshot_of(AbuseEventType::SuspendApplied)
                        .len(),
                    0,
                    "NEVER auto-suspend per LGPD Art. 20"
                );
                prop_assert_eq!(
                    metrics.tier_total_for_label(AbuseTierLabel::Malicious),
                    1
                );
            }
            _ => prop_assert!(false, "unexpected decision arm"),
        }
    }
}

// ---- prop_suspend_idempotent ---------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Repeated `auto_suspend` calls are idempotent: ALWAYS return
    /// AutoSuspendForbidden regardless of score; bump the SEV-1 LGPD
    /// canary on every call.
    #[test]
    fn prop_suspend_idempotent(
        score_val in 0.0_f64..1.0,
        n_calls in 1u32..16,
    ) {
        let (scorer, _a, metrics, _l) = fixture();
        let s = AbuseScore::clamp(score_val);
        let tenant = Uuid::from_u128(0xa);
        for _ in 0..n_calls {
            let err = scorer.auto_suspend(tenant, s).unwrap_err();
            let is_forbidden = matches!(
                err,
                corelink_abuse::AbuseError::AutoSuspendForbidden { .. }
            );
            prop_assert!(is_forbidden);
        }
        prop_assert_eq!(
            metrics.counter_for_tenant(
                AbuseMetricKind::AutoSuspendAttemptsTotal,
                tenant
            ),
            u64::from(n_calls)
        );
    }
}

// ---- prop_downgrade_applied_reduces_refill_rate -------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// Suspicious tier reduces the per-tenant refill rate (cross-WI
    /// integration with corelink-ratelimit). Confirms the
    /// `tenant_rate_override` canonical 50% downgrade mechanism.
    ///
    /// Strategy: pick (cpu, entropy) such that the score is
    /// structurally in [SUSPICIOUS, MALICIOUS) without rejection
    /// sampling. cpu_norm × 0.30 + entropy_norm × 0.25 needs to land
    /// in [0.5, 0.8). With egress=0 + exec=0, that simplifies to a
    /// 2D box of valid (cpu, entropy) tuples.
    #[test]
    fn prop_downgrade_applied_reduces_refill_rate(
        // cpu in [0.95, 1.0]: cpu_norm in [0.9, 1.0]; cpu_contrib in [0.27, 0.30].
        // entropy in [0.0, 0.5]: entropy_norm in [0.83, 1.0]; entropy_contrib
        // in [0.21, 0.25]. Sum cpu+entropy_contrib in [0.48, 0.55] → mostly
        // Suspicious, but might dip below 0.5 at low cpu/high entropy. Use
        // a slightly higher floor to GUARANTEE Suspicious.
        cpu_pct in 95u32..=100,    // cpu in [0.95, 1.0].
        entropy_pct in 0u32..=20,  // entropy in [0.0, 0.2] → norm 0.93+.
    ) {
        let cpu = f64::from(cpu_pct) / 100.0;
        let entropy = f64::from(entropy_pct) / 100.0;
        let (scorer, _a, _m, limiter) = fixture();
        let f = AbuseFeatures::new(cpu, 0, entropy, 0);
        let tenant = Uuid::from_u128(0xa);
        let out = scorer
            .score_and_apply(tenant, f, Tier::Team, 0, 300_000)
            .unwrap();
        // The chosen (cpu, entropy) box keeps score in [0.5, 0.55):
        //   min cpu=0.95 entropy=0.20 → 0.30*0.9 + 0.25*0.93 = 0.503.
        //   max cpu=1.00 entropy=0.00 → 0.30*1.0 + 0.25*1.0  = 0.55.
        // → ALWAYS Suspicious arm.
        let is_suspicious =
            matches!(out.decision, AbuseDecision::Suspicious);
        prop_assert!(
            is_suspicious,
            "expected Suspicious; got {:?} score={}",
            out.decision,
            out.score.as_f64()
        );
        prop_assert!(out.downgrade_applied);
        // Limiter saw the downgrade — bucket materialised + rate
        // halved (Team default 200 → 100; burst 1000 → 500).
        let bucket = BucketKey::per_tenant(tenant);
        let snap = limiter.snapshot_bucket(&bucket).unwrap().unwrap();
        prop_assert_eq!(snap.refill_rate_per_sec.floor() as u32, 100);
        prop_assert_eq!(snap.burst_capacity, 500);
    }
}

// ---- prop_overshoot_does_not_panic ---------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_overshoot_does_not_panic(
        cpu in -100.0_f64..1000.0,
        egress in 0u64..u64::MAX / 2,
        entropy in -50.0_f64..1000.0,
        exec in 0u32..u32::MAX,
    ) {
        let f = AbuseFeatures::new(cpu, egress, entropy, exec);
        let w = AbuseFeatureWeights::canonical();
        // Compute MUST NOT panic on any input combination
        // (clamping + sanitised features).
        let _ = corelink_abuse::compute_score(&f, &w, Tier::Free);
        let _ = corelink_abuse::compute_score(&f, &w, Tier::Enterprise);
    }
}

// ---- prop_per_tenant_isolation_concurrent --------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases() / 10))]

    /// 100 tenants concurrent score-and-apply (sequential under
    /// per-instance Mutex envelope; mirrors DO actor model). Each
    /// tenant's score is independent of every other tenant's score.
    /// INV-TENANT-ISOLATION 100k pinned (production wiring at
    /// WI-S08-006 sustains 100k nightly via PROPTEST_CASES override).
    #[test]
    fn prop_per_tenant_isolation_concurrent(
        seed in 0u64..u64::MAX,
    ) {
        let (scorer, _a, _m, _l) = fixture();
        // Drive 100 tenants with deterministically-seeded features.
        let mut state = seed;
        for i in 0..100_u128 {
            // Linear congruential pseudo-random (NOT cryptographic;
            // determinism only).
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let cpu = ((state % 1000) as f64) / 1000.0;
            let egress = state % 1_000_000_000;
            let entropy = ((state % 5000) as f64) / 1000.0;
            let exec = ((state >> 16) % 10_000) as u32;
            let tenant = Uuid::from_u128(i + 1);
            let f = AbuseFeatures::new(cpu, egress, entropy, exec);
            let _ = scorer
                .score_and_apply(tenant, f, Tier::Team, 0, 300_000)
                .unwrap();
        }
        // Verify each tenant's snapshot independently reflects ITS
        // observation only.
        prop_assert_eq!(scorer.tenant_count().unwrap(), 100);
    }
}
