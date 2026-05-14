<<<<<<< HEAD
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
=======
//! `corelink` — CoreLink CLI binary (WI-S15-001 + WI-S15-005).
//!
//! # Subcommands
//!
//! - `ls`      — list CAS/AC entries for a tenant.
//! - `get`     — download a blob by digest.
//! - `put`     — upload a file as a blob.
//! - `stat`    — show metadata for a digest.
//! - `bench`   — micro-benchmark vs cluster.
//! - `doctor`  — 8-check actionable diagnostic.
//! - `version` — semver + git rev + SLSA attestation link.
//! - `config`  — manage persistent config (`~/.corelink/config.toml`).
//!
//! # Auth (CTRL-CRED-001)
//!
//! `CORELINK_PAT` env var or `~/.corelink/config.toml`. Never via CLI args.
//!
//! # Telemetry
//!
//! Opt-in, default-off (R-S15-14). Enable: `corelink config set telemetry on`.
//! See `docs/cli/telemetry.md` for privacy policy.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod config;
mod telemetry;

use std::time::Instant;

use config::Config;
use telemetry::{TelemetryEvent, emit_if_enabled};
use tracing::{error, info};

fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .compact()
        .init();

    let start = Instant::now();
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        std::process::exit(0);
    }

    let cfg = Config::load().unwrap_or_else(|e| {
        error!("config load error: {e}; using defaults");
        Config::default()
    });

    let subcommand = args[1].as_str();
    let outcome = run_subcommand(subcommand, &args[2..], &cfg);
    let duration_ms = start.elapsed().as_millis() as u64;

    // Emit telemetry ONLY if explicitly opted in (R-S15-14).
    let event = TelemetryEvent::new(subcommand, outcome, duration_ms, cfg.anonymized_id);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio rt");
    rt.block_on(async {
        emit_if_enabled(cfg.telemetry_enabled(), event);
    });

    std::process::exit(if outcome == "ok" { 0 } else { 1 });
}

/// Dispatch subcommand and return `"ok"` or `"err"`.
fn run_subcommand(subcommand: &str, args: &[String], cfg: &Config) -> &'static str {
    match subcommand {
        "ls" => cmd_ls(args, cfg),
        "get" => cmd_get(args, cfg),
        "put" => cmd_put(args, cfg),
        "stat" => cmd_stat(args, cfg),
        "bench" => cmd_bench(args, cfg),
        "doctor" => cmd_doctor(args, cfg),
        "version" => cmd_version(),
        "config" => cmd_config(args),
        _ => {
            error!("unknown subcommand: {subcommand:?}");
            eprintln!("Unknown subcommand: {subcommand:?}. Run `corelink --help`.");
            "err"
        }
    }
}

fn print_help() {
    println!(
        "corelink {version}\n\
         \n\
         USAGE:\n\
         \tcorelink <subcommand> [options]\n\
         \n\
         SUBCOMMANDS:\n\
         \tls\t\tList CAS/AC entries for a tenant\n\
         \tget\t\tDownload a blob by digest\n\
         \tput\t\tUpload a file as a blob\n\
         \tstat\t\tShow metadata for a digest\n\
         \tbench\t\tMicro-benchmark vs cluster\n\
         \tdoctor\t\t8-check actionable diagnostic\n\
         \tversion\t\tVersion + SLSA attestation link\n\
         \tconfig\t\tManage persistent config\n\
         \n\
         AUTH:\n\
         \tSet CORELINK_PAT env var or run: corelink config set pat <token>\n\
         \n\
         TELEMETRY (opt-in default-off):\n\
         \tcorelink config set telemetry on   # enable\n\
         \tcorelink config set telemetry off  # disable\n\
         \tcorelink config list               # inspect\n\
         \tSee docs/cli/telemetry.md for privacy policy.",
        version = env!("CARGO_PKG_VERSION"),
    );
}

// ── Subcommand stubs ──────────────────────────────────────────────────────────
// Full implementations are in WI-S15-001; these stubs satisfy the build
// while telemetry opt-in (WI-S15-005) is the primary focus here.

fn cmd_ls(_args: &[String], _cfg: &Config) -> &'static str {
    info!("ls: listing CAS/AC entries (WI-S15-001)");
    println!("(stub) ls — WI-S15-001 implements full listing");
    "ok"
}

fn cmd_get(_args: &[String], _cfg: &Config) -> &'static str {
    info!("get: downloading blob (WI-S15-001)");
    println!("(stub) get — WI-S15-001 implements full download");
    "ok"
}

fn cmd_put(_args: &[String], _cfg: &Config) -> &'static str {
    info!("put: uploading blob (WI-S15-001)");
    println!("(stub) put — WI-S15-001 implements full upload");
    "ok"
}

fn cmd_stat(_args: &[String], _cfg: &Config) -> &'static str {
    info!("stat: metadata lookup (WI-S15-001)");
    println!("(stub) stat — WI-S15-001 implements full stat");
    "ok"
}

fn cmd_bench(_args: &[String], _cfg: &Config) -> &'static str {
    info!("bench: micro-benchmark (WI-S15-001)");
    println!("(stub) bench — WI-S15-001 implements full benchmark");
    "ok"
}

fn cmd_doctor(_args: &[String], _cfg: &Config) -> &'static str {
    info!("doctor: 8-check diagnostic (WI-S15-001)");
    println!("(stub) doctor — WI-S15-001 implements 8 actionable checks");
    "ok"
}

fn cmd_version() -> &'static str {
    println!(
        "corelink {version}\n\
         git-rev: {git_rev}\n\
         SLSA: https://corelink.dev/slsa/{version}/attestation.json",
        version = env!("CARGO_PKG_VERSION"),
        git_rev = option_env!("GIT_REV").unwrap_or("dev"),
    );
    "ok"
}

fn cmd_config(args: &[String]) -> &'static str {
    if args.is_empty() {
        eprintln!("config: missing subcommand. Use: set <key> <value> | get <key> | list | rotate <field>");
        return "err";
    }

    let mut cfg = Config::load().unwrap_or_else(|e| {
        error!("config load: {e}");
        Config::default()
    });

    match args[0].as_str() {
        "set" => {
            if args.len() < 3 {
                eprintln!("config set: usage: corelink config set <key> <value>");
                return "err";
            }
            match cfg.set(&args[1], &args[2]) {
                Ok(()) => {
                    if let Err(e) = cfg.save() {
                        error!("config save: {e}");
                        return "err";
                    }
                    println!("{} = {}", args[1], args[2]);
                    "ok"
                }
                Err(e) => {
                    eprintln!("config set error: {e}");
                    "err"
                }
            }
        }
        "get" => {
            if args.len() < 2 {
                eprintln!("config get: usage: corelink config get <key>");
                return "err";
            }
            match cfg.get(&args[1]) {
                Ok(v) => {
                    println!("{} = {v}", args[1]);
                    "ok"
                }
                Err(e) => {
                    eprintln!("config get error: {e}");
                    "err"
                }
            }
        }
        "list" => {
            println!("{}", cfg.list());
            "ok"
        }
        "rotate" => {
            if args.len() < 2 {
                eprintln!("config rotate: usage: corelink config rotate telemetry-id");
                return "err";
            }
            if args[1] == "telemetry-id" {
                cfg.rotate_telemetry_id();
                if let Err(e) = cfg.save() {
                    error!("config save: {e}");
                    return "err";
                }
                println!("anonymized_id rotated to {}", cfg.anonymized_id);
                "ok"
            } else {
                eprintln!("config rotate: unknown field {:?}; valid: telemetry-id", args[1]);
                "err"
            }
        }
        other => {
            eprintln!("config: unknown subcommand {other:?}");
            "err"
        }
>>>>>>> wt/wi-s15-005
    }
}
