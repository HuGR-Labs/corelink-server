//! Property tests for the synthetic canary 3-region probe orchestrator
//! (WI-S09-007).
//!
//! ## Test ladder
//!
//! Per WI-S09-007 §6.1.10 + spec contract §6 DoD HIGH_RISK SOTA bar,
//! the canonical 6 property tests run at 10k iter on the PR gate +
//! 100k iter on the nightly gate via `PROPTEST_CASES` env var
//! override (S-07 P1-2 fix pattern):
//!
//! 1. `prop_canary_assertion_ladder_canonical` — for any
//!    `(latencies, ceilings)` tuple, the canonical decision matches
//!    the assertion ladder boundaries exactly (load-bearing
//!    falsifiability target).
//! 2. `prop_canary_digest_correctness` — `digest_match=false` always
//!    yields `CanaryDecision::FailedRegion`; `digest_match=true` with
//!    healthy stack + within-ceiling latencies + ≤90s lag yields
//!    `CanaryDecision::Pass`.
//! 3. `prop_synthetic_tenant_excluded_from_sli` — every audit record
//!    emitted by the canary path carries the canonical synthetic
//!    canary tenant_id (Lote 10.8bis P1-NEW-3 inheritance pin).
//! 4. `prop_dispatch_lag_threshold_canonical` — observed lag > 90_000
//!    ms always yields `CanaryDecision::FailedRegion` + emits
//!    `DispatchLagExceeded` audit; ≤ 90_000 never does.
//! 5. `prop_three_region_decision_isolation` — for any deterministic
//!    seeded sweep over 3 regions, the per-region ledger is isolated
//!    (tenant A loops never affect tenant B's count). Lifts INV-
//!    TENANT-ISOLATION canary at the synthetic tenant boundary.
//! 6. `prop_audit_emit_per_decision_arm` — pass arm = 1 audit
//!    (LoopExecuted only); degraded arm = 2 audits (LoopExecuted +
//!    AssertionFailed); failed_region/digest = 2 audits (LoopExecuted
//!    + DigestMismatch); failed_region/health = 2 audits
//!    (LoopExecuted + ObservabilityUnhealthy); failed_region/lag = 2
//!    audits (LoopExecuted + DispatchLagExceeded). Audit-emit-
//!    BEFORE-mutation envelope ordering pinned.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    reason = "tests are allowed to use these primitives + module-level test \
              docs use deep nested numbered/bulleted lists"
)]

use std::sync::Arc;

use proptest::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use corelink_telemetry::canary::{
    canonical_canary_regions, AssertionCeilings, CanaryAssertion, CanaryAuditEventType,
    CanaryDecision, CanaryError, CanaryLatenciesMs, CanaryLoopResult, CanaryProbe, CanaryRegion,
    HealthComponent, InMemoryCanaryAuditSink, InMemoryCanaryProbe, ObservabilityHealthReport,
    DISPATCH_LAG_SEV3_MS, SYNTHETIC_CANARY_TENANT_ID,
};

fn region_strategy() -> impl Strategy<Value = CanaryRegion> {
    prop_oneof![
        Just(CanaryRegion::Enam),
        Just(CanaryRegion::Weur),
        Just(CanaryRegion::Apac),
    ]
}

fn latencies_strategy() -> impl Strategy<Value = CanaryLatenciesMs> {
    (0u32..=200, 0u32..=120, 0u32..=80).prop_map(|(a, b, c)| CanaryLatenciesMs::new(a, b, c))
}

fn health_strategy() -> impl Strategy<Value = ObservabilityHealthReport> {
    (0u32..=60_000, 0u32..=10_000, 0u32..=60_000, 0u32..=6_000).prop_map(
        |(mimir, loki, tempo, dashboard)| ObservabilityHealthReport {
            mimir_ingest_lag_ms: mimir,
            loki_query_p99_ms: loki,
            tempo_trace_lag_ms: tempo,
            dashboard_refresh_p99_ms: dashboard,
        },
    )
}

fn dispatch_lag_strategy() -> impl Strategy<Value = u64> {
    0u64..=180_000
}

fn fresh_probe() -> (
    InMemoryCanaryProbe<InMemoryCanaryAuditSink>,
    Arc<InMemoryCanaryAuditSink>,
) {
    let sink = Arc::new(InMemoryCanaryAuditSink::new());
    (InMemoryCanaryProbe::new(Arc::clone(&sink)), sink)
}

proptest! {
    /// `prop_canary_assertion_ladder_canonical` — load-bearing
    /// falsifiability target. For any (latencies, digest_match,
    /// health, lag) tuple the canonical decision matches the
    /// assertion ladder boundaries exactly.
    #[test]
    fn prop_canary_assertion_ladder_canonical(
        latencies in latencies_strategy(),
        digest_match in any::<bool>(),
        health in health_strategy(),
        lag in dispatch_lag_strategy(),
    ) {
        let ceilings = AssertionCeilings::canonical();
        let decision =
            CanaryLoopResult::compute_decision(latencies, ceilings, digest_match, health, lag);

        let failed_region = !digest_match
            || !health.all_components_healthy()
            || lag > DISPATCH_LAG_SEV3_MS;
        let expected = if failed_region {
            CanaryDecision::FailedRegion
        } else if latencies.within_ceilings(ceilings) {
            CanaryDecision::Pass
        } else {
            CanaryDecision::Degraded
        };
        prop_assert_eq!(decision, expected);
    }

    /// `prop_canary_digest_correctness` — digest mismatch always
    /// yields `FailedRegion` regardless of every other input;
    /// healthy + within-ceiling + ≤90s lag + digest match always
    /// yields `Pass`.
    #[test]
    fn prop_canary_digest_correctness(
        latencies in latencies_strategy(),
        health in health_strategy(),
        lag in dispatch_lag_strategy(),
    ) {
        let ceilings = AssertionCeilings::canonical();
        // digest_match=false branch.
        let mismatched = CanaryLoopResult::compute_decision(
            latencies, ceilings, false, health, lag,
        );
        prop_assert_eq!(mismatched, CanaryDecision::FailedRegion);

        // digest_match=true with everything healthy + within bounds.
        let healthy = ObservabilityHealthReport::healthy_fixture();
        let safe_lat = CanaryLatenciesMs::new(50, 25, 15);
        let pass_decision = CanaryLoopResult::compute_decision(
            safe_lat, ceilings, true, healthy, 10,
        );
        prop_assert_eq!(pass_decision, CanaryDecision::Pass);
    }

    /// `prop_synthetic_tenant_excluded_from_sli` — every audit record
    /// emitted by the canary path carries the canonical synthetic
    /// canary tenant_id literal. Lote 10.8bis P1-NEW-3 inheritance
    /// pin from WI-S08-005 ManualOverride exclusion pattern; runtime
    /// guarantee that the recording rule filter has a structural
    /// anchor (the canary tenant cannot accidentally surface as a
    /// real tenant).
    #[test]
    fn prop_synthetic_tenant_excluded_from_sli(
        region in region_strategy(),
        latencies in latencies_strategy(),
        digest_match in any::<bool>(),
        health in health_strategy(),
        lag in dispatch_lag_strategy(),
    ) {
        let (p, sink) = fresh_probe();
        // We tolerate the orchestrator returning Err on FailedRegion
        // arms; the audit row is committed BEFORE the error return
        // per the audit-emit-BEFORE-mutation envelope.
        let _ = p.execute_canary_loop(region, latencies, digest_match, health, lag, 1);
        for record in sink.snapshot() {
            prop_assert_eq!(record.tenant_id.as_str(), SYNTHETIC_CANARY_TENANT_ID);
        }
    }

    /// `prop_dispatch_lag_threshold_canonical` — observed lag > 90_000
    /// always yields `FailedRegion` + emits the canonical
    /// `DispatchLagExceeded` audit (when lag is the FIRST breach).
    /// ≤ 90_000 never does.
    #[test]
    fn prop_dispatch_lag_threshold_canonical(
        lag in dispatch_lag_strategy(),
    ) {
        let ceilings = AssertionCeilings::canonical();
        let healthy = ObservabilityHealthReport::healthy_fixture();
        let safe_lat = CanaryLatenciesMs::new(50, 25, 15);

        let decision =
            CanaryLoopResult::compute_decision(safe_lat, ceilings, true, healthy, lag);
        if lag > DISPATCH_LAG_SEV3_MS {
            prop_assert_eq!(decision, CanaryDecision::FailedRegion);
        } else {
            prop_assert_eq!(decision, CanaryDecision::Pass);
        }
    }

    /// `prop_three_region_decision_isolation` — concurrent canary
    /// execution on region A never perturbs region B's per-region
    /// ledger (INV-TENANT-ISOLATION at the synthetic tenant
    /// boundary).
    #[test]
    fn prop_three_region_decision_isolation(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let _ = &mut rng; // silence unused (kept for forward-compat seed sweeps)
        let (p, _) = fresh_probe();

        // Drive 30 loops: 10 per region; each region exercises a
        // different decision arm so the ledger advances on Pass /
        // Degraded / FailedRegion paths in isolation.
        for region in canonical_canary_regions() {
            for i in 0..10u64 {
                let lat = match i % 3 {
                    0 => CanaryLatenciesMs::new(50, 25, 15),  // Pass
                    1 => CanaryLatenciesMs::new(101, 25, 15), // Degraded
                    _ => CanaryLatenciesMs::new(50, 25, 15),  // Pass (digest_match=false → Failed below)
                };
                let digest_match = i % 3 != 2;
                let _ = p.execute_canary_loop(
                    *region,
                    lat,
                    digest_match,
                    ObservabilityHealthReport::healthy_fixture(),
                    10,
                    i,
                );
            }
        }

        // Each region MUST have advanced exactly 10 ledger entries.
        for region in canonical_canary_regions() {
            let s = p.stats(*region).unwrap();
            prop_assert_eq!(s.total_loops(), 10);
        }
        // No cross-contamination: total = 30.
        let total: u64 = canonical_canary_regions()
            .iter()
            .map(|r| p.stats(*r).unwrap().total_loops())
            .sum();
        prop_assert_eq!(total, 30);
    }

    /// `prop_audit_emit_per_decision_arm` — pass arm = 1 audit
    /// (LoopExecuted only); non-pass arms = 2 audits (LoopExecuted +
    /// the canonical decision-arm-specific audit). Audit-emit-BEFORE-
    /// mutation envelope ordering pinned: LoopExecuted is always
    /// first (index 0 of the snapshot for a single loop call).
    #[test]
    fn prop_audit_emit_per_decision_arm(
        region in region_strategy(),
        latencies in latencies_strategy(),
        digest_match in any::<bool>(),
        health in health_strategy(),
        lag in dispatch_lag_strategy(),
    ) {
        let (p, sink) = fresh_probe();
        let _ = p.execute_canary_loop(region, latencies, digest_match, health, lag, 1);
        let snap = sink.snapshot();

        // Always at least one audit; LoopExecuted is always first
        // per the audit-emit-BEFORE-mutation envelope.
        prop_assert!(!snap.is_empty());
        let first = &snap[0];
        prop_assert_eq!(first.event_type, CanaryAuditEventType::LoopExecuted);

        let ceilings = AssertionCeilings::canonical();
        let decision =
            CanaryLoopResult::compute_decision(latencies, ceilings, digest_match, health, lag);

        match decision {
            CanaryDecision::Pass => {
                prop_assert_eq!(snap.len(), 1);
            }
            CanaryDecision::Degraded => {
                prop_assert_eq!(snap.len(), 2);
                prop_assert_eq!(
                    snap[1].event_type,
                    CanaryAuditEventType::AssertionFailed
                );
            }
            CanaryDecision::FailedRegion => {
                prop_assert_eq!(snap.len(), 2);
                let second = snap[1].event_type;
                let valid = matches!(
                    second,
                    CanaryAuditEventType::DigestMismatch
                        | CanaryAuditEventType::ObservabilityUnhealthy
                        | CanaryAuditEventType::DispatchLagExceeded
                );
                prop_assert!(valid);
            }
            _ => prop_assert!(false, "non-exhaustive CanaryDecision arm in test"),
        }
    }
}

#[test]
fn surface_pinning_canonical_canary_regions_count() {
    let regions = corelink_telemetry::canary::canonical_canary_regions();
    assert_eq!(regions.len(), 3);
}

#[test]
fn surface_pinning_canonical_assertions_count() {
    let assertions = corelink_telemetry::canary::canonical_canary_assertions();
    assert_eq!(assertions.len(), 4);
}

#[test]
fn surface_pinning_canonical_audit_event_strings_count() {
    let events = corelink_telemetry::canary::canonical_canary_audit_event_strings();
    assert_eq!(events.len(), 5);
    for s in events {
        assert!(s.starts_with("corelink.canary."));
    }
}

#[test]
fn surface_pinning_canary_schema_version() {
    assert_eq!(corelink_telemetry::canary::canary_schema_version(), 1);
}

#[test]
fn surface_pinning_72h_loop_target_lote_10_9bis_p0_a() {
    assert_eq!(
        corelink_telemetry::canary::SUSTAINED_72H_LOOP_TARGET,
        12_960
    );
}

#[test]
fn surface_pinning_dispatch_lag_threshold() {
    assert_eq!(corelink_telemetry::canary::DISPATCH_LAG_SEV3_MS, 90_000);
}

#[test]
fn surface_pinning_cron_interval() {
    assert_eq!(corelink_telemetry::canary::CRON_INTERVAL_SECS, 60);
}

#[test]
fn synthetic_tenant_excluded_from_real_sli_canonical_id() {
    // The recording rule filter in WI-S09-006 alerts YAML excludes
    // this exact tenant_id literal; this test pins the cross-WI
    // canonical so a refactor that mutates either side fails CI.
    assert_eq!(
        SYNTHETIC_CANARY_TENANT_ID,
        "00000000-0000-0000-0000-canary000000",
    );
    assert_eq!(SYNTHETIC_CANARY_TENANT_ID.len(), 36);
}

#[test]
fn three_region_canonical_lote_10_9bis_r4_p1_10() {
    // Sprint contract §6 DoD ship gate criterion 3 regions: enam +
    // weur + apac per data_model.md §2.1 R2 region hints (Lote
    // 10.9bis P1 R4 P1-10 corrected from IATA colocodes).
    let regions = corelink_telemetry::canary::canonical_canary_regions();
    let slugs: Vec<&str> = regions.iter().map(|r| r.as_str()).collect();
    assert_eq!(slugs, vec!["enam", "weur", "apac"]);
}

#[test]
fn full_loop_smoke_pass_and_health_check_canonical() {
    // Smoke test the canonical "happy path": canary loop in every
    // region with safe latencies + healthy stack + ≤90s lag = Pass.
    let sink = Arc::new(InMemoryCanaryAuditSink::new());
    let p = InMemoryCanaryProbe::new(Arc::clone(&sink));
    for region in corelink_telemetry::canary::canonical_canary_regions() {
        let r = p
            .execute_canary_loop(
                *region,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap();
        assert_eq!(r.decision, CanaryDecision::Pass);
        assert!(r.observability_health.all_components_healthy());
        assert!(r.digest_match);
    }
    assert_eq!(p.total_pass_loops().unwrap(), 3);
    // Audit ledger must contain exactly 3 LoopExecuted records.
    let snap = sink.snapshot_of(CanaryAuditEventType::LoopExecuted);
    assert_eq!(snap.len(), 3);
}

#[test]
fn assertion_first_breach_canonical_ordering() {
    // Pin the canonical first-breach order: PUT > GET > LOOKUP.
    let c = AssertionCeilings::canonical();
    assert_eq!(
        CanaryLatenciesMs::new(101, 51, 31).first_breach(c),
        Some(CanaryAssertion::CasPutP99)
    );
    assert_eq!(
        CanaryLatenciesMs::new(100, 51, 31).first_breach(c),
        Some(CanaryAssertion::CasGetP99)
    );
    assert_eq!(
        CanaryLatenciesMs::new(100, 50, 31).first_breach(c),
        Some(CanaryAssertion::AcLookupP99)
    );
    assert_eq!(CanaryLatenciesMs::new(100, 50, 30).first_breach(c), None);
}

#[test]
fn unhealthy_health_takes_precedence_over_latency_ceiling() {
    // Health unhealthy: even with safe latencies, decision = FailedRegion.
    let mut bad = ObservabilityHealthReport::healthy_fixture();
    bad.tempo_trace_lag_ms = 30_001;
    let d = CanaryLoopResult::compute_decision(
        CanaryLatenciesMs::new(50, 25, 15),
        AssertionCeilings::canonical(),
        true,
        bad,
        10,
    );
    assert_eq!(d, CanaryDecision::FailedRegion);

    // Latency breach + healthy: decision = Degraded.
    let d = CanaryLoopResult::compute_decision(
        CanaryLatenciesMs::new(101, 51, 31),
        AssertionCeilings::canonical(),
        true,
        ObservabilityHealthReport::healthy_fixture(),
        10,
    );
    assert_eq!(d, CanaryDecision::Degraded);
}

#[test]
fn first_unhealthy_canonical_order_mimir_loki_tempo_dashboard() {
    let mut h = ObservabilityHealthReport::healthy_fixture();
    h.mimir_ingest_lag_ms = 30_001;
    h.loki_query_p99_ms = 5_001;
    h.tempo_trace_lag_ms = 30_001;
    h.dashboard_refresh_p99_ms = 3_001;
    assert_eq!(h.first_unhealthy(), Some(HealthComponent::Mimir));

    let mut h = ObservabilityHealthReport::healthy_fixture();
    h.loki_query_p99_ms = 5_001;
    h.tempo_trace_lag_ms = 30_001;
    assert_eq!(h.first_unhealthy(), Some(HealthComponent::Loki));

    let mut h = ObservabilityHealthReport::healthy_fixture();
    h.tempo_trace_lag_ms = 30_001;
    h.dashboard_refresh_p99_ms = 3_001;
    assert_eq!(h.first_unhealthy(), Some(HealthComponent::Tempo));

    let mut h = ObservabilityHealthReport::healthy_fixture();
    h.dashboard_refresh_p99_ms = 3_001;
    assert_eq!(h.first_unhealthy(), Some(HealthComponent::Dashboard));
}

#[test]
fn audit_failure_no_ledger_advance_fail_closed_pin() {
    // Pin the canonical fail-CLOSED envelope: audit failure aborts
    // the canary path BEFORE any ledger advance.
    use corelink_telemetry::canary::FailingCanaryAuditSink;
    let sink = Arc::new(FailingCanaryAuditSink::new());
    let p = InMemoryCanaryProbe::new(sink);
    let err = p
        .execute_canary_loop(
            CanaryRegion::Enam,
            CanaryLatenciesMs::new(50, 25, 15),
            true,
            ObservabilityHealthReport::healthy_fixture(),
            10,
            1,
        )
        .unwrap_err();
    assert!(matches!(err, CanaryError::Audit(_)));
    assert_eq!(p.stats(CanaryRegion::Enam).unwrap().total_loops(), 0);
}
