use super::*;

fn run_id(seed: u128) -> RunId {
    RunId(Uuid::from_u128(seed))
}
fn tenant(seed: u128) -> Uuid {
    Uuid::from_u128(seed)
}

#[test]
fn phase_canonical_strings() {
    assert_eq!(GcPhase::Idle.as_str(), "idle");
    assert_eq!(GcPhase::Mark.as_str(), "mark");
    assert_eq!(GcPhase::Sweep.as_str(), "sweep");
    assert_eq!(GcPhase::PhysicalDelete.as_str(), "physical_delete");
    assert_eq!(GcPhase::Reconcile.as_str(), "reconcile");
    assert_eq!(GcPhase::Completed.as_str(), "completed");
    assert_eq!(GcPhase::Failed.as_str(), "failed");
}

#[test]
fn status_canonical_strings() {
    assert_eq!(GcStatus::Pending.as_str(), "pending");
    assert_eq!(GcStatus::Running.as_str(), "running");
    assert_eq!(GcStatus::Succeeded.as_str(), "succeeded");
    assert_eq!(GcStatus::Crashed.as_str(), "crashed");
    assert_eq!(GcStatus::Aborted.as_str(), "aborted");
    assert_eq!(GcStatus::Failed.as_str(), "failed");
}

#[test]
fn legal_phase_chain() {
    assert!(GcPhase::Idle.can_transition_to(GcPhase::Mark));
    assert!(GcPhase::Mark.can_transition_to(GcPhase::Sweep));
    assert!(GcPhase::Sweep.can_transition_to(GcPhase::PhysicalDelete));
    assert!(GcPhase::PhysicalDelete.can_transition_to(GcPhase::Reconcile));
    assert!(GcPhase::Reconcile.can_transition_to(GcPhase::Completed));
}

#[test]
fn any_active_phase_can_fail() {
    for p in [
        GcPhase::Idle,
        GcPhase::Mark,
        GcPhase::Sweep,
        GcPhase::PhysicalDelete,
        GcPhase::Reconcile,
    ] {
        assert!(p.can_transition_to(GcPhase::Failed), "{p:?}");
    }
}

#[test]
fn terminal_phases_have_no_outbound() {
    for terminal in [GcPhase::Completed, GcPhase::Failed] {
        for next in [
            GcPhase::Idle,
            GcPhase::Mark,
            GcPhase::Sweep,
            GcPhase::PhysicalDelete,
            GcPhase::Reconcile,
            GcPhase::Completed,
            GcPhase::Failed,
        ] {
            assert!(!terminal.can_transition_to(next), "{terminal:?}->{next:?}");
        }
    }
}

#[test]
fn skip_ahead_phase_rejected() {
    assert!(!GcPhase::Idle.can_transition_to(GcPhase::Sweep));
    assert!(!GcPhase::Mark.can_transition_to(GcPhase::Reconcile));
    assert!(!GcPhase::Sweep.can_transition_to(GcPhase::Completed));
}

#[test]
fn reverse_phase_rejected() {
    assert!(!GcPhase::Mark.can_transition_to(GcPhase::Idle));
    assert!(!GcPhase::Sweep.can_transition_to(GcPhase::Mark));
    assert!(!GcPhase::Completed.can_transition_to(GcPhase::Reconcile));
}

#[test]
fn insert_pending_then_lookup() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
    assert_eq!(row.status, GcStatus::Pending);
    assert_eq!(row.phase, GcPhase::Idle);
    assert_eq!(row.mark_started_at_ms, None);
}

#[test]
fn cross_tenant_lookup_returns_none() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    // Tenant B sees nothing — Layer 4 envelope.
    assert!(store.lookup(run_id(1), tenant(2)).unwrap().is_none());
}

#[test]
fn acquire_running_succeeds_then_blocks_second() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    // Second pending row for the same (tenant, region) ; insert
    // succeeds (Pending allowed) but acquire fails:
    store
        .insert_pending(run_id(2), tenant(1), GcRegion::Sam, 300, "cron".into())
        .unwrap();
    let err = store
        .acquire_running(run_id(2), tenant(1), 400)
        .unwrap_err();
    assert!(matches!(err, GcRunStoreError::AlreadyRunning { .. }));
}

#[test]
fn other_region_can_acquire_in_parallel() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store
        .insert_pending(run_id(2), tenant(1), GcRegion::Iad, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    // Same tenant, different region — MUST succeed (per-region
    // partial UNIQUE).
    store.acquire_running(run_id(2), tenant(1), 200).unwrap();
}

#[test]
fn other_tenant_can_acquire_same_region() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store
        .insert_pending(run_id(2), tenant(2), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    // Different tenant, same region — partial UNIQUE on
    // (tenant_id, region) so it's per-(tenant, region).
    store.acquire_running(run_id(2), tenant(2), 200).unwrap();
}

#[test]
fn mark_started_at_is_immutable() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    store
        .transition_phase(run_id(1), tenant(1), GcPhase::Mark, 300)
        .unwrap();
    let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
    assert_eq!(row.mark_started_at_ms, Some(300));
    // Reverse phase to Mark is rejected by transition graph; even
    // if we could, the immutable mark_started_at_ms guard fires.
    let err = store
        .transition_phase(run_id(1), tenant(1), GcPhase::Mark, 999)
        .unwrap_err();
    assert!(matches!(
        err,
        GcRunStoreError::InvalidPhaseTransition { .. }
    ));
}

#[test]
fn full_canonical_chain_run() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    for (to, t) in [
        (GcPhase::Mark, 300),
        (GcPhase::Sweep, 400),
        (GcPhase::PhysicalDelete, 500),
        (GcPhase::Reconcile, 600),
        (GcPhase::Completed, 700),
    ] {
        store.transition_phase(run_id(1), tenant(1), to, t).unwrap();
    }
    store
        .finalize(run_id(1), tenant(1), GcStatus::Succeeded, 800, None)
        .unwrap();
    let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
    assert_eq!(row.status, GcStatus::Succeeded);
    assert_eq!(row.phase, GcPhase::Completed);
    assert_eq!(row.completed_at_ms, Some(800));
    assert_eq!(row.mark_started_at_ms, Some(300));
}

#[test]
fn checkpoint_idempotent_re_run() {
    // After Crash, partial UNIQUE permits a NEW pending+running
    // row for the same (tenant, region) because the crashed row
    // is no longer Running.
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    store
        .finalize(
            run_id(1),
            tenant(1),
            GcStatus::Crashed,
            400,
            Some(FailureContext {
                failed_phase: GcPhase::Mark,
                failed_reason: "worker_panic".into(),
            }),
        )
        .unwrap();
    // New run can take the lock.
    store
        .insert_pending(run_id(2), tenant(1), GcRegion::Sam, 500, "cron".into())
        .unwrap();
    store.acquire_running(run_id(2), tenant(1), 600).unwrap();
    let new_row = store.current_running(tenant(1), GcRegion::Sam).unwrap();
    assert!(new_row.is_some());
    assert_eq!(new_row.unwrap().run_id, run_id(2));
}

#[test]
fn reverse_lifecycle_timestamp_rejected() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    // acquire_running with now < started_at_ms must reject.
    let err = store.acquire_running(run_id(1), tenant(1), 50).unwrap_err();
    assert!(matches!(err, GcRunStoreError::CheckViolation(_)));
}

#[test]
fn checkpoint_increments_counters() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    store.acquire_running(run_id(1), tenant(1), 200).unwrap();
    store
        .checkpoint(
            run_id(1),
            tenant(1),
            300,
            CheckpointDeltas {
                blobs_marked_delta: 1000,
                blobs_swept_delta: 0,
                blobs_physically_deleted_delta: 0,
                bytes_reclaimed_delta: 1024 * 1024,
            },
        )
        .unwrap();
    let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
    assert_eq!(row.blobs_marked_count, 1000);
    assert_eq!(row.bytes_reclaimed, 1024 * 1024);
}

#[test]
fn finalize_with_non_terminal_rejected() {
    let store = InMemoryGcRunStore::new();
    store
        .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
        .unwrap();
    let err = store
        .finalize(run_id(1), tenant(1), GcStatus::Running, 200, None)
        .unwrap_err();
    assert!(matches!(err, GcRunStoreError::CheckViolation(_)));
}

#[test]
fn run_id_text_form_is_canonical_uuid() {
    let r = RunId(Uuid::from_u128(0xdeadbeef_cafe_babe_1234_567890abcdef));
    let t = r.as_text();
    assert_eq!(t.len(), 36);
    assert!(t.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
}
