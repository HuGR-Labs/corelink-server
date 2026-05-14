//! `corelink-enterprise-inquiry` — white-glove enterprise inquiry
//! handler with atomic Slack + CRM dispatch + 24h auto-reply SLA
//! (WI-S19-005).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the enterprise inquiry form backend + Slack + CRM
//! atomic saga + 24h SLA breach detector plus the Slack webhook /
//! HubSpot CRM / SES auto-reply trait surface every production
//! HTTPS path will satisfy, plus in-memory fakes that exercise the
//! load-bearing invariants the production wiring relies on. Property
//! tests cover saga atomicity, idempotency dedup, and SLA breach
//! detection.
//!
//! # Saga PAT-SAGA-001 (reuse S-10) atomicity
//!
//! The handler dispatches Slack notification + CRM entry atomically:
//! BOTH succeed (and the outbox record advances to `Committed`) OR
//! both roll back (compensating Slack delete-or-mark message +
//! outbox advances to `RolledBack`). The outbox table acts as the
//! durable transaction log so the worker drain can resume in-flight
//! sagas after a crash.
//!
//! # 24h auto-reply SLA
//!
//! The background worker scans `enterprise_inquiries` for
//! `replied_at IS NULL AND created_at < now - 24h`; any row found
//! flags a Sev2 (24h white-glove SLA breach). Implemented here as
//! [`EnterpriseInquiryLedger::sla_breaches`] — a pure function over
//! the in-memory store + a configurable `now_ms` parameter so tests
//! can pin behaviour at any synthetic clock.
//!
//! # Invariants enforced
//!
//! - Saga atomicity: outbox advances to `Committed` only after BOTH
//!   Slack + CRM succeed; on either failure outbox advances to
//!   `RolledBack` and a compensating Slack action is dispatched
//!   (pinned by `prop_saga_atomic_commit_or_rollback`).
//! - Idempotency-key dedup: a re-submission with the same
//!   `IdempotencyKey` returns the original receipt without re-firing
//!   Slack/CRM/email (pinned by `prop_idempotency_key_dedup`).
//! - 24h SLA breach detection: any inquiry with
//!   `replied_at == None && now_ms - created_ms >= 24h_ms` is
//!   surfaced (pinned by `prop_sla_breach_detection`).
//! - Audit-emit-BEFORE-mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER;
//!   Lote 10.6bis pattern): audit failure aborts the ledger mutation
//!   and returns a typed error (pinned by
//!   `prop_audit_emit_before_mutation`).
//! - Form validation rejects malformed input (empty company /
//!   malformed email / `additional_notes` > 2000 chars).
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - Cloudflare Worker `POST /v1/enterprise/inquire` handler binding.
//! - Slack webhook HTTPS POST via `worker::send_future` fire-and-
//!   forget per Lote 10.7bis R5 P0-3 (NEVER `tokio::spawn`).
//! - SES auto-reply email HTTPS POST.
//! - D1 migration `0040_enterprise_inquiries.sql` apply in production
//!   schema (this crate stores the table DDL alongside the in-memory
//!   ledger so the migration can be regenerated mechanically).
//!
//! # HubSpot CRM real client (R2-5)
//!
//! The [`hubspot`] module ships [`hubspot::HubSpotCrmClient`] — a
//! production-ready [`crm::CrmClient`] impl wired to the HubSpot REST
//! API (`/crm/v3/objects/{contacts, companies, deals, tickets}`) with
//! Private App token auth, EU/US region routing, residency
//! enforcement, idempotent search-then-create, and exponential
//! backoff retry. HTTP transport is abstracted via the
//! [`hubspot::HubSpotHttp`] trait to preserve the "no tokio in src"
//! rule; the production binary wires a sync transport (e.g. `ureq`)
//! while tests use the in-crate [`hubspot::RecordingHubSpotHttp`]
//! mock.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod crm;
pub mod error;
pub mod form;
pub mod hubspot;
pub mod ledger;
pub mod mailer;
pub mod outbox;
pub mod slack;

pub use audit::{
    canonical_inquiry_audit_event_strings, FailingInquiryAuditSink, InMemoryInquiryAuditSink,
    InquiryAuditEmitError, InquiryAuditEventType, InquiryAuditRecord, InquiryAuditSink,
};
pub use crm::{CrmClient, CrmEntryId, CrmError, FailingCrmClient, InMemoryCrmClient};
pub use hubspot::{
    classify_retry, is_residency_routable, HubSpotConfigError, HubSpotCrmClient, HubSpotHttp,
    HubSpotHttpError, HubSpotMethod, HubSpotRegion, HubSpotRequest, HubSpotResponse, HubSpotSleeper,
    HubSpotToken, NoopSleeper, RecordingHubSpotHttp, RetryDecision, BASE_BACKOFF_MS,
    DEAL_STAGE_ENTERPRISE_INQUIRY, MAX_RETRIES, RETRY_AFTER_CAP_S,
};
pub use error::EnterpriseInquiryError;
pub use form::{
    BYOKRequirementsKind, EnterpriseInquiryForm, IdempotencyKey, InquiryId, InquiryReceipt,
    InquiryStatus, ResidencyKind, Role,
};
pub use ledger::{EnterpriseInquiryLedger, InquiryRecord, SlaBreach};
pub use mailer::{AutoReplyError, AutoReplyMailer, FailingAutoReplyMailer, InMemoryAutoReplyMailer};
pub use outbox::{OutboxRecord, OutboxStatus};
pub use slack::{
    FailingSlackClient, InMemorySlackClient, SlackClient, SlackError, SlackMessageId,
    SlackPostKind,
};

/// Crate canonical schema version constant.
#[must_use]
pub const fn enterprise_inquiry_schema_version() -> u32 {
    1
}

/// 24-hour SLA canonical (WI §6.1 / R-S19-10 white-glove timeline).
pub const SLA_RESPONSE_MS: u64 = 24 * 60 * 60 * 1000;

/// Auto-reply latency budget (5 minutes p99 per WI §6.1.5).
pub const AUTO_REPLY_BUDGET_MS: u64 = 5 * 60 * 1000;

/// Max `additional_notes` length (chars) per WI §6.1.1.
pub const ADDITIONAL_NOTES_MAX: usize = 2_000;

/// Max `company` length (chars) per WI §6.1.1.
pub const COMPANY_MAX: usize = 200;
