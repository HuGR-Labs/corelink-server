//! libFuzzer harness — BLAKE3 hash-chain append determinism +
//! insertion-order dependence.
//!
//! Note on naming: the corelink-audit-chain primitive is a *linear*
//! BLAKE3-256 hash chain (`next = BLAKE3(prev_hash || canonical_bytes)`)
//! rather than a balanced Merkle tree. The fuzz target is named
//! `merkle_append` per the spec sheet for cross-crate naming
//! consistency. The properties asserted match the chain's actual
//! discipline:
//!
//! 1. **Determinism.** Replaying the same `(seed, event_seq)` MUST
//!    produce the same chain head. This is the SOC 2 CC7.2 audit-chain
//!    integrity invariant; any nondeterminism = chain tampering
//!    masquerading as a serde drift.
//! 2. **Insertion-order dependence.** Reversing the order of two
//!    consecutive events MUST produce a different chain head (i.e.
//!    chain is NOT commutative; reorder-as-tamper detection).
//! 3. **No panic.** Random `(kind, tenant, time_ms, sequence_number,
//!    prev_hash, data)` permutations must never abort — they MUST
//!    return `Ok(_)` or `Err(AuditChainError::*)`.

#![no_main]

use corelink_analytics::Region;
use corelink_audit_chain::{
    chain::HashChainBuilder, AuditEvent, AuditEventKind, ChainHash,
};
use libfuzzer_sys::fuzz_target;
use serde_json::json;
use uuid::Uuid;

fn kinds() -> [AuditEventKind; 8] {
    [
        AuditEventKind::Tenant,
        AuditEventKind::CasPut,
        AuditEventKind::CasGet,
        AuditEventKind::AcLookup,
        AuditEventKind::GcPurge,
        AuditEventKind::AuthLogin,
        AuditEventKind::QuotaExceeded,
        AuditEventKind::AbuseDetected,
    ]
}

fn mk_event(
    kind: AuditEventKind,
    seq: u64,
    prev: ChainHash,
    tenant: Uuid,
    time_ms: u64,
    payload_seed: u8,
) -> AuditEvent {
    AuditEvent::new(
        kind,
        "corelink/fuzz",
        Uuid::from_u128(u128::from(seq) ^ u128::from(payload_seed)),
        time_ms,
        tenant,
        Region::Iad,
        seq,
        prev,
        json!({ "seq": seq, "seed": payload_seed }),
    )
}

fuzz_target!(|data: &[u8]| {
    // Need at least: 1 byte tenant-disambig + 1 byte time + 1 byte
    // count + 2 bytes per event for (kind, seed).
    if data.len() < 7 {
        return;
    }

    let tenant_seed = u128::from(data[0]);
    let tenant = Uuid::from_u128(tenant_seed);
    let time_base = u64::from(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
    let n = (usize::from(data[5]) % 16).max(2); // 2..=16 events
    let mut seeds_kinds: Vec<(AuditEventKind, u8)> = Vec::with_capacity(n);
    let kind_set = kinds();
    let mut idx = 6;
    for _ in 0..n {
        if idx + 1 >= data.len() {
            break;
        }
        let k = kind_set[usize::from(data[idx]) % kind_set.len()];
        let seed = data[idx + 1];
        seeds_kinds.push((k, seed));
        idx += 2;
    }
    if seeds_kinds.len() < 2 {
        return;
    }

    // ── (1) Build chain twice — determinism ──────────────────────────
    fn build(
        tenant: Uuid,
        time_base: u64,
        events: &[(AuditEventKind, u8)],
    ) -> Option<ChainHash> {
        let mut b = HashChainBuilder::new();
        let mut head = ChainHash::genesis();
        for (i, (kind, seed)) in events.iter().enumerate() {
            let e = mk_event(*kind, i as u64, head, tenant, time_base.wrapping_add(i as u64), *seed);
            head = b.append(&e).ok()?;
        }
        Some(head)
    }

    let head1 = build(tenant, time_base, &seeds_kinds);
    let head2 = build(tenant, time_base, &seeds_kinds);
    assert_eq!(
        head1, head2,
        "BLAKE3 hash chain MUST be deterministic across two builds with the same event sequence"
    );

    // ── (2) Insertion-order dependence ──────────────────────────────
    // Swap the first two events. If they encode different (kind, seed)
    // the chain head MUST change. If they're byte-identical the head
    // legitimately equals (no contradiction).
    if seeds_kinds[0] != seeds_kinds[1] {
        let mut swapped = seeds_kinds.clone();
        swapped.swap(0, 1);
        let head_swapped = build(tenant, time_base, &swapped);
        if let (Some(h1), Some(h2)) = (head1, head_swapped) {
            assert_ne!(
                h1, h2,
                "swapping two distinct events MUST change the chain head — \
                 chain is NOT order-commutative; reorder-tamper detection"
            );
        }
    }
});
