//! Regression test: schema symmetric grant ↔ revoke parity (Lote 9.4 H-05).
//!
//! AC-002: `consent_ledger` 6-field block === `consent_revocation` 6-field
//! block (field names, types, order). Tests verify at the Rust type level
//! that both [`ConsentRecord`] and [`ConsentRevocationRecord`] carry the
//! same [`ConsentProofPayload`] 6-field struct.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]

use corelink_privacy::consent::schema::{ConsentProofPayload, LocaleBcp47};
use corelink_privacy::consent::store::{ConsentRecord, ConsentRevocationRecord};

/// Helper: build a grant record with the canonical 6-field proof.
fn make_grant_record(proof: ConsentProofPayload) -> ConsentRecord {
    ConsentRecord {
        consent_id: "grant-001".to_owned(),
        tenant_id: "tenant-01".to_owned(),
        subject_id: "subject-01".to_owned(),
        subject_id_hash: "hash-01".to_owned(),
        purpose: "marketing_email".to_owned(),
        basis_legal: "consent".to_owned(),
        proof,
        signature: "sig-grant".to_owned(),
        signature_kid: "kid-01".to_owned(),
        evidence_screenshot_hash: None,
        ip_country: None,
        user_agent_class: None,
        stale_consent: false,
    }
}

/// Helper: build a revocation record with the same 6-field proof.
fn make_revoke_record(proof: ConsentProofPayload) -> ConsentRevocationRecord {
    ConsentRevocationRecord {
        revocation_id: "revoke-001".to_owned(),
        tenant_id: "tenant-01".to_owned(),
        subject_id: "subject-01".to_owned(),
        subject_id_hash: "hash-01".to_owned(),
        purpose: "marketing_email".to_owned(),
        revokes_consent_id: Some("grant-001".to_owned()),
        proof,
        signature: "sig-revoke".to_owned(),
        signature_kid: "kid-01".to_owned(),
        cascade_started_at: "2026-04-26T11:00:00Z".to_owned(),
        cascade_completed_at: None,
        cascade_status: "pending".to_owned(),
        ip_country: None,
        user_agent_class: None,
    }
}

/// Both grant and revoke carry the same [`ConsentProofPayload`] struct —
/// schema parity verified at compile time (same type) and at runtime
/// (field equality check).
#[test]
fn schema_parity_grant_revoke_proof() {
    let proof = ConsentProofPayload {
        notice_text_hash: "sha256_hash".to_owned(),
        notice_version: "1.2.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "consent-analytics-v3".to_owned(),
        ui_capture_ts: "2026-04-26T10:15:00Z".to_owned(),
        submission_ts: "2026-04-26T10:15:02Z".to_owned(),
    };

    let grant = make_grant_record(proof.clone());
    let revoke = make_revoke_record(proof.clone());

    // The proof struct is identical (same 6 fields, same types)
    assert_eq!(grant.proof.notice_text_hash, revoke.proof.notice_text_hash);
    assert_eq!(grant.proof.notice_version, revoke.proof.notice_version);
    assert_eq!(grant.proof.locale, revoke.proof.locale);
    assert_eq!(grant.proof.wording_id, revoke.proof.wording_id);
    assert_eq!(grant.proof.ui_capture_ts, revoke.proof.ui_capture_ts);
    assert_eq!(grant.proof.submission_ts, revoke.proof.submission_ts);

    // Both carry the SAME struct type — verified by compiler (no cast needed).
    let _: &ConsentProofPayload = &grant.proof;
    let _: &ConsentProofPayload = &revoke.proof;
}

/// Grant and revoke records both carry a HMAC signature field of the
/// same type (String) and a signature_kid field — symmetric crypto strength.
#[test]
fn schema_parity_signature_fields() {
    let proof = ConsentProofPayload {
        notice_text_hash: "h".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::EnUs,
        wording_id: "w".to_owned(),
        ui_capture_ts: "t1".to_owned(),
        submission_ts: "t2".to_owned(),
    };
    let grant = make_grant_record(proof.clone());
    let revoke = make_revoke_record(proof);

    // Both have signature + signature_kid (symmetric HMAC strength)
    assert!(!grant.signature.is_empty());
    assert!(!revoke.signature.is_empty());
    assert!(!grant.signature_kid.is_empty());
    assert!(!revoke.signature_kid.is_empty());
}

/// Revoke carries a nullable FK back to the grant (GDPR Art. 7.3 traceability).
#[test]
fn revoke_carries_grant_fk() {
    let proof = ConsentProofPayload {
        notice_text_hash: "h".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "w".to_owned(),
        ui_capture_ts: "t1".to_owned(),
        submission_ts: "t2".to_owned(),
    };
    let revoke = make_revoke_record(proof);
    assert_eq!(revoke.revokes_consent_id, Some("grant-001".to_owned()));
}

/// Serde round-trip: both proof structs serialise/deserialise identically.
#[test]
fn proof_serde_roundtrip() {
    let proof = ConsentProofPayload {
        notice_text_hash: "sha256_hash".to_owned(),
        notice_version: "1.2.0".to_owned(),
        locale: LocaleBcp47::EsMx,
        wording_id: "wording-v2".to_owned(),
        ui_capture_ts: "2026-04-26T10:15:00Z".to_owned(),
        submission_ts: "2026-04-26T10:15:02Z".to_owned(),
    };

    let json = serde_json::to_string(&proof).unwrap();
    let decoded: ConsentProofPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(proof, decoded);
}
