//! `corelink` CLI binary — WI-S15-001.
//!
//! 7 canonical subcommands: `ls`, `get`, `put`, `stat`, `bench`, `doctor`, `version`.
//! Plus `config` for managing `~/.corelink/config.toml`.
//!
//! Auth: env var `CORELINK_PAT` first → config file fallback. `--pat` flag is
//! explicitly **rejected** (CTRL-CRED-001 security control).
//!
//! JSON output: `--output=json` flag is global and uniform across all subcommands.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::expect_used)]

mod auth;
mod client;
mod commands;
mod config;
mod doctor;
mod error;
mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::auth::resolve_pat;
use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::OutputFormat;

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

    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let format = cli.output.unwrap_or(OutputFormat::Text);

    // Commands that do NOT need a PAT.
    match &cli.command {
        Commands::Version => {
            return commands::version::run(format);
        }
        Commands::Config { action } => {
            return run_config(action, format);
        }
        _ => {}
    }

    // All other commands require a resolved + validated PAT.
    let pat = resolve_pat()?;
    let client = CorelinkClient::new(pat)?;

    match cli.command {
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
        // Version + Config already handled above; this branch is unreachable.
        Commands::Version | Commands::Config { .. } => unreachable!(),
    }
}

fn run_config(action: &ConfigAction, format: OutputFormat) -> Result<(), CliError> {
    match action {
        ConfigAction::Set { key, value } => commands::config_cmd::run_set(key, value, format),
        ConfigAction::Get { key } => commands::config_cmd::run_get(key, format),
        ConfigAction::List => commands::config_cmd::run_list(format),
    }
}
