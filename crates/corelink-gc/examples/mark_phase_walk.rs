//! Walk a single mark phase end-to-end against the in-memory fakes.
//! Use this as the canonical "mark phase happy path" smoke for the
//! 3-pass scan + reachable-set computation + candidate emission flow.
//!
//! Run with:
//!     cargo run --example mark_phase_walk -p corelink-gc

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "example: prints + ergonomic unwraps for clarity"
)]

use std::sync::Arc;

use corelink_gc::{
    AcMetaRow, BlobDigest, BlobMetaRow, CountingMarkClock, GcCandidatesStore, GcRegion, GcRunStore,
    InMemoryGcAuditSink, InMemoryGcCandidatesStore, InMemoryGcMetrics, InMemoryGcRunStore,
    InMemoryMarkPhase, InMemoryReachableSetSource, ManifestChunkRow, MarkPhase, RunId,
};
use uuid::Uuid;

fn digest(seed: u8) -> BlobDigest {
    let mut s = format!("{seed:02x}");
    s.push_str(&"0".repeat(BlobDigest::LEN - 2));
    BlobDigest::parse(&s).unwrap()
}

fn main() {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let source = Arc::new(InMemoryReachableSetSource::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingMarkClock::new(1_700_000_000_000));
    let mark = InMemoryMarkPhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&source),
        Arc::clone(&candidates),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );

    let tenant = Uuid::from_u128(42);
    let rid = RunId(Uuid::from_u128(0xa11ce));
    runs.insert_pending(rid, tenant, GcRegion::Sam, 1, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 2).unwrap();

    // Corpus: 1 live blob, 1 orphan, 1 ac-only protected, 1 manifest-only.
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(1),
            refcount: 1,
            size_bytes: 1024,
            last_referenced_at_ms: 500,
        },
    );
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(2),
            refcount: 0,
            size_bytes: 2048,
            last_referenced_at_ms: 500,
        },
    );
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(3),
            refcount: 0,
            size_bytes: 4096,
            last_referenced_at_ms: 500,
        },
    );
    source.push_ac_meta(
        tenant,
        AcMetaRow {
            blob_refs: vec![digest(3)],
            created_at_ms: 600,
        },
    );
    source.push_blob_meta(
        tenant,
        BlobMetaRow {
            digest: digest(4),
            refcount: 0,
            size_bytes: 8192,
            last_referenced_at_ms: 500,
        },
    );
    source.push_manifest_chunk(
        tenant,
        ManifestChunkRow {
            chunk_digest: digest(4),
            created_at_ms: 700,
        },
    );

    let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
    println!("mark phase result:");
    println!("  mark_started_at_ms = {}", result.mark_started_at_ms);
    println!("  rows_scanned       = {}", result.rows_scanned_count);
    println!("  reachable_blobs    = {}", result.reachable_blobs_count);
    println!("  candidates         = {}", result.candidates_count);
    println!("  duration_ms        = {}", result.mark_duration_ms);
    println!("  batches            = {}", result.batches_processed);

    let snap = candidates.snapshot_for_run(tenant, rid).unwrap();
    println!("\ncandidates emitted:");
    for c in snap {
        println!(
            "  digest={} size={}B status={}",
            c.digest,
            c.blob_size_bytes,
            c.status.as_str(),
        );
    }
}
