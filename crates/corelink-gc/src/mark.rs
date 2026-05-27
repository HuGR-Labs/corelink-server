//! Mark phase (WI-S06-002) — multi-pass D1 scan over `blob_meta` +
//! `ac_meta` + `manifest_chunks`; atomic `mark_started_at_ms` capture
//! at phase start (`INV-GC-MARK-STARTED-AT-IMMUTABLE` anchor; canonical
//! TLA `gc_correctness.tla` `MarkPhaseStart` action); reachable-set
//! computation as the union of the 3 sources; non-reachable rows
//! emitted into [`gc_candidates`](crate::MIGRATION_0007_GC_CANDIDATES)
//! for sweep-phase consumption (WI-S06-003).
//!
//! ## Trait-abstraction-defer pattern
//!
//! The production wiring binds the per-region GC Cron Durable Object
//! against:
//!
//! - A real D1 `blob_meta` / `ac_meta` / `manifest_chunks` reader
//!   implementing [`ReachableSetSource`].
//! - A real D1 `gc_candidates` writer implementing
//!   [`GcCandidatesStore`].
//! - A real wall-clock seam implementing [`MarkClock`] (production:
//!   `Date.now()`).
//!
//! This crate ships **only the in-memory pure-logic skeleton** so
//! property tests at 10 k iter exercise every load-bearing invariant
//! (atomicity of the anchor, monotonicity, tenant isolation,
//! reachable-set completeness, batch-bounded scan, jitter accounting,
//! phase budget enforcement) without spinning up miniflare. The real
//! D1 binding lands in WI-S06-007 (PRR ship gate) per the charter
//! `trait-abstraction-defer` pattern.
//!
//! ## Skeleton ↔ canonical TLA action mapping
//!
//! Per WI §1 cripto-driven invariants table:
//!
//! | TLA+ Action (`gc_correctness.tla`) | Rust impl invocation site |
//! |---|---|
//! | `MarkPhaseStart` | [`MarkPhase::execute`] step 1 → [`crate::run::GcRunStore::transition_phase`] with `to = Mark` (atomically captures the anchor via the existing immutable-set semantic) |
//! | `GCMarkStep(blob)` | [`MarkPhase::execute`] step 2 → 3-pass batched scan |
//! | `UpdateActionResult(ac, blob)` | concurrent ac_meta INSERT (out-of-band) |
//! | `SweepStep(blob)` | WI-S06-003 (out of scope here) |
//! | `InvGCReRefProtected` | TLA+ invariant + property test 100k race (WI-S06-006) |
//!
//! The atomic `mark_started_at_ms` capture is enforced by the existing
//! [`crate::run::InMemoryGcRunStore::transition_phase`] guard:
//! `INV-GC-MARK-STARTED-AT-IMMUTABLE` returns
//! [`crate::run::GcRunStoreError::MarkStartedAtImmutable`] on any
//! second-write attempt; the mark phase's idempotent re-run path reads
//! the existing value via [`crate::run::GcRunStore::lookup`] before
//! issuing the transition (mirrors the WI §1 SQL `UPDATE … WHERE
//! mark_started_at_ms IS NULL` semantics — at most one writer succeeds;
//! all subsequent observers read the stable value).

use std::collections::BTreeSet;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::audit::{GcAuditRecord, GcAuditSink, GcAuditSinkError, GcEventType};
use crate::error::GcError;
use crate::metrics::{GcMetricsObserver, GcMetricsObserverError};
use crate::region::GcRegion;
use crate::run::{
    CheckpointDeltas, GcPhase, GcRun, GcRunStore, GcRunStoreError, GcStatus, RunId,
};

/// Embedded canonical migration SQL for `gc_candidates` (WI-S06-002).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. Pinned as a `const` so the
/// canonical-text regression test (`tests/migration_canonical_0007.rs`)
/// turns red on accidental edits.
pub const MIGRATION_0007_GC_CANDIDATES: &str =
    include_str!("../../../migrations/d1/0007_gc_candidates.sql");

// ============================================================================
//  BlobDigest newtype — canonical lower-case hex BLAKE3-256 (64 chars).
// ============================================================================

/// Canonical BLAKE3-256 blob digest in lower-case hex form (64 chars).
///
/// Mirrors the shape stored in `blob_meta.digest` / `ac_meta.blob_refs`
/// / `manifest_chunks.chunk_digest` (TEXT columns; 64-char lower-case
/// hex). Constructor [`BlobDigest::parse`] validates length and
/// alphabet; the inner string is private so a malformed digest cannot
/// reach the persistence layer.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct BlobDigest(String);

impl BlobDigest {
    /// Canonical fixed-length: BLAKE3-256 hex = 32 bytes × 2.
    pub const LEN: usize = 64;

    /// Construct from a borrowed `&str`. Validates length + alphabet.
    ///
    /// # Errors
    ///
    /// Returns [`MarkError::InvalidDigest`] if `raw` is not exactly 64
    /// lower-case hex chars. Upper-case and mixed-case are rejected
    /// because the canonical sink writes lower-case (`format!("{:x}",
    /// hash)` style); accepting upper-case here would create a
    /// canonicalisation hole that bypasses the per-tenant uniqueness
    /// guarantee in `blob_meta` (mixed-case duplicates).
    pub fn parse(raw: &str) -> Result<Self, MarkError> {
        if raw.len() != Self::LEN {
            return Err(MarkError::InvalidDigest {
                actual_len: raw.len(),
            });
        }
        if !raw
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(MarkError::InvalidDigest {
                actual_len: raw.len(),
            });
        }
        Ok(Self(raw.to_owned()))
    }

    /// Borrow the canonical hex form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for BlobDigest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
//  MarkConfig — knobs surfaced by ScheduleConfig in production.
// ============================================================================

/// Canonical D1 batch ceiling per Lote 10.4bis lesson (D1 100 KB
/// envelope limit; 250 rows × ~400 B = 100 KB).
pub const CANONICAL_BATCH_SIZE: u32 = 250;

/// Canonical inter-batch jitter to dodge D1 throttle thundering herd
/// (WI §6.1.6).
pub const CANONICAL_JITTER_MS: u64 = 100;

/// Canonical phase budget @ 1 M blobs (WI §6.1.10 + sprint contract
/// §5.5 SLO-FRESH-GC).
pub const CANONICAL_PHASE_BUDGET_MS: u64 = 10 * 60 * 1000;

/// Knobs driving the mark phase. The defaults pin canonical values from
/// the WI; admin-plane (S-13) exposes the knobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkConfig {
    /// Per-pass batch ceiling (Lote 10.4bis lesson D1 100 KB envelope).
    /// Validated `> 0` and `<= CANONICAL_BATCH_SIZE` at construction.
    batch_size: u32,
    /// Inter-batch jitter (WI §6.1.6 thundering-herd dodge). The fake
    /// does not actually sleep; it accounts the wall-clock advancement
    /// against the budget.
    jitter_ms: u64,
    /// Phase budget; `PhaseBudgetExceeded` if scan duration exceeds.
    phase_budget_ms: u64,
}

impl Default for MarkConfig {
    fn default() -> Self {
        Self {
            batch_size: CANONICAL_BATCH_SIZE,
            jitter_ms: CANONICAL_JITTER_MS,
            phase_budget_ms: CANONICAL_PHASE_BUDGET_MS,
        }
    }
}

impl MarkConfig {
    /// Construct a custom config. Validates `batch_size` is `> 0` and
    /// `<= CANONICAL_BATCH_SIZE` (Lote 10.4bis lesson).
    ///
    /// # Errors
    ///
    /// Returns [`MarkError::InvalidConfig`] if `batch_size` is out of
    /// range.
    pub fn new(batch_size: u32, jitter_ms: u64, phase_budget_ms: u64) -> Result<Self, MarkError> {
        if batch_size == 0 || batch_size > CANONICAL_BATCH_SIZE {
            return Err(MarkError::InvalidConfig {
                reason: "batch_size must be in 1..=CANONICAL_BATCH_SIZE",
            });
        }
        if phase_budget_ms == 0 {
            return Err(MarkError::InvalidConfig {
                reason: "phase_budget_ms must be > 0",
            });
        }
        Ok(Self {
            batch_size,
            jitter_ms,
            phase_budget_ms,
        })
    }

    /// Per-pass batch ceiling.
    #[must_use]
    pub const fn batch_size(self) -> u32 {
        self.batch_size
    }
    /// Inter-batch jitter (ms).
    #[must_use]
    pub const fn jitter_ms(self) -> u64 {
        self.jitter_ms
    }
    /// Phase budget ceiling (ms).
    #[must_use]
    pub const fn phase_budget_ms(self) -> u64 {
        self.phase_budget_ms
    }
}

// ============================================================================
//  Reachable-set source rows (3 passes).
// ============================================================================

/// `blob_meta` row projection used by Pass 1 (live blob enumeration).
/// Mirrors the SQL `SELECT digest, refcount FROM blob_meta WHERE
/// tenant_id = ? AND deleted_at IS NULL ORDER BY digest LIMIT ?
/// OFFSET ?` shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobMetaRow {
    /// Canonical hex digest.
    pub digest: BlobDigest,
    /// Denormalised refcount (reconcile detects drift in WI-S06-005).
    pub refcount: u64,
    /// Blob payload size (bytes; informational; surfaces in
    /// `gc_candidates.blob_size_bytes` for non-reachable rows).
    pub size_bytes: u64,
    /// Last referenced wall-clock instant; informational only — sweep
    /// phase does NOT use this for the protect-if-`>=` decision (the
    /// `mark_started_at_ms` anchor + `ac.created_at` comparison wins
    /// per the canonical TLA semantics).
    pub last_referenced_at_ms: u64,
}

/// `ac_meta` row projection used by Pass 2 (AC outputs scan). Each row
/// surfaces a list of digests the AC envelope references in its
/// outputs array (`json_each(blob_refs)` per the canonical SQL idiom
/// in WI §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcMetaRow {
    /// Digests this AC envelope's outputs reference.
    pub blob_refs: Vec<BlobDigest>,
    /// `ac.created_at` (ms); informational here. Sweep uses it for the
    /// canonical TLA `>=` protect-if-newer decision (WI-S06-003).
    pub created_at_ms: u64,
}

/// `manifest_chunks` row projection used by Pass 3 (multipart manifest
/// chunk enumeration). Each row surfaces one chunk digest the manifest
/// references; the mark phase reaches transitively into `chunks` →
/// `blob` per the WI §1 `manifest_chunks → chunks → blob_digest`
/// 3-hop traversal documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestChunkRow {
    /// Chunk digest the manifest row references.
    pub chunk_digest: BlobDigest,
    /// `manifest_chunks.created_at` (ms); informational.
    pub created_at_ms: u64,
}

/// Trait surfaced by every reachable-set backend (D1 reader / in-memory
/// fake). The 3 methods correspond to the 3 canonical passes in WI §1
/// (blob_meta + ac_meta + manifest_chunks).
///
/// Each method takes `(tenant_id, lower_bound_ms, batch_size, offset)`
/// matching the SQL `WHERE tenant_id = ? AND <ts> >= ? LIMIT ? OFFSET ?`
/// shape. `offset` is the canonical D1 OFFSET form; production wiring
/// migrates to keyset pagination if OFFSET cost regresses (deferred per
/// charter `trait-abstraction-defer`).
pub trait ReachableSetSource: Send + Sync + core::fmt::Debug {
    /// Pass 1 — `blob_meta` live blobs (refcount > 0 AND deleted_at IS
    /// NULL). Returns an empty Vec when no more rows; caller stops
    /// when `len() < batch_size`.
    ///
    /// # Errors
    ///
    /// Backend transport errors (D1 throttle / connection / parse).
    fn pass_blob_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<BlobMetaRow>, MarkError>;

    /// Pass 2 — `ac_meta` outputs (json_each canonical idiom per WI §1
    /// Lote 10.6bis P0-1+P0-2 fix).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn pass_ac_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<AcMetaRow>, MarkError>;

    /// Pass 3 — `manifest_chunks` (multipart 3-hop traversal per WI §1).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn pass_manifest_chunks(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<ManifestChunkRow>, MarkError>;
}

// ============================================================================
//  GcCandidate + status + GcCandidatesStore trait.
// ============================================================================

/// Canonical 4-state status for a `gc_candidates` row. Mark phase
/// inserts with `Candidate`; sweep transitions to `Swept`; physical
/// delete transitions to `PhysicallyDeleted`; sweep also transitions
/// to `ProtectedReRef` when the canonical TLA `>=` protect-if-newer
/// check fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CandidateStatus {
    /// Mark phase inserted this row; sweep pending.
    Candidate,
    /// Sweep phase soft-deleted (WI-S06-003).
    Swept,
    /// Physical delete purged the row (WI-S06-004).
    PhysicallyDeleted,
    /// INV-GC-004 protect-if-`>=` fired (sweep phase).
    ProtectedReRef,
}

impl CandidateStatus {
    /// Canonical lower-snake-case literal matching the SQL CHECK list.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Swept => "swept",
            Self::PhysicallyDeleted => "physically_deleted",
            Self::ProtectedReRef => "protected_re_ref",
        }
    }
}

/// Materialised `gc_candidates` row.
///
/// WI-S06-003 added the `swept_at_ms` / `protected_at_ms` /
/// `protected_reason` fields mirroring the SQL columns of the same
/// name (set by sweep phase via [`GcCandidatesStore::transition_status`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcCandidate {
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Canonical hex digest.
    pub digest: BlobDigest,
    /// INV-GC-004 anchor (denormalised from `gc_run.mark_started_at_ms`).
    pub mark_started_at_ms: u64,
    /// gc_run FK.
    pub mark_run_id: RunId,
    /// Informational.
    pub blob_size_bytes: u64,
    /// Informational.
    pub blob_last_referenced_at_ms: u64,
    /// Status (initially `Candidate`).
    pub status: CandidateStatus,
    /// Created wall-clock instant.
    pub created_at_ms: u64,
    /// Set when sweep phase soft-deletes (`status = Swept`). WI-S06-003.
    pub swept_at_ms: Option<u64>,
    /// Set when sweep phase records protection (`status =
    /// ProtectedReRef`). WI-S06-003.
    pub protected_at_ms: Option<u64>,
    /// Forensic trail set when status flips to `ProtectedReRef`
    /// (e.g. `"ac.created_at = T+1; mark_started_at = T"`). WI-S06-003.
    pub protected_reason: Option<String>,
}

/// Trait surfaced by every `gc_candidates` backend.
///
/// WI-S06-003 added the [`GcCandidatesStore::transition_status`] +
/// [`GcCandidatesStore::lookup`] surfaces so the sweep phase can
/// (a) atomically flip a row from `Candidate` → `Swept` /
/// `ProtectedReRef` and (b) re-read the row to drive idempotent
/// re-execution. The `transition_status` method enforces a monotone
/// status graph at the in-memory seam mirroring the SQL `UPDATE …
/// WHERE status = 'candidate'` idempotent semantic.
pub trait GcCandidatesStore: Send + Sync + core::fmt::Debug {
    /// Insert a fresh candidate row. Idempotent on the composite PK
    /// `(tenant_id, digest, mark_run_id)` — re-inserting the same key
    /// is a no-op (mirrors `INSERT ... ON CONFLICT DO NOTHING`).
    ///
    /// # Errors
    ///
    /// Backend transport errors / CHECK violation.
    fn insert_candidate(&self, candidate: GcCandidate) -> Result<(), MarkError>;

    /// Snapshot every candidate row for `(tenant_id, mark_run_id)`
    /// (used by tests + diagnostic surfaces). Production wiring uses
    /// `SELECT ... WHERE mark_run_id = ? AND status = 'candidate'` to
    /// drive sweep.
    fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        mark_run_id: RunId,
    ) -> Result<Vec<GcCandidate>, MarkError>;

    /// Total candidate row count for `(tenant_id, mark_run_id)`.
    fn count_for_run(&self, tenant_id: Uuid, mark_run_id: RunId) -> Result<u64, MarkError>;

    /// Lookup a single candidate row by composite PK (tenant-scoped).
    /// Returns `Ok(None)` when the row does not exist or belongs to a
    /// different tenant (Layer 4 envelope).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
    ) -> Result<Option<GcCandidate>, MarkError>;

    /// Atomically transition a candidate row's `status` field. The
    /// transition fires only when the row's *current* status equals
    /// `from_status` (mirrors the SQL `UPDATE … SET status = ? …
    /// WHERE … AND status = ?` idempotent semantic).
    ///
    /// Returns `true` when the transition fired; `false` when the row
    /// is absent OR the row's current status does not match
    /// `from_status` (idempotent re-run path).
    ///
    /// `now_ms` populates the matching lifecycle timestamp column
    /// (`swept_at_ms` for `Swept`; `protected_at_ms` for
    /// `ProtectedReRef`); `protected_reason` is set on the
    /// `ProtectedReRef` arm only.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    #[allow(
        clippy::too_many_arguments,
        reason = "trait method mirrors the SQL UPDATE … SET … WHERE … shape \
                  (composite PK 3 cols + from/to status + now_ms + protected_reason); \
                  collapsing into a struct param hurts call-site readability + adds \
                  a per-call allocation in the hot sweep loop."
    )]
    fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
        from_status: CandidateStatus,
        to_status: CandidateStatus,
        now_ms: u64,
        protected_reason: Option<String>,
    ) -> Result<bool, MarkError>;
}

// ============================================================================
//  MarkClock seam — wall-clock injection.
// ============================================================================

/// Wall-clock seam for the mark phase. Production wiring binds
/// `Date.now()`; tests bind a deterministic counter.
pub trait MarkClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;

    /// Account `ms` of jitter (production: actual sleep; fake:
    /// advances the internal counter).
    fn advance_jitter(&self, ms: u64);
}

// ============================================================================
//  MarkResult + MarkError taxonomy.
// ============================================================================

/// Outcome of one mark phase execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkResult {
    /// `INV-GC-MARK-STARTED-AT-IMMUTABLE` anchor — the wall-clock
    /// instant the phase committed in `gc_run.mark_started_at_ms`.
    pub mark_started_at_ms: u64,
    /// Total rows visited across the 3 passes.
    pub rows_scanned_count: u64,
    /// Distinct reachable digests (the union of the 3 pass outputs
    /// minus any digest absent from `blob_meta`).
    pub reachable_blobs_count: u64,
    /// Distinct candidate digests (blob_meta rows whose digest is NOT
    /// in the reachable union).
    pub candidates_count: u64,
    /// End-to-end mark phase wall-clock duration (ms).
    pub mark_duration_ms: u64,
    /// Total batches processed (across the 3 passes).
    pub batches_processed: u32,
}

/// Canonical [`MarkPhase`] error taxonomy.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MarkError {
    /// Caller attempted to construct a [`BlobDigest`] from a string
    /// that is not 64 lower-case hex chars.
    #[error("invalid blob digest: actual_len={actual_len} (canonical=64)")]
    InvalidDigest {
        /// Length the caller supplied.
        actual_len: usize,
    },
    /// Caller supplied an invalid [`MarkConfig`] (e.g. `batch_size = 0`
    /// or `> CANONICAL_BATCH_SIZE`; `phase_budget_ms = 0`).
    #[error("invalid mark config: {reason}")]
    InvalidConfig {
        /// Human-readable reason.
        reason: &'static str,
    },
    /// Phase budget exceeded (WI §6.1.10 + sprint contract §5.5 SLO).
    #[error("phase budget exceeded: duration_ms={duration_ms} > budget_ms={budget_ms}")]
    PhaseBudgetExceeded {
        /// Duration observed at the moment the budget was checked.
        duration_ms: u64,
        /// Budget ceiling.
        budget_ms: u64,
    },
    /// `gc_run` row backend / CHECK violation.
    #[error(transparent)]
    RunStore(#[from] GcRunStoreError),
    /// Audit emission failure — fail-closed per WI §14 (mapped to 503
    /// at the handler level).
    #[error(transparent)]
    Audit(#[from] GcAuditSinkError),
    /// Metrics emission failure (rare; production wiring may downgrade
    /// to log-and-continue).
    #[error(transparent)]
    Metrics(#[from] GcMetricsObserverError),
    /// Backend transport failure (D1 throttle / source / candidates
    /// store).
    #[error("backend error: {0}")]
    Backend(String),
}

impl From<MarkError> for GcError {
    fn from(err: MarkError) -> Self {
        match err {
            MarkError::RunStore(e) => GcError::RunStore(e),
            MarkError::Audit(e) => GcError::Audit(e),
            MarkError::Metrics(e) => GcError::Metrics(e),
            other => GcError::RunStore(GcRunStoreError::Backend(other.to_string())),
        }
    }
}

// ============================================================================
//  MarkPhase trait + InMemoryMarkPhase impl.
// ============================================================================

/// Trait surfaced by every mark phase backend (production CF Cron DO
/// handler / in-memory fake).
pub trait MarkPhase: Send + Sync + core::fmt::Debug {
    /// Execute the mark phase end-to-end for the given `(run_id,
    /// tenant, region)`. Atomically captures `mark_started_at_ms`
    /// (idempotent on re-run), runs the 3-pass scan, computes the
    /// reachable union, inserts candidates, emits audit + metrics.
    ///
    /// # Errors
    ///
    /// Surface as [`MarkError`].
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<MarkResult, MarkError>;
}

/// In-memory mark phase orchestrator (the canonical pure-logic
/// skeleton). Composes the trait dependencies declared at construction
/// time.
pub struct InMemoryMarkPhase<S, R, C, A, M, K>
where
    S: GcRunStore,
    R: ReachableSetSource,
    C: GcCandidatesStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: MarkClock,
{
    runs: std::sync::Arc<S>,
    source: std::sync::Arc<R>,
    candidates: std::sync::Arc<C>,
    audit: std::sync::Arc<A>,
    metrics: std::sync::Arc<M>,
    clock: std::sync::Arc<K>,
    config: MarkConfig,
    /// Snapshot lower bound — production wiring uses
    /// `mark_started_at_ms - 24h_grace`; tests pass `0` so every row is
    /// in scope.
    snapshot_grace_ms: u64,
}

impl<S, R, C, A, M, K> core::fmt::Debug for InMemoryMarkPhase<S, R, C, A, M, K>
where
    S: GcRunStore,
    R: ReachableSetSource,
    C: GcCandidatesStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: MarkClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryMarkPhase")
            .field("config", &self.config)
            .field("snapshot_grace_ms", &self.snapshot_grace_ms)
            .finish_non_exhaustive()
    }
}

impl<S, R, C, A, M, K> InMemoryMarkPhase<S, R, C, A, M, K>
where
    S: GcRunStore,
    R: ReachableSetSource,
    C: GcCandidatesStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: MarkClock,
{
    /// Construct a mark phase orchestrator with the canonical defaults.
    pub fn with_defaults(
        runs: std::sync::Arc<S>,
        source: std::sync::Arc<R>,
        candidates: std::sync::Arc<C>,
        audit: std::sync::Arc<A>,
        metrics: std::sync::Arc<M>,
        clock: std::sync::Arc<K>,
    ) -> Self {
        Self {
            runs,
            source,
            candidates,
            audit,
            metrics,
            clock,
            config: MarkConfig::default(),
            snapshot_grace_ms: 24 * 60 * 60 * 1000,
        }
    }

    /// Construct with an explicit [`MarkConfig`] + snapshot grace.
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 6 trait deps + 2 knobs; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: std::sync::Arc<S>,
        source: std::sync::Arc<R>,
        candidates: std::sync::Arc<C>,
        audit: std::sync::Arc<A>,
        metrics: std::sync::Arc<M>,
        clock: std::sync::Arc<K>,
        config: MarkConfig,
        snapshot_grace_ms: u64,
    ) -> Self {
        Self {
            runs,
            source,
            candidates,
            audit,
            metrics,
            clock,
            config,
            snapshot_grace_ms,
        }
    }

    /// Atomically capture `mark_started_at_ms` (idempotent re-run
    /// safety). Mirrors the WI §1 SQL `UPDATE gc_run SET
    /// mark_started_at_ms = ? WHERE run_id = ? AND mark_started_at_ms
    /// IS NULL` semantic via the existing
    /// [`crate::run::InMemoryGcRunStore`] guard (returns
    /// [`GcRunStoreError::MarkStartedAtImmutable`] on re-write).
    fn capture_mark_anchor(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
    ) -> Result<(GcRun, u64), MarkError> {
        let row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(MarkError::RunStore(GcRunStoreError::NotFound(run_id)))?;
        if let Some(existing) = row.mark_started_at_ms {
            // Idempotent re-run: anchor already set; preserve.
            return Ok((row, existing));
        }
        // The transition_phase enforces the immutable-set semantic and
        // INV-GC-PHASE-MONOTONIC at the same seam.
        let now = self.clock.now_ms().max(row.last_checkpoint_at_ms);
        if row.phase != GcPhase::Idle {
            // Re-enter mark mid-resume without violating the monotonic
            // guard: only Idle → Mark is legal; production wiring must
            // call execute() before any other transition. Tests always
            // start at Idle.
            return Err(MarkError::RunStore(
                GcRunStoreError::InvalidPhaseTransition {
                    from: row.phase.as_str(),
                    to: GcPhase::Mark.as_str(),
                },
            ));
        }
        self.runs
            .transition_phase(run_id, tenant_id, GcPhase::Mark, now)?;
        let row_after = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(MarkError::RunStore(GcRunStoreError::NotFound(run_id)))?;
        let anchor = row_after.mark_started_at_ms.ok_or_else(|| {
            MarkError::RunStore(GcRunStoreError::Backend(
                "mark_started_at_ms unset post-transition".to_owned(),
            ))
        })?;
        Ok((row_after, anchor))
    }

    /// Walk a single pass across batches, accumulating into the caller
    /// supplied folder. Stops when a partial batch (less than
    /// `batch_size`) is observed.
    ///
    /// Returns `(rows_scanned, batches_processed, deadline_check_err)`.
    /// On `PhaseBudgetExceeded`, the partial accumulation is preserved
    /// so the caller can decide on the failure mode (we surface the
    /// error via `Result`).
    fn walk_pass<F, T>(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        deadline_ms: u64,
        mut fetch: F,
        accumulator: &mut Vec<T>,
    ) -> Result<(u64, u32), MarkError>
    where
        F: FnMut(Uuid, u64, u32, u32) -> Result<Vec<T>, MarkError>,
    {
        let mut offset: u32 = 0;
        let mut batches: u32 = 0;
        let mut rows_scanned: u64 = 0;
        loop {
            // Inter-batch jitter (skip on the very first batch).
            if batches > 0 {
                self.clock.advance_jitter(self.config.jitter_ms);
            }
            let now = self.clock.now_ms();
            if now > deadline_ms {
                return Err(MarkError::PhaseBudgetExceeded {
                    duration_ms: now.saturating_sub(deadline_ms.saturating_sub(self.config.phase_budget_ms)),
                    budget_ms: self.config.phase_budget_ms,
                });
            }
            let batch = fetch(tenant_id, snapshot_lower_bound_ms, self.config.batch_size, offset)?;
            let batch_len_u32 = u32::try_from(batch.len()).unwrap_or(u32::MAX);
            rows_scanned = rows_scanned.saturating_add(u64::from(batch_len_u32));
            batches = batches.saturating_add(1);
            let stop = batch_len_u32 < self.config.batch_size;
            for row in batch {
                accumulator.push(row);
            }
            if stop {
                break;
            }
            offset = offset.saturating_add(batch_len_u32);
            if offset == u32::MAX {
                break;
            }
        }
        Ok((rows_scanned, batches))
    }
}

impl<S, R, C, A, M, K> MarkPhase for InMemoryMarkPhase<S, R, C, A, M, K>
where
    S: GcRunStore,
    R: ReachableSetSource,
    C: GcCandidatesStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: MarkClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<MarkResult, MarkError> {
        // 1. Atomically capture mark_started_at_ms (idempotent re-run).
        let (run_row, mark_anchor) = self.capture_mark_anchor(run_id, tenant_id)?;
        if run_row.region != region {
            return Err(MarkError::RunStore(GcRunStoreError::Backend(format!(
                "region mismatch: run.region={} caller.region={}",
                run_row.region.as_str(),
                region.as_str()
            ))));
        }
        // Audit emit phase-start.
        let phase_start_now = self.clock.now_ms();
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::PhaseTransitioned,
            run_id,
            tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::Idle),
            to_phase: Some(GcPhase::Mark),
            created_by_request_id: run_row.created_by_request_id.clone(),
            reason: "mark_phase_started",
            now_ms: phase_start_now,
        })?;
        // 2. Phase budget deadline.
        let deadline_ms = phase_start_now.saturating_add(self.config.phase_budget_ms);
        let snapshot_lower_bound_ms = mark_anchor.saturating_sub(self.snapshot_grace_ms);
        // 3. Pass 1 — blob_meta scan (live blobs).
        let mut blob_rows: Vec<BlobMetaRow> = Vec::new();
        let (p1_rows, p1_batches) = self.walk_pass(
            tenant_id,
            snapshot_lower_bound_ms,
            deadline_ms,
            |t, lb, bs, off| self.source.pass_blob_meta(t, lb, bs, off),
            &mut blob_rows,
        )?;
        // 4. Pass 2 — ac_meta scan (outputs union).
        let mut ac_rows: Vec<AcMetaRow> = Vec::new();
        let (p2_rows, p2_batches) = self.walk_pass(
            tenant_id,
            snapshot_lower_bound_ms,
            deadline_ms,
            |t, lb, bs, off| self.source.pass_ac_meta(t, lb, bs, off),
            &mut ac_rows,
        )?;
        // 5. Pass 3 — manifest_chunks scan.
        let mut manifest_rows: Vec<ManifestChunkRow> = Vec::new();
        let (p3_rows, p3_batches) = self.walk_pass(
            tenant_id,
            snapshot_lower_bound_ms,
            deadline_ms,
            |t, lb, bs, off| self.source.pass_manifest_chunks(t, lb, bs, off),
            &mut manifest_rows,
        )?;
        // 6. Compute reachable union (Pass 1 live blobs + Pass 2 ac_meta
        //    outputs + Pass 3 manifest chunks). Live blob_meta rows
        //    with refcount > 0 are already reachable (Pass 1 filtered
        //    on refcount > 0 at the source); we still union everything
        //    to be defensive — false reachable acceptable, false orphan
        //    catastrophic per WI §9.6.
        let mut reachable: BTreeSet<BlobDigest> = BTreeSet::new();
        for row in &blob_rows {
            if row.refcount > 0 {
                reachable.insert(row.digest.clone());
            }
        }
        for row in &ac_rows {
            for d in &row.blob_refs {
                reachable.insert(d.clone());
            }
        }
        for row in &manifest_rows {
            reachable.insert(row.chunk_digest.clone());
        }
        // 7. Compute candidates: blob_meta rows whose digest is NOT in
        //    the reachable union. Defensive: only digests that physically
        //    exist (Pass 1 saw them) can become candidates — sweep cannot
        //    delete what was never observed.
        let now_for_insert = self.clock.now_ms();
        let mut candidates_count: u64 = 0;
        for row in &blob_rows {
            if reachable.contains(&row.digest) {
                continue;
            }
            self.candidates.insert_candidate(GcCandidate {
                tenant_id,
                digest: row.digest.clone(),
                mark_started_at_ms: mark_anchor,
                mark_run_id: run_id,
                blob_size_bytes: row.size_bytes,
                blob_last_referenced_at_ms: row.last_referenced_at_ms,
                status: CandidateStatus::Candidate,
                created_at_ms: now_for_insert,
                swept_at_ms: None,
                protected_at_ms: None,
                protected_reason: None,
            })?;
            candidates_count = candidates_count.saturating_add(1);
        }
        // 8. Checkpoint counters (blobs_marked_count = reachable
        //    digests observed by mark; idempotent per WI §6.1.5).
        let phase_end_now = self.clock.now_ms();
        let reachable_count = u64::try_from(reachable.len()).unwrap_or(u64::MAX);
        let total_rows_scanned = p1_rows.saturating_add(p2_rows).saturating_add(p3_rows);
        let total_batches = p1_batches.saturating_add(p2_batches).saturating_add(p3_batches);
        let duration_ms = phase_end_now.saturating_sub(phase_start_now);
        self.runs.checkpoint(
            run_id,
            tenant_id,
            phase_end_now,
            CheckpointDeltas {
                blobs_marked_delta: reachable_count,
                ..Default::default()
            },
        )?;
        // 9. Metrics: phase_duration_ms histogram + reachable / candidates
        //    counters (we piggy-back on the canonical 6 metrics with the
        //    Mark phase tag).
        self.metrics
            .record_phase_duration_ms(GcPhase::Mark, tenant_id, region, duration_ms)?;
        // 10. Audit emit phase-completed.
        //
        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification
        // (audit-ordering-high-risk-seal §Escalation 2, 2026-05-27):
        // this emit fires AFTER `candidates.insert_candidate` (loop
        // line 887), `runs.checkpoint` (line 909), and
        // `metrics.record_phase_duration_ms` (line 921) because the
        // payload (`reachable_count`, `candidates_count`,
        // `duration_ms`, `phase_end_now`) is computed FROM those
        // mutations. The two-event (intent + outcome) pattern is
        // satisfied by the `PhaseStarted` emit at line 815, which
        // fires BEFORE any mutation and is the canonical fail-CLOSED
        // envelope (mirrors drata `Sent` post-fix pattern in
        // `specs/_audits/2026-05-27-drata-fail-closed-fix.md` +
        // `corelink-replica-worker::ReplicationStarted`). If this
        // emit fails the orchestrator surfaces the audit error;
        // the started-pair invariant ensures observability of the
        // run-was-attempted regardless.
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::PhaseTransitioned,
            run_id,
            tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::Mark),
            to_phase: Some(GcPhase::Mark),
            created_by_request_id: run_row.created_by_request_id,
            reason: "mark_phase_completed",
            now_ms: phase_end_now,
        })?;
        Ok(MarkResult {
            mark_started_at_ms: mark_anchor,
            rows_scanned_count: total_rows_scanned,
            reachable_blobs_count: reachable_count,
            candidates_count,
            mark_duration_ms: duration_ms,
            batches_processed: total_batches,
        })
    }
}

// ============================================================================
//  In-memory fakes — ReachableSetSource + GcCandidatesStore + MarkClock.
// ============================================================================

/// In-memory [`ReachableSetSource`] fake. Each tenant carries 3
/// independent buffers (mirrors per-tenant SQL filtering).
#[derive(Debug, Default)]
pub struct InMemoryReachableSetSource {
    inner: Mutex<ReachableSetSourceInner>,
}

#[derive(Debug, Default)]
struct ReachableSetSourceInner {
    blob_meta: std::collections::BTreeMap<Uuid, Vec<BlobMetaRow>>,
    ac_meta: std::collections::BTreeMap<Uuid, Vec<AcMetaRow>>,
    manifest_chunks: std::collections::BTreeMap<Uuid, Vec<ManifestChunkRow>>,
}

impl InMemoryReachableSetSource {
    /// Construct an empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a `blob_meta` row for the given tenant. Tests build the
    /// corpus row-by-row.
    pub fn push_blob_meta(&self, tenant_id: Uuid, row: BlobMetaRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.blob_meta.entry(tenant_id).or_default().push(row);
    }

    /// Push an `ac_meta` row.
    pub fn push_ac_meta(&self, tenant_id: Uuid, row: AcMetaRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.ac_meta.entry(tenant_id).or_default().push(row);
    }

    /// Push a `manifest_chunks` row.
    pub fn push_manifest_chunk(&self, tenant_id: Uuid, row: ManifestChunkRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.manifest_chunks.entry(tenant_id).or_default().push(row);
    }

    fn slice<T: Clone>(rows: &[T], offset: u32, batch_size: u32) -> Vec<T> {
        let off = offset as usize;
        if off >= rows.len() {
            return Vec::new();
        }
        let end = off
            .saturating_add(batch_size as usize)
            .min(rows.len());
        rows.get(off..end).map_or_else(Vec::new, <[T]>::to_vec)
    }
}

impl ReachableSetSource for InMemoryReachableSetSource {
    fn pass_blob_meta(
        &self,
        tenant_id: Uuid,
        _snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<BlobMetaRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g.blob_meta.get(&tenant_id).cloned().unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }

    fn pass_ac_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<AcMetaRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g
            .ac_meta
            .get(&tenant_id)
            .map(|r| {
                r.iter()
                    .filter(|row| row.created_at_ms >= snapshot_lower_bound_ms)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }

    fn pass_manifest_chunks(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<ManifestChunkRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g
            .manifest_chunks
            .get(&tenant_id)
            .map(|r| {
                r.iter()
                    .filter(|row| row.created_at_ms >= snapshot_lower_bound_ms)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }
}

/// In-memory [`GcCandidatesStore`] fake. Mirrors the composite PK
/// `(tenant_id, digest, mark_run_id)` semantics — duplicate INSERT is
/// a no-op.
#[derive(Debug, Default)]
pub struct InMemoryGcCandidatesStore {
    inner: Mutex<std::collections::BTreeMap<(Uuid, BlobDigest, RunId), GcCandidate>>,
}

impl InMemoryGcCandidatesStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (diagnostic).
    #[must_use]
    pub fn snapshot(&self) -> Vec<GcCandidate> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.values().cloned().collect()
    }
}

impl GcCandidatesStore for InMemoryGcCandidatesStore {
    fn insert_candidate(&self, candidate: GcCandidate) -> Result<(), MarkError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        let key = (
            candidate.tenant_id,
            candidate.digest.clone(),
            candidate.mark_run_id,
        );
        // Idempotent: composite PK conflict is a no-op (mirrors `INSERT
        // ... ON CONFLICT DO NOTHING`).
        g.entry(key).or_insert(candidate);
        Ok(())
    }

    fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        mark_run_id: RunId,
    ) -> Result<Vec<GcCandidate>, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.values()
            .filter(|c| c.tenant_id == tenant_id && c.mark_run_id == mark_run_id)
            .cloned()
            .collect())
    }

    fn count_for_run(&self, tenant_id: Uuid, mark_run_id: RunId) -> Result<u64, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.values()
            .filter(|c| c.tenant_id == tenant_id && c.mark_run_id == mark_run_id)
            .count() as u64)
    }

    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
    ) -> Result<Option<GcCandidate>, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.get(&(tenant_id, digest.clone(), mark_run_id)).cloned())
    }

    fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
        from_status: CandidateStatus,
        to_status: CandidateStatus,
        now_ms: u64,
        protected_reason: Option<String>,
    ) -> Result<bool, MarkError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        let Some(row) = g.get_mut(&(tenant_id, digest.clone(), mark_run_id)) else {
            return Ok(false);
        };
        // Defense-in-depth Layer 4 envelope (composite PK already binds
        // tenant_id, but a smuggled tenant_id should fail-closed).
        if row.tenant_id != tenant_id {
            return Err(MarkError::Backend(
                "cross_tenant_candidate: row tenant_id mismatch".to_owned(),
            ));
        }
        // Idempotent guard — only fire when current status matches.
        if row.status != from_status {
            return Ok(false);
        }
        row.status = to_status;
        match to_status {
            CandidateStatus::Swept => {
                row.swept_at_ms = Some(now_ms);
            }
            CandidateStatus::ProtectedReRef => {
                row.protected_at_ms = Some(now_ms);
                row.protected_reason = protected_reason;
            }
            CandidateStatus::PhysicallyDeleted | CandidateStatus::Candidate => {
                // PhysicallyDeleted is set by WI-S06-004; re-arming
                // Candidate is a programmer error but the trait surface
                // does not forbid it.
            }
        }
        Ok(true)
    }
}

/// Deterministic counter [`MarkClock`] for tests + property tests.
/// Each [`MarkClock::now_ms`] read advances the internal counter by 1
/// ms (mirrors per-batch wall-clock progression). Jitter accounting
/// adds the supplied ms verbatim.
#[derive(Debug)]
pub struct CountingMarkClock {
    inner: Mutex<u64>,
}

impl CountingMarkClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl MarkClock for CountingMarkClock {
    fn now_ms(&self) -> u64 {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = *g;
        *g = g.saturating_add(1);
        now
    }

    fn advance_jitter(&self, ms: u64) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = g.saturating_add(ms);
    }
}

// ============================================================================
//  Tests
// ============================================================================

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

    use std::sync::Arc;

    use crate::audit::InMemoryGcAuditSink;
    use crate::metrics::InMemoryGcMetrics;
    use crate::run::InMemoryGcRunStore;

    fn digest(seed: u8) -> BlobDigest {
        // 64-char lower-case hex; varies the first byte so each seed is
        // distinct.
        let mut s = format!("{seed:02x}");
        s.push_str(&"0".repeat(BlobDigest::LEN - 2));
        BlobDigest::parse(&s).unwrap()
    }

    type Fixture = (
        InMemoryMarkPhase<
            InMemoryGcRunStore,
            InMemoryReachableSetSource,
            InMemoryGcCandidatesStore,
            InMemoryGcAuditSink,
            InMemoryGcMetrics,
            CountingMarkClock,
        >,
        Arc<InMemoryGcRunStore>,
        Arc<InMemoryReachableSetSource>,
        Arc<InMemoryGcCandidatesStore>,
        Arc<InMemoryGcAuditSink>,
    );

    fn fresh(start_ms: u64) -> Fixture {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let source = Arc::new(InMemoryReachableSetSource::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingMarkClock::new(start_ms));
        let mark = InMemoryMarkPhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&source),
            Arc::clone(&candidates),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        (mark, runs, source, candidates, audit)
    }

    fn seed_running(runs: &InMemoryGcRunStore, rid: RunId, tenant: Uuid, region: GcRegion) {
        runs.insert_pending(rid, tenant, region, 100, "cron".into())
            .unwrap();
        runs.acquire_running(rid, tenant, 200).unwrap();
    }

    #[test]
    fn blob_digest_parse_canonical() {
        let raw = "0".repeat(BlobDigest::LEN);
        let d = BlobDigest::parse(&raw).unwrap();
        assert_eq!(d.as_str(), &raw);
        assert_eq!(format!("{d}"), raw);
    }

    #[test]
    fn blob_digest_rejects_wrong_length() {
        let raw = "0".repeat(63);
        let err = BlobDigest::parse(&raw).unwrap_err();
        assert!(matches!(err, MarkError::InvalidDigest { actual_len: 63 }));
    }

    #[test]
    fn blob_digest_rejects_uppercase() {
        let mut raw = "0".repeat(BlobDigest::LEN - 1);
        raw.insert(0, 'A');
        let err = BlobDigest::parse(&raw).unwrap_err();
        assert!(matches!(err, MarkError::InvalidDigest { .. }));
    }

    #[test]
    fn blob_digest_rejects_non_hex() {
        let mut raw = "0".repeat(BlobDigest::LEN - 1);
        raw.push('z');
        assert!(BlobDigest::parse(&raw).is_err());
    }

    #[test]
    fn mark_config_default_canonical() {
        let cfg = MarkConfig::default();
        assert_eq!(cfg.batch_size(), CANONICAL_BATCH_SIZE);
        assert_eq!(cfg.jitter_ms(), CANONICAL_JITTER_MS);
        assert_eq!(cfg.phase_budget_ms(), CANONICAL_PHASE_BUDGET_MS);
    }

    #[test]
    fn mark_config_rejects_zero_batch() {
        assert!(MarkConfig::new(0, 100, 60_000).is_err());
    }

    #[test]
    fn mark_config_rejects_oversize_batch() {
        assert!(MarkConfig::new(CANONICAL_BATCH_SIZE + 1, 100, 60_000).is_err());
    }

    #[test]
    fn mark_config_rejects_zero_budget() {
        assert!(MarkConfig::new(100, 100, 0).is_err());
    }

    #[test]
    #[allow(
        clippy::const_is_empty,
        reason = "regression guard: catches an accidental clear of \
                  MIGRATION_0007_GC_CANDIDATES even when the const is, by \
                  construction, the embedded SQL bytes."
    )]
    fn migration_const_is_non_empty_and_versioned() {
        assert!(!MIGRATION_0007_GC_CANDIDATES.is_empty());
        assert!(MIGRATION_0007_GC_CANDIDATES.contains("migration 0007"));
        assert!(MIGRATION_0007_GC_CANDIDATES.contains("CREATE TABLE IF NOT EXISTS gc_candidates"));
    }

    #[test]
    fn happy_path_zero_blobs_zero_candidates() {
        let (mark, runs, _src, candidates, audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.rows_scanned_count, 0);
        assert_eq!(result.reachable_blobs_count, 0);
        assert_eq!(result.candidates_count, 0);
        // Mark anchor captured.
        let row = runs.lookup(rid, tenant).unwrap().unwrap();
        assert!(row.mark_started_at_ms.is_some());
        assert_eq!(result.mark_started_at_ms, row.mark_started_at_ms.unwrap());
        // No candidate rows for an empty corpus.
        assert_eq!(candidates.snapshot().len(), 0);
        // Audit emit: phase_started + phase_completed.
        let evts = audit.snapshot_of(GcEventType::PhaseTransitioned);
        assert_eq!(evts.len(), 2);
        assert_eq!(evts[0].reason, "mark_phase_started");
        assert_eq!(evts[1].reason, "mark_phase_completed");
    }

    #[test]
    fn live_blob_via_blob_meta_refcount_is_reachable() {
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        // 1 live blob (refcount = 1); reachable.
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(1),
                refcount: 1,
                size_bytes: 1024,
                last_referenced_at_ms: 500,
            },
        );
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.rows_scanned_count, 1);
        assert_eq!(result.reachable_blobs_count, 1);
        assert_eq!(result.candidates_count, 0);
        assert_eq!(candidates.snapshot().len(), 0);
    }

    #[test]
    fn orphan_blob_becomes_candidate() {
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        // 1 orphan blob (refcount = 0).
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(2),
                refcount: 0,
                size_bytes: 2048,
                last_referenced_at_ms: 500,
            },
        );
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.reachable_blobs_count, 0);
        assert_eq!(result.candidates_count, 1);
        let snap = candidates.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].digest, digest(2));
        assert_eq!(snap[0].status, CandidateStatus::Candidate);
        assert_eq!(snap[0].blob_size_bytes, 2048);
        assert_eq!(snap[0].mark_run_id, rid);
        assert_eq!(snap[0].mark_started_at_ms, result.mark_started_at_ms);
    }

    #[test]
    fn ac_meta_outputs_protect_blob_with_zero_refcount() {
        // Blob B with refcount=0 in blob_meta but referenced in ac_meta
        // outputs MUST be reachable (defense-in-depth: 3-pass union).
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(3),
                refcount: 0,
                size_bytes: 1024,
                last_referenced_at_ms: 500,
            },
        );
        source.push_ac_meta(
            tenant,
            AcMetaRow {
                blob_refs: vec![digest(3)],
                created_at_ms: 600,
            },
        );
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.reachable_blobs_count, 1);
        assert_eq!(result.candidates_count, 0);
        assert_eq!(candidates.snapshot().len(), 0);
    }

    #[test]
    fn manifest_chunks_protect_blob_with_zero_refcount() {
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(4),
                refcount: 0,
                size_bytes: 1024,
                last_referenced_at_ms: 500,
            },
        );
        source.push_manifest_chunk(
            tenant,
            ManifestChunkRow {
                chunk_digest: digest(4),
                created_at_ms: 700,
            },
        );
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.reachable_blobs_count, 1);
        assert_eq!(result.candidates_count, 0);
        assert_eq!(candidates.snapshot().len(), 0);
    }

    #[test]
    fn idempotent_re_run_preserves_mark_anchor() {
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        source.push_blob_meta(
            tenant,
            BlobMetaRow {
                digest: digest(5),
                refcount: 0,
                size_bytes: 256,
                last_referenced_at_ms: 500,
            },
        );
        let r1 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        // Re-run: anchor preserved.
        let r2 = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(r1.mark_started_at_ms, r2.mark_started_at_ms);
        // Idempotent insert: still 1 candidate (composite PK conflict
        // is no-op).
        assert_eq!(candidates.snapshot().len(), 1);
    }

    #[test]
    fn cross_tenant_isolation() {
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid_a = RunId(uuid::Uuid::from_u128(1));
        let rid_b = RunId(uuid::Uuid::from_u128(2));
        let ta = uuid::Uuid::from_u128(10);
        let tb = uuid::Uuid::from_u128(20);
        seed_running(&runs, rid_a, ta, GcRegion::Sam);
        seed_running(&runs, rid_b, tb, GcRegion::Sam);
        // Tenant A: 1 orphan.
        source.push_blob_meta(
            ta,
            BlobMetaRow {
                digest: digest(6),
                refcount: 0,
                size_bytes: 1,
                last_referenced_at_ms: 500,
            },
        );
        // Tenant B: 1 reachable.
        source.push_blob_meta(
            tb,
            BlobMetaRow {
                digest: digest(7),
                refcount: 1,
                size_bytes: 1,
                last_referenced_at_ms: 500,
            },
        );
        let ra = mark.execute(rid_a, ta, GcRegion::Sam).unwrap();
        let rb = mark.execute(rid_b, tb, GcRegion::Sam).unwrap();
        assert_eq!(ra.candidates_count, 1);
        assert_eq!(rb.candidates_count, 0);
        // A's candidate is per-tenant; B sees none.
        let a_snap = candidates.snapshot_for_run(ta, rid_a).unwrap();
        let b_snap = candidates.snapshot_for_run(tb, rid_b).unwrap();
        assert_eq!(a_snap.len(), 1);
        assert_eq!(b_snap.len(), 0);
        // Cross-tenant snapshot returns empty.
        let cross = candidates.snapshot_for_run(tb, rid_a).unwrap();
        assert!(cross.is_empty());
    }

    #[test]
    fn region_mismatch_rejected() {
        let (mark, runs, _src, _cand, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        // Caller passes Iad — mismatch.
        let err = mark.execute(rid, tenant, GcRegion::Iad).unwrap_err();
        assert!(matches!(err, MarkError::RunStore(_)));
    }

    #[test]
    fn run_not_found_rejected() {
        let (mark, _runs, _src, _cand, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(99));
        let tenant = uuid::Uuid::from_u128(42);
        let err = mark.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(
            err,
            MarkError::RunStore(GcRunStoreError::NotFound(_))
        ));
    }

    #[test]
    fn cross_tenant_lookup_rejected() {
        let (mark, runs, _src, _cand, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let owner = uuid::Uuid::from_u128(42);
        let attacker = uuid::Uuid::from_u128(43);
        seed_running(&runs, rid, owner, GcRegion::Sam);
        // Attacker tenant lookup yields NotFound (lookup returns
        // Ok(None) on cross-tenant per Layer 4 envelope; mark surfaces
        // it as NotFound so the trace is unambiguous).
        let err = mark.execute(rid, attacker, GcRegion::Sam).unwrap_err();
        assert!(matches!(
            err,
            MarkError::RunStore(GcRunStoreError::NotFound(_))
        ));
    }

    #[test]
    fn batched_scan_visits_every_row() {
        // Batch size 3 + 7 rows → 3 batches (3+3+1).
        let (mark, runs, source, candidates, _audit) = {
            let runs = Arc::new(InMemoryGcRunStore::new());
            let source = Arc::new(InMemoryReachableSetSource::new());
            let candidates = Arc::new(InMemoryGcCandidatesStore::new());
            let audit = Arc::new(InMemoryGcAuditSink::new());
            let metrics = Arc::new(InMemoryGcMetrics::new());
            let clock = Arc::new(CountingMarkClock::new(1_000));
            let cfg = MarkConfig::new(3, 1, 60_000).unwrap();
            let mark = InMemoryMarkPhase::new(
                Arc::clone(&runs),
                Arc::clone(&source),
                Arc::clone(&candidates),
                Arc::clone(&audit),
                Arc::clone(&metrics),
                clock,
                cfg,
                0,
            );
            (mark, runs, source, candidates, audit)
        };
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        for i in 0..7u8 {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest(i + 10),
                    refcount: 0,
                    size_bytes: u64::from(i),
                    last_referenced_at_ms: 500,
                },
            );
        }
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.rows_scanned_count, 7); // pass1=7 + pass2=0 + pass3=0
        // Pass1 batches: 3 (3+3+1 → stop on partial).
        // Pass2 batches: 1 (empty → stop immediately).
        // Pass3 batches: 1 (empty → stop immediately).
        assert_eq!(result.batches_processed, 5);
        assert_eq!(result.candidates_count, 7);
        assert_eq!(candidates.snapshot().len(), 7);
    }

    #[test]
    fn phase_budget_exceeded_surfaces_error() {
        // Tiny budget + jitter so the second batch trips the deadline.
        let runs = Arc::new(InMemoryGcRunStore::new());
        let source = Arc::new(InMemoryReachableSetSource::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingMarkClock::new(1_000));
        let cfg = MarkConfig::new(1, 100_000, 50).unwrap();
        let mark = InMemoryMarkPhase::new(
            Arc::clone(&runs),
            Arc::clone(&source),
            Arc::clone(&candidates),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
            cfg,
            0,
        );
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        for i in 0..5u8 {
            source.push_blob_meta(
                tenant,
                BlobMetaRow {
                    digest: digest(i + 30),
                    refcount: 1,
                    size_bytes: 1,
                    last_referenced_at_ms: 500,
                },
            );
        }
        let err = mark.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, MarkError::PhaseBudgetExceeded { .. }));
    }

    #[test]
    fn no_mark_after_tombstone_orphans_only() {
        // Property: if blob_meta has 0 rows but ac_meta references a
        // digest, mark MUST NOT emit that digest as a candidate (no
        // physical row → cannot delete).
        let (mark, runs, source, candidates, _audit) = fresh(1_000);
        let rid = RunId(uuid::Uuid::from_u128(1));
        let tenant = uuid::Uuid::from_u128(42);
        seed_running(&runs, rid, tenant, GcRegion::Sam);
        source.push_ac_meta(
            tenant,
            AcMetaRow {
                blob_refs: vec![digest(50)],
                created_at_ms: 600,
            },
        );
        let result = mark.execute(rid, tenant, GcRegion::Sam).unwrap();
        // ac_meta surfaced 1 reachable digest; blob_meta empty so no
        // candidate. Sweep cannot delete what mark never observed.
        assert_eq!(result.reachable_blobs_count, 1);
        assert_eq!(result.candidates_count, 0);
        assert_eq!(candidates.snapshot().len(), 0);
    }

    #[test]
    fn candidate_status_canonical_strings() {
        assert_eq!(CandidateStatus::Candidate.as_str(), "candidate");
        assert_eq!(CandidateStatus::Swept.as_str(), "swept");
        assert_eq!(
            CandidateStatus::PhysicallyDeleted.as_str(),
            "physically_deleted"
        );
        assert_eq!(
            CandidateStatus::ProtectedReRef.as_str(),
            "protected_re_ref"
        );
    }
}
