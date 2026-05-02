//! Atomic CAS quota checker — the canonical hard-block path
//! (CAP-QUOTA-001 100% boundary; per ADR-0020 FROZEN).
//!
//! ## Decision pipeline (per request)
//!
//! For each authenticated `(tenant_id, region, request_bytes, now_ms,
//! now_secs)`:
//!
//! 1. **Idempotent zero-byte check** — if `request_bytes == 0`, return
//!    [`QuotaCasDecision::Allow`] immediately without consulting state
//!    (read-path passthrough; pinned by `prop_idempotent_zero_byte_check`).
//! 2. **CAS attempt loop** (bounded by `config.max_cas_attempts`,
//!    canonical 3):
//!    - **Snapshot read** of the tenant's state via
//!      [`AtomicCasState::lookup`].
//!    - **Boundary check**: race-aware strict-< predicate
//!      `bytes_used + request_bytes < bytes_quota * hard_block_pct`
//!      (canonical 1.0). If predicate fails, fire the canonical 100%
//!      hard-block deny arm:
//!      - emit [`QuotaCasEventType::CasDenied429HardBlock`] audit
//!        BEFORE state mutation (fail-closed envelope; mirror S-07
//!        sprint-close P1-1 fix);
//!      - emit [`QuotaCasEventType::CasRetryAfterEmitted`]
//!        informational record;
//!      - emit [`crate::metrics::QuotaCasResultLabel::Deny`] +
//!        denial counter + retry-after histogram;
//!      - return [`QuotaCasDecision::Deny429`] with canonical
//!        `retry_after_secs = days_until_month_reset_secs(now_secs)`.
//!    - **Conditional write** via
//!      [`AtomicCasState::try_commit_delta`] using the snapshot's
//!      `cas_version`. On success:
//!      - emit [`QuotaCasEventType::CasCheckPassed`] +
//!        [`QuotaCasEventType::CasCommitSucceeded`] audits;
//!      - emit [`crate::metrics::QuotaCasResultLabel::Allow`];
//!      - return [`QuotaCasDecision::Allow`].
//!    - **VersionMismatch** → emit
//!      [`QuotaCasEventType::CasRaceDetected`] audit + bump race
//!      counter + retry (next loop iteration).
//! 3. **Retry-loop exhaustion** → return
//!    [`QuotaCasError::CasRaceExhausted`]. Production wiring maps
//!    this to 503 (transient; client retries with idempotency key).
//!
//! ## Why audit BEFORE write
//!
//! Mirror of WI-S07 P1-1 fix (S-07 sprint-close): emitting the audit
//! AFTER the state mutation creates a window where a successful write
//! is followed by a failed audit emit, leaving an audit gap that the
//! client never sees. The canonical pattern is `lookup → audit emit
//! → state mutate`; audit failure rolls back the orchestration. The
//! orchestrator surfaces [`QuotaCasError::Audit`] which the production
//! wiring maps to 503 so the audit gap doesn't surface as 200.
//!
//! ## F-001 closure
//!
//! The orchestrator state lives in a per-instance `Arc<dyn ...>` /
//! per-instance `Mutex` envelope (NOT a process-global
//! `static LazyLock<Mutex<>>`). Tests instantiate fresh orchestrators
//! per case so the harness cannot accidentally leak state.

use std::sync::Arc;

use uuid::Uuid;

use corelink_eviction::EvictionRegion;

use crate::audit::{
    QuotaCasAuditRecord, QuotaCasAuditSink, QuotaCasEventType,
};
use crate::config::QuotaCasConfig;
use crate::error::QuotaCasError;
use crate::metrics::{
    QuotaCasMetricsObserver, QuotaCasResultLabel,
};
use crate::retry_after::days_until_month_reset_secs;
use crate::state::{AtomicCasState, AtomicCasStateError};

/// Canonical per-request decision arm.
///
/// `Eq` is intentionally NOT derived because the `Allow` arm carries
/// `utilization_pct: f64` (NaN-aware semantics). Tests use `matches!`
/// + per-field equality where needed.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum QuotaCasDecision {
    /// CAS predicate held; bytes_used updated atomically.
    Allow {
        /// Wall-clock instant the decision was rendered.
        decided_at_ms: u64,
        /// Bytes consumed AFTER the commit (`bytes_used + request_bytes`).
        bytes_used_after: u64,
        /// Quota ceiling.
        bytes_quota: u64,
        /// `bytes_used_after / bytes_quota` (defensive 0.0 when
        /// `bytes_quota == 0`).
        utilization_pct: f64,
        /// CAS version observed at successful write.
        cas_version_after: u64,
    },
    /// Canonical 100% hard-block 429 + Retry-After arm fired
    /// (per ADR-0020 FROZEN; supersedes S-07 PROVISIONAL).
    Deny429 {
        /// Wall-clock instant the deny fired.
        decided_at_ms: u64,
        /// Bytes the request would have consumed.
        would_use: u64,
        /// Quota ceiling.
        bytes_quota: u64,
        /// Canonical Retry-After value (seconds; canonical
        /// days-until-month-reset semantic per ADR-0020 FROZEN).
        retry_after_secs: u64,
    },
}

/// Side-effect payload returned alongside the [`QuotaCasDecision`].
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaCasOutcome {
    /// The rendered decision.
    pub decision: QuotaCasDecision,
    /// CAS attempt count (1-indexed) — `1` on first-pass success;
    /// higher when race-detection retries fired.
    pub cas_attempts: u32,
}

/// Trait surfaced by every quota-CAS backend (production CF DO
/// singleton + Tower layer / in-memory orchestrator).
pub trait AtomicQuotaChecker: Send + Sync + core::fmt::Debug {
    /// Render the per-request canonical hard-block decision.
    ///
    /// `now_ms` is the wall-clock instant the request was admitted at
    /// the Tower layer (Unix epoch ms; used in audit records).
    /// `now_secs` is the same instant in seconds (Unix epoch; used by
    /// [`days_until_month_reset_secs`] to compute the canonical
    /// Retry-After value).
    ///
    /// # Errors
    ///
    /// Surface as [`QuotaCasError`].
    fn try_acquire(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        request_bytes: u64,
        now_ms: u64,
        now_secs: i64,
    ) -> Result<QuotaCasOutcome, QuotaCasError>;
}

/// In-memory orchestrator wired to dependencies above.
pub struct InMemoryAtomicQuotaChecker<S, A, M>
where
    S: AtomicCasState,
    A: QuotaCasAuditSink,
    M: QuotaCasMetricsObserver,
{
    state: Arc<S>,
    audit: Arc<A>,
    metrics: Arc<M>,
    config: QuotaCasConfig,
}

impl<S, A, M> core::fmt::Debug for InMemoryAtomicQuotaChecker<S, A, M>
where
    S: AtomicCasState,
    A: QuotaCasAuditSink,
    M: QuotaCasMetricsObserver,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryAtomicQuotaChecker")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, A, M> InMemoryAtomicQuotaChecker<S, A, M>
where
    S: AtomicCasState,
    A: QuotaCasAuditSink,
    M: QuotaCasMetricsObserver,
{
    /// Construct with canonical default config.
    pub fn with_defaults(state: Arc<S>, audit: Arc<A>, metrics: Arc<M>) -> Self {
        Self {
            state,
            audit,
            metrics,
            config: QuotaCasConfig::canonical(),
        }
    }

    /// Construct with explicit config.
    pub fn new(
        state: Arc<S>,
        audit: Arc<A>,
        metrics: Arc<M>,
        config: QuotaCasConfig,
    ) -> Self {
        Self {
            state,
            audit,
            metrics,
            config,
        }
    }

    /// Snapshot the orchestrator's config.
    #[must_use]
    pub const fn config(&self) -> &QuotaCasConfig {
        &self.config
    }

    /// Effective ceiling for the boundary predicate. Computed as
    /// `floor(bytes_quota × hard_block_pct)`. Defensive on NaN /
    /// out-of-range pct (handled by [`QuotaCasConfig::new`] clamp).
    #[must_use]
    fn effective_ceiling(&self, bytes_quota: u64) -> u64 {
        let pct = self.config.hard_block_pct();
        if pct >= 1.0 {
            return bytes_quota;
        }
        // Bounded multiplication via f64; inputs already clamped to
        // [0.5, 1.0] by config.
        let scaled = (bytes_quota as f64) * pct;
        // Floor to u64 (defensive on NaN / Inf / negative — already
        // ruled out by config clamp).
        if !scaled.is_finite() || scaled < 0.0 {
            bytes_quota
        } else {
            scaled.floor() as u64
        }
    }
}

impl<S, A, M> AtomicQuotaChecker for InMemoryAtomicQuotaChecker<S, A, M>
where
    S: AtomicCasState,
    A: QuotaCasAuditSink,
    M: QuotaCasMetricsObserver,
{
    fn try_acquire(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        request_bytes: u64,
        now_ms: u64,
        now_secs: i64,
    ) -> Result<QuotaCasOutcome, QuotaCasError> {
        // Step 1: idempotent zero-byte check (read path / no-op write).
        if request_bytes == 0 {
            // Snapshot for the bytes_used / quota / version fields.
            let row = self
                .state
                .lookup(tenant_id, region)?
                .ok_or(QuotaCasError::TenantStorageStateMissing {
                    tenant_id,
                    region: region.as_str(),
                })?;
            // Audit emit BEFORE returning (fail-closed envelope).
            self.audit.emit(QuotaCasAuditRecord {
                event_type: QuotaCasEventType::CasCheckPassed,
                tenant_id,
                region,
                bytes: Some(0),
                cas_version: Some(row.cas_version),
                cas_attempt: Some(1),
                retry_after_secs: None,
                created_by_request_id: "test".to_string(),
                now_ms,
            })?;
            self.metrics.record_check(QuotaCasResultLabel::Allow)?;
            self.metrics
                .record_check_duration_us(QuotaCasResultLabel::Allow, 0)?;
            let utilization_pct = if row.bytes_quota == 0 {
                0.0
            } else {
                (row.bytes_used as f64) / (row.bytes_quota as f64)
            };
            return Ok(QuotaCasOutcome {
                decision: QuotaCasDecision::Allow {
                    decided_at_ms: now_ms,
                    bytes_used_after: row.bytes_used,
                    bytes_quota: row.bytes_quota,
                    utilization_pct,
                    cas_version_after: row.cas_version,
                },
                cas_attempts: 1,
            });
        }

        let max_attempts = self.config.max_cas_attempts();
        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);

            // Step 2.a: snapshot read.
            let row_opt = self.state.lookup(tenant_id, region)?;
            let Some(row) = row_opt else {
                return Err(QuotaCasError::TenantStorageStateMissing {
                    tenant_id,
                    region: region.as_str(),
                });
            };

            let ceiling = self.effective_ceiling(row.bytes_quota);

            // Step 2.b: race-aware strict-< predicate.
            let would_use = row.bytes_used.checked_add(request_bytes).ok_or(
                QuotaCasError::RequestBytesOverflow {
                    bytes_used: row.bytes_used,
                    request_bytes,
                },
            )?;

            if would_use >= ceiling {
                // Hard-block 429 arm.
                let retry_after = days_until_month_reset_secs(now_secs);
                // Audit emit BEFORE any state mutation (fail-closed
                // envelope; this branch does not mutate state but
                // mirrors the canonical pattern for symmetric reasoning).
                self.audit.emit(QuotaCasAuditRecord {
                    event_type: QuotaCasEventType::CasDenied429HardBlock,
                    tenant_id,
                    region,
                    bytes: Some(would_use),
                    cas_version: Some(row.cas_version),
                    cas_attempt: Some(attempt),
                    retry_after_secs: Some(retry_after),
                    created_by_request_id: "test".to_string(),
                    now_ms,
                })?;
                self.audit.emit(QuotaCasAuditRecord {
                    event_type: QuotaCasEventType::CasRetryAfterEmitted,
                    tenant_id,
                    region,
                    bytes: None,
                    cas_version: None,
                    cas_attempt: None,
                    retry_after_secs: Some(retry_after),
                    created_by_request_id: "test".to_string(),
                    now_ms,
                })?;
                self.metrics.record_check(QuotaCasResultLabel::Deny)?;
                self.metrics.record_denial(tenant_id)?;
                self.metrics.record_retry_after_secs(retry_after)?;
                self.metrics
                    .record_check_duration_us(QuotaCasResultLabel::Deny, 0)?;
                return Ok(QuotaCasOutcome {
                    decision: QuotaCasDecision::Deny429 {
                        decided_at_ms: now_ms,
                        would_use,
                        bytes_quota: row.bytes_quota,
                        retry_after_secs: retry_after,
                    },
                    cas_attempts: attempt,
                });
            }

            // Step 2.c: conditional write.
            // Audit emit BEFORE state mutation (fail-closed envelope).
            self.audit.emit(QuotaCasAuditRecord {
                event_type: QuotaCasEventType::CasCheckPassed,
                tenant_id,
                region,
                bytes: Some(request_bytes),
                cas_version: Some(row.cas_version),
                cas_attempt: Some(attempt),
                retry_after_secs: None,
                created_by_request_id: "test".to_string(),
                now_ms,
            })?;

            let commit_result = self.state.try_commit_delta(
                tenant_id,
                region,
                row.cas_version,
                request_bytes,
            );
            match commit_result {
                Ok(new_state) => {
                    self.audit.emit(QuotaCasAuditRecord {
                        event_type: QuotaCasEventType::CasCommitSucceeded,
                        tenant_id,
                        region,
                        bytes: Some(request_bytes),
                        cas_version: Some(new_state.cas_version),
                        cas_attempt: Some(attempt),
                        retry_after_secs: None,
                        created_by_request_id: "test".to_string(),
                        now_ms,
                    })?;
                    self.metrics.record_check(QuotaCasResultLabel::Allow)?;
                    self.metrics.record_check_duration_us(
                        QuotaCasResultLabel::Allow,
                        0,
                    )?;
                    let utilization_pct = if new_state.bytes_quota == 0 {
                        0.0
                    } else {
                        (new_state.bytes_used as f64)
                            / (new_state.bytes_quota as f64)
                    };
                    return Ok(QuotaCasOutcome {
                        decision: QuotaCasDecision::Allow {
                            decided_at_ms: now_ms,
                            bytes_used_after: new_state.bytes_used,
                            bytes_quota: new_state.bytes_quota,
                            utilization_pct,
                            cas_version_after: new_state.cas_version,
                        },
                        cas_attempts: attempt,
                    });
                }
                Err(AtomicCasStateError::VersionMismatch { .. }) => {
                    // Race detected — emit signal + retry.
                    self.audit.emit(QuotaCasAuditRecord {
                        event_type: QuotaCasEventType::CasRaceDetected,
                        tenant_id,
                        region,
                        bytes: None,
                        cas_version: Some(row.cas_version),
                        cas_attempt: Some(attempt),
                        retry_after_secs: None,
                        created_by_request_id: "test".to_string(),
                        now_ms,
                    })?;
                    self.metrics.record_check(QuotaCasResultLabel::Race)?;
                    self.metrics.record_race_detected(tenant_id)?;
                    if attempt >= max_attempts {
                        self.metrics.record_check_duration_us(
                            QuotaCasResultLabel::Race,
                            0,
                        )?;
                        return Err(QuotaCasError::CasRaceExhausted {
                            tenant_id,
                            attempts: attempt,
                        });
                    }
                    continue;
                }
                Err(other) => {
                    return Err(QuotaCasError::State(other));
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingQuotaCasAuditSink, InMemoryQuotaCasAuditSink};
    use crate::metrics::{InMemoryQuotaCasMetrics, QuotaCasMetricKind};
    use crate::state::{AtomicTenantBytesState, InMemoryAtomicCasState};

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn fresh_state(used: u64, quota: u64) -> AtomicTenantBytesState {
        AtomicTenantBytesState {
            tenant_id: ten_a(),
            region: EvictionRegion::Sam,
            bytes_used: used,
            bytes_quota: quota,
            cas_version: 0,
        }
    }

    type Fixture = (
        InMemoryAtomicQuotaChecker<
            InMemoryAtomicCasState,
            InMemoryQuotaCasAuditSink,
            InMemoryQuotaCasMetrics,
        >,
        Arc<InMemoryAtomicCasState>,
        Arc<InMemoryQuotaCasAuditSink>,
        Arc<InMemoryQuotaCasMetrics>,
    );

    fn fresh() -> Fixture {
        let state = Arc::new(InMemoryAtomicCasState::new());
        let audit = Arc::new(InMemoryQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::with_defaults(
            Arc::clone(&state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        (checker, state, audit, metrics)
    }

    // ---- Allow path -----------------------------------------------

    #[test]
    fn allow_path_under_quota_returns_allow_and_commits() {
        let (checker, state, audit, metrics) = fresh();
        state.seed_row(fresh_state(50, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap();
        match out.decision {
            QuotaCasDecision::Allow {
                bytes_used_after,
                bytes_quota,
                cas_version_after,
                utilization_pct,
                ..
            } => {
                assert_eq!(bytes_used_after, 60);
                assert_eq!(bytes_quota, 100);
                assert_eq!(cas_version_after, 1);
                assert_eq!(utilization_pct, 0.6);
            }
            other => panic!("expected Allow, got {other:?}"),
        }
        assert_eq!(out.cas_attempts, 1);
        // Audit: CheckPassed + CommitSucceeded (no Denied / RaceDetected).
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasCheckPassed).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasCommitSucceeded).len(),
            1
        );
        assert!(audit
            .snapshot_of(QuotaCasEventType::CasDenied429HardBlock)
            .is_empty());
        // Metrics: 1 allow on aggregate counter.
        assert_eq!(
            metrics.counter_total(QuotaCasMetricKind::CheckTotal),
            1
        );
        assert_eq!(
            metrics.check_total_for_label(QuotaCasResultLabel::Allow),
            1
        );
    }

    // ---- Idempotent zero-byte check -------------------------------

    #[test]
    fn zero_byte_request_returns_allow_without_state_mutation() {
        let (checker, state, audit, _m) = fresh();
        state.seed_row(fresh_state(50, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 0, 1_000, 1)
            .unwrap();
        match out.decision {
            QuotaCasDecision::Allow {
                bytes_used_after,
                cas_version_after,
                ..
            } => {
                assert_eq!(bytes_used_after, 50);
                assert_eq!(cas_version_after, 0); // Unchanged.
            }
            other => panic!("expected Allow, got {other:?}"),
        }
        // CheckPassed audit only (no CommitSucceeded — no state mutation).
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasCheckPassed).len(),
            1
        );
        assert!(audit
            .snapshot_of(QuotaCasEventType::CasCommitSucceeded)
            .is_empty());
        // Verify state is unchanged.
        let row = state.lookup(ten_a(), EvictionRegion::Sam).unwrap().unwrap();
        assert_eq!(row.bytes_used, 50);
        assert_eq!(row.cas_version, 0);
    }

    // ---- Hard-block 429 arm ---------------------------------------

    #[test]
    fn write_at_100pct_returns_canonical_deny_429() {
        let (checker, state, audit, metrics) = fresh();
        state.seed_row(fresh_state(100, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap();
        match out.decision {
            QuotaCasDecision::Deny429 {
                would_use,
                bytes_quota,
                retry_after_secs,
                ..
            } => {
                assert_eq!(would_use, 101);
                assert_eq!(bytes_quota, 100);
                // Canonical Retry-After is days-until-month-reset (always > 0).
                assert!(retry_after_secs > 0);
                assert!(retry_after_secs <= 31 * 86_400);
            }
            other => panic!("expected Deny429, got {other:?}"),
        }
        // Audit: Denied + RetryAfterEmitted (no Allow audits).
        assert_eq!(
            audit
                .snapshot_of(QuotaCasEventType::CasDenied429HardBlock)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(QuotaCasEventType::CasRetryAfterEmitted)
                .len(),
            1
        );
        // Metrics: deny + denial counter + retry-after observation.
        assert_eq!(
            metrics.counter_total(QuotaCasMetricKind::DenialsTotal),
            1
        );
        assert_eq!(metrics.retry_after_snapshot().len(), 1);
    }

    #[test]
    fn write_at_99pct_with_request_pushing_over_denies() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(99, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap();
        // 99 + 1 = 100 → predicate `100 < 100` is FALSE → deny.
        assert!(matches!(out.decision, QuotaCasDecision::Deny429 { .. }));
    }

    #[test]
    fn write_at_98pct_with_request_under_quota_allows() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(98, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap();
        // 98 + 1 = 99 < 100 → allow.
        assert!(matches!(out.decision, QuotaCasDecision::Allow { .. }));
    }

    // ---- Boundary: bytes_used = quota - 1 / quota / quota + 1 -----

    #[test]
    fn boundary_quota_minus_one_with_one_byte_request_denies() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(99, 100)).unwrap();
        // 99 + 1 = 100 → strict-< fires deny.
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap();
        assert!(matches!(out.decision, QuotaCasDecision::Deny429 { .. }));
    }

    #[test]
    fn boundary_at_quota_with_zero_byte_allows() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(100, 100)).unwrap();
        // Zero-byte path → allow even at 100% (read path; no mutation).
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 0, 1_000, 1)
            .unwrap();
        assert!(matches!(out.decision, QuotaCasDecision::Allow { .. }));
    }

    #[test]
    fn boundary_above_quota_with_any_request_denies() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(101, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap();
        assert!(matches!(out.decision, QuotaCasDecision::Deny429 { .. }));
    }

    // ---- TenantStorageStateMissing --------------------------------

    #[test]
    fn missing_state_row_returns_typed_error() {
        let (checker, _s, _a, _m) = fresh();
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap_err();
        assert!(matches!(
            err,
            QuotaCasError::TenantStorageStateMissing { .. }
        ));
    }

    #[test]
    fn missing_row_for_zero_byte_path_also_errors() {
        let (checker, _s, _a, _m) = fresh();
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 0, 1_000, 1)
            .unwrap_err();
        assert!(matches!(
            err,
            QuotaCasError::TenantStorageStateMissing { .. }
        ));
    }

    // ---- Tenant isolation -----------------------------------------

    #[test]
    fn tenant_a_at_quota_does_not_block_tenant_b() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(100, 100)).unwrap(); // ten_a saturated.
        state
            .seed_row(AtomicTenantBytesState {
                tenant_id: ten_b(),
                region: EvictionRegion::Sam,
                bytes_used: 0,
                bytes_quota: 1_000,
                cas_version: 0,
            })
            .unwrap();
        // Tenant B writes — should succeed.
        let out = checker
            .try_acquire(ten_b(), EvictionRegion::Sam, 100, 1_000, 1)
            .unwrap();
        assert!(matches!(out.decision, QuotaCasDecision::Allow { .. }));
    }

    // ---- Audit fail-closed ---------------------------------------

    #[test]
    fn audit_failure_aborts_acquire() {
        let state = Arc::new(InMemoryAtomicCasState::new());
        state.seed_row(fresh_state(50, 100)).unwrap();
        let audit = Arc::new(FailingQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::with_defaults(
            Arc::clone(&state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap_err();
        assert!(matches!(err, QuotaCasError::Audit(_)));
        // State unchanged (audit emit BEFORE write).
        let row = state.lookup(ten_a(), EvictionRegion::Sam).unwrap().unwrap();
        assert_eq!(row.bytes_used, 50);
        assert_eq!(row.cas_version, 0);
    }

    // ---- Race detection retry path --------------------------------

    /// Simulator: an AtomicCasState wrapper that bumps the underlying
    /// version EXACTLY ONCE on the first try_commit_delta call (mid-flight
    /// concurrent commit) so the orchestrator must retry. After the bump,
    /// subsequent calls behave normally.
    #[derive(Debug)]
    struct OneShotRaceState {
        inner: Arc<InMemoryAtomicCasState>,
        bumped: std::sync::Mutex<bool>,
    }

    impl OneShotRaceState {
        fn new(inner: Arc<InMemoryAtomicCasState>) -> Self {
            Self {
                inner,
                bumped: std::sync::Mutex::new(false),
            }
        }
    }

    impl AtomicCasState for OneShotRaceState {
        fn lookup(
            &self,
            tenant_id: Uuid,
            region: EvictionRegion,
        ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError>
        {
            self.inner.lookup(tenant_id, region)
        }
        fn try_commit_delta(
            &self,
            tenant_id: Uuid,
            region: EvictionRegion,
            expected_version: u64,
            delta_bytes: u64,
        ) -> Result<AtomicTenantBytesState, AtomicCasStateError> {
            // First call: silently bump the inner version (simulating a
            // mid-flight commit by another actor) THEN attempt the
            // delta — which now fires VersionMismatch.
            let mut g = self.bumped.lock().map_err(|_| {
                AtomicCasStateError::Backend("bumped poisoned".to_string())
            })?;
            if !*g {
                *g = true;
                drop(g);
                // Simulate concurrent commit: bump version by adding 0
                // bytes via a direct seed (the in-memory state allows
                // overwrite via seed_row).
                let cur = self
                    .inner
                    .lookup(tenant_id, region)?
                    .ok_or(AtomicCasStateError::Missing {
                        tenant_id,
                        region: region.as_str(),
                    })?;
                self.inner.seed_row(AtomicTenantBytesState {
                    cas_version: cur.cas_version.saturating_add(1),
                    ..cur
                })?;
                // Now the orchestrator's expected_version is stale.
            }
            self.inner.try_commit_delta(
                tenant_id,
                region,
                expected_version,
                delta_bytes,
            )
        }
        fn seed_row(
            &self,
            state: AtomicTenantBytesState,
        ) -> Result<(), AtomicCasStateError> {
            self.inner.seed_row(state)
        }
    }

    #[test]
    fn cas_race_detected_on_first_attempt_succeeds_on_retry() {
        let inner = Arc::new(InMemoryAtomicCasState::new());
        inner.seed_row(fresh_state(50, 100)).unwrap();
        let race_state = Arc::new(OneShotRaceState::new(Arc::clone(&inner)));
        let audit = Arc::new(InMemoryQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::with_defaults(
            Arc::clone(&race_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap();
        // Should succeed on attempt 2 (after the OneShot bump).
        assert!(matches!(out.decision, QuotaCasDecision::Allow { .. }));
        assert_eq!(out.cas_attempts, 2);
        // Audit: 1 RaceDetected + 2 CheckPassed (one per attempt) + 1 Commit.
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasRaceDetected).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasCheckPassed).len(),
            2
        );
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasCommitSucceeded).len(),
            1
        );
        // Metrics: race counter bumped.
        assert_eq!(
            metrics.counter_total(QuotaCasMetricKind::RaceDetectedTotal),
            1
        );
    }

    /// Simulator: bumps version on EVERY attempt → orchestrator
    /// exhausts retries.
    #[derive(Debug)]
    struct AlwaysRaceState {
        inner: Arc<InMemoryAtomicCasState>,
    }

    impl AlwaysRaceState {
        fn new(inner: Arc<InMemoryAtomicCasState>) -> Self {
            Self { inner }
        }
    }

    impl AtomicCasState for AlwaysRaceState {
        fn lookup(
            &self,
            tenant_id: Uuid,
            region: EvictionRegion,
        ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError>
        {
            self.inner.lookup(tenant_id, region)
        }
        fn try_commit_delta(
            &self,
            tenant_id: Uuid,
            region: EvictionRegion,
            _expected_version: u64,
            _delta_bytes: u64,
        ) -> Result<AtomicTenantBytesState, AtomicCasStateError> {
            // Bump the version, then return mismatch.
            let cur = self
                .inner
                .lookup(tenant_id, region)?
                .ok_or(AtomicCasStateError::Missing {
                    tenant_id,
                    region: region.as_str(),
                })?;
            Err(AtomicCasStateError::VersionMismatch {
                observed: 0,
                actual: cur.cas_version.saturating_add(1),
            })
        }
        fn seed_row(
            &self,
            state: AtomicTenantBytesState,
        ) -> Result<(), AtomicCasStateError> {
            self.inner.seed_row(state)
        }
    }

    #[test]
    fn pathological_race_exhausts_retries() {
        let inner = Arc::new(InMemoryAtomicCasState::new());
        inner.seed_row(fresh_state(50, 100)).unwrap();
        let race_state = Arc::new(AlwaysRaceState::new(Arc::clone(&inner)));
        let audit = Arc::new(InMemoryQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::with_defaults(
            Arc::clone(&race_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap_err();
        match err {
            QuotaCasError::CasRaceExhausted { attempts, .. } => {
                assert_eq!(attempts, 3); // Default canonical max.
            }
            other => panic!("expected CasRaceExhausted, got {other:?}"),
        }
        // 3 RaceDetected audits emitted.
        assert_eq!(
            audit.snapshot_of(QuotaCasEventType::CasRaceDetected).len(),
            3
        );
    }

    // ---- Configurable max attempts -------------------------------

    #[test]
    fn custom_max_attempts_clamps_retries() {
        let inner = Arc::new(InMemoryAtomicCasState::new());
        inner.seed_row(fresh_state(50, 100)).unwrap();
        let race_state = Arc::new(AlwaysRaceState::new(Arc::clone(&inner)));
        let audit = Arc::new(InMemoryQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::new(
            Arc::clone(&race_state),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            QuotaCasConfig::new(1.0, 1),
        );
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 10, 1_000, 1)
            .unwrap_err();
        match err {
            QuotaCasError::CasRaceExhausted { attempts, .. } => {
                assert_eq!(attempts, 1);
            }
            other => panic!("expected CasRaceExhausted, got {other:?}"),
        }
    }

    // ---- effective_ceiling -----------------------------------------

    #[test]
    fn effective_ceiling_at_canonical_pct_equals_quota() {
        let (checker, _s, _a, _m) = fresh();
        assert_eq!(checker.effective_ceiling(100), 100);
        assert_eq!(checker.effective_ceiling(0), 0);
        assert_eq!(checker.effective_ceiling(u64::MAX), u64::MAX);
    }

    #[test]
    fn effective_ceiling_with_lower_pct_floors() {
        let state = Arc::new(InMemoryAtomicCasState::new());
        let audit = Arc::new(InMemoryQuotaCasAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaCasMetrics::new());
        let checker = InMemoryAtomicQuotaChecker::new(
            state,
            audit,
            metrics,
            QuotaCasConfig::new(0.95, 3),
        );
        assert_eq!(checker.effective_ceiling(100), 95);
        assert_eq!(checker.effective_ceiling(1_000), 950);
    }

    // ---- Request bytes overflow ----------------------------------

    #[test]
    fn request_bytes_overflow_returns_typed_error() {
        let (checker, state, _a, _m) = fresh();
        state
            .seed_row(AtomicTenantBytesState {
                tenant_id: ten_a(),
                region: EvictionRegion::Sam,
                bytes_used: u64::MAX,
                bytes_quota: u64::MAX,
                cas_version: 0,
            })
            .unwrap();
        let err = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1)
            .unwrap_err();
        assert!(matches!(err, QuotaCasError::RequestBytesOverflow { .. }));
    }

    // ---- Retry-After canonical pinning ---------------------------

    #[test]
    fn deny_emits_canonical_retry_after_within_bounds() {
        let (checker, state, _a, _m) = fresh();
        state.seed_row(fresh_state(100, 100)).unwrap();
        let out = checker
            .try_acquire(ten_a(), EvictionRegion::Sam, 1, 1_000, 1_700_000_000)
            .unwrap();
        match out.decision {
            QuotaCasDecision::Deny429 { retry_after_secs, .. } => {
                assert!(retry_after_secs >= 1);
                assert!(retry_after_secs <= 31 * 86_400);
            }
            other => panic!("expected Deny429, got {other:?}"),
        }
    }
}
