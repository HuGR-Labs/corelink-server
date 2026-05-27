//! Chaos test: audit emit failure → fail-CLOSED behavior (AC-009).
//!
//! INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116: if audit emit fails,
//! state MUST remain UNCHANGED. The request MUST return an error.
//!
//! Verifies the S-06 P0-2 / S-07 P1-1 audit ordering fix:
//! `lookup → emit_audit → mutate_state`.
//! Test verifies state UNCHANGED on audit emit failure.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

use std::sync::Arc;

use corelink_privacy::consent::{
    audit_emit::FailingConsentAuditSink,
    cascade::InMemoryCascadeSink,
    hmac_sign::InMemoryConsentHmacSigner,
    ledger::{ConsentLedger, InMemoryConsentLedger},
    schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
    store::{ConsentStore, InMemoryConsentStore},
    ConsentLedgerError,
};

fn make_failing_audit_ledger() -> (InMemoryConsentLedger, Arc<InMemoryConsentStore>) {
    let store = Arc::new(InMemoryConsentStore::new());
    let audit = Arc::new(FailingConsentAuditSink);
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    let cascade = Arc::new(InMemoryCascadeSink::new());
    let ledger = InMemoryConsentLedger::new(
        store.clone(),
        audit,
        signer,
        cascade,
        "1.0.0",
    );
    (ledger, store)
}

fn proof() -> ConsentProofPayload {
    ConsentProofPayload {
        notice_text_hash: "abc123".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "wording-v1".to_owned(),
        ui_capture_ts: "2026-05-13T10:00:00Z".to_owned(),
        submission_ts: "2026-05-13T10:00:02Z".to_owned(),
    }
}

/// AC-009: Grant with failing audit → error returned, store UNCHANGED.
///
/// Specifically verifies the fail-CLOSED ordering:
/// emit_audit BEFORE store_grant. Store must be empty after failure.
#[test]
fn grant_audit_fail_returns_error_state_unchanged() {
    let (ledger, store) = make_failing_audit_ledger();

    let result = ledger.grant_consent(
        "tenant-01",
        "subject-01",
        "pt-BR",
        ConsentPurpose::MarketingEmail,
        proof(),
    );

    // Must fail with audit error
    assert!(result.is_err(), "grant must fail when audit emit fails");
    let valid = matches!(result.unwrap_err(), ConsentLedgerError::Audit(_));
    assert!(valid, "error must be Audit variant");

    // State UNCHANGED: no grants stored
    let grants = store.list_grants("tenant-01", "subject-01").unwrap();
    assert_eq!(grants.len(), 0, "store must be UNCHANGED after audit failure");
}

/// AC-009: Revoke with failing audit → error returned, store UNCHANGED.
#[test]
fn revoke_audit_fail_returns_error_state_unchanged() {
    // Use a working ledger to insert a grant first
    let working_store = Arc::new(InMemoryConsentStore::new());
    let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
    {
        use corelink_privacy::consent::audit_emit::InMemoryConsentAuditSink;
        let working_ledger = InMemoryConsentLedger::new(
            working_store.clone(),
            Arc::new(InMemoryConsentAuditSink::new()),
            signer.clone(),
            Arc::new(InMemoryCascadeSink::new()),
            "1.0.0",
        );
        working_ledger
            .grant_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, proof())
            .unwrap();
    }

    // Now create a failing-audit ledger over the same store
    let failing_ledger = InMemoryConsentLedger::new(
        working_store.clone(),
        Arc::new(FailingConsentAuditSink),
        signer,
        Arc::new(InMemoryCascadeSink::new()),
        "1.0.0",
    );

    let revoke_proof = ConsentProofPayload {
        notice_text_hash: "unsubscribe-hash".to_owned(),
        notice_version: "1.0.0".to_owned(),
        locale: LocaleBcp47::PtBr,
        wording_id: "unsubscribe-v1".to_owned(),
        ui_capture_ts: "2026-05-13T11:00:00Z".to_owned(),
        submission_ts: "2026-05-13T11:00:02Z".to_owned(),
    };

    let result = failing_ledger.revoke_consent(
        "t", "s", "pt-BR", ConsentPurpose::MarketingEmail, revoke_proof,
    );

    assert!(result.is_err(), "revoke must fail when audit emit fails");
    let valid = matches!(result.unwrap_err(), ConsentLedgerError::Audit(_));
    assert!(valid, "error must be Audit variant");

    // No revocation records stored
    let revocations = working_store.list_revocations("t", "s").unwrap();
    assert_eq!(revocations.len(), 0, "revocation store must be UNCHANGED after audit failure");
}

/// Verify the FailingConsentAuditSink always returns Audit error.
#[test]
fn failing_sink_always_errors() {
    use corelink_privacy::consent::audit_emit::{
        ConsentAuditEventType, ConsentAuditRecord, ConsentAuditSink, FailingConsentAuditSink,
    };
    let sink = FailingConsentAuditSink;
    let record = ConsentAuditRecord {
        event_type: ConsentAuditEventType::Granted,
        record_id: "r".to_owned(),
        tenant_id: "t".to_owned(),
        subject_id_hash: "h".to_owned(),
        purpose: "marketing_email".to_owned(),
        submission_ts: "ts".to_owned(),
    };
    let err = sink.emit(record).unwrap_err();
    let valid = matches!(err, ConsentLedgerError::Audit(_));
    assert!(valid);
}
