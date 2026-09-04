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
    use super::super::super::audit::{FailingQuotaCasAuditSink, InMemoryQuotaCasAuditSink};
    use super::super::super::metrics::{InMemoryQuotaCasMetrics, QuotaCasMetricKind};
    use super::super::super::state::{AtomicTenantBytesState, InMemoryAtomicCasState};
    use super::super::*;

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
            audit
                .snapshot_of(QuotaCasEventType::CasCommitSucceeded)
                .len(),
            1
        );
        assert!(audit
            .snapshot_of(QuotaCasEventType::CasDenied429HardBlock)
            .is_empty());
        // Metrics: 1 allow on aggregate counter.
        assert_eq!(metrics.counter_total(QuotaCasMetricKind::CheckTotal), 1);
        assert_eq!(metrics.check_total_for_label(QuotaCasResultLabel::Allow), 1);
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
        assert_eq!(metrics.counter_total(QuotaCasMetricKind::DenialsTotal), 1);
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
        ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError> {
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
            let mut g = self
                .bumped
                .lock()
                .map_err(|_| AtomicCasStateError::Backend("bumped poisoned".to_string()))?;
            if !*g {
                *g = true;
                drop(g);
                // Simulate concurrent commit: bump version by adding 0
                // bytes via a direct seed (the in-memory state allows
                // overwrite via seed_row).
                let cur =
                    self.inner
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
            self.inner
                .try_commit_delta(tenant_id, region, expected_version, delta_bytes)
        }
        fn seed_row(&self, state: AtomicTenantBytesState) -> Result<(), AtomicCasStateError> {
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
            audit
                .snapshot_of(QuotaCasEventType::CasCommitSucceeded)
                .len(),
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
        ) -> Result<Option<AtomicTenantBytesState>, AtomicCasStateError> {
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
            let cur =
                self.inner
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
        fn seed_row(&self, state: AtomicTenantBytesState) -> Result<(), AtomicCasStateError> {
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
        let checker =
            InMemoryAtomicQuotaChecker::new(state, audit, metrics, QuotaCasConfig::new(0.95, 3));
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
            QuotaCasDecision::Deny429 {
                retry_after_secs, ..
            } => {
                assert!(retry_after_secs >= 1);
                assert!(retry_after_secs <= 31 * 86_400);
            }
            other => panic!("expected Deny429, got {other:?}"),
        }
    }
}
