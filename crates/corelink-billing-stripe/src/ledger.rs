//! Stripe usage-record ledger trait + InMemory fake.
//!
//! ## Production wiring (deferred to WI-S10-007)
//!
//! Per WI-S10-003 §6.1 the production wiring binds this trait to the
//! Stripe API surface
//! `POST /v1/subscription_items/{subscription_item_id}/usage_records`
//! with the `Idempotency-Key` HTTP header set to the canonical 64-char
//! hex from [`crate::idempotency::derive_idempotency_key`]. Stripe
//! guarantees that within the canonical 24h idempotency window two POSTs
//! with the same `Idempotency-Key` return the SAME `usage_record` — the
//! customer is charged exactly once regardless of retry storms (sprint
//! contract §15 R-003 mitigation).
//!
//! The in-memory fake here pins the same `(idempotency_key)` UNIQUE PK
//! semantics at the trait surface so adversarial tests can falsify
//! INV-BILLING-NO-DUP independently of the production HTTP client.
//!
//! ## INV-BILLING-NO-DUP (HIGH; invariant_registry §3.9 line 137)
//!
//! Per-key UNIQUE membership: the canonical `Idempotency-Key` is the
//! BLAKE3-256 hex of the JCS-canonical aggregate bytes; same aggregate
//! reproduces the same key; the ledger short-circuits to
//! `RecordOutcome::AlreadyExistsIdempotent` on the second sight (Stripe
//! API guarantees the same response within the 24h window).
//!
//! ## F-001 closure
//!
//! The fake holds the per-`IdempotencyKey` ledger under a per-instance
//! `Arc<Mutex<>>` (NOT a `static LazyLock`); tests instantiate fresh
//! ledgers per case so the orchestrator harness cannot accidentally
//! leak state across cases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::StripeUsageLedgerError;
use crate::event::{IdempotencyKey, UsageRecordRequest};

/// Outcome of [`StripeUsageLedger::record`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordOutcome {
    /// First sight at the canonical `Idempotency-Key`: a fresh row was
    /// recorded at the Stripe ledger (in production: the Stripe API
    /// returned 2xx with a fresh `usage_record_id`; in the fake: the
    /// in-memory ledger inserted a new entry).
    Recorded,
    /// Re-emit with the same canonical `Idempotency-Key` within the
    /// canonical 24h Stripe idempotency window: the Stripe ledger is
    /// unchanged + Stripe returns the same `usage_record_id`. The
    /// in-memory fake mirrors the same arm; the orchestrator dispatches
    /// to the `duplicate_rejected` audit path.
    AlreadyExistsIdempotent,
}

/// Stripe usage-record ledger trait. Production wiring composes:
///
/// - `HttpStripeUsageLedger` — `reqwest::Client` POSTs to the canonical
///   `/v1/subscription_items/{id}/usage_records` endpoint with the
///   `Idempotency-Key` header set to the canonical hex; deferred to
///   WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter
///   pattern.
pub trait StripeUsageLedger: Send + Sync + core::fmt::Debug {
    /// Record `request` at the Stripe usage ledger. The canonical
    /// `request.idempotency_key` is the SOURCE OF TRUTH for dedup; the
    /// implementation MUST be idempotent within the canonical 24h
    /// window per the Stripe API contract.
    ///
    /// ## INV-BILLING-NO-DUP enforcement
    ///
    /// - First sight at `request.idempotency_key` → record + return
    ///   `RecordOutcome::Recorded`.
    /// - Re-emit with same `request.idempotency_key` AND byte-identical
    ///   request payload → `RecordOutcome::AlreadyExistsIdempotent`
    ///   (Stripe returns the same `usage_record_id`).
    /// - Re-emit with same `request.idempotency_key` BUT diverged
    ///   payload → `StripeUsageLedgerError::IdempotencyKeyReuse`
    ///   (CRITICAL replay-corruption signal; only triggers if the
    ///   canonical aggregate-bytes derivation has been broken upstream
    ///   by a code-bug regression).
    ///
    /// # Errors
    ///
    /// Returns [`StripeUsageLedgerError::Backend`] on any backend
    /// failure + [`StripeUsageLedgerError::IdempotencyKeyReuse`] on
    /// canonical-bytes divergence.
    fn record(&self, request: &UsageRecordRequest)
        -> Result<RecordOutcome, StripeUsageLedgerError>;

    /// Look up a recorded usage record by canonical idempotency key.
    /// Returns `None` if no record has been written for that key.
    /// Used by the orchestrator to surface the existing record on the
    /// idempotent-duplicate arm + by adversarial tests to verify
    /// INV-BILLING-NO-DUP at the trait surface.
    fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<UsageRecordRequest>, StripeUsageLedgerError>;
}

/// In-memory Stripe usage-record ledger. Per-instance `Arc<Mutex<>>` per
/// the F-001 closure discipline; tests instantiate fresh ledgers per
/// case so cross-test contamination is structurally impossible.
#[derive(Clone, Default, Debug)]
pub struct InMemoryStripeUsageLedger {
    state: Arc<Mutex<LedgerState>>,
}

#[derive(Debug, Default)]
struct LedgerState {
    rows: HashMap<IdempotencyKey, UsageRecordRequest>,
}

impl InMemoryStripeUsageLedger {
    /// Construct a fresh ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every recorded usage request (sorted by canonical
    /// idempotency key hex for deterministic test assertions).
    #[must_use]
    pub fn snapshot(&self) -> Vec<UsageRecordRequest> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<UsageRecordRequest> = g.rows.values().cloned().collect();
        out.sort_by(|a, b| a.idempotency_key.to_hex().cmp(&b.idempotency_key.to_hex()));
        out
    }

    /// Number of recorded usage requests.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.rows.len()
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl StripeUsageLedger for InMemoryStripeUsageLedger {
    fn record(
        &self,
        request: &UsageRecordRequest,
    ) -> Result<RecordOutcome, StripeUsageLedgerError> {
        let mut g = self.state.lock().map_err(|_| {
            StripeUsageLedgerError::Backend(
                "billing-stripe usage ledger mutex poisoned".to_string(),
            )
        })?;
        if let Some(prior) = g.rows.get(&request.idempotency_key) {
            if prior == request {
                return Ok(RecordOutcome::AlreadyExistsIdempotent);
            }
            return Err(StripeUsageLedgerError::IdempotencyKeyReuse {
                idempotency_key: request.idempotency_key.to_hex(),
            });
        }
        g.rows.insert(request.idempotency_key, request.clone());
        Ok(RecordOutcome::Recorded)
    }

    fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<UsageRecordRequest>, StripeUsageLedgerError> {
        let g = self.state.lock().map_err(|_| {
            StripeUsageLedgerError::Backend(
                "billing-stripe usage ledger mutex poisoned".to_string(),
            )
        })?;
        Ok(g.rows.get(key).cloned())
    }
}

/// Always-failing Stripe usage-record ledger for adversarial tests of
/// the fail-CLOSED envelope.
#[derive(Debug, Default)]
pub struct FailingStripeUsageLedger;

impl FailingStripeUsageLedger {
    /// Construct a fresh always-failing ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl StripeUsageLedger for FailingStripeUsageLedger {
    fn record(
        &self,
        _request: &UsageRecordRequest,
    ) -> Result<RecordOutcome, StripeUsageLedgerError> {
        Err(StripeUsageLedgerError::Backend(
            "induced billing-stripe usage ledger failure (test fixture)".to_string(),
        ))
    }

    fn get(
        &self,
        _key: &IdempotencyKey,
    ) -> Result<Option<UsageRecordRequest>, StripeUsageLedgerError> {
        Ok(None)
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
    use crate::event::SubscriptionItemId;
    use uuid::Uuid;

    fn fresh_request(qty: u128, key_byte: u8) -> UsageRecordRequest {
        UsageRecordRequest {
            tenant_id: Uuid::now_v7(),
            billing_period: "2026-05".to_string(),
            event_kind: "cas_put".to_string(),
            total_qty: qty,
            subscription_item_id: SubscriptionItemId::new("si_x"),
            idempotency_key: IdempotencyKey([key_byte; 32]),
            now_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn fresh_ledger_is_empty() {
        let l = InMemoryStripeUsageLedger::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn first_record_returns_recorded() {
        let l = InMemoryStripeUsageLedger::new();
        let req = fresh_request(100, 0xAB);
        let outcome = l.record(&req).unwrap();
        assert_eq!(outcome, RecordOutcome::Recorded);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn duplicate_record_with_same_payload_returns_already_exists() {
        let l = InMemoryStripeUsageLedger::new();
        let req = fresh_request(100, 0xAB);
        let _ = l.record(&req).unwrap();
        let outcome2 = l.record(&req).unwrap();
        assert_eq!(outcome2, RecordOutcome::AlreadyExistsIdempotent);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn duplicate_record_with_diverged_payload_rejects() {
        let l = InMemoryStripeUsageLedger::new();
        let mut req1 = fresh_request(100, 0xAB);
        req1.tenant_id = Uuid::now_v7();
        let mut req2 = req1.clone();
        req2.total_qty = 999;
        let _ = l.record(&req1).unwrap();
        let err = l.record(&req2).unwrap_err();
        assert!(matches!(
            err,
            StripeUsageLedgerError::IdempotencyKeyReuse { .. }
        ));
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let l = InMemoryStripeUsageLedger::new();
        let key = IdempotencyKey([0xFF; 32]);
        assert!(l.get(&key).unwrap().is_none());
    }

    #[test]
    fn get_returns_recorded_request() {
        let l = InMemoryStripeUsageLedger::new();
        let req = fresh_request(100, 0xAB);
        l.record(&req).unwrap();
        let got = l.get(&req.idempotency_key).unwrap().unwrap();
        assert_eq!(got, req);
    }

    #[test]
    fn cloned_ledger_shares_state() {
        let l1 = InMemoryStripeUsageLedger::new();
        let l2 = l1.clone();
        let req = fresh_request(100, 0xAB);
        l1.record(&req).unwrap();
        assert_eq!(l2.len(), 1);
    }

    #[test]
    fn distinct_keys_record_independently() {
        let l = InMemoryStripeUsageLedger::new();
        let r1 = fresh_request(100, 0xAB);
        let r2 = fresh_request(200, 0xCD);
        l.record(&r1).unwrap();
        l.record(&r2).unwrap();
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn snapshot_sorted_by_canonical_hex() {
        let l = InMemoryStripeUsageLedger::new();
        let r1 = fresh_request(1, 0xFF);
        let r2 = fresh_request(2, 0x00);
        let r3 = fresh_request(3, 0x80);
        l.record(&r1).unwrap();
        l.record(&r2).unwrap();
        l.record(&r3).unwrap();
        let snap = l.snapshot();
        assert_eq!(snap.len(), 3);
        // Sorted ascending by hex: 0x00.. < 0x80.. < 0xFF...
        assert_eq!(snap[0].idempotency_key, IdempotencyKey([0x00; 32]));
        assert_eq!(snap[1].idempotency_key, IdempotencyKey([0x80; 32]));
        assert_eq!(snap[2].idempotency_key, IdempotencyKey([0xFF; 32]));
    }

    #[test]
    fn failing_ledger_returns_backend_error() {
        let l = FailingStripeUsageLedger::new();
        let req = fresh_request(100, 0xAB);
        let err = l.record(&req).unwrap_err();
        assert!(matches!(err, StripeUsageLedgerError::Backend(_)));
    }
}
