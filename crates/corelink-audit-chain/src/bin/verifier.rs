//! Daily-verify CLI binary (Wave 15 GA).
//!
//! Usage:
//!
//! ```sh
//! cargo run -p corelink-audit-chain --bin verifier -- <ndjson_chunk_path>...
//! ```
//!
//! For each NDJSON chunk path passed on the CLI, this binary reads the
//! file, parses one [`corelink_audit_chain::AuditEvent`] per line, groups
//! by `tenant_id`, and runs
//! [`corelink_audit_chain::ChainVerifier::verify_chain_from_genesis`]
//! against each tenant's per-tenant slice (concatenated across chunks in
//! sequence order). Exits with code:
//!
//! - `0` if every tenant chain verifies clean.
//! - `1` on chain break / verification error (SEV-0 alert source per
//!   RB-AUDIT-CHAIN-001 — wired by the daily-verify cron workflow).
//!
//! ## Production wiring
//!
//! The GHA cron (`.github/workflows/audit-chain-daily-verify.yml`) lists
//! the most recent 7 days of R2 chunk keys via `wrangler r2 object list`
//! (or the CF API) + downloads each chunk + invokes this binary on the
//! local filesystem paths. The binary is wasm-clean by construction (no
//! `tokio`, no `worker::*`, pure-logic file IO via `std::fs`).
//!
//! ## Why pure-logic
//!
//! The autonomous execution charter forbids `tokio` in `src/` + requires
//! every load-bearing path to be wasm32-buildable. This binary lives in
//! `src/bin/verifier.rs` so it builds on the host toolchain but its only
//! dependencies are `std::fs` + the crate's pure-logic verifier.
//!
//! ## Fail-CLOSED canonical
//!
//! Any parse / verify error returns a non-zero exit + writes a structured
//! diagnostic to stderr. The cron workflow grep's stderr for the SEV-0
//! marker (`AUDIT_CHAIN_BREAK_DETECTED`) and pages on hit.

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::process::ExitCode;

use corelink_audit_chain::{
    AuditEvent, ChainVerifier, InMemoryAuditChainAuditSink,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        // Zero-input is the cron-safe no-op (first day after deploy, no
        // chunks yet to verify). The OK marker keeps the cron grep happy.
        eprintln!("usage: verifier <ndjson_chunk_path>...");
        println!("AUDIT_CHAIN_VERIFY_OK: no input paths (cron no-op)");
        return ExitCode::SUCCESS;
    }

    match run(&args) {
        Ok(summary) => {
            println!("AUDIT_CHAIN_VERIFY_OK: {}", summary);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("AUDIT_CHAIN_BREAK_DETECTED: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(paths: &[String]) -> Result<String, String> {
    let mut by_tenant: BTreeMap<uuid::Uuid, Vec<AuditEvent>> = BTreeMap::new();
    let mut total_events: u64 = 0;
    for p in paths {
        let body = std::fs::read_to_string(p)
            .map_err(|e| format!("read {}: {}", p, e))?;
        for (i, line) in body.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let ev = AuditEvent::from_ndjson_line(trimmed).map_err(|e| {
                format!("parse {} line {}: {}", p, i.saturating_add(1), e)
            })?;
            by_tenant.entry(ev.tenant_id).or_default().push(ev);
            total_events = total_events.saturating_add(1);
        }
    }

    if total_events == 0 {
        return Ok("no events in input (clean)".to_string());
    }

    let audit = std::sync::Arc::new(InMemoryAuditChainAuditSink::new());
    let verifier = ChainVerifier::new(audit);
    let now_ms: u64 = 0; // pure-logic CLI; cron passes real epoch via env if needed.

    let mut tenants_verified: u64 = 0;
    let mut events_verified: u64 = 0;
    for (tenant_id, mut events) in by_tenant {
        events.sort_by_key(|e| e.sequence_number);
        let outcome = verifier
            .verify_chain_from_genesis(&events, tenant_id, "daily-verify-cron", now_ms)
            .map_err(|e| format!("tenant={}: {}", tenant_id, e))?;
        events_verified = events_verified.saturating_add(outcome.events_verified_count);
        tenants_verified = tenants_verified.saturating_add(1);
    }

    Ok(format!(
        "tenants_verified={} events_verified={}",
        tenants_verified, events_verified
    ))
}
