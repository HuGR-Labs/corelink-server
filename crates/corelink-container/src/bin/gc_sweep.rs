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

use corelink_server::gc_sweep::{run_production, GcProductionConfig};

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
            "gc_sweep configuration valid (dry-run; no D1/R2 operation) tenant={} region={} bucket={}",
            config.tenant_id,
            config.region.as_str(),
            config.bucket
        );
        return ExitCode::SUCCESS;
    }
    let storage = match corelink_server::storage::StorageEnv::from_env() {
        Some(storage) => storage,
        None => {
            eprintln!("gc_sweep FAILED (fail-closed): durable D1/R2 StorageEnv is incomplete");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "gc_sweep mode={} observation_only={} tenant={} region={} run={}",
        config.mode.as_str(),
        config.observation_only,
        config.tenant_id,
        config.region.as_str(),
        config
            .run_id
            .map(|run| run.to_string())
            .unwrap_or_else(|| "current-running".to_owned())
    );
    match run_production(&storage, &config).await {
        Ok(report) => {
            println!(
                "gc_sweep report scanned={} reclaimable={} reclaimable_bytes={} deleted={} deleted_bytes={}",
                report.candidates_scanned,
                report.reclaimable_count,
                report.reclaimable_bytes,
                report.deleted_count,
                report.deleted_bytes
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("gc_sweep FAILED (fail-closed): {error}");
            ExitCode::FAILURE
        }
    }
}
