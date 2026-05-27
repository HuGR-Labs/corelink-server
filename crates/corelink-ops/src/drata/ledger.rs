//! Idempotency ledger.
//!
//! Each record's SHA-256 hash is the primary key into the ledger. A
//! cron tick first probes the ledger; on hit the record is skipped (the
//! Drata API was already given this exact payload on a previous run).
//! On miss the runner pushes to Drata, gets the receipt id back, then
//! writes `(hash, stream, receipt_id, sent_at_ms)` so the next run can
//! short-circuit.
//!
//! The in-memory adapter lives here; the D1-backed adapter is wired
//! against the SQL migration `0044_drata_evidence_sent.sql` by the
//! caller (the CF worker), keeping this crate database-agnostic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use super::stream::EvidenceStream;

/// A single ledger row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerEntry {
    /// SHA-256 hex of the record canonical JSON. Primary key.
    pub record_sha256: String,
    /// Stream the record was sent to.
    pub stream: EvidenceStream,
    /// Drata receipt id returned on the successful POST.
    pub receipt_id: String,
    /// Wall-clock timestamp (ms since epoch) the record was sent.
    pub sent_at_ms: i64,
}

/// Ledger error envelope.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum LedgerError {
    /// Underlying storage rejected the read or write.
    #[error("ledger storage failed: {0}")]
    StorageFailed(String),
}

/// Idempotency ledger trait. Both reads and writes must be durable
/// against replay (Object-Lock-style append-only) per CTRL-AUDIT-DRATA-001.
pub trait IdempotencyLedger: core::fmt::Debug + Send + Sync {
    /// Returns `Some(entry)` if `record_sha256` has been sent before,
    /// otherwise `None`.
    ///
    /// # Errors
    ///
    /// Returns `LedgerError::StorageFailed` on storage I/O error.
    fn lookup(&self, record_sha256: &str) -> Result<Option<LedgerEntry>, LedgerError>;

    /// Append a fresh entry. Must be idempotent against a repeat insert
    /// (idempotent UPSERT in production).
    ///
    /// # Errors
    ///
    /// Returns `LedgerError::StorageFailed` on storage I/O error.
    fn record(&self, entry: &LedgerEntry) -> Result<(), LedgerError>;
}

/// In-memory ledger — driven by tests + the CF cold-start cache layer.
#[derive(Clone, Debug, Default)]
pub struct InMemoryIdempotencyLedger {
    inner: Arc<Mutex<HashMap<String, LedgerEntry>>>,
}

impl InMemoryIdempotencyLedger {
    /// Empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when the ledger is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot of all entries (test-only convenience).
    #[must_use]
    pub fn snapshot(&self) -> Vec<LedgerEntry> {
        match self.inner.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(p) => p.into_inner().values().cloned().collect(),
        }
    }
}

impl IdempotencyLedger for InMemoryIdempotencyLedger {
    fn lookup(&self, record_sha256: &str) -> Result<Option<LedgerEntry>, LedgerError> {
        let g = self
            .inner
            .lock()
            .map_err(|e| LedgerError::StorageFailed(format!("mutex poisoned: {e}")))?;
        Ok(g.get(record_sha256).cloned())
    }

    fn record(&self, entry: &LedgerEntry) -> Result<(), LedgerError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| LedgerError::StorageFailed(format!("mutex poisoned: {e}")))?;
        // Repeat-insert idempotent: keep the first receipt.
        g.entry(entry.record_sha256.clone())
            .or_insert_with(|| entry.clone());
        Ok(())
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

    fn entry(hash: &str) -> LedgerEntry {
        LedgerEntry {
            record_sha256: hash.into(),
            stream: EvidenceStream::AuditLogs,
            receipt_id: "rcp_1".into(),
            sent_at_ms: 1,
        }
    }

    #[test]
    fn fresh_lookup_misses() {
        let l = InMemoryIdempotencyLedger::new();
        assert!(l.lookup("nope").unwrap().is_none());
    }

    #[test]
    fn record_then_lookup_hits() {
        let l = InMemoryIdempotencyLedger::new();
        l.record(&entry("h1")).unwrap();
        assert_eq!(l.lookup("h1").unwrap().unwrap().receipt_id, "rcp_1");
    }

    #[test]
    fn record_is_idempotent() {
        let l = InMemoryIdempotencyLedger::new();
        l.record(&entry("h1")).unwrap();
        let mut e2 = entry("h1");
        e2.receipt_id = "rcp_2".into();
        l.record(&e2).unwrap();
        // First receipt preserved.
        assert_eq!(l.lookup("h1").unwrap().unwrap().receipt_id, "rcp_1");
        assert_eq!(l.len(), 1);
    }
}
