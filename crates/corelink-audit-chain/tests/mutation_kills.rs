//! Targeted regression tests that close mutation-testing surface
//! coverage gaps for `corelink-audit-chain`. Written 2026-05-15 as
//! part of the mutation-baseline expansion (specs/_audits/2026-05-15-
//! mutation-expansion.md).
//!
//! A full local `cargo mutants -p corelink-audit-chain` run is
//! INFEASIBLE on a laptop because the property test suite alone runs
//! 10k iterations × 6 props (default PROPTEST_CASES) ≈ ~37s of
//! `cargo test` per cycle × ~180 mutants = ~110 min wall-clock; with
//! the unavoidable rustc rebuild overhead it pushes past the 2h
//! ceiling. The CI nightly workflow (`.github/workflows/mutation-
//! nightly.yml`) runs the full sweep at the 75 % per-crate floor.
//!
//! These tests target the canonical mutation surfaces empirically
//! observed in earlier baselines: `as_str`-style canonical strings on
//! enum variants, match-arm equality, comparison operators on the
//! chain verifier, and constant-return mutants on observable surfaces.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use std::sync::Arc;

use corelink_audit_chain::{
    audit_chain_schema_version, canonical_audit_event_kinds,
    canonical_audit_event_strings, canonical_date_yyyy_mm_dd, canonical_r2_key,
    link_chain_hash_from_canonical, AuditChainAuditEventType, AuditEvent, AuditEventKind,
    AuditExporter, ChainHash, ExportWindow, FailingAuditChainAuditSink, HashChainBuilder,
    InMemoryAuditChainAuditSink, InMemoryAuditExporter, InMemoryR2AuditSink,
    AuditChainAuditSink, GENESIS_PREV_HASH, GENESIS_SEQUENCE_NUMBER,
    CLOUDEVENTS_DATACONTENTTYPE, CLOUDEVENTS_SPECVERSION, EVENT_TYPE_PREFIX,
};
use corelink_analytics::Region;
use uuid::Uuid;

// =====================================================================
// Crate-wide constants — kills `body -> 0` / `body -> default` mutants.
// =====================================================================

#[test]
fn schema_version_is_canonical_one() {
    assert_eq!(audit_chain_schema_version(), 1);
}

#[test]
fn cloudevents_constants_are_canonical_strings() {
    assert_eq!(CLOUDEVENTS_SPECVERSION, "1.0");
    assert_eq!(CLOUDEVENTS_DATACONTENTTYPE, "application/json");
    assert_eq!(EVENT_TYPE_PREFIX, "dev.hugr.corelink.");
    // Non-empty + non-canary.
    for s in [
        CLOUDEVENTS_SPECVERSION,
        CLOUDEVENTS_DATACONTENTTYPE,
        EVENT_TYPE_PREFIX,
    ] {
        assert!(!s.is_empty());
        assert_ne!(s, "xyzzy");
    }
}

#[test]
fn genesis_constants_are_canonical_zero() {
    assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
    assert_eq!(GENESIS_SEQUENCE_NUMBER, 0);
}

// =====================================================================
// AuditEventKind::subject + event_type — canonical 8-string taxonomy.
// =====================================================================

#[test]
fn audit_event_kind_subject_canonical_strings_per_variant() {
    assert_eq!(AuditEventKind::Tenant.subject(), "tenant");
    assert_eq!(AuditEventKind::CasPut.subject(), "cas:put");
    assert_eq!(AuditEventKind::CasGet.subject(), "cas:get");
    assert_eq!(AuditEventKind::AcLookup.subject(), "ac:lookup");
    assert_eq!(AuditEventKind::GcPurge.subject(), "gc:purge");
    assert_eq!(AuditEventKind::AuthLogin.subject(), "auth:login");
    assert_eq!(AuditEventKind::QuotaExceeded.subject(), "quota:exceeded");
    assert_eq!(AuditEventKind::AbuseDetected.subject(), "abuse:detected");
}

#[test]
fn audit_event_kind_subjects_distinct_and_eight_in_count() {
    let all = canonical_audit_event_kinds();
    assert_eq!(all.len(), 8);
    let subjects: Vec<&str> = all.iter().map(|k| k.subject()).collect();
    let mut sorted = subjects.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 8, "8 distinct canonical subjects");
    for s in subjects {
        assert!(!s.is_empty());
        assert_ne!(s, "xyzzy");
    }
}

#[test]
fn audit_event_kind_event_type_canonical_dev_hugr_prefix() {
    for k in canonical_audit_event_kinds() {
        let t = k.event_type();
        assert!(t.starts_with("dev.hugr.corelink."), "event_type must use canonical prefix: {t}");
        assert!(t.ends_with(".v1"), "event_type must be v1-suffixed: {t}");
    }
    // Pin specific mappings.
    assert_eq!(AuditEventKind::Tenant.event_type(), "dev.hugr.corelink.tenant.v1");
    assert_eq!(AuditEventKind::CasPut.event_type(), "dev.hugr.corelink.cas.put.v1");
    assert_eq!(AuditEventKind::AbuseDetected.event_type(), "dev.hugr.corelink.abuse.detected.v1");
}

#[test]
fn audit_event_kind_display_matches_subject() {
    for k in canonical_audit_event_kinds() {
        assert_eq!(format!("{}", k), k.subject());
    }
}

// =====================================================================
// AuditChainAuditEventType — 4-element meta-audit taxonomy + SEV class.
// =====================================================================

#[test]
fn meta_audit_event_type_canonical_strings_per_variant() {
    assert_eq!(
        AuditChainAuditEventType::EventAppended.as_str(),
        "corelink.audit_chain.event_appended"
    );
    assert_eq!(
        AuditChainAuditEventType::ChainVerifiedOk.as_str(),
        "corelink.audit_chain.chain_verified_ok"
    );
    assert_eq!(
        AuditChainAuditEventType::ChainBreakDetected.as_str(),
        "corelink.audit_chain.chain_break_detected"
    );
    assert_eq!(
        AuditChainAuditEventType::SinkFailure.as_str(),
        "corelink.audit_chain.sink_failure"
    );
}

#[test]
fn meta_audit_event_type_canonical_list_matches_variants() {
    let canonical = canonical_audit_event_strings();
    assert_eq!(canonical.len(), 4);
    let variant_strings = [
        AuditChainAuditEventType::EventAppended.as_str(),
        AuditChainAuditEventType::ChainVerifiedOk.as_str(),
        AuditChainAuditEventType::ChainBreakDetected.as_str(),
        AuditChainAuditEventType::SinkFailure.as_str(),
    ];
    for s in canonical.iter() {
        assert!(variant_strings.contains(s), "canonical {s} missing from variants");
    }
    // Distinct.
    let mut sorted: Vec<&str> = canonical.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 4);
}

#[test]
fn meta_audit_sev_classification_canonical() {
    // SEV-0: ChainBreakDetected only.
    assert!(AuditChainAuditEventType::ChainBreakDetected.is_sev0());
    assert!(!AuditChainAuditEventType::EventAppended.is_sev0());
    assert!(!AuditChainAuditEventType::ChainVerifiedOk.is_sev0());
    assert!(!AuditChainAuditEventType::SinkFailure.is_sev0());
    // SEV-1: SinkFailure only.
    assert!(AuditChainAuditEventType::SinkFailure.is_sev1());
    assert!(!AuditChainAuditEventType::EventAppended.is_sev1());
    assert!(!AuditChainAuditEventType::ChainVerifiedOk.is_sev1());
    assert!(!AuditChainAuditEventType::ChainBreakDetected.is_sev1());
    // No variant is BOTH SEV-0 and SEV-1 (each fires its own
    // alerting class; mutating `is_sev0 -> true` or `is_sev1 -> true`
    // would surface in this matrix).
    for v in [
        AuditChainAuditEventType::EventAppended,
        AuditChainAuditEventType::ChainVerifiedOk,
        AuditChainAuditEventType::ChainBreakDetected,
        AuditChainAuditEventType::SinkFailure,
    ] {
        assert!(!(v.is_sev0() && v.is_sev1()), "{v:?} cannot be both sev0+sev1");
    }
}

// =====================================================================
// ChainHash — genesis() + as_bytes() + to_hex() + Display + serde
// (mutations: body -> empty / canary).
// =====================================================================

#[test]
fn chain_hash_genesis_is_zero_array() {
    let g = ChainHash::genesis();
    assert_eq!(g.as_bytes(), &[0u8; 32]);
    assert_eq!(g.to_hex(), "0".repeat(64));
    assert_eq!(format!("{}", g), "0".repeat(64));
}

#[test]
fn chain_hash_to_hex_is_64_lowercase_chars_for_nonzero() {
    let h = ChainHash([0xAB; 32]);
    let s = h.to_hex();
    assert_eq!(s.len(), 64);
    assert_eq!(s, "ab".repeat(32));
    // Lowercase + hex-only.
    for c in s.chars() {
        assert!(c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase()));
    }
}

// =====================================================================
// link_chain_hash_from_canonical — BLAKE3 link primitive. Mutations
// to the function body to constants would collapse the chain.
// =====================================================================

#[test]
fn link_chain_hash_from_canonical_is_deterministic_and_distinguishes_inputs() {
    let prev = ChainHash::genesis();
    let bytes_a = b"event-A".as_slice();
    let bytes_b = b"event-B".as_slice();
    let h_a1 = link_chain_hash_from_canonical(&prev, bytes_a);
    let h_a2 = link_chain_hash_from_canonical(&prev, bytes_a);
    let h_b = link_chain_hash_from_canonical(&prev, bytes_b);

    assert_eq!(h_a1, h_a2, "deterministic on same input");
    assert_ne!(h_a1, h_b, "distinct inputs produce distinct hashes");
    // Hash is NOT the genesis (zero) hash — kills `body -> ChainHash::genesis()`.
    assert_ne!(h_a1.as_bytes(), &[0u8; 32]);
    // Hash depends on prev_hash too (chain dependency).
    let other_prev = ChainHash([1u8; 32]);
    let h_with_other_prev = link_chain_hash_from_canonical(&other_prev, bytes_a);
    assert_ne!(h_a1, h_with_other_prev);
}

// =====================================================================
// HashChainBuilder genesis state + sequence/head advance contract.
// =====================================================================

#[test]
fn hash_chain_builder_new_starts_at_genesis() {
    let b = HashChainBuilder::new();
    assert_eq!(b.head().as_bytes(), &GENESIS_PREV_HASH);
    assert_eq!(b.next_sequence(), GENESIS_SEQUENCE_NUMBER);
}

#[test]
fn hash_chain_builder_resume_round_trips_state() {
    let custom_head = ChainHash([0x42; 32]);
    let b = HashChainBuilder::resume(custom_head, 1234);
    assert_eq!(b.head().as_bytes(), &[0x42; 32]);
    assert_eq!(b.next_sequence(), 1234);
}

// =====================================================================
// sink::canonical_date_yyyy_mm_dd — pure-logic date conversion.
// Mutating arithmetic in the date formula yields wrong date string.
// =====================================================================

#[test]
fn canonical_date_yyyy_mm_dd_pins_canonical_vectors() {
    // Epoch (1970-01-01).
    assert_eq!(canonical_date_yyyy_mm_dd(0), "1970-01-01");
    // 1 day later.
    assert_eq!(canonical_date_yyyy_mm_dd(86_400_000), "1970-01-02");
    // 2024-01-01 — 54 years × ~365.25 ≈ 19_723 days from epoch.
    // Concrete: 2024-01-01 00:00:00 UTC = 1_704_067_200_000 ms.
    assert_eq!(
        canonical_date_yyyy_mm_dd(1_704_067_200_000),
        "2024-01-01"
    );
    // 2026-05-15 (today reference).
    // 2026-05-15 00:00:00 UTC = 1_778_803_200_000 ms
    // (1_778_889_600_000 ms = 2026-05-16 — verified empirically).
    assert_eq!(
        canonical_date_yyyy_mm_dd(1_778_889_600_000),
        "2026-05-16"
    );
}

#[test]
fn canonical_date_yyyy_mm_dd_format_is_iso8601_shape() {
    let d = canonical_date_yyyy_mm_dd(1_704_067_200_000);
    assert_eq!(d.len(), 10);
    let parts: Vec<&str> = d.split('-').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].len(), 4);
    assert_eq!(parts[1].len(), 2);
    assert_eq!(parts[2].len(), 2);
}

// =====================================================================
// sink::canonical_r2_key — composite key format pin.
// =====================================================================

#[test]
fn canonical_r2_key_format_pinned() {
    let tenant = Uuid::from_u128(0);
    let key = canonical_r2_key(tenant, 0, 0);
    // audit/{uuid}/1970-01-01/00000000.cloudevent.ndjson
    assert_eq!(
        key,
        format!(
            "audit/{}/1970-01-01/00000000.cloudevent.ndjson",
            tenant
        )
    );
    // Sequence zero-padded to 8 digits.
    let key_42 = canonical_r2_key(tenant, 0, 42);
    assert!(key_42.contains("/00000042.cloudevent.ndjson"));
    // Big sequence (still 8 digits or more — production strict 8 only
    // for seq < 10^8).
    let key_max = canonical_r2_key(tenant, 0, 99_999_999);
    assert!(key_max.contains("/99999999.cloudevent.ndjson"));
}

// =====================================================================
// InMemoryAuditChainAuditSink + FailingAuditChainAuditSink — sink
// contract pinning (kills body constant-return mutations).
// =====================================================================

fn rec(t: AuditChainAuditEventType) -> corelink_audit_chain::AuditChainAuditRecord {
    corelink_audit_chain::AuditChainAuditRecord {
        event_type: t,
        chain_subject: "cas:put",
        tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
        created_by_request_id: "mut-test".to_string(),
        now_ms: 1,
        sequence_number: 0,
    }
}

#[test]
fn in_memory_meta_audit_sink_is_empty_false_after_emit_and_len_tracks() {
    let sink = InMemoryAuditChainAuditSink::new();
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
    sink.emit(rec(AuditChainAuditEventType::EventAppended)).expect("emit");
    // Kills `is_empty -> true`.
    assert!(!sink.is_empty());
    assert_eq!(sink.len(), 1);
    sink.emit(rec(AuditChainAuditEventType::ChainVerifiedOk)).expect("emit");
    sink.emit(rec(AuditChainAuditEventType::ChainBreakDetected)).expect("emit");
    // Kills `len -> 0` / `len -> 1`.
    assert_eq!(sink.len(), 3);
}

#[test]
fn in_memory_meta_audit_sink_snapshot_of_filters_by_event_type() {
    let sink = InMemoryAuditChainAuditSink::new();
    sink.emit(rec(AuditChainAuditEventType::EventAppended)).expect("emit");
    sink.emit(rec(AuditChainAuditEventType::EventAppended)).expect("emit");
    sink.emit(rec(AuditChainAuditEventType::ChainVerifiedOk)).expect("emit");
    sink.emit(rec(AuditChainAuditEventType::SinkFailure)).expect("emit");

    let appended = sink.snapshot_of(AuditChainAuditEventType::EventAppended);
    assert_eq!(appended.len(), 2);
    let verified = sink.snapshot_of(AuditChainAuditEventType::ChainVerifiedOk);
    assert_eq!(verified.len(), 1);
    let failure = sink.snapshot_of(AuditChainAuditEventType::SinkFailure);
    assert_eq!(failure.len(), 1);
    let breakage = sink.snapshot_of(AuditChainAuditEventType::ChainBreakDetected);
    assert_eq!(breakage.len(), 0);
}

#[test]
fn cloned_in_memory_meta_audit_sink_shares_buffer() {
    let s1 = InMemoryAuditChainAuditSink::new();
    let s2: Arc<dyn AuditChainAuditSink> = Arc::new(s1.clone());
    s1.emit(rec(AuditChainAuditEventType::EventAppended)).expect("emit on s1");
    assert_eq!(s1.len(), 1);
    // The clone shares the buffer; emitting on s2 also reflects in s1.
    s2.emit(rec(AuditChainAuditEventType::ChainVerifiedOk)).expect("emit on s2");
    assert_eq!(s1.len(), 2);
}

#[test]
fn failing_meta_audit_sink_returns_store_error_with_message() {
    let sink = FailingAuditChainAuditSink::new();
    let err = sink.emit(rec(AuditChainAuditEventType::EventAppended)).unwrap_err();
    // Kills `emit -> Ok(())` mutation.
    let s = format!("{err}");
    assert!(s.contains("induced") || s.contains("audit"), "diagnostic must mention induced/audit: {s}");
}

// =====================================================================
// 2026-05-15 follow-on: kills the 26 missed mutants surfaced by the
// empirical `cargo mutants -p corelink-audit-chain` measured sweep
// (see specs/_audits/2026-05-15-mutation-full-sweep.md). 22 of the 26
// were `delete match arm` mutations on `event::region_from_str`
// (private serde helper) — covered indirectly via the public
// `AuditEvent::to_ndjson_line` → `from_ndjson_line` round-trip path
// for every Region variant. The remaining 4 target observable
// constant-return mutations on the exporter + sink + the
// `days_since_epoch_to_ymd` `(mp - 9)` arithmetic mutation.
// =====================================================================

/// Sweep every `Region` variant through the canonical NDJSON
/// round-trip. Kills 22 `delete match arm` mutations on
/// `event::region_from_str` (deserialize side): for each region, the
/// emitted line must round-trip back to the SAME enum variant; a
/// deleted match arm would cause that specific region to fail
/// deserialization (`unknown Region` serde error), which the round-
/// trip equality assertion catches.
#[test]
fn region_ndjson_round_trip_covers_every_variant() {
    // The 22 canonical regions (must mirror Region as_str list).
    let regions: [(Region, &str); 22] = [
        (Region::Iad, "iad"),
        (Region::Sjc, "sjc"),
        (Region::Dfw, "dfw"),
        (Region::Sea, "sea"),
        (Region::Ord, "ord"),
        (Region::Lhr, "lhr"),
        (Region::Fra, "fra"),
        (Region::Ams, "ams"),
        (Region::Cdg, "cdg"),
        (Region::Mad, "mad"),
        (Region::Gru, "gru"),
        (Region::Eze, "eze"),
        (Region::Bog, "bog"),
        (Region::Nrt, "nrt"),
        (Region::Sin, "sin"),
        (Region::Syd, "syd"),
        (Region::Hkg, "hkg"),
        (Region::Bom, "bom"),
        (Region::Icn, "icn"),
        (Region::Jnb, "jnb"),
        (Region::Cpt, "cpt"),
        (Region::Dxb, "dxb"),
    ];
    let tenant = Uuid::from_u128(0xDEAD_BEEF);
    let id = Uuid::from_u128(0xCAFE_BABE);
    for (region, code) in regions {
        let ev = AuditEvent::genesis(
            AuditEventKind::Tenant,
            "test-source",
            id,
            1_000,
            tenant,
            region,
            serde_json::json!({}),
        );
        let line = ev.to_ndjson_line().expect("serialize");
        // The wire payload must carry the canonical 3-letter code.
        assert!(
            line.contains(&format!("\"{code}\"")),
            "ndjson missing canonical code {code}: {line}"
        );
        // Round-trip yields the SAME variant; a deleted match arm in
        // `region_from_str` would surface here as a serde
        // "unknown Region" deserialization error or wrong-variant.
        let back = AuditEvent::from_ndjson_line(&line)
            .expect("deserialize must succeed for canonical region");
        assert_eq!(back.region, region, "round-trip preserves Region::{code}");
    }
}

/// Distinguishes every Region variant: 22 round-trips, 22 distinct
/// enums. A `delete match arm` collapsing two regions to the same
/// post-deserialization variant (e.g. all → first) would surface as a
/// duplicate; the sorted-dedup invariant catches that.
#[test]
fn region_ndjson_round_trip_distinguishes_all_22_variants() {
    let regions = [
        Region::Iad, Region::Sjc, Region::Dfw, Region::Sea, Region::Ord,
        Region::Lhr, Region::Fra, Region::Ams, Region::Cdg, Region::Mad,
        Region::Gru, Region::Eze, Region::Bog, Region::Nrt, Region::Sin,
        Region::Syd, Region::Hkg, Region::Bom, Region::Icn, Region::Jnb,
        Region::Cpt, Region::Dxb,
    ];
    let tenant = Uuid::from_u128(1);
    let id = Uuid::from_u128(2);
    let mut round_tripped: Vec<Region> = Vec::with_capacity(22);
    for region in regions {
        let ev = AuditEvent::genesis(
            AuditEventKind::Tenant,
            "src",
            id,
            1_000,
            tenant,
            region,
            serde_json::json!({}),
        );
        let line = ev.to_ndjson_line().expect("ser");
        let back = AuditEvent::from_ndjson_line(&line).expect("deser");
        round_tripped.push(back.region);
    }
    // Distinctness — kills a single-arm survivor collapsing variants.
    let mut sorted = round_tripped.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 22, "22 distinct regions after round-trip");
}

// =====================================================================
// `InMemoryAuditExporter::audit_emitted_count` — kills `-> Ok(0)` and
// `-> Ok(1)` constant-return mutations. After two distinct
// `export_window` calls the count MUST equal 2 (≠ 0 and ≠ 1).
// =====================================================================

#[test]
fn exporter_audit_emitted_count_reflects_real_emit_count() {
    let exp = InMemoryAuditExporter::new();
    // Pre-state: zero.
    assert_eq!(exp.audit_emitted_count().expect("guard"), 0);
    // Two distinct export_window calls on an empty tenant — each emits
    // one audit-of-audit record per the CTRL-AUDIT-001 fail-closed
    // envelope (export still completes for an empty tenant per
    // `empty_tenant_yields_empty_export`).
    exp.export_window("tenant-a", ExportWindow::new(0, 1_000).expect("window"))
        .expect("export");
    exp.export_window("tenant-b", ExportWindow::new(0, 2_000).expect("window"))
        .expect("export");
    // Kills `-> Ok(0)` AND `-> Ok(1)`: actual count is exactly 2.
    let count = exp.audit_emitted_count().expect("guard");
    assert_eq!(count, 2, "two export_window calls => two audit-of-audit records");
    // Cross-check via the snapshot path.
    let snap = exp.audit_emitted_snapshot().expect("snap");
    assert_eq!(snap.len(), 2);
}

// =====================================================================
// `days_since_epoch_to_ymd` arithmetic — kills `(mp - 9) -> (mp / 9)`
// at sink.rs:145:55. For January `mp == 10` (10-9 == 10/9 == 1, blind
// spot); for February `mp == 11` (11-9 == 2 ≠ 11/9 == 1). Existing
// `canonical_date_yyyy_mm_dd_pins_canonical_vectors` covers Jan only;
// this test adds a February vector to break the algebraic collision.
// =====================================================================

#[test]
fn canonical_date_february_kills_minus_to_divide_mutation() {
    // 2024-02-01 UTC = 1_706_745_600_000 ms (Feb has mp == 11; 11-9 == 2,
    // 11/9 == 1 — distinguishes the mutation).
    assert_eq!(canonical_date_yyyy_mm_dd(1_706_745_600_000), "2024-02-01");
    // 2026-02-15 UTC = 1_771_113_600_000 ms (second February vector).
    assert_eq!(canonical_date_yyyy_mm_dd(1_771_113_600_000), "2026-02-15");
    // March (mp == 12; 12-9 == 3 ≠ 12/9 == 1) — second-order check.
    // 2024-03-01 UTC = 1_709_251_200_000 ms.
    assert_eq!(canonical_date_yyyy_mm_dd(1_709_251_200_000), "2024-03-01");
    // November (mp == 8; 8-9 underflows on integer; never hit because
    // `mp < 10` branch takes over — defensive cross-check on the
    // OTHER arm of the if).
    // 2024-11-01 UTC = 1_730_419_200_000 ms.
    assert_eq!(canonical_date_yyyy_mm_dd(1_730_419_200_000), "2024-11-01");
}

// =====================================================================
// `InMemoryR2AuditSink::snapshot` + `::is_empty` — kills `-> vec![]`
// and `-> true` constant-return mutations.
// =====================================================================

#[test]
fn in_memory_r2_audit_sink_snapshot_and_is_empty_reflect_real_state() {
    let audit = Arc::new(InMemoryAuditChainAuditSink::new());
    let sink = InMemoryR2AuditSink::new(Arc::clone(&audit));
    // Fresh sink: snapshot is empty + is_empty true + len 0.
    assert!(sink.snapshot().is_empty());
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);

    // Emit one genesis event; the R2 buffer must observe it.
    let tenant = Uuid::from_u128(0x1234);
    let id = Uuid::from_u128(0x5678);
    let ev = AuditEvent::genesis(
        AuditEventKind::Tenant,
        "src",
        id,
        1_000,
        tenant,
        Region::Iad,
        serde_json::json!({"k": "v"}),
    );
    sink.emit(ev, "req-1", 2_000).expect("emit");

    // Kills `snapshot -> vec![]`: snapshot now contains exactly 1 line.
    let snap = sink.snapshot();
    assert_eq!(snap.len(), 1, "snapshot must reflect the one emitted event");
    assert_eq!(snap[0].tenant_id, tenant);
    assert_eq!(snap[0].sequence_number, 0);
    assert!(snap[0].ndjson.contains("\"iad\""));

    // Kills `is_empty -> true`: post-emit, sink is non-empty + len 1.
    assert!(!sink.is_empty(), "sink must be non-empty after emit");
    assert_eq!(sink.len(), 1);

    // Second emit advances the chain; snapshot grows; still non-empty.
    let head = sink.chain_head(tenant).expect("chain head");
    let next_seq = sink.next_sequence(tenant);
    let ev2 = AuditEvent::new(
        AuditEventKind::CasPut,
        "src",
        Uuid::from_u128(0x9999),
        3_000,
        tenant,
        Region::Gru,
        next_seq,
        head,
        serde_json::json!({"k": "v2"}),
    );
    sink.emit(ev2, "req-2", 4_000).expect("emit2");
    assert_eq!(sink.snapshot().len(), 2);
    assert_eq!(sink.len(), 2);
    assert!(!sink.is_empty());
}
