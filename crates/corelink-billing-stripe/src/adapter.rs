//! `StripeBillingAdapter` trait + `InMemoryStripeBillingAdapter`
//! orchestrator.
//!
//! The orchestrator wires the three primitives — JCS canonicalization +
//! BLAKE3 idempotency-key derivation (via [`crate::idempotency`]), the
//! audit-fail-CLOSED envelope (via [`crate::audit`]), and the
//! [`crate::ledger::StripeUsageLedger`] — into a single record pipeline
//! that satisfies every load-bearing invariant the production HTTP-client
//! Stripe wiring will depend on (per the `trait-abstraction-defer`
//! charter pattern).
//!
//! ## Record pipeline (per usage emit)
//!
//! For each input `(AggregatedCounter, SubscriptionItemId, now_ms)`:
//!
//! 1. **Canonicalize** the aggregate via JCS RFC 8785.
//! 2. **Derive** the canonical `Idempotency-Key` via BLAKE3-256 of the
//!    canonical bytes (mirrors the upstream chain digest convention;
//!    same aggregate → same key).
//! 3. **Audit BEFORE state mutation**: emit
//!    `corelink.billing_stripe.usage_recorded` (or
//!    `duplicate_rejected`) BEFORE the ledger record. Audit failure
//!    aborts the pipeline (NO Stripe API call, NO ledger write).
//! 4. **Ledger record**: delegate to the trait surface; outcome
//!    discriminates the decision arm.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-003 §6.1 + sprint contract, every decision arm fires its
//! canonical audit BEFORE the state-mutating step:
//!
//! - `usage_recorded` fires BEFORE the ledger record on the first-sight
//!   arm.
//! - `duplicate_rejected` fires BEFORE returning the duplicate decision
//!   on the AlreadyExistsIdempotent arm.
//! - On ledger backend failure the orchestrator currently surfaces the
//!   `StripeError::Ledger` directly (no `sink_failure` audit; the
//!   production wiring at WI-S10-007 fans this out to the canonical
//!   PAT-QUEUE-EVENTS-001 retry queue + alerting layer).
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! `StripeError::Audit` (caller sees no state mutation past the point
//! of the failure).
//!
//! ## Why concrete trait-generic ledger (mirrors aggregator pattern)
//!
//! The orchestrator is generic over both the audit sink + the ledger so
//! adversarial tests can substitute the failing fakes
//! ([`crate::audit::FailingStripeAuditSink`] +
//! [`crate::ledger::FailingStripeUsageLedger`]) without re-running the
//! production HTTP client. Production wiring at WI-S10-007 ships its
//! own `HttpStripeBillingAdapter` variant that delegates to
//! `reqwest::Client` POSTs at the canonical Stripe API surface.
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + ledger as `Arc` handles
//! passed at construction; per-instance state lives inside those Arc'd
//! primitives. Tests instantiate fresh orchestrators per case so
//! cross-test contamination is structurally impossible.

use std::sync::Arc;

use corelink_billing_aggregator::AggregatedCounter;

use crate::audit::{
    StripeAuditEventType, StripeAuditRecord, StripeAuditSink,
};
use crate::error::StripeError;
use crate::event::{
    IdempotencyKey, StripeAdapterDecision, SubscriptionItemId, UsageRecordRequest,
};
use crate::idempotency::{
    compute_canonical_aggregate_bytes, derive_idempotency_key_from_canonical,
};
use crate::ledger::{RecordOutcome, StripeUsageLedger};

/// Stripe billing adapter trait. Production wiring composes:
///
/// - `HttpStripeBillingAdapter` — `reqwest::Client` POSTs to the
///   canonical `/v1/subscription_items/{id}/usage_records` endpoint
///   with the canonical `Idempotency-Key` HTTP header; deferred to
///   WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter
///   pattern.
pub trait StripeBillingAdapter: Send + Sync + core::fmt::Debug {
    /// Record `aggregate` as a Stripe usage record, scoped to
    /// `subscription_item_id`. Idempotent on the canonical
    /// BLAKE3-256-derived `Idempotency-Key`.
    ///
    /// # Errors
    ///
    /// - [`StripeError::Canonicalization`] when JCS fails.
    /// - [`StripeError::Audit`] when the audit envelope rejects any
    ///   decision arm (fail-CLOSED at the trait surface).
    /// - [`StripeError::Ledger`] when the underlying ledger rejects
    ///   the record (Stripe API backend failure / canonical-bytes
    ///   divergence on idempotency-key reuse).
    /// - [`StripeError::Internal`] when a per-instance mutex is
    ///   poisoned.
    fn record_usage(
        &self,
        aggregate: &AggregatedCounter,
        subscription_item_id: SubscriptionItemId,
        now_ms: u64,
    ) -> Result<StripeAdapterDecision, StripeError>;

    /// Look up a recorded usage record by canonical idempotency key.
    /// Used by reconciliation (WI-S10-004) Layer 3 + adversarial tests.
    ///
    /// # Errors
    ///
    /// Returns the canonical [`StripeError::Ledger`] on backend
    /// failure.
    fn get_recorded(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<UsageRecordRequest>, StripeError>;
}

/// In-memory Stripe billing adapter orchestrator. Composes the audit
/// sink + the ledger via `Arc` handles; both are generic over the trait
/// so test fakes (e.g. [`crate::audit::FailingStripeAuditSink`] +
/// [`crate::ledger::FailingStripeUsageLedger`]) compose directly at
/// construction.
#[derive(Clone, Debug)]
pub struct InMemoryStripeBillingAdapter<A, L>
where
    A: StripeAuditSink + 'static,
    L: StripeUsageLedger + 'static,
{
    audit: Arc<A>,
    ledger: Arc<L>,
}

impl<A, L> InMemoryStripeBillingAdapter<A, L>
where
    A: StripeAuditSink + 'static,
    L: StripeUsageLedger + 'static,
{
    /// Construct with explicit audit + ledger handles.
    pub fn new(audit: Arc<A>, ledger: Arc<L>) -> Self {
        Self { audit, ledger }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the ledger.
    #[must_use]
    pub fn ledger(&self) -> &Arc<L> {
        &self.ledger
    }

    fn audit_record(
        event_type: StripeAuditEventType,
        tenant_id: uuid::Uuid,
        now_ms: u64,
        context: String,
    ) -> StripeAuditRecord {
        StripeAuditRecord {
            event_type,
            tenant_id: Some(tenant_id),
            now_ms,
            context,
        }
    }
}

impl<A, L> StripeBillingAdapter for InMemoryStripeBillingAdapter<A, L>
where
    A: StripeAuditSink + 'static,
    L: StripeUsageLedger + 'static,
{
    fn record_usage(
        &self,
        aggregate: &AggregatedCounter,
        subscription_item_id: SubscriptionItemId,
        now_ms: u64,
    ) -> Result<StripeAdapterDecision, StripeError> {
        // Canonicalize + derive the canonical idempotency key.
        let canonical = compute_canonical_aggregate_bytes(aggregate)?;
        let idempotency_key = derive_idempotency_key_from_canonical(&canonical);

        let tenant_id = aggregate.data.tenant_id;
        let total_qty = aggregate.data.total_qty;

        // Check the ledger BEFORE the audit so we can emit the
        // appropriate audit kind (usage_recorded vs duplicate_rejected).
        // The ledger.get is a pure read so observation does not mutate
        // state; the audit fires BEFORE the ledger.record state mutation
        // arm.
        let prior = self
            .ledger
            .get(&idempotency_key)
            .map_err(StripeError::from)?;

        let request = UsageRecordRequest {
            tenant_id,
            billing_period: aggregate.data.billing_period.clone(),
            event_kind: aggregate.data.event_kind.as_str().to_string(),
            total_qty,
            subscription_item_id: subscription_item_id.clone(),
            idempotency_key,
            now_ms,
        };

        if let Some(existing) = prior {
            if existing == request {
                // Duplicate-arm audit BEFORE returning the decision.
                let rec = Self::audit_record(
                    StripeAuditEventType::DuplicateRejected,
                    tenant_id,
                    now_ms,
                    format!(
                        "duplicate usage record idempotency_key={}",
                        idempotency_key.to_hex()
                    ),
                );
                self.audit.emit(rec)?;
                return Ok(StripeAdapterDecision::DuplicateRejected {
                    idempotency_key,
                    tenant_id,
                });
            }
            // Ledger holds a record at the same key but the recomputed
            // request payload diverges (canonical bytes regression).
            // Fall through to ledger.record which surfaces the
            // canonical IdempotencyKeyReuse arm.
        }

        // First-sight arm: emit usage_recorded BEFORE the ledger write.
        let rec = Self::audit_record(
            StripeAuditEventType::UsageRecorded,
            tenant_id,
            now_ms,
            format!(
                "recording usage idempotency_key={} subscription_item={} qty={}",
                idempotency_key.to_hex(),
                subscription_item_id,
                total_qty
            ),
        );
        self.audit.emit(rec)?;

        match self.ledger.record(&request)? {
            RecordOutcome::Recorded => Ok(StripeAdapterDecision::UsageRecorded {
                idempotency_key,
                subscription_item_id,
                tenant_id,
                total_qty,
            }),
            RecordOutcome::AlreadyExistsIdempotent => {
                // Defensive arm: ledger reported Already at the same
                // key with byte-identical payload; the prior-get check
                // above didn't catch it (e.g. concurrent mutation
                // between get + record). Treat as duplicate-arm.
                Ok(StripeAdapterDecision::DuplicateRejected {
                    idempotency_key,
                    tenant_id,
                })
            }
        }
    }

    fn get_recorded(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<UsageRecordRequest>, StripeError> {
        Ok(self.ledger.get(key)?)
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
    use crate::audit::{FailingStripeAuditSink, InMemoryStripeAuditSink};
    use crate::ledger::{FailingStripeUsageLedger, InMemoryStripeUsageLedger};
    use corelink_billing_aggregator::ChainHash;
    use corelink_billing_emit::{IdemKey, UsageEventKind};
    use uuid::Uuid;

    type Adapter = InMemoryStripeBillingAdapter<
        InMemoryStripeAuditSink,
        InMemoryStripeUsageLedger,
    >;

    fn fresh_adapter() -> (
        Adapter,
        Arc<InMemoryStripeAuditSink>,
        Arc<InMemoryStripeUsageLedger>,
    ) {
        let audit = Arc::new(InMemoryStripeAuditSink::new());
        let ledger = Arc::new(InMemoryStripeUsageLedger::new());
        let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
        (a, audit, ledger)
    }

    fn fresh_aggregate(
        tenant: Uuid,
        period: &str,
        kind: UsageEventKind,
        qty: u128,
    ) -> AggregatedCounter {
        AggregatedCounter::new(
            "corelink/region/iad/aggregator",
            Uuid::now_v7(),
            1_700_000_000_000,
            0,
            ChainHash::genesis(),
            tenant,
            period,
            kind,
            qty,
            1,
            0,
            1000,
            vec![IdemKey::genesis()],
        )
    }

    #[test]
    fn first_record_returns_usage_recorded_with_audit() {
        let (a, audit, ledger) = fresh_adapter();
        let tenant = Uuid::now_v7();
        let agg = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        let dec = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
            .unwrap();
        let recorded = matches!(dec, StripeAdapterDecision::UsageRecorded { .. });
        assert!(recorded);
        assert_eq!(ledger.len(), 1);
        assert_eq!(
            audit.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
            1
        );
    }

    #[test]
    fn duplicate_record_returns_duplicate_rejected_audit() {
        let (a, audit, ledger) = fresh_adapter();
        let tenant = Uuid::now_v7();
        let agg = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        let _ = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
            .unwrap();
        let dec2 = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
            .unwrap();
        let dup = matches!(dec2, StripeAdapterDecision::DuplicateRejected { .. });
        assert!(dup);
        assert_eq!(ledger.len(), 1);
        assert_eq!(
            audit.snapshot_of(StripeAuditEventType::DuplicateRejected).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
            1
        );
    }

    #[test]
    fn audit_failure_aborts_record_no_state_mutation() {
        let audit: Arc<FailingStripeAuditSink> = Arc::new(FailingStripeAuditSink::new());
        let ledger: Arc<InMemoryStripeUsageLedger> = Arc::new(InMemoryStripeUsageLedger::new());
        let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
        let tenant = Uuid::now_v7();
        let agg = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        let err = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
            .unwrap_err();
        assert!(matches!(err, StripeError::Audit(_)));
        // Ledger unchanged: audit fail-CLOSED.
        assert_eq!(ledger.len(), 0);
    }

    #[test]
    fn ledger_failure_propagates_after_audit() {
        let audit: Arc<InMemoryStripeAuditSink> = Arc::new(InMemoryStripeAuditSink::new());
        let ledger: Arc<FailingStripeUsageLedger> = Arc::new(FailingStripeUsageLedger::new());
        let a = InMemoryStripeBillingAdapter::new(Arc::clone(&audit), Arc::clone(&ledger));
        let tenant = Uuid::now_v7();
        let agg = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        let err = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1_700_000_000_000)
            .unwrap_err();
        assert!(matches!(err, StripeError::Ledger(_)));
        // Audit row landed BEFORE the ledger failure.
        assert_eq!(
            audit.snapshot_of(StripeAuditEventType::UsageRecorded).len(),
            1
        );
    }

    #[test]
    fn cross_tenant_idempotency_keys_distinct() {
        let (a, _audit, ledger) = fresh_adapter();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let agg1 = fresh_aggregate(t1, "2026-05", UsageEventKind::CasPut, 100);
        let agg2 = fresh_aggregate(t2, "2026-05", UsageEventKind::CasPut, 100);
        a.record_usage(&agg1, SubscriptionItemId::new("si_x"), 1).unwrap();
        a.record_usage(&agg2, SubscriptionItemId::new("si_y"), 1).unwrap();
        // Two distinct keys → two ledger entries.
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn cross_period_idempotency_keys_distinct() {
        let (a, _audit, ledger) = fresh_adapter();
        let tenant = Uuid::now_v7();
        let mut agg1 = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        agg1.id = Uuid::now_v7();
        let mut agg2 = fresh_aggregate(tenant, "2026-06", UsageEventKind::CasPut, 100);
        agg2.id = Uuid::now_v7();
        a.record_usage(&agg1, SubscriptionItemId::new("si_x"), 1).unwrap();
        a.record_usage(&agg2, SubscriptionItemId::new("si_x"), 1).unwrap();
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn get_recorded_returns_recorded_request() {
        let (a, _audit, _ledger) = fresh_adapter();
        let tenant = Uuid::now_v7();
        let agg = fresh_aggregate(tenant, "2026-05", UsageEventKind::CasPut, 100);
        let dec = a
            .record_usage(&agg, SubscriptionItemId::new("si_x"), 1)
            .unwrap();
        let key = match dec {
            StripeAdapterDecision::UsageRecorded { idempotency_key, .. } => idempotency_key,
            other => unreachable!("{other:?}"),
        };
        let got = a.get_recorded(&key).unwrap().unwrap();
        assert_eq!(got.tenant_id, tenant);
        assert_eq!(got.total_qty, 100);
    }

    #[test]
    fn get_recorded_returns_none_for_unknown_key() {
        let (a, _audit, _ledger) = fresh_adapter();
        let key = IdempotencyKey([0xFF; 32]);
        assert!(a.get_recorded(&key).unwrap().is_none());
    }
}
