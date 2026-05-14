//! Regression test: strict locale enforcement (CTRL-PRIV-CONSENT-005; AC-007).
//!
//! Accept-Language header MUST match payload.locale.
//! Mismatch → 422 LocaleMismatch error; NO consent row inserted; NO
//! audit event emitted.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]

use corelink_privacy_consent_ledger::locale_enforce::{enforce_locale, LocaleEnforceError};
use corelink_privacy_consent_ledger::schema::LocaleBcp47;

// ── Unit tests for enforce_locale ────────────────────────────────────────────

#[test]
fn pt_br_matches_pt_br() {
    assert!(enforce_locale("pt-BR", LocaleBcp47::PtBr).is_ok());
}

#[test]
fn en_us_matches_en_us() {
    assert!(enforce_locale("en-US", LocaleBcp47::EnUs).is_ok());
}

#[test]
fn es_mx_matches_es_mx() {
    assert!(enforce_locale("es-MX", LocaleBcp47::EsMx).is_ok());
}

#[test]
fn pt_br_header_en_us_payload_rejects() {
    let err = enforce_locale("pt-BR", LocaleBcp47::EnUs).unwrap_err();
    assert_eq!(err.payload_locale, LocaleBcp47::EnUs);
    assert!(err.header_locale.contains("pt-BR"));
}

#[test]
fn en_us_header_pt_br_payload_rejects() {
    let err = enforce_locale("en-US", LocaleBcp47::PtBr).unwrap_err();
    assert_eq!(err.payload_locale, LocaleBcp47::PtBr);
}

#[test]
fn multi_value_accept_language_uses_primary() {
    // "pt-BR,en;q=0.9" — primary is pt-BR
    assert!(enforce_locale("pt-BR,en;q=0.9", LocaleBcp47::PtBr).is_ok());
    // mismatch when primary doesn't match
    assert!(enforce_locale("pt-BR,en;q=0.9", LocaleBcp47::EnUs).is_err());
}

#[test]
fn unsupported_locale_in_header_rejects() {
    // fr-FR is not a canonical CoreLink locale
    let result = enforce_locale("fr-FR", LocaleBcp47::PtBr);
    assert!(result.is_err());
}

#[test]
fn case_insensitive_matching() {
    assert!(enforce_locale("PT-BR", LocaleBcp47::PtBr).is_ok());
    assert!(enforce_locale("EN-US", LocaleBcp47::EnUs).is_ok());
    assert!(enforce_locale("es-mx", LocaleBcp47::EsMx).is_ok());
}

#[test]
fn error_display_contains_both_locales() {
    let err = LocaleEnforceError {
        payload_locale: LocaleBcp47::PtBr,
        header_locale: "en-US".to_owned(),
    };
    let s = err.to_string();
    assert!(s.contains("pt-BR"));
    assert!(s.contains("en-US"));
}

// ── Integration: locale mismatch blocks grant (no audit, no row) ─────────────

#[test]
fn grant_locale_mismatch_no_audit_no_row() {
    use std::sync::Arc;
    use corelink_privacy_consent_ledger::{
        audit_emit::InMemoryConsentAuditSink,
        cascade::InMemoryCascadeSink,
        hmac_sign::InMemoryConsentHmacSigner,
        ledger::{ConsentLedger, InMemoryConsentLedger},
        schema::{ConsentProofPayload, ConsentPurpose},
        store::{ConsentStore, InMemoryConsentStore},
        ConsentLedgerError,
    };

    let store = Arc::new(InMemoryConsentStore::new());
    let audit = Arc::new(InMemoryConsentAuditSink::new());
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    let cascade = Arc::new(InMemoryCascadeSink::new());
    let ledger = InMemoryConsentLedger::new(
        store.clone(),
        audit.clone(),
        signer,
        cascade,
        "1.0.0",
    );

    let proof = ConsentProofPayload {
        notice_text_hash: "hash".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,          // payload says pt-BR
        wording_id: "w".to_owned(),
        ui_capture_ts: "ts".to_owned(),
        submission_ts: "ts2".to_owned(),
    };

    let result = ledger.grant_consent(
        "t", "s",
        "en-US",                            // header says en-US
        ConsentPurpose::MarketingEmail,
        proof,
    );

    // Must reject
    assert!(result.is_err());
    let valid = matches!(result.unwrap_err(), ConsentLedgerError::LocaleMismatch(_));
    assert!(valid, "error must be LocaleMismatch");

    // No audit events (rejection is NOT a grant event)
    assert_eq!(audit.captured().len(), 0);

    // No store rows
    assert_eq!(store.list_grants("t", "s").unwrap().len(), 0);
}
