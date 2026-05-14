//! `corelink` CLI binary — WI-S15-001 + telemetry opt-in (WI-S15-005).
//!
//! 7 canonical subcommands: `ls`, `get`, `put`, `stat`, `bench`, `doctor`, `version`.
//! Plus `config` for managing `~/.corelink/config.toml`.
//!
//! Auth: env var `CORELINK_PAT` first → config file fallback. `--pat` flag is
//! explicitly **rejected** (CTRL-CRED-001 security control).
//!
//! JSON output: `--output=json` flag is global and uniform across all subcommands.
//!
//! Telemetry: opt-in default-off (R-S15-14). Enable via
//! `corelink config set telemetry.enabled true`. Emission is detached / non-blocking
//! (FM-150); CLI exit code is never affected by telemetry endpoint health.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::expect_used)]

mod auth;
mod client;
mod commands;
mod config;
mod doctor;
mod error;
mod output;
mod telemetry;

use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::auth::resolve_pat;
use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::OutputFormat;
use crate::telemetry::{emit_if_enabled, TelemetryEvent};

/// CoreLink content-addressable cache CLI.
#[derive(Debug, Parser)]
#[command(name = "corelink")]
#[command(version, about = "CoreLink content-addressable cache CLI", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Output format: `text` (default) or `json`.
    ///
    /// JSON output is parsing-friendly for CI scripts. Schema: docs/cli/json-output-schema.md.
    #[arg(long, global = true, value_name = "FORMAT")]
    output: Option<OutputFormat>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum Commands {
    /// List CAS/AC entries for a tenant.
    Ls {
        /// Tenant ID (required).
        #[arg(long, value_name = "TENANT_ID")]
        tenant: String,
        /// Optional prefix filter.
        #[arg(long, value_name = "PREFIX")]
        prefix: Option<String>,
        /// Maximum entries to return (default 100).
        #[arg(long, value_name = "N", default_value = "100")]
        limit: u32,
        /// Pagination cursor from a previous response.
        #[arg(long, value_name = "CURSOR")]
        cursor: Option<String>,
    },

    /// Download a blob by digest. Performs client-side BLAKE3 verify (CTRL-CAS-002).
    Get {
        /// BLAKE3 digest of the blob to download.
        digest: String,
        /// Output file path. Defaults to stdout if not specified.
        #[arg(short = 'o', long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Upload a blob. Computes BLAKE3 digest unless `--digest` is provided.
    Put {
        /// Path to the file to upload.
        file: PathBuf,
        /// Pre-computed BLAKE3 digest (optional; computed if omitted).
        #[arg(long, value_name = "DIGEST")]
        digest: Option<String>,
    },

    /// Show metadata + size + age for a digest.
    Stat {
        /// BLAKE3 digest to inspect.
        digest: String,
    },

    /// Run micro-benchmark vs cluster. Default: full write+read round-trip (100 ops).
    Bench {
        /// Benchmark write operations only.
        #[arg(long, conflicts_with_all = ["read", "full"])]
        write: bool,
        /// Benchmark read operations only.
        #[arg(long, conflicts_with_all = ["write", "full"])]
        read: bool,
        /// Full write+read round-trip (default when no flag given).
        #[arg(long, conflicts_with_all = ["write", "read"])]
        full: bool,
    },

    /// Run 8 diagnostic checks: network, auth, storage, BYOK, region, quota, client verify.
    Doctor {
        /// Output JSON (shorthand for `--output=json`).
        #[arg(long)]
        json: bool,
    },

    /// Print version, git rev, and SLSA attestation link.
    Version,

    /// Manage local config (~/.corelink/config.toml).
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Record a runbook dry-run drill (WI-S17-003 / PAT-RUNBOOK-DRILL-001).
    ///
    /// Builds + validates a `DrillRecord` (drift detection > 2x = FM-202
    /// flag) and emits JSON on stdout. Offline: no PAT required. Pipe to
    /// the admin runbook-drill API or `wrangler d1 execute` to persist in
    /// the D1 `runbook_drills` table (migration 0033).
    RunbookDrill {
        #[command(subcommand)]
        action: RunbookDrillAction,
    },
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum RunbookDrillAction {
    /// Build + emit a drill record (no I/O; pipe to host adapter).
    Record {
        /// Runbook id (e.g. `RB-FM-051`).
        #[arg(long, value_name = "RB_ID")]
        runbook_id: String,
        /// Operator id (e.g. `op_gschneiter`); zero PII per CTRL-PRIV-001.
        #[arg(long, value_name = "OP_ID")]
        executor: String,
        /// EVT-017 evidence URL (asciinema cast in R2 evidence-runbooks/).
        #[arg(long, value_name = "URL")]
        evidence: String,
        /// Actual duration in seconds.
        #[arg(long, value_name = "SECS")]
        duration_seconds: i64,
        /// Expected duration in seconds (from runbook spec).
        #[arg(long, value_name = "SECS")]
        expected_seconds: i64,
        /// Drill succeeded (default true; pass --no-success to record fail).
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        success: bool,
        /// Optional override unix timestamp (defaults to now).
        #[arg(long, value_name = "UNIX_SECS")]
        executed_at: Option<i64>,
        /// Optional sanitized notes (no PII / secrets per CTRL-PRIV-001).
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum ConfigAction {
    /// Set a config key (e.g., `corelink config set telemetry.enabled true`).
    Set {
        /// Config key in dotted form (e.g., `auth.pat`, `defaults.tenant_id`, `telemetry.enabled`).
        key: String,
        /// Value to set.
        value: String,
    },
    /// Get the current value of a config key.
    Get {
        /// Config key in dotted form.
        key: String,
    },
    /// List all config (PAT is redacted).
    List,
    /// Rotate the anonymised telemetry ID (generates a fresh UUID v4).
    Rotate {
        /// Field to rotate. Currently only `telemetry-id` is supported.
        field: String,
    },
}

#[tokio::main]
async fn main() {
    // Initialise structured tracing (RUST_LOG controls verbosity).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    // CTRL-CRED-001: reject `--pat` flag before clap parses anything.
    // Inspect raw args so the user gets a clear security error, not a clap parse error.
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args
        .iter()
        .any(|a| a == "--pat" || a.starts_with("--pat="))
    {
        eprintln!("error: PAT must NOT be passed as a CLI argument (security control CTRL-CRED-001).");
        eprintln!("       Use env var CORELINK_PAT or config file ~/.corelink/config.toml instead.");
        std::process::exit(2);
    }

    let start = Instant::now();
    let (subcommand_label, result) = run().await;
    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let outcome: &'static str = if result.is_ok() { "ok" } else { "err" };

    // Telemetry (WI-S15-005): opt-in, non-blocking, detached.
    // Load config best-effort; if it fails we simply skip emission (privacy-safe).
    if let Ok(cfg) = config::load() {
        let event = TelemetryEvent::new(
            subcommand_label.to_owned(),
            outcome,
            duration_ms,
            cfg.telemetry.anonymized_id,
        );
        emit_if_enabled(cfg.telemetry.enabled, event);
    }

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> (&'static str, Result<(), CliError>) {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // Print clap's formatted help/error and exit with its preferred code.
            e.exit();
        }
    };
    let format = cli.output.unwrap_or(OutputFormat::Text);
    let label = subcommand_label(&cli.command);

    // Commands that do NOT need a PAT.
    match &cli.command {
        Commands::Version => {
            return (label, commands::version::run(format));
        }
        Commands::Config { action } => {
            return (label, run_config(action, format));
        }
        Commands::RunbookDrill { action } => {
            return (label, run_runbook_drill(action, format));
        }
        _ => {}
    }

    // All other commands require a resolved + validated PAT.
    let pat = match resolve_pat() {
        Ok(p) => p,
        Err(e) => return (label, Err(e)),
    };
    let client = match CorelinkClient::new(pat) {
        Ok(c) => c,
        Err(e) => return (label, Err(e)),
    };

    let res = match cli.command {
        Commands::Ls { tenant, prefix, limit, cursor } => {
            commands::ls::run(
                &client,
                &tenant,
                prefix.as_deref(),
                Some(limit),
                cursor.as_deref(),
                format,
            )
            .await
        }
        Commands::Get { digest, output } => {
            commands::get::run(&client, &digest, output, format).await
        }
        Commands::Put { file, digest } => {
            commands::put::run(&client, &file, digest.as_deref(), format).await
        }
        Commands::Stat { digest } => commands::stat::run(&client, &digest, format).await,
        Commands::Bench { write, read, full } => {
            commands::bench::run(&client, write, read, full, format).await
        }
        Commands::Doctor { json } => {
            commands::doctor_cmd::run(&client, json, format).await
        }
        // Version + Config + RunbookDrill already handled above; unreachable.
        Commands::Version | Commands::Config { .. } | Commands::RunbookDrill { .. } => {
            unreachable!()
        }
    };
    (label, res)
}

fn subcommand_label(cmd: &Commands) -> &'static str {
    match cmd {
        Commands::Ls { .. } => "ls",
        Commands::Get { .. } => "get",
        Commands::Put { .. } => "put",
        Commands::Stat { .. } => "stat",
        Commands::Bench { .. } => "bench",
        Commands::Doctor { .. } => "doctor",
        Commands::Version => "version",
        Commands::Config { .. } => "config",
        Commands::RunbookDrill { .. } => "runbook-drill",
    }
}

fn run_runbook_drill(action: &RunbookDrillAction, format: OutputFormat) -> Result<(), CliError> {
    match action {
        RunbookDrillAction::Record {
            runbook_id,
            executor,
            evidence,
            duration_seconds,
            expected_seconds,
            success,
            executed_at,
            notes,
        } => commands::runbook_drill::run(
            runbook_id,
            executor,
            *duration_seconds,
            *expected_seconds,
            evidence,
            *success,
            *executed_at,
            notes.as_deref(),
            format,
        ),
    }
}

fn run_config(action: &ConfigAction, format: OutputFormat) -> Result<(), CliError> {
    match action {
        ConfigAction::Set { key, value } => commands::config_cmd::run_set(key, value, format),
        ConfigAction::Get { key } => commands::config_cmd::run_get(key, format),
        ConfigAction::List => commands::config_cmd::run_list(format),
        ConfigAction::Rotate { field } => {
            if field == "telemetry-id" {
                config::rotate_telemetry_id()?;
                println!("Rotated telemetry.anonymized_id");
                Ok(())
            } else {
                Err(CliError::Other(format!(
                    "config rotate: unknown field {field:?}; valid: telemetry-id"
                )))
            }
        }
    }
}
