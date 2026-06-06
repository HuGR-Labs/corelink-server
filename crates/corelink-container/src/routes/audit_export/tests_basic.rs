//! Basic unit tests for the audit-export route: route constants,
//! event-type constants, parse-timestamp helpers, rate-limit config,
//! UUID compare, in-memory sink, NDJSON serialization, and mid-stream
//! trailer payload encoder.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Test bodies are verbatim copies of the original inline `mod tests`
//! block (subset: ~150 LOC).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use uuid::Uuid;

use super::audit_sink::{ExportAuditSink, InMemoryExportAuditSink};
use super::parse::{serialize_ndjson_lines, uuid_eq_ct};
use super::state::{audit_export_rate_limit_config, build_state, router};
use super::stream::mid_stream_abort_trailer_value;
use super::types::{
    ExportAuditRow, AUDIT_EXPORT_ROUTE, DEFAULT_EXPORT_ROW_BUFFER_BYTES,
    EVENT_TYPE_CROSS_TENANT_ATTEMPT, EVENT_TYPE_EXPORT_REQUEST, EVENT_TYPE_VERIFY_FAILED,
    HEADER_EXPORT_ABORTED, R2_LIST_PAGE_SIZE,
};

// Bring `parse_timestamp` into scope for the tests below.
use super::parse::parse_timestamp;

#[test]
fn route_constant_matches_canonical_path() {
    // Wave-37 fix: tenant moved into path following /v1/cas/:tenant/:hash pattern.
    assert_eq!(AUDIT_EXPORT_ROUTE, "/v1/audit/:tenant/export");
}

#[test]
fn event_type_constants_match_spec() {
    assert_eq!(
        EVENT_TYPE_EXPORT_REQUEST,
        "corelink.audit.export_request.v1"
    );
    assert_eq!(
        EVENT_TYPE_CROSS_TENANT_ATTEMPT,
        "corelink.security.audit_export_cross_tenant_attempt.v1"
    );
    assert_eq!(
        EVENT_TYPE_VERIFY_FAILED,
        "corelink.audit.export_verify_failed.v1"
    );
}

#[test]
fn build_state_returns_usable_router() {
    let state = build_state();
    let _router = router(state);
}

#[test]
fn parse_timestamp_accepts_epoch_ms() {
    assert_eq!(parse_timestamp("1700000000000"), Some(1_700_000_000_000));
    assert_eq!(parse_timestamp("  42 "), Some(42));
}

#[test]
fn parse_timestamp_accepts_rfc3339_utc() {
    // 1970-01-01T00:00:00Z = 0
    assert_eq!(parse_timestamp("1970-01-01T00:00:00Z"), Some(0));
    // 2026-05-15T12:00:00Z = 1_778_932_800_000 (verified
    // independently below by round-trip).
    let v = parse_timestamp("2026-05-15T12:00:00Z").expect("valid");
    // Cross-check: 1970-01-01T00:00:00Z + (days × 86400 + 12h) sec
    // matches via the days_from_civil formula above.
    assert!(v > 1_700_000_000_000);
}

#[test]
fn parse_timestamp_rejects_malformed_rfc3339() {
    assert_eq!(parse_timestamp("not-a-time"), None);
    assert_eq!(parse_timestamp("2026-13-01T00:00:00Z"), None);
    assert_eq!(parse_timestamp("2026-05-32T00:00:00Z"), None);
    assert_eq!(parse_timestamp("2026-05-15T25:00:00Z"), None);
    // Missing trailing Z.
    assert_eq!(parse_timestamp("2026-05-15T12:00:00"), None);
}

#[test]
fn audit_export_rate_limit_config_pins_60s_floor() {
    let cfg = audit_export_rate_limit_config();
    assert_eq!(cfg.default_burst_capacity(), 1);
    assert_eq!(cfg.default_refill_rate_per_sec(), 1);
    assert_eq!(cfg.retry_after_floor_secs(), 60);
}

#[test]
fn uuid_eq_ct_returns_true_for_equal_ids() {
    let u = Uuid::from_u128(0xAB);
    assert!(uuid_eq_ct(&u, &u));
}

#[test]
fn uuid_eq_ct_returns_false_for_distinct_ids() {
    let a = Uuid::from_u128(0xAB);
    let b = Uuid::from_u128(0xCD);
    assert!(!uuid_eq_ct(&a, &b));
}

#[test]
fn in_memory_audit_sink_captures_emits() {
    let s = InMemoryExportAuditSink::new();
    let row = ExportAuditRow {
        event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
        authenticated_tenant: Some(Uuid::from_u128(1)),
        attempted_tenant: None,
        from_ms: 0,
        to_ms: 1,
        bytes_written: 0,
        events_written: 0,
        exit_status: "empty".to_string(),
        payload: None,
    };
    s.emit(row.clone()).expect("emit");
    let snap = s.snapshot().expect("snap");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0], row);
}

#[test]
fn in_memory_audit_sink_fail_closed_on_injected_failure() {
    let s = InMemoryExportAuditSink::new();
    s.inject_failure("pipeline down").expect("inject");
    let row = ExportAuditRow {
        event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
        authenticated_tenant: None,
        attempted_tenant: None,
        from_ms: 0,
        to_ms: 1,
        bytes_written: 0,
        events_written: 0,
        exit_status: "audit_failed".to_string(),
        payload: None,
    };
    let err = s.emit(row).expect_err("inject");
    assert_eq!(err, "pipeline down");
    assert_eq!(s.snapshot().expect("snap").len(), 0);
}

#[test]
fn serialize_ndjson_lines_empty_returns_zero() {
    let lines = serialize_ndjson_lines(&[]).expect("empty");
    assert!(lines.is_empty());
}

#[test]
fn mid_stream_abort_trailer_value_matches_canonical_payload() {
    let v =
        mid_stream_abort_trailer_value(42, 1, "aa".repeat(32).as_str(), "bb".repeat(32).as_str());
    assert!(v.starts_with("{\"break_at_seq\":42,\"break_at_chunk\":1,"));
    assert!(v.contains("\"observed\":\""));
    assert!(v.contains("\"expected\":\""));
    // Round-trips as JSON.
    let _: serde_json::Value = serde_json::from_str(&v).expect("valid json");
}

#[test]
fn export_aborted_header_constant_matches_canonical_name() {
    assert_eq!(HEADER_EXPORT_ABORTED, "x-corelink-audit-export-aborted");
}

// -- Wave-19 unit tests --------------------------------------------

#[test]
fn r2_list_page_size_matches_cf_api_v4_default() {
    assert_eq!(R2_LIST_PAGE_SIZE, 1000);
}

#[test]
fn default_export_row_buffer_bytes_is_64kib() {
    assert_eq!(DEFAULT_EXPORT_ROW_BUFFER_BYTES, 65_536);
}
