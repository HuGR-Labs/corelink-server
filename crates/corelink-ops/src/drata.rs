//! `corelink-drata-sync` — SOC 2 Drata evidence collection pipeline (R5-prep).
//!
//! This crate is the canonical wiring that pulls compliance evidence from
//! internal CoreLink sources and pushes it to Drata's continuous-monitoring
//! REST API. A CF Cron Worker calls [`runner::SyncRunner::run_daily`] at
//! 03:00 UTC; on each tick the runner drains every [`stream::EvidenceStream`]
//! source, deduplicates already-sent records via the
//! [`ledger::IdempotencyLedger`] (SHA-256 over the canonical record payload),
//! POSTs the survivors to the Drata endpoint matching the stream's
//! `endpoint_path()`, and records the resulting receipt id back into the
//! ledger plus the audit chain (event type
//! `corelink.compliance.drata_evidence_sent` per CTRL-AUDIT-DRATA-001).
//!
//! # Wire surface
//!
//! - [`stream::EvidenceStream`] — six-variant `#[non_exhaustive]` routing
//!   enum (AuditLogs / AccessReviews / CredentialManagement /
//!   ChangeManagement / IncidentResponse / VulnerabilityManagement).
//! - [`record::EvidenceRecord`] — the canonical payload envelope every
//!   source materializes into (`stream`, `source_id`, `occurred_at_ms`,
//!   `metadata` as a `BTreeMap<String, String>` so the SHA-256 is
//!   deterministic, never raw PII per CTRL-PRIV-001).
//! - [`record::record_sha256`] — canonical hashing helper that drives
//!   idempotency.
//! - [`drata::DrataClient`] / [`drata::DrataHttpClient`] — the Drata REST
//!   client trait + production reqwest::blocking wiring with
//!   Bearer-token auth, three-retry exponential-backoff policy on 5xx
//!   and 429, and `Idempotency-Key` header set to the record hash.
//! - [`drata::InMemoryDrataClient`] — in-memory fake recording every push
//!   for unit tests.
//! - [`audit::SyncAuditSink`] / [`audit::SyncAuditEvent`] — every push
//!   emits a `corelink.compliance.drata_evidence_sent` (or `.failed`)
//!   envelope BEFORE the network outcome is reported (fail-CLOSED,
//!   INV-AUDIT-EMIT-ATOMIC).
//! - [`ledger::IdempotencyLedger`] — fingerprint dedup; in-memory fake
//!   ships in this crate, D1-backed adapter is wired by callers against
//!   migration `0044_drata_evidence_sent.sql`.
//! - [`runner::SyncRunner`] — orchestrator the CF Cron Worker invokes.
//! - [`retry::RetryPolicy`] — synchronous retry policy (250ms initial,
//!   8s cap, 3 retries).
//!
//! # Privacy / safety
//!
//! - The Bearer token sourced from `DRATA_API_KEY` is NEVER logged or
//!   echoed in audit envelopes — [`redact::redact_api_key`] truncates to
//!   `drata-***<last4>`.
//! - Evidence payloads carry ONLY metadata + content hashes. No tenant
//!   identifiers, customer emails, or PAT strings cross the wire per
//!   CTRL-PRIV-001. The metadata map is a `BTreeMap` so callers cannot
//!   accidentally serialise raw struct fields.
//! - Audit envelope is the canonical reference for every push; the
//!   compliance officer can replay the chain to reconstruct any Drata
//!   submission without re-reading Drata itself.
//!
//! # Drata API surface (assumptions)
//!
//! The framework currently targets the Drata public REST API. Endpoint
//! paths below are placeholders pending Drata's published v1 surface
//! (see `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` §"API surface
//! assumptions"); the framework is decoupled from the exact path strings
//! via [`stream::EvidenceStream::endpoint_path`] so a single config
//! review can swap them.
//!
//! - `POST /v1/evidence/audit-logs`
//! - `POST /v1/evidence/access-reviews`
//! - `POST /v1/evidence/credential-management`
//! - `POST /v1/evidence/change-management`
//! - `POST /v1/evidence/incident-response`
//! - `POST /v1/evidence/vulnerability-management`
//!
//! All endpoints accept the same envelope schema (the
//! [`record::EvidenceRecord`] serialised as JSON) and return the canonical
//! Drata receipt id in the response body under `receipt_id`.
//!
//! # Runtime
//!
//! Production wiring uses `reqwest::blocking`. There is no `tokio`
//! dependency in `src/`. `wiremock` is dev-only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
#[allow(
    clippy::module_inception,
    reason = "W35-P2-OPS absorption: the inner `drata.rs` is the HTTPS Drata REST client (DrataClient / InMemoryDrataClient); the outer `drata.rs` is the absorbed `corelink-drata-sync` crate root. Inner module name mirrors the public `DrataClient` API anchor that downstream readers grep for; renaming would lose grep-ability. Pre-existing pattern in the original `corelink-drata-sync` crate."
)]
pub mod drata;
pub mod ledger;
pub mod record;
pub mod redact;
pub mod retry;
pub mod runner;
pub mod stream;

pub use audit::{
    InMemorySyncAuditSink, SyncAuditError, SyncAuditEvent, SyncAuditOutcome, SyncAuditSink,
};
pub use drata::{
    DrataClient, DrataClientError, DrataHttpClient, DrataReceipt, InMemoryDrataClient, RecordedPush,
};
pub use ledger::{IdempotencyLedger, InMemoryIdempotencyLedger, LedgerEntry, LedgerError};
pub use record::{record_sha256, EvidenceRecord};
pub use redact::redact_api_key;
pub use retry::{RetryDecision, RetryPolicy};
pub use runner::{SyncOutcome, SyncRunner, SyncRunnerError};
pub use stream::EvidenceStream;
