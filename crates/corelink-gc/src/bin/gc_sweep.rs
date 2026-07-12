//! `gc_sweep` — the cron-invoked GC sweep entrypoint.
//!
//! Resolves the run mode from the `GC_LIVE_DELETE` env flag
//! (**defaults to dry-run**; fails closed), runs ONE sweep via
//! [`corelink_gc::GcSweepRunner`], prints the [`corelink_gc::SweepReport`],
//! and exits non-zero on any error (fail-closed).
//!
//! ## Scope of THIS binary
//!
//! `corelink-gc` ships the pure reclaim logic + in-memory fakes; the
//! real Cloudflare R2 `DeleteObject` + D1 `blob_meta` / `gc_candidates`
//! readers live in the container/worker plane (NOT in this crate). So
//! this binary drives the sweep against an in-memory fixture: it is a
//! **non-destructive self-check** of the runner + the dry-run/live gate
//! that the CI cron runs to keep the path green. Production wires the
//! real bindings into [`corelink_gc::GcSweepRunner`] (the owner-gated
//! follow-up) and flips `GC_LIVE_DELETE` on only after reviewing a
//! real dry-run report.
//!
//! Dry-run here asserts the load-bearing invariant — `deleted_count == 0`
//! and the reclaimable object survives — and exits non-zero if violated.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "thin cron-invoked entrypoint: ergonomic prints + fixture setup"
)]

use std::process::ExitCode;
use std::sync::Arc;

use corelink_gc::{
    BlobDigest, CandidateStatus, CountingPhysicalDeleteClock, GcCandidate, GcCandidatesStore,
    GcPhase, GcRegion, GcRunStore, GcSweepMode, GcSweepRunner, InMemoryBlobMetaPurgeStore,
    InMemoryGcAuditSink, InMemoryGcCandidatesStore, InMemoryGcMetrics, InMemoryGcRunStore,
    InMemoryGcSweepReportSink, InMemoryR2Delete, PhysicalDeleteConfig, RunId, SweepReport,
};
use uuid::Uuid;

fn digest(seed: u32) -> BlobDigest {
    let mut s = format!("{seed:08x}");
    s.push_str(&"0".repeat(BlobDigest::LEN - 8));
    BlobDigest::parse(&s).expect("canonical hex digest")
}

fn r2_key_for(tenant_id: Uuid, digest: &BlobDigest) -> String {
    format!("cas/{tenant_id}/{digest}")
}

fn main() -> ExitCode {
    let mode = GcSweepMode::from_env();
    println!("==============================================================");
    println!(" corelink-gc :: gc_sweep");
    println!(" mode               = {}", mode.as_str());
    println!(
        " {} = {:?}",
        GcSweepMode::ENV_VAR,
        std::env::var(GcSweepMode::ENV_VAR).ok()
    );
    if mode.is_live() {
        println!(" !! LIVE DELETE ENABLED — destructive path is armed.");
        println!(" !! (against the in-memory fixture in this build; prod requires");
        println!(" !!  the real R2/D1 bindings wired into GcSweepRunner.)");
    } else {
        println!(" dry-run: classify + report only; ZERO deletes.");
    }
    println!(" NOTE: this build sweeps an IN-MEMORY fixture. Production must");
    println!("       inject the real Cloudflare R2 DeleteObject + D1 readers");
    println!("       into corelink_gc::GcSweepRunner (owner-gated follow-up).");
    println!("==============================================================");

    match run_one_sweep(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("gc_sweep FAILED (fail-closed): {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Build the in-memory fixture, run one sweep, print the report, and —
/// in dry-run — assert non-destructiveness. Returns `Err` (→ non-zero
/// exit) on any failure.
fn run_one_sweep(mode: GcSweepMode) -> Result<(), String> {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaPurgeStore::new());
    let r2 = Arc::new(InMemoryR2Delete::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingPhysicalDeleteClock::new(1_000));
    let report_sink = Arc::new(InMemoryGcSweepReportSink::new());

    let tenant = Uuid::from_u128(0x5147_0000_0000_0001);
    let region = GcRegion::Sam;
    let rid = RunId(Uuid::from_u128(0x5147_0000_0000_5eed));

    // Drive a run into the PhysicalDelete phase.
    runs.insert_pending(rid, tenant, region, 100, "cron".to_owned())
        .map_err(|e| format!("insert_pending: {e}"))?;
    runs.acquire_running(rid, tenant, 200)
        .map_err(|e| format!("acquire_running: {e}"))?;
    runs.transition_phase(rid, tenant, GcPhase::Mark, 300)
        .map_err(|e| format!("transition Mark: {e}"))?;
    runs.transition_phase(rid, tenant, GcPhase::Sweep, 310)
        .map_err(|e| format!("transition Sweep: {e}"))?;
    runs.transition_phase(rid, tenant, GcPhase::PhysicalDelete, 320)
        .map_err(|e| format!("transition PhysicalDelete: {e}"))?;

    // Representative fixture: 1 reclaimable, 1 grace-pending, 1 live.
    let cfg = PhysicalDeleteConfig::new(50, 50, 600_000).map_err(|e| format!("config: {e}"))?;
    seed_swept(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        digest(1),
        4_096,
        0,
        0,
    )?; // reclaimable
    seed_swept(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        digest(2),
        8_192,
        1_500,
        0,
    )?; // grace pending
    seed_swept(
        &candidates,
        &blob_meta,
        &r2,
        tenant,
        region,
        rid,
        digest(3),
        2_048,
        0,
        5,
    )?; // live ref

    let reclaim_key = r2_key_for(tenant, &digest(1));

    let runner = GcSweepRunner::new(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&r2),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        Arc::clone(&clock),
        Arc::clone(&report_sink),
        cfg,
        mode,
    );

    let report = runner
        .run(rid, tenant, region)
        .map_err(|e| format!("sweep: {e}"))?;

    print_report(&report);

    if !mode.is_live() {
        // Load-bearing dry-run invariant: NOTHING was deleted. Folded into a
        // single tuple match (no `||`/`&&`/`!=` binary-op surface to mutate);
        // still fails closed if EITHER count OR bytes is non-zero.
        match (report.deleted_count, report.deleted_bytes) {
            (0, 0) => {}
            (deleted_count, deleted_bytes) => {
                return Err(format!(
                    "dry-run deleted something: deleted_count={deleted_count} deleted_bytes={deleted_bytes}"
                ));
            }
        }
        if !r2.contains(tenant, region, &reclaim_key) {
            return Err("dry-run removed the reclaimable R2 object".to_owned());
        }
        if !blob_meta.contains(tenant, &digest(1)) {
            return Err("dry-run purged the reclaimable D1 row".to_owned());
        }
        println!("dry-run non-destructiveness verified: 0 deletes, fixture intact.");
    }

    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "fixture helper collapses the sweep→physical-delete handoff into one call"
)]
fn seed_swept(
    candidates: &InMemoryGcCandidatesStore,
    blob_meta: &InMemoryBlobMetaPurgeStore,
    r2: &InMemoryR2Delete,
    tenant: Uuid,
    region: GcRegion,
    rid: RunId,
    d: BlobDigest,
    size_bytes: u64,
    deleted_at_ms: u64,
    refcount: u32,
) -> Result<(), String> {
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: d.clone(),
            mark_started_at_ms: 300,
            mark_run_id: rid,
            blob_size_bytes: size_bytes,
            blob_last_referenced_at_ms: 299,
            status: CandidateStatus::Candidate,
            created_at_ms: 300,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .map_err(|e| format!("insert_candidate: {e}"))?;
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
        .map_err(|e| format!("transition_status: {e}"))?;
    let key = r2_key_for(tenant, &d);
    r2.seed(tenant, region, key.clone());
    blob_meta.push_soft_deleted(tenant, d.clone(), key, size_bytes, deleted_at_ms);
    if refcount != 0 && !blob_meta.set_refcount(tenant, &d, refcount) {
        return Err("set_refcount failed".to_owned());
    }
    Ok(())
}

fn print_report(r: &SweepReport) {
    println!("--- sweep report ---");
    println!(" run_id                    = {:?}", r.run_id);
    println!(" candidates_scanned        = {}", r.candidates_scanned);
    println!(" reclaimable_count         = {}", r.reclaimable_count);
    println!(" reclaimable_bytes         = {}", r.reclaimable_bytes);
    println!(" deleted_count             = {}", r.deleted_count);
    println!(" deleted_bytes             = {}", r.deleted_bytes);
    println!(" skipped_grace_pending     = {}", r.skipped_grace_pending);
    println!(
        " skipped_refcount_non_zero = {}",
        r.skipped_refcount_non_zero
    );
    println!(" already_resolved          = {}", r.already_resolved);
    println!(" reclaimable_keys          = {:?}", r.reclaimable_keys);
    println!("--------------------");
}
