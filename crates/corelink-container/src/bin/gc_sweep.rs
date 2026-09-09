//! Native production GC sweep binary.
//!
//! This production-capable target is intentionally distinct from the
//! `corelink-gc` crate's shipped `gc_sweep` dry-run self-check. It has no
//! fixture fallback: missing storage or scope configuration exits non-zero
//! before any D1/R2 operation. Promotion into the image remains owner-gated.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the cron utility emits one operator-readable report"
)]

use std::process::ExitCode;

use corelink_gc::CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS;
use corelink_server::gc_sweep::{run_production, GcProductionConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match GcProductionConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("gc_sweep FAILED (fail-closed): {error}");
            return ExitCode::FAILURE;
        }
    };
    if std::env::var("GC_VALIDATE_ONLY").ok().as_deref() == Some("true") {
        println!(
            "gc_sweep configuration valid (dry-run; no D1/R2 operation) tenant={} region={} bucket={} max_candidates={}",
            config.tenant_id,
            config.region.as_str(),
            config.bucket,
            config.max_candidates
        );
        return ExitCode::SUCCESS;
    }
    println!(
        "gc_sweep mode={} observation_only={} tenant={} region={} run={} max_candidates={}",
        config.mode.as_str(),
        config.observation_only,
        config.tenant_id,
        config.region.as_str(),
        config
            .run_id
            .map(|run| run.to_string())
            .unwrap_or_else(|| "current-running".to_owned()),
        config.max_candidates
    );
    match run_production(&config) {
        Ok(report) => {
            let evidence = json!({
                "schema_version": 1,
                "mode": report.mode.as_str(),
                "observation_only": true,
                "run_id": report.run_id.as_text(),
                "tenant_id": report.tenant_id.to_string(),
                "region": report.region.as_str(),
                "candidates_scanned": report.candidates_scanned,
                "reclaimable_count": report.reclaimable_count,
                "reclaimable_bytes": report.reclaimable_bytes,
                "delete_count": report.deleted_count,
                "deleted_bytes": report.deleted_bytes,
                "skipped_grace_pending": report.skipped_grace_pending,
                "skipped_refcount_non_zero": report.skipped_refcount_non_zero,
                "already_resolved": report.already_resolved,
                "duration_ms": report.duration_ms,
                "observed_at_ms": report.now_ms,
                "phase_budget_ms": CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS,
                "max_candidates": config.max_candidates,
            });
            println!("gc_sweep report {evidence}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("gc_sweep FAILED (fail-closed): {error}");
            ExitCode::FAILURE
        }
    }
}
