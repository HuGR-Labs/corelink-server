//! Canonical regression vectors for WI-S01-005 — pinned, hardcoded
//! cross-language reference values that catch any silent drift in
//! capability advertisement, error code mapping, audit envelope shape,
//! and proto-canonical constants.
//!
//! A diff in this file is a **deliberate** spec change: it must be paired
//! with an update to the WI changelog + spec contract version bump.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test code: canonical regression vectors"
)]

use corelink_hash::Digest;
use corelink_reapi::audit::{AuditPrincipal, AuditTime};
use corelink_reapi::{
    cache_capabilities, server_capabilities, AuditEnvelopeBuilder, HashErrorMapping,
    MAX_BATCH_TOTAL_SIZE_BYTES, MAX_CAS_BLOB_SIZE_BYTES, REAPI_PUT_COMPLETED,
};
use uuid::Uuid;

/// Canonical numeric values per WI-S01-005 §6.1.3 + spec contract S-01
/// v1.4.0.
#[test]
fn capability_constants_canonical_values() {
    // 4 MiB = 4 194 304 bytes.
    assert_eq!(MAX_BATCH_TOTAL_SIZE_BYTES, 4_194_304);
    // 5 MiB = 5 242 880 bytes.
    assert_eq!(MAX_CAS_BLOB_SIZE_BYTES, 5_242_880);
}

/// Capability advertisement is byte-stable.
#[test]
fn server_capabilities_canonical_payload() {
    let caps = server_capabilities();
    assert_eq!(caps.api_version.major, 2);
    assert_eq!(caps.api_version.minor, 12);
    assert_eq!(caps.api_version.patch, 0);
    assert_eq!(
        caps.cache_capabilities.max_batch_total_size_bytes,
        MAX_BATCH_TOTAL_SIZE_BYTES
    );
    assert_eq!(
        caps.cache_capabilities.max_cas_blob_size_bytes,
        MAX_CAS_BLOB_SIZE_BYTES
    );
    // BLAKE3 must be FIRST advertised digest function (canonical S-01 hash).
    assert_eq!(
        caps.cache_capabilities.digest_functions[0] as i32, 9,
        "BLAKE3 wire tag is `9`"
    );
}

/// Hash mismatch always maps to gRPC `ABORTED` (10) + COR_CAS_DIGEST_MISMATCH.
/// Per WI-S01-005 v1.3.0 fix: NOT `INTERNAL` (13).
#[test]
fn hash_mismatch_maps_to_aborted_canonical() {
    let m = corelink_hash::HashMismatch.mapping();
    assert_eq!(m.taxonomy_code, "COR_CAS_DIGEST_MISMATCH");
    assert_eq!(m.grpc_code, 10);
}

/// `cache_capabilities()` is byte-stable across builds.
#[test]
fn cache_capabilities_byte_stable() {
    let a = cache_capabilities();
    let b = cache_capabilities();
    assert_eq!(a, b);
}

/// CloudEvents 1.0 envelope shape is byte-stable for the canonical
/// known-good (request_id, digest, principal, region) tuple. A diff in
/// this golden JSON is a deliberate spec change.
#[test]
fn audit_envelope_canonical_golden() {
    let principal = AuditPrincipal {
        tenant_id: Uuid::from_u128(0x019384A0_BEEF_7000_8000_000000000001),
        principal_id: Uuid::from_u128(0x019384A0_F00D_7000_8000_000000000001),
        region: "wnam",
    };
    let time = AuditTime {
        envelope_id: Uuid::from_u128(0x019384A0_ABCD_7000_8000_000000000001),
    };
    let envelope = AuditEnvelopeBuilder::put_completed(
        principal,
        "req-canonical",
        "blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
        11,
        time,
    )
    .build();
    let json = envelope.to_json().unwrap();
    // Canonical golden — order of fields matches `serde::Serialize` derive.
    let expected = r#"{"specversion":"1.0","id":"019384a0-abcd-7000-8000-000000000001","source":"corelink://tenant/019384a0-beef-7000-8000-000000000001","type":"corelink.cas.put_completed","subject":"blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24","datacontenttype":"application/json","data":{"tenant_id":"019384a0-beef-7000-8000-000000000001","principal_id":"019384a0-f00d-7000-8000-000000000001","digest":"blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24","size_bytes":11,"region":"wnam","request_id":"req-canonical"}}"#;
    assert_eq!(json, expected);
    assert_eq!(envelope.r#type, REAPI_PUT_COMPLETED);
    // Body bytes are NEVER in the audit envelope (INV-NO-BODY-IN-LOGS).
    assert!(!json.contains("hello world"));
}

/// REAPI Digest hash field MUST be the lowercase 64-char BLAKE3 hex.
///
/// Confirms the cross-WI digest format contract: `corelink-hash` +
/// `corelink-meta` + `corelink-reapi` all agree on `blake3:<hex>` canonical
/// text + 64-char raw hex on the wire.
#[test]
fn digest_canonical_text_is_blake3_64_hex() {
    let d = Digest::compute(b"hello world");
    let hex = d.to_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    assert_eq!(
        hex,
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    );
}
