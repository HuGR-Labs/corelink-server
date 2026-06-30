//! `billing-reconcile-run` — the scheduled prod billing-reconciliation
//! runner (closes enterprise-DD #5: live usage→Stripe drift detection).
//!
//! Usage:
//!
//! ```sh
//! billing-reconcile-run --input <reconcile-input.json> \
//!     [--report <report.json>] [--fail-on sev3|sev2|sev1]
//! ```
//!
//! Reads a [`corelink_billing_reconcile::ReconcileRunInput`] JSON
//! document (the three layer totals per `(tenant, billing_period)`
//! extracted from the REAL prod sources — Layer 1 Σ R2 usage events +
//! Layer 2 Σ D1 `usage_counter` over the Cloudflare D1 HTTP API, and
//! Layer 3 Σ Stripe usage_records over the Stripe REST API — assembled
//! by `.github/workflows/billing-reconcile-daily.yml`) on `--input`
//! (or stdin when omitted / `-`), runs ONE canonical reconcile pass via
//! [`corelink_billing_reconcile::run_reconcile_pass`], writes the drift
//! [`corelink_billing_reconcile::ReconcileReport`] to `--report` (or
//! stdout), and exits:
//!
//! - `0` — every tenant within the Quiet tier (no escalation). Prints
//!   `BILLING_RECONCILE_CLEAN`.
//! - `1` — drift escalated to / past the `--fail-on` floor (default
//!   `sev3`: ANY drift past Quiet fails — owner zero-tolerance
//!   mandate). Prints `BILLING_RECONCILE_DRIFT_DETECTED::<tenant>::<sev>`
//!   per escalated tenant. The cron pages on the marker.
//! - `2` — the pass itself errored (malformed input / source / sink
//!   failure). Prints `BILLING_RECONCILE_ERROR`. Fail-CLOSED: an error
//!   is NEVER reported as "clean".
//!
//! ## READ-ONLY + report-only
//!
//! The pass NEVER mutates Stripe or D1 (the SEV-1 auto-pause arm runs
//! against an in-memory DRY-RUN control — see
//! [`corelink_billing_reconcile::run`]). This binary is a comparison +
//! report only.
//!
//! ## Why pure-logic
//!
//! Like the `corelink-audit-chain` `verifier` bin, this lives in
//! `src/bin/` so it builds on the host toolchain with no `tokio` / HTTP
//! client — the credentialed extraction is the workflow's job. Pure
//! `std::fs` + the crate's pure-logic reconciler.

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::Read;
use std::process::ExitCode;

use corelink_billing_reconcile::{
    parse_input, run_reconcile_pass, ReconcileConfig, ReconcileReport, ReconcileSeverity,
    MARKER_CLEAN, MARKER_DRIFT, MARKER_ERROR,
};

/// Parsed CLI args.
struct Args {
    input: Option<String>,
    report: Option<String>,
    fail_on: ReconcileSeverity,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut input = None;
    let mut report = None;
    let mut fail_on = ReconcileSeverity::Sev3;
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--input" => {
                input = Some(it.next().ok_or("--input requires a value")?.clone());
            }
            "--report" => {
                report = Some(it.next().ok_or("--report requires a value")?.clone());
            }
            "--fail-on" => {
                let v = it.next().ok_or("--fail-on requires a value")?;
                fail_on = ReconcileSeverity::parse_floor(v)
                    .ok_or_else(|| format!("--fail-on must be sev3|sev2|sev1; got '{v}'"))?;
            }
            "-h" | "--help" => {
                return Err("help".to_string());
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
    }
    Ok(Args {
        input,
        report,
        fail_on,
    })
}

fn read_input_bytes(path: &Option<String>) -> Result<Vec<u8>, String> {
    match path {
        Some(p) if p != "-" => std::fs::read(p).map_err(|e| format!("read {p}: {e}")),
        _ => {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("read stdin: {e}"))?;
            Ok(buf)
        }
    }
}

fn write_report(report: &ReconcileReport, path: &Option<String>) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| format!("serialize report: {e}"))?;
    match path {
        Some(p) => std::fs::write(p, json.as_bytes()).map_err(|e| format!("write {p}: {e}")),
        None => {
            println!("{json}");
            Ok(())
        }
    }
}

fn run(argv: &[String]) -> Result<ExitCode, String> {
    let args = parse_args(argv)?;
    let bytes = read_input_bytes(&args.input)?;
    let input = parse_input(&bytes).map_err(|e| e.to_string())?;
    let report = run_reconcile_pass(&input, ReconcileConfig::default()).map_err(|e| e.to_string())?;

    // Always emit the report artifact first (forensic evidence trail).
    write_report(&report, &args.report)?;

    // Per-escalated-tenant drift markers (the cron greps + pages).
    for o in &report.outcomes {
        if o.severity >= ReconcileSeverity::Sev3 {
            println!(
                "{MARKER_DRIFT}::{}::{}  max_drift_pct={} decision={}",
                o.tenant_id,
                o.severity.as_str(),
                o.max_drift_pct,
                o.decision.as_str()
            );
        }
    }

    if report.fails_at(args.fail_on) {
        eprintln!(
            "{MARKER_DRIFT}: billing_period={} max_severity={} escalations={}/{} (fail-on={})",
            report.billing_period,
            report.max_severity.as_str(),
            report.escalation_count,
            report.tenant_count,
            args.fail_on.as_str()
        );
        Ok(ExitCode::FAILURE)
    } else {
        println!(
            "{MARKER_CLEAN}: billing_period={} tenants={} escalations={} max_severity={}",
            report.billing_period,
            report.tenant_count,
            report.escalation_count,
            report.max_severity.as_str()
        );
        Ok(ExitCode::SUCCESS)
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(code) => code,
        Err(msg) if msg == "help" => {
            println!(
                "usage: billing-reconcile-run --input <input.json> [--report <out.json>] [--fail-on sev3|sev2|sev1]"
            );
            ExitCode::SUCCESS
        }
        Err(msg) => {
            // Fail-CLOSED: a run error is exit 2 (distinct from a clean
            // drift detection at exit 1) and is NEVER a silent clean.
            eprintln!("{MARKER_ERROR}: {msg}");
            ExitCode::from(2)
        }
    }
}
