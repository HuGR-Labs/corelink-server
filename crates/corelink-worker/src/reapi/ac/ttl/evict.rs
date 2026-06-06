//! Tenant-scoped batched eviction logic (WI-S04-005 §6.1.5 + §9.3).
//!
//! ## Order of operations
//!
//! Per WI §9.3 + ADR-0019 the canonical step ordering is:
//!
//! 1. **R2 envelope DELETE** (external; idempotent at the canonical
//!    key shape `ac-<region>/<tenant_prefix>/<digest>.json`).
//! 2. On R2 success → **D1 row DELETE** + **audit_outbox INSERT**
//!    (atomic D1 batch; trait surface keeps the two ops together).
//! 3. On D1 success → **KV negative-cache invalidate** (idempotent).
//!
//! On R2 failure: D1 row is preserved (still references the surviving
//! envelope); next cron tick retries (idempotent — `expires_at < now`
//! still selects the row). On D1 failure post-R2-success: the R2
//! envelope is gone but the D1 row stays; next cron tick re-runs the
//! delete (idempotent: `delete_tenant_scoped` returns `Ok(false)` if
//! the row is already gone). Orphan window = 1 cron interval per
//! ADR-0035 H-3 reconcile pattern.
//!
//! ## Bounded batch
//!
//! [`MAX_BATCH_SIZE`] = 250 rows per tick (WI-S04-005 Lote 10.4bis P0
//! fix; D1 100 KB batch ceiling — each row produces D1 DELETE +
//! audit_outbox INSERT pair ~400 bytes; 250 × 400 = 100 KB exactly
//! aligned). Callers MUST cap at this value; the batch helper
//! [`EvictBatch::run_one_batch`] reads up to `min(limit,
//! MAX_BATCH_SIZE)` rows.
//!
//! ## Tenant-scoped sweep
//!
//! Every batch is scoped to **a single tenant** + **a single region**
//! per `INV-AC-EVICT-TENANT-SCOPED`. The cron worker enumerates the
//! tenants with expired rows in its region (via
//! [`super::super::meta::AcMetaStore::tenants_with_expired`]) and
//! drains each tenant's queue one batch at a time. Cross-tenant
//! batches are structurally impossible — the trait surface refuses to
//! select rows across tenant boundaries.

use core::fmt;
use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use super::super::audit::{AcAuditRecord, AcEventType, AuditSink, AuditSinkError};
use super::super::handler::AcEnvelopeStore;
use super::super::meta::{AcExpiredCandidate, AcMetaError, AcMetaStore};
use super::super::neg_cache::AcNegCache;
use super::super::types::ActionDigest;
use crate::cache::kv::KvBackend;
use crate::Region;
use crate::TenantCtx;
use corelink_tenant_path::TenantDerivationKey;

/// Canonical bounded batch ceiling — 250 rows per tick.
///
/// **Why 250**: WI-S04-005 §6.1.5 Lote 10.4bis P0 fix. Each evicted
/// row produces a D1 `DELETE FROM ac_meta` + `INSERT INTO
/// audit_outbox` pair, ~400 bytes serialized; the D1 batch ceiling is
/// 100 KB; `250 × 400 = 100 KB` exactly aligned. R2 DELETE for 250
/// rows is sequential ~2.5 s, well within the 30 s alarm budget.
pub const MAX_BATCH_SIZE: usize = 250;

/// Errors surfaced by [`EvictBatch::run_one_batch`].
#[derive(Debug, Error)]
pub enum EvictError {
    /// `ac_meta` backend error (D1 SELECT / DELETE failure).
    #[error("ac_meta backend error: {0}")]
    Meta(#[from] AcMetaError),
    /// Audit sink error (audit_outbox INSERT failure / SIEM webhook).
    #[error("audit sink error: {0}")]
    Audit(#[from] AuditSinkError),
    /// Region the worker is pinned to does not match the candidate's
    /// region. Programmer wiring error.
    #[error("region mismatch: worker pinned to {worker} but candidate is {candidate}")]
    RegionMismatch {
        /// Region the worker is pinned to.
        worker: Region,
        /// Region the candidate carries.
        candidate: Region,
    },
    /// Caller supplied a batch limit > [`MAX_BATCH_SIZE`]. Programmer
    /// error; the cron worker MUST cap at the ceiling.
    #[error("batch_size {requested} exceeds canonical ceiling {ceiling}")]
    BatchSizeExceeded {
        /// Caller-requested limit.
        requested: usize,
        /// Canonical ceiling [`MAX_BATCH_SIZE`].
        ceiling: usize,
    },
}

/// Per-row outcome of [`EvictBatch::evict_row`]. The cron worker
/// aggregates these into a [`EvictBatchOutcome`] so metrics +
/// runbook signals can break down per failure class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvictRowOutcome {
    /// Row evicted cleanly (R2 DELETE OK + D1 DELETE OK + audit
    /// emitted + KV invalidated).
    Evicted,
    /// R2 DELETE returned an error after the canonical 1-retry
    /// backoff. D1 row preserved; audit `ac.evict.r2_failed`
    /// emitted; next cron tick retries.
    R2Failed {
        /// Backend error string from the R2 adapter.
        reason: String,
    },
    /// D1 DELETE returned an error after R2 success. R2 envelope
    /// already gone; D1 row preserved; audit emitted; next cron tick
    /// retries idempotently.
    D1Failed {
        /// Backend error string from the D1 adapter.
        reason: String,
    },
    /// Row already gone at delete time (race with manual eviction /
    /// previous cron tick). No-op; counted as success.
    AlreadyGone,
    /// Audit emit failed after a successful R2+D1 delete. The row is
    /// gone; surface as a per-row error so the runbook can flag the
    /// audit gap (forensic chain integrity).
    AuditEmitFailed {
        /// Backend error from the audit sink.
        reason: String,
    },
}

/// Aggregate outcome of [`EvictBatch::run_one_batch`] — counters per
/// row class. The cron worker emits these as metrics
/// (`corelink.ac.ttl.rows_evicted_total`,
/// `corelink.ac.ttl.r2_delete_failed_total`, etc.) per WI §6.1.10.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvictBatchOutcome {
    /// Rows successfully evicted (R2+D1+audit+KV all OK).
    pub evicted: usize,
    /// Rows that failed at R2 DELETE (preserved D1).
    pub r2_failed: usize,
    /// Rows that failed at D1 DELETE post-R2-success.
    pub d1_failed: usize,
    /// Rows that were already gone at delete time (idempotent retry
    /// post-prior-tick).
    pub already_gone: usize,
    /// Rows whose audit emit failed post-delete.
    pub audit_emit_failed: usize,
}

impl EvictBatchOutcome {
    /// Total rows in the batch (sum of all classes).
    #[must_use]
    pub const fn total(&self) -> usize {
        self.evicted + self.r2_failed + self.d1_failed + self.already_gone + self.audit_emit_failed
    }

    /// Whether this batch consumed the full canonical limit. A
    /// batch_size at the cap signals sustained workload that may
    /// outpace the cron interval — the worker raises a metric for
    /// the runbook (`RB-FM-AC-TTL-STORM`) per WI §6.1.10 +
    /// `RB-FM-AC-TTL-STORM` Detection.
    #[must_use]
    pub const fn hit_cap(&self) -> bool {
        self.total() >= MAX_BATCH_SIZE
    }
}

/// Tenant-scoped eviction batch driver. Holds the trait-object
/// dependencies the cron worker needs to drain a single
/// `(region, tenant_id)` queue.
pub struct EvictBatch<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    region: Region,
    meta: Arc<M>,
    envelope_store: Arc<E>,
    audit: Arc<A>,
    neg_cache: Arc<AcNegCache<K>>,
    /// `TenantDerivationKey` used by [`AcNegCache`] for the
    /// per-tenant prefix lookup. The TTL worker does **not** use the
    /// TDK to derive R2 paths (those come off the `tenant_prefix`
    /// column per ADR-0035 H-3 + WI-S04-005 §9.5). It is purely the
    /// negative-cache `TenantCtx::new` ctor's input.
    tdk: Arc<TenantDerivationKey>,
}

impl<M, E, A, K> fmt::Debug for EvictBatch<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvictBatch")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl<M, E, A, K> EvictBatch<M, E, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Construct a fresh batch driver pinned to `region`.
    pub fn new(
        region: Region,
        meta: Arc<M>,
        envelope_store: Arc<E>,
        audit: Arc<A>,
        neg_cache: Arc<AcNegCache<K>>,
        tdk: Arc<TenantDerivationKey>,
    ) -> Self {
        Self {
            region,
            meta,
            envelope_store,
            audit,
            neg_cache,
            tdk,
        }
    }

    /// Region this batch is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Drain up to `limit` expired rows for `(region, tenant_id)`.
    /// Caller MUST pass `limit <= MAX_BATCH_SIZE`; the helper
    /// surfaces [`EvictError::BatchSizeExceeded`] on violation
    /// (defense-in-depth: the cron worker already caps, but the trait
    /// surface refuses to silently truncate).
    ///
    /// The batch is **strictly tenant-scoped** + **strictly region-
    /// pinned**. A candidate that mismatches the worker's region
    /// raises [`EvictError::RegionMismatch`] (programmer error; per-
    /// region cron sharding makes this unreachable in production).
    pub async fn run_one_batch(
        &self,
        tenant_id: Uuid,
        now_ms: u64,
        limit: usize,
        request_id: &str,
    ) -> Result<EvictBatchOutcome, EvictError> {
        if limit > MAX_BATCH_SIZE {
            return Err(EvictError::BatchSizeExceeded {
                requested: limit,
                ceiling: MAX_BATCH_SIZE,
            });
        }
        let candidates = self
            .meta
            .select_expired_for_region(self.region, tenant_id, now_ms, limit)
            .await?;
        let mut outcome = EvictBatchOutcome::default();
        for candidate in candidates {
            // Defense-in-depth region check (the trait already
            // filters by region, but we re-assert the contract at
            // the row boundary).
            if candidate.region != self.region {
                return Err(EvictError::RegionMismatch {
                    worker: self.region,
                    candidate: candidate.region,
                });
            }
            // Defense-in-depth tenant check (the trait already
            // filters by tenant_id, but we re-assert too — a future
            // refactor that loosens the SELECT predicate would
            // surface here as a panic-free programmer error).
            debug_assert_eq!(candidate.tenant_id, tenant_id);
            let row_outcome = self.evict_row(&candidate, request_id, now_ms).await?;
            match row_outcome {
                EvictRowOutcome::Evicted => outcome.evicted += 1,
                EvictRowOutcome::R2Failed { .. } => outcome.r2_failed += 1,
                EvictRowOutcome::D1Failed { .. } => outcome.d1_failed += 1,
                EvictRowOutcome::AlreadyGone => outcome.already_gone += 1,
                EvictRowOutcome::AuditEmitFailed { .. } => outcome.audit_emit_failed += 1,
            }
        }
        Ok(outcome)
    }

    /// Evict a single row in the canonical R2 → D1 → audit → KV
    /// order. Public so the cron worker can drive eviction one row
    /// at a time when the batch helper is too coarse (e.g., backoff
    /// scheduling on R2-failed rows).
    pub async fn evict_row(
        &self,
        candidate: &AcExpiredCandidate,
        request_id: &str,
        now_ms: u64,
    ) -> Result<EvictRowOutcome, EvictError> {
        // Step 1 — R2 envelope DELETE. The R2 backend's
        // `AcEnvelopeStore` trait does not yet expose `delete` — for
        // the in-memory pure-logic path we leave the envelope in
        // place (idempotent re-PUT on the next UPDATE will overwrite;
        // production wiring lands `delete` alongside the real CF R2
        // binding shim in WI-S04-006). The pure-logic path still
        // exercises the canonical D1 + audit + KV ordering.
        //
        // For the deferred R2 envelope DELETE: the envelope_store
        // field is held so the production binding can be swapped in
        // without changing this surface. The contract documented
        // here pins the canonical R2-first sequencing.
        //
        // The pure-logic path proceeds directly to D1 (R2 step is a
        // no-op against the in-memory fake; production binding will
        // call `envelope_store.delete(...)` at this seam — its
        // signature is intentionally absent from the public trait
        // until the production binding lands).
        let _ = &self.envelope_store; // touch field for future binding
        let r2_failed_reason = self.r2_delete(candidate).await.err();
        if let Some(reason) = r2_failed_reason {
            // Audit emit — `ac.evict.r2_failed` (canonical event
            // type per WI §6.1.5 step 6.a). D1 row is preserved.
            self.audit.emit(self.make_record(
                AcEventType::EvictR2Failed,
                candidate,
                request_id,
                "r2_delete_failed",
                now_ms,
            ))?;
            return Ok(EvictRowOutcome::R2Failed { reason });
        }

        // Step 2 — D1 row DELETE (tenant + region scoped).
        match self
            .meta
            .delete_tenant_scoped(candidate.tenant_id, &candidate.action_digest, self.region)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                // Already gone (race with manual eviction or prior
                // cron tick). Skip audit emit + KV invalidation —
                // either the prior tick already emitted, or the
                // manual op handled it.
                return Ok(EvictRowOutcome::AlreadyGone);
            }
            Err(e) => {
                let reason = format!("{e}");
                let _ = self.audit.emit(self.make_record(
                    AcEventType::EvictD1Failed,
                    candidate,
                    request_id,
                    "d1_delete_failed",
                    now_ms,
                ));
                return Ok(EvictRowOutcome::D1Failed { reason });
            }
        }

        // Step 3 — audit emit `ac.evict.ttl_expired`.
        if let Err(e) = self.audit.emit(self.make_record(
            AcEventType::EvictTtlExpired,
            candidate,
            request_id,
            "ttl_expired",
            now_ms,
        )) {
            return Ok(EvictRowOutcome::AuditEmitFailed {
                reason: format!("{e}"),
            });
        }

        // Step 4 — KV negative-cache invalidate (idempotent).
        // Soft-fail: KV outage does not roll back the eviction (the
        // row is gone in D1; KV is a hint cache).
        let tenant_ctx = TenantCtx::new(self.tdk.as_ref(), candidate.tenant_id, self.region);
        let _ = self
            .neg_cache
            .invalidate_on_update(&tenant_ctx, &candidate.action_digest)
            .await;

        Ok(EvictRowOutcome::Evicted)
    }

    async fn r2_delete(&self, _candidate: &AcExpiredCandidate) -> Result<(), String> {
        // Pure-logic path: the canonical R2 DELETE is a no-op against
        // the in-memory fake (the envelope store does not yet expose
        // `delete`). Production wiring (WI-S04-006) overrides this
        // path against a real R2 binding adapter that emits HTTP
        // DELETE on the canonical key shape. The retry-once-with-
        // backoff policy lives in the production binding (test-
        // injected via a `Retrying` decorator alongside the binding
        // shim) — pure-logic core stays simple.
        Ok(())
    }

    fn make_record(
        &self,
        event_type: AcEventType,
        candidate: &AcExpiredCandidate,
        request_id: &str,
        reason: &'static str,
        now_ms: u64,
    ) -> AcAuditRecord {
        AcAuditRecord {
            event_type,
            tenant_id: candidate.tenant_id,
            region: self.region,
            action_digest: ActionDigest::new(candidate.action_digest, 0),
            result_hash: None,
            request_id: request_id.to_string(),
            reason,
            missing_outputs: Vec::new(),
            now_ms,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_arguments,
    clippy::field_reassign_with_default,
    reason = "test code: panics surface as test failures by design; helper signature mirrors the AcUpsertRequest field set; field-reassign on outcome keeps assertions readable"
)]
mod tests {
    use super::super::super::audit::InMemoryAuditSink;
    use super::super::super::handler::InMemoryAcEnvelopeStore;
    use super::super::super::meta::{AcKey, AcUpsertRequest, InMemoryAcMetaStore};
    use super::super::super::types::ActionDigest;
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
        store: &InMemoryAcMetaStore,
        tdk: &TenantDerivationKey,
        tenant_id: Uuid,
        action_seed: &[u8],
        result_seed: &[u8],
        now_ms: u64,
        ttl_ms: Option<u64>,
        region: Region,
    ) -> AcKey {
        let action_hash = Digest::compute(action_seed);
        let key = AcKey::new(tenant_id, action_hash);
        let prefix = derive_prefix(tdk, tenant_id);
        let result_hash = super::super::super::types::ResultHash::compute(
            &super::super::super::types::ActionResult::new(
                Vec::new(),
                Vec::new(),
                0,
                result_seed.to_vec(),
            ),
        );
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
        store.upsert(req).await.unwrap();
        key
    }

    async fn build_batch(
        region: Region,
    ) -> (
        EvictBatch<InMemoryAcMetaStore, InMemoryAcEnvelopeStore, InMemoryAuditSink, InMemoryKv>,
        Arc<InMemoryAcMetaStore>,
        Arc<InMemoryAuditSink>,
        Arc<TenantDerivationKey>,
    ) {
        let meta = Arc::new(InMemoryAcMetaStore::new());
        let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let neg = Arc::new(AcNegCache::new(region, InMemoryKv::new()).unwrap());
        let tdk = fixed_tdk();
        let batch = EvictBatch::new(
            region,
            Arc::clone(&meta),
            Arc::clone(&envelope),
            Arc::clone(&audit),
            Arc::clone(&neg),
            Arc::clone(&tdk),
        );
        (batch, meta, audit, tdk)
    }

    #[tokio::test]
    async fn evict_evicts_only_target_tenants_rows() {
        let (batch, meta, audit, tdk) = build_batch(Region::Wnam).await;
        // A: expired
        upsert(
            &meta,
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a1",
            b"r1",
            1_000,
            Some(1_000),
            Region::Wnam,
        )
        .await;
        // B: expired (must not be touched)
        upsert(
            &meta,
            tdk.as_ref(),
            fixed_tenant_b(),
            b"b1",
            b"r2",
            1_000,
            Some(1_000),
            Region::Wnam,
        )
        .await;
        let outcome = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, 100, "req-tick-1")
            .await
            .unwrap();
        assert_eq!(outcome.evicted, 1);
        assert_eq!(outcome.total(), 1);
        // A's row gone, B's row preserved.
        let key_a = AcKey::new(fixed_tenant_a(), Digest::compute(b"a1"));
        let key_b = AcKey::new(fixed_tenant_b(), Digest::compute(b"b1"));
        assert!(meta.get(&key_a).await.unwrap().is_none());
        assert!(meta.get(&key_b).await.unwrap().is_some());
        // Audit emitted.
        let evicted = audit.snapshot_of(AcEventType::EvictTtlExpired);
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].tenant_id, fixed_tenant_a());
    }

    #[tokio::test]
    async fn evict_idempotent_on_already_deleted_row() {
        let (batch, meta, _audit, tdk) = build_batch(Region::Wnam).await;
        upsert(
            &meta,
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a1",
            b"r1",
            1_000,
            Some(1_000),
            Region::Wnam,
        )
        .await;
        let _ = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, 100, "tick-1")
            .await
            .unwrap();
        // Second tick — row already gone, must be a no-op.
        let outcome = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, 100, "tick-2")
            .await
            .unwrap();
        assert_eq!(outcome.total(), 0);
    }

    #[tokio::test]
    async fn evict_refuses_batch_size_above_ceiling() {
        let (batch, _meta, _audit, _tdk) = build_batch(Region::Wnam).await;
        let err = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, MAX_BATCH_SIZE + 1, "tick-x")
            .await
            .unwrap_err();
        assert!(matches!(err, EvictError::BatchSizeExceeded { .. }));
    }

    #[tokio::test]
    async fn evict_skips_non_expired_rows() {
        let (batch, meta, _audit, tdk) = build_batch(Region::Wnam).await;
        upsert(
            &meta,
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a1",
            b"r1",
            1_000,
            Some(60_000_000),
            Region::Wnam,
        )
        .await; // far-future expiry
        let outcome = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, 100, "tick-1")
            .await
            .unwrap();
        assert_eq!(outcome.total(), 0);
        let key = AcKey::new(fixed_tenant_a(), Digest::compute(b"a1"));
        assert!(meta.get(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn evict_respects_per_region_pinning() {
        let (batch, meta, _audit, tdk) = build_batch(Region::Wnam).await;
        // Insert a Weur expired row — must be invisible to wnam worker.
        upsert(
            &meta,
            tdk.as_ref(),
            fixed_tenant_a(),
            b"a1",
            b"r1",
            1_000,
            Some(1_000),
            Region::Weur,
        )
        .await;
        let outcome = batch
            .run_one_batch(fixed_tenant_a(), 5_000_000, 100, "tick-1")
            .await
            .unwrap();
        assert_eq!(outcome.total(), 0); // wnam sees nothing
        let key = AcKey::new(fixed_tenant_a(), Digest::compute(b"a1"));
        assert!(meta.get(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn batch_outcome_hit_cap_at_max_batch_size() {
        let mut o = EvictBatchOutcome::default();
        o.evicted = MAX_BATCH_SIZE;
        assert!(o.hit_cap());
        o.evicted = MAX_BATCH_SIZE - 1;
        assert!(!o.hit_cap());
    }

    #[test]
    fn max_batch_size_is_250() {
        assert_eq!(MAX_BATCH_SIZE, 250);
    }
}
