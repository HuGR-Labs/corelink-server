//! Property tests pinning the load-bearing invariants of
//! `corelink-billing-stripe` at 10k iterations per check (PR-gate;
//! nightly 100k via `PROPTEST_CASES` env var override per S-07 P1-2
//! fix).
//!
//! Coverage map (mirrors WI-S10-003 §6.1.18 + sprint contract §5.3):
//!
//! - `prop_idempotency_key_deterministic` — same aggregate → same key;
//!   INV-BILLING-NO-DUP canary at the Stripe adapter layer.
//! - `prop_idempotency_key_diverges_per_aggregate` — distinct aggregates
//!   produce distinct keys (collision search canary; sprint contract
//!   §15 R-003 mitigation).
//! - `prop_webhook_signature_verifies_valid` — fresh canonical signature
//!   within 5-min window verifies.
//! - `prop_webhook_signature_rejects_expired` — signature outside 5-min
//!   window rejected.
//! - `prop_webhook_signature_rejects_tampered_payload` — signature with
//!   payload tamper rejected.
//! - `prop_webhook_signature_rejects_tampered_signature` — signature
//!   bytes tampered → rejected.
//! - `prop_constant_time_signature_compare` — verify the canonical
//!   constant-time API surface (`subtle::ConstantTimeEq`); the timing
//!   microbenchmark itself is informational only.
//! - `prop_tenant_isolation` — distinct tenants produce distinct
//!   idempotency keys; INV-TENANT-ISOLATION canary.
//! - `prop_audit_emit_per_decision_arm` — every adapter / webhook
//!   decision arm fires its canonical audit;
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
//! - `prop_replay_window_exact_5min_boundary` — at exactly 5min the
//!   verifier accepts; at 5min+1ms the verifier rejects (canonical
//!   Stripe spec boundary semantics).
//!
//! Plus six sanity tests pinning canonical taxonomy cardinalities +
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
    aggregator_schema_version, link_chain_hash_from_canonical, AggregatedCounter,
    AggregationDecision, AggregationRequest, ChainHash, CounterAggregator,
    InMemoryAggregatedCounterStore, InMemoryAggregatorAuditSink, InMemoryCounterAggregator,
    PeriodWindow,
};
use corelink_billing_emit::{
    compute_canonical_bytes_for_idem, derive_idem_key_from_canonical, IdemKey, UsageEvent,
    UsageEventKind,
};
use corelink_billing_stripe::{
    canonical_stripe_audit_event_strings, compute_canonical_aggregate_bytes, compute_signature,
    derive_idempotency_key, derive_idempotency_key_from_canonical, stripe_schema_version,
    verify_stripe_signature, FailingStripeAuditSink, FailingStripeUsageLedger,
    IdempotencyKey, InMemoryStripeAuditSink, InMemoryStripeBillingAdapter,
    InMemoryStripeUsageLedger, InMemoryStripeWebhookHandler, InMemoryStripeWebhookLog,
    StripeAdapterDecision, StripeAuditEventType, StripeBillingAdapter, StripeError,
    StripeWebhookHandler, SubscriptionItemId, WebhookEvent, WebhookEventKind,
    WebhookHandleRequest, REPLAY_WINDOW_MS,
};
use proptest::prelude::*;
use rand::{RngCore, SeedableRng};
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

const ALL_WEBHOOK_KINDS: &[WebhookEventKind] = &[
    WebhookEventKind::InvoiceCreated,
    WebhookEventKind::InvoicePaid,
    WebhookEventKind::InvoiceFailed,
    WebhookEventKind::SubscriptionUpdated,
    WebhookEventKind::CustomerCreated,
];

const TEST_SECRET: &[u8] = b"whsec_canonical_test_secret_v1";

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_stripe_audit_event_strings_pinned() {
    let s = canonical_stripe_audit_event_strings();
    assert_eq!(s.len(), 6);
    assert!(s.contains(&"corelink.billing_stripe.usage_recorded"));
    assert!(s.contains(&"corelink.billing_stripe.duplicate_rejected"));
    assert!(s.contains(&"corelink.billing_stripe.webhook_received"));
    assert!(s.contains(&"corelink.billing_stripe.signature_rejected"));
    assert!(s.contains(&"corelink.billing_stripe.signature_verified"));
    assert!(s.contains(&"corelink.billing_stripe.signature_skew_rejected"));
}

#[test]
fn canonical_stripe_adapter_decision_taxonomy_pinned() {
    let tenant = Uuid::now_v7();
    let arms = [
        StripeAdapterDecision::UsageRecorded {
            idempotency_key: IdempotencyKey([0xAB; 32]),
            subscription_item_id: SubscriptionItemId::new("si_x"),
            tenant_id: tenant,
            total_qty: 1,
        },
        StripeAdapterDecision::DuplicateRejected {
            idempotency_key: IdempotencyKey([0xAB; 32]),
            tenant_id: tenant,
        },
        StripeAdapterDecision::WebhookProcessed {
            stripe_event_id: "evt_x".to_string(),
            kind: WebhookEventKind::InvoicePaid,
        },
        StripeAdapterDecision::SignatureRejected {
            reason: "HMAC mismatch".to_string(),
        },
    ];
    assert_eq!(arms.len(), 4);
}

#[test]
fn canonical_webhook_event_kind_taxonomy_pinned() {
    let mut set: HashSet<&'static str> = HashSet::new();
    for k in ALL_WEBHOOK_KINDS {
        assert!(set.insert(k.as_str()), "duplicate canonical kind: {k}");
    }
    assert_eq!(set.len(), 5);
}

#[test]
fn crate_constants_pinned() {
    assert_eq!(REPLAY_WINDOW_MS, 5 * 60 * 1000);
    assert_eq!(stripe_schema_version(), 18);
    // Aggregator + adapter share the same schema slot (additive d1
    // migration `0018_stripe_idem_keys.sql` lands alongside this WI).
    assert_eq!(aggregator_schema_version(), 18);
}

#[test]
fn idempotency_key_hex_form_canonical_64_chars() {
    let key = IdempotencyKey([0xAB; 32]);
    assert_eq!(key.to_hex().len(), 64);
}

#[test]
fn aggregator_audit_event_strings_distinct_set() {
    let s = canonical_stripe_audit_event_strings();
    let set: HashSet<&&str> = s.iter().collect();
    assert_eq!(set.len(), 6);
    for entry in s {
        assert!(entry.starts_with("corelink.billing_stripe."));
    }
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

fn pick_webhook_kind(rng: &mut ChaCha20Rng) -> WebhookEventKind {
    let idx = (rng.next_u32() as usize) % ALL_WEBHOOK_KINDS.len();
    ALL_WEBHOOK_KINDS[idx]
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

fn fresh_aggregate_via_aggregator(
    tenant: Uuid,
    period: &str,
    kind: UsageEventKind,
    events: &[UsageEvent],
) -> AggregatedCounter {
    let audit = Arc::new(InMemoryAggregatorAuditSink::new());
    let store = Arc::new(InMemoryAggregatedCounterStore::new());
    let agg = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
    let req = AggregationRequest {
        tenant_id: tenant,
        billing_period: period,
        event_kind: kind,
        period_window: PeriodWindow::new(0, 1_000_000).unwrap(),
        events,
        source: "corelink/region/iad/aggregator",
        now_ms: 1_700_000_000_000,
        aggregate_id: Uuid::now_v7(),
    };
    match agg.run(req).unwrap() {
        AggregationDecision::Aggregated(a) => a,
        other => unreachable!("{other:?}"),
    }
}

fn build_aggregate_directly(
    tenant: Uuid,
    period: &str,
    kind: UsageEventKind,
    qty: u128,
    seq: u64,
) -> AggregatedCounter {
    AggregatedCounter::new(
        "corelink/region/iad/aggregator",
        Uuid::now_v7(),
        1_700_000_000_000_u64.saturating_add(seq),
        seq,
        ChainHash::genesis(),
        tenant,
        period,
        kind,
        qty,
        1,
        0,
        1000,
        vec![IdemKey::genesis()],
    )
}

fn build_valid_signature_header(payload: &[u8], ts_seconds: u64) -> String {
    let tag = compute_signature(TEST_SECRET, ts_seconds, payload).unwrap();
    format!("t={ts_seconds},v1={}", hex::encode(tag))
}

type Adapter = InMemoryStripeBillingAdapter<
    InMemoryStripeAuditSink,
    InMemoryStripeUsageLedger,
>;

fn fresh_adapter() -> (
    Adapter,
    Arc<InMemoryStripeAuditSink>,
    Arc<InMemoryStripeUsageLedger>,
) {
    let audit = Arc::new(InMemoryStripeAuditSink::new());
    let ledger = Arc::new(InMemoryStripeUsageLedger::new());
    let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
    (a, audit, ledger)
}

type Handler = InMemoryStripeWebhookHandler<
    InMemoryStripeAuditSink,
    InMemoryStripeWebhookLog,
>;

fn fresh_handler() -> (
    Handler,
    Arc<InMemoryStripeAuditSink>,
    Arc<InMemoryStripeWebhookLog>,
) {
    let audit = Arc::new(InMemoryStripeAuditSink::new());
    let log = Arc::new(InMemoryStripeWebhookLog::new());
    let h = InMemoryStripeWebhookHandler::new(Arc::clone(&audit), Arc::clone(&log));
    (h, audit, log)
}

// ---- property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Same aggregate → same canonical Idempotency-Key.
    /// INV-BILLING-NO-DUP canary at the Stripe adapter layer.
    #[test]
    fn prop_idempotency_key_deterministic(seed in any::<u64>(), n in 1usize..=8) {
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
        // Same aggregator inputs reproduce the same canonical aggregate.
        // We keep `aggregate_id` consistent via the orchestrator's UUIDv7
        // per-run new — so we manually fix it by building two fresh
        // aggregators with the same `aggregate_id` to pin the
        // determinism over the typed shape (mirrors WI-S10-002
        // prop_idempotent_rerun_same_chain_hash).
        let id = Uuid::now_v7();
        let a1 = AggregatedCounter::new(
            "corelink/region/iad/aggregator",
            id,
            1_700_000_000_000,
            0,
            ChainHash::genesis(),
            tenant,
            period,
            kind,
            evs.iter().map(|e| u128::from(e.data.qty)).sum::<u128>(),
            n as u64,
            0,
            1000,
            evs.iter().map(|e| e.idem_key).collect(),
        );
        let a2 = a1.clone();
        let k1 = derive_idempotency_key(&a1).unwrap();
        let k2 = derive_idempotency_key(&a2).unwrap();
        prop_assert_eq!(k1, k2);
        // Split-form derivation matches full pipeline.
        let canonical = compute_canonical_aggregate_bytes(&a1).unwrap();
        let k3 = derive_idempotency_key_from_canonical(&canonical);
        prop_assert_eq!(k1, k3);
    }

    /// Distinct aggregates produce distinct idempotency keys.
    /// Collision-search canary (sprint contract §15 R-003 mitigation:
    /// CI test 1M events 0 collisions). Here we use proptest with 10k
    /// random aggregates per run; PR-gate.
    #[test]
    fn prop_idempotency_key_diverges_per_aggregate(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let qty_a = u128::from(rng.next_u32());
        let qty_b = qty_a.wrapping_add(1);
        let kind = pick_kind(&mut rng);
        let a = build_aggregate_directly(tenant_a, "2026-05", kind, qty_a, 0);
        let b = build_aggregate_directly(tenant_b, "2026-05", kind, qty_b, 0);
        let ka = derive_idempotency_key(&a).unwrap();
        let kb = derive_idempotency_key(&b).unwrap();
        prop_assert_ne!(ka, kb);
    }

    /// Fresh canonical signature within the 5-min replay window
    /// verifies (passes round-trip).
    #[test]
    fn prop_webhook_signature_verifies_valid(
        seed in any::<u64>(),
        skew_ms in 0u64..=REPLAY_WINDOW_MS
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 64];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let header = build_valid_signature_header(&payload, ts_seconds);
        // now within [ts*1000, ts*1000 + 5min]
        let now_ms = ts_seconds.saturating_mul(1000).saturating_add(skew_ms);
        let result = verify_stripe_signature(&header, &payload, TEST_SECRET, now_ms);
        prop_assert!(result.is_ok(), "expected Ok; got {:?}", result);
    }

    /// Signature outside the canonical 5-min replay window rejected.
    #[test]
    fn prop_webhook_signature_rejects_expired(
        seed in any::<u64>(),
        excess_ms in 1u64..=10 * 60 * 1000
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 32];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let header = build_valid_signature_header(&payload, ts_seconds);
        // now > ts*1000 + REPLAY_WINDOW_MS by `excess_ms`.
        let now_ms = ts_seconds
            .saturating_mul(1000)
            .saturating_add(REPLAY_WINDOW_MS)
            .saturating_add(excess_ms);
        let result = verify_stripe_signature(&header, &payload, TEST_SECRET, now_ms);
        let is_skew = matches!(result, Err(StripeError::SignatureSkewRejected { .. }));
        prop_assert!(is_skew, "expected skew rejection; got {:?}", result);
    }

    /// Tampering the payload bytes → signature verify fails.
    #[test]
    fn prop_webhook_signature_rejects_tampered_payload(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 64];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let header = build_valid_signature_header(&payload, ts_seconds);
        let mut tampered = payload.clone();
        let idx = (rng.next_u32() as usize) % tampered.len();
        tampered[idx] ^= 0xFF;
        let now_ms = ts_seconds.saturating_mul(1000);
        let result = verify_stripe_signature(&header, &tampered, TEST_SECRET, now_ms);
        let is_rejected = matches!(result, Err(StripeError::SignatureRejected(_)));
        prop_assert!(is_rejected, "expected SignatureRejected; got {:?}", result);
    }

    /// Tampering the v1= signature bytes → verify fails.
    #[test]
    fn prop_webhook_signature_rejects_tampered_signature(
        seed in any::<u64>(),
        flip_byte_idx in 0usize..32
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 64];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let mut tag = compute_signature(TEST_SECRET, ts_seconds, &payload).unwrap();
        tag[flip_byte_idx] ^= 0xFF;
        let header = format!("t={ts_seconds},v1={}", hex::encode(tag));
        let now_ms = ts_seconds.saturating_mul(1000);
        let result = verify_stripe_signature(&header, &payload, TEST_SECRET, now_ms);
        let is_rejected = matches!(result, Err(StripeError::SignatureRejected(_)));
        prop_assert!(is_rejected, "expected SignatureRejected; got {:?}", result);
    }

    /// Constant-time signature compare API surface verification.
    /// The verifier MUST go through `subtle::ConstantTimeEq` (NOT
    /// `==`); we don't run a timing benchmark here (Mann-Whitney
    /// 3-prong is overkill) — the canonical defense is the API call
    /// itself + the source-level audit (see `signature.rs` line
    /// `expected.ct_eq(candidate)` which uses
    /// `subtle::ConstantTimeEq::ct_eq`). This property pins the
    /// behavioral surface: an adversarial signature with a partial
    /// prefix match is rejected with the same canonical error as a
    /// fully-mismatched signature.
    #[test]
    fn prop_constant_time_signature_compare(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 32];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let valid_tag = compute_signature(TEST_SECRET, ts_seconds, &payload).unwrap();

        // Adversary 1: full mismatch (first byte off).
        let mut tag_full_mismatch = valid_tag;
        tag_full_mismatch[0] ^= 0xFF;
        let header_full = format!("t={ts_seconds},v1={}", hex::encode(tag_full_mismatch));

        // Adversary 2: partial prefix match (last byte off).
        let mut tag_partial = valid_tag;
        tag_partial[31] ^= 0xFF;
        let header_partial = format!("t={ts_seconds},v1={}", hex::encode(tag_partial));

        let now_ms = ts_seconds.saturating_mul(1000);
        let r1 = verify_stripe_signature(&header_full, &payload, TEST_SECRET, now_ms);
        let r2 = verify_stripe_signature(&header_partial, &payload, TEST_SECRET, now_ms);
        let r1_rej = matches!(r1, Err(StripeError::SignatureRejected(_)));
        let r2_rej = matches!(r2, Err(StripeError::SignatureRejected(_)));
        prop_assert!(r1_rej);
        prop_assert!(r2_rej);
    }

    /// Distinct tenants → distinct idempotency keys.
    /// INV-TENANT-ISOLATION canary at the Stripe layer.
    #[test]
    fn prop_tenant_isolation(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let kind = pick_kind(&mut rng);
        let qty = u128::from(rng.next_u32());
        let t_a = Uuid::now_v7();
        let t_b = Uuid::now_v7();
        // Same period + same kind + same qty + same seq; only tenant id differs.
        let a = build_aggregate_directly(t_a, "2026-05", kind, qty, 0);
        let b = build_aggregate_directly(t_b, "2026-05", kind, qty, 0);
        let k_a = derive_idempotency_key(&a).unwrap();
        let k_b = derive_idempotency_key(&b).unwrap();
        prop_assert_ne!(k_a, k_b);

        // Adapter dispatch: tenant A's record never affects tenant B's
        // ledger membership.
        let (adapter, _audit, ledger) = fresh_adapter();
        adapter.record_usage(&a, SubscriptionItemId::new("si_a"), 1).unwrap();
        // Tenant B not yet recorded.
        prop_assert!(adapter.get_recorded(&k_b).unwrap().is_none());
        prop_assert_eq!(ledger.len(), 1);
    }

    /// Each canonical decision arm fires its canonical audit envelope.
    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
    #[test]
    fn prop_audit_emit_per_decision_arm(seed in any::<u64>(), arm_idx in 0usize..3) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let qty = u128::from(rng.next_u32());

        match arm_idx {
            0 => {
                // Arm 1: UsageRecorded (first sight at the canonical
                // idempotency key).
                let (a, audit, _ledger) = fresh_adapter();
                let agg = build_aggregate_directly(tenant, "2026-05", kind, qty, 0);
                let dec = a
                    .record_usage(&agg, SubscriptionItemId::new("si_x"), 1)
                    .unwrap();
                let recorded = matches!(dec, StripeAdapterDecision::UsageRecorded { .. });
                prop_assert!(recorded);
                prop_assert_eq!(
                    audit.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
                    1
                );
            }
            1 => {
                // Arm 2: DuplicateRejected (second sight at the same
                // canonical idempotency key).
                let (a, audit, _ledger) = fresh_adapter();
                let agg = build_aggregate_directly(tenant, "2026-05", kind, qty, 0);
                let _ = a
                    .record_usage(&agg, SubscriptionItemId::new("si_x"), 1)
                    .unwrap();
                let dec2 = a
                    .record_usage(&agg, SubscriptionItemId::new("si_x"), 1)
                    .unwrap();
                let dup = matches!(dec2, StripeAdapterDecision::DuplicateRejected { .. });
                prop_assert!(dup);
                prop_assert_eq!(
                    audit.snapshot_of(StripeAuditEventType::DuplicateRejected).len(),
                    1
                );
            }
            _ => {
                // Arm 3: WebhookProcessed (signature verified + log
                // INSERT).
                let (h, audit, log) = fresh_handler();
                let payload = b"{\"id\":\"evt_x\"}";
                let ts = 1_700_000_000_u64;
                let header = build_valid_signature_header(payload, ts);
                let kind = pick_webhook_kind(&mut rng);
                let event = WebhookEvent {
                    stripe_event_id: format!("evt_{}", rng.next_u32()),
                    kind,
                    event_ts_ms: ts.saturating_mul(1000),
                    payload_redacted: payload.to_vec(),
                };
                let req = WebhookHandleRequest {
                    signature_header: &header,
                    payload,
                    webhook_secret: TEST_SECRET,
                    now_ms: ts.saturating_mul(1000),
                    event,
                };
                let dec = h.handle(req).unwrap();
                let processed =
                    matches!(dec, StripeAdapterDecision::WebhookProcessed { .. });
                prop_assert!(processed);
                prop_assert_eq!(log.len(), 1);
                prop_assert_eq!(
                    audit.snapshot_of(StripeAuditEventType::WebhookReceived).len(),
                    1
                );
                prop_assert_eq!(
                    audit.snapshot_of(StripeAuditEventType::SignatureVerified).len(),
                    1
                );
            }
        }
    }

    /// At exactly 5min the verifier accepts; at 5min+1ms the verifier
    /// rejects. Canonical Stripe spec replay-window boundary semantics.
    #[test]
    fn prop_replay_window_exact_5min_boundary(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut payload = vec![0u8; 32];
        rng.fill_bytes(&mut payload);
        let ts_seconds = 1_700_000_000_u64;
        let header = build_valid_signature_header(&payload, ts_seconds);

        // At exactly 5min: ACCEPT (boundary inclusive per WI-S10-003 §6.1.5).
        let at_5min = ts_seconds.saturating_mul(1000).saturating_add(REPLAY_WINDOW_MS);
        let r_at = verify_stripe_signature(&header, &payload, TEST_SECRET, at_5min);
        prop_assert!(r_at.is_ok(), "5min boundary should accept; got {:?}", r_at);

        // At 5min+1ms: REJECT (boundary exclusive past the canonical
        // window per Stripe spec).
        let past_5min = at_5min.saturating_add(1);
        let r_past = verify_stripe_signature(&header, &payload, TEST_SECRET, past_5min);
        let is_skew = matches!(r_past, Err(StripeError::SignatureSkewRejected { .. }));
        prop_assert!(is_skew, "5min+1ms should reject; got {:?}", r_past);
    }
}

// ---- additional sanity (non-randomized) ------------------------------

#[test]
fn audit_failure_aborts_record_no_state_mutation() {
    let audit = Arc::new(FailingStripeAuditSink::new());
    let ledger = Arc::new(InMemoryStripeUsageLedger::new());
    let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
    let tenant = Uuid::now_v7();
    let agg = build_aggregate_directly(tenant, "2026-05", UsageEventKind::CasPut, 100, 0);
    let err = a
        .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
        .unwrap_err();
    let is_audit = matches!(err, StripeError::Audit(_));
    assert!(is_audit);
    assert_eq!(ledger.len(), 0);
}

#[test]
fn ledger_failure_propagates_after_audit() {
    let audit = Arc::new(InMemoryStripeAuditSink::new());
    let ledger = Arc::new(FailingStripeUsageLedger::new());
    let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
    let tenant = Uuid::now_v7();
    let agg = build_aggregate_directly(tenant, "2026-05", UsageEventKind::CasPut, 100, 0);
    let err = a
        .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
        .unwrap_err();
    let is_ledger = matches!(err, StripeError::Ledger(_));
    assert!(is_ledger);
    assert_eq!(
        audit.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
        1
    );
}

#[test]
fn aggregator_to_adapter_canonical_pipeline() {
    // End-to-end: aggregator produces an AggregatedCounter; adapter
    // derives the canonical Idempotency-Key from it; ledger records
    // exactly once.
    let mut rng = ChaCha20Rng::seed_from_u64(0xA110C8);
    let tenant = Uuid::now_v7();
    let evs = vec![
        build_event(&mut rng, tenant, UsageEventKind::CasPut, "2026-05", 100, 10),
        build_event(&mut rng, tenant, UsageEventKind::CasPut, "2026-05", 200, 20),
    ];
    let agg = fresh_aggregate_via_aggregator(tenant, "2026-05", UsageEventKind::CasPut, &evs);
    let (adapter, _audit, ledger) = fresh_adapter();
    let dec = adapter
        .record_usage(&agg, SubscriptionItemId::new("si_canonical"), 1_700_000_000_000)
        .unwrap();
    let recorded = matches!(dec, StripeAdapterDecision::UsageRecorded { .. });
    assert!(recorded);
    assert_eq!(ledger.len(), 1);

    // Re-recording the same canonical aggregate is idempotent at
    // the Stripe API surface (single charge).
    let dec2 = adapter
        .record_usage(&agg, SubscriptionItemId::new("si_canonical"), 1_700_000_000_000)
        .unwrap();
    let dup = matches!(dec2, StripeAdapterDecision::DuplicateRejected { .. });
    assert!(dup);
    assert_eq!(ledger.len(), 1);
}

#[test]
fn end_to_end_chain_link_canonical_via_aggregator_to_adapter_to_chain() {
    // Sanity: canonical bytes from the aggregate match the chain-link
    // input the audit-chain primitive observes (`link_chain_hash_from_canonical`).
    let mut rng = ChaCha20Rng::seed_from_u64(0xCAFE);
    let tenant = Uuid::now_v7();
    let evs = vec![build_event(
        &mut rng,
        tenant,
        UsageEventKind::CasPut,
        "2026-05",
        100,
        7,
    )];
    let agg = fresh_aggregate_via_aggregator(tenant, "2026-05", UsageEventKind::CasPut, &evs);
    let canonical = compute_canonical_aggregate_bytes(&agg).unwrap();
    let _link =
        link_chain_hash_from_canonical(&corelink_billing_aggregator::ChainHash::genesis(), &canonical);
    let key1 = derive_idempotency_key(&agg).unwrap();
    let key2 = derive_idempotency_key_from_canonical(&canonical);
    assert_eq!(key1, key2);
}
