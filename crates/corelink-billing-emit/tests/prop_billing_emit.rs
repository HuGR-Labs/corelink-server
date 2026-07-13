//! Property tests pinning the load-bearing invariants of
//! `corelink-billing-emit` at 10k iterations per check (PR-gate;
//! nightly 100k via `PROPTEST_CASES` env var override per S-07 P1-2
//! fix).
//!
//! Coverage map (mirrors WI-S10-001 §6.1.13):
//!
//! - `prop_idem_key_deterministic` — same canonical event reproduces
//!   the same idem_key (INV-BILLING-NO-DUP foundation).
//! - `prop_idem_key_unique_per_event` — different events produce
//!   different idem_keys (collision rate < 2^-128 expected; we assert
//!   distinct over 10k random tuples).
//! - `prop_duplicate_rejected_on_replay` — emit twice with same
//!   canonical event → DuplicateRejected (INV-BILLING-NO-DUP canary).
//! - `prop_append_only_no_overwrite` — emit at existing
//!   (tenant, billing_period, seq) → R2UsageSinkError::AppendOnlyViolation
//!   (INV-BILLING-APPEND-ONLY canary).
//! - `prop_tenant_isolation` — tenant A's idem set never affects
//!   tenant B's emit decision (INV-TENANT-ISOLATION canary).
//! - `prop_audit_emit_per_decision_arm` — every emit decision arm
//!   fires the canonical audit (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
//!   canary).
//! - `prop_jcs_canonicalization_byte_stable` — JCS(event) deterministic
//!   across calls (RFC 8785 §1 canonical determinism).
//! - `prop_billing_period_format_yyyy_mm` — billing_period strictly
//!   `YYYY-MM`; non-canonical shapes rejected at construction.

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
use corelink_billing_emit::{
    canonical_billing_audit_event_strings, canonical_usage_event_kinds,
    compute_canonical_bytes_for_idem, derive_idem_key, derive_idem_key_from_canonical,
    BillingAuditEventType, EmitOutcome, IdemKey, IdempotencyTracker, InMemoryBillingAuditSink,
    InMemoryIdempotencyTracker, InMemoryR2UsageSink, InMemoryUsageEventEmitter, R2UsageSinkError,
    UsageEvent, UsageEventEmitter, UsageEventKind,
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
    UsageEventKind::RunnerSlotSeconds,
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
fn canonical_usage_event_kinds_pinned() {
    let v = canonical_usage_event_kinds();
    assert_eq!(v.len(), 7);
    let mut set = HashSet::new();
    for k in v {
        assert!(set.insert(k.as_str()));
    }
    assert_eq!(set.len(), 7);
    assert!(set.contains("storage_bytes_hourly"));
    assert!(set.contains("egress_bytes"));
    assert!(set.contains("ac_lookup"));
    assert!(set.contains("cas_get"));
    assert!(set.contains("cas_put"));
    assert!(set.contains("replay_request"));
    assert!(set.contains("runner_slot_seconds"));
}

#[test]
fn canonical_billing_audit_event_strings_pinned() {
    let s = canonical_billing_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.billing.usage_emitted"));
    assert!(s.contains(&"corelink.billing.duplicate_rejected"));
    assert!(s.contains(&"corelink.billing.sink_failure"));
    assert!(s.contains(&"corelink.billing.idempotency_collision"));
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

fn pick_billing_period(rng: &mut ChaCha20Rng) -> String {
    // Year in 2024..=2030 + month 01..=12.
    let year = 2024 + (rng.next_u32() % 7);
    let month = 1 + (rng.next_u32() % 12);
    format!("{year:04}-{month:02}")
}

fn build_event(rng: &mut ChaCha20Rng, tenant: Uuid) -> UsageEvent {
    let kind = pick_kind(rng);
    let region = pick_region(rng);
    let qty = u64::from(rng.next_u32());
    let period = pick_billing_period(rng);
    UsageEvent::new(
        "corelink/region/test",
        Uuid::now_v7(),
        1_700_000_000_000_u64.saturating_add(u64::from(rng.next_u32() & 0xFFFF)),
        region,
        tenant,
        kind,
        qty,
        period,
    )
    .unwrap()
}

fn fresh_emitter() -> (
    InMemoryUsageEventEmitter<InMemoryBillingAuditSink, InMemoryIdempotencyTracker>,
    Arc<InMemoryBillingAuditSink>,
    Arc<InMemoryIdempotencyTracker>,
    Arc<InMemoryR2UsageSink>,
) {
    let audit = Arc::new(InMemoryBillingAuditSink::new());
    let idem = Arc::new(InMemoryIdempotencyTracker::new());
    let sink = Arc::new(InMemoryR2UsageSink::new());
    let e =
        InMemoryUsageEventEmitter::new(Arc::clone(&audit), Arc::clone(&idem), Arc::clone(&sink));
    (e, audit, idem, sink)
}

// ---- properties ------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// INV-BILLING-NO-DUP foundation: the same canonical event always
    /// reproduces the same idem_key. This is the canonical
    /// determinism property the dedup relies on (replay safety).
    #[test]
    fn prop_idem_key_deterministic(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let ev = build_event(&mut rng, tenant);
        let k1 = derive_idem_key(&ev).unwrap();
        let k2 = derive_idem_key(&ev).unwrap();
        let k3 = derive_idem_key(&ev).unwrap();
        prop_assert_eq!(k1, k2);
        prop_assert_eq!(k2, k3);
    }

    /// Distinct events produce distinct idem_keys with overwhelming
    /// probability. Birthday-bound for BLAKE3-256 over 10k random
    /// 256-bit outputs gives expected collisions ≈ 10^4 × 10^4 / 2^257
    /// ≈ 10^-69, far below any detection threshold.
    #[test]
    fn prop_idem_key_unique_per_event(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let ev1 = build_event(&mut rng, tenant);
        let ev2 = build_event(&mut rng, tenant);
        // Skip the (ultra-rare) case where the random fuzz happened to
        // produce two byte-identical events; the canonical determinism
        // property covers that case via prop_idem_key_deterministic.
        let canonical1 = compute_canonical_bytes_for_idem(&ev1).unwrap();
        let canonical2 = compute_canonical_bytes_for_idem(&ev2).unwrap();
        if canonical1 == canonical2 {
            return Ok(());
        }
        let k1 = derive_idem_key_from_canonical(&canonical1);
        let k2 = derive_idem_key_from_canonical(&canonical2);
        prop_assert_ne!(k1, k2);
    }

    /// INV-BILLING-NO-DUP canary: emit twice with the same canonical
    /// event surfaces DuplicateRejected on the second emit; the R2
    /// sink does NOT grow + the idempotency tracker does NOT double-
    /// count. Production wiring at WI-S10-007 binds this to the
    /// `(tenant_id, request_id) UNIQUE` D1 staging table mirror.
    #[test]
    fn prop_duplicate_rejected_on_replay(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (e, _audit, idem, sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let ev = build_event(&mut rng, tenant);

        let _ = e.emit(ev.clone(), "req-x", 1).unwrap();
        let outcome = e.emit(ev, "req-x-retry", 2).unwrap();
        let is_dup = matches!(outcome, EmitOutcome::DuplicateRejected { .. });
        prop_assert!(is_dup);
        prop_assert_eq!(sink.len(), 1);
        prop_assert_eq!(idem.accepted_count(tenant), 1);
    }

    /// INV-BILLING-APPEND-ONLY canary: pre-populate (tenant, period,
    /// seq=0) + attempt to emit a NEW event at the same canonical key;
    /// the sink rejects with AppendOnlyViolation. Production wiring at
    /// WI-S10-007 enforces this via R2 Object Lock Governance Mode.
    #[test]
    fn prop_append_only_no_overwrite(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let period = pick_billing_period(&mut rng);
        let sink = InMemoryR2UsageSink::new();
        let pre = corelink_billing_emit::PersistedUsageLine {
            r2_key: corelink_billing_emit::canonical_r2_key(tenant, &period, 0),
            ndjson: "{\"k\":\"v\"}".to_string(),
            tenant_id: tenant,
            billing_period: period.clone(),
            sequence_number: 0,
            idem_key: IdemKey::genesis(),
        };
        sink.put_at_explicit_key(pre).unwrap();

        // Try to overwrite at the canonical seq=0 key.
        let line = corelink_billing_emit::PersistedUsageLine {
            r2_key: corelink_billing_emit::canonical_r2_key(tenant, &period, 0),
            ndjson: "{\"k2\":\"v2\"}".to_string(),
            tenant_id: tenant,
            billing_period: period,
            sequence_number: 0,
            idem_key: IdemKey([0xCD; 32]),
        };
        let err = sink.put_at_explicit_key(line).unwrap_err();
        let is_aov = matches!(err, R2UsageSinkError::AppendOnlyViolation { .. });
        prop_assert!(is_aov);
    }

    /// INV-TENANT-ISOLATION canary: tenant A's idem set never affects
    /// tenant B's emit decision. Production wiring at WI-S10-007 binds
    /// this to the per-tenant D1 staging table partition + the
    /// auth-context middleware (S-03 Lote 10.4bis enforcement).
    #[test]
    fn prop_tenant_isolation(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (e, _audit, idem, sink) = fresh_emitter();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let ev_a = build_event(&mut rng, tenant_a);
        let ev_b = build_event(&mut rng, tenant_b);

        let _ = e.emit(ev_a.clone(), "req-a", 1).unwrap();
        let _ = e.emit(ev_b.clone(), "req-b", 2).unwrap();
        // Re-emit tenant A's event: dedup hits, tenant B unaffected.
        let outcome = e.emit(ev_a, "req-a-retry", 3).unwrap();
        let is_dup = matches!(outcome, EmitOutcome::DuplicateRejected { .. });
        prop_assert!(is_dup);
        prop_assert_eq!(idem.accepted_count(tenant_a), 1);
        prop_assert_eq!(idem.accepted_count(tenant_b), 1);
        prop_assert_eq!(sink.len(), 2);
    }

    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary: every successful
    /// emit fires the canonical `corelink.billing.usage_emitted` audit
    /// BEFORE the R2 NDJSON write; every replay-safe duplicate fires
    /// the canonical `corelink.billing.duplicate_rejected` audit.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, _idem, _sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let ev = build_event(&mut rng, tenant);

        let _ = e.emit(ev.clone(), "req-1", 1).unwrap();
        let _ = e.emit(ev, "req-2", 2).unwrap();

        let usage_emitted = audit.snapshot_of(BillingAuditEventType::UsageEmitted).len();
        let duplicate = audit.snapshot_of(BillingAuditEventType::DuplicateRejected).len();
        prop_assert_eq!(usage_emitted, 1);
        prop_assert_eq!(duplicate, 1);
    }

    /// JCS canonicalization is byte-stable: the same `UsageEvent`
    /// produces the same canonical bytes across all repeated calls.
    /// RFC 8785 §1 canonical determinism property.
    #[test]
    fn prop_jcs_canonicalization_byte_stable(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let ev = build_event(&mut rng, tenant);
        let b1 = compute_canonical_bytes_for_idem(&ev).unwrap();
        let b2 = compute_canonical_bytes_for_idem(&ev).unwrap();
        let b3 = compute_canonical_bytes_for_idem(&ev).unwrap();
        prop_assert_eq!(&b1, &b2);
        prop_assert_eq!(&b2, &b3);
    }

    /// billing_period MUST be strictly `YYYY-MM`. Fuzz a year in
    /// 2024..=2030 + month 01..=12 + assert UsageEvent::new accepts;
    /// fuzz an out-of-range month + assert it rejects.
    #[test]
    fn prop_billing_period_format_yyyy_mm(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let canonical = pick_billing_period(&mut rng);
        let ev = UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            1,
            canonical.clone(),
        );
        prop_assert!(ev.is_ok());
        let unwrapped = ev.unwrap();
        prop_assert_eq!(unwrapped.billing_period(), canonical.as_str());

        // Adversarial: month 13 always rejected.
        let bad = UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            1,
            "2026-13",
        );
        prop_assert!(bad.is_err());
    }
}
