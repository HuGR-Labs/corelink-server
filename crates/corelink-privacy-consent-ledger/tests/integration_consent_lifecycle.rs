//! Integration test: full consent lifecycle (AC-001 through AC-010).
//!
//! Covers signup → grant → list → verify → revoke → cascade.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures"
)]

use std::sync::Arc;

use corelink_privacy_consent_ledger::{
    audit_emit::InMemoryConsentAuditSink,
    cascade::InMemoryCascadeSink,
    hmac_sign::InMemoryConsentHmacSigner,
    ledger::{ConsentLedger, InMemoryConsentLedger, VerifyParams},
    schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
    store::InMemoryConsentStore,
};

fn make_ledger() -> (
    InMemoryConsentLedger,
    Arc<InMemoryConsentAuditSink>,
    Arc<InMemoryCascadeSink>,
) {
    let store = Arc::new(InMemoryConsentStore::new());
    let audit = Arc::new(InMemoryConsentAuditSink::new());
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    let cascade = Arc::new(InMemoryCascadeSink::new());
    let ledger = InMemoryConsentLedger::new(
        store,
        audit.clone(),
        signer,
        cascade.clone(),
        "1.0.0",
    );
    (ledger, audit, cascade)
}

fn grant_proof() -> ConsentProofPayload {
    ConsentProofPayload {
        notice_text_hash: "sha256_of_pt-br_v1.0.0_text".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "consent-analytics-v3".to_owned(),
        ui_capture_ts: "2026-04-26T10:15:00Z".to_owned(),
        submission_ts: "2026-04-26T10:15:02Z".to_owned(),
    }
}

fn revoke_proof() -> ConsentProofPayload {
    ConsentProofPayload {
        notice_text_hash: "sha256_of_unsubscribe_pt-br_v1.0.0_text".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "unsubscribe-marketing-v3".to_owned(),
        ui_capture_ts: "2026-04-26T11:00:00Z".to_owned(),
        submission_ts: "2026-04-26T11:00:02Z".to_owned(),
    }
}

// AC-001: Consent grant happy path with 6-field proof
#[test]
fn ac001_grant_happy_path() {
    let (ledger, audit, _) = make_ledger();
    let receipt = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::AnalyticsPersonalized,
            grant_proof(),
        )
        .unwrap();

    assert!(!receipt.consent_id.is_empty(), "consent_id must be populated");
    assert_eq!(receipt.signature.len(), 64, "HMAC-SHA256 hex-64");
    assert!(receipt.verify_url.contains("/v1/consent/verify"), "verify_url present");
    assert!(!receipt.replay, "first grant is not a replay");

    let events = audit.captured();
    assert_eq!(events.len(), 1, "exactly 1 audit event");
    assert_eq!(
        events[0].event_type.as_cloudevent_type(),
        "dev.hugr.corelink.consent.granted.v1"
    );
    // subject_id_hash must NOT equal the raw subject_id (CTRL-PRIV-014)
    assert_ne!(events[0].subject_id_hash, "subject-01");
}

// AC-002: Schema symmetric grant ↔ revoke (Lote 9.4 H-05)
#[test]
fn ac002_symmetric_schema_grant_revoke() {
    let (ledger, audit, cascade) = make_ledger();
    // Grant first
    let grant_receipt = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::MarketingEmail,
            grant_proof(),
        )
        .unwrap();

    // Revoke with symmetric 6-field proof
    let revoke_receipt = ledger
        .revoke_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::MarketingEmail,
            revoke_proof(),
        )
        .unwrap();

    assert!(!revoke_receipt.revocation_id.is_empty());
    assert_eq!(revoke_receipt.signature.len(), 64, "revoke HMAC-SHA256 hex-64");
    assert!(!revoke_receipt.cascade_eta.is_empty());
    assert!(!grant_receipt.consent_id.is_empty());

    let events = audit.captured();
    assert_eq!(events.len(), 2, "grant + revoke audit events");
    assert_eq!(
        events[1].event_type.as_cloudevent_type(),
        "dev.hugr.corelink.consent.revoked.v1"
    );

    // Cascade enqueued
    let tasks = cascade.captured();
    assert_eq!(tasks.len(), 1, "cascade task enqueued");
    assert_eq!(tasks[0].purpose, "marketing_email");
}

// AC-004: Verify endpoint stateless (no DB lookup)
#[test]
fn ac004_verify_stateless() {
    let (ledger, _, _) = make_ledger();
    let receipt = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::AnalyticsPersonalized,
            grant_proof(),
        )
        .unwrap();

    let vp = VerifyParams {
        record_id: &receipt.consent_id,
        tenant_id: "tenant-01",
        purpose: "analytics_personalized",
        notice_text_hash: "sha256_of_pt-br_v1.0.0_text",
        notice_version: "1.0.0",
        locale: "pt-BR",
        wording_id: "consent-analytics-v3",
        ui_capture_ts: "2026-04-26T10:15:00Z",
        submission_ts: "2026-04-26T10:15:02Z",
        signature_hex: &receipt.signature,
        kid: "kid",
    };
    let resp = ledger.verify_grant_signature(&vp).unwrap();

    assert!(resp.valid, "verify must return valid=true");
    assert!(resp.error.is_none());
    assert_eq!(resp.purpose, "analytics_personalized");
    assert_eq!(resp.locale, "pt-BR");
}

// AC-004 negative: tampered signature → invalid
#[test]
fn ac004_tampered_signature_invalid() {
    let (ledger, _, _) = make_ledger();
    let receipt = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::AnalyticsPersonalized,
            grant_proof(),
        )
        .unwrap();

    let tampered_sig = "a".repeat(64);
    let vp = VerifyParams {
        record_id: &receipt.consent_id,
        tenant_id: "tenant-01",
        purpose: "analytics_personalized",
        notice_text_hash: "sha256_of_pt-br_v1.0.0_text",
        notice_version: "1.0.0",
        locale: "pt-BR",
        wording_id: "consent-analytics-v3",
        ui_capture_ts: "2026-04-26T10:15:00Z",
        submission_ts: "2026-04-26T10:15:02Z",
        signature_hex: &tampered_sig,
        kid: "kid",
    };
    let resp = ledger.verify_grant_signature(&vp).unwrap();

    assert!(!resp.valid);
    assert_eq!(resp.error.as_deref(), Some("invalid_signature"));
}

// AC-005: HMAC tenant-scoping isolation
#[test]
fn ac005_tenant_scoping_isolation() {
    let (ledger, _, _) = make_ledger();
    let receipt = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "pt-BR",
            ConsentPurpose::AnalyticsPersonalized,
            grant_proof(),
        )
        .unwrap();

    // Try to verify T1 signature against T2
    let vp = VerifyParams {
        record_id: &receipt.consent_id,
        tenant_id: "tenant-02", // different tenant
        purpose: "analytics_personalized",
        notice_text_hash: "sha256_of_pt-br_v1.0.0_text",
        notice_version: "1.0.0",
        locale: "pt-BR",
        wording_id: "consent-analytics-v3",
        ui_capture_ts: "2026-04-26T10:15:00Z",
        submission_ts: "2026-04-26T10:15:02Z",
        signature_hex: &receipt.signature,
        kid: "kid",
    };
    let resp = ledger.verify_grant_signature(&vp).unwrap();

    assert!(!resp.valid, "cross-tenant verify must fail");
}

// AC-007: Locale enforcement strict
#[test]
fn ac007_locale_mismatch_rejects() {
    let (ledger, audit, _) = make_ledger();
    let err = ledger
        .grant_consent(
            "tenant-01",
            "subject-01",
            "en-US",                           // header
            ConsentPurpose::MarketingEmail,
            grant_proof(),                     // payload says pt-BR
        )
        .unwrap_err();

    let valid = matches!(
        err,
        corelink_privacy_consent_ledger::ConsentLedgerError::LocaleMismatch(_)
    );
    assert!(valid, "locale mismatch must produce LocaleMismatch error");

    // No audit event emitted (rejection is not a grant)
    assert_eq!(audit.captured().len(), 0, "no audit on locale mismatch");
}

// AC-008: Idempotency replay
#[test]
fn ac008_idempotency_replay() {
    let (ledger, _, _) = make_ledger();
    let p = grant_proof();
    let r1 = ledger
        .grant_consent(
            "tenant-01", "subject-01", "pt-BR",
            ConsentPurpose::BetaFeatures, p.clone(),
        )
        .unwrap();
    let r2 = ledger
        .grant_consent(
            "tenant-01", "subject-01", "pt-BR",
            ConsentPurpose::BetaFeatures, p,
        )
        .unwrap();
    assert!(!r1.replay);
    assert!(r2.replay, "second identical submit must be replay");
    assert_eq!(r1.consent_id, r2.consent_id);
    assert_eq!(r1.signature, r2.signature);
}

// AC-010: List consents history
#[test]
fn ac010_list_consents() {
    let (ledger, _, _) = make_ledger();
    // Grant 2 purposes
    ledger
        .grant_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, grant_proof())
        .unwrap();
    ledger
        .grant_consent("t", "s", "pt-BR", ConsentPurpose::BetaFeatures, {
            let mut p = grant_proof();
            p.ui_capture_ts = "2026-04-26T10:20:00Z".to_owned();
            p
        })
        .unwrap();
    // Revoke 1
    ledger
        .revoke_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, revoke_proof())
        .unwrap();

    let list = ledger.list_consents("t", "s").unwrap();
    assert_eq!(list.consents.len(), 2, "2 grant records");
    assert_eq!(list.revocations.len(), 1, "1 revocation record");
    // Each entry has a verify_url
    for entry in &list.consents {
        assert!(entry.verify_url.contains("verify"));
    }
}

// Canonical CloudEvents type strings pinned
#[test]
fn canonical_audit_event_strings_pinned() {
    use corelink_privacy_consent_ledger::canonical_consent_audit_event_strings;
    let s = canonical_consent_audit_event_strings();
    assert_eq!(s.len(), 2);
    assert!(s.contains(&"dev.hugr.corelink.consent.granted.v1"));
    assert!(s.contains(&"dev.hugr.corelink.consent.revoked.v1"));
}

// Canonical purpose count = 12
#[test]
fn canonical_purpose_count_is_12() {
    use corelink_privacy_consent_ledger::canonical_consent_purposes;
    assert_eq!(canonical_consent_purposes().len(), 12);
}

// Schema version constant
#[test]
fn schema_version_constant() {
    use corelink_privacy_consent_ledger::consent_schema_version;
    assert_eq!(consent_schema_version(), 4);
}

// Purpose legal_basis mapping canonical
#[test]
fn purpose_legal_basis_mapping() {
    use corelink_privacy_consent_ledger::schema::{ConsentPurpose, LegalBasis};
    assert_eq!(ConsentPurpose::ServiceDelivery.legal_basis(), LegalBasis::Contract);
    assert_eq!(ConsentPurpose::AccountManagement.legal_basis(), LegalBasis::Contract);
    assert_eq!(ConsentPurpose::RegulatoryCompliance.legal_basis(), LegalBasis::LegalObligation);
    assert_eq!(ConsentPurpose::SecurityMonitoring.legal_basis(), LegalBasis::LegitimateInterest);
    assert_eq!(ConsentPurpose::AnalyticsAggregated.legal_basis(), LegalBasis::LegitimateInterest);
    assert_eq!(ConsentPurpose::AnalyticsPersonalized.legal_basis(), LegalBasis::Consent);
    assert_eq!(ConsentPurpose::MarketingEmail.legal_basis(), LegalBasis::Consent);
    assert_eq!(ConsentPurpose::TrainingMlModels.legal_basis(), LegalBasis::Consent);
}

// Revocable purposes only for Consent basis
#[test]
fn revocable_only_consent_basis() {
    use corelink_privacy_consent_ledger::{canonical_consent_purposes, schema::LegalBasis};
    for purpose in canonical_consent_purposes() {
        let revocable = purpose.is_revocable();
        let is_consent_basis = matches!(purpose.legal_basis(), LegalBasis::Consent);
        assert_eq!(
            revocable, is_consent_basis,
            "is_revocable must match Consent basis for {purpose:?}"
        );
    }
}
