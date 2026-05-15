//! Sync runner. Invoked once per cron tick (production: 03:00 UTC daily
//! via CF Cron Worker). Drains every incoming evidence record, dedups
//! via the ledger, pushes survivors to Drata, persists the receipt back
//! to the ledger, and emits the audit envelope.
//!
//! The runner is intentionally agnostic about how records reach it. The
//! caller (the CF worker) queries each source (audit_outbox, tenants,
//! pat issuance, GH webhooks, PD timeline, pentest findings), maps each
//! domain row to an [`crate::record::EvidenceRecord`], and hands the
//! whole batch to [`SyncRunner::run_batch`]. This keeps the crate
//! testable end-to-end without coupling to D1 or any specific webhook
//! framework.

use std::sync::Arc;

use thiserror::Error;

use crate::audit::{SyncAuditError, SyncAuditEvent, SyncAuditOutcome, SyncAuditSink};
use crate::drata::{DrataClient, DrataClientError};
use crate::ledger::{IdempotencyLedger, LedgerEntry, LedgerError};
use crate::record::{record_sha256, EvidenceRecord};

/// Per-record outcome aggregated across the batch.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncOutcome {
    /// Records pushed to Drata successfully.
    pub sent: u32,
    /// Records short-circuited because the ledger already had them.
    pub skipped: u32,
    /// Records that failed (transport exhausted OR permanent reject).
    pub failed: u32,
}

impl SyncOutcome {
    /// Total records considered (sent + skipped + failed).
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.sent
            .saturating_add(self.skipped)
            .saturating_add(self.failed)
    }
}

/// Runner error envelope. Fatal — surfaces only when the audit chain
/// rejects an emit (fail-CLOSED) or the ledger storage is broken. Per
/// record Drata errors are swallowed into `SyncOutcome.failed` so a
/// single 4xx does not abort the whole batch.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncRunnerError {
    /// Audit-chain emit failed.
    #[error("runner audit emit failed: {0}")]
    AuditFailed(SyncAuditError),
    /// Ledger storage failed.
    #[error("runner ledger failed: {0}")]
    LedgerFailed(LedgerError),
    /// Misconfiguration discovered at runtime (e.g. clock returned
    /// negative).
    #[error("runner misconfigured: {0}")]
    Misconfigured(String),
}

impl From<SyncAuditError> for SyncRunnerError {
    fn from(e: SyncAuditError) -> Self {
        Self::AuditFailed(e)
    }
}

impl From<LedgerError> for SyncRunnerError {
    fn from(e: LedgerError) -> Self {
        Self::LedgerFailed(e)
    }
}

/// Clock trait. Production wires `SystemTime::now()`; tests inject a
/// fixed clock so audit timestamps are deterministic.
pub trait Clock: core::fmt::Debug + Send + Sync {
    /// Current wall-clock time in milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// System-clock wiring.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO);
        i64::try_from(now.as_millis()).unwrap_or(i64::MAX)
    }
}

/// Fixed clock for tests.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

/// Runner orchestrator.
pub struct SyncRunner {
    drata: Arc<dyn DrataClient>,
    ledger: Arc<dyn IdempotencyLedger>,
    audit: Arc<dyn SyncAuditSink>,
    clock: Arc<dyn Clock>,
    api_key_redacted: String,
}

impl core::fmt::Debug for SyncRunner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SyncRunner")
            .field("api_key_redacted", &self.api_key_redacted)
            .finish_non_exhaustive()
    }
}

impl SyncRunner {
    /// Construct a runner. `api_key_redacted` is included in every audit
    /// envelope so the chain can be matched against rotated tokens; the
    /// caller pre-redacts via [`crate::redact::redact_api_key`].
    pub fn new(
        drata: Arc<dyn DrataClient>,
        ledger: Arc<dyn IdempotencyLedger>,
        audit: Arc<dyn SyncAuditSink>,
        clock: Arc<dyn Clock>,
        api_key_redacted: impl Into<String>,
    ) -> Self {
        Self {
            drata,
            ledger,
            audit,
            clock,
            api_key_redacted: api_key_redacted.into(),
        }
    }

    /// Process a batch of records. The runner returns the aggregate
    /// outcome; per-record failures are routed into the audit chain
    /// (fail-OPEN per record) but the batch itself only aborts when the
    /// audit sink OR ledger storage fails (fail-CLOSED at infra layer).
    ///
    /// # Errors
    ///
    /// - [`SyncRunnerError::AuditFailed`] — audit chain rejected.
    /// - [`SyncRunnerError::LedgerFailed`] — ledger storage rejected.
    /// - [`SyncRunnerError::Misconfigured`] — internal encode error
    ///   (would only fire if the canonical JSON encoder fails on a
    ///   well-typed `BTreeMap<String, String>`, which it cannot in
    ///   practice; surfaced anyway for completeness).
    pub fn run_batch(&self, records: &[EvidenceRecord]) -> Result<SyncOutcome, SyncRunnerError> {
        let mut outcome = SyncOutcome {
            sent: 0,
            skipped: 0,
            failed: 0,
        };
        for record in records {
            let hash = record_sha256(record)
                .map_err(|e| SyncRunnerError::Misconfigured(format!("record encode: {e}")))?;
            if let Some(existing) = self.ledger.lookup(&hash)? {
                self.audit.emit(&SyncAuditEvent {
                    outcome: SyncAuditOutcome::Skipped,
                    stream: record.stream,
                    record_sha256: hash.clone(),
                    api_key_redacted: self.api_key_redacted.clone(),
                    receipt_id: Some(existing.receipt_id),
                    final_status: None,
                    attempts: 0,
                    reason: Some("idempotent: ledger hit".to_string()),
                })?;
                outcome.skipped = outcome.skipped.saturating_add(1);
                continue;
            }
            match self.drata.push(record, &hash) {
                Ok(receipt) => {
                    let entry = LedgerEntry {
                        record_sha256: hash.clone(),
                        stream: record.stream,
                        receipt_id: receipt.receipt_id.clone(),
                        sent_at_ms: self.clock.now_ms(),
                    };
                    self.ledger.record(&entry)?;
                    self.audit.emit(&SyncAuditEvent {
                        outcome: SyncAuditOutcome::Sent,
                        stream: record.stream,
                        record_sha256: hash.clone(),
                        api_key_redacted: self.api_key_redacted.clone(),
                        receipt_id: Some(receipt.receipt_id),
                        final_status: Some(receipt.http_status),
                        attempts: 1,
                        reason: None,
                    })?;
                    outcome.sent = outcome.sent.saturating_add(1);
                }
                Err(e) => {
                    let (status, reason) = drata_error_summary(&e);
                    self.audit.emit(&SyncAuditEvent {
                        outcome: SyncAuditOutcome::Failed,
                        stream: record.stream,
                        record_sha256: hash.clone(),
                        api_key_redacted: self.api_key_redacted.clone(),
                        receipt_id: None,
                        final_status: status,
                        attempts: 1,
                        reason: Some(reason),
                    })?;
                    outcome.failed = outcome.failed.saturating_add(1);
                }
            }
        }
        Ok(outcome)
    }

    /// Daily-cron entry point. Identical to [`Self::run_batch`] — kept
    /// as a distinct symbol so the CF worker has a stable name to wire
    /// against and the audit chain can correlate `daily` vs `manual`
    /// invocations via the calling site.
    ///
    /// # Errors
    ///
    /// See [`Self::run_batch`].
    pub fn run_daily(&self, records: &[EvidenceRecord]) -> Result<SyncOutcome, SyncRunnerError> {
        self.run_batch(records)
    }
}

fn drata_error_summary(e: &DrataClientError) -> (Option<u16>, String) {
    match e {
        DrataClientError::TransportExhausted { reason, .. } => (None, reason.clone()),
        DrataClientError::PermanentReject { status, reason } => (Some(*status), reason.clone()),
        DrataClientError::Misconfigured(r) | DrataClientError::EncodeFailed(r) => (None, r.clone()),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::InMemorySyncAuditSink;
    use crate::drata::InMemoryDrataClient;
    use crate::ledger::InMemoryIdempotencyLedger;
    use crate::stream::EvidenceStream;

    fn runner() -> (
        SyncRunner,
        Arc<InMemoryDrataClient>,
        Arc<InMemoryIdempotencyLedger>,
        Arc<InMemorySyncAuditSink>,
    ) {
        let drata = Arc::new(InMemoryDrataClient::new());
        let ledger = Arc::new(InMemoryIdempotencyLedger::new());
        let audit = Arc::new(InMemorySyncAuditSink::new());
        let r = SyncRunner::new(
            Arc::clone(&drata) as Arc<dyn DrataClient>,
            Arc::clone(&ledger) as Arc<dyn IdempotencyLedger>,
            Arc::clone(&audit) as Arc<dyn SyncAuditSink>,
            Arc::new(FixedClock(42)),
            "drata-***1234",
        );
        (r, drata, ledger, audit)
    }

    fn rec(id: &str) -> EvidenceRecord {
        EvidenceRecord::new(
            EvidenceStream::AuditLogs,
            id,
            1,
            [("k", "v")],
        )
    }

    #[test]
    fn single_send_records_outcome_and_ledger() {
        let (r, drata, ledger, audit) = runner();
        let out = r.run_batch(&[rec("a")]).unwrap();
        assert_eq!(out.sent, 1);
        assert_eq!(out.skipped, 0);
        assert_eq!(out.failed, 0);
        assert_eq!(drata.push_count(), 1);
        assert_eq!(ledger.len(), 1);
        let evt = audit.snapshot();
        assert_eq!(evt.len(), 1);
        assert_eq!(evt[0].outcome, SyncAuditOutcome::Sent);
    }

    #[test]
    fn repeat_record_is_skipped() {
        let (r, drata, ledger, audit) = runner();
        r.run_batch(&[rec("a")]).unwrap();
        let out = r.run_batch(&[rec("a")]).unwrap();
        assert_eq!(out.skipped, 1);
        assert_eq!(out.sent, 0);
        // Drata only called once.
        assert_eq!(drata.push_count(), 1);
        assert_eq!(ledger.len(), 1);
        let evts = audit.snapshot();
        assert_eq!(evts.len(), 2);
        assert_eq!(evts[1].outcome, SyncAuditOutcome::Skipped);
    }

    #[test]
    fn drata_4xx_records_failure_without_ledger_write() {
        let (r, drata, ledger, audit) = runner();
        drata.enqueue_error(DrataClientError::PermanentReject {
            status: 422,
            reason: "schema".into(),
        });
        let out = r.run_batch(&[rec("a")]).unwrap();
        assert_eq!(out.failed, 1);
        assert_eq!(ledger.len(), 0);
        let evts = audit.snapshot();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].outcome, SyncAuditOutcome::Failed);
        assert_eq!(evts[0].final_status, Some(422));
    }

    #[test]
    fn run_daily_is_alias() {
        let (r, _drata, _ledger, _audit) = runner();
        let out = r.run_daily(&[rec("a"), rec("b")]).unwrap();
        assert_eq!(out.sent, 2);
        assert_eq!(out.total(), 2);
    }
}
