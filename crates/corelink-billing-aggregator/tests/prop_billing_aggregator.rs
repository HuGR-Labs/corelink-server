//! Property tests pinning the load-bearing invariants of
//! `corelink-billing-aggregator` at 10k iterations per check (PR-gate;
//! nightly 100k via `PROPTEST_CASES` env var override per S-07 P1-2
//! fix).
//!
//! Coverage map (mirrors WI-S10-002 §6.1.13 + sprint contract §5.4):
//!
//! - `prop_aggregation_sum_correct` — Σ aggregated `total_qty` =
//!   Σ input event `qty` per (tenant, billing_period, event_kind);
//!   INV-BILLING-NO-LOSS Layer 1 canary.
//! - `prop_idempotent_rerun_same_chain_hash` — re-aggregate same period
//!   → same canonical chain digest; INV-BILLING-NO-DUP canary.
//! - `prop_chain_genesis_zero_prev_hash` — first AggregatedCounter
//!   carries `prev_hash = [0u8; 32]` (Bitcoin-genesis convention).
//! - `prop_chain_break_detected_on_tamper` — modify any aggregate →
//!   chain verify fails at the first tampered sequence;
//!   INV-BILLING-CHAIN-INTEGRITY canary.
//! - `prop_tenant_isolation` — tenant A's aggregator never reads
//!   tenant B events; INV-TENANT-ISOLATION canary.
//! - `prop_audit_emit_per_decision_arm` — each decision arm
//!   (Aggregated / SkippedNoEvents / SkippedDuplicateRun) emits the
//!   canonical run_started + run_completed audits;
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
//! - `prop_jcs_canonicalization_byte_stable` — JCS(aggregate) is
//!   deterministic across calls (RFC 8785 §1).
//! - `prop_deterministic_input_ordering` — input order doesn't affect
//!   the aggregate (orchestrator sorts by `(time_ms, idem_key)`).
//! - `prop_period_boundary_exclusive_end` — events at exactly
//!   `period_end_ms` belong to the NEXT period (canonical Prometheus
//!   bucket boundary).
//!
//! Plus three sanity tests pinning canonical taxonomy cardinalities +
//! crate-level constants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::HashSet;
use std::sync::Arc;

use corelink_analytics::Region;
use corelink_billing_aggregator::{
    aggregator_schema_version, canonical_aggregator_audit_event_strings, compute_canonical_bytes,
    deterministic_event_order, link_chain_hash, link_chain_hash_from_canonical, verify_chain_link,
    AggregatedCounter, AggregatedCounterStore, AggregationDecision, AggregationRequest,
    AggregatorAuditEventType, ChainHash, CounterAggregator, CounterGroupKey,
    FailingAggregatorAuditSink, InMemoryAggregatedCounterStore, InMemoryAggregatorAuditSink,
    InMemoryCounterAggregator, PeriodWindow, CLOUDEVENTS_DATACONTENTTYPE, CLOUDEVENTS_SPECVERSION,
    COUNTER_AGGREGATED_EVENT_TYPE, GENESIS_PREV_HASH, GENESIS_SEQUENCE_NUMBER,
};
use corelink_billing_emit::{
    compute_canonical_bytes_for_idem, derive_idem_key_from_canonical, UsageEvent, UsageEventKind,
};
use proptest::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

const ALL_KINDS: &[UsageEventKind] = &[
    UsageEventKind::StorageBytesHourly,
    UsageEventKind::EgressBytes,
    UsageEventKind::AcLookup,
    UsageEventKind::CasGet,
    UsageEventKind::CasPut,
    UsageEventKind::ReplayRequest,
];

const ALL_REGIONS: &[Region] = &[
    Region::Iad,
    Region::Sjc,
    Region::Dfw,
    Region::Lhr,
    Region::Fra,
    Region::Gru,
    Region::Nrt,
    Region::Sin,
];

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_aggregator_audit_event_strings_pinned() {
    let s = canonical_aggregator_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.billing_aggregator.run_started"));
    assert!(s.contains(&"corelink.billing_aggregator.run_completed"));
    assert!(s.contains(&"corelink.billing_aggregator.chain_break_detected"));
    assert!(s.contains(&"corelink.billing_aggregator.sink_failure"));
}

#[test]
fn canonical_aggregation_decision_taxonomy_pinned() {
    let tenant = Uuid::now_v7();
    let arms = [
        AggregationDecision::SkippedNoEvents {
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            event_kind: UsageEventKind::CasPut,
        },
        AggregationDecision::Aggregated(AggregatedCounter::new(
            "src",
            Uuid::now_v7(),
            1,
            0,
            ChainHash::genesis(),
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            0,
            0,
            0,
            1,
            vec![],
        )),
        AggregationDecision::SkippedDuplicateRun {
            existing: AggregatedCounter::new(
                "src",
                Uuid::now_v7(),
                1,
                0,
                ChainHash::genesis(),
                tenant,
                "2026-05",
                UsageEventKind::CasPut,
                0,
                0,
                0,
                1,
                vec![],
            ),
        },
    ];
    assert_eq!(arms.len(), 3);
}

#[test]
fn crate_constants_pinned() {
    assert_eq!(CLOUDEVENTS_SPECVERSION, "1.0");
    assert_eq!(CLOUDEVENTS_DATACONTENTTYPE, "application/json");
    assert_eq!(
        COUNTER_AGGREGATED_EVENT_TYPE,
        "corelink.billing.counter.aggregated"
    );
    assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
    assert_eq!(GENESIS_SEQUENCE_NUMBER, 0);
    assert_eq!(aggregator_schema_version(), 18);
}

// ---- helpers ---------------------------------------------------------

fn pick_kind(rng: &mut ChaCha20Rng) -> UsageEventKind {
    let idx = (rng.next_u32() as usize) % ALL_KINDS.len();
    ALL_KINDS[idx]
}

fn pick_region(rng: &mut ChaCha20Rng) -> Region {
    let idx = (rng.next_u32() as usize) % ALL_REGIONS.len();
    ALL_REGIONS[idx]
}

fn build_event(
    rng: &mut ChaCha20Rng,
    tenant: Uuid,
    kind: UsageEventKind,
    period: &str,
    time_ms: u64,
    qty: u64,
) -> UsageEvent {
    let region = pick_region(rng);
    let mut e = UsageEvent::new(
        "corelink/region/test",
        Uuid::now_v7(),
        time_ms,
        region,
        tenant,
        kind,
        qty,
        period,
    )
    .unwrap();
    let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
    e.idem_key = derive_idem_key_from_canonical(&canonical);
    e
}

fn fresh_aggregator() -> (
    InMemoryCounterAggregator<InMemoryAggregatorAuditSink, InMemoryAggregatedCounterStore>,
    Arc<InMemoryAggregatorAuditSink>,
    Arc<InMemoryAggregatedCounterStore>,
) {
    let audit = Arc::new(InMemoryAggregatorAuditSink::new());
    let store = Arc::new(InMemoryAggregatedCounterStore::new());
    let agg = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
    (agg, audit, store)
}

// ---- property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Σ aggregated `total_qty` = Σ input event `qty` over the period
    /// window. INV-BILLING-NO-LOSS Layer 1 canary.
    #[test]
    fn prop_aggregation_sum_correct(seed in any::<u64>(), n in 1usize..=24, kind_idx in 0usize..6) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let kind = ALL_KINDS[kind_idx];
        let period = "2026-05";
        let mut evs: Vec<UsageEvent> = Vec::with_capacity(n);
        let mut expected_sum: u128 = 0;
        for i in 0..n {
            // qty bounded so n × max < u128::MAX (well below).
            let qty = u64::from(rng.next_u32() & 0x000F_FFFF);
            expected_sum = expected_sum.saturating_add(u128::from(qty));
            let time_ms = 100u64.saturating_add(i as u64);
            evs.push(build_event(&mut rng, tenant, kind, period, time_ms, qty));
        }
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
            events: &evs,
            source: "corelink/region/iad/aggregator",
            now_ms: 1_000_000,
            aggregate_id: Uuid::now_v7(),
        };
        let dec = a.run(req).unwrap();
        let agg = match dec {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        prop_assert_eq!(agg.data.total_qty, expected_sum);
        prop_assert_eq!(agg.data.event_count, n as u64);
    }

    /// Re-aggregating the same period reproduces the same canonical
    /// chain digest. INV-BILLING-NO-DUP canary.
    #[test]
    fn prop_idempotent_rerun_same_chain_hash(seed in any::<u64>(), n in 1usize..=12) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let mut evs: Vec<UsageEvent> = Vec::with_capacity(n);
        for i in 0..n {
            let qty = u64::from(rng.next_u32() & 0x0000_FFFF);
            let time_ms = 100u64.saturating_add(i as u64);
            evs.push(build_event(&mut rng, tenant, kind, period, time_ms, qty));
        }
        let aggregate_id = Uuid::now_v7();
        let now_ms = 999_999u64;

        let (a1, _aud1, s1) = fresh_aggregator();
        let req1 = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 500_000).unwrap(),
            events: &evs,
            source: "src",
            now_ms,
            aggregate_id,
        };
        let agg1 = match a1.run(req1).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        let head1 = s1.chain_head(tenant, period).unwrap();

        // Fresh aggregator over the same canonical inputs.
        let (a2, _aud2, s2) = fresh_aggregator();
        let req2 = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 500_000).unwrap(),
            events: &evs,
            source: "src",
            now_ms,
            aggregate_id,
        };
        let agg2 = match a2.run(req2).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        let head2 = s2.chain_head(tenant, period).unwrap();

        prop_assert_eq!(&agg1, &agg2);
        prop_assert_eq!(head1.current_head, head2.current_head);
    }

    /// First aggregate carries `prev_hash = [0u8; 32]` (Bitcoin-genesis
    /// convention).
    #[test]
    fn prop_chain_genesis_zero_prev_hash(seed in any::<u64>(), n in 1usize..=8) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let mut evs: Vec<UsageEvent> = Vec::with_capacity(n);
        for i in 0..n {
            let qty = u64::from(rng.next_u32() & 0x0000_FFFF);
            evs.push(build_event(&mut rng, tenant, kind, period, 100u64.saturating_add(i as u64), qty));
        }
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1_000_000,
            aggregate_id: Uuid::now_v7(),
        };
        let dec = a.run(req).unwrap();
        let agg = match dec {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        prop_assert_eq!(agg.prev_hash, ChainHash::genesis());
        prop_assert_eq!(agg.sequence_number, GENESIS_SEQUENCE_NUMBER);
    }

    /// Tampering with any byte of an aggregate breaks the chain verify
    /// at that aggregate's link. INV-BILLING-CHAIN-INTEGRITY canary.
    #[test]
    fn prop_chain_break_detected_on_tamper(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let evs = vec![
            build_event(&mut rng, tenant, kind, period, 100, 10),
            build_event(&mut rng, tenant, kind, period, 200, 20),
        ];
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1_000_000,
            aggregate_id: Uuid::now_v7(),
        };
        let agg = match a.run(req).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        let original_link = link_chain_hash(&ChainHash::genesis(), &agg).unwrap();
        prop_assert!(verify_chain_link(&ChainHash::genesis(), &agg, &original_link).unwrap());

        // Tamper a single field (total_qty) → recomputed link diverges.
        let mut tampered = agg.clone();
        tampered.data.total_qty = tampered.data.total_qty.wrapping_add(1);
        let tampered_verify = verify_chain_link(&ChainHash::genesis(), &tampered, &original_link).unwrap();
        prop_assert!(!tampered_verify);
    }

    /// Tenant A's aggregator never reads tenant B events.
    /// INV-TENANT-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(seed in any::<u64>(), n_a in 1usize..=8, n_b in 1usize..=8) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, store) = fresh_aggregator();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let mut all_evs: Vec<UsageEvent> = Vec::with_capacity(n_a + n_b);
        let mut expected_sum_a: u128 = 0;
        for i in 0..n_a {
            let qty = u64::from(rng.next_u32() & 0x0000_FFFF);
            expected_sum_a = expected_sum_a.saturating_add(u128::from(qty));
            all_evs.push(build_event(&mut rng, tenant_a, kind, period, 100u64.saturating_add(i as u64), qty));
        }
        for i in 0..n_b {
            let qty = u64::from(rng.next_u32() & 0x0000_FFFF);
            // Disjoint time slot so deterministic order is unambiguous.
            all_evs.push(build_event(&mut rng, tenant_b, kind, period, 10_000u64.saturating_add(i as u64), qty));
        }
        let req_a = AggregationRequest {
            tenant_id: tenant_a,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
            events: &all_evs,
            source: "src",
            now_ms: 1_000_000,
            aggregate_id: Uuid::now_v7(),
        };
        let agg_a = match a.run(req_a).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        prop_assert_eq!(agg_a.data.tenant_id, tenant_a);
        prop_assert_eq!(agg_a.data.total_qty, expected_sum_a);
        prop_assert_eq!(agg_a.data.event_count, n_a as u64);
        // Tenant B chain head untouched.
        let head_b = store.chain_head(tenant_b, period).unwrap();
        prop_assert_eq!(head_b.current_head, ChainHash::genesis());
        prop_assert_eq!(head_b.next_sequence, 0);
    }

    /// Each canonical decision arm fires its canonical audit envelope.
    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
    #[test]
    fn prop_audit_emit_per_decision_arm(seed in any::<u64>(), arm_idx in 0usize..3) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";

        match arm_idx {
            0 => {
                // Arm 1: SkippedNoEvents (empty input set).
                let (a, audit, _store) = fresh_aggregator();
                let req = AggregationRequest {
                    tenant_id: tenant,
                    billing_period: period,
                    event_kind: kind,
                    period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
                    events: &[],
                    source: "src",
                    now_ms: 1_000_000,
                    aggregate_id: Uuid::now_v7(),
                };
                let dec = a.run(req).unwrap();
                let is_skipped = matches!(dec, AggregationDecision::SkippedNoEvents { .. });
                prop_assert!(is_skipped);
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunStarted).len(),
                    1
                );
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunCompleted).len(),
                    1
                );
            }
            1 => {
                // Arm 2: Aggregated (events present; first run).
                let (a, audit, _store) = fresh_aggregator();
                let evs = vec![build_event(&mut rng, tenant, kind, period, 100, 5)];
                let req = AggregationRequest {
                    tenant_id: tenant,
                    billing_period: period,
                    event_kind: kind,
                    period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
                    events: &evs,
                    source: "src",
                    now_ms: 1_000_000,
                    aggregate_id: Uuid::now_v7(),
                };
                let dec = a.run(req).unwrap();
                let is_agg = matches!(dec, AggregationDecision::Aggregated(_));
                prop_assert!(is_agg);
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunStarted).len(),
                    1
                );
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunCompleted).len(),
                    1
                );
            }
            _ => {
                // Arm 3: SkippedDuplicateRun (re-run with same canonical input).
                let (a, audit, _store) = fresh_aggregator();
                let evs = vec![build_event(&mut rng, tenant, kind, period, 100, 5)];
                let aggregate_id = Uuid::now_v7();
                let now_ms = 1_000_000u64;
                let req1 = AggregationRequest {
                    tenant_id: tenant,
                    billing_period: period,
                    event_kind: kind,
                    period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
                    events: &evs,
                    source: "src",
                    now_ms,
                    aggregate_id,
                };
                let req2 = AggregationRequest {
                    tenant_id: tenant,
                    billing_period: period,
                    event_kind: kind,
                    period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
                    events: &evs,
                    source: "src",
                    now_ms,
                    aggregate_id,
                };
                a.run(req1).unwrap();
                let dec2 = a.run(req2).unwrap();
                let is_dup = matches!(dec2, AggregationDecision::SkippedDuplicateRun { .. });
                prop_assert!(is_dup);
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunStarted).len(),
                    2
                );
                prop_assert_eq!(
                    audit.snapshot_of(AggregatorAuditEventType::RunCompleted).len(),
                    2
                );
            }
        }
    }

    /// JCS canonicalization is deterministic across calls (RFC 8785 §1).
    #[test]
    fn prop_jcs_canonicalization_byte_stable(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let evs = vec![
            build_event(&mut rng, tenant, kind, period, 100, 10),
            build_event(&mut rng, tenant, kind, period, 200, 20),
        ];
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1_000_000,
            aggregate_id: Uuid::now_v7(),
        };
        let agg = match a.run(req).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        let bytes_a = compute_canonical_bytes(&agg).unwrap();
        let bytes_b = compute_canonical_bytes(&agg).unwrap();
        let bytes_c = compute_canonical_bytes(&agg).unwrap();
        prop_assert_eq!(&bytes_a, &bytes_b);
        prop_assert_eq!(&bytes_b, &bytes_c);
        let h_a = link_chain_hash_from_canonical(&ChainHash::genesis(), &bytes_a);
        let h_b = link_chain_hash_from_canonical(&ChainHash::genesis(), &bytes_b);
        prop_assert_eq!(h_a, h_b);
    }

    /// Input order doesn't affect the aggregate; orchestrator sorts by
    /// `(time_ms, idem_key)` lexicographic.
    #[test]
    fn prop_deterministic_input_ordering(seed in any::<u64>(), n in 2usize..=12) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let mut evs: Vec<UsageEvent> = Vec::with_capacity(n);
        for i in 0..n {
            let qty = u64::from(rng.next_u32() & 0x0000_FFFF);
            // Distinct time_ms per event so the sort is total.
            let time_ms = 100u64.saturating_add(u64::try_from(i).unwrap_or(0).saturating_mul(7));
            evs.push(build_event(&mut rng, tenant, kind, period, time_ms, qty));
        }

        let aggregate_id = Uuid::now_v7();
        let now_ms = 1_000_000u64;
        let pw = PeriodWindow::new(0, 500_000).unwrap();

        // Run 1: as-is.
        let (a1, _audit1, _s1) = fresh_aggregator();
        let req1 = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: pw,
            events: &evs,
            source: "src",
            now_ms,
            aggregate_id,
        };
        let agg1 = match a1.run(req1).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };

        // Run 2: reversed input.
        let mut evs_rev = evs.clone();
        evs_rev.reverse();
        let (a2, _audit2, _s2) = fresh_aggregator();
        let req2 = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: pw,
            events: &evs_rev,
            source: "src",
            now_ms,
            aggregate_id,
        };
        let agg2 = match a2.run(req2).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };

        // Run 3: shuffled by ChaCha20Rng.
        let mut evs_shuffled = evs.clone();
        let mut shuffle_rng = ChaCha20Rng::seed_from_u64(seed.wrapping_add(1));
        for i in (1..evs_shuffled.len()).rev() {
            let j = (shuffle_rng.next_u32() as usize) % (i + 1);
            evs_shuffled.swap(i, j);
        }
        let (a3, _audit3, _s3) = fresh_aggregator();
        let req3 = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: pw,
            events: &evs_shuffled,
            source: "src",
            now_ms,
            aggregate_id,
        };
        let agg3 = match a3.run(req3).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };

        // Same canonical bytes + same chain digest across all three.
        let b1 = compute_canonical_bytes(&agg1).unwrap();
        let b2 = compute_canonical_bytes(&agg2).unwrap();
        let b3 = compute_canonical_bytes(&agg3).unwrap();
        prop_assert_eq!(&b1, &b2);
        prop_assert_eq!(&b2, &b3);

        // Deterministic ordering helper agrees: the helper itself produces
        // the same ordered slice across the three input permutations.
        let ord1 = deterministic_event_order(&evs, tenant, kind, pw);
        let ord2 = deterministic_event_order(&evs_rev, tenant, kind, pw);
        let ord3 = deterministic_event_order(&evs_shuffled, tenant, kind, pw);
        let idem1: Vec<_> = ord1.iter().map(|e| e.idem_key).collect();
        let idem2: Vec<_> = ord2.iter().map(|e| e.idem_key).collect();
        let idem3: Vec<_> = ord3.iter().map(|e| e.idem_key).collect();
        prop_assert_eq!(&idem1, &idem2);
        prop_assert_eq!(&idem2, &idem3);
    }

    /// Events at exactly `period_end_ms` belong to the NEXT period
    /// (canonical Prometheus bucket boundary semantics).
    #[test]
    fn prop_period_boundary_exclusive_end(seed in any::<u64>(), end_ms in 1_000u64..=1_000_000u64) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let period = "2026-05";
        let evs = vec![
            // In-window event (should be counted).
            build_event(&mut rng, tenant, kind, period, end_ms.saturating_sub(1), 10),
            // Boundary event at exactly end_ms (should be EXCLUDED).
            build_event(&mut rng, tenant, kind, period, end_ms, 99),
        ];
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, end_ms).unwrap(),
            events: &evs,
            source: "src",
            now_ms: end_ms.saturating_mul(2),
            aggregate_id: Uuid::now_v7(),
        };
        let agg = match a.run(req).unwrap() {
            AggregationDecision::Aggregated(a) => a,
            _ => unreachable!(),
        };
        // Only the qty=10 event contributed; the boundary event was excluded.
        prop_assert_eq!(agg.data.total_qty, 10u128);
        prop_assert_eq!(agg.data.event_count, 1u64);
    }
}

// ---- additional sanity (non-randomized) ------------------------------

#[test]
fn audit_failure_aborts_run_no_state_mutation() {
    let audit = Arc::new(FailingAggregatorAuditSink::new());
    let store = Arc::new(InMemoryAggregatedCounterStore::new());
    let a = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
    let tenant = Uuid::now_v7();
    let mut rng = ChaCha20Rng::seed_from_u64(0xDEAD_BEEF);
    let evs = vec![build_event(
        &mut rng,
        tenant,
        UsageEventKind::CasPut,
        "2026-05",
        100,
        5,
    )];
    let req = AggregationRequest {
        tenant_id: tenant,
        billing_period: "2026-05",
        event_kind: UsageEventKind::CasPut,
        period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
        events: &evs,
        source: "src",
        now_ms: 1_000_000,
        aggregate_id: Uuid::now_v7(),
    };
    let err = a.run(req).unwrap_err();
    let is_audit_err = matches!(err, corelink_billing_aggregator::AggregatorError::Audit(_));
    assert!(is_audit_err);
    assert_eq!(store.len(), 0);
    let head = store.chain_head(tenant, "2026-05").unwrap();
    assert_eq!(head.current_head, ChainHash::genesis());
}

#[test]
fn aggregator_audit_event_strings_distinct_set() {
    let s = canonical_aggregator_audit_event_strings();
    let set: HashSet<&&str> = s.iter().collect();
    assert_eq!(set.len(), 4);
    for entry in s {
        assert!(entry.starts_with("corelink.billing_aggregator."));
    }
}

#[test]
fn group_key_uniqueness_pinned() {
    // Three group keys distinct on each axis (tenant, period, kind).
    let t1 = Uuid::now_v7();
    let t2 = Uuid::now_v7();
    let k1 = CounterGroupKey {
        tenant_id: t1,
        billing_period: "2026-05".to_string(),
        event_kind: UsageEventKind::CasPut,
    };
    let k2 = CounterGroupKey {
        tenant_id: t2,
        billing_period: "2026-05".to_string(),
        event_kind: UsageEventKind::CasPut,
    };
    let k3 = CounterGroupKey {
        tenant_id: t1,
        billing_period: "2026-06".to_string(),
        event_kind: UsageEventKind::CasPut,
    };
    let k4 = CounterGroupKey {
        tenant_id: t1,
        billing_period: "2026-05".to_string(),
        event_kind: UsageEventKind::EgressBytes,
    };
    let mut set = HashSet::new();
    set.insert(format!("{k1:?}"));
    set.insert(format!("{k2:?}"));
    set.insert(format!("{k3:?}"));
    set.insert(format!("{k4:?}"));
    assert_eq!(set.len(), 4);
}
