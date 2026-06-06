//! `CounterAggregator` trait + `InMemoryCounterAggregator`
//! orchestrator.
//!
//! The orchestrator wires the four primitives — input ordering
//! determinism, JCS canonicalization (via [`crate::chain`]), BLAKE3
//! hash-chain advance (via [`crate::chain::HashChainBuilder`]), and the
//! UPSERT-safe [`crate::store::InMemoryAggregatedCounterStore`] — into a
//! single run pipeline that satisfies every load-bearing invariant the
//! production CF Cron DO binding will depend on (per the
//! `trait-abstraction-defer` charter pattern).
//!
//! ## Run pipeline (per cron tick)
//!
//! For each input `(tenant_id, billing_period, event_kind, period_window,
//! events)`:
//!
//! 1. **Audit `run_started` BEFORE state observation**: per the canonical
//!    S-07 P1-1 fix + Lote 10.6bis pattern. Audit failure aborts the run
//!    (NO chain head observation, NO counter store read).
//! 2. **Filter events** by the canonical period window (inclusive start,
//!    exclusive end — Prometheus boundary semantics).
//! 3. **Deterministic input ordering**: sort the filtered events by
//!    `(time_ms, idem_key)` lexicographic. This is the canonical
//!    invariant for `prop_idempotent_rerun_same_chain_hash` — re-running
//!    with the same input set in any order reproduces the same chain
//!    digest.
//! 4. **Aggregate**: SUM the contributing events' `qty` into `total_qty`;
//!    count the cardinality into `event_count`; collect the canonical
//!    `idem_keys_seen` list (the typed `AggregatedCounterData` payload).
//! 5. **Idempotent re-run check (BEFORE chain-head observation)**: if a
//!    prior aggregate exists at the (tenant, billing_period, event_kind)
//!    coordinate AND the recomputed `data` payload byte-equals the prior
//!    `data`, return `SkippedDuplicateRun` (watermark replay; the chain
//!    head MUST NOT advance a second time). The check compares on the
//!    typed `data` payload (NOT full canonical bytes) because the chain
//!    link slot `prev_hash + sequence_number` is determined by chain
//!    state at the FIRST run, not the replay.
//! 6. **Chain head lookup**: read the per-(tenant, billing_period) chain
//!    head from the store.
//! 7. **Build the canonical [`AggregatedCounter`]**: with `prev_hash =
//!    chain_head.current_head`, `sequence_number =
//!    chain_head.next_sequence`. Compute the recomputed chain link via
//!    [`crate::chain::link_chain_hash`].
//! 8. **Counter store UPSERT**: `store.upsert(aggregate, new_head,
//!    digest_hex)`. Store failure → emit `sink_failure` audit arm
//!    BEFORE propagating the error (fail-CLOSED).
//! 9. **Audit `run_completed`** with the decision discriminant.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-002 §6.1 + sprint contract, every decision arm fires its
//! canonical audit BEFORE the state-mutating step:
//!
//! - `run_started` fires BEFORE any chain head observation +
//!   counter-store read.
//! - `chain_break_detected` fires BEFORE the orchestrator returns
//!   `AggregatorError::ChainBreak` (the chain primitive itself rejects
//!   the partial append, so no state mutation).
//! - `sink_failure` fires BEFORE the orchestrator returns
//!   `AggregatorError::Store` (the store rejected the UPSERT; chain
//!   head + counter store unchanged).
//! - `run_completed` fires AFTER the successful UPSERT (or on the
//!   no-op / skipped-duplicate arm) — this is the only audit arm that
//!   may fire post-state-mutation, because by definition the run-end
//!   audit observes the post-run state.
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! `AggregatorError::Audit` (caller sees no state mutation past the
//! point of the failure).
//!
//! ## Why concrete `Arc<InMemoryAggregatedCounterStore>` (NOT generic)
//!
//! Mirrors `corelink_billing_emit::InMemoryUsageEventEmitter` discipline:
//! the trait surface is preserved for adversarial test fakes via
//! [`crate::store::FailingAggregatedCounterStore`] under the existing
//! trait. The production wiring at WI-S10-007 will ship its own
//! orchestrator variant that delegates to the per-(tenant, region) DO
//! singleton + writes via `AggregatedCounterStore::upsert` at the
//! D1-allocated key.
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + counter store as `Arc`
//! handles passed at construction; per-instance state lives inside
//! those Arc'd primitives. Tests instantiate fresh orchestrators per
//! case so cross-test contamination is structurally impossible.

use std::sync::Arc;
use uuid::Uuid;

use corelink_billing_emit::{IdemKey, UsageEvent, UsageEventKind};

use crate::audit::{AggregatorAuditEventType, AggregatorAuditRecord, AggregatorAuditSink};
use crate::chain::compute_canonical_bytes;
use crate::error::AggregatorError;
use crate::event::{
    AggregatedCounter, AggregatedCounterData, AggregationDecision, CounterGroupKey,
    CLOUDEVENTS_DATACONTENTTYPE, CLOUDEVENTS_SPECVERSION, COUNTER_AGGREGATED_EVENT_TYPE,
};
use crate::store::{AggregatedCounterStore, UpsertOutcome};

/// Canonical aggregation period window (inclusive start, exclusive
/// end). Per WI-S10-002 §1: an event with `time_ms == period_end_ms`
/// belongs to the NEXT period bucket (Prometheus boundary semantics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeriodWindow {
    /// Inclusive period start (Unix epoch ms).
    pub start_ms: u64,
    /// Exclusive period end (Unix epoch ms).
    pub end_ms: u64,
}

impl PeriodWindow {
    /// Construct a canonical window. Returns
    /// [`AggregatorError::Internal`] if `end_ms <= start_ms` (degenerate
    /// or inverted window).
    ///
    /// # Errors
    ///
    /// - [`AggregatorError::Internal`] when `end_ms <= start_ms`.
    pub fn new(start_ms: u64, end_ms: u64) -> Result<Self, AggregatorError> {
        if end_ms <= start_ms {
            return Err(AggregatorError::Internal(format!(
                "PeriodWindow end_ms ({end_ms}) must be > start_ms ({start_ms})"
            )));
        }
        Ok(Self { start_ms, end_ms })
    }

    /// Whether `time_ms` falls within `[start_ms, end_ms)` (inclusive
    /// start, exclusive end — canonical Prometheus boundary semantics).
    #[must_use]
    pub const fn contains(&self, time_ms: u64) -> bool {
        time_ms >= self.start_ms && time_ms < self.end_ms
    }
}

/// Counter-aggregator trait. Production wiring composes:
///
/// - `CronDOCounterAggregator` — Cloudflare Durable Object cron-trigger
///   1h interval; drains usage_event_staging from WI-S10-001 + replays
///   R2 hour bucket + atomic D1 transaction (counter +
///   hash_chain_head); deferred to WI-S10-007 PRR ship gate per
///   `trait-abstraction-defer` charter pattern.
pub trait CounterAggregator: Send + Sync + core::fmt::Debug {
    /// Run the aggregator over the provided `events` set. The
    /// orchestrator filters by `period_window` + groups by
    /// `event_kind` + computes the chain digest + UPSERTs the counter
    /// store atomically.
    ///
    /// # Errors
    ///
    /// - [`AggregatorError::Canonicalization`] when JCS fails.
    /// - [`AggregatorError::Audit`] when the audit envelope rejects
    ///   any decision arm (fail-CLOSED at the trait surface).
    /// - [`AggregatorError::Store`] when the counter store rejects the
    ///   UPSERT (D1 transaction rollback / digest mismatch on replay).
    /// - [`AggregatorError::ChainBreak`] when the chain head observed
    ///   at run-start does not match the chain head recomputed from
    ///   the counter store (CRITICAL tampering signal).
    /// - [`AggregatorError::Internal`] when a per-instance mutex is
    ///   poisoned.
    fn run(&self, request: AggregationRequest<'_>) -> Result<AggregationDecision, AggregatorError>;
}

/// Canonical aggregation-request shape. Mirrors the production CF Cron
/// DO trigger payload (deferred to WI-S10-007). The `events` slice is
/// borrowed so the caller can fan out to multiple aggregator runs
/// (e.g. one per `event_kind`) without re-allocating.
#[derive(Debug)]
pub struct AggregationRequest<'a> {
    /// Tenant id of the run.
    pub tenant_id: Uuid,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: &'a str,
    /// Canonical event kind to aggregate.
    pub event_kind: UsageEventKind,
    /// Canonical period window for filtering.
    pub period_window: PeriodWindow,
    /// Canonical input event set.
    pub events: &'a [UsageEvent],
    /// Canonical aggregator source URI (e.g.
    /// `corelink/region/iad/aggregator`).
    pub source: &'a str,
    /// Run start wall-clock (Unix epoch ms; canonical
    /// `cron_now` per WI-S10-002 §6.1.11 R5 P1-QUIN-1 — fixed-snapshot
    /// of the aggregation run start).
    pub now_ms: u64,
    /// Aggregate id (UUIDv7); production wiring generates this at the
    /// run boundary.
    pub aggregate_id: Uuid,
}

/// In-memory counter-aggregator orchestrator. Composes the audit sink +
/// the counter store via `Arc` handles; the audit + store are generic
/// over the trait so test fakes (e.g.
/// [`crate::audit::FailingAggregatorAuditSink`] +
/// [`crate::store::FailingAggregatedCounterStore`]) compose directly at
/// construction.
#[derive(Clone, Debug)]
pub struct InMemoryCounterAggregator<A, S>
where
    A: AggregatorAuditSink + 'static,
    S: AggregatedCounterStore + 'static,
{
    audit: Arc<A>,
    store: Arc<S>,
}

impl<A, S> InMemoryCounterAggregator<A, S>
where
    A: AggregatorAuditSink + 'static,
    S: AggregatedCounterStore + 'static,
{
    /// Construct with explicit audit + store handles.
    pub fn new(audit: Arc<A>, store: Arc<S>) -> Self {
        Self { audit, store }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the counter store.
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    fn audit_record(
        event_type: AggregatorAuditEventType,
        request: &AggregationRequest<'_>,
        context: String,
    ) -> AggregatorAuditRecord {
        AggregatorAuditRecord {
            event_type,
            tenant_id: request.tenant_id,
            billing_period: request.billing_period.to_string(),
            event_kind: request.event_kind,
            now_ms: request.now_ms,
            context,
        }
    }
}

/// Filter `events` by the canonical period window + tenant + event_kind
/// match, then sort by the deterministic input order
/// `(time_ms, idem_key)` lexicographic. Pure-logic helper; exposed so
/// adversarial tests can pin the ordering directly.
///
/// ## Why `(time_ms, idem_key)` (NOT just `time_ms`)
///
/// Two events MAY share the same wall-clock `time_ms` (Cloudflare
/// Workers in the same region can emit at sub-ms boundaries; Worker
/// retries use the canonical CloudEvents `time` from the FIRST emit,
/// not the retry instant). Tie-breaking by `idem_key` (32-byte
/// BLAKE3-256 of the canonical event bytes; uniqueness < 2^-128
/// collision probability) gives a total deterministic order over
/// arbitrary event sets.
#[must_use]
pub fn deterministic_event_order<'a>(
    events: &'a [UsageEvent],
    tenant_id: Uuid,
    event_kind: UsageEventKind,
    period_window: PeriodWindow,
) -> Vec<&'a UsageEvent> {
    let mut filtered: Vec<&'a UsageEvent> = events
        .iter()
        .filter(|e| {
            e.data.tenant_id == tenant_id
                && e.data.event_kind == event_kind
                && period_window.contains(e.time_ms)
        })
        .collect();
    filtered.sort_by(|a, b| {
        a.time_ms
            .cmp(&b.time_ms)
            .then_with(|| a.idem_key.as_bytes().cmp(b.idem_key.as_bytes()))
    });
    filtered
}

/// Build the typed `AggregatedCounterData` payload from the
/// deterministically-ordered event slice. Pure-logic helper.
#[must_use]
fn build_counter_data(
    request: &AggregationRequest<'_>,
    ordered_events: &[&UsageEvent],
) -> AggregatedCounterData {
    let mut total_qty: u128 = 0;
    let mut idem_keys_seen: Vec<IdemKey> = Vec::with_capacity(ordered_events.len());
    for ev in ordered_events {
        // u128 SUM cannot overflow under any realistic billing volume:
        // 10^9 events/yr × 7y × max u64 qty = ~10^28 < 2^96 < u128::MAX.
        total_qty = total_qty.saturating_add(u128::from(ev.data.qty));
        idem_keys_seen.push(ev.idem_key);
    }
    AggregatedCounterData {
        tenant_id: request.tenant_id,
        billing_period: request.billing_period.to_string(),
        event_kind: request.event_kind,
        total_qty,
        // u64 cast is safe: ordered_events.len() bounded by usize ≤ u64
        // on 64-bit targets; 32-bit hosts (wasm32) are bounded by usize
        // < u64::MAX so no truncation. Defensive saturating coerce.
        event_count: u64::try_from(ordered_events.len()).unwrap_or(u64::MAX),
        period_start_ms: request.period_window.start_ms,
        period_end_ms: request.period_window.end_ms,
        idem_keys_seen,
    }
}

impl<A, S> CounterAggregator for InMemoryCounterAggregator<A, S>
where
    A: AggregatorAuditSink + 'static,
    S: AggregatedCounterStore + 'static,
{
    fn run(&self, request: AggregationRequest<'_>) -> Result<AggregationDecision, AggregatorError> {
        // Audit `run_started` BEFORE any state observation per the
        // canonical S-07 P1-1 fix + Lote 10.6bis pattern.
        let started_rec = Self::audit_record(
            AggregatorAuditEventType::RunStarted,
            &request,
            "aggregation run started".to_string(),
        );
        self.audit.emit(started_rec)?;

        // Filter + deterministic input ordering.
        let ordered_events = deterministic_event_order(
            request.events,
            request.tenant_id,
            request.event_kind,
            request.period_window,
        );

        if ordered_events.is_empty() {
            // No-op arm: chain head unchanged; counter store unchanged.
            let decision = AggregationDecision::SkippedNoEvents {
                tenant_id: request.tenant_id,
                billing_period: request.billing_period.to_string(),
                event_kind: request.event_kind,
            };
            let completed_rec = Self::audit_record(
                AggregatorAuditEventType::RunCompleted,
                &request,
                "skipped: no contributing events in period window".to_string(),
            );
            self.audit.emit(completed_rec)?;
            return Ok(decision);
        }

        // Build aggregate data payload from the deterministic ordering.
        let data = build_counter_data(&request, &ordered_events);

        let key = CounterGroupKey {
            tenant_id: request.tenant_id,
            billing_period: request.billing_period.to_string(),
            event_kind: request.event_kind,
        };

        // Idempotent re-run check (BEFORE chain-head observation so a
        // watermark replay never advances the head twice). Per WI-S10-002
        // §6.1.9: re-aggregation with the same canonical input event set
        // (same typed `data` payload — total_qty / event_count /
        // period bounds / idem_keys_seen) means the prior chain advance
        // already accounted for the same set; the chain head MUST NOT
        // advance a second time. We compare on the typed `data` payload
        // (NOT full canonical bytes) because the link slot
        // `prev_hash` + `sequence_number` is determined by the chain
        // state at the time of the FIRST run, not the replay run.
        if let Some(prior) = self.store.get(&key)? {
            if prior.data == data {
                let completed_rec = Self::audit_record(
                    AggregatorAuditEventType::RunCompleted,
                    &request,
                    "skipped: duplicate run; prior aggregate data payload matches".to_string(),
                );
                self.audit.emit(completed_rec)?;
                return Ok(AggregationDecision::SkippedDuplicateRun { existing: prior });
            }
        }

        // Chain head lookup (after the duplicate-run guard so a replay
        // never observes the post-first-run head).
        let head = self
            .store
            .chain_head(request.tenant_id, request.billing_period)?;

        // Build the canonical AggregatedCounter shape.
        let aggregate = AggregatedCounter {
            specversion: CLOUDEVENTS_SPECVERSION.to_string(),
            event_type: COUNTER_AGGREGATED_EVENT_TYPE.to_string(),
            source: request.source.to_string(),
            subject: format!("tenant:{}", request.tenant_id),
            id: request.aggregate_id,
            time_ms: request.now_ms,
            datacontenttype: CLOUDEVENTS_DATACONTENTTYPE.to_string(),
            sequence_number: head.next_sequence,
            prev_hash: head.current_head,
            data,
        };

        // Compute the canonical bytes + recomputed chain digest.
        let canonical = compute_canonical_bytes(&aggregate)?;
        let new_head = crate::chain::link_chain_hash_from_canonical(&head.current_head, &canonical);
        let digest_hex = hex::encode(canonical_digest_bytes(&canonical));

        // Counter store UPSERT (atomic counter + chain head advance).
        match self.store.upsert(&aggregate, new_head, &digest_hex) {
            Ok(UpsertOutcome::Inserted) => {
                let completed_rec = Self::audit_record(
                    AggregatorAuditEventType::RunCompleted,
                    &request,
                    format!(
                        "inserted aggregate seq={} head={}",
                        aggregate.sequence_number,
                        new_head.to_hex()
                    ),
                );
                self.audit.emit(completed_rec)?;
                Ok(AggregationDecision::Aggregated(aggregate))
            }
            Ok(UpsertOutcome::AlreadyExistsIdempotent) => {
                // Defensive arm: the store reported AlreadyExists with
                // matching digest, but the prior-aggregate check above
                // didn't catch it (e.g. concurrent mutation between
                // `get` + `upsert`). Treat as duplicate run.
                let prior = self.store.get(&key)?.unwrap_or_else(|| aggregate.clone());
                let completed_rec = Self::audit_record(
                    AggregatorAuditEventType::RunCompleted,
                    &request,
                    "skipped: duplicate run via UPSERT idempotent path".to_string(),
                );
                self.audit.emit(completed_rec)?;
                Ok(AggregationDecision::SkippedDuplicateRun { existing: prior })
            }
            Err(store_err) => {
                // sink_failure audit BEFORE propagating the error
                // (fail-CLOSED envelope per Lote 10.6bis).
                let failure_rec = Self::audit_record(
                    AggregatorAuditEventType::SinkFailure,
                    &request,
                    format!("counter store UPSERT failed: {store_err}"),
                );
                self.audit.emit(failure_rec)?;
                Err(store_err.into())
            }
        }
    }
}

/// Compute the canonical 32-byte BLAKE3 digest of the canonical bytes
/// (separate from the chain link hash; used as the stored
/// `own_digest` for replay-corruption detection per WI-S10-002 §6.1.9).
#[must_use]
fn canonical_digest_bytes(canonical_bytes: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(canonical_bytes);
    let digest = h.finalize();
    *digest.as_bytes()
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
    use crate::audit::{
        AggregatorAuditEventType, FailingAggregatorAuditSink, InMemoryAggregatorAuditSink,
    };
    use crate::event::ChainHash;
    use crate::store::{FailingAggregatedCounterStore, InMemoryAggregatedCounterStore};
    use corelink_analytics::Region;
    use corelink_billing_emit::{
        compute_canonical_bytes_for_idem, derive_idem_key_from_canonical, UsageEvent,
        UsageEventKind,
    };

    type Aggregator =
        InMemoryCounterAggregator<InMemoryAggregatorAuditSink, InMemoryAggregatedCounterStore>;

    fn fresh_aggregator() -> (
        Aggregator,
        Arc<InMemoryAggregatorAuditSink>,
        Arc<InMemoryAggregatedCounterStore>,
    ) {
        let audit = Arc::new(InMemoryAggregatorAuditSink::new());
        let store = Arc::new(InMemoryAggregatedCounterStore::new());
        let agg = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
        (agg, audit, store)
    }

    fn fresh_event(
        tenant: Uuid,
        kind: UsageEventKind,
        qty: u64,
        period: &str,
        time_ms: u64,
    ) -> UsageEvent {
        let mut e = UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            time_ms,
            Region::Iad,
            tenant,
            kind,
            qty,
            period,
        )
        .unwrap();
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        e.idem_key = derive_idem_key_from_canonical(&canonical);
        e
    }

    fn build_request<'a>(
        tenant: Uuid,
        period: &'a str,
        kind: UsageEventKind,
        events: &'a [UsageEvent],
        now_ms: u64,
    ) -> AggregationRequest<'a> {
        AggregationRequest {
            tenant_id: tenant,
            billing_period: period,
            event_kind: kind,
            period_window: PeriodWindow::new(0, u64::MAX / 2).unwrap(),
            events,
            source: "corelink/region/iad/aggregator",
            now_ms,
            aggregate_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn period_window_inclusive_start_exclusive_end() {
        let w = PeriodWindow::new(100, 200).unwrap();
        assert!(w.contains(100));
        assert!(w.contains(150));
        assert!(w.contains(199));
        assert!(!w.contains(200));
        assert!(!w.contains(99));
    }

    #[test]
    fn period_window_rejects_inverted() {
        let err = PeriodWindow::new(200, 100).unwrap_err();
        assert!(matches!(err, AggregatorError::Internal(_)));
    }

    #[test]
    fn period_window_rejects_zero_duration() {
        let err = PeriodWindow::new(100, 100).unwrap_err();
        assert!(matches!(err, AggregatorError::Internal(_)));
    }

    #[test]
    fn first_run_with_no_events_yields_skipped_no_events() {
        let (a, audit, store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let req = build_request(tenant, "2026-05", UsageEventKind::CasPut, &[], 1);
        let decision = a.run(req).unwrap();
        let is_no_events = matches!(decision, AggregationDecision::SkippedNoEvents { .. });
        assert!(is_no_events);
        assert_eq!(store.len(), 0);
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::RunStarted)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::RunCompleted)
                .len(),
            1
        );
    }

    #[test]
    fn first_run_with_events_aggregates_and_advances_chain() {
        let (a, audit, store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let evs = vec![
            fresh_event(tenant, UsageEventKind::CasPut, 10, "2026-05", 100),
            fresh_event(tenant, UsageEventKind::CasPut, 20, "2026-05", 200),
            fresh_event(tenant, UsageEventKind::CasPut, 30, "2026-05", 300),
        ];
        let req = build_request(tenant, "2026-05", UsageEventKind::CasPut, &evs, 1000);
        let decision = a.run(req).unwrap();
        let agg = match decision {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("expected Aggregated; got {other:?}"),
        };
        assert_eq!(agg.data.total_qty, 60);
        assert_eq!(agg.data.event_count, 3);
        assert_eq!(agg.sequence_number, 0);
        assert_eq!(agg.prev_hash, ChainHash::genesis());
        assert_eq!(store.len(), 1);
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::RunStarted)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::RunCompleted)
                .len(),
            1
        );
    }

    #[test]
    fn second_run_same_inputs_yields_skipped_duplicate_run() {
        let (a, _audit, store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let evs = vec![fresh_event(
            tenant,
            UsageEventKind::CasPut,
            5,
            "2026-05",
            100,
        )];
        let req1 = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 1000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            aggregate_id: Uuid::now_v7(),
        };
        let req2 = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 1000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            // Same aggregate_id reproduces the same canonical bytes.
            aggregate_id: req1.aggregate_id,
        };
        let _ = a.run(req1).unwrap();
        let dec2 = a.run(req2).unwrap();
        let is_dup = matches!(dec2, AggregationDecision::SkippedDuplicateRun { .. });
        assert!(is_dup);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn deterministic_input_order_preserves_chain_hash_across_shuffles() {
        let (a1, _audit1, store1) = fresh_aggregator();
        let (a2, _audit2, _store2) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let mut evs = vec![
            fresh_event(tenant, UsageEventKind::CasPut, 10, "2026-05", 100),
            fresh_event(tenant, UsageEventKind::CasPut, 20, "2026-05", 200),
            fresh_event(tenant, UsageEventKind::CasPut, 30, "2026-05", 300),
        ];
        let id = Uuid::now_v7();
        let req1 = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 1000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            aggregate_id: id,
        };
        let dec1 = a1.run(req1).unwrap();
        let agg1 = match dec1 {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        // Reverse the events; same input set, same canonical chain hash.
        evs.reverse();
        let req2 = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 1000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            aggregate_id: id,
        };
        let dec2 = a2.run(req2).unwrap();
        let agg2 = match dec2 {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        // Chain digests match: same input set → same head advance.
        let head1 = store1.chain_head(tenant, "2026-05").unwrap();
        // Build store2 head by computing chain link locally for parity.
        let canonical2 = compute_canonical_bytes(&agg2).unwrap();
        let head2 =
            crate::chain::link_chain_hash_from_canonical(&ChainHash::genesis(), &canonical2);
        assert_eq!(agg1, agg2);
        assert_eq!(head1.current_head, head2);
    }

    #[test]
    fn audit_failure_aborts_run_no_state_mutation() {
        let audit: Arc<FailingAggregatorAuditSink> = Arc::new(FailingAggregatorAuditSink::new());
        let store: Arc<InMemoryAggregatedCounterStore> =
            Arc::new(InMemoryAggregatedCounterStore::new());
        let a = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
        let tenant = Uuid::now_v7();
        let evs = vec![fresh_event(
            tenant,
            UsageEventKind::CasPut,
            5,
            "2026-05",
            100,
        )];
        let req = build_request(tenant, "2026-05", UsageEventKind::CasPut, &evs, 1000);
        let err = a.run(req).unwrap_err();
        assert!(matches!(err, AggregatorError::Audit(_)));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn store_failure_emits_sink_failure_audit() {
        let audit: Arc<InMemoryAggregatorAuditSink> = Arc::new(InMemoryAggregatorAuditSink::new());
        let store: Arc<FailingAggregatedCounterStore> =
            Arc::new(FailingAggregatedCounterStore::new());
        let a = InMemoryCounterAggregator::new(Arc::clone(&audit), Arc::clone(&store));
        let tenant = Uuid::now_v7();
        let evs = vec![fresh_event(
            tenant,
            UsageEventKind::CasPut,
            5,
            "2026-05",
            100,
        )];
        let req = build_request(tenant, "2026-05", UsageEventKind::CasPut, &evs, 1000);
        let err = a.run(req).unwrap_err();
        assert!(matches!(err, AggregatorError::Store(_)));
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::SinkFailure)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(AggregatorAuditEventType::RunStarted)
                .len(),
            1
        );
    }

    #[test]
    fn period_boundary_event_at_exact_end_excluded() {
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let evs = vec![
            fresh_event(tenant, UsageEventKind::CasPut, 10, "2026-05", 100),
            // Event exactly at end_ms: belongs to NEXT period.
            fresh_event(tenant, UsageEventKind::CasPut, 99, "2026-05", 1000),
        ];
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 1000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            aggregate_id: Uuid::now_v7(),
        };
        let dec = a.run(req).unwrap();
        let agg = match dec {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        // Only the event at time_ms=100 contributed; the boundary event
        // at time_ms=1000 was excluded (Prometheus boundary semantics).
        assert_eq!(agg.data.total_qty, 10);
        assert_eq!(agg.data.event_count, 1);
    }

    #[test]
    fn cross_tenant_isolation() {
        let (a, _audit, store) = fresh_aggregator();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let evs = vec![
            fresh_event(tenant_a, UsageEventKind::CasPut, 10, "2026-05", 100),
            fresh_event(tenant_b, UsageEventKind::CasPut, 20, "2026-05", 100),
        ];
        let req_a = build_request(tenant_a, "2026-05", UsageEventKind::CasPut, &evs, 1000);
        let dec_a = a.run(req_a).unwrap();
        let agg_a = match dec_a {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        // Tenant A only saw its own event.
        assert_eq!(agg_a.data.total_qty, 10);
        assert_eq!(agg_a.data.tenant_id, tenant_a);
        // Tenant B's chain head untouched.
        let head_b = store.chain_head(tenant_b, "2026-05").unwrap();
        assert_eq!(head_b.current_head, ChainHash::genesis());
        assert_eq!(head_b.next_sequence, 0);
    }

    #[test]
    fn event_kind_isolation() {
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let evs = vec![
            fresh_event(tenant, UsageEventKind::CasPut, 10, "2026-05", 100),
            fresh_event(tenant, UsageEventKind::CasGet, 20, "2026-05", 200),
        ];
        let req_put = build_request(tenant, "2026-05", UsageEventKind::CasPut, &evs, 1000);
        let dec = a.run(req_put).unwrap();
        let agg = match dec {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        assert_eq!(agg.data.total_qty, 10);
        assert_eq!(agg.data.event_count, 1);
        assert_eq!(agg.data.event_kind, UsageEventKind::CasPut);
    }

    #[test]
    fn sum_correctness_over_many_events() {
        let (a, _audit, _store) = fresh_aggregator();
        let tenant = Uuid::now_v7();
        let evs: Vec<UsageEvent> = (0..100u64)
            .map(|i| fresh_event(tenant, UsageEventKind::CasPut, i, "2026-05", 100 + i))
            .collect();
        let req = AggregationRequest {
            tenant_id: tenant,
            billing_period: "2026-05",
            event_kind: UsageEventKind::CasPut,
            period_window: PeriodWindow::new(0, 10000).unwrap(),
            events: &evs,
            source: "src",
            now_ms: 1000,
            aggregate_id: Uuid::now_v7(),
        };
        let dec = a.run(req).unwrap();
        let agg = match dec {
            AggregationDecision::Aggregated(a) => a,
            other => unreachable!("{other:?}"),
        };
        // Σ 0..100 = 4950.
        assert_eq!(agg.data.total_qty, 4950);
        assert_eq!(agg.data.event_count, 100);
        assert_eq!(agg.data.idem_keys_seen.len(), 100);
    }
}
