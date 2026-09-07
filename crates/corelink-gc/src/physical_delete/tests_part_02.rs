#[test]
fn already_resolved_for_protected_re_ref() {
    // A candidate in ProtectedReRef state (sweep INV-GC-004) is
    // never physical-deleted; observed as AlreadyResolved.
    let (phase, _runs, candidates, blob_meta, r2, _audit) = fresh(1_000_000);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    let d = digest(0x5555);
    let mark_anchor = 500;
    seed_swept_candidate(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        GcRegion::Sam,
        rid,
        d.clone(),
        mark_anchor,
        64,
        mark_anchor + 100,
    );
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Swept,
            CandidateStatus::ProtectedReRef,
            mark_anchor + 200,
            Some("test_protected".to_owned()),
        )
        .unwrap();
    let row = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    let dec = phase.step_candidate(&row, GcRegion::Sam).unwrap();
    match dec {
        PhysicalDeleteDecision::AlreadyResolved { observed_status } => {
            assert_eq!(observed_status, CandidateStatus::ProtectedReRef);
        }
        other => panic!("expected AlreadyResolved, got {other:?}"),
    }
}

#[test]
fn already_resolved_when_blob_meta_undeleted() {
    // CAP-GC-002 reversibility: customer re-uploaded same digest;
    // CAS write handler reset deleted_at_ms = NULL. Physical-delete
    // sees no purge state → AlreadyResolved.
    let mark_anchor = 1_000;
    let now_start = mark_anchor + GRACE_CAS_MS + 10_000;
    let (phase, runs, candidates, _blob_meta, _r2, audit) = fresh(now_start);
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xfeed);
    // Insert a Swept candidate but DO NOT seed a soft-deleted
    // blob_meta row → the CAP-GC-002 undelete fast-path.
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes: 64,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            mark_anchor + 100,
            None,
        )
        .unwrap();
    let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_deleted_count, 0);
    assert_eq!(result.already_resolved_count, 1);
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
}

#[test]
fn r2_backend_failure_preserves_d1_row() {
    // Custom R2 trait that always errors; assert no D1 mutation
    // and no audit emit (orchestrator preserves the candidate row
    // for next-tick retry).
    #[derive(Debug, Default)]
    struct FailingR2;
    impl R2Delete for FailingR2 {
        fn delete(
            &self,
            _tenant_id: Uuid,
            _region: GcRegion,
            _key: &str,
        ) -> Result<R2DeleteOutcome, R2DeleteError> {
            Err(R2DeleteError::Backend("simulated_503".to_owned()))
        }
    }

    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaPurgeStore::new());
    let r2: Arc<FailingR2> = Arc::new(FailingR2);
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingPhysicalDeleteClock::new(now_start));
    let phase = InMemoryPhysicalDeletePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xdf01);
    let key = r2_key_for(tenant, &d);
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes: 4096,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            deleted_at_ms,
            None,
        )
        .unwrap();
    blob_meta.push_soft_deleted(tenant, d.clone(), key, 4096, deleted_at_ms);

    let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::R2BackendError(_)));
    // gc_candidate row preserved as Swept.
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::Swept);
    // blob_meta row preserved.
    assert!(blob_meta.contains(tenant, &d));
    // No audit emit.
    assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
}

#[test]
fn audit_emit_failure_aborts_step() {
    // Use a sink that always errors; assert the orchestrator
    // surfaces AuditEmissionFailed and the candidate is preserved.
    #[derive(Debug, Default)]
    struct FailingSink;
    impl GcAuditSink for FailingSink {
        fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
            Err(GcAuditSinkError::Store("simulated_audit_fail".to_owned()))
        }
    }

    let mark_anchor = 1_000;
    let deleted_at_ms = mark_anchor + 100;
    let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaPurgeStore::new());
    let r2 = Arc::new(InMemoryR2Delete::new());
    let audit = Arc::new(FailingSink);
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingPhysicalDeleteClock::new(now_start));
    let phase = InMemoryPhysicalDeletePhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    let tenant = Uuid::from_u128(1);
    let rid = RunId(Uuid::from_u128(2));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest(0xa11d);
    let key = r2_key_for(tenant, &d);
    r2.seed(tenant, GcRegion::Sam, key.clone());
    blob_meta.push_soft_deleted(tenant, d.clone(), key, 4096, deleted_at_ms);
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes: 4096,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    candidates
        .transition_status(
            tenant,
            &d,
            rid,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            deleted_at_ms,
            None,
        )
        .unwrap();

    let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
    assert!(matches!(err, PhysicalDeleteError::AuditEmissionFailed(_)));
    // gc_candidate row preserved as Swept.
    let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
    assert_eq!(cand.status, CandidateStatus::Swept);
    // R2 was called BEFORE audit emit (Lote 10.6bis P0-2 ordering),
    // so the R2 key is removed; production wiring's reconcile job
    // (WI-S06-005) detects the orphan blob_meta row in the next 24h.
}

#[test]
fn region_value_round_trip_in_audit() {
    // Sanity: every region appears in the audit emit.
    for region in GcRegion::all() {
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(u128::from(region.as_str().len() as u32) + 1);
        let rid = RunId(Uuid::from_u128(42));
        seed_run_in_physical_delete(&runs, rid, tenant, *region, mark_anchor);
        let d = digest(0xc0de);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            *region,
            rid,
            d,
            mark_anchor,
            256,
            deleted_at_ms,
        );
        let _ = phase.execute(rid, tenant, *region).unwrap();
        let recs = audit.snapshot_of(GcEventType::PhysicalDeleted);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].region, *region);
    }
}

#[test]
fn purge_state_lookup_returns_none_when_not_soft_deleted() {
    // A row whose deleted_at_ms = None → lookup_purge_state returns
    // None.
    let store = InMemoryBlobMetaPurgeStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0x9999);
    // No soft-delete; lookup returns None.
    let res = store.lookup_purge_state(tenant, &d).unwrap();
    assert!(res.is_none());
}

#[test]
fn conditional_purge_skips_when_grace_pending() {
    let store = InMemoryBlobMetaPurgeStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0x1234);
    let deleted_at_ms = 1_000;
    store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
    // now - deleted_at_ms == grace_period_ms exactly → strict `>` SKIP.
    let now = deleted_at_ms + GRACE_CAS_MS;
    let purged = store
        .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
        .unwrap();
    assert!(!purged);
    assert!(store.contains(tenant, &d));
}

#[test]
fn conditional_purge_fires_when_one_ms_past_grace() {
    let store = InMemoryBlobMetaPurgeStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0x5678);
    let deleted_at_ms = 1_000;
    store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
    // now - deleted_at_ms == grace_period_ms + 1 → strict `>` FIRES.
    let now = deleted_at_ms + GRACE_CAS_MS + 1;
    let purged = store
        .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
        .unwrap();
    assert!(purged);
    assert!(!store.contains(tenant, &d));
}

#[test]
fn conditional_purge_skips_when_refcount_non_zero() {
    let store = InMemoryBlobMetaPurgeStore::new();
    let tenant = Uuid::from_u128(1);
    let d = digest(0xab12);
    let deleted_at_ms = 1_000;
    store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
    assert!(store.set_refcount(tenant, &d, 1));
    // Past grace + refcount=1 → skip.
    let now = deleted_at_ms + GRACE_CAS_MS + 100_000;
    let purged = store
        .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
        .unwrap();
    assert!(!purged);
    assert!(store.contains(tenant, &d));
}
