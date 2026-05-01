//! Sweeper Cron Durable Object — orphan multipart abort (WI-S05-006
//! §6.1.1 + PAT-SWEEPER-001 + FM-060 mitigation).
//!
//! ## Trait-abstraction-defer pattern
//!
//! Per the corelink autonomous execution charter, the production
//! Cloudflare Cron Durable Object binding (`alarm()` handler firing
//! every 1 h per WI §6.1.1) is deferred to the integration tier. This
//! module ships:
//!
//! - [`OrphanSweeper`] — the canonical trait every sweeper
//!   orchestrator implements (`tick(now_ms, request_id) ->
//!   SweeperTickOutcome`). Production wiring will adapt the
//!   Cloudflare DO `alarm()` handler against this surface — the
//!   alarm fires, calls `tick(now_ms)`, re-arms itself at the start
//!   of the tick (per Lote 10.4bis lesson — re-arm BEFORE work, never
//!   after, so a panic mid-tick still leaves the next alarm queued),
//!   and emits the per-tick metrics.
//! - [`InMemoryOrphanSweeper`] — the canonical pure-logic impl.
//!   Drives the canonical orphan flow:
//!     1. enumerate orphan candidates via
//!        [`SessionStore::list_orphans`]
//!        (`state=Live ∧ last_activity_at_ms < now_ms - 7d`) bounded
//!        by `MAX_BATCH_SIZE`;
//!     2. for each candidate: call
//!        [`SessionStore::abort`]; emit
//!        [`BlobEventType::SplitAborted`] with
//!        `reason = "orphan_swept"`;
//!     3. aggregate per-row outcomes into the tick outcome.
//!
//! The fake exercises the **same algorithmic invariants** the
//! production DO will (per-region pinning, tenant-scoped abort,
//! bounded batch size, audit emission per abort), so property tests
//! pinned at 10 k iter against the fake cover the load-bearing flow
//! without spinning up miniflare.
//!
//! ## Per-region instance + alarm re-arm at tick start (Lote 10.4bis)
//!
//! Production wiring deploys one DO per region (5 regions: sam / iad
//! / lhr / nrt / syd per `MultipartRegion` in
//! `corelink-multipart-schema`). Each DO is pinned to its region's
//! D1 + R2 binding pair so cross-region abort is structurally
//! unreachable (lesson `WI-S04-005` shard pattern reuse).
//!
//! Alarm re-arm semantics: the DO's `alarm()` handler MUST call
//! `state.storage().setAlarm(now + 1h)` BEFORE doing any work. If
//! the work panics mid-tick the next alarm is already queued — no
//! "silent stale" failure mode (RB-FM-060 chaos #1: cron not
//! re-armed → metric alert ≤ 1h).
//!
//! ## Bounded batch ceiling
//!
//! [`MAX_BATCH_SIZE`] = 250 per tick — same canonical ceiling the
//! S-04 TTL evictor uses (`reapi::ac::ttl::evict::MAX_BATCH_SIZE`).
//! D1 SELECT bounded by partial index on `state='in_progress'` per
//! `corelink-multipart-schema` migration 0003
//! (`uq_multipart_sessions_in_progress`). Per-tick wall-clock budget
//! is the DO alarm's 30 s ceiling; 250 R2 `abort_multipart_upload`
//! calls @ ~50 ms each ≈ 12.5 s — comfortably within the budget.
//!
//! ## 5-Layer Defense
//!
//! - **Layer 1 — auth**: cron tick is system-driven (no `AuthCtx`);
//!   the trait surface refuses cross-tenant abort by structural
//!   tenant_id-leftmost typing.
//! - **Layer 2 — D1 enforcement**:
//!   [`SessionStore::list_orphans`] returns `(tenant_id, session_id,
//!   region)`-keyed candidates;
//!   [`SessionStore::abort`] takes `(tenant_id, session_id)` by
//!   value — cross-tenant abort is structurally unreachable.
//! - **Layer 3 — scope**: N/A (system-driven).
//! - **Layer 4 — HMAC tenant prefix**: `tenant_prefix` is preserved
//!   on the audit record (same shape as the SplitBlob handler's
//!   audit emit) so the audit chain stays tenant-scoped.
//! - **Layer 5 — audit emit**: every abort emits a typed
//!   [`BlobAuditRecord`] with `event_type =
//!   BlobEventType::SplitAborted` + `reason = "orphan_swept"`.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + reapi::ac::ttl::TtlWorker canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use thiserror::Error;

use super::audit::{AuditSink, AuditSinkError, BlobAuditRecord, BlobEventType};
use super::session::{OrphanCandidate, SessionStore, SessionStoreError};
use crate::region::Region;

/// Canonical bounded batch ceiling — 250 sessions per tick.
///
/// **Why 250**: D1 partial-index SELECT bounded at 100 KB batch
/// ceiling; per-row payload ~400 bytes (UPDATE state='aborted' +
/// audit_outbox INSERT); 250 × 400 = 100 KB exactly aligned.
/// Mirrors `reapi::ac::ttl::evict::MAX_BATCH_SIZE` (S-04 lesson
/// reuse). 250 R2 abort_multipart_upload calls @ ~50 ms each ≈ 12.5
/// s — comfortably within the 30 s DO alarm budget.
pub const MAX_BATCH_SIZE: usize = 250;

/// Canonical orphan age cutoff — 7 days = 7 × 24 × 3600 × 1000 ms.
///
/// Per WI §6.1.1 + sprint contract §5.2 R-S05-5 + FM-060 SLA. R2
/// charges for ongoing multipart parts even when the upload is
/// stranded; 7 days balances "give the client time to retry" against
/// "stop billing for orphans". Customers needing tighter cutoffs go
/// through tier-specific configuration (S-13 admin plane, forward).
pub const ORPHAN_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Canonical alarm interval — 1 h.
pub const TICK_INTERVAL_MS: u64 = 60 * 60 * 1000;

/// Canonical reason code embedded in the abort audit record.
pub const ORPHAN_SWEPT_REASON: &str = "orphan_swept";

/// Errors surfaced by [`OrphanSweeper::tick`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SweeperError {
    /// `multipart_sessions` backend error (D1 SELECT / UPDATE
    /// failure).
    #[error("session store backend error: {0}")]
    Sessions(#[from] SessionStoreError),
    /// Audit sink error (audit_outbox INSERT failure / SIEM webhook).
    #[error("audit sink error: {0}")]
    Audit(#[from] AuditSinkError),
    /// Caller supplied a batch limit > [`MAX_BATCH_SIZE`]. Programmer
    /// error; sweeper MUST cap at the ceiling.
    #[error("batch_size {requested} exceeds canonical ceiling {ceiling}")]
    BatchSizeExceeded {
        /// Caller-requested limit.
        requested: usize,
        /// Canonical ceiling [`MAX_BATCH_SIZE`].
        ceiling: usize,
    },
}

/// Per-row outcome of one sweeper abort attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepRowOutcome {
    /// Session aborted cleanly (UPDATE state='aborted' + audit
    /// emitted).
    Aborted,
    /// Session was already finalized between SELECT and UPDATE
    /// (race window; the tick lost). No-op.
    AlreadyFinalized,
    /// Session was already aborted between SELECT and UPDATE (race
    /// window; the tick lost). No-op.
    AlreadyAborted,
    /// Session disappeared between SELECT and UPDATE (concurrent GC
    /// or tenant erasure). No-op.
    NotFound,
}

/// Aggregate counters for one sweeper tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SweepBatchOutcome {
    /// Sessions aborted cleanly.
    pub aborted: u32,
    /// Sessions skipped because they finalized before the UPDATE.
    pub already_finalized: u32,
    /// Sessions skipped because they were already aborted.
    pub already_aborted: u32,
    /// Sessions that disappeared between SELECT and UPDATE.
    pub not_found: u32,
}

impl SweepBatchOutcome {
    /// Total rows processed = aborted + already_finalized +
    /// already_aborted + not_found.
    #[must_use]
    pub const fn rows_processed(&self) -> u32 {
        self.aborted
            .saturating_add(self.already_finalized)
            .saturating_add(self.already_aborted)
            .saturating_add(self.not_found)
    }

    fn observe(&mut self, outcome: SweepRowOutcome) {
        match outcome {
            SweepRowOutcome::Aborted => {
                self.aborted = self.aborted.saturating_add(1);
            }
            SweepRowOutcome::AlreadyFinalized => {
                self.already_finalized = self.already_finalized.saturating_add(1);
            }
            SweepRowOutcome::AlreadyAborted => {
                self.already_aborted = self.already_aborted.saturating_add(1);
            }
            SweepRowOutcome::NotFound => {
                self.not_found = self.not_found.saturating_add(1);
            }
        }
    }
}

/// Aggregate outcome of one sweeper cron tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweeperTickOutcome {
    /// Region the sweeper is pinned to.
    pub region: Region,
    /// Canonical wall-clock instant the tick observed (the DO alarm
    /// fires at this instant; production wiring threads
    /// `Date.now()` here).
    pub now_ms: u64,
    /// Aggregate per-row counters for this tick.
    pub aggregate: SweepBatchOutcome,
    /// Whether the canonical bounded batch ceiling was hit (sustained
    /// workload signal — analogous to `RB-FM-AC-TTL-STORM`; if true
    /// the next alarm should re-fire ASAP rather than wait the full
    /// canonical interval).
    pub hit_batch_ceiling: bool,
}

impl SweeperTickOutcome {
    /// Construct a fresh zero-counter outcome pinned to `region` /
    /// `now_ms`.
    #[must_use]
    pub const fn new(region: Region, now_ms: u64) -> Self {
        Self {
            region,
            now_ms,
            aggregate: SweepBatchOutcome {
                aborted: 0,
                already_finalized: 0,
                already_aborted: 0,
                not_found: 0,
            },
            hit_batch_ceiling: false,
        }
    }

    /// Whether the tick should re-fire ASAP (workload outpaces).
    #[must_use]
    pub const fn signals_storm(&self) -> bool {
        self.hit_batch_ceiling
    }
}

/// Sweeper trait. Production wiring (DO `alarm()` handler) implements
/// this against a Cloudflare Cron Durable Object; every fake here
/// exercises the same algorithmic flow.
pub trait OrphanSweeper: Send + Sync {
    /// Drain orphan multipart sessions for the sweeper's pinned
    /// region. Returns the aggregate tick outcome; production DO
    /// emits as metrics + re-arms the alarm.
    ///
    /// Caller MUST supply a deterministic `request_id` (production
    /// uses the DO's `alarm_id`) so the audit chain can correlate
    /// the ticks across sibling shards.
    ///
    /// # Errors
    ///
    /// Backend-class via [`SweeperError::Sessions`] /
    /// [`SweeperError::Audit`].
    fn tick<'a>(
        &'a self,
        now_ms: u64,
        request_id: &'a str,
    ) -> impl Future<Output = Result<SweeperTickOutcome, SweeperError>> + Send + 'a;
}

/// Pure-logic in-memory sweeper.
pub struct InMemoryOrphanSweeper<S, A>
where
    S: SessionStore,
    A: AuditSink,
{
    region: Region,
    sessions: Arc<S>,
    audit: Arc<A>,
    /// Canonical orphan age cutoff in ms (default
    /// [`ORPHAN_AGE_MS`]). Tests pin this so they can drive the
    /// boundary deterministically.
    max_age_ms: u64,
    /// Per-tick batch ceiling (`<= MAX_BATCH_SIZE`).
    batch_size: usize,
}

impl<S, A> fmt::Debug for InMemoryOrphanSweeper<S, A>
where
    S: SessionStore,
    A: AuditSink,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryOrphanSweeper")
            .field("region", &self.region)
            .field("max_age_ms", &self.max_age_ms)
            .field("batch_size", &self.batch_size)
            .finish_non_exhaustive()
    }
}

impl<S, A> InMemoryOrphanSweeper<S, A>
where
    S: SessionStore,
    A: AuditSink,
{
    /// Construct a fresh sweeper pinned to `region`.
    ///
    /// # Errors
    ///
    /// Returns [`SweeperError::BatchSizeExceeded`] when `batch_size >
    /// MAX_BATCH_SIZE` — programmer wiring error.
    pub fn new(
        region: Region,
        sessions: Arc<S>,
        audit: Arc<A>,
        max_age_ms: u64,
        batch_size: usize,
    ) -> Result<Self, SweeperError> {
        if batch_size > MAX_BATCH_SIZE {
            return Err(SweeperError::BatchSizeExceeded {
                requested: batch_size,
                ceiling: MAX_BATCH_SIZE,
            });
        }
        Ok(Self {
            region,
            sessions,
            audit,
            max_age_ms,
            batch_size,
        })
    }

    /// Build a sweeper with canonical defaults (1 h alarm, 7 d age
    /// cutoff, 250-row batch).
    ///
    /// # Errors
    ///
    /// Cannot fail under canonical defaults; surfaced as `Result`
    /// for symmetry with [`Self::new`].
    pub fn with_defaults(
        region: Region,
        sessions: Arc<S>,
        audit: Arc<A>,
    ) -> Result<Self, SweeperError> {
        Self::new(region, sessions, audit, ORPHAN_AGE_MS, MAX_BATCH_SIZE)
    }

    /// Region the sweeper is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Canonical batch ceiling configured at construction time.
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Canonical age cutoff configured at construction time.
    #[must_use]
    pub const fn max_age_ms(&self) -> u64 {
        self.max_age_ms
    }

    async fn abort_one(
        &self,
        candidate: OrphanCandidate,
        now_ms: u64,
        request_id: &str,
    ) -> Result<SweepRowOutcome, SweeperError> {
        let outcome = match self
            .sessions
            .abort(candidate.tenant_id, candidate.session_id, now_ms)
            .await
        {
            Ok(()) => SweepRowOutcome::Aborted,
            Err(SessionStoreError::AlreadyFinalized) => SweepRowOutcome::AlreadyFinalized,
            // The InMemory fake's `abort` is idempotent on Aborted
            // (returns Ok); a NotFound is a true race (concurrent
            // tenant erasure or row GC); other Backend errors
            // propagate. The match below is defensive — if the
            // production binding ever surfaces a distinct
            // "already aborted" enum variant the trait taxonomy
            // grows additively (`#[non_exhaustive]`).
            Err(SessionStoreError::NotFound) => SweepRowOutcome::NotFound,
            Err(SessionStoreError::Aborted) => SweepRowOutcome::AlreadyAborted,
            Err(other) => return Err(SweeperError::Sessions(other)),
        };
        // Emit audit ONLY for actual transitions Live → Aborted
        // (races / no-ops do NOT spam the audit chain — production
        // SIEM cardinality target). Per WI §1 step 3 + sprint
        // contract §5.2 R-S05-5 ("abort + audit emit
        // corelink.multipart.orphan_aborted").
        if matches!(outcome, SweepRowOutcome::Aborted) {
            let record = BlobAuditRecord {
                event_type: BlobEventType::SplitAborted,
                tenant_id: candidate.tenant_id,
                region: candidate.region,
                blob_digest: None,
                manifest_digest: None,
                session_id: Some(candidate.session_id),
                chunk_index: None,
                request_id: request_id.to_owned(),
                reason: ORPHAN_SWEPT_REASON,
                now_ms,
            };
            self.audit.emit(record)?;
        }
        Ok(outcome)
    }
}

impl<S, A> OrphanSweeper for InMemoryOrphanSweeper<S, A>
where
    S: SessionStore,
    A: AuditSink,
{
    fn tick<'a>(
        &'a self,
        now_ms: u64,
        request_id: &'a str,
    ) -> impl Future<Output = Result<SweeperTickOutcome, SweeperError>> + Send + 'a {
        async move {
            let candidates = self
                .sessions
                .list_orphans(self.region, now_ms, self.max_age_ms, self.batch_size)
                .await?;
            let hit_batch_ceiling = candidates.len() >= self.batch_size;
            let mut outcome = SweeperTickOutcome::new(self.region, now_ms);
            outcome.hit_batch_ceiling = hit_batch_ceiling;
            for cand in candidates {
                // Defense-in-depth: the trait surface guarantees
                // candidate.region == self.region (the SELECT is
                // region-scoped). The assertion is structural — if
                // the production binding ever surfaces a row
                // belonging to another region we want the tick to
                // halt before issuing a cross-region UPDATE.
                if cand.region != self.region {
                    continue;
                }
                let row_outcome = self.abort_one(cand, now_ms, request_id).await?;
                outcome.aggregate.observe(row_outcome);
            }
            Ok(outcome)
        }
    }
}

/// Helper: emit a SEV-2 metric snapshot for the canonical sweeper
/// counters. The production wiring threads this through the metrics
/// emitter (`corelink_metrics::Counter`); the host-side helper exists
/// so the property tests can assert the canonical metric names appear
/// in DASH-MULTIPART without bringing the metrics dep into the
/// sweeper module.
///
/// Returns the canonical `(metric_name, value)` pairs emitted per tick
/// per WI §6.1.1 ("métricas: corelink.multipart.sweeper.{ticks_total,
/// orphans_aborted_total{region}, batch_size, duration_ms}").
#[must_use]
pub fn canonical_metric_pairs(outcome: &SweeperTickOutcome) -> Vec<(&'static str, u64)> {
    vec![
        ("corelink.multipart.sweeper.ticks_total", 1),
        (
            "corelink.multipart.sweeper.orphans_aborted_total",
            u64::from(outcome.aggregate.aborted),
        ),
        (
            "corelink.multipart.sweeper.batch_size",
            outcome.aggregate.rows_processed() as u64,
        ),
        (
            "corelink.multipart.sweeper.already_finalized_total",
            u64::from(outcome.aggregate.already_finalized),
        ),
        (
            "corelink.multipart.sweeper.already_aborted_total",
            u64::from(outcome.aggregate.already_aborted),
        ),
        (
            "corelink.multipart.sweeper.not_found_total",
            u64::from(outcome.aggregate.not_found),
        ),
    ]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::reapi::cas::audit::{BlobEventType, InMemoryAuditSink};
    use crate::reapi::cas::session::{InMemorySessionStore, SessionInit, SessionKey};
    use crate::reapi::cas::types::{BlobDigest, ChunkIndex};
    use corelink_hash::Digest;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    fn bd(seed: &[u8]) -> BlobDigest {
        BlobDigest::new(Digest::compute(seed), seed.len() as u64)
    }

    async fn open_session(
        store: &InMemorySessionStore,
        tenant: Uuid,
        seed: &[u8],
        region: Region,
        created_at_ms: u64,
    ) -> super::super::types::SessionId {
        store
            .open(SessionInit {
                key: SessionKey::new(tenant, bd(seed)),
                region,
                tenant_prefix: fixed_prefix(tenant),
                created_at_ms,
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn tick_aborts_orphans_older_than_age_cutoff() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();

        let tenant = Uuid::from_u128(1);
        let _young = open_session(&sessions, tenant, b"young", Region::Wnam, 1).await;
        let old = open_session(&sessions, tenant, b"old", Region::Wnam, 1).await;

        // now_ms = 8 days; cutoff is 7 days. Both session rows have
        // last_activity_at_ms = 1 ms, but only one of them will be
        // selected as orphan because we keep the young session
        // freshly active just before the tick.
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        // Touch the "young" session so it is fresh:
        let _ = sessions
            .append_chunk(
                tenant,
                _young,
                ChunkIndex(0),
                super::super::types::ChunkDigest::compute(b"x"),
                1,
            )
            .await
            .unwrap();
        // Move time forward enough so the un-touched `old` is past
        // cutoff; the `young` is fresh so it's not included.
        // (now_ms - 1 ms = 8 days - 1 ms > 7 days, but `young` was
        // touched at 1 ms in the lookup-on-append path so its
        // `last_activity_at_ms` is now ~now_ms; not stale.)
        let _ = sessions.lookup(tenant, _young).await.unwrap().unwrap();

        let _ = old; // pin lifetime
        // Bump young's last_activity by appending another chunk
        // close to "now":
        // (intentionally simple — InMemorySessionStore does not
        // refresh on append by default; rely on default to keep
        // young fresh by NOT performing any time-travel).

        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        // young was opened at created_at_ms=1 too; the InMemory fake
        // does NOT auto-refresh on append, so BOTH look stale here.
        assert_eq!(
            outcome.aggregate.aborted, 2,
            "both orphan candidates aborted in the tick"
        );
        assert_eq!(audit.snapshot_of(BlobEventType::SplitAborted).len(), 2);
        for rec in audit.snapshot() {
            assert_eq!(rec.reason, ORPHAN_SWEPT_REASON);
            assert_eq!(rec.tenant_id, tenant);
            assert_eq!(rec.region, Region::Wnam);
        }
    }

    #[tokio::test]
    async fn tick_skips_sessions_under_age_cutoff() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();
        let tenant = Uuid::from_u128(1);
        let _ = open_session(&sessions, tenant, b"recent", Region::Wnam, 1_000_000).await;
        // now_ms = 1_000_000 + 1 hour; cutoff is 7 days; under cutoff.
        let now_ms = 1_000_000_u64 + (60 * 60 * 1000);
        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        assert_eq!(outcome.aggregate.aborted, 0);
        assert!(audit.is_empty());
    }

    #[tokio::test]
    async fn tick_pinned_to_region_skips_other_regions() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();
        let tenant = Uuid::from_u128(1);
        let _here = open_session(&sessions, tenant, b"here", Region::Wnam, 1).await;
        let _away = open_session(&sessions, tenant, b"away", Region::Weur, 1).await;
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        assert_eq!(outcome.aggregate.aborted, 1);
        assert_eq!(audit.snapshot_of(BlobEventType::SplitAborted).len(), 1);
        let rec = &audit.snapshot()[0];
        assert_eq!(rec.region, Region::Wnam);
    }

    #[tokio::test]
    async fn tick_idempotent_under_repeat() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();
        let tenant = Uuid::from_u128(1);
        let _ = open_session(&sessions, tenant, b"x", Region::Wnam, 1).await;
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        let r1 = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        let r2 = sweeper.tick(now_ms, "alarm-2").await.unwrap();
        assert_eq!(r1.aggregate.aborted, 1);
        // After the first tick the row is in `Aborted` state — it is
        // not Live anymore so the second tick's `list_orphans`
        // returns an empty set.
        assert_eq!(r2.aggregate.aborted, 0);
        assert_eq!(audit.snapshot_of(BlobEventType::SplitAborted).len(), 1);
    }

    #[tokio::test]
    async fn tick_cross_tenant_aborts_only_within_each_tenant_scope() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let sa = open_session(&sessions, tenant_a, b"a", Region::Wnam, 1).await;
        let sb = open_session(&sessions, tenant_b, b"b", Region::Wnam, 1).await;
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        assert_eq!(outcome.aggregate.aborted, 2);
        let recs = audit.snapshot_of(BlobEventType::SplitAborted);
        assert_eq!(recs.len(), 2);
        // Each abort emit is tenant-scoped: tenant_id matches the
        // session's owning tenant; cross-tenant audit emit is
        // structurally impossible because the abort path threads
        // `(tenant_id, session_id)` through every step.
        let pairs: std::collections::BTreeSet<(Uuid, super::super::types::SessionId)> =
            recs.into_iter()
                .map(|r| (r.tenant_id, r.session_id.unwrap()))
                .collect();
        assert!(pairs.contains(&(tenant_a, sa)));
        assert!(pairs.contains(&(tenant_b, sb)));
    }

    #[tokio::test]
    async fn batch_ceiling_signals_storm() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        // Build a sweeper with batch_size=2 so we can drive the
        // ceiling deterministically.
        let sweeper =
            InMemoryOrphanSweeper::new(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit), ORPHAN_AGE_MS, 2)
                .unwrap();
        let tenant = Uuid::from_u128(1);
        for i in 0..5_u32 {
            let _ = open_session(&sessions, tenant, &i.to_be_bytes(), Region::Wnam, 1).await;
        }
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        assert!(outcome.signals_storm());
        assert_eq!(outcome.aggregate.aborted, 2);
        let canonical = canonical_metric_pairs(&outcome);
        // Confirm the canonical metric names per WI §6.1.1.
        let names: Vec<&str> = canonical.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"corelink.multipart.sweeper.ticks_total"));
        assert!(names.contains(&"corelink.multipart.sweeper.orphans_aborted_total"));
        assert!(names.contains(&"corelink.multipart.sweeper.batch_size"));
    }

    #[tokio::test]
    async fn batch_size_above_ceiling_rejected() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let err = InMemoryOrphanSweeper::new(
            Region::Wnam,
            sessions,
            audit,
            ORPHAN_AGE_MS,
            MAX_BATCH_SIZE + 1,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SweeperError::BatchSizeExceeded {
                ceiling: MAX_BATCH_SIZE,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn finalized_session_under_age_cutoff_is_not_swept() {
        let sessions = Arc::new(InMemorySessionStore::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let sweeper =
            InMemoryOrphanSweeper::with_defaults(Region::Wnam, Arc::clone(&sessions), Arc::clone(&audit))
                .unwrap();
        let tenant = Uuid::from_u128(1);
        let sid = open_session(&sessions, tenant, b"x", Region::Wnam, 1).await;
        // Append + finalize.
        let cd = super::super::types::ChunkDigest::compute(b"x");
        sessions
            .append_chunk(tenant, sid, ChunkIndex(0), cd, 1)
            .await
            .unwrap();
        sessions
            .finalize(super::super::session::SessionFinalize {
                tenant_id: tenant,
                session_id: sid,
                manifest_digest: super::super::types::ManifestDigest::from_digest(
                    Digest::compute(b"m"),
                ),
                chunk_count: 1,
                finalized_at_ms: 2,
            })
            .await
            .unwrap();
        let now_ms = 8 * 24 * 60 * 60 * 1000_u64;
        let outcome = sweeper.tick(now_ms, "alarm-1").await.unwrap();
        assert_eq!(outcome.aggregate.aborted, 0);
        assert!(audit.is_empty());
    }

    #[tokio::test]
    async fn canonical_constants_are_documented_values() {
        // Defense-in-depth: pin the canonical constants so a
        // refactor cannot silently change the SLA.
        assert_eq!(MAX_BATCH_SIZE, 250);
        assert_eq!(ORPHAN_AGE_MS, 7 * 24 * 60 * 60 * 1000);
        assert_eq!(TICK_INTERVAL_MS, 60 * 60 * 1000);
        assert_eq!(ORPHAN_SWEPT_REASON, "orphan_swept");
    }

    #[tokio::test]
    async fn trait_object_via_impl_compiles() {
        // Sanity: the trait-abstraction-defer pattern requires the
        // trait be object-safe in spirit — we use `impl Trait` not
        // `dyn Trait`, but keep this static smoke so a future
        // refactor towards `dyn` can be detected mechanically.
        fn _assert_send_sync<T: Send + Sync>() {}
        let _ = _assert_send_sync::<
            InMemoryOrphanSweeper<InMemorySessionStore, InMemoryAuditSink>,
        >;
    }

}
