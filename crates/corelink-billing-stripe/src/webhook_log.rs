//! Stripe webhook event log trait + InMemory fake.
//!
//! ## Production wiring (deferred to WI-S10-007)
//!
//! Per WI-S10-003 §6.1.6 the production wiring binds this trait to a
//! D1 table `stripe_event_log` keyed by Stripe `evt_*` event id with
//! UNIQUE constraint:
//!
//! ```sql
//! CREATE TABLE stripe_event_log (
//!   stripe_event_id TEXT PRIMARY KEY,    -- Stripe evt_* UNIQUE
//!   event_type      TEXT NOT NULL,       -- 5 canonical types
//!   payload_redacted JSONB NOT NULL,     -- PII-redacted via Serialize impl
//!   received_at_ms  INTEGER NOT NULL,    -- canonical Unix epoch ms
//!   ...
//! );
//! ```
//!
//! The PRIMARY KEY UNIQUE prevents Stripe webhook double-processing
//! (Stripe may deliver the same webhook 2× under network failure;
//! sprint contract §15 R-007 mitigation).
//!
//! ## INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven Lote 6.2; S-09 inherit)
//!
//! The webhook log is INSERT-only at the storage layer; the in-memory
//! fake here pins the same append-only semantics so adversarial tests
//! can falsify the invariant independently of the production D1
//! binding.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::StripeWebhookLogError;
use crate::event::WebhookEvent;

/// Outcome of [`StripeWebhookLog::insert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebhookInsertOutcome {
    /// First sight at the canonical Stripe `evt_*` id: a fresh log row
    /// was inserted; the orchestrator dispatches the typed event to the
    /// downstream handler.
    Inserted,
    /// Re-delivery of the same canonical Stripe `evt_*` id within the
    /// canonical 7y retention window: the log is unchanged + the
    /// orchestrator short-circuits to the `duplicate_rejected`
    /// audit arm. INV-BILLING-NO-DUP enforcement at the webhook layer.
    AlreadyExists,
}

/// Stripe webhook event log trait. Production wiring composes:
///
/// - `D1StripeWebhookLog` — D1 INSERT into `stripe_event_log` with
///   `(stripe_event_id) UNIQUE`; ON CONFLICT DO NOTHING returns 0 rows
///   affected → `AlreadyExists` short-circuit; deferred to WI-S10-007
///   PRR ship gate per `trait-abstraction-defer` charter pattern.
pub trait StripeWebhookLog: Send + Sync + core::fmt::Debug {
    /// Insert `event` into the canonical webhook log. Idempotent on
    /// `event.stripe_event_id` UNIQUE.
    ///
    /// # Errors
    ///
    /// Returns [`StripeWebhookLogError::Backend`] on any backend
    /// failure.
    fn insert(&self, event: &WebhookEvent) -> Result<WebhookInsertOutcome, StripeWebhookLogError>;

    /// Look up a recorded webhook event by canonical Stripe event id.
    /// Returns `None` if no event has been logged for that id.
    fn get(&self, stripe_event_id: &str) -> Result<Option<WebhookEvent>, StripeWebhookLogError>;
}

/// In-memory Stripe webhook event log. Per-instance `Arc<Mutex<>>` per
/// the F-001 closure discipline; tests instantiate fresh logs per case
/// so cross-test contamination is structurally impossible.
#[derive(Clone, Default, Debug)]
pub struct InMemoryStripeWebhookLog {
    state: Arc<Mutex<HashMap<String, WebhookEvent>>>,
}

impl InMemoryStripeWebhookLog {
    /// Construct a fresh log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of recorded webhook events.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl StripeWebhookLog for InMemoryStripeWebhookLog {
    fn insert(&self, event: &WebhookEvent) -> Result<WebhookInsertOutcome, StripeWebhookLogError> {
        let mut g = self.state.lock().map_err(|_| {
            StripeWebhookLogError::Backend("billing-stripe webhook log mutex poisoned".to_string())
        })?;
        if g.contains_key(&event.stripe_event_id) {
            return Ok(WebhookInsertOutcome::AlreadyExists);
        }
        g.insert(event.stripe_event_id.clone(), event.clone());
        Ok(WebhookInsertOutcome::Inserted)
    }

    fn get(&self, stripe_event_id: &str) -> Result<Option<WebhookEvent>, StripeWebhookLogError> {
        let g = self.state.lock().map_err(|_| {
            StripeWebhookLogError::Backend("billing-stripe webhook log mutex poisoned".to_string())
        })?;
        Ok(g.get(stripe_event_id).cloned())
    }
}

/// Always-failing webhook log for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingStripeWebhookLog;

impl FailingStripeWebhookLog {
    /// Construct a fresh always-failing log.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl StripeWebhookLog for FailingStripeWebhookLog {
    fn insert(&self, _event: &WebhookEvent) -> Result<WebhookInsertOutcome, StripeWebhookLogError> {
        Err(StripeWebhookLogError::Backend(
            "induced billing-stripe webhook log failure (test fixture)".to_string(),
        ))
    }

    fn get(&self, _stripe_event_id: &str) -> Result<Option<WebhookEvent>, StripeWebhookLogError> {
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
    use crate::event::WebhookEventKind;

    fn fresh_event(id: &str, kind: WebhookEventKind) -> WebhookEvent {
        WebhookEvent {
            stripe_event_id: id.to_string(),
            kind,
            event_ts_ms: 1_700_000_000_000,
            payload_redacted: b"{\"redacted\":true}".to_vec(),
        }
    }

    #[test]
    fn fresh_log_is_empty() {
        let l = InMemoryStripeWebhookLog::new();
        assert!(l.is_empty());
    }

    #[test]
    fn first_insert_returns_inserted() {
        let l = InMemoryStripeWebhookLog::new();
        let e = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        let outcome = l.insert(&e).unwrap();
        assert_eq!(outcome, WebhookInsertOutcome::Inserted);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn duplicate_insert_returns_already_exists() {
        let l = InMemoryStripeWebhookLog::new();
        let e = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        l.insert(&e).unwrap();
        let outcome2 = l.insert(&e).unwrap();
        assert_eq!(outcome2, WebhookInsertOutcome::AlreadyExists);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn distinct_event_ids_inserted_independently() {
        let l = InMemoryStripeWebhookLog::new();
        let e1 = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        let e2 = fresh_event("evt_2", WebhookEventKind::InvoiceFailed);
        l.insert(&e1).unwrap();
        l.insert(&e2).unwrap();
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let l = InMemoryStripeWebhookLog::new();
        assert!(l.get("evt_unknown").unwrap().is_none());
    }

    #[test]
    fn get_returns_inserted_event() {
        let l = InMemoryStripeWebhookLog::new();
        let e = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        l.insert(&e).unwrap();
        let got = l.get("evt_1").unwrap().unwrap();
        assert_eq!(got, e);
    }

    #[test]
    fn cloned_log_shares_state() {
        let l1 = InMemoryStripeWebhookLog::new();
        let l2 = l1.clone();
        let e = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        l1.insert(&e).unwrap();
        assert_eq!(l2.len(), 1);
    }

    #[test]
    fn failing_log_returns_backend_error() {
        let l = FailingStripeWebhookLog::new();
        let e = fresh_event("evt_1", WebhookEventKind::InvoicePaid);
        let err = l.insert(&e).unwrap_err();
        assert!(matches!(err, StripeWebhookLogError::Backend(_)));
    }
}
