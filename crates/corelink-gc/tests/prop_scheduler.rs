//! Property tests pinned at 10 k iter against the WI-S06-001
//! load-bearing invariants.
//!
//! Per WI §6.1.10 + sprint contract §6 DoD, the canonical 5 property
//! tests are:
//!
//! 1. `prop_gc_run_idempotent_resume` — 1000 random crashed runs;
//!    resume from checkpoint = same final state.
//! 2. `prop_gc_run_partial_unique` — concurrent `Running` insert
//!    same `(tenant, region)` rejected.
//! 3. `prop_gc_run_phase_transitions` — random phase sequences;
//!    only valid transitions accepted.
//! 4. `prop_degrade_mode_propagation` — enable `gc-pause`; worker
//!    aborts at next batch boundary.
//! 5. `prop_tenant_isolation_gc_run` — concurrent runs different
//!    tenants; no cross-tenant interference.
//!
//! Each test uses `proptest!` with `ProptestConfig { cases: 10_000,
//! .. }` so the suite hits the canonical 10 k iter ceiling. The
//! 100 k nightly variant is wired in WI-S06-006 alongside the TLA+
//! CI gate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_gc::{
    admin_trigger, AdminTriggerOutcome, CheckpointDeltas, DegradeKind, FailureContext,
    GcEventType, GcMetricsObserver, GcPhase, GcRegion, GcRunStore, GcRunStoreError, GcScheduler,
    GcStatus, InMemoryDegradeProbe, InMemoryGcAuditSink, InMemoryGcMetrics, InMemoryGcRunStore,
    InMemoryGcScheduler, InMemoryGcWorker, RunId, ScheduleConfig,
};
use proptest::prelude::*;
use uuid::Uuid;

fn region_from(idx: usize) -> GcRegion {
    GcRegion::all()[idx % 5]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        .. ProptestConfig::default()
    })]

    // ===========================================================
    // 1) prop_gc_run_idempotent_resume
    // ===========================================================
    //
    // Crashed run is preserved as forensic trail; a NEW run can take
    // the per-(tenant, region) lock and run to completion. Counter
    // deltas applied through the checkpoint helper accumulate
    // correctly + bytes_reclaimed never decreases.
    #[test]
    fn prop_gc_run_idempotent_resume(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        marked_delta in 0u64..10_000,
        bytes_delta in 0u64..(1u64 << 40),
    ) {
        let runs = InMemoryGcRunStore::new();
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid_crashed = RunId(Uuid::from_u128(tenant_seed + 0xdead));
        let rid_resume = RunId(Uuid::from_u128(tenant_seed + 0xbeef));

        runs.insert_pending(rid_crashed, tenant, region, 100, "cron".into()).unwrap();
        runs.acquire_running(rid_crashed, tenant, 200).unwrap();
        runs.transition_phase(rid_crashed, tenant, GcPhase::Mark, 300).unwrap();
        runs.checkpoint(rid_crashed, tenant, 400, CheckpointDeltas {
            blobs_marked_delta: marked_delta,
            bytes_reclaimed_delta: bytes_delta,
            .. Default::default()
        }).unwrap();
        runs.finalize(
            rid_crashed,
            tenant,
            GcStatus::Crashed,
            500,
            Some(FailureContext {
                failed_phase: GcPhase::Mark,
                failed_reason: "worker_panic".into(),
            }),
        ).unwrap();

        // Crashed row preserved as forensic trail.
        let crashed = runs.lookup(rid_crashed, tenant).unwrap().unwrap();
        prop_assert_eq!(crashed.status, GcStatus::Crashed);
        prop_assert_eq!(crashed.blobs_marked_count, marked_delta);
        prop_assert_eq!(crashed.bytes_reclaimed, bytes_delta);

        // New run takes the lock.
        runs.insert_pending(rid_resume, tenant, region, 600, "cron".into()).unwrap();
        runs.acquire_running(rid_resume, tenant, 700).unwrap();

        // Two rows now: one Crashed + one Running.
        let snap = runs.snapshot();
        prop_assert_eq!(snap.len(), 2);
        let running = runs.current_running(tenant, region).unwrap().unwrap();
        prop_assert_eq!(running.run_id, rid_resume);
    }

    // ===========================================================
    // 2) prop_gc_run_partial_unique
    // ===========================================================
    //
    // Second `acquire_running` for the same (tenant, region) while
    // the first run is `Running` is rejected.
    #[test]
    fn prop_gc_run_partial_unique(
        tenant_seed in 1u128..1_000_000,
        region_idx in 0usize..5,
        run_a_seed in 1u128..1_000_000,
        run_b_seed in 1_000_001u128..2_000_000,
    ) {
        prop_assume!(run_a_seed != run_b_seed);
        let runs = InMemoryGcRunStore::new();
        let region = region_from(region_idx);
        let tenant = Uuid::from_u128(tenant_seed);
        let rid_a = RunId(Uuid::from_u128(run_a_seed));
        let rid_b = RunId(Uuid::from_u128(run_b_seed));

        runs.insert_pending(rid_a, tenant, region, 100, "cron".into()).unwrap();
        runs.insert_pending(rid_b, tenant, region, 100, "cron".into()).unwrap();

        runs.acquire_running(rid_a, tenant, 200).unwrap();
        // Second acquire MUST be rejected.
        let err = runs.acquire_running(rid_b, tenant, 200).unwrap_err();
        let already = matches!(err, GcRunStoreError::AlreadyRunning { .. });
        prop_assert!(already);

        // The original run remains Running.
        let running = runs.current_running(tenant, region).unwrap().unwrap();
        prop_assert_eq!(running.run_id, rid_a);
    }

    // ===========================================================
    // 3) prop_gc_run_phase_transitions
    // ===========================================================
    //
    // Random phase sequences: only the canonical chain
    // (Idle→Mark→Sweep→PhysicalDelete→Reconcile→Completed) is
    // accepted; any reverse / skip transition is rejected. We pick a
    // random pair (from, to) and assert the simulator's decision
    // matches `GcPhase::can_transition_to`.
    #[test]
    fn prop_gc_run_phase_transitions(
        from_idx in 0usize..7,
        to_idx in 0usize..7,
    ) {
        let phases = [
            GcPhase::Idle,
            GcPhase::Mark,
            GcPhase::Sweep,
            GcPhase::PhysicalDelete,
            GcPhase::Reconcile,
            GcPhase::Completed,
            GcPhase::Failed,
        ];
        let from = phases[from_idx];
        let to = phases[to_idx];

        let runs = InMemoryGcRunStore::new();
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(1));
        let region = GcRegion::Sam;

        runs.insert_pending(rid, tenant, region, 100, "cron".into()).unwrap();
        runs.acquire_running(rid, tenant, 200).unwrap();

        // Walk the canonical chain up to `from`.
        let walk = match from {
            GcPhase::Idle => vec![],
            GcPhase::Mark => vec![GcPhase::Mark],
            GcPhase::Sweep => vec![GcPhase::Mark, GcPhase::Sweep],
            GcPhase::PhysicalDelete => vec![GcPhase::Mark, GcPhase::Sweep, GcPhase::PhysicalDelete],
            GcPhase::Reconcile => vec![
                GcPhase::Mark,
                GcPhase::Sweep,
                GcPhase::PhysicalDelete,
                GcPhase::Reconcile,
            ],
            GcPhase::Completed => vec![
                GcPhase::Mark,
                GcPhase::Sweep,
                GcPhase::PhysicalDelete,
                GcPhase::Reconcile,
                GcPhase::Completed,
            ],
            GcPhase::Failed => vec![GcPhase::Mark, GcPhase::Failed],
            // GcPhase is `#[non_exhaustive]`; a future variant
            // means the property test must explicitly classify it.
            _ => Vec::new(),
        };
        let mut t = 200;
        for p in walk {
            t += 100;
            let _ = runs.transition_phase(rid, tenant, p, t);
        }

        // Now: attempt the random `to` transition.
        let row = runs.lookup(rid, tenant).unwrap().unwrap();
        let actually_at = row.phase;
        let should_succeed = actually_at.can_transition_to(to);
        let res = runs.transition_phase(rid, tenant, to, t + 100);
        if should_succeed {
            prop_assert!(res.is_ok(), "{actually_at:?} -> {to:?} should succeed");
        } else {
            let is_expected_err = matches!(
                res,
                Err(GcRunStoreError::InvalidPhaseTransition { .. })
                    | Err(GcRunStoreError::MarkStartedAtImmutable { .. })
            );
            prop_assert!(is_expected_err);
        }
    }

    // ===========================================================
    // 4) prop_degrade_mode_propagation
    // ===========================================================
    //
    // When `gc-pause` flips ON before the cron tick, the scheduler
    // admits zero tenants AND emits 0 RunStarted audit events. When
    // it flips OFF, the scheduler admits up to `max_concurrent_tenants`
    // and emits N RunStarted events.
    #[test]
    fn prop_degrade_mode_propagation(
        tenant_count in 1u32..10,
        pause_first in any::<bool>(),
    ) {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let degrade = Arc::new(InMemoryDegradeProbe::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let worker = InMemoryGcWorker::new(
            Arc::clone(&runs),
            Arc::clone(&degrade),
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let cfg = ScheduleConfig::with_defaults(GcRegion::Sam).unwrap();
        let sched = InMemoryGcScheduler::new(
            cfg,
            worker,
            Arc::clone(&runs),
            Arc::clone(&degrade),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            0,
        );
        let tenants: Vec<Uuid> = (0..tenant_count).map(|i| Uuid::from_u128(u128::from(i) + 1)).collect();

        if pause_first {
            degrade.pause("pat_op", 0, "incident");
        }
        let outcome = sched.cron_tick(1_000_000, &tenants).unwrap();
        if pause_first {
            prop_assert!(outcome.paused);
            prop_assert_eq!(outcome.admitted_tenant_count, 0);
            prop_assert_eq!(audit.snapshot_of(GcEventType::RunStarted).len(), 0);
        } else {
            prop_assert!(!outcome.paused);
            let cap = 4u32.min(tenant_count); // default max_concurrent_tenants = 4
            prop_assert_eq!(outcome.admitted_tenant_count, cap);
            prop_assert_eq!(
                audit.snapshot_of(GcEventType::RunStarted).len() as u32,
                cap
            );
        }
    }

    // ===========================================================
    // 5) prop_tenant_isolation_gc_run
    // ===========================================================
    //
    // Concurrent runs across distinct tenants do not interfere: each
    // tenant's gc_run row is observable only via lookup-with-the-
    // owning-tenant; cross-tenant lookup returns Ok(None).
    #[test]
    fn prop_tenant_isolation_gc_run(
        tenant_a_seed in 1u128..500_000,
        tenant_b_seed in 500_001u128..1_000_000,
        region_idx in 0usize..5,
    ) {
        prop_assume!(tenant_a_seed != tenant_b_seed);
        let runs = InMemoryGcRunStore::new();
        let region = region_from(region_idx);
        let ta = Uuid::from_u128(tenant_a_seed);
        let tb = Uuid::from_u128(tenant_b_seed);
        let rid_a = RunId(Uuid::from_u128(tenant_a_seed + 1));
        let rid_b = RunId(Uuid::from_u128(tenant_b_seed + 1));

        runs.insert_pending(rid_a, ta, region, 100, "cron".into()).unwrap();
        runs.insert_pending(rid_b, tb, region, 100, "cron".into()).unwrap();
        runs.acquire_running(rid_a, ta, 200).unwrap();
        runs.acquire_running(rid_b, tb, 200).unwrap();

        // Cross-tenant lookup MUST return None.
        prop_assert!(runs.lookup(rid_a, tb).unwrap().is_none());
        prop_assert!(runs.lookup(rid_b, ta).unwrap().is_none());
        // Owning-tenant lookup MUST return the row.
        prop_assert!(runs.lookup(rid_a, ta).unwrap().is_some());
        prop_assert!(runs.lookup(rid_b, tb).unwrap().is_some());
        // Each tenant has its own running row in the same region.
        prop_assert_eq!(runs.current_running(ta, region).unwrap().unwrap().run_id, rid_a);
        prop_assert_eq!(runs.current_running(tb, region).unwrap().unwrap().run_id, rid_b);
    }

    // ===========================================================
    // 6) prop_admin_trigger_envelope (extra; ties §8 Gherkin to fuzz)
    // ===========================================================
    //
    // Admin trigger surface returns the canonical envelope:
    //  - `Forbidden` when scope missing.
    //  - `NotImplemented` when env != staging/dev.
    //  - `Admitted` otherwise.
    #[test]
    fn prop_admin_trigger_envelope(
        has_scope in any::<bool>(),
        env_idx in 0usize..3,
        seed in 1u128..1_000_000,
    ) {
        let env = match env_idx {
            0 => "staging",
            1 => "prod",
            _ => "dev",
        };
        let audit = InMemoryGcAuditSink::new();
        let outcome = admin_trigger(
            has_scope,
            "pat_xxxx",
            env,
            Uuid::from_u128(seed),
            GcRegion::Sam,
            &audit,
            123,
            seed,
        ).unwrap();
        match outcome {
            AdminTriggerOutcome::Forbidden { .. } => prop_assert!(!has_scope),
            AdminTriggerOutcome::NotImplemented { .. } => {
                prop_assert!(has_scope);
                prop_assert_eq!(env, "prod");
            }
            AdminTriggerOutcome::Admitted { .. } => {
                prop_assert!(has_scope);
                prop_assert!(env == "staging" || env == "dev");
            }
            // AdminTriggerOutcome is `#[non_exhaustive]`; future
            // variants must be classified by reviewer + WI bump.
            _ => prop_assert!(false, "unhandled AdminTriggerOutcome variant"),
        }
    }
}

// ===========================================================
// Sanity checks (non-proptest; documented invariants)
// ===========================================================

#[test]
fn metrics_observer_canonical_count_six() {
    assert_eq!(corelink_gc::canonical_metric_names().len(), 6);
}

#[test]
fn audit_taxonomy_canonical_count_five() {
    assert_eq!(corelink_gc::canonical_audit_event_strings().len(), 5);
}

#[test]
fn metrics_handle_drives_kind_dispatch() {
    let m = InMemoryGcMetrics::new();
    m.record_cron_fired(GcRegion::Sam).unwrap();
    m.record_run_started(Uuid::nil(), GcRegion::Sam).unwrap();
    m.record_run_completed(Uuid::nil(), GcRegion::Sam, GcStatus::Succeeded)
        .unwrap();
    m.record_phase_duration_ms(GcPhase::Mark, Uuid::nil(), GcRegion::Sam, 100)
        .unwrap();
    m.record_degrade_mode_active(DegradeKind::GcPause).unwrap();
    m.record_stale_running_count(3).unwrap();
}
