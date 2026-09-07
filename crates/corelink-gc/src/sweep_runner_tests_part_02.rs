#[test]
fn unknown_run_fails_closed() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        GcSweepMode::DryRun,
    );
    let err = runner
        .run(
            RunId(Uuid::from_u128(0xdead)),
            Uuid::from_u128(1),
            GcRegion::Syd,
        )
        .unwrap_err();
    assert!(matches!(err, GcSweepError::PhysicalDelete(_)));
    assert!(report_sink.is_empty());
}

#[test]
fn region_mismatch_fails_closed_before_candidate_scan() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(0x71);
    let rid = RunId(Uuid::from_u128(0x72));
    seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, 300);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        GcSweepMode::DryRun,
    );

    let err = runner.run(rid, tenant, GcRegion::Iad).unwrap_err();
    assert!(matches!(
        err,
        GcSweepError::PhysicalDelete(PhysicalDeleteError::RegionMismatch { .. })
    ));
    assert!(report_sink.is_empty());
}

#[test]
fn dry_run_fails_closed_when_phase_budget_is_exceeded() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let tenant = Uuid::from_u128(0x73);
    let region = GcRegion::Sam;
    let rid = RunId(Uuid::from_u128(0x74));
    seed_run_in_physical_delete(&runs, rid, tenant, region, 300);
    let cfg = PhysicalDeleteConfig::new(10, 10, 1).unwrap();
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: digest(1),
            mark_started_at_ms: 300,
            mark_run_id: rid,
            blob_size_bytes: 1,
            blob_last_referenced_at_ms: 299,
            status: CandidateStatus::Candidate,
            created_at_ms: 300,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    let runner = GcSweepRunner::new(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        Arc::clone(&report_sink),
        cfg,
        GcSweepMode::DryRun,
    );

    let err = runner.run(rid, tenant, region).unwrap_err();
    assert!(matches!(
        err,
        GcSweepError::PhysicalDelete(PhysicalDeleteError::PhaseBudgetExceeded { .. })
    ));
    assert!(report_sink.is_empty());
}

/// Build a minimal but fully-populated [`SweepReport`] for sink tests.
fn sample_report() -> SweepReport {
    SweepReport {
        mode: GcSweepMode::DryRun,
        run_id: RunId(Uuid::from_u128(0xfeed)),
        tenant_id: Uuid::from_u128(42),
        region: GcRegion::Iad,
        candidates_scanned: 3,
        reclaimable_count: 2,
        reclaimable_bytes: 8_192,
        deleted_count: 0,
        deleted_bytes: 0,
        skipped_grace_pending: 1,
        skipped_refcount_non_zero: 0,
        already_resolved: 0,
        reclaimable_keys: vec![r2_key_for(Uuid::from_u128(42), &digest(1))],
        duration_ms: 5,
        now_ms: 1_005,
    }
}

#[test]
fn in_memory_sink_snapshot_and_is_empty_round_trip() {
    let sink = InMemoryGcSweepReportSink::new();

    // Fresh sink: empty, zero-length, empty snapshot.
    // (kills `is_empty -> true` only if it also flips to false later,
    //  and kills `snapshot -> vec![]` via the populated round-trip.)
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
    assert!(sink.snapshot().is_empty());

    let report = sample_report();
    sink.emit_report(&report).unwrap();

    // After one emit: NOT empty (kills `is_empty -> true`), len 1.
    assert!(!sink.is_empty());
    assert_eq!(sink.len(), 1);

    // snapshot returns the real captured report, not vec![]
    // (kills `snapshot -> Vec::new()`).
    let snap = sink.snapshot();
    assert_eq!(snap.len(), 1);
    let got = &snap[0];
    assert_eq!(got.mode, report.mode);
    assert_eq!(got.run_id, report.run_id);
    assert_eq!(got.tenant_id, report.tenant_id);
    assert_eq!(got.region, report.region);
    assert_eq!(got.candidates_scanned, report.candidates_scanned);
    assert_eq!(got.reclaimable_count, report.reclaimable_count);
    assert_eq!(got.reclaimable_bytes, report.reclaimable_bytes);
    assert_eq!(got.skipped_grace_pending, report.skipped_grace_pending);
    assert_eq!(got.reclaimable_keys, report.reclaimable_keys);

    // A second emit accumulates (snapshot reflects every report).
    sink.emit_report(&report).unwrap();
    assert_eq!(sink.len(), 2);
    assert_eq!(sink.snapshot().len(), 2);
}

#[test]
fn gc_sweep_runner_debug_renders_struct_name_and_mode() {
    let (runs, candidates, blob_meta, r2, audit, metrics, clock, report_sink) = fresh_deps(1_000);
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2,
        audit,
        metrics,
        clock,
        report_sink,
        GcSweepMode::DryRun,
    );

    // The real Debug impl is `debug_struct("GcSweepRunner").field("mode", ..)`;
    // the mutant returns `Ok(())` → an empty string → these fail.
    let rendered = format!("{runner:?}");
    assert!(
        rendered.contains("GcSweepRunner"),
        "debug output missing struct name: {rendered:?}"
    );
    assert!(
        rendered.contains("mode"),
        "debug output missing mode field: {rendered:?}"
    );
}
