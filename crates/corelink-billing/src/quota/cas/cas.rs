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
//!      - emit [`super::metrics::QuotaCasResultLabel::Deny`] +
//!        denial counter + retry-after histogram;
//!      - return [`QuotaCasDecision::Deny429`] with canonical
//!        `retry_after_secs = days_until_month_reset_secs(now_secs)`.
//!    - **Conditional write** via
//!      [`AtomicCasState::try_commit_delta`] using the snapshot's
//!      `cas_version`. On success:
//!      - emit [`QuotaCasEventType::CasCheckPassed`] +
//!        [`QuotaCasEventType::CasCommitSucceeded`] audits;
//!      - emit [`super::metrics::QuotaCasResultLabel::Allow`];
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

use super::audit::{QuotaCasAuditRecord, QuotaCasAuditSink, QuotaCasEventType};
use super::config::QuotaCasConfig;
use super::error::QuotaCasError;
use super::metrics::{QuotaCasMetricsObserver, QuotaCasResultLabel};
use super::retry_after::days_until_month_reset_secs;
use super::state::{AtomicCasState, AtomicCasStateError};

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
    pub fn new(state: Arc<S>, audit: Arc<A>, metrics: Arc<M>, config: QuotaCasConfig) -> Self {
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
            let row = self.state.lookup(tenant_id, region)?.ok_or(
                QuotaCasError::TenantStorageStateMissing {
                    tenant_id,
                    region: region.as_str(),
                },
            )?;
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

            let commit_result =
                self.state
                    .try_commit_delta(tenant_id, region, row.cas_version, request_bytes);
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
                    self.metrics
                        .record_check_duration_us(QuotaCasResultLabel::Allow, 0)?;
                    let utilization_pct = if new_state.bytes_quota == 0 {
                        0.0
                    } else {
                        (new_state.bytes_used as f64) / (new_state.bytes_quota as f64)
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
                        self.metrics
                            .record_check_duration_us(QuotaCasResultLabel::Race, 0)?;
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
#[path = "cas_tests.rs"]
mod cas_tests;
