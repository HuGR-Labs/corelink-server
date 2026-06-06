//! Regression test: notice version major bump force re-consent (AC-006;
//! CTRL-PRIV-CONSENT-005; DD-002).
//!
//! When `current_notice_version` major > submitted proof major, the
//! request is rejected with `NoticeVersionStale`. NO fallback to
//! legitimate_interest (corrige GPT P0-1 round-1).

#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use corelink_privacy::consent::notice_version_check::{
    check_notice_version, is_notice_version_stale,
};

// ── Unit tests ──────────────────────────────────────────────────────────────

#[test]
fn same_major_ok() {
    assert!(check_notice_version("1.5.0", "1.0.0").is_ok());
    assert!(check_notice_version("2.9.0", "2.0.0").is_ok());
}

#[test]
fn older_major_stale() {
    let err = check_notice_version("2.0.0", "1.5.0").unwrap_err();
    assert_eq!(err.current_major, 2);
    assert_eq!(err.submitted_major, 1);
}

#[test]
fn is_stale_fn_returns_true_on_older_major() {
    assert!(is_notice_version_stale("2.0.0", "1.9.9"));
    assert!(!is_notice_version_stale("1.0.0", "1.0.0"));
    assert!(!is_notice_version_stale("1.0.0", "2.0.0")); // submitted ahead — not stale
}

#[test]
fn malformed_version_not_stale() {
    // Fail-open on parse errors
    assert!(!is_notice_version_stale("bad", "also-bad"));
    assert!(!is_notice_version_stale("2.0.0", "bad"));
}

// ── Integration: stale version rejects grant ─────────────────────────────────

#[test]
fn grant_with_stale_notice_version_rejects() {
    use corelink_privacy::consent::{
        audit_emit::InMemoryConsentAuditSink,
        cascade::InMemoryCascadeSink,
        hmac_sign::InMemoryConsentHmacSigner,
        ledger::{ConsentLedger, InMemoryConsentLedger},
        schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
        store::{ConsentStore, InMemoryConsentStore},
        ConsentLedgerError,
    };
    use std::sync::Arc;

    let store = Arc::new(InMemoryConsentStore::new());
    let audit = Arc::new(InMemoryConsentAuditSink::new());
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    let cascade = Arc::new(InMemoryCascadeSink::new());

    // Current version is 2.0.0
    let ledger = InMemoryConsentLedger::new(store.clone(), audit.clone(), signer, cascade, "2.0.0");

    // Submit proof with old version 1.5.0 → stale
    let proof = ConsentProofPayload {
        notice_text_hash: "hash".to_owned(),
        notice_version: "1.5.0".to_owned(), // major 1 < current major 2
        locale: LocaleBcp47::EnUs,
        wording_id: "w".to_owned(),
        ui_capture_ts: "ts".to_owned(),
        submission_ts: "ts2".to_owned(),
    };

    let result = ledger.grant_consent("t", "s", "en-US", ConsentPurpose::MarketingEmail, proof);

    assert!(result.is_err());
    let valid = matches!(
        result.unwrap_err(),
        ConsentLedgerError::NoticeVersionStale {
            current: 2,
            submitted: 1
        }
    );
    assert!(valid, "error must be NoticeVersionStale");

    // No audit event (rejection before audit)
    assert_eq!(audit.captured().len(), 0);
    // No store rows
    assert_eq!(store.list_grants("t", "s").unwrap().len(), 0);
}

#[test]
fn grant_with_current_version_ok() {
    use corelink_privacy::consent::{
        audit_emit::InMemoryConsentAuditSink,
        cascade::InMemoryCascadeSink,
        hmac_sign::InMemoryConsentHmacSigner,
        ledger::{ConsentLedger, InMemoryConsentLedger},
        schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
        store::InMemoryConsentStore,
    };
    use std::sync::Arc;

    let ledger = InMemoryConsentLedger::new(
        Arc::new(InMemoryConsentStore::new()),
        Arc::new(InMemoryConsentAuditSink::new()),
        Arc::new(InMemoryConsentHmacSigner::new_test()),
        Arc::new(InMemoryCascadeSink::new()),
        "2.0.0",
    );

    let proof = ConsentProofPayload {
        notice_text_hash: "hash".to_owned(),
        notice_version: "2.0.0".to_owned(), // same major ✓
        locale: LocaleBcp47::EnUs,
        wording_id: "w".to_owned(),
        ui_capture_ts: "ts".to_owned(),
        submission_ts: "ts2".to_owned(),
    };

    let result = ledger.grant_consent("t", "s", "en-US", ConsentPurpose::BetaFeatures, proof);
    assert!(result.is_ok(), "current notice version must be accepted");
}
