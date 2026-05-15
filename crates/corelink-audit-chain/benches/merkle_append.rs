//! Criterion benchmark — audit-chain BLAKE3 hash-chain append + 10k-event
//! chain-head compute.
//!
//! # Targets
//!
//! - `chain/append_single`: p99 < 200 µs per event (JCS + BLAKE3 link).
//! - `chain/append_10k`: total wall-clock for 10k sequential appends —
//!   serves as the regression alarm for chain-head compute time. Soft
//!   target: < 2 s on a release build (i.e. ≤ 200 µs per event amortised).
//!
//! Note: the audit chain is a BLAKE3 hash chain (Bitcoin-block-header
//! pattern) — NOT a Merkle tree. The "merkle_append" filename is
//! preserved from the WI spec but the implementation reflects the actual
//! chain shape (`next_hash = BLAKE3(prev_hash || JCS-canonical-bytes)`).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_analytics::Region;
use corelink_audit_chain::{
    chain::HashChainBuilder,
    event::{AuditEvent, AuditEventKind, ChainHash},
};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use serde_json::json;
use uuid::Uuid;

fn make_event(seq: u64, prev: ChainHash, tenant: Uuid) -> AuditEvent {
    AuditEvent::new(
        AuditEventKind::CasPut,
        "corelink/region/iad",
        Uuid::now_v7(),
        1_700_000_000_000_u64.saturating_add(seq),
        tenant,
        Region::Iad,
        seq,
        prev,
        json!({"size_bytes": 1024, "blob_hash": "blake3:bench"}),
    )
}

fn bench_append_single(c: &mut Criterion) {
    let tenant = Uuid::now_v7();
    c.bench_function("audit_chain/append_single", |b| {
        b.iter_batched(
            || {
                let builder = HashChainBuilder::new();
                let e = make_event(0, ChainHash::genesis(), tenant);
                (builder, e)
            },
            |(mut builder, e)| {
                let h = builder.append(black_box(&e)).expect("append");
                black_box(h);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_append_10k(c: &mut Criterion) {
    let tenant = Uuid::now_v7();
    let mut g = c.benchmark_group("audit_chain/append_10k");
    g.sample_size(10);
    g.throughput(Throughput::Elements(10_000));
    g.bench_function("sequential", |b| {
        b.iter(|| {
            let mut builder = HashChainBuilder::new();
            let mut prev = ChainHash::genesis();
            for seq in 0u64..10_000 {
                let e = make_event(seq, prev, tenant);
                prev = builder.append(black_box(&e)).expect("append");
            }
            black_box(prev);
        });
    });
    g.finish();
}

criterion_group!(benches, bench_append_single, bench_append_10k);
criterion_main!(benches);
