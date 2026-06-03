//! `AggregatedCounterStore` trait + `InMemoryAggregatedCounterStore`
//! UPSERT-safe ledger.
//!
//! ## D1 atomic transaction (production wiring at WI-S10-007)
//!
//! Per WI-S10-002 §6.1.8, the production wiring binds this trait to a
//! D1 atomic batch:
//!
//! ```text
//! INSERT INTO usage_counter (tenant_id, region, sku, hour, qty_bytes,
//!                            qty_ops, event_count, aggregated_at,
//!                            prev_hash, own_digest, schema_version)
//! VALUES (...) ON CONFLICT (tenant_id, region, sku, hour) DO UPDATE
//! SET ...;
//! UPDATE hash_chain_head SET current_head = ?, last_aggregated_hour = ?,
//!                            updated_at = ?
//! WHERE region = ? AND chain_kind = 'usage_counter';
//! ```
//!
//! Both writes land in the same `db.batch` so a transient D1 error
//! rolls BOTH back (atomic). The in-memory fake here pins the same
//! atomic semantics at the trait surface so adversarial tests can
//! falsify INV-BILLING-NO-LOSS Layer 1 + INV-BILLING-NO-DUP
//! independently of the production D1 binding (deferred to WI-S10-007
//! PRR ship gate per `trait-abstraction-defer` charter pattern).
//!
//! ## INV-BILLING-NO-DUP (HIGH; invariant_registry §3.9 line 137)
//!
//! PRIMARY KEY `(tenant_id, billing_period, event_kind)` UNIQUE in the
//! in-memory store + UPSERT semantics: re-aggregation of the same
//! period reproduces the same digest (deterministic input ordering;
//! WI-S10-002 §1 invariant 9). The store rejects digest divergence
//! with `AggregatedCounterStoreError::DigestMismatch` (replay
//! corruption SEV-1 source).
//!
//! ## INV-BILLING-NO-LOSS Layer 1 (HIGH; invariant_registry §3.9 line 136)
//!
//! Σ(R2 events qty) = Σ(stored aggregate `total_qty`) per (tenant,
//! billing_period, event_kind). The store does NOT enforce the SUM
//! correspondence directly — that's the orchestrator + reconciliation
//! worker (WI-S10-004) Layer 1 concern; the store enforces APPEND-ONLY
//! (no DELETE / arbitrary UPDATE) so the Σ correspondence is
//! verifiable downstream.
//!
//! ## F-001 closure
//!
//! The store holds the per-`CounterGroupKey` ledger + the per-(tenant,
//! billing_period) chain-head map under a per-instance `Arc<Mutex<>>`
//! (NOT a `static LazyLock`). Tests instantiate fresh stores per case
//! so the orchestrator harness cannot accidentally leak chain state
//! across cases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::error::AggregatedCounterStoreError;
use crate::event::{AggregatedCounter, ChainHash, CounterGroupKey};

/// Per-(tenant, billing_period) chain-head record (production wiring at
/// the D1 `hash_chain_head` table; in-memory fake here pins the
/// canonical resume coordinate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainHeadRecord {
    /// Current chain head (the `prev_hash` slot of the next aggregate
    /// to be appended).
    pub current_head: ChainHash,
    /// Sequence number of the next aggregate to be appended.
    pub next_sequence: u64,
}

impl ChainHeadRecord {
    /// Canonical genesis head: `current_head == [0u8; 32]`,
    /// `next_sequence == 0`.
    #[must_use]
    pub const fn genesis() -> Self {
        Self {
            current_head: ChainHash::genesis(),
            next_sequence: 0,
        }
    }
}

impl Default for ChainHeadRecord {
    fn default() -> Self {
        Self::genesis()
    }
}

/// Counter-store trait. Production wiring composes:
///
/// - `D1AggregatedCounterStore` — D1 atomic batch UPSERT (counter row
///   plus `hash_chain_head` UPDATE in the same `db.batch`); deferred
///   to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter
///   pattern.
pub trait AggregatedCounterStore: Send + Sync + core::fmt::Debug {
    /// Persist `aggregate` durably + atomically advance the per-(tenant,
    /// billing_period) chain head to the recomputed digest. Production
    /// wiring fans out to D1 atomic batch.
    ///
    /// ## UPSERT semantics
    ///
    /// - First sight at `(tenant, billing_period, event_kind)` →
    ///   INSERT; chain head advances; return `UpsertOutcome::Inserted`.
    /// - Re-aggregation with byte-identical canonical bytes →
    ///   idempotent UPSERT (the stored row is overwritten with the
    ///   recomputed aggregate; chain head advances per-call so the
    ///   CALLER deduplicates upstream via the chain-head lookup);
    ///   return `UpsertOutcome::AlreadyExistsIdempotent`.
    /// - Re-aggregation with diverged canonical bytes →
    ///   `AggregatedCounterStoreError::DigestMismatch` (replay
    ///   corruption SEV-1 source).
    ///
    /// # Errors
    ///
    /// Returns [`AggregatedCounterStoreError::Backend`] on any backend
    /// failure + [`AggregatedCounterStoreError::DigestMismatch`] on
    /// replay-corruption divergence.
    fn upsert(
        &self,
        aggregate: &AggregatedCounter,
        new_chain_head: ChainHash,
        recomputed_digest_hex: &str,
    ) -> Result<UpsertOutcome, AggregatedCounterStoreError>;

    /// Look up the per-(tenant, billing_period) chain head record.
    /// Returns the canonical genesis record (head = `[0u8; 32]`,
    /// next_sequence = 0) if the chain has no aggregates yet.
    fn chain_head(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<ChainHeadRecord, AggregatedCounterStoreError>;

    /// Look up an existing aggregate by its canonical group key (the
    /// 3-tuple `(tenant_id, billing_period, event_kind)`). Returns
    /// `None` if no aggregate has been recorded for that coordinate.
    fn get(
        &self,
        key: &CounterGroupKey,
    ) -> Result<Option<AggregatedCounter>, AggregatedCounterStoreError>;
}

/// Outcome of [`AggregatedCounterStore::upsert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpsertOutcome {
    /// First sight at the canonical group key: a fresh row was
    /// inserted + the chain advanced.
    Inserted,
    /// Re-aggregation with byte-identical canonical bytes: the stored
    /// row was overwritten in place (idempotent UPSERT) + the chain
    /// advance was skipped (caller's responsibility to dedup via
    /// `chain_head()` lookup).
    AlreadyExistsIdempotent,
}

/// In-memory counter store. Per-instance `Arc<Mutex<>>` per the F-001
/// closure discipline; tests instantiate fresh stores per case so
/// cross-test contamination is structurally impossible.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAggregatedCounterStore {
    state: Arc<Mutex<StoreState>>,
}

#[derive(Debug, Default)]
struct StoreState {
    // Per-(tenant, billing_period, event_kind) ledger.
    rows: HashMap<CounterGroupKey, StoredAggregate>,
    // Per-(tenant, billing_period) chain head map.
    heads: HashMap<(Uuid, String), ChainHeadRecord>,
}

#[derive(Clone, Debug)]
struct StoredAggregate {
    aggregate: AggregatedCounter,
    digest_hex: String,
}

impl InMemoryAggregatedCounterStore {
    /// Construct a fresh store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every stored aggregate (chronologically ordered by
    /// `(tenant_id, billing_period, sequence_number)`).
    #[must_use]
    pub fn snapshot_aggregates(&self) -> Vec<AggregatedCounter> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<AggregatedCounter> =
            g.rows.values().map(|s| s.aggregate.clone()).collect();
        out.sort_by(|a, b| {
            a.data
                .tenant_id
                .cmp(&b.data.tenant_id)
                .then_with(|| a.data.billing_period.cmp(&b.data.billing_period))
                .then_with(|| a.sequence_number.cmp(&b.sequence_number))
        });
        out
    }

    /// Number of stored aggregates.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.rows.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AggregatedCounterStore for InMemoryAggregatedCounterStore {
    fn upsert(
        &self,
        aggregate: &AggregatedCounter,
        new_chain_head: ChainHash,
        recomputed_digest_hex: &str,
    ) -> Result<UpsertOutcome, AggregatedCounterStoreError> {
        let mut g = self.state.lock().map_err(|_| {
            AggregatedCounterStoreError::Backend("counter store mutex poisoned".to_string())
        })?;
        let key = aggregate.group_key();

        let outcome = if let Some(prior) = g.rows.get(&key) {
            if prior.digest_hex == recomputed_digest_hex {
                UpsertOutcome::AlreadyExistsIdempotent
            } else {
                return Err(AggregatedCounterStoreError::DigestMismatch {
                    tenant_id: key.tenant_id.to_string(),
                    billing_period: key.billing_period.clone(),
                    event_kind: key.event_kind.as_str().to_string(),
                    stored_digest_hex: prior.digest_hex.clone(),
                    recomputed_digest_hex: recomputed_digest_hex.to_string(),
                });
            }
        } else {
            UpsertOutcome::Inserted
        };

        g.rows.insert(
            key.clone(),
            StoredAggregate {
                aggregate: aggregate.clone(),
                digest_hex: recomputed_digest_hex.to_string(),
            },
        );

        // Chain head advance — only for fresh inserts. Idempotent
        // replays (AlreadyExistsIdempotent) skip the head advance per
        // the trait contract; the caller dedups upstream via the
        // chain-head lookup.
        if outcome == UpsertOutcome::Inserted {
            let head_key = (key.tenant_id, key.billing_period.clone());
            let new_record = ChainHeadRecord {
                current_head: new_chain_head,
                next_sequence: aggregate.sequence_number.saturating_add(1),
            };
            g.heads.insert(head_key, new_record);
        }
        Ok(outcome)
    }

    fn chain_head(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<ChainHeadRecord, AggregatedCounterStoreError> {
        let g = self.state.lock().map_err(|_| {
            AggregatedCounterStoreError::Backend("counter store mutex poisoned".to_string())
        })?;
        Ok(g.heads
            .get(&(tenant_id, billing_period.to_string()))
            .copied()
            .unwrap_or_default())
    }

    fn get(
        &self,
        key: &CounterGroupKey,
    ) -> Result<Option<AggregatedCounter>, AggregatedCounterStoreError> {
        let g = self.state.lock().map_err(|_| {
            AggregatedCounterStoreError::Backend("counter store mutex poisoned".to_string())
        })?;
        Ok(g.rows.get(key).map(|s| s.aggregate.clone()))
    }
}

/// Always-failing counter store for adversarial tests of the
/// fail-CLOSED envelope (sink_failure audit arm).
#[derive(Debug, Default)]
pub struct FailingAggregatedCounterStore;

impl FailingAggregatedCounterStore {
    /// Construct a fresh always-failing store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AggregatedCounterStore for FailingAggregatedCounterStore {
    fn upsert(
        &self,
        _aggregate: &AggregatedCounter,
        _new_chain_head: ChainHash,
        _recomputed_digest_hex: &str,
    ) -> Result<UpsertOutcome, AggregatedCounterStoreError> {
        Err(AggregatedCounterStoreError::Backend(
            "induced billing-aggregator counter store failure (test fixture)".to_string(),
        ))
    }

    fn chain_head(
        &self,
        _tenant_id: Uuid,
        _billing_period: &str,
    ) -> Result<ChainHeadRecord, AggregatedCounterStoreError> {
        Ok(ChainHeadRecord::genesis())
    }

    fn get(
        &self,
        _key: &CounterGroupKey,
    ) -> Result<Option<AggregatedCounter>, AggregatedCounterStoreError> {
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
    use crate::chain::{compute_canonical_bytes, link_chain_hash};
    use corelink_billing_emit::{IdemKey, UsageEventKind};
    use uuid::Uuid;

    fn fresh_aggregate(seq: u64, prev: ChainHash, tenant: Uuid, qty: u128) -> AggregatedCounter {
        AggregatedCounter::new(
            "src",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(seq),
            seq,
            prev,
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            qty,
            1,
            0,
            1000,
            vec![IdemKey::genesis()],
        )
    }

    #[test]
    fn fresh_store_is_empty() {
        let s = InMemoryAggregatedCounterStore::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn fresh_chain_head_is_genesis() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let head = s.chain_head(tenant, "2026-05").unwrap();
        assert_eq!(head, ChainHeadRecord::genesis());
    }

    #[test]
    fn first_upsert_inserts_and_advances_chain() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let canonical = compute_canonical_bytes(&a).unwrap();
        let new_head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let digest_hex = hex::encode(canonical.iter().take(32).copied().collect::<Vec<_>>());
        let outcome = s.upsert(&a, new_head, &digest_hex).unwrap();
        assert_eq!(outcome, UpsertOutcome::Inserted);
        let head = s.chain_head(tenant, "2026-05").unwrap();
        assert_eq!(head.current_head, new_head);
        assert_eq!(head.next_sequence, 1);
    }

    #[test]
    fn idempotent_upsert_with_same_digest_skips_chain_advance() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let new_head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let digest_hex = "ab".repeat(32);
        let _ = s.upsert(&a, new_head, &digest_hex).unwrap();
        // Re-upsert with same digest; outcome = AlreadyExistsIdempotent.
        let outcome2 = s.upsert(&a, new_head, &digest_hex).unwrap();
        assert_eq!(outcome2, UpsertOutcome::AlreadyExistsIdempotent);
        // Chain head unchanged after idempotent re-upsert.
        let head = s.chain_head(tenant, "2026-05").unwrap();
        assert_eq!(head.current_head, new_head);
        assert_eq!(head.next_sequence, 1);
    }

    #[test]
    fn upsert_with_diverged_digest_rejects_with_mismatch() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let new_head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let digest_a = "ab".repeat(32);
        let digest_b = "cd".repeat(32);
        let _ = s.upsert(&a, new_head, &digest_a).unwrap();
        let err = s.upsert(&a, new_head, &digest_b).unwrap_err();
        assert!(matches!(
            err,
            AggregatedCounterStoreError::DigestMismatch { .. }
        ));
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let key = CounterGroupKey {
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            event_kind: UsageEventKind::CasPut,
        };
        assert!(s.get(&key).unwrap().is_none());
    }

    #[test]
    fn get_returns_stored_aggregate() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let new_head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let digest_hex = "ab".repeat(32);
        s.upsert(&a, new_head, &digest_hex).unwrap();
        let key = a.group_key();
        let got = s.get(&key).unwrap().unwrap();
        assert_eq!(got.data.total_qty, 10);
    }

    #[test]
    fn cross_period_chains_partitioned() {
        let s = InMemoryAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let mut a_may = fresh_aggregate(0, ChainHash::genesis(), tenant, 1);
        a_may.data.billing_period = "2026-05".to_string();
        let mut a_jun = fresh_aggregate(0, ChainHash::genesis(), tenant, 2);
        a_jun.data.billing_period = "2026-06".to_string();
        let head_may = link_chain_hash(&ChainHash::genesis(), &a_may).unwrap();
        let head_jun = link_chain_hash(&ChainHash::genesis(), &a_jun).unwrap();
        s.upsert(&a_may, head_may, &"a".repeat(64)).unwrap();
        s.upsert(&a_jun, head_jun, &"b".repeat(64)).unwrap();
        // Per-period chains independent.
        assert_eq!(
            s.chain_head(tenant, "2026-05").unwrap().current_head,
            head_may
        );
        assert_eq!(
            s.chain_head(tenant, "2026-06").unwrap().current_head,
            head_jun
        );
    }

    #[test]
    fn cloned_store_shares_state() {
        let s1 = InMemoryAggregatedCounterStore::new();
        let s2 = s1.clone();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        s1.upsert(&a, head, &"a".repeat(64)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn failing_store_returns_backend_error() {
        let s = FailingAggregatedCounterStore::new();
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 10);
        let head = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let err = s.upsert(&a, head, &"a".repeat(64)).unwrap_err();
        assert!(matches!(err, AggregatedCounterStoreError::Backend(_)));
    }
}
