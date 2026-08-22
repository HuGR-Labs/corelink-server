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

// Wave-19 crate-root aliases — keep the `crate::verify_ndjson::...`
// and `crate::audit_export::...` import paths uniform between the
// binary's `commands::*` modules and the lib's mirror `#[path]`
// includes. The `pub use` makes each alias a crate-root path
// (`crate::verify_ndjson`, `crate::audit_export`) rather than just a
// local binding. Used by `commands::verify_ndjson_http` (HTTP path
// pulls in the offline verifier helpers) and by the `verify_ndjson`
// test module (fixture builder lives in `audit_export`).
pub use commands::audit as audit_export;
pub use commands::verify_ndjson;

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
        /// Client-side filter: keep only blobs whose hash starts with this prefix (applies to the current page only).
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
        #[arg(short = 'o', long = "out", value_name = "FILE", id = "out_file")]
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

    /// Audit log export + verify (SOC 2 / GDPR / LGPD evidence).
    ///
    /// Emits a tamper-proof export (JSON-LD CloudEvents + Merkle proofs)
    /// for a given time window, content-addressed by BLAKE3. Offline
    /// re-verify with `corelink audit verify <export>`.
    Audit {
        #[command(subcommand)]
        action: AuditAction,
    },

    // -----------------------------------------------------------------------
    // Stream-1 "ridiculously easy to use" additions
    // -----------------------------------------------------------------------
    /// Show identity: tenant_id, token_prefix, route_kind.
    ///
    /// Calls `GET /v1/users/me` and caches tenant_id in
    /// `~/.corelink/config.toml` for use by `put`/`get`/`ac`.
    Whoami,

    /// Save a PAT and cache your tenant_id.
    ///
    /// Writes the PAT to `~/.corelink/config.toml` (0600 perms), then
    /// calls `GET /v1/users/me` to resolve and cache the tenant_id.
    Login {
        /// Personal Access Token (`corelink_<env>_<id>.<secret>.<sig>`).
        #[arg(long = "token", value_name = "PAT", hide_env_values = true)]
        token: String,
    },

    /// Action cache operations: `put <digest> <result>` / `get <digest>`.
    Ac {
        #[command(subcommand)]
        action: AcAction,
    },

    /// Wire a Bazel repo to the CoreLink remote cache.
    ///
    /// Appends a managed remote-cache block to `.bazelrc` + writes
    /// `.corelink/credentials` (mode 0600). Idempotent; `--force`
    /// rewrites. Endpoint + tenant are derived from your config.
    BazelInit {
        /// Rewrite an existing managed `.bazelrc` block + credentials.
        #[arg(long)]
        force: bool,
    },

    /// Tenant data-portability / offboarding (GDPR exit).
    Tenant {
        #[command(subcommand)]
        action: TenantAction,
    },

    /// Content-addressable store access (escape-hatch / migration).
    Cas {
        #[command(subcommand)]
        action: CasAction,
    },

    /// Bulk pre-warm the tenant CAS from a local directory.
    ///
    /// Content-addressed, so re-imports dedup for free. `s3://` sources
    /// are a flagged gap (sync locally first).
    Import {
        /// Source: a local directory (or an `s3://…` URI — flagged).
        source: String,
    },

    /// CI cache migration helpers.
    Ci {
        #[command(subcommand)]
        action: CiAction,
    },
}

/// Subcommands for `corelink tenant`.
#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum TenantAction {
    /// Export a content-addressed portability bundle (audit slice + CAS
    /// index) for `--tenant-id` to `--output`.
    Export {
        /// Tenant id (defaults to your cached tenant).
        #[arg(long = "tenant-id", value_name = "TENANT_ID")]
        tenant_id: Option<String>,
        /// Destination file for the bundle.
        #[arg(long = "out", value_name = "FILE", id = "out_file")]
        output: PathBuf,
    },
    /// Re-verify every content hash in a previously exported bundle.
    VerifyExport {
        /// Path to the export bundle produced by `tenant export`.
        #[arg(long, value_name = "FILE")]
        archive: PathBuf,
    },
}

/// Subcommands for `corelink cas`.
#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum CasAction {
    /// Download one blob by digest (`sha256:`/`blake3:` prefix ok).
    Get {
        /// Digest of the blob to download.
        digest: String,
        /// Output file path. Defaults to stdout if not specified.
        #[arg(short = 'o', long = "out", value_name = "FILE", id = "out_file")]
        output: Option<PathBuf>,
    },
    /// Bulk-download every blob in the tenant CAS to a local directory.
    Export {
        /// Tenant to export (`me` = your own; cross-tenant is impossible).
        #[arg(long, value_name = "TENANT_ID", default_value = "me")]
        tenant: String,
        /// Destination directory (an `s3://…` URI is a flagged gap).
        #[arg(long = "out", value_name = "DEST", id = "out_file")]
        output: String,
    },
}

/// Subcommands for `corelink ci`.
#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum CiAction {
    /// One-shot mirror of a local cache directory into CoreLink CAS.
    Mirror {
        /// Source cache (a local directory; service names are flagged).
        #[arg(long, value_name = "SRC")]
        from: String,
        /// Sink — only `corelink` is wired.
        #[arg(long, value_name = "SINK")]
        to: String,
    },
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum AuditAction {
    /// Export the audit log for `--tenant` over `[since, until)`.
    Export {
        /// Tenant id (pseudonymous UUIDv7).
        #[arg(long, value_name = "TENANT_ID")]
        tenant: String,
        /// Window lower bound (Unix epoch ms; inclusive).
        #[arg(long, value_name = "MS")]
        since: u64,
        /// Window upper bound (Unix epoch ms; exclusive).
        #[arg(long, value_name = "MS")]
        until: u64,
        /// Serialization format. Default: `json-ld`.
        #[arg(long, value_name = "FORMAT", default_value = "json-ld")]
        format: commands::audit::ExportFormat,
        /// Output directory (file name is content-addressed). Defaults to cwd.
        #[arg(long = "out", value_name = "DIR", id = "out_file")]
        output: Option<PathBuf>,
        /// Embed Merkle inclusion proofs in each event row.
        #[arg(long = "include-merkle-proofs")]
        include_merkle_proofs: bool,
        /// Re-validate every proof BEFORE writing (fail-CLOSED).
        #[arg(long)]
        verify: bool,
        /// Use a deterministic fixture transport (offline mode; no PAT).
        /// Generates `--fixture-events` synthetic events for the tenant.
        #[arg(long)]
        fixture: bool,
        /// Number of fixture events (only with `--fixture`).
        #[arg(long, value_name = "N", default_value_t = 10)]
        fixture_events: u32,
        /// Start time for fixture events (Unix epoch ms; only with `--fixture`).
        #[arg(long, value_name = "MS", default_value_t = 0)]
        fixture_start_ms: u64,
    },

    /// Re-validate a previously exported JSON-LD audit log file.
    Verify {
        /// Path to the JSON-LD export file.
        path: PathBuf,
    },

    /// Print recent audit events for a tenant (windowed pull from the
    /// live export route; not a live follow).
    ///
    /// Default window is the trailing hour. Filter with
    /// `--filter event_type=corelink.observability.export_ready`.
    Tail {
        /// Tenant id (defaults to your cached tenant).
        #[arg(long, value_name = "TENANT_ID")]
        tenant: Option<String>,
        /// `key=value` predicate (keys: event_type, subject, source,
        /// region, tenant_id, sequence_number, id).
        #[arg(long, value_name = "key=value")]
        filter: Option<String>,
        /// Window lower bound (Unix epoch ms; default now-1h).
        #[arg(long, value_name = "MS")]
        since: Option<u64>,
        /// Max events to print (default 50).
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },

    /// Re-verify a streaming NDJSON envelope produced by
    /// `GET /v1/audit/export` (WI-S09-008 customer-CLI AC).
    ///
    /// Two transports:
    ///
    /// **Offline (wave-17)** — `--ndjson <FILE>`: reads the envelope
    /// from disk. The file carries one `{event, proof}` line per
    /// audit row plus a trailing `{"manifest": ...}` line. The
    /// `--chain-head-anchor` MUST be the value the server published
    /// in the `X-CoreLink-Audit-Export-Chain-Head-Anchor` response
    /// header at export time. Exit 0 on full chain integrity
    /// verified; exit 1 with a structured chain-break error on any
    /// divergence.
    ///
    /// **HTTP-aware (wave-19)** — `--url <URL> --bearer <PAT>`:
    /// streams the export response, reads the
    /// `x-corelink-audit-export-aborted` HTTP trailer (wave-18 abort
    /// signal). On trailer detection: prints
    /// `AUDIT_EXPORT_ABORTED: break_at_seq=N break_at_chunk=M
    /// observed=<hex> expected=<hex>` to stderr and exits
    /// **sysexits DATAERR (65)**. Otherwise pipes the bytes through
    /// the wave-17 verifier against the chain-head anchor (either the
    /// response header or `--chain-head-anchor` flag if supplied —
    /// both must match if both are present). Exit 0 on success;
    /// non-zero (other than 65) on network / HTTP errors.
    VerifyNdjson {
        /// Path to the NDJSON envelope file (offline mode; mutually
        /// exclusive with `--url`).
        #[arg(long = "ndjson", value_name = "FILE", conflicts_with = "url")]
        ndjson: Option<PathBuf>,
        /// Export URL (`https://corelink-api.humangr.com/v1/audit/export?...`).
        /// Mutually exclusive with `--ndjson`. Requires `--bearer`.
        #[arg(
            long = "url",
            value_name = "URL",
            conflicts_with = "ndjson",
            requires = "bearer"
        )]
        url: Option<String>,
        /// Bearer token (PAT) for the export request. Read from
        /// `CORELINK_PAT` env var if not supplied. Never logged.
        #[arg(
            long = "bearer",
            value_name = "TOKEN",
            env = "CORELINK_PAT",
            hide_env_values = true
        )]
        bearer: Option<String>,
        /// 64-char BLAKE3 chain-head anchor (from response header
        /// `X-CoreLink-Audit-Export-Chain-Head-Anchor`). REQUIRED for
        /// `--ndjson`; OPTIONAL for `--url` (recovered from the
        /// response header automatically; supply only for defence-in-
        /// depth cross-check).
        #[arg(long = "chain-head-anchor", value_name = "HEX")]
        chain_head_anchor: Option<String>,
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

/// Subcommands for `corelink ac`.
#[derive(Debug, Subcommand)]
#[non_exhaustive]
enum AcAction {
    /// Store an action result: `ac put <digest> <result-file>`.
    Put {
        /// Action digest key (e.g. a sha256 of the action inputs).
        digest: String,
        /// Path to the result file to store under `digest`.
        result: std::path::PathBuf,
    },
    /// Retrieve an action result: `ac get <digest> [-o <file>]`.
    Get {
        /// Action digest key.
        digest: String,
        /// Output file path. Defaults to stdout if not specified.
        #[arg(short = 'o', long = "out", value_name = "FILE", id = "out_file")]
        output: Option<std::path::PathBuf>,
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
    /// Apply a whole TOML config document (`--file corelink.toml`).
    ///
    /// Every leaf key is validated as if set individually — the path
    /// used by the observability how-tos to push
    /// `[observability.export.*]` blocks.
    Apply {
        /// Path to a TOML config document.
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
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
        eprintln!(
            "error: PAT must NOT be passed as a CLI argument (security control CTRL-CRED-001)."
        );
        eprintln!(
            "       Use env var CORELINK_PAT or config file ~/.corelink/config.toml instead."
        );
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
        // Wave-19 — `verify-ndjson --url` abort-trailer arm exits
        // with sysexits DATAERR (65) so wrapping scripts (Drata /
        // SIEM / re-export automation) can distinguish a data-
        // integrity event from generic CLI failure. The structured
        // diagnostic was already printed to stderr by the command
        // module; we deliberately skip re-printing here.
        if matches!(e, CliError::AuditExportAborted) {
            std::process::exit(commands::verify_ndjson_http::EXIT_DATAERR);
        }
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
        Commands::Audit { action } => {
            return (label, run_audit(action, format).await);
        }
        // Login does not read a PAT from env/config — it receives it explicitly.
        Commands::Login { token } => {
            return (label, commands::login::run(token, format).await);
        }
        // Offline: verifying a previously exported bundle must NOT require
        // a live PAT (a departed customer may have revoked all tokens).
        Commands::Tenant {
            action: TenantAction::VerifyExport { archive },
        } => {
            return (label, commands::tenant::run_verify_export(archive, format));
        }
        _ => {}
    }

    // All other commands require a resolved + validated PAT.
    let pat = match resolve_pat() {
        Ok(p) => p,
        Err(e) => return (label, Err(e)),
    };

    // `bazel-init` needs the PAT value (to write scoped credentials) but
    // performs no network I/O, so it runs before the HTTP client is built.
    if let Commands::BazelInit { force } = &cli.command {
        return (label, commands::bazel_init::run(&pat, *force, format));
    }

    let client = match CorelinkClient::new(pat) {
        Ok(c) => c,
        Err(e) => return (label, Err(e)),
    };

    let res = match cli.command {
        Commands::Ls {
            tenant,
            prefix,
            limit,
            cursor,
        } => {
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
        Commands::Doctor { json } => commands::doctor_cmd::run(&client, json, format).await,
        // Stream-1 additions.
        Commands::Whoami => commands::whoami::run(&client, format).await,
        Commands::Ac { action } => run_ac(&client, &action, format).await,
        // Missing-commands wave.
        Commands::Tenant { action } => run_tenant(&client, &action, format).await,
        Commands::Cas { action } => run_cas(&client, &action, format).await,
        Commands::Import { source } => commands::import_cmd::run(&client, &source, format).await,
        Commands::Ci { action } => run_ci(&client, &action, format).await,
        // Handled above; unreachable here.
        Commands::Version
        | Commands::Config { .. }
        | Commands::RunbookDrill { .. }
        | Commands::Audit { .. }
        | Commands::Login { .. }
        | Commands::BazelInit { .. } => unreachable!(),
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
        Commands::Audit { .. } => "audit",
        Commands::Whoami => "whoami",
        Commands::Login { .. } => "login",
        Commands::Ac { .. } => "ac",
        Commands::BazelInit { .. } => "bazel-init",
        Commands::Tenant { .. } => "tenant",
        Commands::Cas { .. } => "cas",
        Commands::Import { .. } => "import",
        Commands::Ci { .. } => "ci",
    }
}

async fn run_tenant(
    client: &CorelinkClient,
    action: &TenantAction,
    format: OutputFormat,
) -> Result<(), CliError> {
    match action {
        TenantAction::Export { tenant_id, output } => {
            let tenant = tenant_id
                .clone()
                .or_else(|| client.tenant_id().map(str::to_owned))
                .ok_or_else(|| {
                    CliError::Other(
                        "tenant export: no --tenant-id and no cached tenant — run \
                         `corelink login`/`corelink whoami` or pass --tenant-id."
                            .to_owned(),
                    )
                })?;
            commands::tenant::run_export(client, &tenant, output, format).await
        }
        // Also handled in the no-PAT block; kept for exhaustiveness.
        TenantAction::VerifyExport { archive } => {
            commands::tenant::run_verify_export(archive, format)
        }
    }
}

async fn run_cas(
    client: &CorelinkClient,
    action: &CasAction,
    format: OutputFormat,
) -> Result<(), CliError> {
    match action {
        CasAction::Get { digest, output } => {
            commands::cas::run_get(client, digest, output.clone(), format).await
        }
        CasAction::Export { tenant, output } => {
            commands::cas::run_export(client, tenant, output, format).await
        }
    }
}

async fn run_ci(
    client: &CorelinkClient,
    action: &CiAction,
    format: OutputFormat,
) -> Result<(), CliError> {
    match action {
        CiAction::Mirror { from, to } => commands::ci::run(client, from, to, format).await,
    }
}

async fn run_audit(action: &AuditAction, format: OutputFormat) -> Result<(), CliError> {
    match action {
        AuditAction::Export {
            tenant,
            since,
            until,
            format: export_format,
            output,
            include_merkle_proofs,
            verify,
            fixture,
            fixture_events,
            fixture_start_ms,
        } => {
            let out_dir = output
                .clone()
                .unwrap_or_else(commands::audit::default_output_dir);
            if *fixture {
                // Offline fixture transport — no PAT required, fully deterministic.
                let tenant_uuid: uuid::Uuid = tenant.parse().map_err(|e| {
                    CliError::Other(format!("--tenant must be a valid UUIDv7: {e}"))
                })?;
                let exporter = commands::audit::build_fixture_exporter(
                    tenant_uuid,
                    *fixture_events,
                    *fixture_start_ms,
                )?;
                commands::audit::run_export(
                    &exporter,
                    tenant,
                    *since,
                    *until,
                    *export_format,
                    &out_dir,
                    *include_merkle_proofs,
                    *verify,
                    format,
                )?;
                Ok(())
            } else {
                // Production: stream the NDJSON window from the live
                // `GET /v1/audit/:tenant/export` route and persist it
                // content-addressed (re-verify with `audit verify-ndjson`).
                let pat = resolve_pat()?;
                let client = CorelinkClient::new(pat)?;
                commands::audit_net::run_export_production(
                    &client, tenant, *since, *until, &out_dir, format,
                )
                .await
            }
        }
        AuditAction::Tail {
            tenant,
            filter,
            since,
            limit,
        } => {
            let pat = resolve_pat()?;
            let client = CorelinkClient::new(pat)?;
            let t = tenant
                .clone()
                .or_else(|| client.tenant_id().map(str::to_owned))
                .ok_or_else(|| {
                    CliError::Other(
                        "audit tail: no --tenant and no cached tenant — run \
                         `corelink login`/`corelink whoami` or pass --tenant."
                            .to_owned(),
                    )
                })?;
            commands::audit_net::run_tail(&client, &t, filter.as_deref(), *since, *limit, format)
                .await
        }
        AuditAction::Verify { path } => {
            commands::audit::run_verify(path, format)?;
            Ok(())
        }
        AuditAction::VerifyNdjson {
            ndjson,
            url,
            bearer,
            chain_head_anchor,
        } => {
            match (ndjson, url) {
                (Some(path), None) => {
                    let anchor = chain_head_anchor.as_deref().ok_or_else(|| {
                        CliError::Other(
                            "audit verify-ndjson --ndjson requires --chain-head-anchor (the value from response header X-CoreLink-Audit-Export-Chain-Head-Anchor)".to_owned(),
                        )
                    })?;
                    commands::verify_ndjson::run_verify_ndjson(path, anchor, format)?;
                    Ok(())
                }
                (None, Some(u)) => {
                    let token = bearer.as_deref().ok_or_else(|| {
                        CliError::Other(
                            "audit verify-ndjson --url requires --bearer <TOKEN> (or env CORELINK_PAT)".to_owned(),
                        )
                    })?;
                    let outcome = commands::verify_ndjson_http::run_verify_ndjson_http(
                        u,
                        token,
                        chain_head_anchor.as_deref(),
                        format,
                    )
                    .await?;
                    // The abort arm prints to stderr inside the
                    // command + must exit with sysexits DATAERR (65).
                    // We thread this through a dedicated CliError
                    // variant that `main` translates to the right
                    // process exit code below.
                    if matches!(
                        outcome,
                        commands::verify_ndjson_http::HttpVerifyOutcome::AbortedMidStream { .. }
                    ) {
                        return Err(CliError::AuditExportAborted);
                    }
                    Ok(())
                }
                (Some(_), Some(_)) => Err(CliError::Other(
                    "audit verify-ndjson: --ndjson and --url are mutually exclusive".to_owned(),
                )),
                (None, None) => Err(CliError::Other(
                    "audit verify-ndjson: supply either --ndjson <FILE> or --url <URL>".to_owned(),
                )),
            }
        }
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

async fn run_ac(
    client: &CorelinkClient,
    action: &AcAction,
    format: OutputFormat,
) -> Result<(), CliError> {
    match action {
        AcAction::Put { digest, result } => {
            commands::ac::run_put(client, digest, result, format).await
        }
        AcAction::Get { digest, output } => {
            commands::ac::run_get(client, digest, output.clone(), format).await
        }
    }
}

fn run_config(action: &ConfigAction, format: OutputFormat) -> Result<(), CliError> {
    match action {
        ConfigAction::Set { key, value } => commands::config_cmd::run_set(key, value, format),
        ConfigAction::Get { key } => commands::config_cmd::run_get(key, format),
        ConfigAction::List => commands::config_cmd::run_list(format),
        ConfigAction::Apply { file } => commands::config_cmd::run_apply(file, format),
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

#[cfg(test)]
mod cli_tests {
    use super::Cli;
    use clap::CommandFactory as _;

    /// B1 regression: the global `--output` (format) flag must NOT share a
    /// clap arg id with any subcommand file-output arg. `debug_assert()`
    /// panics on a duplicate arg id, so this proves the collision is gone.
    #[test]
    fn cli_command_has_no_arg_id_collisions() {
        Cli::command().debug_assert();
    }
}
