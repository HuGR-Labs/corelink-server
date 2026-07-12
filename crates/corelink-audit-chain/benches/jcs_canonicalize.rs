//! Criterion benchmark — RFC 8785 JCS canonicalization on canonical
//! audit-event sizes (100B / 1KB / 10KB `data` payloads).
//!
//! # Targets
//!
//! - 100 B payload: p99 < 30 µs.
//! - 1 KB payload: p99 < 100 µs.
//! - 10 KB payload: p99 < 1 ms.
//!
//! JCS sits on the audit-chain hot path: every event emit canonicalizes
//! the entire `AuditEvent` shape (event-kind metadata + chain link
//! slots + `data` tree). A regression here directly inflates the
//! SLO-LATENCY-AUDIT-EMIT budget (see `specs/03_architecture/slo_catalog.md`).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use corelink_analytics::Region;
use corelink_audit_chain::{
    chain::compute_canonical_bytes,
    event::{AuditEvent, AuditEventKind, ChainHash},
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde_json::json;
use std::hint::black_box;
use uuid::Uuid;

/// Build a synthetic `data` payload of approximately `target_bytes` size.
fn make_payload(target_bytes: usize) -> serde_json::Value {
    // ASCII-only string padding so JCS UTF-8 cost is predictable.
    let pad = "x".repeat(target_bytes.saturating_sub(40));
    json!({"k": pad, "n": 42, "ok": true})
}

fn make_event(data: serde_json::Value) -> AuditEvent {
    AuditEvent::new(
        AuditEventKind::CasPut,
        "corelink/region/iad",
        Uuid::now_v7(),
        1_700_000_000_000_u64,
        Uuid::now_v7(),
        Region::Iad,
        0,
        ChainHash::genesis(),
        data,
    )
}

fn bench_jcs(c: &mut Criterion) {
    let mut g = c.benchmark_group("audit_chain/jcs_canonicalize");
    for &size in &[100usize, 1024, 10_240] {
        let payload = make_payload(size);
        let event = make_event(payload);
        let canonical_size = compute_canonical_bytes(&event).expect("canonical").len();
        g.throughput(Throughput::Bytes(canonical_size as u64));
        g.bench_with_input(
            BenchmarkId::from_parameter(format!("{size}B-data")),
            &event,
            |b, event| {
                b.iter(|| {
                    let bytes = compute_canonical_bytes(black_box(event)).expect("canonical");
                    black_box(bytes);
                });
            },
        );
    }
    g.finish();
}

criterion_group!(benches, bench_jcs);
criterion_main!(benches);
