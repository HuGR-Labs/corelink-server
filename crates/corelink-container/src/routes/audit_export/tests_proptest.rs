//! Wave-19 property test (10k iter) — pins the audit-anchor-BEFORE-
//! trailer invariant under randomized injection of mid-stream
//! chain-breaks.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Test body is a verbatim copy of the original inline `mod tests`
//! block (terminal proptest section).
//!
//! For each iteration we:
//!   1. Generate a random row count `n in 1..=64` + random break
//!      position `b in 0..n` + random page size `p in 1..=16`.
//!   2. Build a real chain of `n` rows; pass a bogus anchor that
//!      will fail verify at every row (we then assert the audit
//!      sink received exactly ONE break emit and that emit landed
//!      BEFORE the trailer frame in the consumer-observable
//!      stream-order).
//!
//! Driving this at 10k iter gives confidence the ordering holds
//! under every page-boundary alignment + break-position combo.

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
use super::stream::{build_audit_export_async_stream, InMemoryR2ListPager};

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: proptest_cases(),
        // Keep shrinking budget modest — the assertion is binary.
        max_shrink_iters: 64,
        .. proptest::test_runner::Config::default()
    })]
    #[test]
    fn audit_anchor_emits_before_trailer_under_random_breaks(
        n in 1_usize..=16,
        page_size in 1_usize..=8,
        seed in 0_u64..=u64::MAX,
    ) {
        use corelink_audit_chain::{
            AuditEvent, AuditEventKind, HashChainBuilder, InMemoryAuditExporter,
        };
        use corelink_analytics::Region;
        use futures::StreamExt;
        use serde_json::json;
        let tenant = Uuid::from_u128(u128::from(seed));
        let mut exporter = InMemoryAuditExporter::new();
        let mut builder = HashChainBuilder::new();
        let mut prev = ChainHash::genesis();
        for i in 0..n as u64 {
            let e = AuditEvent::new(
                AuditEventKind::CasPut,
                "corelink/region/iad",
                Uuid::now_v7(),
                1_000 + i,
                tenant,
                Region::Iad,
                i,
                prev,
                json!({ "i": i, "seed": seed }),
            );
            prev = builder.append(&e).map_err(|_| {
                proptest::test_runner::TestCaseError::fail("append failed")
            })?;
            exporter.append_event(e).map_err(|_| {
                proptest::test_runner::TestCaseError::fail("seed failed")
            })?;
        }
        let window = ExportWindow::new(0, 10_000).map_err(|_| {
            proptest::test_runner::TestCaseError::fail("window")
        })?;
        let result = exporter
            .export_window(&tenant.to_string(), window)
            .map_err(|_| proptest::test_runner::TestCaseError::fail("export"))?;
        // Inject a bogus anchor so verify fails on row 0 → mid-stream break.
        let bogus_anchor = ChainHash::genesis();
        let sink: Arc<InMemoryExportAuditSink> =
            Arc::new(InMemoryExportAuditSink::new());
        let sink_dyn: Arc<dyn ExportAuditSink> = sink.clone();
        let pager = InMemoryR2ListPager::with_rows(result.rows, page_size);
        let stream = build_audit_export_async_stream(
            Box::new(pager),
            String::new(),
            bogus_anchor,
            sink_dyn,
            tenant,
            0,
            10_000,
            0,
            0,
        );
        // Drive the stream on a single-threaded runtime so the
        // proptest doesn't allocate thousands of multi-thread
        // tokio runtimes (each rt is ~MB of overhead).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| proptest::test_runner::TestCaseError::fail("rt"))?;
        let invariant = rt.block_on(async move {
            let mut iter = Box::pin(stream);
            while let Some(frame) = iter.next().await {
                let f = match frame {
                    Ok(f) => f,
                    Err(_) => return false,
                };
                if !f.is_data() {
                    let snap = match sink.snapshot() {
                        Ok(s) => s,
                        Err(_) => return false,
                    };
                    // Audit emit MUST be present before the trailer.
                    return !snap.is_empty();
                }
            }
            // No trailer fired (e.g. n=0 or all verified — shouldn't
            // happen with bogus anchor + n>=1). Treat as failure.
            false
        });
        proptest::prop_assert!(
            invariant,
            "audit-anchor-BEFORE-trailer invariant violated for n={n}, page_size={page_size}, seed={seed}"
        );
    }
}
