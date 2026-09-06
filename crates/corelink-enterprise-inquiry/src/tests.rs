use super::*;
use crate::audit::{FailingInquiryAuditSink, InMemoryInquiryAuditSink};
use crate::crm::{FailingCrmClient, InMemoryCrmClient};
use crate::encryption::{FailingInquiryPayloadEncryptor, InMemoryInquiryPayloadEncryptor};
use crate::form::{BYOKRequirementsKind, ResidencyKind, Role};
use crate::mailer::InMemoryAutoReplyMailer;
use crate::slack::{FailingSlackClient, InMemorySlackClient};

fn form() -> EnterpriseInquiryForm {
    EnterpriseInquiryForm {
        company: "Acme Corp".to_string(),
        role: Role::Ciso,
        email: "ciso@acme.example".to_string(),
        phone_optional: Some("+15555550100".to_string()),
        expected_gb_per_month: 5_000,
        byok_requirements: BYOKRequirementsKind::AwsKms,
        residency_requirements: ResidencyKind::Eu,
        additional_notes: Some("multi-region read-through".to_string()),
        locale: "en-US".to_string(),
        use_case: "cache".to_string(),
    }
}

fn enc_config() -> LedgerEncryptionConfig {
    LedgerEncryptionConfig::for_tenant("tenant-1", b"search-key-v1".to_vec())
}

fn happy_ledger() -> EnterpriseInquiryLedger<
    InMemoryInquiryAuditSink,
    InMemorySlackClient,
    InMemoryCrmClient,
    InMemoryAutoReplyMailer,
    InMemoryInquiryPayloadEncryptor,
> {
    EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    )
}

#[test]
fn happy_path_commits_outbox_and_sends_reply() {
    let l = happy_ledger();
    let receipt = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    assert_eq!(receipt.status, InquiryStatus::Committed);
    let outbox = l.get_outbox(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(outbox.status, OutboxStatus::Committed);
    assert!(outbox.slack_message_id.is_some());
    assert!(outbox.crm_entry_id.is_some());
}

#[test]
fn r2_11_plaintext_pii_never_reaches_ledger_state() {
    let l = happy_ledger();
    let _ = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    // Inspect the in-memory mirror — no PII may appear in any
    // Debug-rendered field.
    let rec = l.get_inquiry(&InquiryId::new("inq-1")).unwrap();
    let dbg = format!("{rec:?}");
    assert!(!dbg.contains("ciso@acme.example"), "email leaked: {dbg}");
    assert!(!dbg.contains("+15555550100"), "phone leaked: {dbg}");
    assert!(!dbg.contains("multi-region read-through"), "notes leaked");
    // Acme Corp should NOT appear — only the surrogate AC.
    assert!(!dbg.contains("Acme Corp"), "company leaked: {dbg}");
    // Sanitised surrogates ARE expected to appear.
    assert!(dbg.contains("AC")); // company initials
    assert!(dbg.contains("acme.example")); // email domain
}

#[test]
fn r2_11_slack_payload_contains_no_pii() {
    let slack = InMemorySlackClient::new();
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        slack.clone(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let _ = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    let posts = slack.snapshot();
    assert_eq!(posts.len(), 1);
    let body = &posts[0].body;
    assert!(
        !body.contains("ciso@acme.example"),
        "email in slack: {body}"
    );
    assert!(!body.contains("+15555550100"), "phone in slack: {body}");
    assert!(!body.contains("Acme Corp"), "company in slack: {body}");
    assert!(
        !body.contains("multi-region read-through"),
        "notes in slack"
    );
    // Surrogates present.
    assert!(body.contains("AC"));
    assert!(body.contains("acme.example"));
    assert!(body.contains("ciso"));
}

#[test]
fn r2_11_hubspot_boundary_decrypts_pii() {
    let crm = InMemoryCrmClient::new();
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        InMemorySlackClient::new(),
        crm.clone(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let _ = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    // The CRM fake unsealed the payload at its boundary; the
    // recovered email is what the HubSpot adapter would push.
    assert_eq!(
        crm.last_unsealed_email().as_deref(),
        Some("ciso@acme.example")
    );
}

#[test]
fn r2_11_system_cmk_fallback_for_prospect() {
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        LedgerEncryptionConfig::system_prospect(b"k".to_vec()),
    );
    let _ = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    let rec = l.get_inquiry(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(rec.tenant_id, SYSTEM_CMK_TENANT_TAG);
    assert_eq!(rec.sealed.payload.aad.tenant_id, SYSTEM_CMK_TENANT_TAG);
}

#[test]
fn r2_11_byok_tenant_happy_path_aad_binding() {
    let l = happy_ledger();
    let _ = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    let rec = l.get_inquiry(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(rec.tenant_id, "tenant-1");
    assert_eq!(rec.sealed.payload.aad.tenant_id, "tenant-1");
    // Payload hash + company hash are non-empty hex blobs.
    assert_eq!(rec.sealed.payload.aad.payload_hash_blake3_hex.len(), 64);
    assert_eq!(rec.sealed.payload.aad.company_hash_hex.len(), 64);
}

#[test]
fn r2_11_encryption_failure_rolls_back_atomically() {
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        FailingInquiryPayloadEncryptor,
        enc_config(),
    );
    let err = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap_err();
    assert!(matches!(err, EnterpriseInquiryError::Encryption(_)));
    // No D1 row, no outbox entry.
    assert!(l.get_inquiry(&InquiryId::new("inq-1")).is_none());
    assert!(l.get_outbox(&InquiryId::new("inq-1")).is_none());
}

#[test]
fn slack_failure_rolls_back_no_crm() {
    let crm = InMemoryCrmClient::new();
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        FailingSlackClient,
        crm.clone(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let err = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap_err();
    assert!(matches!(err, EnterpriseInquiryError::Slack(_)));
    // No CRM entry was created.
    assert_eq!(crm.len(), 0);
    let outbox = l.get_outbox(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(outbox.status, OutboxStatus::RolledBack);
}

#[test]
fn crm_failure_rolls_back_with_compensating_slack_mark() {
    let slack = InMemorySlackClient::new();
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        slack.clone(),
        FailingCrmClient,
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let err = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap_err();
    assert!(matches!(err, EnterpriseInquiryError::Crm(_)));
    // Slack received TWO posts: new_inquiry + rollback.
    let posts = slack.snapshot();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].kind, SlackPostKind::NewInquiry);
    assert_eq!(posts[1].kind, SlackPostKind::Rollback);
    let outbox = l.get_outbox(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(outbox.status, OutboxStatus::RolledBack);
}

#[test]
fn audit_failure_fails_closed() {
    let l = EnterpriseInquiryLedger::new(
        FailingInquiryAuditSink,
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let err = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap_err();
    assert!(matches!(err, EnterpriseInquiryError::Audit(_)));
    // Nothing was persisted: audit fired BEFORE the mutation.
    assert!(l.get_inquiry(&InquiryId::new("inq-1")).is_none());
}

#[test]
fn idempotency_key_dedupes_resubmission() {
    let l = happy_ledger();
    let r1 = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid-1",
        )
        .unwrap();
    // Resubmit with the SAME idempotency key but a different
    // inquiry id — the ledger MUST return the original receipt.
    let r2 = l
        .submit_inquiry(
            form(),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-2"),
            2_000,
            "cid-2",
        )
        .unwrap();
    assert_eq!(r1.inquiry_id, r2.inquiry_id);
    assert_eq!(r1.created_ms, r2.created_ms);
}

#[test]
fn sla_breach_detected_after_24h_window() {
    // Use a mailer that rejects so replied_ms stays None — that
    // is the only way the SLA detector can fire.
    let l = EnterpriseInquiryLedger::new(
        InMemoryInquiryAuditSink::new(),
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        crate::mailer::FailingAutoReplyMailer,
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    let _ = l.submit_inquiry(
        form(),
        IdempotencyKey::new("k-1"),
        InquiryId::new("inq-1"),
        1_000,
        "cid-1",
    );
    // No breach immediately after submission.
    assert!(l.sla_breaches(1_000, "cid-1").unwrap().is_empty());
    // Breach surfaces once the 24h window has elapsed.
    let later = 1_000 + crate::SLA_RESPONSE_MS;
    let breaches = l.sla_breaches(later, "cid-1").unwrap();
    assert_eq!(breaches.len(), 1);
    assert_eq!(breaches[0].inquiry_id, InquiryId::new("inq-1"));
    assert_eq!(breaches[0].elapsed_ms, crate::SLA_RESPONSE_MS);
}

#[test]
fn drain_partial_escalates_pending_rows() {
    let l = happy_ledger();
    let _ = l.submit_inquiry(
        form(),
        IdempotencyKey::new("k-1"),
        InquiryId::new("inq-1"),
        1_000,
        "cid-1",
    );
    // Force-pending injection via direct state mutation (test-
    // only): commit re-set to pending.
    {
        let mut g = l.state.lock().unwrap();
        if let Some(row) = g.outbox.get_mut(&InquiryId::new("inq-1")) {
            row.status = OutboxStatus::Pending;
            row.created_ms = 1_000;
        }
    }
    // Now drain at now=1_000 + 6min — should escalate (5min
    // partial-state ceiling).
    let now = 1_000 + 6 * 60 * 1000;
    let escalated = l
        .drain_partial_escalations(now, 5 * 60 * 1000, "cid-1")
        .unwrap();
    assert_eq!(escalated, vec![InquiryId::new("inq-1")]);
    let outbox = l.get_outbox(&InquiryId::new("inq-1")).unwrap();
    assert_eq!(outbox.status, OutboxStatus::PartialEscalated);
}

#[test]
fn spam_blocked_records_audit_only() {
    let audit = InMemoryInquiryAuditSink::new();
    let l = EnterpriseInquiryLedger::new(
        audit.clone(),
        InMemorySlackClient::new(),
        InMemoryCrmClient::new(),
        InMemoryAutoReplyMailer::new(),
        InMemoryInquiryPayloadEncryptor::new(),
        enc_config(),
    );
    l.record_spam_blocked(InquiryId::new("inq-1"), 1_000, "cid-1")
        .unwrap();
    assert_eq!(audit.snapshot().len(), 1);
    assert_eq!(
        audit.snapshot()[0].event_type,
        InquiryAuditEventType::SpamBlocked
    );
    // No inquiry persisted.
    assert!(l.get_inquiry(&InquiryId::new("inq-1")).is_none());
}
