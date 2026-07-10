//! Manual admin trigger for `POST /v1/admin/gc/trigger`.
//!
//! WI-S06-001 §6.1.6. Two entry points:
//!
//! - [`admin_trigger`] — the ORIGINAL staging-stub envelope (403 / 501 /
//!   200) that mints a run id + emits audit WITHOUT driving a real GC
//!   run. It returns **501** outside staging because it has no scheduler
//!   to delegate to. Retained for the staging smoke path + the property
//!   suite that pins its 403/501/200 contract.
//! - [`admin_trigger_scheduled`] — the REAL, scheduler-driven path. Given
//!   a live [`crate::scheduler::GcScheduler`] it drives a genuine
//!   single-tenant GC run (`cron_tick(now_ms, &[tenant])`: insert_pending
//!   → acquire_running → worker.execute_run) in ANY environment, prod
//!   included. This is the production wiring the stub's 501 deferred; the
//!   remaining S-13 admin-plane work is only the HTTP mount + the
//!   prod-scheduler instantiation (D1-backed run store), NOT the trigger
//!   logic itself.
//!
//! Canonical stub envelope ([`admin_trigger`]):
//!
//! - **403** when the supplied PAT lacks the `gc:trigger` scope.
//! - **501** when the environment is not staging (`prod` returns
//!   501 until a scheduler is wired — use [`admin_trigger_scheduled`]).
//! - **200** otherwise — the trigger acquires a fresh gc_run row
//!   per the canonical scheduler path.

use uuid::Uuid;

use crate::audit::{GcAuditRecord, GcAuditSink, GcEventType};
use crate::error::GcError;
use crate::region::GcRegion;
use crate::run::{GcPhase, GcStatus, RunId};
use crate::scheduler::{CronTickOutcome, GcScheduler, GcSchedulerError};

/// Canonical admin trigger outcome envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdminTriggerOutcome {
    /// 200 — trigger admitted; worker spawned with the supplied
    /// `run_id`.
    Admitted {
        /// The newly-minted run identifier.
        run_id: RunId,
    },
    /// 403 — PAT lacked the `gc:trigger` scope.
    Forbidden {
        /// PAT id hex prefix (no raw secret).
        pat_id_hex: String,
    },
    /// 501 — environment is not staging.
    NotImplemented {
        /// Environment label that rejected the trigger.
        environment: String,
    },
}

/// Drive the canonical staging-stub admin trigger.
///
/// # Parameters
///
/// - `pat_has_gc_trigger_scope` — caller-validated boolean (S-03 PAT
///   middleware decodes the `gc:trigger` bit; S-13 admin plane
///   re-validates).
/// - `pat_id_hex` — short hex prefix of the PAT id (audit forensics;
///   no raw secret).
/// - `environment` — current deployment environment (`"staging"` /
///   `"prod"` / `"dev"`).
/// - `tenant_id` — target tenant.
/// - `region` — target region.
/// - `audit` — audit sink for the canonical
///   `corelink.gc.run_started` (or `corelink.gc.run_aborted` for the
///   403 / 501 paths).
/// - `now_ms` — wall-clock instant.
/// - `run_id_seed` — caller-supplied seed for the minted run id
///   (production: UUIDv7 minted from the request id).
///
/// # Errors
///
/// Surfaces as [`GcError::UnauthorizedTrigger`] /
/// [`GcError::NotImplementedInEnvironment`] when the caller wants to
/// promote the structured envelope into the canonical error
/// taxonomy. The default envelope is returned via [`AdminTriggerOutcome`].
#[allow(
    clippy::too_many_arguments,
    reason = "canonical admin_trigger envelope per WI-S06-001 §6.1.6 — every parameter is a load-bearing input from the S-13 admin plane wiring; refactoring into a struct would obscure the canonical 403/200/501 contract"
)]
pub fn admin_trigger<A: GcAuditSink>(
    pat_has_gc_trigger_scope: bool,
    pat_id_hex: &str,
    environment: &str,
    tenant_id: Uuid,
    region: GcRegion,
    audit: &A,
    now_ms: u64,
    run_id_seed: u128,
) -> Result<AdminTriggerOutcome, GcError> {
    // 403 path — PAT scope check.
    if !pat_has_gc_trigger_scope {
        // Audit emit so the S-09 audit chain captures the denial.
        audit.emit(GcAuditRecord {
            event_type: GcEventType::RunAborted,
            run_id: RunId(Uuid::from_u128(run_id_seed)),
            tenant_id,
            region,
            status: GcStatus::Aborted,
            from_phase: Some(GcPhase::Idle),
            to_phase: None,
            created_by_request_id: pat_id_hex.to_owned(),
            reason: "admin_trigger_unauthorized",
            now_ms,
        })?;
        return Ok(AdminTriggerOutcome::Forbidden {
            pat_id_hex: pat_id_hex.to_owned(),
        });
    }

    // 501 path — non-staging env.
    let is_staging = matches!(environment, "staging" | "dev");
    if !is_staging {
        return Ok(AdminTriggerOutcome::NotImplemented {
            environment: environment.to_owned(),
        });
    }

    // 200 path — admit. Production wiring delegates to the canonical
    // scheduler entry point; the stub returns the minted run_id
    // directly.
    let rid = RunId(Uuid::from_u128(run_id_seed));
    audit.emit(GcAuditRecord {
        event_type: GcEventType::RunStarted,
        run_id: rid,
        tenant_id,
        region,
        status: GcStatus::Running,
        from_phase: Some(GcPhase::Idle),
        to_phase: None,
        created_by_request_id: pat_id_hex.to_owned(),
        reason: "admin_trigger_admitted",
        now_ms,
    })?;
    Ok(AdminTriggerOutcome::Admitted { run_id: rid })
}

/// Outcome of the REAL, scheduler-driven admin trigger
/// ([`admin_trigger_scheduled`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduledTriggerOutcome {
    /// 403 — PAT lacked the `gc:trigger` scope. No GC run was driven.
    Forbidden {
        /// PAT id hex prefix (no raw secret).
        pat_id_hex: String,
    },
    /// The trigger drove a real single-tenant cron tick through the
    /// supplied scheduler; carries the canonical [`CronTickOutcome`]
    /// (`admitted_tenant_count` is 1 when the run started, 0 when it was
    /// skipped as already-running or degrade-`paused`).
    Ran(CronTickOutcome),
}

/// Drive the REAL admin trigger against a live [`GcScheduler`].
///
/// Unlike [`admin_trigger`] (the staging stub that returns 501 outside
/// staging), this delegates to the canonical scheduler path — a manual
/// admin trigger is exactly a single-tenant cron tick — so it performs a
/// genuine GC run in ANY environment (prod included):
/// `scheduler.cron_tick(now_ms, &[tenant_id])` runs insert_pending →
/// acquire_running → `worker.execute_run` for the one target tenant,
/// with the scheduler's own degrade-probe / audit / metrics wiring.
///
/// # Parameters
///
/// Same PAT-scope + audit-forensics inputs as [`admin_trigger`]; `region`
/// is the target region (the scheduler is region-pinned to the same one)
/// and feeds the 403-path audit record. `scheduler` is the live
/// region-pinned scheduler the trigger drives.
///
/// # Errors
///
/// [`GcSchedulerError`] from the underlying `cron_tick` (degrade-probe /
/// run-store / audit / metrics backend faults), or the wrapped
/// [`GcError`] from the 403-path audit emit.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors admin_trigger's canonical envelope inputs plus the live scheduler collaborator; every parameter is load-bearing"
)]
pub fn admin_trigger_scheduled<S, A>(
    pat_has_gc_trigger_scope: bool,
    pat_id_hex: &str,
    tenant_id: Uuid,
    region: GcRegion,
    audit: &A,
    now_ms: u64,
    run_id_seed: u128,
    scheduler: &S,
) -> Result<ScheduledTriggerOutcome, GcSchedulerError>
where
    S: GcScheduler,
    A: GcAuditSink,
{
    // 403 path — PAT scope check (fail-CLOSED before any scheduler work).
    if !pat_has_gc_trigger_scope {
        audit
            .emit(GcAuditRecord {
                event_type: GcEventType::RunAborted,
                run_id: RunId(Uuid::from_u128(run_id_seed)),
                tenant_id,
                region,
                status: GcStatus::Aborted,
                from_phase: Some(GcPhase::Idle),
                to_phase: None,
                created_by_request_id: pat_id_hex.to_owned(),
                reason: "admin_trigger_unauthorized",
                now_ms,
            })
            .map_err(|e| GcSchedulerError::Gc(GcError::from(e)))?;
        return Ok(ScheduledTriggerOutcome::Forbidden {
            pat_id_hex: pat_id_hex.to_owned(),
        });
    }

    // Real path — drive a single-tenant cron tick. The scheduler mints the
    // run id, records metrics, emits the RunStarted audit, and delegates to
    // the worker; degrade-`gc-pause` surfaces as `paused` (admitted 0).
    let outcome = scheduler.cron_tick(now_ms, &[tenant_id])?;
    Ok(ScheduledTriggerOutcome::Ran(outcome))
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
    use crate::audit::InMemoryGcAuditSink;

    #[test]
    fn unauthorized_trigger_returns_forbidden() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            false,
            "pat_aaaa",
            "staging",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Forbidden { .. }));
        let snap = audit.snapshot_of(GcEventType::RunAborted);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].reason, "admin_trigger_unauthorized");
    }

    #[test]
    fn prod_returns_not_implemented() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "prod",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::NotImplemented { .. }));
        // Audit emit for the 200 path is suppressed; the 403 path is
        // also not entered → audit sink remains empty.
        assert!(audit.is_empty());
    }

    #[test]
    fn staging_admits() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "staging",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Admitted { .. }));
        let snap = audit.snapshot_of(GcEventType::RunStarted);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].reason, "admin_trigger_admitted");
    }

    #[test]
    fn dev_admits_too() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "dev",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Admitted { .. }));
    }

    // ── Scheduler-driven REAL admin trigger ──────────────────────────────────

    use std::sync::Arc;

    use crate::degrade::InMemoryDegradeProbe;
    use crate::metrics::InMemoryGcMetrics;
    use crate::run::{GcRunStore, InMemoryGcRunStore};
    use crate::schedule::ScheduleConfig;
    use crate::scheduler::InMemoryGcScheduler;
    use crate::worker::InMemoryGcWorker;

    type TestScheduler = InMemoryGcScheduler<
        InMemoryGcWorker<
            InMemoryGcRunStore,
            InMemoryDegradeProbe,
            InMemoryGcAuditSink,
            InMemoryGcMetrics,
        >,
        InMemoryGcRunStore,
        InMemoryDegradeProbe,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
    >;

    /// Build a real in-memory scheduler pinned to `region`; return it plus its
    /// run store + degrade probe + the scheduler's OWN audit sink so tests can
    /// observe the durable effect (RunStarted) and the degrade-pause path.
    fn scheduler_fixture(
        region: GcRegion,
    ) -> (
        TestScheduler,
        Arc<InMemoryGcRunStore>,
        Arc<InMemoryDegradeProbe>,
        Arc<InMemoryGcAuditSink>,
    ) {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let degrade = Arc::new(InMemoryDegradeProbe::new());
        let sched_audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let worker = InMemoryGcWorker::new(
            Arc::clone(&runs),
            Arc::clone(&degrade),
            Arc::clone(&sched_audit),
            Arc::clone(&metrics),
        );
        let cfg = ScheduleConfig::with_defaults(region).unwrap();
        let sched = InMemoryGcScheduler::new(
            cfg,
            worker,
            Arc::clone(&runs),
            Arc::clone(&degrade),
            Arc::clone(&sched_audit),
            metrics,
            0x5ced_u128,
        );
        (sched, runs, degrade, sched_audit)
    }

    #[test]
    fn scheduled_trigger_forbidden_without_scope_drives_no_run() {
        let (sched, _runs, _degrade, sched_audit) = scheduler_fixture(GcRegion::Sam);
        let trigger_audit = InMemoryGcAuditSink::new();
        let tenant = Uuid::from_u128(0xa11ce);
        let out = admin_trigger_scheduled(
            false, "pat_bbbb", tenant, GcRegion::Sam, &trigger_audit, 1_000, 0xbbbb, &sched,
        )
        .unwrap();
        assert!(matches!(out, ScheduledTriggerOutcome::Forbidden { .. }));
        // A denial emits the RunAborted forensic record …
        let snap = trigger_audit.snapshot_of(GcEventType::RunAborted);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].reason, "admin_trigger_unauthorized");
        // … and drives NO real GC run (the scheduler never started one).
        assert!(sched_audit.snapshot_of(GcEventType::RunStarted).is_empty());
    }

    #[test]
    fn scheduled_trigger_runs_real_gc_for_tenant() {
        let (sched, _runs, _degrade, sched_audit) = scheduler_fixture(GcRegion::Iad);
        let trigger_audit = InMemoryGcAuditSink::new();
        let tenant = Uuid::from_u128(0xf00d);
        let out = admin_trigger_scheduled(
            true, "pat_cccc", tenant, GcRegion::Iad, &trigger_audit, 2_000, 0xcccc, &sched,
        )
        .unwrap();
        // The real scheduler only increments `admitted` AFTER insert_pending →
        // acquire_running → worker.execute_run all succeed, so admitted==1 is
        // proof the full GC run path executed for the tenant.
        match out {
            ScheduledTriggerOutcome::Ran(outcome) => {
                assert_eq!(outcome.admitted_tenant_count, 1);
                assert!(!outcome.paused);
                assert_eq!(outcome.region, GcRegion::Iad);
            }
            other => panic!("expected Ran, got {other:?}"),
        }
        // Durable effect: the scheduler emitted a RunStarted for THIS tenant
        // (the worker then drives it to completion, so the row is no longer
        // `current_running` by the time the tick returns).
        let started = sched_audit.snapshot_of(GcEventType::RunStarted);
        assert!(
            started.iter().any(|r| r.tenant_id == tenant),
            "scheduler must have started a real run for the tenant"
        );
    }

    #[test]
    fn scheduled_trigger_respects_degrade_pause() {
        let (sched, runs, degrade, sched_audit) = scheduler_fixture(GcRegion::Lhr);
        degrade.pause("pat_op", 1_000_000, "incident_admin");
        let trigger_audit = InMemoryGcAuditSink::new();
        let tenant = Uuid::from_u128(0xdead);
        let out = admin_trigger_scheduled(
            true, "pat_dddd", tenant, GcRegion::Lhr, &trigger_audit, 3_000, 0xdddd, &sched,
        )
        .unwrap();
        match out {
            ScheduledTriggerOutcome::Ran(outcome) => {
                assert!(outcome.paused);
                assert_eq!(outcome.admitted_tenant_count, 0);
            }
            other => panic!("expected Ran(paused), got {other:?}"),
        }
        // Degrade-pause admits nothing: no run started, no running row.
        assert!(sched_audit.snapshot_of(GcEventType::RunStarted).is_empty());
        assert!(runs
            .current_running(tenant, GcRegion::Lhr)
            .unwrap()
            .is_none());
    }
}
