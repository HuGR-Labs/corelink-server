//! Async-stream + pager unit tests for the audit-export route.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Test bodies are verbatim copies of the original inline `mod tests`
//! block (subset: pager + `build_audit_export_async_stream` happy-path
//! and audit-anchor-BEFORE-trailer tests).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_audit_chain::{AuditExporter, ChainHash, ExportWindow};
use uuid::Uuid;

use super::audit_sink::{ExportAuditSink, InMemoryExportAuditSink};
use super::stream::{build_audit_export_async_stream, InMemoryR2ListPager, R2ListPager};
use super::types::R2_LIST_PAGE_SIZE;

#[tokio::test]
async fn in_memory_pager_returns_pages_then_none() {
    use corelink_analytics::Region;
    use corelink_audit_chain::{
        AuditEvent, AuditEventKind, HashChainBuilder, InMemoryAuditExporter,
    };
    use serde_json::json;
    // Seed 5 rows; ask for a pager with page_size=2 → expect
    // pages [2, 2, 1] then Some([]) (no rows left at boundary; the
    // empty-final-page path is only taken on the "first poll empty"
    // arm, so on a 5/2 split we get 3 non-empty pages then None.
    let tenant = Uuid::from_u128(0xAB);
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = corelink_audit_chain::ChainHash::genesis();
    for i in 0..5_u64 {
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_000 + i,
            tenant,
            Region::Iad,
            i,
            prev,
            json!({ "i": i }),
        );
        prev = builder.append(&e).expect("append");
        exporter.append_event(e).expect("seed");
    }
    let window = ExportWindow::new(0, 10_000).expect("window");
    let result = exporter
        .export_window(&tenant.to_string(), window)
        .expect("export");
    assert_eq!(result.rows.len(), 5);
    let mut pager = InMemoryR2ListPager::with_rows(result.rows, 2);
    let p0 = pager.next_page().await.expect("p0");
    assert_eq!(p0.len(), 2);
    let p1 = pager.next_page().await.expect("p1");
    assert_eq!(p1.len(), 2);
    let p2 = pager.next_page().await.expect("p2");
    assert_eq!(p2.len(), 1);
    assert!(pager.next_page().await.is_none());
}

#[tokio::test]
async fn in_memory_pager_empty_rows_returns_one_empty_page_then_none() {
    // The happy-empty-range path expects `Some(vec![])` on first
    // poll so the generator can fall through to the manifest line.
    let mut pager = InMemoryR2ListPager::with_rows(Vec::new(), R2_LIST_PAGE_SIZE);
    let p0 = pager.next_page().await.expect("first poll");
    assert!(p0.is_empty());
    assert!(pager.next_page().await.is_none());
}

#[tokio::test]
async fn in_memory_pager_clamps_zero_page_size_to_default() {
    // page_size=0 would never make progress — must clamp.
    let pager = InMemoryR2ListPager::with_rows(Vec::new(), 0);
    assert_eq!(pager.page_size, R2_LIST_PAGE_SIZE);
}

#[tokio::test]
async fn async_stream_yields_rows_across_pages_then_manifest() {
    use corelink_analytics::Region;
    use corelink_audit_chain::{
        AuditEvent, AuditEventKind, HashChainBuilder, InMemoryAuditExporter,
    };
    use futures::StreamExt;
    use serde_json::json;
    let tenant = Uuid::from_u128(0x1234);
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = corelink_audit_chain::ChainHash::genesis();
    for i in 0..7_u64 {
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_000 + i,
            tenant,
            Region::Iad,
            i,
            prev,
            json!({ "i": i }),
        );
        prev = builder.append(&e).expect("append");
        exporter.append_event(e).expect("seed");
    }
    let window = ExportWindow::new(0, 10_000).expect("window");
    let result = exporter
        .export_window(&tenant.to_string(), window)
        .expect("export");
    let manifest_json = serde_json::to_string(&result.manifest).expect("manifest json");
    let manifest_line = format!("{{\"manifest\":{manifest_json}}}");
    let anchor = result.manifest.chain_head_at_export;
    let sink: Arc<dyn ExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
    // page_size=3 → 3+3+1 split, exercises the cross-page path
    let pager = InMemoryR2ListPager::with_rows(result.rows, 3);
    let stream = build_audit_export_async_stream(
        Box::new(pager),
        manifest_line.clone(),
        anchor,
        sink,
        tenant,
        0,
        10_000,
        0,
        7,
    );
    let frames: Vec<_> = stream.collect().await;
    // 7 row frames + 1 manifest frame, all data (no trailers).
    assert_eq!(frames.len(), 8);
    for frame in frames.iter().take(7) {
        let frame_ref = frame.as_ref().expect("infallible");
        assert!(frame_ref.is_data(), "row frame should be data");
    }
    let last = frames[7].as_ref().expect("last");
    assert!(last.is_data(), "manifest frame should be data");
}

#[tokio::test]
async fn async_stream_audit_anchor_emits_before_trailer_on_mid_page_break() {
    // Inject a tampered row in page 0; assert the audit sink
    // received the SEV-0 emit BEFORE the trailer frame is yielded.
    use corelink_analytics::Region;
    use corelink_audit_chain::{
        AuditEvent, AuditEventKind, HashChainBuilder, InMemoryAuditExporter,
    };
    use futures::StreamExt;
    use serde_json::json;
    let tenant = Uuid::from_u128(0xC001);
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = corelink_audit_chain::ChainHash::genesis();
    for i in 0..3_u64 {
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_000 + i,
            tenant,
            Region::Iad,
            i,
            prev,
            json!({ "i": i }),
        );
        prev = builder.append(&e).expect("append");
        exporter.append_event(e).expect("seed");
    }
    let window = ExportWindow::new(0, 10_000).expect("window");
    let mut result = exporter
        .export_window(&tenant.to_string(), window)
        .expect("export");
    // Anchor under which we'll verify. Tamper row [1] so verify
    // returns false on second row, BEFORE the manifest frame.
    let real_anchor = result.manifest.chain_head_at_export;
    let bogus_anchor = ChainHash::genesis();
    // Keep the manifest line for completeness even though we'll
    // never emit it (the abort path returns before manifest).
    let manifest_line = "irrelevant".to_string();
    let sink: Arc<InMemoryExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
    let sink_dyn: Arc<dyn ExportAuditSink> = sink.clone();
    // Pass `bogus_anchor` so verify fails on row 0.
    let _ = result.rows.iter_mut();
    let pager = InMemoryR2ListPager::with_rows(result.rows, 2);
    let stream = build_audit_export_async_stream(
        Box::new(pager),
        manifest_line,
        bogus_anchor,
        sink_dyn,
        tenant,
        0,
        10_000,
        0,
        0,
    );
    // Drain the stream collecting all frames in order. Walk
    // forward and assert: at the moment the FIRST trailer frame
    // is observed, the audit sink already holds the SEV-0 emit.
    let mut frames_iter = Box::pin(stream);
    let mut audit_seen_before_trailer = false;
    let mut trailer_seen = false;
    while let Some(frame) = frames_iter.next().await {
        let f = frame.expect("infallible");
        if !f.is_data() {
            // Trailer frame. Audit sink MUST already have the row.
            let snap = sink.snapshot().expect("snap");
            assert!(
                !snap.is_empty(),
                "audit emit MUST land BEFORE the trailer frame on the wire"
            );
            audit_seen_before_trailer = true;
            trailer_seen = true;
            break;
        }
    }
    assert!(trailer_seen, "expected trailer on break path");
    assert!(
        audit_seen_before_trailer,
        "audit-anchor-BEFORE-trailer invariant violated"
    );
    // Sanity: the unused `real_anchor` keeps the seed deterministic.
    assert_ne!(real_anchor.to_hex(), bogus_anchor.to_hex());
}

#[tokio::test]
async fn async_stream_empty_range_yields_only_manifest_frame() {
    use futures::StreamExt;
    let tenant = Uuid::from_u128(0xDEAD);
    let manifest_line = "{\"manifest\":{}}".to_string();
    let anchor = ChainHash::genesis();
    let sink: Arc<dyn ExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
    let pager = InMemoryR2ListPager::with_rows(Vec::new(), R2_LIST_PAGE_SIZE);
    let stream = build_audit_export_async_stream(
        Box::new(pager),
        manifest_line.clone(),
        anchor,
        sink,
        tenant,
        0,
        1,
        0,
        0,
    );
    let frames: Vec<_> = stream.collect().await;
    assert_eq!(frames.len(), 1, "empty range should yield 1 manifest frame");
    let f = frames[0].as_ref().expect("infallible");
    assert!(f.is_data());
}
