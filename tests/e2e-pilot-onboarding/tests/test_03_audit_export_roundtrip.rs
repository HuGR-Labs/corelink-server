//! Wave-23 pilot onboarding — Test 3: 24h audit export round-trip.
//!
//! Drives signup + a CAS upload, exports the resulting audit chain
//! over the canonical 24h window, then verifies the NDJSON body +
//! manifest against the canonical chain-integrity contract. Pins:
//!
//! - Export produces NDJSON whose line count matches the manifest
//!   `row_count`.
//! - Manifest `body_digest_hex` matches BLAKE3 of the NDJSON body
//!   byte-for-byte.
//! - Chain integrity holds across the entire export
//!   (`prev_hash` linkage).
//! - Tampering with a single byte in the body fails verification.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_pilot_onboarding::{
    canonical_blob_payloads, canonical_pilot_tenant, AuditChainError, AuditExportError,
    PilotHarness, PilotHarnessError, FIXED_NOW_MS, ONE_DAY_MS,
};

#[test]
fn wave23_pilot_audit_export_24h_roundtrip() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-audit-happy");

    h.complete_signup(&tenant).unwrap();
    let payloads = canonical_blob_payloads(&tenant.slug);
    h.batch_upload_blobs(&tenant, &payloads).unwrap();

    // 24h window centred on the canonical now-pin: [now - 12h, now + 12h).
    let half = ONE_DAY_MS / 2;
    let window_start = FIXED_NOW_MS - half;
    let window_end = FIXED_NOW_MS + half;
    let (body, manifest) = h
        .export_audit_window(&tenant, window_start, window_end)
        .unwrap();

    assert_eq!(manifest.tenant_id, tenant.tenant_id);
    assert_eq!(manifest.window_start_ms, window_start);
    assert_eq!(manifest.window_end_ms, window_end);
    // 4 signup + 1 BatchUploadAccepted + 100 BlobPutCommitted = 105;
    // the export EXCLUDES its own AuditExportStarted marker.
    assert_eq!(manifest.row_count, 4 + 1 + 100);

    // NDJSON line count matches manifest row count.
    let line_count = body.split(|b| *b == b'\n').filter(|s| !s.is_empty()).count();
    assert_eq!(line_count, manifest.row_count);

    // Manifest body digest matches recomputed BLAKE3.
    let recomputed = blake3::hash(&body).to_hex().to_string();
    assert_eq!(recomputed, manifest.body_digest_hex);

    // Full chain-integrity round-trip.
    h.verify_audit_export(&body, &manifest).unwrap();
}

#[test]
fn wave23_pilot_audit_export_tampered_body_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-audit-tamper");
    h.complete_signup(&tenant).unwrap();
    let payloads = canonical_blob_payloads(&tenant.slug);
    h.batch_upload_blobs(&tenant, &payloads).unwrap();

    let (mut body, manifest) = h
        .export_audit_window(&tenant, 0, FIXED_NOW_MS + ONE_DAY_MS)
        .unwrap();

    // Flip one byte in the middle of the NDJSON body.
    let mid = body.len() / 2;
    body[mid] ^= 0x01;

    let err = h.verify_audit_export(&body, &manifest).unwrap_err();
    assert!(
        matches!(
            err,
            PilotHarnessError::AuditChain(
                AuditChainError::ManifestDigestMismatch
                    | AuditChainError::ChainBreak { .. }
                    | AuditChainError::ManifestRowCountMismatch
            )
        ),
        "got: {err:?}"
    );
}

#[test]
fn wave23_pilot_audit_export_invalid_window_rejected() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-audit-window");
    h.complete_signup(&tenant).unwrap();
    let err = h
        .export_audit_window(&tenant, FIXED_NOW_MS + ONE_DAY_MS, FIXED_NOW_MS)
        .unwrap_err();
    assert!(
        matches!(
            err,
            PilotHarnessError::AuditExport(AuditExportError::InvalidWindow)
        ),
        "got: {err:?}"
    );
}
