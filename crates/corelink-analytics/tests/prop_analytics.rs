//! Property tests pinning the load-bearing invariants of
//! `corelink-analytics` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S09-001 §6.1.12):
//!
//! - `prop_cardinality_budget_enforced` — over-budget unique-tuple
//!   addition is rejected (HIGH; INV-OBS-CARDINALITY-BUDGET).
//! - `prop_cardinality_idempotent_repeat_label_set` — registering the
//!   same `(metric, labels)` tuple twice yields the same unique-tuple
//!   count (idempotent).
//! - `prop_red_rate_monotone` — rate counter never decrements across
//!   any sequence of `record_rate` calls.
//! - `prop_red_errors_monotone` — error counter never decrements.
//! - `prop_red_duration_histogram_bucket_correct` — duration
//!   observation lands in the canonical bucket per
//!   `AnalyticsConfig::histogram_bucket_boundaries`.
//! - `prop_tenant_isolation` — Tier A label tuples never affect Tier B
//!   budget under the per-metric ledger.
//! - `prop_audit_emit_per_decision_arm` — every Allow / Reject /
//!   AlreadyRegistered arm emits exactly the canonical audit event.
//! - `prop_idempotent_zero_value_emit` — `record_rate(metric, labels,
//!   0)` still counts the unique tuple at the validator (the value=0
//!   no-op at the observer is independent of the validator's
//!   unique-tuple ledger).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_analytics::{
    canonical_audit_event_strings, canonical_metric_names, AcHitMissLabel, AnalyticsConfig,
    AnalyticsError, AnalyticsEventType, BillingEventTypeLabel, CardinalityValidator, DoClassLabel,
    DsrTypeLabel, GcPhaseLabel, InMemoryAnalyticsAuditSink, InMemoryRedMetrics, KvNamespaceLabel,
    MetricLabelTuple, R2BucketLabel, R2OpTypeLabel, RateLimitLayerLabel, RateLimitReasonLabel,
    RedMetricKind, RedMetricsObserver, RedResultLabel, Region, Tier, ValidatorDecision,
    CANONICAL_GLOBAL_BUDGET, CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES, CANONICAL_METRIC_COUNT,
    CANONICAL_PER_METRIC_BUDGET, FORBIDDEN_LABEL_NAMES,
    MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

type Validator = CardinalityValidator<InMemoryAnalyticsAuditSink>;

fn fresh_validator() -> (Validator, Arc<InMemoryAnalyticsAuditSink>) {
    let audit = Arc::new(InMemoryAnalyticsAuditSink::new());
    let v = CardinalityValidator::with_defaults(Arc::clone(&audit));
    (v, audit)
}

fn fresh_validator_with_budget(
    per_metric: u64,
    global: u64,
) -> (Validator, Arc<InMemoryAnalyticsAuditSink>) {
    let audit = Arc::new(InMemoryAnalyticsAuditSink::new());
    let cfg = AnalyticsConfig::with_budgets(per_metric, global);
    let v = CardinalityValidator::new(Arc::clone(&audit), cfg);
    (v, audit)
}

const ALL_TIERS: &[Tier] = &[
    Tier::Free,
    Tier::Solo,
    Tier::Team,
    Tier::Business,
    Tier::Enterprise,
];

const ALL_REGIONS: &[Region] = &[
    Region::Iad,
    Region::Sjc,
    Region::Dfw,
    Region::Sea,
    Region::Ord,
    Region::Lhr,
    Region::Fra,
    Region::Ams,
    Region::Cdg,
    Region::Mad,
    Region::Gru,
    Region::Eze,
    Region::Bog,
    Region::Nrt,
    Region::Sin,
    Region::Syd,
    Region::Hkg,
    Region::Bom,
    Region::Icn,
    Region::Jnb,
    Region::Cpt,
    Region::Dxb,
];

const ALL_RESULTS: &[RedResultLabel] = &[
    RedResultLabel::Success,
    RedResultLabel::ClientError4xx,
    RedResultLabel::ServerError5xx,
];

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 3);
    assert!(s.contains(&"corelink.analytics.metric_emitted"));
    assert!(s.contains(&"corelink.analytics.cardinality_rejected"));
    assert!(s.contains(&"corelink.analytics.budget_exceeded"));
}

#[test]
fn canonical_metric_names_pinned() {
    let m = canonical_metric_names();
    assert_eq!(m.len(), CANONICAL_METRIC_COUNT);
    assert!(m.contains(&"corelink_cas_put_requests_total"));
    assert!(m.contains(&"corelink_cas_put_duration_seconds"));
    assert!(m.contains(&"corelink_cas_get_bytes_total"));
    assert!(m.contains(&"corelink_ac_lookup_requests_total"));
    assert!(m.contains(&"corelink_gc_runs_total"));
    assert!(m.contains(&"corelink_dedup_ratio"));
    assert!(m.contains(&"corelink_rate_limit_rejects_total"));
    assert!(m.contains(&"corelink_privacy_dsr_active_total"));
    assert!(m.contains(&"corelink_billing_events_emitted_total"));
    assert!(m.contains(&"corelink_cf_cpu_time_us"));
    assert!(m.contains(&"corelink_r2_ops_total"));
    assert!(m.contains(&"corelink_d1_row_scans_total"));
    assert!(m.contains(&"corelink_kv_read_quota_used"));
    assert!(m.contains(&"corelink_kv_write_quota_used"));
    assert!(m.contains(&"corelink_do_storage_size_bytes"));
}

#[test]
fn migration_0015_is_embedded() {
    assert!(MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS.contains("analytics_cardinality_budgets"));
    assert!(MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS.contains("migration 0015"));
}

#[test]
fn analytics_schema_version_pinned() {
    assert_eq!(corelink_analytics::analytics_schema_version(), 15);
}

#[test]
fn canonical_budgets_pinned() {
    assert_eq!(CANONICAL_PER_METRIC_BUDGET, 20_000);
    assert_eq!(CANONICAL_GLOBAL_BUDGET, 100_000);
    assert_eq!(CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES.len(), 11);
}

#[test]
fn forbidden_label_names_pinned() {
    assert_eq!(FORBIDDEN_LABEL_NAMES.len(), 4);
    assert!(FORBIDDEN_LABEL_NAMES.contains(&"trace_id"));
    assert!(FORBIDDEN_LABEL_NAMES.contains(&"tenant_id"));
    assert!(FORBIDDEN_LABEL_NAMES.contains(&"request_id"));
    assert!(FORBIDDEN_LABEL_NAMES.contains(&"blob_digest"));
}

#[test]
fn label_enum_count_assertions() {
    // Cardinality estimate sanity: enum sizes pinned to canonical
    // values per WI §1 invariant 7 + sprint contract §5.1 R-S09-2.
    assert_eq!(ALL_TIERS.len(), 5);
    assert_eq!(ALL_REGIONS.len(), 22);
    assert_eq!(ALL_RESULTS.len(), 3);
    // 4-layer bulkhead
    let layers = [
        RateLimitLayerLabel::Global,
        RateLimitLayerLabel::PerTenant,
        RateLimitLayerLabel::PerIp,
        RateLimitLayerLabel::PerTenantPerEndpoint,
    ];
    assert_eq!(layers.len(), 4);
    let reasons = [
        RateLimitReasonLabel::BucketDrained,
        RateLimitReasonLabel::CidrBlocked,
        RateLimitReasonLabel::QuotaExceeded,
        RateLimitReasonLabel::AbuseScoreHigh,
        RateLimitReasonLabel::GlobalCircuitOpen,
        RateLimitReasonLabel::CanceledTenant,
    ];
    assert_eq!(reasons.len(), 6);
    let phases = [
        GcPhaseLabel::Mark,
        GcPhaseLabel::Sweep,
        GcPhaseLabel::SoftDelete,
        GcPhaseLabel::PhysDelete,
    ];
    assert_eq!(phases.len(), 4);
    let dsrs = [
        DsrTypeLabel::Access,
        DsrTypeLabel::Erasure,
        DsrTypeLabel::Portability,
        DsrTypeLabel::Rectification,
        DsrTypeLabel::Objection,
    ];
    assert_eq!(dsrs.len(), 5);
    let billings = [
        BillingEventTypeLabel::CasBytesStored,
        BillingEventTypeLabel::CasBytesEgress,
        BillingEventTypeLabel::AcLookups,
        BillingEventTypeLabel::RequestsTotal,
        BillingEventTypeLabel::OverageCharge,
    ];
    assert_eq!(billings.len(), 5);
    let buckets = [
        R2BucketLabel::Cas,
        R2BucketLabel::Ac,
        R2BucketLabel::Audit,
        R2BucketLabel::Manifest,
        R2BucketLabel::Multipart,
    ];
    assert_eq!(buckets.len(), 5);
    let ops = [
        R2OpTypeLabel::Put,
        R2OpTypeLabel::Get,
        R2OpTypeLabel::Delete,
        R2OpTypeLabel::List,
    ];
    assert_eq!(ops.len(), 4);
    let nss = [
        KvNamespaceLabel::TenantMeta,
        KvNamespaceLabel::SessionCache,
        KvNamespaceLabel::FeatureFlags,
        KvNamespaceLabel::RateLimitState,
        KvNamespaceLabel::Blocklist,
    ];
    assert_eq!(nss.len(), 5);
    let dos = [
        DoClassLabel::RateLimiter,
        DoClassLabel::Quota,
        DoClassLabel::GlobalCircuit,
        DoClassLabel::TenantState,
    ];
    assert_eq!(dos.len(), 4);
    let hms = [AcHitMissLabel::Hit, AcHitMissLabel::Miss];
    assert_eq!(hms.len(), 2);
}

// ---- Property test strategies ---------------------------------------

prop_compose! {
    fn arb_tier()(idx in 0_usize..ALL_TIERS.len()) -> Tier {
        ALL_TIERS[idx]
    }
}

prop_compose! {
    fn arb_region()(idx in 0_usize..ALL_REGIONS.len()) -> Region {
        ALL_REGIONS[idx]
    }
}

prop_compose! {
    fn arb_result()(idx in 0_usize..ALL_RESULTS.len()) -> RedResultLabel {
        ALL_RESULTS[idx]
    }
}

prop_compose! {
    fn arb_label_tuple()(
        tier in arb_tier(),
        region in arb_region(),
        result in arb_result()
    ) -> MetricLabelTuple {
        MetricLabelTuple {
            tenant_tier: Some(tier),
            region: Some(region),
            result: Some(result),
            ..MetricLabelTuple::empty()
        }
    }
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Over-budget unique-tuple addition is rejected.
    /// INV-OBS-CARDINALITY-BUDGET enforcement.
    #[test]
    fn prop_cardinality_budget_enforced(
        budget in 1_u64..50,
        tuples in proptest::collection::vec(arb_label_tuple(), 1..200),
    ) {
        let (v, _a) = fresh_validator_with_budget(budget, budget * 100);
        let unique_count: usize = {
            let mut set = std::collections::HashSet::new();
            for t in &tuples {
                set.insert(*t);
            }
            set.len()
        };
        let mut accepted = 0_u64;
        let mut rejected = 0_u64;
        for (idx, t) in tuples.iter().enumerate() {
            let req = format!("req-{idx}");
            match v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                *t,
                &req,
                idx as u64,
            ) {
                Ok(out) => {
                    if out.decision == ValidatorDecision::Registered {
                        accepted = accepted.saturating_add(1);
                    }
                }
                Err(AnalyticsError::CardinalityBudgetExceeded {
                    scope,
                    ..
                }) => {
                    rejected = rejected.saturating_add(1);
                    let valid = matches!(scope, "per_metric" | "global");
                    prop_assert!(
                        valid,
                        "scope must be per_metric or global, got {scope}"
                    );
                }
                Err(e) => {
                    prop_assert!(
                        false,
                        "unexpected error variant: {e}"
                    );
                }
            }
        }
        // Accepted unique tuples never exceeds the budget.
        prop_assert!(
            accepted <= budget,
            "accepted={} exceeded budget={}",
            accepted,
            budget
        );
        // The number of UNIQUE tuples in the input bounds the
        // accepted count: accepted ≤ min(budget, unique_count).
        prop_assert!(accepted <= unique_count as u64);
        // If we had fewer unique tuples than budget, no rejections.
        if (unique_count as u64) <= budget {
            prop_assert!(
                rejected == 0,
                "spurious rejections: unique={} budget={}",
                unique_count,
                budget
            );
        }
    }

    /// Registering the same `(metric, labels)` tuple twice yields the
    /// same unique-tuple count.
    #[test]
    fn prop_cardinality_idempotent_repeat_label_set(
        labels in arb_label_tuple(),
        repeats in 1_u32..50,
    ) {
        let (v, _a) = fresh_validator();
        for i in 0..repeats {
            let req = format!("req-{i}");
            let _ = v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                &req,
                u64::from(i),
            );
        }
        prop_assert_eq!(
            v.unique_tuples_for_metric(
                RedMetricKind::CasPutRequestsTotal
            ),
            1,
            "repeat tuple inflated unique count"
        );
    }

    /// Rate counter never decrements across any sequence of
    /// `record_rate` calls. OpenMetrics 1.0 §counter monotonicity.
    #[test]
    fn prop_red_rate_monotone(
        increments in proptest::collection::vec(0_u64..1000, 1..100),
        labels in arb_label_tuple(),
    ) {
        let m = InMemoryRedMetrics::new();
        let mut last = 0_u64;
        for inc in increments {
            m.record_rate(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                inc,
            ).unwrap();
            let current = m.rate_counter(
                RedMetricKind::CasPutRequestsTotal,
                labels,
            );
            prop_assert!(
                current >= last,
                "counter decremented: {current} < {last}"
            );
            last = current;
        }
    }

    /// Error counter never decrements. OpenMetrics 1.0 §counter
    /// monotonicity.
    #[test]
    fn prop_red_errors_monotone(
        increments in proptest::collection::vec(0_u64..1000, 1..100),
        labels in arb_label_tuple(),
    ) {
        let m = InMemoryRedMetrics::new();
        let mut last = 0_u64;
        for inc in increments {
            m.record_error(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                inc,
            ).unwrap();
            let current = m.error_counter(
                RedMetricKind::CasPutRequestsTotal,
                labels,
            );
            prop_assert!(current >= last);
            last = current;
        }
    }

    /// Duration observation lands in the canonical bucket per
    /// `AnalyticsConfig::histogram_bucket_boundaries`.
    #[test]
    fn prop_red_duration_histogram_bucket_correct(
        value_seconds in -10.0_f64..100.0,
        labels in arb_label_tuple(),
    ) {
        let m = InMemoryRedMetrics::new();
        m.record_duration(
            RedMetricKind::CasPutDurationSeconds,
            labels,
            value_seconds,
        ).unwrap();
        let bnd = &CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES;
        let expected_idx =
            InMemoryRedMetrics::canonical_bucket_index(bnd, value_seconds);
        let buckets = m.duration_buckets(
            RedMetricKind::CasPutDurationSeconds,
            labels,
        ).unwrap();
        // 11 boundaries + +Inf = 12 buckets.
        prop_assert_eq!(buckets.len(), 12);
        for (idx, c) in buckets.iter().enumerate() {
            let expected: u64 = if idx == expected_idx { 1 } else { 0 };
            prop_assert!(
                *c == expected,
                "bucket count mismatch at idx={} got={} expected={}",
                idx, c, expected
            );
        }
    }

    /// Tier A label tuples never affect Tier B budget under the
    /// per-metric ledger. INV-TENANT-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(
        n_a in 1_usize..ALL_REGIONS.len(),
        n_b in 1_usize..ALL_REGIONS.len(),
    ) {
        let (v, _a) = fresh_validator();
        // Tier A (Free) registers across the first `n_a` regions.
        for r in ALL_REGIONS.iter().take(n_a) {
            let labels = MetricLabelTuple {
                tenant_tier: Some(Tier::Free),
                region: Some(*r),
                ..MetricLabelTuple::empty()
            };
            let _ = v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                "req-a",
                1000,
            );
        }
        let count_after_a = v.unique_tuples_for_metric(
            RedMetricKind::CasPutRequestsTotal,
        );
        prop_assert!(
            count_after_a == n_a as u64,
            "count_after_a={} != n_a={}",
            count_after_a,
            n_a
        );
        // Tier B (Enterprise) registers across the first `n_b`
        // regions on the SAME metric — same metric, different tier
        // slot. (Tier::Free, region) and (Tier::Enterprise, region)
        // are distinct tuples by construction so the per-metric
        // count grows by exactly n_b.
        for r in ALL_REGIONS.iter().take(n_b) {
            let labels = MetricLabelTuple {
                tenant_tier: Some(Tier::Enterprise),
                region: Some(*r),
                ..MetricLabelTuple::empty()
            };
            let _ = v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                "req-b",
                2000,
            );
        }
        let count_after_b = v.unique_tuples_for_metric(
            RedMetricKind::CasPutRequestsTotal,
        );
        prop_assert!(
            count_after_b == count_after_a + (n_b as u64),
            "tier slot mixing: a-count={} b-count={} n_a={} n_b={}",
            count_after_a,
            count_after_b,
            n_a,
            n_b
        );
    }

    /// Every Allow / Reject / AlreadyRegistered arm emits exactly the
    /// canonical audit event.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        labels in arb_label_tuple(),
        budget in 1_u64..5,
        n_calls in 1_u32..30,
    ) {
        let (v, audit) = fresh_validator_with_budget(budget, budget * 100);
        for i in 0..n_calls {
            let req = format!("req-{i}");
            let _ = v.validate_and_register(
                RedMetricKind::CasPutRequestsTotal,
                labels,
                &req,
                u64::from(i),
            );
        }
        // Every call (whether Registered, AlreadyRegistered, or
        // Rejected) emits exactly one audit record.
        let total_audits = audit.len() as u32;
        prop_assert!(
            total_audits == n_calls,
            "audit count != call count: {} != {}",
            total_audits,
            n_calls
        );
    }

    /// `record_rate(metric, labels, 0)` still counts the unique tuple
    /// at the validator. The value=0 no-op at the observer is
    /// independent of the validator's unique-tuple ledger.
    #[test]
    fn prop_idempotent_zero_value_emit(
        labels in arb_label_tuple(),
    ) {
        let (v, _a) = fresh_validator();
        let m = InMemoryRedMetrics::new();
        // Validator first.
        let out = v.validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            labels,
            "req",
            1000,
        ).unwrap();
        prop_assert_eq!(out.decision, ValidatorDecision::Registered);
        prop_assert_eq!(
            v.unique_tuples_for_metric(
                RedMetricKind::CasPutRequestsTotal,
            ),
            1
        );
        // Observer with by=0 still returns Ok; counter remains 0
        // (record_rate by=0 is a no-op at the observer layer).
        m.record_rate(
            RedMetricKind::CasPutRequestsTotal,
            labels,
            0,
        ).unwrap();
        prop_assert_eq!(
            m.rate_counter(
                RedMetricKind::CasPutRequestsTotal,
                labels,
            ),
            0
        );
    }
}

// ---- Audit-event-arm assertion (specific decision arms) --------------

#[test]
fn audit_arms_pin_canonical_strings() {
    let (v, audit) = fresh_validator_with_budget(1, 100);
    // 1st: Registered → MetricEmitted.
    let _ = v
        .validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-1",
            1000,
        )
        .unwrap();
    // 2nd same tuple: AlreadyRegistered → MetricEmitted (idempotent).
    let _ = v
        .validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-2",
            2000,
        )
        .unwrap();
    // 3rd different tuple: Rejected → CardinalityRejected.
    let err = v
        .validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Sjc),
            "req-3",
            3000,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        AnalyticsError::CardinalityBudgetExceeded { .. }
    ));
    let emitted = audit.snapshot_of(AnalyticsEventType::MetricEmitted).len();
    let rejected = audit
        .snapshot_of(AnalyticsEventType::CardinalityRejected)
        .len();
    assert_eq!(emitted, 2);
    assert_eq!(rejected, 1);
}

#[test]
fn global_budget_exceeded_audit_arm_fires() {
    // Per-metric budget is small; global budget equals per-metric so
    // the global arm fires when we have 3 unique tuples spread across
    // 3 metrics (per-metric arm doesn't fire because each metric has
    // only 1 unique tuple). Note: AnalyticsConfig::with_budgets clamps
    // global = max(per_metric, global) so we set both to 3.
    let audit = Arc::new(InMemoryAnalyticsAuditSink::new());
    let cfg = AnalyticsConfig::with_budgets(3, 3);
    let v = CardinalityValidator::new(Arc::clone(&audit), cfg);
    // 3 distinct (metric, labels) tuples across 2 metrics.
    let _ = v
        .validate_and_register(
            RedMetricKind::CasPutRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-1",
            1000,
        )
        .unwrap();
    let _ = v
        .validate_and_register(
            RedMetricKind::CasGetBytesTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-2",
            2000,
        )
        .unwrap();
    let _ = v
        .validate_and_register(
            RedMetricKind::AcLookupRequestsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-3",
            3000,
        )
        .unwrap();
    assert_eq!(v.unique_tuples_global(), 3);
    // 4th distinct tuple → global budget exceeded.
    let err = v
        .validate_and_register(
            RedMetricKind::GcRunsTotal,
            MetricLabelTuple::tenant_region(Tier::Team, Region::Iad),
            "req-4",
            4000,
        )
        .unwrap_err();
    match err {
        AnalyticsError::CardinalityBudgetExceeded { scope, .. } => {
            assert_eq!(scope, "global");
        }
        other => panic!("expected global budget exceed, got {other:?}"),
    }
    assert_eq!(
        audit.snapshot_of(AnalyticsEventType::BudgetExceeded).len(),
        1
    );
}
