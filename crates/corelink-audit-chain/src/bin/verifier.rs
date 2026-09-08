//! Daily-verify CLI binary (Wave 15 GA).
//!
//! Usage:
//!
//! ```sh
//! cargo run -p corelink-audit-chain --bin verifier -- <ndjson_chunk_path>...
//! ```
//!
//! For each NDJSON chunk path passed on the CLI, this binary reads the file and
//! verifies every line. Two line formats are accepted, distinguished by the
//! `schema` field:
//!
//! - **`corelink.audit.sealed.v1`** — the LIVE chain's archive
//!   ([`corelink_audit_chain::SealedArchiveLine`], written by the container's
//!   `POST /_internal/audit/archive`). Lines are grouped per
//!   `(tenant_id, region)` chain partition and checked with
//!   [`corelink_audit_chain::verify_chunk`], which recomputes each link from
//!   the PERSISTED JCS bytes. This is the format the daily cron sees in R2.
//! - **typed [`corelink_audit_chain::AuditEvent`]** — the crate's own producer
//!   stream, grouped by `tenant_id` and checked with
//!   [`corelink_audit_chain::ChainVerifier::verify_chain_from_genesis`].
//!
//! Exits with code:
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
    verify_chunk_for_epoch, AuditEvent, ChainEpoch, ChainHash, ChainVerifier,
    InMemoryAuditChainAuditSink, LinkKeyring, SealedArchiveLine, SEALED_LINE_SCHEMA,
    SEALED_LINE_SCHEMA_V2,
};
use zeroize::Zeroizing;

fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (keyring, args) = match parse_args(&raw_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!(
                "usage: verifier [--chain-keyring PATH] <ndjson_chunk_path>...\nerror: {error}"
            );
            return ExitCode::FAILURE;
        }
    };
    if args.is_empty() {
        // Zero-input is the cron-safe no-op (first day after deploy, no
        // chunks yet to verify). The OK marker keeps the cron grep happy.
        eprintln!("usage: verifier [--chain-keyring PATH] <ndjson_chunk_path>...");
        println!("AUDIT_CHAIN_VERIFY_OK: no input paths (cron no-op)");
        return ExitCode::SUCCESS;
    }

    match run(&args, &keyring) {
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

fn run(paths: &[String], keyring: &LinkKeyring) -> Result<String, String> {
    let mut by_tenant: BTreeMap<uuid::Uuid, Vec<AuditEvent>> = BTreeMap::new();
    // Sealed-row lines are keyed by the LIVE chain's partition — `(tenant_id,
    // region)` — and their tenant id is an opaque `audit_outbox.tenant_id`
    // string, so they are grouped separately from the typed `AuditEvent` path
    // rather than forced through `uuid::Uuid`.
    let mut by_partition: BTreeMap<(String, String), Vec<SealedArchiveLine>> = BTreeMap::new();
    let mut total_events: u64 = 0;
    for p in paths {
        let body = std::fs::read_to_string(p).map_err(|e| format!("read {}: {}", p, e))?;
        for (i, line) in body.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if is_sealed_line(trimmed) {
                let sealed: SealedArchiveLine = serde_json::from_str(trimmed)
                    .map_err(|e| format!("parse {} line {}: {}", p, i.saturating_add(1), e))?;
                by_partition
                    .entry((sealed.tenant_id.clone(), sealed.region.clone()))
                    .or_default()
                    .push(sealed);
            } else {
                let ev = AuditEvent::from_ndjson_line(trimmed)
                    .map_err(|e| format!("parse {} line {}: {}", p, i.saturating_add(1), e))?;
                by_tenant.entry(ev.tenant_id).or_default().push(ev);
            }
            total_events = total_events.saturating_add(1);
        }
    }

    if total_events == 0 {
        return Ok("no events in input (clean)".to_string());
    }

    let mut chains_verified: u64 = 0;
    let mut events_verified: u64 = 0;

    // ---- sealed-row chains (the LIVE audit trail) ----
    //
    // A day-windowed archive starts wherever the window starts, so this asserts
    // the window's OWN properties: contiguous sequence numbers, each line's
    // `prev_hash` equal to the previous line's `chain_hash`, and every link
    // reproducible as `BLAKE3(prev_hash || canonical_jcs)` from the persisted
    // bytes. It deliberately does NOT demand the window begin at genesis —
    // that would fail every archive that is not the tenant's first day.
    for ((tenant_id, region), mut lines) in by_partition {
        lines.sort_by_key(|l| l.sequence_number);
        lines.dedup_by(|a, b| a.sequence_number == b.sequence_number && a.row_id == b.row_id);
        verify_sealed_partition(&lines, keyring)
            .map_err(|e| format!("tenant={}: region={}: {}", tenant_id, region, e))?;
        events_verified = events_verified.saturating_add(lines.len() as u64);
        chains_verified = chains_verified.saturating_add(1);
    }

    // ---- typed AuditEvent chains (the crate's own producer) ----
    let audit = std::sync::Arc::new(InMemoryAuditChainAuditSink::new());
    let verifier = ChainVerifier::new(audit);
    let now_ms: u64 = 0; // pure-logic CLI; cron passes real epoch via env if needed.
    for (tenant_id, mut events) in by_tenant {
        events.sort_by_key(|e| e.sequence_number);
        let outcome = verifier
            .verify_chain_from_genesis(&events, tenant_id, "daily-verify-cron", now_ms)
            .map_err(|e| format!("tenant={}: {}", tenant_id, e))?;
        events_verified = events_verified.saturating_add(outcome.events_verified_count);
        chains_verified = chains_verified.saturating_add(1);
    }

    Ok(format!(
        "chains_verified={} events_verified={}",
        chains_verified, events_verified
    ))
}

/// Parse the optional write-only keyring before reading archive data. Invalid
/// material fails closed; no partial keyring or legacy downgrade is allowed.
fn parse_args(args: &[String]) -> Result<(LinkKeyring, Vec<String>), String> {
    let mut keyring_path: Option<&str> = None;
    let mut paths = Vec::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--chain-keyring" {
            index = index.saturating_add(1);
            keyring_path = Some(
                args.get(index)
                    .ok_or_else(|| "--chain-keyring requires a path".to_owned())?,
            );
        } else if let Some(path) = arg.strip_prefix("--chain-keyring=") {
            if path.is_empty() {
                return Err("--chain-keyring requires a path".to_owned());
            }
            keyring_path = Some(path);
        } else if arg == "--help" || arg == "-h" {
            return Err("--chain-keyring PATH is optional; paths are NDJSON archives".to_owned());
        } else if arg.starts_with('-') {
            return Err(format!("unknown option: {arg}"));
        } else {
            paths.push(arg.clone());
        }
        index = index.saturating_add(1);
    }
    let keyring = match keyring_path {
        Some(path) => {
            let raw = Zeroizing::new(
                std::fs::read_to_string(path).map_err(|e| format!("read keyring {path}: {e}"))?,
            );
            LinkKeyring::parse_json(&raw).map_err(|e| format!("parse keyring {path}: {e}"))?
        }
        None => LinkKeyring::default(),
    };
    Ok((keyring, paths))
}

fn parse_chain_hash(raw: &str) -> Result<ChainHash, String> {
    let bytes =
        hex::decode(raw).map_err(|_| "first keyed line has malformed prev_hash".to_owned())?;
    if bytes.len() != 32 {
        return Err("first keyed line has malformed prev_hash length".to_owned());
    }
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(ChainHash(hash))
}

fn verify_sealed_partition(
    lines: &[SealedArchiveLine],
    keyring: &LinkKeyring,
) -> Result<(), String> {
    let Some(first) = lines.first() else {
        return Err("sealed archive chunk is empty".to_owned());
    };
    if lines
        .iter()
        .all(|line| line.algorithm_id == 0 && line.epoch_id == 0 && line.link_key_id.is_none())
    {
        return corelink_audit_chain::verify_chunk(lines).map_err(|e| e.to_string());
    }
    if first.algorithm_id != 1 || first.epoch_id == 0 {
        return Err("unsupported or malformed keyed archive metadata".to_owned());
    }
    let link_key_id = first
        .link_key_id
        .ok_or_else(|| "keyed archive line is missing link_key_id".to_owned())?;
    let key = keyring
        .get(link_key_id)
        .ok_or_else(|| format!("link-key id {link_key_id} is not present in keyring"))?;
    let epoch = ChainEpoch::keyed_successor(
        first.epoch_id,
        link_key_id,
        first.sequence_number,
        parse_chain_hash(&first.prev_hash)?,
        first.epoch_id.saturating_sub(1),
    )
    .map_err(|e| format!("invalid keyed archive epoch: {e}"))?;
    verify_chunk_for_epoch(lines, &epoch, Some(key)).map_err(|e| e.to_string())
}

/// Cheap discriminator: a sealed-archive line carries the `schema` tag.
///
/// Parsing into a `serde_json::Value` first (rather than trying
/// `SealedArchiveLine` and falling back on error) keeps a MALFORMED sealed line
/// a hard failure instead of silently re-routing it to the `AuditEvent` parser
/// and reporting the wrong diagnostic.
fn is_sealed_line(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|v| {
            v.get("schema")
                .and_then(serde_json::Value::as_str)
                .map(|s| s == SEALED_LINE_SCHEMA || s == SEALED_LINE_SCHEMA_V2)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_keyring_option_is_fail_closed() {
        assert!(parse_args(&["--chain-keyring".to_owned()]).is_err());
        assert!(parse_args(&["--unknown".to_owned()]).is_err());
        let (keyring, paths) = parse_args(&["archive.ndjson".to_owned()]).expect("plain CLI");
        assert!(keyring.is_empty());
        assert_eq!(paths, vec!["archive.ndjson"]);
    }
}
