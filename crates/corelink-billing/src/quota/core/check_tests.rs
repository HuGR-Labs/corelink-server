#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::super::*;
    use corelink_eviction::{InMemoryTenantStorageStateStore, TenantStorageStateRow};

    use super::super::super::audit::InMemoryQuotaAuditSink;
    use super::super::super::metrics::InMemoryQuotaMetrics;
    use super::super::super::reservation::InMemoryReservationTracker;

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
            metrics.counter_total(super::super::super::metrics::QuotaMetricKind::CheckTotal),
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
            metrics.counter_total(super::super::super::metrics::QuotaMetricKind::DenialsTotal),
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
            metrics.counter_total(super::super::super::metrics::QuotaMetricKind::CheckTotal),
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
        assert!(matches!(err, QuotaError::TenantStorageStateMissing { .. }));
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
        let r = engine.release_reservation(ten_a(), rid(999), 2000).unwrap();
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
            audit.snapshot_of(QuotaEventType::ReservationExpired).len(),
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
        use super::super::super::audit::FailingQuotaAuditSink;
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
