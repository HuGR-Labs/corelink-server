//! Eviction phase orchestrator — wires `BlobMetaSoftDeleteStore`,
//! `AcReferenceProbe`, `TenantStorageStateStore`, `EvictionAuditSink`,
//! `EvictionMetricsObserver`, and `EvictionClock` into the canonical
//! eviction decision pipeline.
//!
//! ## Phase chain
//!
//! For each (tenant, region) pass:
//!
//! 1. **Capture `evict_started_at_ms`** from the clock BEFORE any scan
//!    (commit-then-scan ordering; mirrors S-06
//!    `mark_started_at_ms` capture per WI §6.1.6).
//! 2. **Read `tenant_storage_state` row** for `(tenant, region)` —
//!    drives the 95% trigger gate.
//! 3. **Quota gate**:
//!    - `bytes_used / bytes_quota < 95%` → emit
//!      `corelink.evict.skipped_quota_ok` + `touch_evict_watermark` +
//!      return `EvictionResult::skipped_quota_ok = 1`.
//!    - `bytes_used / bytes_quota >= 95%` → emit
//!      `corelink.evict.quota_trigger_fired` + proceed to LRU scan.
//! 4. **LRU scan** — list candidates whose `last_accessed_at_ms <
//!    cutoff_ms (= now - tier_ttl_ms)` ordered ASC; bounded by
//!    `MAX_LRU_BATCH_SIZE` (250).
//! 5. **Per-candidate decision** via [`InMemoryEvictionPhase::step_candidate`]:
//!    - **Reachable check** (race-aware strict `<`): if active
//!      reference exists, emit `corelink.evict.skipped_reachable` +
//!      bump `cascade_prevented_total`; surface
//!      [`EvictionDecision::SkipReachable`].
//!    - **TTL guard**: if `last_accessed_at_ms >= cutoff_ms` (within
//!      window), emit `corelink.evict.skipped_ttl`; surface
//!      [`EvictionDecision::SkipTtlNotExpired`].
//!    - **Soft-delete**: emit `corelink.evict.evicted` BEFORE the
//!      UPDATE (fail-closed envelope; production wiring rolls back
//!      D1 batch on emit failure); call
//!      `BlobMetaSoftDeleteStore::soft_delete_for_eviction`; surface
//!      [`EvictionDecision::Evict`] with `bytes_reclaimed`.
//! 6. **Apply storage_state update** atomic with the per-candidate
//!    soft-delete (subtract `bytes_reclaimed` from `bytes_used`,
//!    accumulate to lifetime, set `last_evict_at_ms`).
//! 7. **Phase budget probe** per candidate (5 min p99 @ 100k
//!    candidates per WI §10.s07.002.4 budget; cheaper per-candidate
//!    than per-mutation, mirrors S-06 sweep batched probe pattern).
//!
//! ## Idempotent re-run guard
//!
//! A row whose `deleted_at_ms IS NOT NULL` is treated as
//! [`EvictionDecision::SkipTtlNotExpired`] in the in-memory fake
//! (already resolved); production wiring uses the SQL `WHERE
//! deleted_at_ms IS NULL` predicate to filter at the LRU scan level.

use std::sync::Arc;
use std::sync::Mutex;

use uuid::Uuid;

use crate::audit::{EvictionAuditRecord, EvictionAuditSink, EvictionEventType, EvictionReason};
use crate::blob_meta::{BlobLruRow, BlobMetaSoftDeleteStore, SoftDeleteOutcome};
use crate::error::EvictionError;
use crate::metrics::EvictionMetricsObserver;
use crate::reachable::{AcReferenceProbe, AcReferenceWitness};
use crate::region::EvictionRegion;
use crate::storage_state::TenantStorageStateStore;
use crate::tier::{ttl_for_tier, Tier};

/// Canonical 95% quota trigger threshold (CAP-EVICT-003 boundary).
pub const QUOTA_TRIGGER_THRESHOLD_PCT: f64 = 0.95;

/// Canonical 90% target headroom — eviction reclaims until tenant
/// utilization drops below this.
pub const QUOTA_TARGET_HEADROOM_PCT: f64 = 0.90;

/// Canonical eviction cooldown — once an eviction pass fires for a
/// `(tenant, region)`, the next cron tick within this window is
/// short-circuited. 1 hour mirrors WI §6.1.10 idempotency gate.
pub const EVICTION_COOLDOWN_MS: u64 = 60 * 60 * 1000;

/// Canonical D1 batch cap — eviction LRU scan bounded at 250 rows
/// per pass (Lote 10.5bis lesson).
pub const MAX_LRU_BATCH_SIZE: usize = 250;

/// Canonical eviction phase budget — 5 min p99 @ 100k candidates
/// per WI §10.s07.002.4.
pub const CANONICAL_EVICTION_PHASE_BUDGET_MS: u64 = 5 * 60 * 1000;

// ============================================================================
//  EvictionClock seam.
// ============================================================================

/// Wall-clock seam for the eviction phase. Mirrors
/// `corelink-gc::sweep::SweepClock` — deterministic test seam without
/// binding directly to `Date.now()`.
pub trait EvictionClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;
}

/// Counter-driven [`EvictionClock`] used by tests.
#[derive(Debug)]
pub struct CountingEvictionClock {
    inner: Mutex<u64>,
}

impl CountingEvictionClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl EvictionClock for CountingEvictionClock {
    fn now_ms(&self) -> u64 {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = *g;
        *g = g.saturating_add(1);
        now
    }
}

// ============================================================================
//  EvictionConfig — knobs surfaced by the production binding.
// ============================================================================

/// Knobs driving the eviction phase. Defaults pin canonical values
/// from WI §6.1.10 + spec contract §5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvictionConfig {
    /// Per-tenant tier (resolves to per-tier TTL via
    /// [`ttl_for_tier`]).
    tier: Tier,
    /// Optional admin-override TTL (Enterprise tier only; capped
    /// 730d). When `None`, the canonical default applies.
    enterprise_ttl_override_days: Option<u32>,
    /// Phase budget ceiling.
    phase_budget_ms: u64,
    /// LRU scan batch cap.
    max_lru_batch_size: usize,
}

impl EvictionConfig {
    /// Construct with the canonical default knobs for a tier.
    ///
    /// # Errors
    ///
    /// Returns [`crate::tier::TierTtlOverrideError`] when an
    /// override is supplied for a non-Enterprise tier OR exceeds the
    /// hard cap.
    pub fn new(
        tier: Tier,
        enterprise_ttl_override_days: Option<u32>,
    ) -> Result<Self, crate::tier::TierTtlOverrideError> {
        // Validate the override at construction time so the runtime
        // path never sees an invalid combination.
        let _ = crate::tier::ttl_for_tier_with_override(tier, enterprise_ttl_override_days)?;
        Ok(Self {
            tier,
            enterprise_ttl_override_days,
            phase_budget_ms: CANONICAL_EVICTION_PHASE_BUDGET_MS,
            max_lru_batch_size: MAX_LRU_BATCH_SIZE,
        })
    }

    /// Default config (Free tier; no override).
    #[must_use]
    pub fn default_for_free_tier() -> Self {
        Self {
            tier: Tier::Free,
            enterprise_ttl_override_days: None,
            phase_budget_ms: CANONICAL_EVICTION_PHASE_BUDGET_MS,
            max_lru_batch_size: MAX_LRU_BATCH_SIZE,
        }
    }

    /// Resolved TTL (ms) for the configured tier (honours override).
    #[must_use]
    pub fn ttl_ms(&self) -> u64 {
        match crate::tier::ttl_for_tier_with_override(self.tier, self.enterprise_ttl_override_days)
        {
            Ok(v) => v,
            // The override was validated at construction; this branch
            // is unreachable in practice. Fall back to canonical
            // default.
            Err(_) => ttl_for_tier(self.tier),
        }
    }

    /// Configured tier.
    #[must_use]
    pub const fn tier(&self) -> Tier {
        self.tier
    }

    /// Phase budget (ms).
    #[must_use]
    pub const fn phase_budget_ms(&self) -> u64 {
        self.phase_budget_ms
    }

    /// LRU scan batch cap.
    #[must_use]
    pub const fn max_lru_batch_size(&self) -> usize {
        self.max_lru_batch_size
    }
}

// ============================================================================
//  EvictionDecision + EvictionResult.
// ============================================================================

/// Per-candidate decision produced by
/// [`InMemoryEvictionPhase::step_candidate`].
///
/// `Eq` is intentionally NOT derived because the `SkipQuotaOk` arm
/// carries `utilization_pct: f64` (NaN-aware semantics). Tests use
/// `matches!` + per-field equality where needed.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EvictionDecision {
    /// Soft-delete fired.
    Evict {
        /// Wall-clock instant the soft-delete fired.
        evicted_at_ms: u64,
        /// `blob_meta.size_bytes` of the row just soft-deleted.
        bytes_reclaimed: u64,
        /// First-8-hex prefix of the candidate digest.
        digest_hex8: String,
    },
    /// INV-GC-001 inheritance fires — race-aware reachable check
    /// returned an active reference. Did NOT delete.
    SkipReachable {
        /// Wall-clock instant the skip fired.
        skipped_at_ms: u64,
        /// Anchor (`evict_started_at_ms`).
        evict_started_at_ms: u64,
        /// Forensic witness from the offending `ac_meta` row.
        witness: AcReferenceWitness,
    },
    /// Candidate's `last_accessed_at_ms` is within the per-tier TTL
    /// window (NOT yet expired) OR the row was already soft-deleted.
    SkipTtlNotExpired {
        /// Wall-clock instant the skip fired.
        skipped_at_ms: u64,
        /// `blob_meta.last_accessed_at_ms` observed.
        last_accessed_at_ms: u64,
        /// `cutoff_ms = now - tier_ttl_ms` against which the row
        /// was compared.
        cutoff_ms: u64,
    },
    /// Per-region pass observed `bytes_used / bytes_quota < 95%`;
    /// no eviction fired (CAP-EVICT-003 trigger gate).
    SkipQuotaOk {
        /// Wall-clock instant the skip fired.
        skipped_at_ms: u64,
        /// Observed utilization pct (`bytes_used / bytes_quota`).
        utilization_pct: f64,
    },
}

/// Aggregate outcome of one eviction phase execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvictionResult {
    /// Anchor (`evict_started_at_ms`) the eviction ran against.
    pub evict_started_at_ms: u64,
    /// Total candidates inspected (sum across decisions).
    pub candidates_processed: u64,
    /// Confirmed orphans soft-deleted.
    pub blobs_evicted_count: u64,
    /// INV-GC-001 inheritance fires — sustained spike alert at >5%
    /// (cascade_prevented_total).
    pub cascade_prevented_count: u64,
    /// Skipped — within TTL window OR already resolved.
    pub skipped_ttl_count: u64,
    /// Skipped — tenant below 95% quota AND no TTL pass requested.
    pub skipped_quota_ok_count: u64,
    /// Sum of `blob_meta.size_bytes` across `Evict` decisions.
    pub bytes_reclaimed: u64,
    /// End-to-end eviction duration (ms).
    pub eviction_duration_ms: u64,
    /// Total audit events emitted (one per non-no-op decision).
    pub audit_events_emitted: u64,
    /// Whether the 95% trigger arm fired this pass.
    pub quota_trigger_fired: bool,
}

impl EvictionResult {
    /// Construct a fresh empty result anchored at the eviction-started
    /// instant.
    #[must_use]
    pub const fn empty(evict_started_at_ms: u64) -> Self {
        Self {
            evict_started_at_ms,
            candidates_processed: 0,
            blobs_evicted_count: 0,
            cascade_prevented_count: 0,
            skipped_ttl_count: 0,
            skipped_quota_ok_count: 0,
            bytes_reclaimed: 0,
            eviction_duration_ms: 0,
            audit_events_emitted: 0,
            quota_trigger_fired: false,
        }
    }
}

// ============================================================================
//  EvictionPhase trait + InMemoryEvictionPhase orchestrator.
// ============================================================================

/// Trait surfaced by every eviction phase backend (production CF Cron
/// DO handler / in-memory fake).
pub trait EvictionPhase: Send + Sync + core::fmt::Debug {
    /// Execute the daily-cron eviction pass for `(tenant, region)`.
    /// Wires the canonical phase chain (storage-state read → quota
    /// gate → LRU scan → per-candidate decision → soft-delete +
    /// storage_state apply).
    ///
    /// # Errors
    ///
    /// Surface as [`EvictionError`].
    fn execute_daily(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<EvictionResult, EvictionError>;

    /// Execute the ad-hoc 95% quota-trigger pass for `(tenant,
    /// region)` with an explicit reclaim target. Caller (typically
    /// the WI-S07-003 quota middleware) invokes this via
    /// `worker::send_future()` fire-and-forget.
    ///
    /// # Errors
    ///
    /// Surface as [`EvictionError`].
    fn execute_quota_trigger(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        target_bytes_to_reclaim: u64,
    ) -> Result<EvictionResult, EvictionError>;
}

/// In-memory eviction phase orchestrator (the canonical pure-logic
/// skeleton).
pub struct InMemoryEvictionPhase<B, X, S, A, M, K>
where
    B: BlobMetaSoftDeleteStore,
    X: AcReferenceProbe,
    S: TenantStorageStateStore,
    A: EvictionAuditSink,
    M: EvictionMetricsObserver,
    K: EvictionClock,
{
    blob_meta: Arc<B>,
    ac_probe: Arc<X>,
    storage_state: Arc<S>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: EvictionConfig,
}

impl<B, X, S, A, M, K> core::fmt::Debug for InMemoryEvictionPhase<B, X, S, A, M, K>
where
    B: BlobMetaSoftDeleteStore,
    X: AcReferenceProbe,
    S: TenantStorageStateStore,
    A: EvictionAuditSink,
    M: EvictionMetricsObserver,
    K: EvictionClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryEvictionPhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<B, X, S, A, M, K> InMemoryEvictionPhase<B, X, S, A, M, K>
where
    B: BlobMetaSoftDeleteStore,
    X: AcReferenceProbe,
    S: TenantStorageStateStore,
    A: EvictionAuditSink,
    M: EvictionMetricsObserver,
    K: EvictionClock,
{
    /// Construct with the canonical default config (Free tier).
    pub fn with_defaults(
        blob_meta: Arc<B>,
        ac_probe: Arc<X>,
        storage_state: Arc<S>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            blob_meta,
            ac_probe,
            storage_state,
            audit,
            metrics,
            clock,
            config: EvictionConfig::default_for_free_tier(),
        }
    }

    /// Construct with an explicit config.
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 6 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        blob_meta: Arc<B>,
        ac_probe: Arc<X>,
        storage_state: Arc<S>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: EvictionConfig,
    ) -> Self {
        Self {
            blob_meta,
            ac_probe,
            storage_state,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> EvictionConfig {
        self.config
    }

    /// Process a single candidate row through the eviction decision
    /// pipeline. Visible for property tests so the decision boundary
    /// can be exercised independently of the phase orchestration.
    ///
    /// # Errors
    ///
    /// Surface as [`EvictionError`].
    pub fn step_candidate(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        candidate: &BlobLruRow,
        evict_started_at_ms: u64,
        cutoff_ms: u64,
    ) -> Result<EvictionDecision, EvictionError> {
        // 0. Idempotent guard: already soft-deleted → SkipTtl arm
        // (treats as already-resolved).
        if candidate.deleted_at_ms.is_some() {
            return Ok(EvictionDecision::SkipTtlNotExpired {
                skipped_at_ms: self.clock.now_ms(),
                last_accessed_at_ms: candidate.last_accessed_at_ms,
                cutoff_ms,
            });
        }
        // 1. TTL guard: within window → skip.
        if candidate.last_accessed_at_ms >= cutoff_ms {
            let now = self.clock.now_ms();
            self.audit.emit(EvictionAuditRecord {
                event_type: EvictionEventType::SkippedTtl,
                tenant_id,
                region,
                tier: self.config.tier,
                digest_hex8: candidate.digest.hex8().to_string(),
                bytes: None,
                reason: EvictionReason::SkippedTtl,
                created_by_request_id: "cron".to_string(),
                now_ms: now,
            })?;
            return Ok(EvictionDecision::SkipTtlNotExpired {
                skipped_at_ms: now,
                last_accessed_at_ms: candidate.last_accessed_at_ms,
                cutoff_ms,
            });
        }
        // 2. Race-aware reachable check (strict `<` evict arm).
        let witness = self.ac_probe.find_active_reference(
            tenant_id,
            &candidate.digest,
            evict_started_at_ms,
        )?;
        if let Some(w) = witness {
            let now = self.clock.now_ms();
            // Fail-closed audit BEFORE bumping the metric so a failed
            // emit aborts the per-candidate decision (matching the
            // canonical fail-closed envelope).
            self.audit.emit(EvictionAuditRecord {
                event_type: EvictionEventType::SkippedReachable,
                tenant_id,
                region,
                tier: self.config.tier,
                digest_hex8: candidate.digest.hex8().to_string(),
                bytes: None,
                reason: EvictionReason::SkippedReachable,
                created_by_request_id: "cron".to_string(),
                now_ms: now,
            })?;
            self.metrics.record_cascade_prevented(tenant_id)?;
            return Ok(EvictionDecision::SkipReachable {
                skipped_at_ms: now,
                evict_started_at_ms,
                witness: w,
            });
        }
        // 3. Soft-delete path. Audit BEFORE the UPDATE (fail-closed
        // envelope; production wiring rolls back D1 batch on emit
        // failure). Compute bytes from the candidate row (the lookup
        // already cached size_bytes; no extra D1 read needed).
        let now = self.clock.now_ms();
        self.audit.emit(EvictionAuditRecord {
            event_type: EvictionEventType::Evicted,
            tenant_id,
            region,
            tier: self.config.tier,
            digest_hex8: candidate.digest.hex8().to_string(),
            bytes: Some(candidate.size_bytes),
            reason: EvictionReason::TtlExpired,
            created_by_request_id: "cron".to_string(),
            now_ms: now,
        })?;
        let outcome = self
            .blob_meta
            .soft_delete_for_eviction(tenant_id, &candidate.digest, now)?;
        match outcome {
            SoftDeleteOutcome::Deleted { size_bytes } => {
                self.metrics
                    .record_bytes_reclaimed(tenant_id, self.config.tier, size_bytes)?;
                self.metrics
                    .record_ttl_expired(tenant_id, self.config.tier)?;
                self.metrics.record_lru_evicted(tenant_id)?;
                Ok(EvictionDecision::Evict {
                    evicted_at_ms: now,
                    bytes_reclaimed: size_bytes,
                    digest_hex8: candidate.digest.hex8().to_string(),
                })
            }
            SoftDeleteOutcome::AlreadyResolved => {
                // Concurrent winner already soft-deleted; surface as
                // SkipTtl (already resolved).
                Ok(EvictionDecision::SkipTtlNotExpired {
                    skipped_at_ms: now,
                    last_accessed_at_ms: candidate.last_accessed_at_ms,
                    cutoff_ms,
                })
            }
        }
    }

    /// Internal helper: execute a phase pass with the given quota-gate
    /// behaviour (`enforce_quota_gate = true` for daily cron;
    /// `false` for ad-hoc quota trigger which bypasses the 95%
    /// threshold and reclaims toward the explicit target).
    fn execute_internal(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        enforce_quota_gate: bool,
        explicit_target_bytes: Option<u64>,
    ) -> Result<EvictionResult, EvictionError> {
        // Cron metric.
        self.metrics.record_cron_fired(region)?;

        // Step 1: capture evict_started_at_ms BEFORE the scan.
        let evict_started_at_ms = self.clock.now_ms();
        let mut result = EvictionResult::empty(evict_started_at_ms);

        // Step 2: read storage_state.
        let row_opt = self.storage_state.lookup(tenant_id, region)?;
        let Some(row) = row_opt else {
            return Err(EvictionError::TenantStorageStateMissing {
                tenant_id,
                region: region.as_str(),
            });
        };

        // Step 3: quota gate (daily cron only).
        if enforce_quota_gate && !row.at_or_above_trigger() {
            // SkipQuotaOk arm — emit audit + touch watermark + return.
            let now = self.clock.now_ms();
            self.audit.emit(EvictionAuditRecord {
                event_type: EvictionEventType::SkippedQuotaOk,
                tenant_id,
                region,
                tier: self.config.tier,
                digest_hex8: String::new(),
                bytes: None,
                reason: EvictionReason::SkippedQuotaOk,
                created_by_request_id: "cron".to_string(),
                now_ms: now,
            })?;
            self.storage_state
                .touch_evict_watermark(tenant_id, region, now)?;
            result.skipped_quota_ok_count = 1;
            result.audit_events_emitted = result.audit_events_emitted.saturating_add(1);
            // Phase duration metric.
            let phase_end = self.clock.now_ms();
            let duration = phase_end.saturating_sub(evict_started_at_ms);
            result.eviction_duration_ms = duration;
            self.metrics.record_duration_ms(region, duration)?;
            return Ok(result);
        }

        // The trigger fired — emit canonical event.
        if enforce_quota_gate {
            let now = self.clock.now_ms();
            self.audit.emit(EvictionAuditRecord {
                event_type: EvictionEventType::QuotaTriggerFired,
                tenant_id,
                region,
                tier: self.config.tier,
                digest_hex8: String::new(),
                bytes: explicit_target_bytes
                    .or(Some(crate::trigger::target_bytes_to_reclaim(&row))),
                reason: EvictionReason::QuotaTriggerFired,
                created_by_request_id: "cron".to_string(),
                now_ms: now,
            })?;
            self.metrics.record_quota_trigger_fired(tenant_id)?;
            result.quota_trigger_fired = true;
            result.audit_events_emitted = result.audit_events_emitted.saturating_add(1);
        }

        // Step 4: LRU scan — list candidates whose
        // last_accessed_at_ms < cutoff (TTL expired).
        let ttl_ms = self.config.ttl_ms();
        let cutoff_ms = evict_started_at_ms.saturating_sub(ttl_ms);
        let candidates = self.blob_meta.list_lru_candidates(
            tenant_id,
            cutoff_ms,
            self.config.max_lru_batch_size,
        )?;

        // Bump candidates_scanned metric.
        let n_candidates = candidates.len() as u64;
        if n_candidates > 0 {
            self.metrics
                .record_candidates_scanned(tenant_id, n_candidates)?;
        }

        // Phase budget tracking.
        let deadline_ms = evict_started_at_ms.saturating_add(self.config.phase_budget_ms);

        // Optional reclaim target — when set (ad-hoc quota trigger),
        // the loop short-circuits once `result.bytes_reclaimed >= target`.
        let target = explicit_target_bytes;

        // Step 5..7: per-candidate decision loop.
        for candidate in &candidates {
            // Phase-budget probe per candidate.
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(EvictionError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(evict_started_at_ms),
                    budget_ms: self.config.phase_budget_ms,
                });
            }

            let decision =
                self.step_candidate(tenant_id, region, candidate, evict_started_at_ms, cutoff_ms)?;
            result.candidates_processed = result.candidates_processed.saturating_add(1);
            match decision {
                EvictionDecision::Evict {
                    bytes_reclaimed, ..
                } => {
                    result.blobs_evicted_count = result.blobs_evicted_count.saturating_add(1);
                    result.bytes_reclaimed = result.bytes_reclaimed.saturating_add(bytes_reclaimed);
                    result.audit_events_emitted = result.audit_events_emitted.saturating_add(1);
                    // Apply storage_state reclaim atomic.
                    let reclaim_now = self.clock.now_ms();
                    self.storage_state.apply_eviction_reclaim(
                        tenant_id,
                        region,
                        bytes_reclaimed,
                        reclaim_now,
                    )?;
                    // Short-circuit if explicit target met.
                    if let Some(t) = target {
                        if result.bytes_reclaimed >= t {
                            break;
                        }
                    }
                }
                EvictionDecision::SkipReachable { .. } => {
                    result.cascade_prevented_count =
                        result.cascade_prevented_count.saturating_add(1);
                    result.audit_events_emitted = result.audit_events_emitted.saturating_add(1);
                }
                EvictionDecision::SkipTtlNotExpired { .. } => {
                    result.skipped_ttl_count = result.skipped_ttl_count.saturating_add(1);
                    // Skipped-TTL audit emit handled inside step_candidate.
                    result.audit_events_emitted = result.audit_events_emitted.saturating_add(1);
                }
                EvictionDecision::SkipQuotaOk { .. } => {
                    // Per-candidate SkipQuotaOk should not surface;
                    // the quota gate is per-tenant, not per-candidate.
                    result.skipped_quota_ok_count = result.skipped_quota_ok_count.saturating_add(1);
                }
            }
        }

        // Step 8: phase duration metric.
        let phase_end = self.clock.now_ms();
        let duration = phase_end.saturating_sub(evict_started_at_ms);
        result.eviction_duration_ms = duration;
        self.metrics.record_duration_ms(region, duration)?;

        Ok(result)
    }
}

impl<B, X, S, A, M, K> EvictionPhase for InMemoryEvictionPhase<B, X, S, A, M, K>
where
    B: BlobMetaSoftDeleteStore,
    X: AcReferenceProbe,
    S: TenantStorageStateStore,
    A: EvictionAuditSink,
    M: EvictionMetricsObserver,
    K: EvictionClock,
{
    fn execute_daily(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<EvictionResult, EvictionError> {
        self.execute_internal(tenant_id, region, true, None)
    }

    fn execute_quota_trigger(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        target_bytes_to_reclaim: u64,
    ) -> Result<EvictionResult, EvictionError> {
        // Ad-hoc trigger bypasses the per-tenant quota gate (the
        // caller already verified the tenant is at >= 95%); the loop
        // short-circuits at the explicit target.
        self.execute_internal(tenant_id, region, false, Some(target_bytes_to_reclaim))
    }
}

// ============================================================================
//  Tests (unit + cross-component sanity).
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
    use crate::audit::InMemoryEvictionAuditSink;
    use crate::blob_meta::{EvictionBlobDigest, InMemoryBlobMetaSoftDeleteStore};
    use crate::metrics::InMemoryEvictionMetrics;
    use crate::reachable::InMemoryAcReferenceProbe;
    use crate::storage_state::{InMemoryTenantStorageStateStore, TenantStorageStateRow};

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn dig(seed: u8) -> EvictionBlobDigest {
        let bytes = [seed; 32];
        let mut s = String::with_capacity(64);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        EvictionBlobDigest::new(s).unwrap()
    }

    fn lru_row(seed: u8, last_accessed: u64, size: u64) -> BlobLruRow {
        BlobLruRow {
            digest: dig(seed),
            size_bytes: size,
            created_at_ms: 1,
            last_accessed_at_ms: last_accessed,
            deleted_at_ms: None,
        }
    }

    fn state_row(
        tenant: Uuid,
        region: EvictionRegion,
        used: u64,
        quota: u64,
    ) -> TenantStorageStateRow {
        TenantStorageStateRow {
            tenant_id: tenant,
            region,
            bytes_used: used,
            bytes_quota: quota,
            bytes_used_updated_at_ms: 1,
            last_synced_at_ms: 1,
            last_evict_at_ms: None,
            bytes_reclaimed_lifetime: 0,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    type Fixture = (
        InMemoryEvictionPhase<
            InMemoryBlobMetaSoftDeleteStore,
            InMemoryAcReferenceProbe,
            InMemoryTenantStorageStateStore,
            InMemoryEvictionAuditSink,
            InMemoryEvictionMetrics,
            CountingEvictionClock,
        >,
        Arc<InMemoryBlobMetaSoftDeleteStore>,
        Arc<InMemoryAcReferenceProbe>,
        Arc<InMemoryTenantStorageStateStore>,
        Arc<InMemoryEvictionAuditSink>,
        Arc<InMemoryEvictionMetrics>,
    );

    fn fresh(start_ms: u64, config: EvictionConfig) -> Fixture {
        let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
        let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
        let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
        let audit = Arc::new(InMemoryEvictionAuditSink::new());
        let metrics = Arc::new(InMemoryEvictionMetrics::new());
        let clock = Arc::new(CountingEvictionClock::new(start_ms));
        let phase = InMemoryEvictionPhase::new(
            Arc::clone(&blob_meta),
            Arc::clone(&ac_probe),
            Arc::clone(&storage_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
            config,
        );
        (phase, blob_meta, ac_probe, storage_state, audit, metrics)
    }

    // ---- canonical constants ---------------------------------------

    #[test]
    fn canonical_constants_pinned() {
        assert!((QUOTA_TRIGGER_THRESHOLD_PCT - 0.95).abs() < f64::EPSILON);
        assert!((QUOTA_TARGET_HEADROOM_PCT - 0.90).abs() < f64::EPSILON);
        assert_eq!(EVICTION_COOLDOWN_MS, 60 * 60 * 1000);
        assert_eq!(MAX_LRU_BATCH_SIZE, 250);
        assert_eq!(CANONICAL_EVICTION_PHASE_BUDGET_MS, 5 * 60 * 1000);
    }

    #[test]
    fn config_default_for_free_tier() {
        let c = EvictionConfig::default_for_free_tier();
        assert_eq!(c.tier(), Tier::Free);
        assert_eq!(c.ttl_ms(), 7 * 86_400_000);
        assert_eq!(c.phase_budget_ms(), CANONICAL_EVICTION_PHASE_BUDGET_MS);
        assert_eq!(c.max_lru_batch_size(), MAX_LRU_BATCH_SIZE);
    }

    #[test]
    fn config_enterprise_with_override_validates() {
        let c = EvictionConfig::new(Tier::Enterprise, Some(500)).unwrap();
        assert_eq!(c.ttl_ms(), 500 * 86_400_000);
    }

    #[test]
    fn config_enterprise_above_cap_rejects_at_construction() {
        let err = EvictionConfig::new(Tier::Enterprise, Some(800)).unwrap_err();
        assert!(matches!(
            err,
            crate::tier::TierTtlOverrideError::ExceedsMaxTtl { .. }
        ));
    }

    // ---- step_candidate ---------------------------------------------

    #[test]
    fn step_already_soft_deleted_yields_skip_ttl() {
        let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
        let mut row = lru_row(1, 50, 1024);
        row.deleted_at_ms = Some(800);
        let d = phase
            .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
            .unwrap();
        assert!(matches!(d, EvictionDecision::SkipTtlNotExpired { .. }));
    }

    #[test]
    fn step_within_ttl_window_yields_skip_ttl() {
        let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
        // last_accessed = 6000 >= cutoff = 5000 → skip TTL.
        let row = lru_row(1, 6000, 1024);
        let d = phase
            .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
            .unwrap();
        assert!(matches!(d, EvictionDecision::SkipTtlNotExpired { .. }));
    }

    #[test]
    fn step_reachable_yields_skip_reachable() {
        let (phase, b, x, _s, _a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
        let row = lru_row(1, 100, 1024);
        b.push_row(ten_a(), row.clone()).unwrap();
        // Push an AC ref older than evict_started_at_ms = 10000.
        x.push_ac_row(ten_a(), "ac-1", vec![row.digest.clone()], 5000, None)
            .unwrap();
        let d = phase
            .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
            .unwrap();
        assert!(matches!(d, EvictionDecision::SkipReachable { .. }));
        assert_eq!(
            m.counter_total(crate::metrics::EvictionMetricKind::CascadePrevented),
            1
        );
    }

    #[test]
    fn step_evict_path_emits_audit_and_metrics() {
        let (phase, b, _x, _s, a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
        let row = lru_row(1, 100, 4096);
        b.push_row(ten_a(), row.clone()).unwrap();
        let d = phase
            .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
            .unwrap();
        match d {
            EvictionDecision::Evict {
                bytes_reclaimed, ..
            } => {
                assert_eq!(bytes_reclaimed, 4096);
            }
            other => panic!("expected Evict, got {other:?}"),
        }
        assert_eq!(a.snapshot_of(EvictionEventType::Evicted).len(), 1);
        assert_eq!(
            m.counter_total(crate::metrics::EvictionMetricKind::BytesReclaimed),
            4096
        );
        assert_eq!(
            m.counter_total(crate::metrics::EvictionMetricKind::TtlExpired),
            1
        );
        assert_eq!(
            m.counter_total(crate::metrics::EvictionMetricKind::LruEvicted),
            1
        );
        // blob_meta now soft-deleted.
        let snap = b.snapshot(ten_a(), &row.digest).unwrap();
        assert!(snap.deleted_at_ms.is_some());
    }

    // ---- execute_daily ----------------------------------------------

    #[test]
    fn execute_daily_returns_storage_state_missing() {
        let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
        let err = phase
            .execute_daily(ten_a(), EvictionRegion::Sam)
            .unwrap_err();
        assert!(matches!(
            err,
            EvictionError::TenantStorageStateMissing { .. }
        ));
    }

    #[test]
    fn execute_daily_below_threshold_skips_quota_ok() {
        let (phase, _b, _x, s, a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
        s.push_row(state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let r = phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
        assert_eq!(r.skipped_quota_ok_count, 1);
        assert_eq!(r.blobs_evicted_count, 0);
        assert!(!r.quota_trigger_fired);
        assert_eq!(a.snapshot_of(EvictionEventType::SkippedQuotaOk).len(), 1);
        assert_eq!(
            m.counter_total(crate::metrics::EvictionMetricKind::CronFired),
            1
        );
    }

    #[test]
    fn execute_daily_at_threshold_fires_trigger_and_evicts() {
        // Start clock far enough into the future that `cutoff_ms =
        // now - 7d` is well above 0 (otherwise saturating-sub
        // collapses cutoff to 0 and a row with last_accessed=1
        // doesn't satisfy `< cutoff`).
        let start = 30_u64 * 86_400_000; // 30d in ms
        let (phase, b, _x, s, a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
        // Tenant at 96% quota.
        s.push_row(state_row(ten_a(), EvictionRegion::Sam, 96, 100))
            .unwrap();
        // Push a cold blob: last_accessed = 1 << cutoff (start - 7d).
        let cold = lru_row(1, 1, 30);
        b.push_row(ten_a(), cold).unwrap();
        let r = phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
        assert!(r.quota_trigger_fired);
        assert_eq!(r.blobs_evicted_count, 1);
        assert_eq!(r.bytes_reclaimed, 30);
        // QuotaTriggerFired audit emit.
        assert_eq!(a.snapshot_of(EvictionEventType::QuotaTriggerFired).len(), 1);
        // Storage state row was reclaimed (96 - 30 = 66).
        let row = s.snapshot(ten_a(), EvictionRegion::Sam).unwrap();
        assert_eq!(row.bytes_used, 66);
        assert_eq!(row.bytes_reclaimed_lifetime, 30);
    }

    // ---- execute_quota_trigger --------------------------------------

    #[test]
    fn execute_quota_trigger_short_circuits_at_target() {
        let start = 30_u64 * 86_400_000;
        let (phase, b, _x, s, _a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
        s.push_row(state_row(ten_a(), EvictionRegion::Sam, 200, 100))
            .unwrap();
        // 5 cold blobs of 50 bytes each = 250 reclaimable.
        for i in 0..5 {
            b.push_row(ten_a(), lru_row(i, 1 + u64::from(i), 50))
                .unwrap();
        }
        // Target = 100 bytes — the loop should evict 2 blobs and stop.
        let r = phase
            .execute_quota_trigger(ten_a(), EvictionRegion::Sam, 100)
            .unwrap();
        assert!(r.bytes_reclaimed >= 100);
        // Should NOT have evicted all 5.
        assert!(r.blobs_evicted_count < 5);
    }

    // ---- tenant isolation -------------------------------------------

    #[test]
    fn tenant_isolation_eviction_does_not_touch_other_tenant() {
        let start = 30_u64 * 86_400_000;
        let (phase, b, _x, s, _a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
        s.push_row(state_row(ten_a(), EvictionRegion::Sam, 100, 100))
            .unwrap();
        s.push_row(state_row(ten_b(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        // Cold blob for A.
        let blob_a = lru_row(1, 1, 30);
        b.push_row(ten_a(), blob_a.clone()).unwrap();
        // Cold blob for B (would be eligible if scanned).
        let blob_b = lru_row(2, 1, 30);
        b.push_row(ten_b(), blob_b.clone()).unwrap();
        phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
        // Tenant B's blob is untouched.
        let snap_b = b.snapshot(ten_b(), &blob_b.digest).unwrap();
        assert!(snap_b.deleted_at_ms.is_none());
    }

    // ---- BLOB-only scope --------------------------------------------

    #[test]
    fn blob_only_scope_chunks_table_never_consulted() {
        // The InMemoryEvictionPhase has NO chunks-table dependency.
        // This test pins the wiring contract: the orchestrator's
        // type signature expects only blob_meta + ac_probe +
        // storage_state — adding a chunks-table store would be a
        // BREAKING change visible at compile time.
        let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
        // Sanity: the config is the only field we expose.
        let c = phase.config();
        assert_eq!(c.tier(), Tier::Free);
    }
}
