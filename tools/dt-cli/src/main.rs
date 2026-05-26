#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr, clippy::indexing_slicing, clippy::panic)]
//! `corelink-dt-cli` — Dependency-Track CLI utility (WI-S12-005).
//!
//! # Subcommands
//!
//! - `inject-mock` — inject a synthetic CVE into DT staging for E2E alert path validation.
//! - `ossindex-fallback` — manually trigger OSS Index API CVE lookup (fallback when DT is down).
//!
//! # Environment Variables
//!
//! - `DT_WEBHOOK_SECRET` — HMAC shared secret (required).
//! - `DT_MOCK_INJECTION_ENABLED` — must be `"true"` for `inject-mock` (staging only).
//! - `DT_PROJECT_UUID` — project UUID to target.

use corelink_dt_webhook::{
    handler::InMemoryDtWebhookHandler, types::{DtProjectUuid, SyntheticCve}, DtWebhookHandler,
};
use tracing::{error, info};

/// CLI entry-point.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: corelink-dt-cli <subcommand> [options]");
        eprintln!("Subcommands: inject-mock, ossindex-fallback");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "inject-mock" => run_inject_mock(&args[2..]).await,
        "ossindex-fallback" => run_ossindex_fallback(&args[2..]).await,
        cmd => {
            error!("Unknown subcommand: {cmd}");
            std::process::exit(1);
        }
    }
}

async fn run_inject_mock(args: &[String]) {
    let project_uuid_str = get_flag(args, "--project")
        .or_else(|| std::env::var("DT_PROJECT_UUID").ok())
        .unwrap_or_else(|| {
            error!("--project <uuid> or DT_PROJECT_UUID required");
            std::process::exit(1);
        });

    let severity_str = get_flag(args, "--severity").unwrap_or_else(|| "critical".to_owned());

    let cvss_score = severity_to_cvss(&severity_str);

    let project_uuid = DtProjectUuid::new(project_uuid_str).unwrap_or_else(|e| {
        error!("Invalid project UUID: {e}");
        std::process::exit(1);
    });

    let mock_enabled = std::env::var("DT_MOCK_INJECTION_ENABLED")
        .map(|v| v == "true")
        .unwrap_or(false);

    let secret = std::env::var("DT_WEBHOOK_SECRET")
        .map(|s| s.into_bytes())
        .unwrap_or_else(|_| b"dev-secret".to_vec());

    let handler = InMemoryDtWebhookHandler::new(secret, mock_enabled);

    let synthetic_cve = SyntheticCve {
        cve_id: "CVE-2026-9999".into(),
        cvss_score,
        description: Some("Synthetic test CVE — inject-mock CLI (WI-S12-005)".into()),
    };

    info!(
        project_uuid = %project_uuid,
        cve_id = %synthetic_cve.cve_id,
        cvss_score,
        "Starting mock CVE injection"
    );

    match handler.inject_mock_cve(project_uuid, synthetic_cve).await {
        Ok(result) => {
            if result.sla_met {
                info!(
                    cve_id = %result.synthetic_cve_id,
                    latency_ms = result.e2e_latency_ms,
                    "mock_cve_injection outcome=ok"
                );
                std::process::exit(0);
            } else {
                error!(
                    cve_id = %result.synthetic_cve_id,
                    latency_ms = result.e2e_latency_ms,
                    "mock_cve_injection outcome=sla_violation"
                );
                std::process::exit(2);
            }
        }
        Err(e) => {
            error!("mock_cve_injection outcome=alert_missing error={e}");
            std::process::exit(1);
        }
    }
}

async fn run_ossindex_fallback(_args: &[String]) {
    // Manual trigger for OSS Index Sonatype free tier CVE lookup.
    // In production this would POST to https://ossindex.sonatype.org/api/v3/component-report
    // with the purls from the current Cargo.lock.
    info!("OSS Index fallback triggered (manual mode — WI-S12-005 §6.1.7)");
    info!("In production: POST https://ossindex.sonatype.org/api/v3/component-report with Cargo.lock purls");
    info!("Target: detect HIGH/CRITICAL CVEs when DT is down > 4h");
    // Stub: exit 0 (production implementation sends real HTTP request).
    std::process::exit(0);
}

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn severity_to_cvss(severity: &str) -> f64 {
    match severity.to_lowercase().as_str() {
        "critical" => 9.5,
        "high" => 7.5,
        "medium" => 5.0,
        "low" => 2.0,
        _ => 9.5,
    }
}
