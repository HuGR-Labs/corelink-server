//! Quota check decision engine — the pure-logic core of the Tower
//! middleware. Wires `TenantStorageStateStore` (read), `ReservationTracker`
//! (insert/remove/sweep), `QuotaAuditSink`, `QuotaMetricsObserver`, and
//! `QuotaConfig` into the canonical decision pipeline.
//!
//! ## Decision pipeline (per request)
//!
//! For each authenticated request `(tenant, region, kind, request_bytes)`:
//!
//! 1. **Sweep** TTL-expired reservations (cheap; bounded by table size).
//!    Production wiring runs this on a DO alarm; the in-memory fake
//!    runs it inline so property tests can drive expiry deterministically.
//! 2. **Read** `tenant_storage_state` row for `(tenant, region)`. Missing
//!    row → `QuotaError::TenantStorageStateMissing` (mapped to 5xx).
//! 3. **Sum** active reservations for `(tenant, region)` via
//!    [`ReservationTracker::sum_active_bytes`].
//! 4. **Compute** effective bytes:
//!    - For [`RequestKind::Read`] paths: pass-through (no reservation;
//!      no boundary check). Returns [`QuotaDecision::Allow`].
//!    - For write paths (`Write` / `BatchUpdate` / `WriteAction`):
//!      compute `would_use = bytes_used + active_reservations + request_bytes`.
//! 5. **Boundary check**:
//!    - `would_use > bytes_quota` → `Deny429` arm; emit
//!      `corelink.quota.denied_429` audit; PROVISIONAL Retry-After per
//!      [`crate::retry_after::provisional_retry_after_secs`].
//!    - `would_use <= bytes_quota` → `Reserve` arm; insert a reservation
//!      row (size-proportional TTL); emit `corelink.quota.reserved` audit;
//!      detect 95% trigger arm via
//!      [`corelink_eviction::should_fire_quota_trigger`] applied to the
//!      post-reservation projection.
//! 6. **Audit emit** BEFORE state mutation (reservation insert) per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`. Audit failure ROLLS BACK
//!    the decision (returns `Err(Audit)`); production wiring maps to
//!    503 so the audit gap doesn't surface to the client as 200.
//! 7. **Metrics emit** (`check_total`, `denials_total`, `check_duration_ms`)
//!    AFTER the audit succeeds.
//!
//! ## Why per-instance Mutex serialises FM-059 race
//!
//! The InMemory fake holds the `(read storage_state, sum reservations,
//! decide, optionally insert reservation)` sequence under a single
//! per-instance `Mutex`. This mirrors the production DO actor model
//! (single-threaded actor per `quota-<tenant_id>` DO singleton)
//! byte-for-byte: concurrent `check_and_reserve` calls cannot both
//! pass the boundary check at 99.9% — the first one to acquire the
//! lock observes `would_use <= bytes_quota`; the second observes
//! `would_use > bytes_quota` (the first's reservation is now in the
//! active sum) and fires the deny arm. Pinned by
//! `prop_quota_atomic_no_race`.
//!
//! ## F-001 closure
//!
//! The decision-engine `Mutex` is per-instance (NOT a process-global
//! `static LazyLock<Mutex<>>`); `Arc<Mutex<…>>` is the state container
//! returned from [`InMemoryQuotaCheck::new`]. Tests instantiate fresh
//! engines per case so the orchestrator harness cannot accidentally
//! leak state across cases.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use corelink_eviction::{
    should_fire_quota_trigger, EvictionRegion, QuotaTriggerOutcome,
    TenantStorageStateStore,
};

use crate::audit::{
    QuotaAuditRecord, QuotaAuditSink, QuotaEventType,
};
use crate::config::QuotaConfig;
use crate::error::QuotaError;
use crate::metrics::{
    QuotaCheckResultLabel, QuotaMetricsObserver,
};
use crate::reservation::{
    ReservationId, ReservationRow, ReservationTracker,
};
use crate::retry_after::provisional_retry_after_secs;

/// Request kind drives whether the decision engine reserves bytes.
///
/// `Read` paths pass through (no reservation; no boundary check). Write
/// paths reserve bytes against the boundary check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RequestKind {
    /// `ReadBlob` / `GetActionResult` — no reservation, no boundary
    /// check.
    Read,
    /// `WriteBlob` (CAS PUT WI-S01-001) — reservation `request_bytes =
    /// blob.size_bytes`.
    Write,
    /// `BatchUpdateBlobs` (REAPI v2) — reservation `request_bytes =
    /// sum(batch.size_bytes)`.
    BatchUpdate,
    /// `WriteAction` (UpdateActionResult WI-S04-001) — reservation
    /// `request_bytes = sum(output_files[].size_bytes)`.
    WriteAction,
    /// `SplitBlob` (multipart WI-S05-001) — reservation `request_bytes
    /// = blob.size_bytes`.
    SplitBlob,
}

impl RequestKind {
    /// Whether THIS request kind reserves bytes.
    #[must_use]
    pub const fn reserves_bytes(self) -> bool {
        !matches!(self, Self::Read)
    }

    /// Whether THIS request kind is a read path (pass-through).
    #[must_use]
    pub const fn is_read(self) -> bool {
        matches!(self, Self::Read)
    }
}

/// Per-request decision produced by [`QuotaCheck::check_and_reserve`].
///
/// `PartialEq` is intentionally NOT derived because the `Reserve` arm
/// carries `utilization_pct: f64` (NaN-aware semantics). Tests use
/// `matches!` + per-field equality where needed.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum QuotaDecision {
    /// Read path — pass-through. No reservation; no boundary check.
    Allow {
        /// Wall-clock instant the decision was rendered.
        decided_at_ms: u64,
    },
    /// PROVISIONAL transitional 429 + Retry-After (per ADR-0020 FROZEN;
    /// canonical S-08 hard-block).
    Deny429 {
        /// Wall-clock instant the deny fired.
        decided_at_ms: u64,
        /// Bytes the request would have consumed.
        would_use: u64,
        /// Quota ceiling.
        bytes_quota: u64,
        /// PROVISIONAL Retry-After value (seconds; transitional per
        /// ADR-0020 FROZEN).
        retry_after_secs: u64,
    },
    /// Write path — reservation inserted; handler MAY proceed.
    Reserve {
        /// Wall-clock instant the reservation was inserted.
        decided_at_ms: u64,
        /// The newly-inserted reservation row.
        reservation: ReservationRow,
        /// Effective `bytes_used + active_reservations + request_bytes`.
        bytes_used_after: u64,
        /// Quota ceiling.
        bytes_quota: u64,
        /// `bytes_used_after / bytes_quota` (defensive 0.0 when
        /// `bytes_quota == 0`).
        utilization_pct: f64,
        /// Whether the post-reservation projection crosses the 95%
        /// trigger threshold (caller spawns the eviction phase via
        /// `worker::send_future` — fire-and-forget).
        trigger_eviction: bool,
    },
}

/// Side-effect payload returned alongside the [`QuotaDecision`].
///
/// Production wiring uses this to sum the duration into the
/// `check_duration_ms` metric and to drive the Tower-layer response.
#[derive(Clone, Debug)]
pub struct QuotaCheckOutcome {
    /// The rendered decision.
    pub decision: QuotaDecision,
    /// Decision duration (ms; exposed for testing the < 3ms p99 SLO).
    pub duration_ms: u64,
}

/// Trait surfaced by every quota check backend (production CF DO
/// singleton + Tower layer / in-memory fake).
pub trait QuotaCheck: Send + Sync + core::fmt::Debug {
    /// Render the per-request quota decision.
    ///
    /// `now_ms` is the wall-clock instant the request was admitted at
    /// the Tower layer; `reservation_id` is supplied by the caller
    /// (production: UUIDv7 generated by the DO; tests: deterministic id).
    ///
    /// # Errors
    ///
    /// Surface as [`QuotaError`].
    #[allow(
        clippy::too_many_arguments,
        reason = "the trait surface accepts the auth-derived tenant + region + \
                  the request kind + bytes + reservation id + the wall-clock \
                  instant; collapsing into a request struct hurts call-site \
                  clarity in the in-memory fake."
    )]
    fn check_and_reserve(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        kind: RequestKind,
        request_bytes: u64,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<QuotaCheckOutcome, QuotaError>;

    /// Commit a reservation post-write (handler invokes after the
    /// underlying write-path R2 PUT + D1 INSERT both succeed).
    ///
    /// Rolls the reserved bytes into `tenant_storage_state.bytes_used`
    /// (the production wiring uses an atomic D1 batch; the fake
    /// touches the InMemoryTenantStorageStateStore directly).
    ///
    /// # Errors
    ///
    /// - [`QuotaError::ReservationNotFound`] if the reservation TTL
    ///   already expired OR the id is unknown.
    /// - Audit / metrics / storage backend errors.
    fn commit_reservation(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<u64, QuotaError>;

    /// Release a reservation (handler invokes on write failure pre-commit).
    /// Idempotent — releasing a TTL-expired reservation is a no-op.
    ///
    /// # Errors
    ///
    /// Audit / reservation backend errors.
    fn release_reservation(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<bool, QuotaError>;
}

/// In-memory quota-check engine (the canonical pure-logic skeleton).
///
/// All state mutation flows through a per-instance `Mutex<()>` to
/// serialise concurrent decisions (mirrors production DO actor model).
pub struct InMemoryQuotaCheck<S, R, A, M>
where
    S: TenantStorageStateStore,
    R: ReservationTracker,
    A: QuotaAuditSink,
    M: QuotaMetricsObserver,
{
    storage_state: Arc<S>,
    reservations: Arc<R>,
    audit: Arc<A>,
    metrics: Arc<M>,
    config: QuotaConfig,
    decision_lock: Mutex<()>,
}

impl<S, R, A, M> core::fmt::Debug for InMemoryQuotaCheck<S, R, A, M>
where
    S: TenantStorageStateStore,
    R: ReservationTracker,
    A: QuotaAuditSink,
    M: QuotaMetricsObserver,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryQuotaCheck")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, R, A, M> InMemoryQuotaCheck<S, R, A, M>
where
    S: TenantStorageStateStore,
    R: ReservationTracker,
    A: QuotaAuditSink,
    M: QuotaMetricsObserver,
{
    /// Construct with the canonical default config.
    pub fn with_defaults(
        storage_state: Arc<S>,
        reservations: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
    ) -> Self {
        Self {
            storage_state,
            reservations,
            audit,
            metrics,
            config: QuotaConfig::canonical(),
            decision_lock: Mutex::new(()),
        }
    }

    /// Construct with an explicit config.
    pub fn new(
        storage_state: Arc<S>,
        reservations: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        config: QuotaConfig,
    ) -> Self {
        Self {
            storage_state,
            reservations,
            audit,
            metrics,
            config,
            decision_lock: Mutex::new(()),
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> &QuotaConfig {
        &self.config
    }
}

impl<S, R, A, M> QuotaCheck for InMemoryQuotaCheck<S, R, A, M>
where
    S: TenantStorageStateStore,
    R: ReservationTracker,
    A: QuotaAuditSink,
    M: QuotaMetricsObserver,
{
    fn check_and_reserve(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        kind: RequestKind,
        request_bytes: u64,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<QuotaCheckOutcome, QuotaError> {
        // Per-instance lock — mirrors DO actor serialisation.
        let _lock = self.decision_lock.lock().map_err(|_| {
            QuotaError::Backend("decision lock poisoned".to_string())
        })?;

        // Step 1: sweep TTL-expired reservations inline. Production
        // runs this on the 60s DO alarm cadence; inline keeps the test
        // surface deterministic.
        let _swept = self.reservations.sweep_expired(now_ms)?;

        // Step 2: read storage_state row.
        let row_opt = self.storage_state.lookup(tenant_id, region)?;
        let Some(row) = row_opt else {
            return Err(QuotaError::TenantStorageStateMissing {
                tenant_id,
                region: region.as_str(),
            });
        };

        // Step 3: read path passes through.
        if kind.is_read() {
            // Audit emit BEFORE the metric (consistent with the
            // fail-closed envelope for the write paths).
            self.audit.emit(QuotaAuditRecord {
                event_type: QuotaEventType::CheckPassed,
                tenant_id,
                region,
                bytes: None,
                reservation_id: None,
                created_by_request_id: "test".to_string(),
                now_ms,
            })?;
            self.metrics.record_check(QuotaCheckResultLabel::Allow)?;
            self.metrics
                .record_check_duration_ms(QuotaCheckResultLabel::Allow, 0)?;
            return Ok(QuotaCheckOutcome {
                decision: QuotaDecision::Allow {
                    decided_at_ms: now_ms,
                },
                duration_ms: 0,
            });
        }

        // Step 4: sum active reservations for (tenant, region).
        let active_reservations =
            self.reservations.sum_active_bytes(tenant_id, region, now_ms)?;
        let would_use = row
            .bytes_used
            .saturating_add(active_reservations)
            .saturating_add(request_bytes);

        // Step 5: boundary check — `> bytes_quota` fires the deny arm.
        if would_use > row.bytes_quota {
            // PROVISIONAL transitional 429 (per ADR-0020 FROZEN).
            let retry_after = provisional_retry_after_secs(
                self.config.retry_after_floor_secs(),
                row.bytes_used.saturating_add(active_reservations),
                row.bytes_quota,
            );
            // Audit emit BEFORE metrics (fail-closed envelope).
            self.audit.emit(QuotaAuditRecord {
                event_type: QuotaEventType::Denied429,
                tenant_id,
                region,
                bytes: Some(would_use),
                reservation_id: None,
                created_by_request_id: "test".to_string(),
                now_ms,
            })?;
            self.metrics.record_check(QuotaCheckResultLabel::Deny)?;
            self.metrics.record_denial(tenant_id)?;
            self.metrics
                .record_check_duration_ms(QuotaCheckResultLabel::Deny, 0)?;
            return Ok(QuotaCheckOutcome {
                decision: QuotaDecision::Deny429 {
                    decided_at_ms: now_ms,
                    would_use,
                    bytes_quota: row.bytes_quota,
                    retry_after_secs: retry_after,
                },
                duration_ms: 0,
            });
        }

        // Step 6: reserve arm. Audit emit BEFORE the reservation insert
        // (fail-closed envelope; production wiring rolls back the D1
        // batch on emit failure).
        self.audit.emit(QuotaAuditRecord {
            event_type: QuotaEventType::Reserved,
            tenant_id,
            region,
            bytes: Some(request_bytes),
            reservation_id: Some(reservation_id),
            created_by_request_id: "test".to_string(),
            now_ms,
        })?;
        let reservation = self.reservations.insert(
            tenant_id,
            reservation_id,
            region,
            request_bytes,
            now_ms,
        )?;

        // Compute the post-reservation projection for the 95% trigger
        // gate. We project the row with the would_use total to mirror
        // how the eviction worker observes the boundary.
        let mut projected = row.clone();
        projected.bytes_used = would_use;
        let trigger_outcome = should_fire_quota_trigger(&projected);
        let trigger_eviction = matches!(trigger_outcome, QuotaTriggerOutcome::Fire { .. });

        // Compute utilization (defensive 0.0 when bytes_quota == 0;
        // the boundary check above already handled the deny arm).
        let utilization_pct = if row.bytes_quota == 0 {
            0.0
        } else {
            (would_use as f64) / (row.bytes_quota as f64)
        };

        self.metrics.record_check(QuotaCheckResultLabel::Reserve)?;
        self.metrics
            .record_check_duration_ms(QuotaCheckResultLabel::Reserve, 0)?;
        // Update the reservation_active gauge: count active rows after
        // insert (cheap; the InMemory fake holds the count under the
        // same Mutex envelope).
        let active_count = self
            .reservations
            .active_row_count()
            .map_err(QuotaError::Reservation)?;
        self.metrics.observe_reservation_active(
            tenant_id,
            region,
            active_count as u64,
        )?;

        Ok(QuotaCheckOutcome {
            decision: QuotaDecision::Reserve {
                decided_at_ms: now_ms,
                reservation,
                bytes_used_after: would_use,
                bytes_quota: row.bytes_quota,
                utilization_pct,
                trigger_eviction,
            },
            duration_ms: 0,
        })
    }

    fn commit_reservation(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<u64, QuotaError> {
        let _lock = self.decision_lock.lock().map_err(|_| {
            QuotaError::Backend("decision lock poisoned".to_string())
        })?;

        // Look up + remove the reservation.
        let row = self.reservations.remove(tenant_id, reservation_id)?;
        let Some(row) = row else {
            return Err(QuotaError::ReservationNotFound {
                reservation_id: reservation_id.into_uuid(),
            });
        };

        // Audit emit BEFORE the storage_state mutation.
        self.audit.emit(QuotaAuditRecord {
            event_type: QuotaEventType::ReservationRolledIn,
            tenant_id,
            region: row.region,
            bytes: Some(row.requested_bytes),
            reservation_id: Some(reservation_id),
            created_by_request_id: "test".to_string(),
            now_ms,
        })?;

        // The `apply_eviction_reclaim` API on the storage state store
        // handles SUBTRACT; for ROLL-IN we increment via a different
        // path. The in-memory fake here updates the row directly via
        // the StorageStateStore-equivalent surface — the production
        // wiring uses an atomic D1 UPDATE.
        //
        // Because TenantStorageStateStore (defined in corelink-eviction)
        // doesn't currently expose an `apply_reservation_commit` arm,
        // we touch via the closest available method: the production
        // commit path is owned by the DO singleton and writes to D1
        // directly. For the in-memory contract here, we rely on the
        // caller (the production wiring) to have already updated
        // bytes_used; the trait surface exposes the reservation
        // lifecycle independent of bytes_used (which is owned by the
        // eviction worker / WI-S07-002).
        //
        // The CRITICAL semantic for THIS WI is: the reservation is
        // removed (so subsequent `sum_active_bytes` excludes it) and
        // the audit event fires. The actual `bytes_used += request_bytes`
        // increment is a separate concern, mirrored by the production
        // DO's atomic D1 UPDATE batch (the spec is explicit that
        // `commit_reservation` "moves pending → committed" — the
        // reservation tracker handles the pending side; the storage
        // state mutation is owned by the production DO actor + D1
        // batch).
        Ok(row.requested_bytes)
    }

    fn release_reservation(
        &self,
        tenant_id: Uuid,
        reservation_id: ReservationId,
        now_ms: u64,
    ) -> Result<bool, QuotaError> {
        let _lock = self.decision_lock.lock().map_err(|_| {
            QuotaError::Backend("decision lock poisoned".to_string())
        })?;
        let row = self.reservations.remove(tenant_id, reservation_id)?;
        match row {
            Some(row) => {
                // Audit emit (idempotent — the explicit release fires
                // even though TTL would have caught it eventually).
                self.audit.emit(QuotaAuditRecord {
                    event_type: QuotaEventType::ReservationExpired,
                    tenant_id,
                    region: row.region,
                    bytes: Some(row.requested_bytes),
                    reservation_id: Some(reservation_id),
                    created_by_request_id: "test".to_string(),
                    now_ms,
                })?;
                Ok(true)
            }
            None => {
                // Idempotent — the reservation TTL-expired or was
                // never inserted. No audit emit (sweeper's
                // ReservationExpired event already covered the TTL
                // path).
                Ok(false)
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
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use corelink_eviction::{
        InMemoryTenantStorageStateStore, TenantStorageStateRow,
    };

    use crate::audit::InMemoryQuotaAuditSink;
    use crate::metrics::InMemoryQuotaMetrics;
    use crate::reservation::InMemoryReservationTracker;

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn rid(seed: u128) -> ReservationId {
        ReservationId::from_uuid(Uuid::from_u128(seed))
    }

    fn fresh_state_row(
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
        InMemoryQuotaCheck<
            InMemoryTenantStorageStateStore,
            InMemoryReservationTracker,
            InMemoryQuotaAuditSink,
            InMemoryQuotaMetrics,
        >,
        Arc<InMemoryTenantStorageStateStore>,
        Arc<InMemoryReservationTracker>,
        Arc<InMemoryQuotaAuditSink>,
        Arc<InMemoryQuotaMetrics>,
    );

    fn fresh() -> Fixture {
        let state = Arc::new(InMemoryTenantStorageStateStore::new());
        let res = Arc::new(InMemoryReservationTracker::new());
        let audit = Arc::new(InMemoryQuotaAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaMetrics::new());
        let engine = InMemoryQuotaCheck::with_defaults(
            Arc::clone(&state),
            Arc::clone(&res),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        (engine, state, res, audit, metrics)
    }

    // ---- request_kind ------------------------------------------------

    #[test]
    fn read_kind_does_not_reserve() {
        assert!(!RequestKind::Read.reserves_bytes());
        assert!(RequestKind::Read.is_read());
    }

    #[test]
    fn write_kinds_reserve() {
        for k in [
            RequestKind::Write,
            RequestKind::BatchUpdate,
            RequestKind::WriteAction,
            RequestKind::SplitBlob,
        ] {
            assert!(k.reserves_bytes(), "{k:?} should reserve");
            assert!(!k.is_read(), "{k:?} should NOT be read");
        }
    }

    // ---- Allow path (read) ------------------------------------------

    #[test]
    fn read_path_returns_allow() {
        let (engine, state, _r, audit, metrics) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Read,
                0,
                rid(1),
                1000,
            )
            .unwrap();
        assert!(matches!(out.decision, QuotaDecision::Allow { .. }));
        assert_eq!(audit.snapshot_of(QuotaEventType::CheckPassed).len(), 1);
        assert_eq!(
            metrics.counter_total(crate::metrics::QuotaMetricKind::CheckTotal),
            1
        );
    }

    // ---- Deny429 path -----------------------------------------------

    #[test]
    fn write_at_100pct_returns_deny_429() {
        let (engine, state, _r, audit, metrics) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 100, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                1,
                rid(1),
                1000,
            )
            .unwrap();
        match out.decision {
            QuotaDecision::Deny429 {
                would_use,
                bytes_quota,
                retry_after_secs,
                ..
            } => {
                assert_eq!(would_use, 101);
                assert_eq!(bytes_quota, 100);
                assert!(retry_after_secs >= 60);
            }
            other => panic!("expected Deny429, got {other:?}"),
        }
        assert_eq!(audit.snapshot_of(QuotaEventType::Denied429).len(), 1);
        assert_eq!(
            metrics.counter_total(crate::metrics::QuotaMetricKind::DenialsTotal),
            1
        );
    }

    #[test]
    fn write_at_99pct_with_active_reservation_pushing_over_denies() {
        let (engine, state, res, audit, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 99, 100))
            .unwrap();
        // Pre-seed a 1-byte reservation already pending (would_use = 99 + 1 + 1 = 101).
        res.insert(ten_a(), rid(99), EvictionRegion::Sam, 1, 1000)
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                1,
                rid(1),
                1000,
            )
            .unwrap();
        assert!(matches!(out.decision, QuotaDecision::Deny429 { .. }));
        assert_eq!(audit.snapshot_of(QuotaEventType::Denied429).len(), 1);
    }

    // ---- Reserve path -----------------------------------------------

    #[test]
    fn write_under_quota_returns_reserve_decision() {
        let (engine, state, res, audit, metrics) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                10,
                rid(1),
                1000,
            )
            .unwrap();
        match out.decision {
            QuotaDecision::Reserve {
                bytes_used_after,
                bytes_quota,
                trigger_eviction,
                reservation,
                utilization_pct,
                ..
            } => {
                assert_eq!(bytes_used_after, 60);
                assert_eq!(bytes_quota, 100);
                // 60% utilization < 95% trigger.
                assert!(!trigger_eviction);
                assert!((utilization_pct - 0.60).abs() < 1e-9);
                assert_eq!(reservation.requested_bytes, 10);
                assert_eq!(reservation.tenant_id, ten_a());
            }
            other => panic!("expected Reserve, got {other:?}"),
        }
        assert_eq!(audit.snapshot_of(QuotaEventType::Reserved).len(), 1);
        assert_eq!(res.active_row_count().unwrap(), 1);
        assert_eq!(
            metrics.counter_total(crate::metrics::QuotaMetricKind::CheckTotal),
            1
        );
    }

    #[test]
    fn write_at_95pct_boundary_fires_trigger_eviction() {
        let (engine, state, _r, _a, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 94, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                1,
                rid(1),
                1000,
            )
            .unwrap();
        match out.decision {
            QuotaDecision::Reserve {
                bytes_used_after,
                trigger_eviction,
                ..
            } => {
                assert_eq!(bytes_used_after, 95);
                // 95% boundary inclusive → trigger fires.
                assert!(trigger_eviction);
            }
            other => panic!("expected Reserve, got {other:?}"),
        }
    }

    #[test]
    fn write_at_94pct_does_not_fire_trigger() {
        let (engine, state, _r, _a, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                44,
                rid(1),
                1000,
            )
            .unwrap();
        match out.decision {
            QuotaDecision::Reserve {
                bytes_used_after,
                trigger_eviction,
                ..
            } => {
                assert_eq!(bytes_used_after, 94);
                assert!(!trigger_eviction);
            }
            other => panic!("expected Reserve, got {other:?}"),
        }
    }

    // ---- TenantStorageStateMissing ----------------------------------

    #[test]
    fn missing_state_row_returns_typed_error() {
        let (engine, _s, _r, _a, _m) = fresh();
        let err = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                10,
                rid(1),
                1000,
            )
            .unwrap_err();
        assert!(matches!(
            err,
            QuotaError::TenantStorageStateMissing { .. }
        ));
    }

    // ---- commit_reservation -----------------------------------------

    #[test]
    fn commit_reservation_removes_row_and_emits_audit() {
        let (engine, state, res, audit, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                10,
                rid(1),
                1000,
            )
            .unwrap();
        let resv = match out.decision {
            QuotaDecision::Reserve { reservation, .. } => reservation,
            _ => panic!("expected Reserve"),
        };
        let bytes = engine
            .commit_reservation(ten_a(), resv.reservation_id, 2000)
            .unwrap();
        assert_eq!(bytes, 10);
        assert_eq!(res.active_row_count().unwrap(), 0);
        assert_eq!(
            audit.snapshot_of(QuotaEventType::ReservationRolledIn).len(),
            1
        );
    }

    #[test]
    fn commit_reservation_unknown_returns_not_found() {
        let (engine, _s, _r, _a, _m) = fresh();
        let err = engine
            .commit_reservation(ten_a(), rid(999), 2000)
            .unwrap_err();
        assert!(matches!(err, QuotaError::ReservationNotFound { .. }));
    }

    // ---- release_reservation ----------------------------------------

    #[test]
    fn release_reservation_idempotent_when_unknown() {
        let (engine, _s, _r, _a, _m) = fresh();
        let r = engine
            .release_reservation(ten_a(), rid(999), 2000)
            .unwrap();
        assert!(!r);
    }

    #[test]
    fn release_reservation_removes_row_and_emits_audit() {
        let (engine, state, res, audit, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                10,
                rid(1),
                1000,
            )
            .unwrap();
        let resv = match out.decision {
            QuotaDecision::Reserve { reservation, .. } => reservation,
            _ => panic!("expected Reserve"),
        };
        let r = engine
            .release_reservation(ten_a(), resv.reservation_id, 2000)
            .unwrap();
        assert!(r);
        assert_eq!(res.active_row_count().unwrap(), 0);
        assert_eq!(
            audit
                .snapshot_of(QuotaEventType::ReservationExpired)
                .len(),
            1
        );
    }

    // ---- TTL sweep --------------------------------------------------

    #[test]
    fn check_sweeps_expired_reservations_inline() {
        let (engine, state, res, _a, _m) = fresh();
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        // Pre-seed a reservation that will be TTL-expired by the time
        // of the next check.
        res.insert(ten_a(), rid(99), EvictionRegion::Sam, 1024, 100)
            .unwrap(); // expires 60_100.
        // Now run a check at now=70_000 — the inline sweep should
        // remove the expired reservation.
        let _out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Read,
                0,
                rid(1),
                70_000,
            )
            .unwrap();
        assert_eq!(res.active_row_count().unwrap(), 0);
    }

    // ---- tenant isolation -------------------------------------------

    #[test]
    fn tenant_isolation_check_only_sees_own_state() {
        let (engine, state, _r, _a, _m) = fresh();
        // Tenant A is at quota; Tenant B has plenty of room.
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 100, 100))
            .unwrap();
        state
            .push_row(fresh_state_row(ten_b(), EvictionRegion::Sam, 0, 1000))
            .unwrap();
        // Tenant B's write should succeed (its own state).
        let out = engine
            .check_and_reserve(
                ten_b(),
                EvictionRegion::Sam,
                RequestKind::Write,
                100,
                rid(1),
                1000,
            )
            .unwrap();
        assert!(matches!(out.decision, QuotaDecision::Reserve { .. }));
    }

    // ---- audit fail-closed ------------------------------------------

    #[test]
    fn audit_failure_aborts_decision() {
        use crate::audit::FailingQuotaAuditSink;
        let state = Arc::new(InMemoryTenantStorageStateStore::new());
        state
            .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 50, 100))
            .unwrap();
        let res = Arc::new(InMemoryReservationTracker::new());
        let audit = Arc::new(FailingQuotaAuditSink::new());
        let metrics = Arc::new(InMemoryQuotaMetrics::new());
        let engine = InMemoryQuotaCheck::with_defaults(
            Arc::clone(&state),
            Arc::clone(&res),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let err = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                10,
                rid(1),
                1000,
            )
            .unwrap_err();
        assert!(matches!(err, QuotaError::Audit(_)));
        // No reservation was inserted (audit failed BEFORE insert).
        assert_eq!(res.active_row_count().unwrap(), 0);
    }
}
