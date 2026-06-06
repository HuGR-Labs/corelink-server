//! Cron orchestrator trait + in-memory fake (WI-S04-005 §6.1.4).
//!
//! ## Trait-abstraction-defer pattern
//!
//! Per the corelink autonomous execution charter, the production
//! Cloudflare Cron Durable Object binding is deferred to WI-S04-006
//! (alongside the REAPI conformance suite + miniflare/wrangler-dev
//! integration). This crate ships:
//!
//! - [`TtlWorker`] — the canonical trait every cron orchestrator
//!   implements (`tick(now_ms) -> TtlWorkerTickOutcome`). Production
//!   wiring will adapt the Cloudflare DO `alarm()` handler against
//!   this surface — the alarm fires, calls `tick(now_ms)`, re-arms
//!   itself, and returns. The trait stays sync-call-shape so the DO
//!   adapter is a thin wrapper.
//! - [`InMemoryTtlWorker`] — the canonical pure-logic impl. Drives
//!   the canonical tenant-scoped sweep:
//!   1. enumerate tenants with expired rows in the worker's region;
//!   2. for each tenant, drain up to [`MAX_BATCH_SIZE`] rows per
//!      sub-batch (one [`EvictBatch::run_one_batch`] call) until the
//!      tenant has none left or the alarm budget is exhausted;
//!   3. aggregate per-row outcomes into the tick outcome.
//!
//! The fake exercises the **same algorithmic invariants** the
//! production DO will (per-region pinning, tenant-scoped sweeps,
//! bounded batch size, R2-first ordering, audit emission per
//! eviction), so property tests pinned at 10 k iter against the fake
//! cover the load-bearing flow without spinning up miniflare.
//!
//! ## Anti-stop semantics
//!
//! The fake's `tick` loop terminates when:
//!
//! - No tenant has any expired rows left in the worker's region.
//! - A configurable per-tick row ceiling is reached (defense-in-
//!   depth: even with 250-row batches, an unbounded outer loop on a
//!   pathological dataset could keep the cron tick alive past the
//!   30 s alarm budget; the ceiling lets tests pin the boundary).
//!
//! The DO production wrapper enforces the 30 s wall-clock budget via
//! the alarm system; the fake takes a row ceiling instead so tests
//! are deterministic.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use super::super::audit::AuditSink;
use super::super::handler::AcEnvelopeStore;
use super::super::meta::{AcMetaError, AcMetaStore};
use super::super::neg_cache::AcNegCache;
use super::evict::{EvictBatch, EvictBatchOutcome, EvictError, MAX_BATCH_SIZE};
use crate::cache::kv::KvBackend;
use crate::Region;
use corelink_tenant_path::TenantDerivationKey;

/// Errors surfaced by [`TtlWorker::tick`].
#[derive(Debug, Error)]
pub enum TtlWorkerError {
    /// `ac_meta` backend error during the tenant enumeration step.
    #[error("ac_meta backend error: {0}")]
    Meta(#[from] AcMetaError),
    /// Per-batch eviction error (R2 failure / region mismatch /
    /// batch-size ceiling violation).
    #[error("evict batch error: {0}")]
    Evict(#[from] EvictError),
}

/// Aggregate outcome of a single cron tick.
///
/// Produced by [`TtlWorker::tick`]; the production DO wrapper emits
/// these as metrics on every alarm fire per WI §6.1.10.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TtlWorkerTickOutcome {
    /// Region the worker is pinned to.
    pub region: Region,
    /// How many distinct tenants the tick swept.
    pub tenants_swept: usize,
    /// How many sub-batches the tick ran across all tenants.
    pub batches_run: usize,
    /// Aggregate per-row counters (sum across all sub-batches).
    pub aggregate: EvictBatchOutcome,
    /// Whether the worker stopped because the canonical row ceiling
    /// was reached (sustained workload signal — RB-FM-AC-TTL-STORM).
    pub hit_row_ceiling: bool,
}

impl TtlWorkerTickOutcome {
    /// Construct a fresh outcome pinned to `region` with zeroed
    /// counters.
    #[must_use]
    pub fn new(region: Region) -> Self {
        Self {
            region,
            tenants_swept: 0,
            batches_run: 0,
            aggregate: EvictBatchOutcome::default(),
            hit_row_ceiling: false,
        }
    }
}

impl TtlWorkerTickOutcome {
    /// Whether the tick should re-fire ASAP (workload outpaces).
    /// Production DO honors this by re-arming the alarm at the
    /// minimum interval (60 s per WI §6.1.4 + `RB-FM-AC-TTL-STORM`)
    /// rather than the canonical 1 h when set.
    #[must_use]
    pub const fn signals_storm(&self) -> bool {
        self.hit_row_ceiling
    }
}

/// Cron orchestrator trait. Production wiring (WI-S04-006) implements
/// against a Cloudflare Cron Durable Object (`alarm()` handler);
/// every fake here exercises the same algorithmic flow.
pub trait TtlWorker: Send + Sync {
    /// Drain expired rows for the worker's pinned region. Returns
    /// the aggregate tick outcome; production DO emits as metrics +
    /// re-arms the alarm.
    ///
    /// Caller MUST supply a deterministic `request_id` (production
    /// uses the DO's `alarm_id` or a UUIDv7 minted at alarm fire) so
    /// the audit chain can correlate the ticks across multiple
    /// shards.
    fn tick<'a>(
        &'a self,
        now_ms: u64,
        request_id: &'a str,
    ) -> impl Future<Output = Result<TtlWorkerTickOutcome, TtlWorkerError>> + Send + 'a;
}

/// Pure-logic in-memory cron orchestrator (WI-S04-005). Drives the
/// canonical tenant-scoped sweep.
pub struct InMemoryTtlWorker<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    region: Region,
    batch: Arc<EvictBatch<M, E, A, K>>,
    meta: Arc<M>,
    /// Per-tick row ceiling — tests pin this so they can assert the
    /// "RB-FM-AC-TTL-STORM" signal. Production DO uses a wall-clock
    /// ceiling (30 s alarm budget) instead; both ceilings exist
    /// independently — the row ceiling protects the in-memory tests
    /// from runaway loops.
    row_ceiling: usize,
    /// Per-tenant batch size cap (`<= MAX_BATCH_SIZE`).
    batch_size: usize,
}

impl<M, E, A, K> fmt::Debug for InMemoryTtlWorker<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryTtlWorker")
            .field("region", &self.region)
            .field("row_ceiling", &self.row_ceiling)
            .field("batch_size", &self.batch_size)
            .finish_non_exhaustive()
    }
}

impl<M, E, A, K> InMemoryTtlWorker<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Construct a fresh worker pinned to `region`.
    ///
    /// # Errors
    ///
    /// Returns an error string when `batch_size > MAX_BATCH_SIZE` —
    /// programmer wiring error.
    #[allow(
        clippy::too_many_arguments,
        reason = "8-arg shape mirrors the canonical AC handler builder dependency set + per-tick budget knobs; further compaction would obscure the cron worker's wiring contract"
    )]
    pub fn new(
        region: Region,
        meta: Arc<M>,
        envelope_store: Arc<E>,
        audit: Arc<A>,
        neg_cache: Arc<AcNegCache<K>>,
        tdk: Arc<TenantDerivationKey>,
        row_ceiling: usize,
        batch_size: usize,
    ) -> Result<Self, &'static str> {
        if batch_size == 0 {
            return Err("batch_size must be >= 1");
        }
        if batch_size > MAX_BATCH_SIZE {
            return Err("batch_size exceeds MAX_BATCH_SIZE");
        }
        let batch = Arc::new(EvictBatch::new(
            region,
            Arc::clone(&meta),
            envelope_store,
            audit,
            neg_cache,
            tdk,
        ));
        Ok(Self {
            region,
            batch,
            meta,
            row_ceiling,
            batch_size,
        })
    }

    /// Region this worker is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Per-tenant batch size.
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }
}

impl<M, E, A, K> TtlWorker for InMemoryTtlWorker<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn tick<'a>(
        &'a self,
        now_ms: u64,
        request_id: &'a str,
    ) -> impl Future<Output = Result<TtlWorkerTickOutcome, TtlWorkerError>> + Send + 'a {
        async move {
            let mut outcome = TtlWorkerTickOutcome::new(self.region);
            // Step 1 — enumerate tenants with expired rows.
            let tenants: Vec<Uuid> = self.meta.tenants_with_expired(self.region, now_ms).await?;
            outcome.tenants_swept = tenants.len();

            // Step 2 — drain each tenant's queue. Round-robin across
            // tenants so a single tenant with millions of expired
            // rows can't starve the others within a tick budget.
            let mut active: Vec<Uuid> = tenants;
            'tick: while !active.is_empty() {
                let mut next_active = Vec::with_capacity(active.len());
                for tenant in active {
                    if outcome.aggregate.total() >= self.row_ceiling {
                        outcome.hit_row_ceiling = true;
                        break 'tick;
                    }
                    let remaining = self.row_ceiling.saturating_sub(outcome.aggregate.total());
                    let limit = self.batch_size.min(remaining);
                    if limit == 0 {
                        outcome.hit_row_ceiling = true;
                        break 'tick;
                    }
                    let batch_outcome = self
                        .batch
                        .run_one_batch(tenant, now_ms, limit, request_id)
                        .await?;
                    outcome.batches_run += 1;
                    aggregate_into(&mut outcome.aggregate, &batch_outcome);
                    // If this tenant returned a full batch, requeue —
                    // they may have more expired rows.
                    if batch_outcome.total() == limit && limit > 0 {
                        next_active.push(tenant);
                    }
                }
                active = next_active;
            }
            Ok(outcome)
        }
    }
}

fn aggregate_into(target: &mut EvictBatchOutcome, src: &EvictBatchOutcome) {
    target.evicted += src.evicted;
    target.r2_failed += src.r2_failed;
    target.d1_failed += src.d1_failed;
    target.already_gone += src.already_gone;
    target.audit_emit_failed += src.audit_emit_failed;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_arguments,
    reason = "test code: panics surface as test failures by design; helper signature mirrors the AcUpsertRequest field set"
)]
mod tests {
    use super::super::super::audit::{AcEventType, InMemoryAuditSink};
    use super::super::super::handler::InMemoryAcEnvelopeStore;
    use super::super::super::meta::{AcKey, AcUpsertRequest, InMemoryAcMetaStore};
    use super::super::super::types::{ActionDigest, ActionResult, ResultHash};
    use super::*;
    use crate::cache::kv::InMemoryKv;
    use corelink_hash::Digest;
    use corelink_tenant_path::derive_prefix;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_tdk() -> Arc<TenantDerivationKey> {
        Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
    }

    fn fixed_tenant_a() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn fixed_tenant_b() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
    }

    async fn upsert(
        meta: &InMemoryAcMetaStore,
        tdk: &TenantDerivationKey,
        tenant_id: Uuid,
        action_seed: &[u8],
        result_seed: &[u8],
        now_ms: u64,
        ttl_ms: Option<u64>,
        region: Region,
    ) {
        let action_hash = Digest::compute(action_seed);
        let key = AcKey::new(tenant_id, action_hash);
        let prefix = derive_prefix(tdk, tenant_id);
        let result_hash = ResultHash::compute(&ActionResult::new(
            Vec::new(),
            Vec::new(),
            0,
            result_seed.to_vec(),
        ));
        let req = AcUpsertRequest {
            key,
            action_digest: ActionDigest::new(action_hash, action_seed.len() as i64),
            tenant_prefix: prefix,
            region,
            result_hash,
            path_key_id: 1,
            sig_key_id: 1,
            now_ms,
            ttl_ms,
        };
        meta.upsert(req).await.unwrap();
    }

    async fn build_worker(
        region: Region,
        row_ceiling: usize,
        batch_size: usize,
    ) -> (
        InMemoryTtlWorker<
            InMemoryAcMetaStore,
            InMemoryAcEnvelopeStore,
            InMemoryAuditSink,
            InMemoryKv,
        >,
        Arc<InMemoryAcMetaStore>,
        Arc<InMemoryAuditSink>,
        Arc<TenantDerivationKey>,
    ) {
        let meta = Arc::new(InMemoryAcMetaStore::new());
        let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let neg = Arc::new(AcNegCache::new(region, InMemoryKv::new()).unwrap());
        let tdk = fixed_tdk();
        let worker = InMemoryTtlWorker::new(
            region,
            Arc::clone(&meta),
            Arc::clone(&envelope),
            Arc::clone(&audit),
            Arc::clone(&neg),
            Arc::clone(&tdk),
            row_ceiling,
            batch_size,
        )
        .unwrap();
        (worker, meta, audit, tdk)
    }

    #[tokio::test]
    async fn tick_drains_all_expired_rows_in_region() {
        let (worker, meta, audit, tdk) = build_worker(Region::Wnam, 1000, 100).await;
        // 5 expired across A, 3 expired across B, 1 fresh.
        for i in 0_u64..5 {
            upsert(
                meta.as_ref(),
                tdk.as_ref(),
                fixed_tenant_a(),
                &[b'a', i as u8],
                &[b'r', i as u8],
                1_000,
                Some(1_000),
                Region::Wnam,
            )
            .await;
        }
        for i in 0_u64..3 {
            upsert(
                meta.as_ref(),
                tdk.as_ref(),
                fixed_tenant_b(),
                &[b'b', i as u8],
                &[b'r', i as u8],
                1_000,
                Some(1_000),
                Region::Wnam,
            )
            .await;
        }
        upsert(
            meta.as_ref(),
            tdk.as_ref(),
            fixed_tenant_a(),
            b"keep-a",
            b"keep-r",
            1_000,
            Some(60_000_000),
            Region::Wnam,
        )
        .await;
        let outcome = worker.tick(5_000_000, "tick-1").await.unwrap();
        assert_eq!(outcome.tenants_swept, 2);
        assert_eq!(outcome.aggregate.evicted, 8);
        assert!(!outcome.hit_row_ceiling);
        // Only fresh row remains.
        let key = AcKey::new(fixed_tenant_a(), Digest::compute(b"keep-a"));
        assert!(meta.get(&key).await.unwrap().is_some());
        let evicted = audit.snapshot_of(AcEventType::EvictTtlExpired);
        assert_eq!(evicted.len(), 8);
    }

    #[tokio::test]
    async fn tick_respects_per_region_pinning() {
        let (worker, meta, _audit, tdk) = build_worker(Region::Wnam, 1000, 100).await;
        // wnam: 2 expired
        upsert(
            meta.as_ref(),
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a1",
            b"r1",
            1_000,
            Some(1_000),
            Region::Wnam,
        )
        .await;
        upsert(
            meta.as_ref(),
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a2",
            b"r2",
            1_000,
            Some(1_000),
            Region::Wnam,
        )
        .await;
        // weur: 5 expired (must not be touched)
        for i in 0_u64..5 {
            upsert(
                meta.as_ref(),
                tdk.as_ref(),
                fixed_tenant_a(),
                &[b'w', i as u8],
                &[b'r', i as u8],
                1_000,
                Some(1_000),
                Region::Weur,
            )
            .await;
        }
        let outcome = worker.tick(5_000_000, "tick-1").await.unwrap();
        assert_eq!(outcome.aggregate.evicted, 2); // wnam only
                                                  // weur rows preserved (different region; this worker is wnam).
        for i in 0_u64..5 {
            let key = AcKey::new(fixed_tenant_a(), Digest::compute(&[b'w', i as u8]));
            assert!(meta.get(&key).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn tick_signals_storm_at_row_ceiling() {
        let (worker, meta, _audit, tdk) = build_worker(Region::Wnam, 5, 10).await;
        for i in 0_u64..20 {
            upsert(
                meta.as_ref(),
                tdk.as_ref(),
                fixed_tenant_a(),
                &[b'a', i as u8],
                &[b'r', i as u8],
                1_000,
                Some(1_000),
                Region::Wnam,
            )
            .await;
        }
        let outcome = worker.tick(5_000_000, "tick-1").await.unwrap();
        assert!(outcome.hit_row_ceiling);
        assert!(outcome.signals_storm());
        assert!(outcome.aggregate.evicted <= 5);
    }

    #[tokio::test]
    async fn tick_idempotent_when_no_expired_rows() {
        let (worker, _meta, _audit, _tdk) = build_worker(Region::Wnam, 1000, 100).await;
        let outcome = worker.tick(5_000_000, "tick-1").await.unwrap();
        assert_eq!(outcome.tenants_swept, 0);
        assert_eq!(outcome.aggregate.total(), 0);
    }

    #[test]
    fn worker_rejects_zero_batch_size() {
        let meta = Arc::new(InMemoryAcMetaStore::new());
        let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let neg = Arc::new(AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap());
        let tdk = fixed_tdk();
        let r = InMemoryTtlWorker::new(Region::Wnam, meta, envelope, audit, neg, tdk, 1000, 0);
        assert!(r.is_err());
    }

    #[test]
    fn worker_rejects_batch_size_above_ceiling() {
        let meta = Arc::new(InMemoryAcMetaStore::new());
        let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let neg = Arc::new(AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap());
        let tdk = fixed_tdk();
        let r = InMemoryTtlWorker::new(
            Region::Wnam,
            meta,
            envelope,
            audit,
            neg,
            tdk,
            1000,
            MAX_BATCH_SIZE + 1,
        );
        assert!(r.is_err());
    }
}
