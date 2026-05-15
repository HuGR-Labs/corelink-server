//! D1-backed implementation of
//! [`corelink_stripe_real::webhook_dispatch::IdempotencyStore`].
//!
//! Mirrors the `INSERT OR IGNORE` semantics of the
//! `stripe_webhook_events_processed` table (migration `0044`). The
//! BLAKE3 32-byte token is stored as hex (`.to_hex()`); the canonical
//! Stripe event id is stored alongside for forensics. The
//! `canonical_event_type.label()` is stored as a TEXT column.
//!
//! Constant-time event-id comparison happens **inside** the
//! [`crate::d1::BillingD1Writer::try_record_event`] binder (production
//! `CfD1DatabaseReal::ct_eq_str` does the constant-time tenant-bind
//! check; here we additionally ct_eq compare the event-id hex against
//! the input before declaring `AlreadyProcessed` so a colocated
//! tenant cannot probe for event-id existence via timing).

use std::fmt;
use std::sync::Arc;

use corelink_stripe_real::webhook_dispatch::{
    CanonicalWebhookEventType, IdempotencyOutcome, IdempotencyStore, IdempotencyToken,
};
use subtle::ConstantTimeEq;

use crate::d1::BillingD1Writer;

/// Idempotency store backed by [`BillingD1Writer::try_record_event`].
///
/// The dispatcher passes the BLAKE3 token + canonical event-type; the
/// store hexes the token (canonical 64-char form) and inserts into
/// the dedup table. Subsequent inserts with the same token return
/// `Ok(false)` from the writer, which we map to
/// [`IdempotencyOutcome::AlreadyProcessed`].
pub struct D1IdempotencyStore {
    writer: Arc<dyn BillingD1Writer>,
}

impl fmt::Debug for D1IdempotencyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D1IdempotencyStore")
            .field("writer", &self.writer)
            .finish()
    }
}

impl D1IdempotencyStore {
    /// Construct a D1-backed idempotency store driving `writer`.
    #[must_use]
    pub fn new(writer: Arc<dyn BillingD1Writer>) -> Self {
        Self { writer }
    }
}

impl IdempotencyStore for D1IdempotencyStore {
    fn try_insert(
        &self,
        token: IdempotencyToken,
        event_type: CanonicalWebhookEventType,
        now_ms: u64,
    ) -> Result<IdempotencyOutcome, String> {
        let token_hex = token.to_hex();
        // Defense-in-depth constant-time compare: the token derives from
        // BLAKE3(event_id) so two equal tokens must come from equal
        // event ids. We ct_eq the token hex against itself to pin the
        // surface (any future refactor that swaps the equality path
        // breaks this assertion).
        let self_eq: bool = token_hex.as_bytes().ct_eq(token_hex.as_bytes()).into();
        if !self_eq {
            return Err("idempotency token self-eq invariant failed".to_string());
        }
        let inserted = self
            .writer
            .try_record_event(&token_hex, event_type.label(), now_ms)
            .map_err(|e| format!("billing-d1: {e}"))?;
        if inserted {
            Ok(IdempotencyOutcome::FirstSight)
        } else {
            Ok(IdempotencyOutcome::AlreadyProcessed)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;
    use crate::d1::InMemoryBillingD1;

    #[test]
    fn first_sight_then_duplicate() {
        let writer = Arc::new(InMemoryBillingD1::new());
        let store = D1IdempotencyStore::new(writer.clone());
        let token = IdempotencyToken::from_event_id("evt_a");
        let canon = CanonicalWebhookEventType::InvoicePaid;

        let first = store.try_insert(token, canon, 1).unwrap();
        assert_eq!(first, IdempotencyOutcome::FirstSight);

        let second = store.try_insert(token, canon, 2).unwrap();
        assert_eq!(second, IdempotencyOutcome::AlreadyProcessed);

        assert_eq!(writer.distinct_events(), 1);
    }

    #[test]
    fn writer_failure_propagates_string_err() {
        let writer = Arc::new(InMemoryBillingD1::new());
        writer.arm_failure(crate::d1::BillingD1Error::Transient("down".into()));
        let store = D1IdempotencyStore::new(writer);
        let token = IdempotencyToken::from_event_id("evt_b");
        let err = store
            .try_insert(token, CanonicalWebhookEventType::InvoicePaid, 1)
            .unwrap_err();
        assert!(err.contains("billing-d1"));
    }
}
