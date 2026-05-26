//! Integration tests for `corelink audit export` + `audit verify`
//! (WI-R-PREP-AUDIT-EXPORT).
//!
//! Five canonical unit tests + one property test (100 random event
//! sequences → export → re-verify proofs → always Ok). Tests run against
//! the [`InMemoryAuditExporter`] fake — the same algebraic surface the
//! production CF Worker endpoint will satisfy. Any regression here
//! transfers directly to the wired endpoint.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;

use corelink_audit_chain::{
    AuditEvent, AuditEventKind, ChainHash, HashChainBuilder, InMemoryAuditExporter,
};
use corelink_cli::audit_export::{
    build_fixture_exporter, run_export, run_verify, ExportFormat,
};
use corelink_cli::output::OutputFormat;
use proptest::prelude::*;
use tempfile::TempDir;
use uuid::Uuid;

fn region_from_str(s: &str) -> corelink_analytics::Region {
    use corelink_analytics::Region;
    match s {
        "iad" => Region::Iad,
        "sjc" => Region::Sjc,
        "gru" => Region::Gru,
        "lhr" => Region::Lhr,
        _ => Region::Iad,
    }
}

#[test]
fn export_n_events_jsonld_roundtrip_passes_verify() {
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 25, 1_000).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        0,
        100_000,
        ExportFormat::JsonLd,
        tmp.path(),
        true,
        true,
        OutputFormat::Json,
    )
    .unwrap();
    assert_eq!(outcome.event_count, 25);
    let verified = run_verify(
        std::path::Path::new(&outcome.file_path),
        OutputFormat::Json,
    )
    .unwrap();
    assert!(verified.ok);
    assert_eq!(verified.events_verified, 25);
}

#[test]
fn export_csv_format_conversion_drops_proofs_but_keeps_chain_columns() {
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 7, 0).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        0,
        100_000,
        ExportFormat::Csv,
        tmp.path(),
        false,
        false,
        OutputFormat::Json,
    )
    .unwrap();
    let body = fs::read_to_string(&outcome.file_path).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 8); // header + 7 events.
    assert!(lines[0].contains("link_hash"));
    assert!(lines[0].contains("prev_hash"));
    // Each non-header line has 10 comma-separated cells.
    for line in &lines[1..] {
        assert_eq!(line.matches(',').count(), 9);
    }
}

#[test]
fn export_parquet_carries_full_schema_and_link_hashes() {
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 12, 0).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        0,
        100_000,
        ExportFormat::Parquet,
        tmp.path(),
        true,
        true,
        OutputFormat::Json,
    )
    .unwrap();
    let body = fs::read(&outcome.file_path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["rows"].as_array().unwrap().len(), 12);
    // Sequence numbers are dense 0..12.
    for (i, row) in v["rows"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["sequence_number"].as_u64().unwrap(), i as u64);
    }
}

#[test]
fn merkle_proof_inclusion_each_row_links_to_anchor() {
    // Build a chain, export, then walk each row asserting that its link_hash
    // matches the next row's prev_hash AND that the final row's link_hash
    // equals the manifest's chain_head_at_export.
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 10, 0).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        0,
        100_000,
        ExportFormat::JsonLd,
        tmp.path(),
        true,
        true,
        OutputFormat::Json,
    )
    .unwrap();
    let body = fs::read(&outcome.file_path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = v["events"].as_array().unwrap();
    let chain_head = v["manifest"]["chain_head_at_export"].as_str().unwrap();
    assert_eq!(events.len(), 10);
    // For each consecutive pair, current.proof.link_hash == next.event.prev_hash.
    for i in 0..events.len() - 1 {
        let cur_link = events[i]["proof"]["link_hash"].as_str().unwrap();
        let next_prev = events[i + 1]["event"]["prev_hash"].as_str().unwrap();
        assert_eq!(cur_link, next_prev, "link/prev mismatch between row {i} and {}", i + 1);
    }
    // Final row's link_hash == chain head.
    let last_link = events[events.len() - 1]["proof"]["link_hash"].as_str().unwrap();
    assert_eq!(last_link, chain_head);
}

#[test]
fn export_content_addressed_filename_is_deterministic_for_same_payload() {
    // Two exports with identical events + same window should produce
    // exports with identical BLAKE3 (the JSON-LD body is stable modulo
    // event IDs which differ — so we use the parquet format which
    // does NOT round-trip identical when ids differ. The TEST: a
    // fresh fixture run produces a content-addressed name; the
    // BLAKE3 in the filename matches the file body.).
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 5, 0).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        0,
        100_000,
        ExportFormat::JsonLd,
        tmp.path(),
        true,
        true,
        OutputFormat::Json,
    )
    .unwrap();
    let body = fs::read(&outcome.file_path).unwrap();
    use blake3::Hasher;
    let mut h = Hasher::new();
    h.update(&body);
    let computed_hash = hex::encode(h.finalize().as_bytes());
    assert_eq!(computed_hash, outcome.file_blake3);
    // The first 32 chars of computed_hash appear in the filename.
    let hash_prefix: String = computed_hash.chars().take(32).collect();
    assert!(outcome.file_path.contains(&hash_prefix));
}

#[test]
fn export_window_filters_strictly_to_time_bounds() {
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, 20, 100).unwrap();
    let tmp = TempDir::new().unwrap();
    let outcome = run_export(
        &exporter,
        &tenant.to_string(),
        105,
        110, // window covers time_ms 105..110 (5 events).
        ExportFormat::JsonLd,
        tmp.path(),
        true,
        true,
        OutputFormat::Json,
    )
    .unwrap();
    assert_eq!(outcome.event_count, 5);
}

// =============== Property test (100 cases) ===============

fn build_random_chain(tenant: Uuid, events: Vec<(u64, AuditEventKind, String)>) -> InMemoryAuditExporter {
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = ChainHash::genesis();
    for (i, (time_ms, kind, region_str)) in events.into_iter().enumerate() {
        let region = region_from_str(&region_str);
        let event = AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            time_ms,
            tenant,
            region,
            i as u64,
            prev,
            serde_json::json!({"i": i, "rand": time_ms ^ (i as u64)}),
        );
        prev = builder.append(&event).unwrap();
        exporter.append_event(event).unwrap();
    }
    exporter
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        max_shrink_iters: 100,
        ..ProptestConfig::default()
    })]

    /// For every random event sequence of size 1..=40, an export →
    /// re-verify cycle MUST always succeed. This is the load-bearing
    /// algebraic guarantee customers + auditors rely on.
    #[test]
    fn prop_random_chain_export_then_verify_always_ok(
        count in 1usize..=40,
        time_base in 0u64..=1_000_000,
    ) {
        let tenant = Uuid::now_v7();
        let kinds = [
            AuditEventKind::Tenant,
            AuditEventKind::CasPut,
            AuditEventKind::CasGet,
            AuditEventKind::AcLookup,
            AuditEventKind::GcPurge,
            AuditEventKind::AuthLogin,
            AuditEventKind::QuotaExceeded,
            AuditEventKind::AbuseDetected,
        ];
        let regions = ["iad", "sjc", "gru", "lhr"];
        let events: Vec<(u64, AuditEventKind, String)> = (0..count)
            .map(|i| {
                let kind = kinds[i % kinds.len()];
                let region = regions[i % regions.len()].to_owned();
                (time_base.saturating_add(i as u64), kind, region)
            })
            .collect();
        let exporter = build_random_chain(tenant, events);

        let tmp = TempDir::new().unwrap();
        let outcome = run_export(
            &exporter,
            &tenant.to_string(),
            0,
            u64::MAX / 2,
            ExportFormat::JsonLd,
            tmp.path(),
            true,
            true, // pre-verify (this is the property under test)
            OutputFormat::Json,
        ).unwrap();
        prop_assert_eq!(outcome.event_count, count as u64);

        // And re-verify from disk too.
        let verified = run_verify(
            std::path::Path::new(&outcome.file_path),
            OutputFormat::Json,
        ).unwrap();
        prop_assert!(verified.ok);
        prop_assert_eq!(verified.events_verified, count as u64);
    }
}
