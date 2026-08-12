//! `runner-aggregate-run` — the pure shadow runner-aggregation binary
//! (WI-S10-007 Phase B).
//!
//! Reads a [`RunnerAggregateInput`] JSON on stdin, writes a
//! [`RunnerAggregateOutput`] JSON on stdout, exits non-zero on a malformed input
//! or a hash-chain break. PURE: no tokio, no HTTP, no credentials, no clock read
//! (the `now_ms` is in the input). The credentialed D1 drain (SELECT the
//! not-yet-aggregated `usage_event_staging` rows for `runner_vcpu_seconds`) and
//! the atomic write (UPSERT `runner_usage_counter` + UPDATE
//! `runner_hash_chain_head`, migration 0094) are the `billing-aggregate-runner`
//! cron workflow's job — exactly as `billing-reconcile-run` leaves credentialed
//! extraction to its workflow.
//!
//! Usage: `... | runner-aggregate-run` (stdin→stdout). Shadow-only: it bills
//! nothing; the `shadow_ledger` in the output is "what WOULD be charged".

#![allow(
    clippy::print_stderr,
    reason = "an operator CLI: the shadow summary + errors are deliberately written to stderr (stdout carries only the JSON output)."
)]

use std::io::{Read, Write};
use std::process::ExitCode;

use corelink_runner_aggregate::{aggregate_runner_usage, RunnerAggregateInput};

fn main() -> ExitCode {
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        eprintln!("runner-aggregate-run: failed to read stdin: {e}");
        return ExitCode::FAILURE;
    }

    let input: RunnerAggregateInput = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("runner-aggregate-run: invalid RunnerAggregateInput JSON: {e}");
            return ExitCode::FAILURE;
        }
    };

    let output = match aggregate_runner_usage(&input) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("runner-aggregate-run: aggregation failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let rendered = match serde_json::to_string_pretty(&output) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("runner-aggregate-run: failed to serialize output: {e}");
            return ExitCode::FAILURE;
        }
    };

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    if let Err(e) = writeln!(lock, "{rendered}") {
        eprintln!("runner-aggregate-run: failed to write stdout: {e}");
        return ExitCode::FAILURE;
    }

    // A concise shadow summary to stderr (operator-visible; never touches stdout).
    eprintln!(
        "runner-aggregate-run: {} counter row(s), {} shadow line(s), {} skipped, \
         total shadow = {} millicents (${}.{:02}) — SHADOW, billed nothing",
        output.counters.len(),
        output.shadow_ledger.len(),
        output.skipped.len(),
        output.total_shadow_millicents,
        output.total_shadow_millicents / 100_000,
        (output.total_shadow_millicents / 1_000) % 100,
    );

    ExitCode::SUCCESS
}
