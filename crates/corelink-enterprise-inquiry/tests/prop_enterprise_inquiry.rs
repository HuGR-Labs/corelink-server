//! Property tests for `corelink-enterprise-inquiry` (WI-S19-005 + R2-11).
//!
//! Pinned invariants:
//!   - `prop_saga_atomic_commit_or_rollback` — Slack+CRM dispatch
//!     terminates in `Committed` (both legs ok) XOR `RolledBack`
//!     (either leg rejected); no `Pending` row survives a synchronous
//!     `submit_inquiry` return.
//!   - `prop_idempotency_key_dedup` — N re-submissions sharing an
//!     idempotency key dispatch exactly 1 Slack + 1 CRM + 1 email.
//!   - `prop_sla_breach_detection` — every committed inquiry whose
//!     auto-reply has not landed AND whose age ≥ 24h surfaces as a
//!     breach.
//!   - `prop_audit_emit_before_mutation` — the failing audit sink
//!     short-circuits every mutation path; no ledger state changes.
//!   - **R2-11 / S-19 P1-NEW-3**: `prop_sealed_payload_unreadable_without_correct_aad`
//!     ratifies that every random `InquiryForm` seals to a payload
//!     that fails-CLOSED when unsealed under any tenant_id other than
//!     the one bound at seal time.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use corelink_enterprise_inquiry::{
    seal_inquiry, unseal_inquiry, BYOKRequirementsKind, EnterpriseInquiryError,
    EnterpriseInquiryForm, EnterpriseInquiryLedger, FailingCrmClient, FailingInquiryAuditSink,
    FailingSlackClient, IdempotencyKey, InMemoryAutoReplyMailer, InMemoryCrmClient,
    InMemoryInquiryAuditSink, InMemoryInquiryPayloadEncryptor, InMemorySlackClient, InquiryId,
    InquiryStatus, LedgerEncryptionConfig, OutboxStatus, ResidencyKind, Role, SlackPostKind,
    SLA_RESPONSE_MS,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 32
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(32)
}

fn form(company: &str, gb: u64, byok: BYOKRequirementsKind) -> EnterpriseInquiryForm {
    EnterpriseInquiryForm::new(
        company,
        Role::Ciso,
        format!("ciso@{}.example", company.to_lowercase()),
        None,
        gb,
        byok,
        ResidencyKind::Eu,
        None,
        "en-US",
        "cache",
    )
}

fn enc_config() -> LedgerEncryptionConfig {
    LedgerEncryptionConfig::for_tenant("tenant-prop", b"prop-key".to_vec())
}

fn byok_strat() -> impl Strategy<Value = BYOKRequirementsKind> {
    prop_oneof![
        Just(BYOKRequirementsKind::None),
        Just(BYOKRequirementsKind::AwsKms),
        Just(BYOKRequirementsKind::GcpKms),
        Just(BYOKRequirementsKind::AzureKv),
        Just(BYOKRequirementsKind::Vault),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(), .. ProptestConfig::default()
    })]

    /// Saga atomicity: every terminated submit_inquiry leaves the
    /// outbox in `Committed` (both legs ok) or `RolledBack` (either
    /// leg rejected) — never `Pending`.
    #[test]
    fn prop_saga_atomic_commit_or_rollback(
        slack_ok in any::<bool>(),
        crm_ok in any::<bool>(),
        byok in byok_strat(),
        gb in 0u64..10_000,
    ) {
        // Branch on the 4-cell happy/rollback matrix.
        let inquiry_id = InquiryId::new("inq-x");
        let key = IdempotencyKey::new("k-x");
        let f = form("Acme", gb, byok);

        let final_status: OutboxStatus = match (slack_ok, crm_ok) {
            (true, true) => {
                let l = EnterpriseInquiryLedger::new(
                    InMemoryInquiryAuditSink::new(),
                    InMemorySlackClient::new(),
                    InMemoryCrmClient::new(),
                    InMemoryAutoReplyMailer::new(),
                    InMemoryInquiryPayloadEncryptor::new(),
                    enc_config(),
                );
                let r = l.submit_inquiry(f, key, inquiry_id.clone(), 1_000, "cid").unwrap();
                prop_assert_eq!(r.status, InquiryStatus::Committed);
                l.get_outbox(&inquiry_id).unwrap().status
            }
            (false, _) => {
                let l = EnterpriseInquiryLedger::new(
                    InMemoryInquiryAuditSink::new(),
                    FailingSlackClient,
                    InMemoryCrmClient::new(),
                    InMemoryAutoReplyMailer::new(),
                    InMemoryInquiryPayloadEncryptor::new(),
                    enc_config(),
                );
                let err = l.submit_inquiry(f, key, inquiry_id.clone(), 1_000, "cid").unwrap_err();
                match &err {
                    EnterpriseInquiryError::Slack(inner) => {
                        prop_assert!(!format!("{inner:?}").is_empty(),
                            "Slack inner must carry a cause");
                    }
                    other => prop_assert!(false, "expected EnterpriseInquiryError::Slack, got {other:?}"),
                }
                l.get_outbox(&inquiry_id).unwrap().status
            }
            (true, false) => {
                let l = EnterpriseInquiryLedger::new(
                    InMemoryInquiryAuditSink::new(),
                    InMemorySlackClient::new(),
                    FailingCrmClient,
                    InMemoryAutoReplyMailer::new(),
                    InMemoryInquiryPayloadEncryptor::new(),
                    enc_config(),
                );
                let err = l.submit_inquiry(f, key, inquiry_id.clone(), 1_000, "cid").unwrap_err();
                match &err {
                    EnterpriseInquiryError::Crm(inner) => {
                        prop_assert!(!format!("{inner:?}").is_empty(),
                            "Crm inner must carry a cause");
                    }
                    other => prop_assert!(false, "expected EnterpriseInquiryError::Crm, got {other:?}"),
                }
                l.get_outbox(&inquiry_id).unwrap().status
            }
        };

        let committed_expected = slack_ok && crm_ok;
        if committed_expected {
            prop_assert_eq!(final_status, OutboxStatus::Committed);
        } else {
            prop_assert_eq!(final_status, OutboxStatus::RolledBack);
        }
    }

    /// Idempotency key dedup: N re-submissions sharing the key emit
    /// exactly 1 Slack post + 1 CRM entry + 1 auto-reply.
    #[test]
    fn prop_idempotency_key_dedup(n in 1u32..6) {
        let slack = InMemorySlackClient::new();
        let crm = InMemoryCrmClient::new();
        let mailer = InMemoryAutoReplyMailer::new();
        let l = EnterpriseInquiryLedger::new(
            InMemoryInquiryAuditSink::new(),
            slack.clone(),
            crm.clone(),
            mailer.clone(),
            InMemoryInquiryPayloadEncryptor::new(),
            enc_config(),
        );

        let key = IdempotencyKey::new("k-dedup");
        let mut last_id = None;
        for i in 0..n {
            let receipt = l.submit_inquiry(
                form("Acme", 100, BYOKRequirementsKind::AwsKms),
                key.clone(),
                InquiryId::new(format!("inq-{i}")),
                1_000 + u64::from(i),
                "cid",
            ).unwrap();
            if let Some(prev) = last_id.as_ref() {
                prop_assert_eq!(&receipt.inquiry_id, prev);
            } else {
                last_id = Some(receipt.inquiry_id.clone());
            }
        }
        prop_assert_eq!(slack.len(), 1);
        prop_assert_eq!(crm.len(), 1);
        prop_assert_eq!(mailer.len(), 1);
    }

    /// 24h SLA breach detection: every inquiry without replied_ms
    /// older than SLA_RESPONSE_MS surfaces.
    #[test]
    fn prop_sla_breach_detection(elapsed_ms in 0u64..(SLA_RESPONSE_MS * 3)) {
        // Use a FailingAutoReplyMailer so replied_ms stays None.
        let l = EnterpriseInquiryLedger::new(
            InMemoryInquiryAuditSink::new(),
            InMemorySlackClient::new(),
            InMemoryCrmClient::new(),
            corelink_enterprise_inquiry::FailingAutoReplyMailer,
            InMemoryInquiryPayloadEncryptor::new(),
            enc_config(),
        );
        let _ = l.submit_inquiry(
            form("Acme", 100, BYOKRequirementsKind::AwsKms),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid",
        );
        let now = 1_000 + elapsed_ms;
        let breaches = l.sla_breaches(now, "cid").unwrap();
        if elapsed_ms >= SLA_RESPONSE_MS {
            prop_assert_eq!(breaches.len(), 1);
            prop_assert_eq!(&breaches[0].inquiry_id, &InquiryId::new("inq-1"));
        } else {
            prop_assert!(breaches.is_empty());
        }
    }

    /// Fail-CLOSED on audit emit: the failing audit sink short-
    /// circuits every mutation; no inquiry / outbox row is persisted.
    #[test]
    fn prop_audit_emit_before_mutation(gb in 0u64..10_000) {
        let l = EnterpriseInquiryLedger::new(
            FailingInquiryAuditSink,
            InMemorySlackClient::new(),
            InMemoryCrmClient::new(),
            InMemoryAutoReplyMailer::new(),
            InMemoryInquiryPayloadEncryptor::new(),
            enc_config(),
        );
        let inquiry_id = InquiryId::new("inq-1");
        let err = l.submit_inquiry(
            form("Acme", gb, BYOKRequirementsKind::None),
            IdempotencyKey::new("k-1"),
            inquiry_id.clone(),
            1_000,
            "cid",
        ).unwrap_err();
        match &err {
            EnterpriseInquiryError::Audit(inner) => {
                prop_assert!(!format!("{inner:?}").is_empty(),
                    "Audit inner must carry a cause");
            }
            other => prop_assert!(false, "expected EnterpriseInquiryError::Audit, got {other:?}"),
        }
        prop_assert!(l.get_inquiry(&inquiry_id).is_none());
        prop_assert!(l.get_outbox(&inquiry_id).is_none());
    }

    /// CRM-failure rollback dispatches BOTH the original Slack post
    /// AND a compensating Slack ROLLBACK mark.
    #[test]
    fn prop_compensating_slack_mark_on_crm_failure(gb in 0u64..10_000) {
        let slack = InMemorySlackClient::new();
        let l = EnterpriseInquiryLedger::new(
            InMemoryInquiryAuditSink::new(),
            slack.clone(),
            FailingCrmClient,
            InMemoryAutoReplyMailer::new(),
            InMemoryInquiryPayloadEncryptor::new(),
            enc_config(),
        );
        let err = l.submit_inquiry(
            form("Acme", gb, BYOKRequirementsKind::AwsKms),
            IdempotencyKey::new("k-1"),
            InquiryId::new("inq-1"),
            1_000,
            "cid",
        ).unwrap_err();
        match &err {
            EnterpriseInquiryError::Crm(inner) => {
                prop_assert!(!format!("{inner:?}").is_empty(),
                    "Crm inner must carry a cause");
            }
            other => prop_assert!(false, "expected EnterpriseInquiryError::Crm, got {other:?}"),
        }
        let posts = slack.snapshot();
        prop_assert_eq!(posts.len(), 2);
        prop_assert_eq!(posts[0].kind, SlackPostKind::NewInquiry);
        prop_assert_eq!(posts[1].kind, SlackPostKind::Rollback);
    }

    /// R2-11 / S-19 P1-NEW-3: every random `InquiryForm` seals to a
    /// payload that fails-CLOSED when unsealed under any tenant_id
    /// other than the one bound at seal time (AAD binding cannot be
    /// spoofed by ciphertext substitution).
    #[test]
    fn prop_sealed_payload_unreadable_without_correct_aad(
        company in "[A-Za-z][A-Za-z0-9 ]{0,40}",
        gb in 0u64..10_000,
        byok in byok_strat(),
        seed_byte in any::<u8>(),
    ) {
        let f = form(&company, gb, byok);
        // Validate so we skip degenerate companies.
        prop_assume!(f.validate().is_ok());
        let mut seed = [0u8; 32];
        seed[0] = seed_byte;
        let enc = InMemoryInquiryPayloadEncryptor::with_seed(seed);
        let sealed = seal_inquiry(&f, "tenant-correct", b"k", &enc).unwrap();
        // Correct tenant id → unseal works.
        let recovered = unseal_inquiry(&sealed, "tenant-correct", &enc).unwrap();
        prop_assert_eq!(&recovered.company, &f.company);
        // Any other tenant id → fail-CLOSED.
        let result = unseal_inquiry(&sealed, "tenant-attacker", &enc);
        prop_assert!(result.is_err());
    }
}
