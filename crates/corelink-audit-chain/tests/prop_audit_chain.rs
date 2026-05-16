//! Property tests pinning the load-bearing invariants of
//! `corelink-audit-chain` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S09-004 §6.1.11):
//!
//! - `prop_chain_append_only` — chain only grows; never rewrites
//!   (INV-AUDIT-APPEND-ONLY canary).
//! - `prop_chain_verify_passes_on_unmodified` — full chain verifies
//!   clean (INV-OBS-AUDIT-CHAIN-INTEGRITY positive case).
//! - `prop_chain_break_detected_on_tamper` — modify any event → verify
//!   fails at first tampered seq (INV-OBS-AUDIT-CHAIN-INTEGRITY
//!   negative case).
//! - `prop_genesis_zero_prev_hash` — first event has zero prev_hash
//!   (Bitcoin-genesis-block convention).
//! - `prop_chain_sequence_monotonic` — seq strictly increases by 1.
//! - `prop_jcs_canonicalization_deterministic` — JCS(event) byte-stable
//!   across iterations.
//! - `prop_tenant_isolation` — tenant A chain never references tenant B
//!   events (INV-TENANT-ISOLATION canary).
//! - `prop_audit_emit_per_event_type` — every chain emit + verify
//!   decision arm fires the canonical audit (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::{
    canonical_audit_event_kinds, canonical_audit_event_strings, compute_canonical_bytes,
    ArchiveReceipt, AuditChainAuditEventType, AuditChainError, AuditEvent, AuditEventKind,
    ChainHash, ChainVerifier, HashChainBuilder, InMemoryAuditChainAuditSink,
    InMemoryNeonShadowSink, InMemoryR2AuditSink, InMemoryShadowSyncAuditSink, NeonShadowError,
    NeonShadowSink, ShadowEventRow, GENESIS_PREV_HASH, GENESIS_SEQUENCE_NUMBER,
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

const ALL_KINDS: &[AuditEventKind] = &[
    AuditEventKind::Tenant,
    AuditEventKind::CasPut,
    AuditEventKind::CasGet,
    AuditEventKind::AcLookup,
    AuditEventKind::GcPurge,
    AuditEventKind::AuthLogin,
    AuditEventKind::QuotaExceeded,
    AuditEventKind::AbuseDetected,
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
fn canonical_audit_event_kinds_pinned() {
    let v = canonical_audit_event_kinds();
    assert_eq!(v.len(), 8);
    let mut set = std::collections::HashSet::new();
    for k in v {
        assert!(set.insert(k.subject()));
    }
    assert_eq!(set.len(), 8);
    assert!(set.contains("tenant"));
    assert!(set.contains("cas:put"));
    assert!(set.contains("cas:get"));
    assert!(set.contains("ac:lookup"));
    assert!(set.contains("gc:purge"));
    assert!(set.contains("auth:login"));
    assert!(set.contains("quota:exceeded"));
    assert!(set.contains("abuse:detected"));
}

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.audit_chain.event_appended"));
    assert!(s.contains(&"corelink.audit_chain.chain_verified_ok"));
    assert!(s.contains(&"corelink.audit_chain.chain_break_detected"));
    assert!(s.contains(&"corelink.audit_chain.sink_failure"));
}

#[test]
fn genesis_constants_pinned() {
    assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
    assert_eq!(GENESIS_SEQUENCE_NUMBER, 0);
}

// ---- helpers ---------------------------------------------------------

fn pick_kind(rng: &mut ChaCha20Rng) -> AuditEventKind {
    let idx = (rng.next_u32() as usize) % ALL_KINDS.len();
    ALL_KINDS[idx]
}

fn pick_region(rng: &mut ChaCha20Rng) -> Region {
    let idx = (rng.next_u32() as usize) % ALL_REGIONS.len();
    ALL_REGIONS[idx]
}

fn build_chain(
    sink: &InMemoryR2AuditSink<InMemoryAuditChainAuditSink>,
    tenant: Uuid,
    n: u64,
    rng: &mut ChaCha20Rng,
) -> Vec<AuditEvent> {
    let mut chain = Vec::new();
    for i in 0..n {
        let (seq, prev) = sink.next_link_inputs(tenant);
        let kind = pick_kind(rng);
        let region = pick_region(rng);
        let ev = AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(i),
            tenant,
            region,
            seq,
            prev,
            serde_json::json!({"i": i, "kind": kind.subject()}),
        );
        sink.emit(ev.clone(), "req-x", 1_700_000_000_000_u64.saturating_add(i))
            .unwrap();
        chain.push(ev);
    }
    chain
}

// ---- properties ------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// INV-AUDIT-APPEND-ONLY canary: chain only grows; the head sequence
    /// after `n` emits is exactly `n` and the buffer length is exactly
    /// `n`. No emit ever rewrites a prior event.
    #[test]
    fn prop_chain_append_only(
        seed in any::<u64>(),
        chain_len in 0u64..50u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        let _ = build_chain(&sink, tenant, chain_len, &mut rng);
        prop_assert_eq!(sink.next_sequence(tenant), chain_len);
        prop_assert_eq!(sink.snapshot_for_tenant(tenant).len() as u64, chain_len);
        // Buffer is monotone-increasing in sequence number; no rewrites.
        let snap = sink.snapshot_for_tenant(tenant);
        for (i, line) in snap.iter().enumerate() {
            prop_assert_eq!(line.sequence_number, i as u64);
        }
    }

    /// INV-OBS-AUDIT-CHAIN-INTEGRITY positive case: a full chain
    /// verifies clean (no break, all events accounted for).
    #[test]
    fn prop_chain_verify_passes_on_unmodified(
        seed in any::<u64>(),
        chain_len in 1u64..30u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let verifier = ChainVerifier::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        let chain = build_chain(&sink, tenant, chain_len, &mut rng);
        let outcome = verifier
            .verify_chain_from_genesis(&chain, tenant, "verifier", 1)
            .unwrap();
        prop_assert_eq!(outcome.events_verified_count, chain_len);
        prop_assert!(outcome.first_break_at_seq.is_none());
        // Independent recomputation matches producer-side chain head
        // (the falsifiability target of INV-OBS-AUDIT-CHAIN-INTEGRITY).
        prop_assert_eq!(outcome.last_verified_hash, sink.chain_head(tenant).unwrap());
    }

    /// INV-OBS-AUDIT-CHAIN-INTEGRITY negative case: tampering any event
    /// in a chain of ≥ 2 events surfaces a `ChainBreak` error at some
    /// sequence in the chain. The verifier never returns Ok on a
    /// tampered chain.
    ///
    /// **Tamper-mode coverage**: hash chains have a subtle but
    /// canonical property — tampering the LAST event's `data` alone is
    /// structurally undetected because the link is carried by the NEXT
    /// event's `prev_hash` slot (and there is no next event). We
    /// cover both cases:
    /// 1. Tamper non-last `data` → detected via next event's
    ///    prev_hash mismatch.
    /// 2. Tamper any event's own `prev_hash` slot → detected at
    ///    THAT event's position (its claimed prev_hash diverges from
    ///    the running chain head).
    ///
    /// The tamper-mode is selected deterministically from the rng so
    /// shrinking is reproducible.
    #[test]
    fn prop_chain_break_detected_on_tamper(
        seed in any::<u64>(),
        chain_len in 2u64..30u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let verifier = ChainVerifier::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        let mut chain = build_chain(&sink, tenant, chain_len, &mut rng);
        // Two tamper modes; mode 0 tampers non-last `data` (detected at
        // tamper_idx + 1 via prev_hash mismatch); mode 1 tampers any
        // event's prev_hash slot (detected at tamper_idx itself).
        let mode = rng.next_u32() % 2;
        let tamper_idx = match mode {
            // Non-last position for data-tamper.
            0 => (rng.next_u32() as u64 % (chain_len.saturating_sub(1))) as usize,
            // Any position for prev_hash-tamper (event 0 included; flipping
            // genesis prev_hash from zero to non-zero is detected at event 0).
            _ => (rng.next_u32() as u64 % chain_len) as usize,
        };
        if mode == 0 {
            chain[tamper_idx].data = serde_json::json!({"tampered": true, "at_idx": tamper_idx});
        } else {
            let mut bytes = *chain[tamper_idx].prev_hash.as_bytes();
            bytes[0] ^= 0xFF;
            chain[tamper_idx].prev_hash = ChainHash(bytes);
        }
        let res = verifier.verify_chain_from_genesis(&chain, tenant, "verifier", 1);
        let is_chain_break = matches!(&res, Err(AuditChainError::ChainBreak { .. }));
        prop_assert!(is_chain_break, "expected ChainBreak; got {res:?} (mode={mode}, idx={tamper_idx})");
    }

    /// First event in any chain has `prev_hash == [0u8; 32]` AND
    /// `sequence_number == 0`. Bitcoin-genesis-block convention; WI §1
    /// invariant 1.
    #[test]
    fn prop_genesis_zero_prev_hash(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        // build_chain with len=1 produces only the genesis.
        let chain = build_chain(&sink, tenant, 1, &mut rng);
        let g = &chain[0];
        prop_assert!(g.is_genesis());
        prop_assert_eq!(g.sequence_number, 0);
        prop_assert_eq!(g.prev_hash.as_bytes(), &GENESIS_PREV_HASH);
    }

    /// Sequence numbers strictly increase by 1 starting from 0. The
    /// builder rejects out-of-order emits at the boundary.
    #[test]
    fn prop_chain_sequence_monotonic(
        seed in any::<u64>(),
        chain_len in 1u64..30u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        let chain = build_chain(&sink, tenant, chain_len, &mut rng);
        for (i, ev) in chain.iter().enumerate() {
            prop_assert_eq!(ev.sequence_number, i as u64);
        }
        // Builder also pins this at the trait surface: try to append
        // an event with the wrong sequence + assert SequenceOrderingViolation.
        let mut builder = HashChainBuilder::new();
        let bad_event = AuditEvent::new(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000,
            tenant,
            Region::Iad,
            42, // wrong: expected 0
            ChainHash::genesis(),
            serde_json::json!({}),
        );
        let err = builder.append(&bad_event).unwrap_err();
        let is_seq_violation = matches!(err, AuditChainError::SequenceOrderingViolation { .. });
        prop_assert!(is_seq_violation);
    }

    /// JCS canonicalization is byte-stable: the same `AuditEvent`
    /// produces the same canonical bytes across all repeated calls.
    /// RFC 8785 §1 canonical determinism property.
    #[test]
    fn prop_jcs_canonicalization_deterministic(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let tenant = Uuid::now_v7();
        let kind = pick_kind(&mut rng);
        let region = pick_region(&mut rng);
        let seq = rng.next_u32() as u64 % 1000;
        let ev = AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(seq),
            tenant,
            region,
            seq,
            ChainHash([rng.next_u32() as u8; 32]),
            serde_json::json!({"k": "v", "n": seq}),
        );
        let b1 = compute_canonical_bytes(&ev).unwrap();
        let b2 = compute_canonical_bytes(&ev).unwrap();
        let b3 = compute_canonical_bytes(&ev).unwrap();
        prop_assert_eq!(&b1, &b2);
        prop_assert_eq!(&b2, &b3);
    }

    /// INV-TENANT-ISOLATION canary: tenant A's chain never references
    /// tenant B's events. Chains are partitioned per-tenant in the
    /// sink; the verifier rejects cross-tenant slices at the boundary.
    #[test]
    fn prop_tenant_isolation(
        seed in any::<u64>(),
        chain_len in 1u64..15u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let verifier = ChainVerifier::new(Arc::clone(&audit));
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let chain_a = build_chain(&sink, tenant_a, chain_len, &mut rng);
        let _chain_b = build_chain(&sink, tenant_b, chain_len, &mut rng);
        // Sink partitions by tenant.
        let snap_a = sink.snapshot_for_tenant(tenant_a);
        let snap_b = sink.snapshot_for_tenant(tenant_b);
        prop_assert_eq!(snap_a.len() as u64, chain_len);
        prop_assert_eq!(snap_b.len() as u64, chain_len);
        for line in &snap_a {
            prop_assert_eq!(line.tenant_id, tenant_a);
        }
        for line in &snap_b {
            prop_assert_eq!(line.tenant_id, tenant_b);
        }
        // Verifier rejects a cross-tenant slice (tenant_a chain
        // verified under tenant_b expectation surfaces
        // TenantIsolationViolation).
        if !chain_a.is_empty() {
            let res = verifier.verify_chain_from_genesis(&chain_a, tenant_b, "verifier", 1);
            let is_iso = matches!(&res, Err(AuditChainError::TenantIsolationViolation { .. }));
            prop_assert!(is_iso);
        }
        // Per-tenant chains verify cleanly under their own tenant.
        let outcome_a = verifier
            .verify_chain_from_genesis(&chain_a, tenant_a, "verifier", 1)
            .unwrap();
        prop_assert!(outcome_a.first_break_at_seq.is_none());
    }

    /// Every chain emit fires a canonical
    /// `corelink.audit_chain.event_appended` audit BEFORE buffer
    /// mutation; every verifier decision arm fires the canonical audit
    /// (`chain_verified_ok` for clean walks; `chain_break_detected`
    /// for break detection).
    #[test]
    fn prop_audit_emit_per_event_type(
        seed in any::<u64>(),
        chain_len in 1u64..15u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
        let verifier = ChainVerifier::new(Arc::clone(&audit));
        let tenant = Uuid::now_v7();
        let chain = build_chain(&sink, tenant, chain_len, &mut rng);
        // Audit count for EventAppended == chain length.
        prop_assert_eq!(
            audit
                .snapshot_of(AuditChainAuditEventType::EventAppended)
                .len() as u64,
            chain_len
        );
        // Run a clean verifier round.
        let _ = verifier
            .verify_chain_from_genesis(&chain, tenant, "verifier", 1)
            .unwrap();
        prop_assert!(!audit
            .snapshot_of(AuditChainAuditEventType::ChainVerifiedOk)
            .is_empty());
        // Tamper a copy + run verifier; assert ChainBreakDetected
        // audit fires. Tamper the LAST event's prev_hash slot (always
        // detected at the last event itself, regardless of chain length).
        let mut tampered = chain.clone();
        let last = tampered.len().saturating_sub(1);
        let mut bytes = *tampered[last].prev_hash.as_bytes();
        bytes[0] ^= 0xFF;
        tampered[last].prev_hash = ChainHash(bytes);
        let _ = verifier.verify_chain_from_genesis(&tampered, tenant, "verifier", 2);
        prop_assert!(!audit
            .snapshot_of(AuditChainAuditEventType::ChainBreakDetected)
            .is_empty());
    }

    /// INV-AUTH-SCHEMA-RLS-DEFAULT-ON + INV-TENANT-ISOLATION canary
    /// (Wave-20 fix-stream, finding B-P1-01).
    ///
    /// Models the SQL-layer RLS `WITH CHECK` gate added in
    /// `migrations/neon/0002_audit_events_shadow_with_check.sql`. The
    /// `InMemoryNeonShadowSink` is the test substrate for the production
    /// `RealNeonShadowSink` + Postgres RLS pair; this proptest fixes the
    /// sink's bound tenant as `tenant_a` (≡ `app.current_tenant = A` in
    /// SQL) but submits a chunk whose `tenant_id` is the distinct
    /// `tenant_b` (≡ INSERT carrying `tenant_id = B`). The defence-in-
    /// depth invariant: every such cross-tenant INSERT MUST be rejected
    /// fail-CLOSED — neither the app-layer pin (this sink) nor the
    /// SQL-layer `WITH CHECK` gate may admit the row. 10k iterations.
    #[test]
    fn prop_cross_tenant_insert_rejected_by_rls_with_check_or_app_pin(
        seed in any::<u64>(),
        seq_offset in 0u64..1000u64,
        event_time_offset in 0u64..1000u64,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let _ = rng.next_u64();
        // Deterministic distinct tenants (drawn from rng so the
        // shrinker can collapse to a minimal failing pair if any).
        let tenant_a = Uuid::now_v7();
        let mut tenant_b_bytes = [0u8; 16];
        rng.fill_bytes(&mut tenant_b_bytes);
        // Ensure tenant_b ≠ tenant_a (clear collision in the unlikely
        // case the v7 timestamp + rng path collide). If equal, flip
        // a high byte — Uuid::now_v7 monotonic + per-process so the
        // collision space is effectively empty.
        let tenant_b = Uuid::from_bytes(tenant_b_bytes);
        prop_assume!(tenant_b != tenant_a);

        let region = corelink_analytics::Region::Iad;
        let audit_emit = Arc::new(InMemoryShadowSyncAuditSink::new());
        // Sink bound to tenant_a (SQL equivalent: SET LOCAL
        // app.current_tenant = '<tenant_a uuid>').
        let shadow_a = InMemoryNeonShadowSink::new(tenant_a, region, audit_emit.clone());

        let base_ms = 1_700_000_000_000u64;
        // Cross-tenant INSERT attempt: row carries tenant_b.
        let cross_row = ShadowEventRow::new(
            tenant_b, // <-- cross-tenant; RLS WITH CHECK + app pin MUST reject.
            seq_offset,
            base_ms.saturating_add(event_time_offset),
            "corelink.cas.put.v1".to_string(),
            ChainHash::genesis(),
            ChainHash([0xAB; 32]),
            region,
            "{}".to_string(),
        );
        // The receipt carries tenant_b — exercises the receipt-level
        // pre-check arm (the FIRST defence layer).
        let receipt_cross = ArchiveReceipt {
            r2_key: "audit/2023/11/14/00000000.ndjson".to_string(),
            tenant_id: tenant_b,
            first_event_time_ms: base_ms.saturating_add(event_time_offset),
            last_event_time_ms: base_ms.saturating_add(event_time_offset),
            first_sequence_number: seq_offset,
            last_sequence_number: seq_offset,
            prev_hash_anchor: ChainHash::genesis(),
            chain_head_after: ChainHash([0xAB; 32]),
            bytes_written: 1,
            events_written: 1,
        };
        let err = shadow_a
            .sync_chunk(&receipt_cross, &[cross_row], base_ms.saturating_add(event_time_offset))
            .expect_err("cross-tenant INSERT must be rejected");
        let is_tenant_iso = matches!(err, NeonShadowError::TenantIsolationViolation { .. });
        prop_assert!(is_tenant_iso);
        // The shadow must remain empty — no cross-tenant row landed.
        prop_assert_eq!(shadow_a.row_count(), 0);
        // tenant_a's aggregate query must see zero rows.
        let buckets = shadow_a
            .aggregate_event_count(0, u64::MAX, None)
            .expect("aggregate ok");
        prop_assert!(buckets.is_empty());
    }
}
