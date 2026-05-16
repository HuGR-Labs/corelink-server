//! Wave-23 pilot onboarding — Test 2: first CAS upload.
//!
//! Exercises `BatchUpdateBlobs` with 100 canonical 4-KiB blobs. Pins:
//!
//! - 100 blobs land in the tenant CAS under the canonical
//!   `tenants/<tenant_id>/cas/` prefix.
//! - One `BatchUploadAccepted` audit row + 100 `BlobPutCommitted` rows
//!   fire, in canonical order, BEFORE any tenant-visible state flip
//!   (fail-CLOSED ordering).
//! - Tenant-prefix scoping is enforced: another tenant cannot read
//!   blobs uploaded by the pilot tenant.
//! - GET roundtrip returns byte-identical payloads for every digest.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_pilot_onboarding::{
    blob_digest_hex, canonical_blob_payloads, canonical_pilot_tenant, AuditEventKind, CasError,
    PilotHarness, PilotHarnessError, CANONICAL_BLOB_COUNT,
};

#[test]
fn wave23_pilot_batch_upload_100_blobs() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-cas-happy");
    h.complete_signup(&tenant).unwrap();

    let payloads = canonical_blob_payloads(&tenant.slug);
    assert_eq!(payloads.len(), CANONICAL_BLOB_COUNT);

    let receipt = h.batch_upload_blobs(&tenant, &payloads).unwrap();
    assert_eq!(receipt.blobs_uploaded, CANONICAL_BLOB_COUNT);
    assert_eq!(
        receipt.tenant_prefix,
        format!("tenants/{}/cas/", tenant.tenant_id)
    );
    assert_eq!(receipt.digests_hex.len(), CANONICAL_BLOB_COUNT);
    // Every digest matches the BLAKE3 of its canonical payload.
    for (i, p) in payloads.iter().enumerate() {
        assert_eq!(receipt.digests_hex[i], blob_digest_hex(p));
    }

    // CAS holds exactly 100 blobs for the tenant.
    assert_eq!(h.cas_blob_count(&tenant), CANONICAL_BLOB_COUNT);

    // GET roundtrip — every digest returns its original bytes.
    for (i, p) in payloads.iter().enumerate() {
        let got = h.read_blob(&tenant, &receipt.digests_hex[i]).unwrap();
        assert_eq!(&got, p);
    }

    // Audit chain shape: 4 signup rows + 1 BatchUploadAccepted + 100
    // BlobPutCommitted = 105.
    let rows = h.audit_snapshot(&tenant);
    assert_eq!(rows.len(), 4 + 1 + CANONICAL_BLOB_COUNT);
    assert_eq!(rows[4].kind, AuditEventKind::BatchUploadAccepted);
    for i in 0..CANONICAL_BLOB_COUNT {
        assert_eq!(rows[5 + i].kind, AuditEventKind::BlobPutCommitted);
    }
    // Chain integrity across the entire 105-row sequence.
    for w in rows.windows(2) {
        assert_eq!(w[1].prev_hash_hex, w[0].row_hash_hex);
    }
}

#[test]
fn wave23_pilot_upload_requires_active_subscription() {
    let h = PilotHarness::new();
    let tenant = canonical_pilot_tenant("pilot-cas-no-sub");
    // No signup → no subscription.
    let payloads = canonical_blob_payloads(&tenant.slug);
    let err = h.batch_upload_blobs(&tenant, &payloads).unwrap_err();
    assert!(
        matches!(err, PilotHarnessError::Cas(CasError::TenantNotProvisioned(_))),
        "got: {err:?}"
    );
    // No state, no audit.
    assert_eq!(h.audit_row_count(&tenant), 0);
}

#[test]
fn wave23_pilot_cross_tenant_read_is_rejected() {
    let h = PilotHarness::new();
    let pilot = canonical_pilot_tenant("pilot-cas-isolation-a");
    let other = canonical_pilot_tenant("pilot-cas-isolation-b");
    h.complete_signup(&pilot).unwrap();
    h.complete_signup(&other).unwrap();

    let payloads = canonical_blob_payloads(&pilot.slug);
    let receipt = h.batch_upload_blobs(&pilot, &payloads).unwrap();

    // `other` tries to read a digest that lives only under `pilot` —
    // INV-MULTIPART-PATH-TENANT-SCOPED must reject.
    let err = h.read_blob(&other, &receipt.digests_hex[0]).unwrap_err();
    assert!(
        matches!(err, PilotHarnessError::Cas(CasError::TenantPrefixViolation)),
        "got: {err:?}"
    );
}
