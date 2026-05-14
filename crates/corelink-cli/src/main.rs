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
    }
}
