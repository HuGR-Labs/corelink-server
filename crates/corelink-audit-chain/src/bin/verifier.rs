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
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine;
use corelink_audit_chain::{
    canonical_date_yyyy_mm_dd, sealed_chunk_key, verify_chunk_for_epoch, AuditEvent, ChainEpoch,
    ChainHash, ChainVerifier, InMemoryAuditChainAuditSink, LinkKeyring, SealedArchiveLine,
    SEALED_LINE_SCHEMA, SEALED_LINE_SCHEMA_V2,
};
use ed25519_dalek::Verifier;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const CHECKPOINT_SCHEMA: &str = "corelink.audit.daily-checkpoint.v1";
const CHECKPOINT_MAC_DOMAIN: &[u8] = b"corelink/audit-chain/daily-checkpoint/v1\0";
const WITNESS_DOMAIN: &[u8] = b"corelink/audit-chain/head-witness/v1\0";
const HEAD_RECORD_DOMAIN: &[u8] = b"corelink/audit-chain/head-record/v1\0";
const RECEIPT_DOMAIN: &[u8] = b"corelink/audit-chain/head-witness-receipt/v1\0";

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct PartitionCheckpoint {
    schema: String,
    tenant_id: String,
    region: String,
    through_day: String,
    last_sequence: u64,
    last_chain_hash: String,
    current_epoch_id: u64,
    current_link_key_id: Option<u64>,
    next_sequence: u64,
    next_prev_hash: String,
    next_day: String,
    checkpoint_key_id: u64,
    previous_checkpoint_mac: String,
    checkpoint_mac: String,
}

#[derive(serde::Serialize)]
struct CheckpointMacPayload<'a> {
    schema: &'a str,
    tenant_id: &'a str,
    region: &'a str,
    through_day: &'a str,
    last_sequence: u64,
    last_chain_hash: &'a str,
    current_epoch_id: u64,
    current_link_key_id: Option<u64>,
    next_sequence: u64,
    next_prev_hash: &'a str,
    next_day: &'a str,
    checkpoint_key_id: u64,
    previous_checkpoint_mac: &'a str,
}

#[derive(Default)]
struct CliArgs {
    keyring: LinkKeyring,
    paths: Vec<String>,
    verify_day: Option<String>,
    checkpoint_key_id: Option<u64>,
    checkpoint_in: Option<PathBuf>,
    checkpoint_out: Option<PathBuf>,
    witness_id: Option<String>,
    witness_public_keys: Option<PathBuf>,
    witness_receipts_dir: Option<PathBuf>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWitnessReceipt {
    receipt_jcs_b64: String,
    receipt_signature_b64: String,
    witness_jcs_b64: String,
    witness_key_id: u64,
    witness_record_hash: String,
    witness_sequence: u64,
    tenant_id: String,
    region: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessReceiptJcs {
    committed_at_ms: u64,
    head_record_hash: String,
    previous_witness_hash: String,
    receipt_version: u8,
    region: String,
    tenant_id: String,
    witness_id: String,
    witness_key_id: u64,
    witness_record_hash: String,
    witness_sequence: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessRecord {
    head_message_b64: String,
    head_record_hash: String,
    head_signature_b64: String,
    previous_witness_hash: String,
    region: String,
    tenant_id: String,
    witness_sequence: u64,
    witness_version: u8,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessHeadV2 {
    epoch_id: u64,
    epoch_ledger_hash: String,
    epoch_ledger_sequence: u64,
    head_hash: String,
    head_message_version: u8,
    next_sequence: u64,
    region: String,
    signing_key_id: u64,
    tenant_id: String,
}

#[derive(Clone)]
struct PartitionAnchor {
    tenant_id: String,
    region: String,
    next_sequence: u64,
    next_prev_hash: String,
    current_epoch_id: u64,
    current_link_key_id: Option<u64>,
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!(
                "usage: verifier [--chain-keyring PATH] <ndjson_chunk_path>...\nerror: {error}"
            );
            return ExitCode::FAILURE;
        }
    };
    if args.paths.is_empty() && args.checkpoint_out.is_none() {
        // Zero-input is the cron-safe no-op (first day after deploy, no
        // chunks yet to verify). The OK marker keeps the cron grep happy.
        eprintln!("usage: verifier [--chain-keyring PATH] <ndjson_chunk_path>...");
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

fn run(args: &CliArgs) -> Result<String, String> {
    let input_checkpoints = load_checkpoints(args)?;
    let bootstrap_anchors = load_bootstrap_anchors(args)?;
    let mut by_tenant: BTreeMap<uuid::Uuid, Vec<AuditEvent>> = BTreeMap::new();
    // Sealed-row lines are keyed by the LIVE chain's partition — `(tenant_id,
    // region)` — and their tenant id is an opaque `audit_outbox.tenant_id`
    // string, so they are grouped separately from the typed `AuditEvent` path
    // rather than forced through `uuid::Uuid`.
    let mut by_partition: BTreeMap<(String, String), Vec<SealedArchiveLine>> = BTreeMap::new();
    let mut total_events: u64 = 0;
    for p in &args.paths {
        let body = std::fs::read_to_string(p).map_err(|e| format!("read {}: {}", p, e))?;
        let mut file_sealed = Vec::new();
        for (i, line) in body.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if is_sealed_line(trimmed) {
                let sealed: SealedArchiveLine = serde_json::from_str(trimmed)
                    .map_err(|e| format!("parse {} line {}: {}", p, i.saturating_add(1), e))?;
                file_sealed.push(sealed.clone());
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
        if !file_sealed.is_empty() {
            let day = args.verify_day.as_deref().ok_or_else(|| {
                "sealed archive verification requires --verify-day for object binding".to_owned()
            })?;
            verify_archive_object_binding(p, &file_sealed, day, body.as_bytes())?;
        }
    }

    if total_events == 0 && args.checkpoint_out.is_none() {
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
    let mut next_checkpoints = BTreeMap::new();
    for ((tenant_id, region), mut lines) in by_partition {
        lines.sort_by_key(|l| l.sequence_number);
        reject_duplicate_lines(&lines)
            .map_err(|e| format!("tenant={tenant_id}: region={region}: {e}"))?;
        let partition = (tenant_id.clone(), region.clone());
        let prior_checkpoint = input_checkpoints.get(&partition);
        let checkpoint_anchor = prior_checkpoint.map(|value| PartitionAnchor {
            tenant_id: value.tenant_id.clone(),
            region: value.region.clone(),
            next_sequence: value.next_sequence,
            next_prev_hash: value.next_prev_hash.clone(),
            current_epoch_id: value.current_epoch_id,
            current_link_key_id: value.current_link_key_id,
        });
        let first = lines.first().ok_or("sealed partition empty")?;
        let witness_matches: Vec<&PartitionAnchor> = bootstrap_anchors
            .get(&partition)
            .into_iter()
            .flatten()
            .filter(|anchor| anchor_matches_first(anchor, first))
            .collect();
        let anchor = if let Some(checkpoint) = checkpoint_anchor.as_ref() {
            checkpoint
        } else if witness_matches.len() == 1 {
            witness_matches[0]
        } else {
            return Err(format!(
                "tenant={tenant_id}: region={region}: authenticated external bootstrap anchor is absent or ambiguous"
            ));
        };
        verify_sealed_partition(&lines, &args.keyring, anchor)
            .map_err(|e| format!("tenant={}: region={}: {}", tenant_id, region, e))?;
        if args.checkpoint_out.is_some() {
            let checkpoint = checkpoint_after(&lines, prior_checkpoint, args)?;
            next_checkpoints.insert(partition, checkpoint);
        }
        events_verified = events_verified.saturating_add(lines.len() as u64);
        chains_verified = chains_verified.saturating_add(1);
    }
    for (partition, checkpoint) in &input_checkpoints {
        if !next_checkpoints.contains_key(partition) {
            next_checkpoints.insert(partition.clone(), carry_checkpoint(checkpoint, args)?);
        }
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

    if let Some(path) = &args.checkpoint_out {
        write_checkpoints_atomic(path, next_checkpoints.values())?;
    }
    Ok(format!(
        "chains_verified={} events_verified={}",
        chains_verified, events_verified
    ))
}

fn verify_archive_object_binding(
    path: &str,
    lines: &[SealedArchiveLine],
    day: &str,
    object_bytes: &[u8],
) -> Result<(), String> {
    let first = lines
        .first()
        .ok_or_else(|| "archive object is empty".to_owned())?;
    if lines.iter().any(|line| {
        u64::try_from(line.enqueued_at_ms)
            .ok()
            .map(canonical_date_yyyy_mm_dd)
            .as_deref()
            != Some(day)
    }) {
        return Err(format!("archive object {path} contains a cross-day row"));
    }
    let base_key = sealed_chunk_key(first);
    let expected_key = if first.schema == SEALED_LINE_SCHEMA_V2 {
        if lines.iter().any(|line| {
            line.epoch_id != first.epoch_id
                || line.algorithm_id != first.algorithm_id
                || line.link_key_id != first.link_key_id
        }) {
            return Err("keyed archive object mixes epoch metadata".to_owned());
        }
        format!(
            "{base_key}.epoch-{}.{}",
            first.epoch_id,
            domain_hash(&[], object_bytes)
        )
    } else {
        base_key
    };
    if !path.replace('\\', "/").ends_with(&expected_key) {
        return Err(format!(
            "archive object path does not match authenticated row key: {path}"
        ));
    }
    Ok(())
}

fn reject_duplicate_lines(lines: &[SealedArchiveLine]) -> Result<(), String> {
    if lines
        .windows(2)
        .any(|pair| pair[0].sequence_number == pair[1].sequence_number)
    {
        return Err("duplicate sequence number".to_owned());
    }
    let mut row_ids = std::collections::BTreeSet::new();
    if lines
        .iter()
        .any(|line| !row_ids.insert(line.row_id.as_str()))
    {
        return Err("duplicate row id".to_owned());
    }
    Ok(())
}

fn anchor_matches_first(anchor: &PartitionAnchor, first: &SealedArchiveLine) -> bool {
    let same_epoch = anchor.current_epoch_id == first.epoch_id
        && anchor
            .current_link_key_id
            .is_none_or(|key_id| Some(key_id) == first.link_key_id);
    let rotated_epoch = anchor.current_epoch_id.checked_add(1) == Some(first.epoch_id)
        && anchor.current_link_key_id != first.link_key_id;
    anchor.tenant_id == first.tenant_id
        && anchor.region == first.region
        && anchor.next_sequence == first.sequence_number
        && anchor.next_prev_hash == first.prev_hash
        && (same_epoch || rotated_epoch)
}

/// Parse the optional write-only keyring before reading archive data. Invalid
/// material fails closed; no partial keyring or legacy downgrade is allowed.
fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut keyring_path: Option<&str> = None;
    let mut parsed = CliArgs::default();
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
        } else if matches!(
            arg.as_str(),
            "--verify-day"
                | "--checkpoint-key-id"
                | "--checkpoint-in"
                | "--checkpoint-out"
                | "--witness-id"
                | "--witness-public-keys"
                | "--witness-receipts-dir"
        ) {
            index = index
                .checked_add(1)
                .ok_or_else(|| "argument index overflow".to_owned())?;
            let value = args
                .get(index)
                .ok_or_else(|| format!("{arg} requires a value"))?;
            match arg.as_str() {
                "--verify-day" => parsed.verify_day = Some(value.clone()),
                "--checkpoint-key-id" => {
                    parsed.checkpoint_key_id =
                        Some(value.parse::<u64>().map_err(|_| {
                            "--checkpoint-key-id must be a positive integer".to_owned()
                        })?)
                }
                "--checkpoint-in" => parsed.checkpoint_in = Some(PathBuf::from(value)),
                "--checkpoint-out" => parsed.checkpoint_out = Some(PathBuf::from(value)),
                "--witness-id" => parsed.witness_id = Some(value.clone()),
                "--witness-public-keys" => parsed.witness_public_keys = Some(PathBuf::from(value)),
                "--witness-receipts-dir" => {
                    parsed.witness_receipts_dir = Some(PathBuf::from(value))
                }
                _ => unreachable!(),
            }
        } else if arg == "--help" || arg == "-h" {
            return Err("--chain-keyring PATH is optional; paths are NDJSON archives".to_owned());
        } else if arg.starts_with('-') {
            return Err(format!("unknown option: {arg}"));
        } else {
            parsed.paths.push(arg.clone());
        }
        index = index.saturating_add(1);
    }
    parsed.keyring = match keyring_path {
        Some(path) => {
            let raw = Zeroizing::new(
                std::fs::read_to_string(path).map_err(|e| format!("read keyring {path}: {e}"))?,
            );
            LinkKeyring::parse_json(&raw).map_err(|e| format!("parse keyring {path}: {e}"))?
        }
        None => LinkKeyring::default(),
    };
    let checkpoint_mode = parsed.checkpoint_in.is_some()
        || parsed.checkpoint_out.is_some()
        || parsed.verify_day.is_some()
        || parsed.checkpoint_key_id.is_some()
        || parsed.witness_id.is_some()
        || parsed.witness_public_keys.is_some()
        || parsed.witness_receipts_dir.is_some();
    if checkpoint_mode
        && (parsed.verify_day.is_none()
            || parsed.checkpoint_key_id.is_none()
            || parsed.checkpoint_out.is_none())
    {
        return Err(
            "checkpoint mode requires --verify-day, --checkpoint-key-id, and --checkpoint-out"
                .to_owned(),
        );
    }
    let witness_options = usize::from(parsed.witness_id.is_some())
        + usize::from(parsed.witness_public_keys.is_some())
        + usize::from(parsed.witness_receipts_dir.is_some());
    if witness_options != 0 && witness_options != 3 {
        return Err("witness bootstrap requires id, public keys, and receipt directory".to_owned());
    }
    if let Some(id) = parsed.checkpoint_key_id {
        if id == 0 || parsed.keyring.get(id).is_none() {
            return Err("checkpoint key id is absent from the supplied keyring".to_owned());
        }
    }
    Ok(parsed)
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

fn checkpoint_payload(checkpoint: &PartitionCheckpoint) -> CheckpointMacPayload<'_> {
    CheckpointMacPayload {
        schema: &checkpoint.schema,
        tenant_id: &checkpoint.tenant_id,
        region: &checkpoint.region,
        through_day: &checkpoint.through_day,
        last_sequence: checkpoint.last_sequence,
        last_chain_hash: &checkpoint.last_chain_hash,
        current_epoch_id: checkpoint.current_epoch_id,
        current_link_key_id: checkpoint.current_link_key_id,
        next_sequence: checkpoint.next_sequence,
        next_prev_hash: &checkpoint.next_prev_hash,
        next_day: &checkpoint.next_day,
        checkpoint_key_id: checkpoint.checkpoint_key_id,
        previous_checkpoint_mac: &checkpoint.previous_checkpoint_mac,
    }
}

fn checkpoint_mac(
    checkpoint: &PartitionCheckpoint,
    keyring: &LinkKeyring,
) -> Result<String, String> {
    let key = keyring.get(checkpoint.checkpoint_key_id).ok_or_else(|| {
        format!(
            "checkpoint key id {} is absent from keyring",
            checkpoint.checkpoint_key_id
        )
    })?;
    let canonical = serde_jcs::to_vec(&checkpoint_payload(checkpoint))
        .map_err(|error| format!("canonicalize checkpoint: {error}"))?;
    let mut hasher = blake3::Hasher::new_keyed(key.as_bytes());
    hasher.update(CHECKPOINT_MAC_DOMAIN);
    hasher.update(&canonical);
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_checkpoint(
    checkpoint: &PartitionCheckpoint,
    day: &str,
    keyring: &LinkKeyring,
) -> Result<(), String> {
    if checkpoint.schema != CHECKPOINT_SCHEMA {
        return Err("checkpoint has unknown schema".to_owned());
    }
    if checkpoint.next_day != day {
        return Err("checkpoint replay or wrong verification day".to_owned());
    }
    if checkpoint.next_sequence
        != checkpoint
            .last_sequence
            .checked_add(1)
            .ok_or_else(|| "checkpoint sequence overflows u64".to_owned())?
        || checkpoint.next_prev_hash != checkpoint.last_chain_hash
    {
        return Err("checkpoint next expectations are inconsistent".to_owned());
    }
    parse_chain_hash(&checkpoint.last_chain_hash)?;
    parse_chain_hash(&checkpoint.previous_checkpoint_mac)?;
    let expected = checkpoint_mac(checkpoint, keyring)?;
    if expected
        .as_bytes()
        .ct_eq(checkpoint.checkpoint_mac.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err("checkpoint MAC authentication failed".to_owned());
    }
    Ok(())
}

fn load_checkpoints(
    args: &CliArgs,
) -> Result<BTreeMap<(String, String), PartitionCheckpoint>, String> {
    let Some(path) = &args.checkpoint_in else {
        return Ok(BTreeMap::new());
    };
    let day = args
        .verify_day
        .as_deref()
        .ok_or_else(|| "checkpoint input requires verify day".to_owned())?;
    let body = std::fs::read_to_string(path)
        .map_err(|e| format!("read checkpoint {}: {e}", path.display()))?;
    let mut checkpoints = BTreeMap::new();
    for (index, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let checkpoint: PartitionCheckpoint = serde_json::from_str(line)
            .map_err(|e| format!("parse checkpoint line {}: {e}", index.saturating_add(1)))?;
        validate_checkpoint(&checkpoint, day, &args.keyring)?;
        let partition = (checkpoint.tenant_id.clone(), checkpoint.region.clone());
        if checkpoints.insert(partition, checkpoint).is_some() {
            return Err("checkpoint contains duplicate partition".to_owned());
        }
    }
    Ok(checkpoints)
}

fn decode_canonical_base64<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| format!("{label} is malformed base64"))?;
    if decoded.len() != N || base64::engine::general_purpose::STANDARD.encode(&decoded) != value {
        return Err(format!("{label} is not canonical padded base64"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("{label} has wrong length"))
}

fn decode_canonical_base64_vec(value: &str, label: &str) -> Result<Vec<u8>, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| format!("{label} is malformed base64"))?;
    if base64::engine::general_purpose::STANDARD.encode(&decoded) != value {
        return Err(format!("{label} is not canonical padded base64"));
    }
    Ok(decoded)
}

fn domain_hash(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(payload);
    hasher.finalize().to_hex().to_string()
}

fn verify_domain_signature(
    key: &ed25519_dalek::VerifyingKey,
    domain: &[u8],
    payload: &[u8],
    encoded: &str,
) -> Result<(), String> {
    let signature = ed25519_dalek::Signature::from_bytes(&decode_canonical_base64::<64>(
        encoded,
        "witness signature",
    )?);
    let mut message =
        Vec::with_capacity(domain.len().saturating_add(8).saturating_add(payload.len()));
    message.extend_from_slice(domain);
    message.extend_from_slice(
        &u64::try_from(payload.len())
            .map_err(|_| "witness payload too large")?
            .to_be_bytes(),
    );
    message.extend_from_slice(payload);
    key.verify(&message, &signature)
        .map_err(|_| "witness signature verification failed".to_owned())
}

fn parse_witness_keys(path: &Path) -> Result<BTreeMap<u64, ed25519_dalek::VerifyingKey>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read witness keys: {e}"))?;
    let entries: BTreeMap<String, String> =
        serde_json::from_str(&raw).map_err(|_| "witness public-key registry malformed")?;
    if entries.is_empty() {
        return Err("witness public-key registry is empty".to_owned());
    }
    let mut keys = BTreeMap::new();
    for (id, encoded) in entries {
        let id = id
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0 && value.to_string() == id)
            .ok_or("witness public-key id is not canonical")?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&decode_canonical_base64::<32>(
            &encoded,
            "witness public key",
        )?)
        .map_err(|_| "witness public key is invalid")?;
        keys.insert(id, key);
    }
    Ok(keys)
}

fn load_bootstrap_anchors(
    args: &CliArgs,
) -> Result<BTreeMap<(String, String), Vec<PartitionAnchor>>, String> {
    let (Some(witness_id), Some(keys_path), Some(receipts_dir)) = (
        &args.witness_id,
        &args.witness_public_keys,
        &args.witness_receipts_dir,
    ) else {
        return Ok(BTreeMap::new());
    };
    let keys = parse_witness_keys(keys_path)?;
    let mut anchors = BTreeMap::new();
    for item in std::fs::read_dir(receipts_dir)
        .map_err(|e| format!("read witness receipt directory: {e}"))?
    {
        let path = item
            .map_err(|e| format!("read witness capture entry: {e}"))?
            .path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("read witness receipt: {e}"))?;
        let stored: StoredWitnessReceipt =
            serde_json::from_slice(&bytes).map_err(|_| "stored witness receipt malformed")?;
        let witness_bytes =
            decode_canonical_base64_vec(&stored.witness_jcs_b64, "witness record JCS")?;
        let record: WitnessRecord =
            serde_json::from_slice(&witness_bytes).map_err(|_| "witness record malformed")?;
        if serde_jcs::to_vec(&record).map_err(|_| "witness record canonicalization failed")?
            != witness_bytes
            || record.witness_version != 1
            || record.tenant_id != stored.tenant_id
            || record.region != stored.region
            || record.witness_sequence != stored.witness_sequence
            || domain_hash(WITNESS_DOMAIN, &witness_bytes) != stored.witness_record_hash
        {
            return Err("witness record/hash/sequence binding failed".to_owned());
        }
        let receipt_bytes =
            decode_canonical_base64_vec(&stored.receipt_jcs_b64, "witness receipt JCS")?;
        let receipt: WitnessReceiptJcs =
            serde_json::from_slice(&receipt_bytes).map_err(|_| "witness receipt malformed")?;
        if serde_jcs::to_vec(&receipt).map_err(|_| "witness receipt canonicalization failed")?
            != receipt_bytes
            || receipt.receipt_version != 1
            || receipt.witness_id != *witness_id
            || receipt.witness_key_id != stored.witness_key_id
            || receipt.witness_sequence != stored.witness_sequence
            || receipt.witness_record_hash != stored.witness_record_hash
            || receipt.tenant_id != stored.tenant_id
            || receipt.region != stored.region
            || receipt.head_record_hash != record.head_record_hash
            || receipt.previous_witness_hash != record.previous_witness_hash
        {
            return Err("witness receipt fields do not bind historical record".to_owned());
        }
        let receipt_key = keys
            .get(&stored.witness_key_id)
            .ok_or("witness receipt key id absent from registry")?;
        verify_domain_signature(
            receipt_key,
            RECEIPT_DOMAIN,
            &receipt_bytes,
            &stored.receipt_signature_b64,
        )?;
        let head_bytes = decode_canonical_base64_vec(&record.head_message_b64, "witness head JCS")?;
        let head: WitnessHeadV2 =
            serde_json::from_slice(&head_bytes).map_err(|_| "witness head malformed")?;
        let head_signature =
            decode_canonical_base64::<64>(&record.head_signature_b64, "head signature")?;
        let mut head_material = Vec::new();
        head_material.extend_from_slice(HEAD_RECORD_DOMAIN);
        head_material.extend_from_slice(
            &u64::try_from(head_bytes.len())
                .map_err(|_| "head too large")?
                .to_be_bytes(),
        );
        head_material.extend_from_slice(&head_bytes);
        head_material.extend_from_slice(&head_signature);
        if serde_jcs::to_vec(&head).map_err(|_| "witness head canonicalization failed")?
            != head_bytes
            || head.head_message_version != 2
            || head.tenant_id != record.tenant_id
            || head.region != record.region
            || domain_hash(&[], &head_material) != record.head_record_hash
        {
            return Err("witness head record binding failed".to_owned());
        }
        let anchor = PartitionAnchor {
            tenant_id: head.tenant_id.clone(),
            region: head.region.clone(),
            next_sequence: head.next_sequence,
            next_prev_hash: head.head_hash,
            current_epoch_id: head.epoch_id,
            current_link_key_id: None,
        };
        anchors
            .entry((head.tenant_id, head.region))
            .or_insert_with(Vec::new)
            .push(anchor);
    }
    if anchors.is_empty() {
        return Err("witness receipt directory contains no anchors".to_owned());
    }
    Ok(anchors)
}

fn next_day(day: &str) -> Result<String, String> {
    let mut parts = day.split('-');
    let year = parts
        .next()
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or_else(|| "invalid verify day".to_owned())?;
    let month = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| "invalid verify day".to_owned())?;
    let date = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| "invalid verify day".to_owned())?;
    if parts.next().is_some() || year < 1970 || !(1..=12).contains(&month) {
        return Err("invalid verify day".to_owned());
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if date == 0 || date > days[(month - 1) as usize] {
        return Err("invalid verify day".to_owned());
    }
    let (next_year, next_month, next_date) = if date < days[(month - 1) as usize] {
        (year, month, date + 1)
    } else if month < 12 {
        (year, month + 1, 1)
    } else {
        (
            year.checked_add(1)
                .ok_or_else(|| "verify day year overflows".to_owned())?,
            1,
            1,
        )
    };
    Ok(format!("{next_year:04}-{next_month:02}-{next_date:02}"))
}

fn sign_checkpoint(
    mut checkpoint: PartitionCheckpoint,
    args: &CliArgs,
) -> Result<PartitionCheckpoint, String> {
    checkpoint.checkpoint_mac = checkpoint_mac(&checkpoint, &args.keyring)?;
    Ok(checkpoint)
}

fn checkpoint_after(
    lines: &[SealedArchiveLine],
    prior: Option<&PartitionCheckpoint>,
    args: &CliArgs,
) -> Result<PartitionCheckpoint, String> {
    let last = lines
        .last()
        .ok_or_else(|| "cannot checkpoint an empty partition".to_owned())?;
    let day = args
        .verify_day
        .as_deref()
        .ok_or_else(|| "checkpoint output requires verify day".to_owned())?;
    let next_sequence = last
        .sequence_number
        .checked_add(1)
        .ok_or_else(|| "checkpoint sequence overflows u64".to_owned())?;
    sign_checkpoint(
        PartitionCheckpoint {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            tenant_id: last.tenant_id.clone(),
            region: last.region.clone(),
            through_day: day.to_owned(),
            last_sequence: last.sequence_number,
            last_chain_hash: last.chain_hash.clone(),
            current_epoch_id: last.epoch_id,
            current_link_key_id: last.link_key_id,
            next_sequence,
            next_prev_hash: last.chain_hash.clone(),
            next_day: next_day(day)?,
            checkpoint_key_id: args
                .checkpoint_key_id
                .ok_or_else(|| "missing checkpoint key id".to_owned())?,
            previous_checkpoint_mac: prior
                .map_or_else(|| "00".repeat(32), |value| value.checkpoint_mac.clone()),
            checkpoint_mac: String::new(),
        },
        args,
    )
}

fn carry_checkpoint(
    prior: &PartitionCheckpoint,
    args: &CliArgs,
) -> Result<PartitionCheckpoint, String> {
    let day = args
        .verify_day
        .as_deref()
        .ok_or_else(|| "checkpoint output requires verify day".to_owned())?;
    sign_checkpoint(
        PartitionCheckpoint {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            tenant_id: prior.tenant_id.clone(),
            region: prior.region.clone(),
            through_day: day.to_owned(),
            last_sequence: prior.last_sequence,
            last_chain_hash: prior.last_chain_hash.clone(),
            current_epoch_id: prior.current_epoch_id,
            current_link_key_id: prior.current_link_key_id,
            next_sequence: prior.next_sequence,
            next_prev_hash: prior.next_prev_hash.clone(),
            next_day: next_day(day)?,
            checkpoint_key_id: args
                .checkpoint_key_id
                .ok_or_else(|| "missing checkpoint key id".to_owned())?,
            previous_checkpoint_mac: prior.checkpoint_mac.clone(),
            checkpoint_mac: String::new(),
        },
        args,
    )
}

fn write_checkpoints_atomic<'a>(
    path: &Path,
    checkpoints: impl Iterator<Item = &'a PartitionCheckpoint>,
) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "checkpoint output already exists: {}",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "checkpoint output has no parent".to_owned())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("create checkpoint directory: {e}"))?;
    let temp = parent.join(format!(".checkpoint-{}.tmp", uuid::Uuid::now_v7()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("create checkpoint temp: {e}"))?;
        for checkpoint in checkpoints {
            serde_json::to_writer(&mut file, checkpoint)
                .map_err(|e| format!("serialize checkpoint: {e}"))?;
            file.write_all(b"\n")
                .map_err(|e| format!("write checkpoint: {e}"))?;
        }
        file.sync_all()
            .map_err(|e| format!("sync checkpoint: {e}"))?;
        std::fs::hard_link(&temp, path)
            .map_err(|e| format!("publish immutable checkpoint: {e}"))?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("sync checkpoint directory: {e}"))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp);
    result
}

fn verify_sealed_partition(
    lines: &[SealedArchiveLine],
    keyring: &LinkKeyring,
    anchor: &PartitionAnchor,
) -> Result<(), String> {
    let Some(first_line) = lines.first() else {
        return Err("sealed archive chunk is empty".to_owned());
    };
    if !anchor_matches_first(anchor, first_line) {
        return Err(
            "first archive row does not match authenticated checkpoint expectations".to_owned(),
        );
    }

    // A daily window may cross E0->E1 or a later key rotation. Verify every
    // immutable epoch segment separately while retaining continuity at the
    // segment boundary. The former implementation selected an algorithm from
    // the first line and therefore rejected every legitimate mixed-epoch day.
    let mut start = 0_usize;
    let mut prior_epoch: Option<u64> = Some(anchor.current_epoch_id);
    let mut seen_link_key_ids = std::collections::BTreeSet::new();
    if let Some(link_key_id) = anchor.current_link_key_id {
        seen_link_key_ids.insert(link_key_id);
    }
    while start < lines.len() {
        let first = lines
            .get(start)
            .ok_or_else(|| "sealed archive segment disappeared".to_owned())?;
        let identity = (first.algorithm_id, first.epoch_id, first.link_key_id);
        let relative_end = lines.get(start..).and_then(|tail| {
            tail.iter()
                .position(|line| (line.algorithm_id, line.epoch_id, line.link_key_id) != identity)
        });
        let end = relative_end
            .map(|offset| {
                start
                    .checked_add(offset)
                    .ok_or_else(|| "sealed archive segment index overflow".to_owned())
            })
            .transpose()?
            .unwrap_or(lines.len());
        let segment = lines
            .get(start..end)
            .ok_or_else(|| "sealed archive epoch segment is out of bounds".to_owned())?;

        if start > 0 {
            let previous = lines
                .get(start.saturating_sub(1))
                .ok_or_else(|| "sealed archive predecessor disappeared".to_owned())?;
            if previous.sequence_number.checked_add(1) != Some(first.sequence_number)
                || first.prev_hash != previous.chain_hash
            {
                return Err("sealed archive epoch boundary is not contiguous".to_owned());
            }
        }

        match identity {
            (0, 0, None) => {
                if prior_epoch.is_some_and(|epoch| epoch != 0) || first.schema != SEALED_LINE_SCHEMA
                {
                    return Err(
                        "legacy E0 appears after a keyed epoch or uses v2 schema".to_owned()
                    );
                }
                verify_chunk_for_epoch(segment, &ChainEpoch::legacy(), None)
                    .map_err(|error| error.to_string())?;
            }
            (1, epoch_id, Some(link_key_id)) if epoch_id > 0 && link_key_id > 0 => {
                if first.schema != SEALED_LINE_SCHEMA_V2 {
                    return Err("keyed epoch does not use the v2 archive schema".to_owned());
                }
                if let Some(previous_epoch_id) = prior_epoch {
                    let continuing_checkpoint_epoch = start == 0 && previous_epoch_id == epoch_id;
                    if !continuing_checkpoint_epoch
                        && previous_epoch_id.checked_add(1) != Some(epoch_id)
                    {
                        return Err(
                            "sealed archive epoch sequence is not forward-contiguous".to_owned()
                        );
                    }
                    if !continuing_checkpoint_epoch && !seen_link_key_ids.insert(link_key_id) {
                        return Err("audit link-key rotation reused an earlier key id".to_owned());
                    }
                } else if !seen_link_key_ids.insert(link_key_id) {
                    return Err("audit link-key rotation reused an earlier key id".to_owned());
                }
                let key = keyring.get(link_key_id).ok_or_else(|| {
                    format!("historical link-key id {link_key_id} is not present in keyring")
                })?;
                let epoch = ChainEpoch::keyed_successor(
                    epoch_id,
                    link_key_id,
                    first.sequence_number,
                    parse_chain_hash(&first.prev_hash)?,
                    epoch_id.saturating_sub(1),
                )
                .map_err(|error| format!("invalid keyed archive epoch: {error}"))?;
                verify_chunk_for_epoch(segment, &epoch, Some(key))
                    .map_err(|error| error.to_string())?;
            }
            _ => return Err("unsupported or malformed keyed archive metadata".to_owned()),
        }
        prior_epoch = Some(first.epoch_id);
        start = end;
    }
    Ok(())
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

    fn line(
        epoch: &ChainEpoch,
        key: Option<&corelink_audit_chain::LinkKey>,
        sequence: u64,
        previous: ChainHash,
    ) -> SealedArchiveLine {
        let canonical_jcs = format!(r#"{{"sequence":{sequence}}}"#);
        let chain_hash =
            corelink_audit_chain::link_for_epoch(epoch, &previous, canonical_jcs.as_bytes(), key)
                .expect("test epoch link");
        SealedArchiveLine {
            schema: if epoch.epoch_id() == 0 {
                SEALED_LINE_SCHEMA.to_owned()
            } else {
                SEALED_LINE_SCHEMA_V2.to_owned()
            },
            algorithm_id: epoch.algorithm().id(),
            epoch_id: epoch.epoch_id(),
            link_key_id: epoch.link_key_id(),
            tenant_id: "tenant-test".to_owned(),
            region: "weur".to_owned(),
            sequence_number: sequence,
            prev_hash: previous.to_hex(),
            chain_hash: chain_hash.to_hex(),
            enqueued_at_ms: 0,
            row_id: format!("row-{sequence}"),
            canonical_jcs,
        }
    }

    fn genesis_anchor() -> PartitionAnchor {
        PartitionAnchor {
            tenant_id: "tenant-test".to_owned(),
            region: "weur".to_owned(),
            next_sequence: 0,
            next_prev_hash: ChainHash::genesis().to_hex(),
            current_epoch_id: 0,
            current_link_key_id: None,
        }
    }

    fn anchor_from_checkpoint(value: &PartitionCheckpoint) -> PartitionAnchor {
        PartitionAnchor {
            tenant_id: value.tenant_id.clone(),
            region: value.region.clone(),
            next_sequence: value.next_sequence,
            next_prev_hash: value.next_prev_hash.clone(),
            current_epoch_id: value.current_epoch_id,
            current_link_key_id: value.current_link_key_id,
        }
    }

    #[test]
    fn cli_keyring_option_is_fail_closed() -> Result<(), String> {
        assert!(parse_args(&["--chain-keyring".to_owned()]).is_err());
        assert!(parse_args(&["--unknown".to_owned()]).is_err());
        let parsed = parse_args(&["archive.ndjson".to_owned()])?;
        assert!(parsed.keyring.is_empty());
        assert_eq!(parsed.paths, vec!["archive.ndjson"]);
        Ok(())
    }

    #[test]
    fn daily_verifier_accepts_a_contiguous_mixed_epoch_rotation() -> Result<(), String> {
        let keyring_json = format!(r#"{{"7":"{}","8":"{}"}}"#, "ab".repeat(32), "cd".repeat(32));
        let keyring = LinkKeyring::parse_json(&keyring_json).map_err(|e| e.to_string())?;
        let e0 = ChainEpoch::legacy();
        let first = line(&e0, None, 0, ChainHash::genesis());
        let e1 = ChainEpoch::keyed_successor(1, 7, 1, parse_chain_hash(&first.chain_hash)?, 0)
            .map_err(|e| e.to_string())?;
        let second = line(&e1, keyring.get(7), 1, parse_chain_hash(&first.chain_hash)?);
        let e2 = ChainEpoch::keyed_successor(2, 8, 2, parse_chain_hash(&second.chain_hash)?, 1)
            .map_err(|e| e.to_string())?;
        let third = line(
            &e2,
            keyring.get(8),
            2,
            parse_chain_hash(&second.chain_hash)?,
        );
        verify_sealed_partition(&[first, second, third], &keyring, &genesis_anchor())
    }

    #[test]
    fn daily_verifier_fails_closed_on_historic_key_loss_or_rotation_reuse() -> Result<(), String> {
        let complete_json = format!(r#"{{"7":"{}","8":"{}"}}"#, "ab".repeat(32), "cd".repeat(32));
        let complete = LinkKeyring::parse_json(&complete_json).map_err(|e| e.to_string())?;
        let legacy = line(&ChainEpoch::legacy(), None, 0, ChainHash::genesis());
        let e1 = ChainEpoch::keyed_successor(1, 7, 1, parse_chain_hash(&legacy.chain_hash)?, 0)
            .map_err(|e| e.to_string())?;
        let first = line(
            &e1,
            complete.get(7),
            1,
            parse_chain_hash(&legacy.chain_hash)?,
        );
        let missing = LinkKeyring::parse_json(&format!(r#"{{"8":"{}"}}"#, "cd".repeat(32)))
            .map_err(|e| e.to_string())?;
        assert!(verify_sealed_partition(
            &[legacy.clone(), first.clone()],
            &missing,
            &genesis_anchor()
        )
        .is_err_and(|e| e.contains("historical link-key id 7")));

        let e2_reused =
            ChainEpoch::keyed_successor(2, 7, 2, parse_chain_hash(&first.chain_hash)?, 1)
                .map_err(|e| e.to_string())?;
        let second = line(
            &e2_reused,
            complete.get(7),
            2,
            parse_chain_hash(&first.chain_hash)?,
        );
        assert!(
            verify_sealed_partition(&[legacy, first, second], &complete, &genesis_anchor())
                .is_err_and(|e| e.contains("reused an earlier key id"))
        );
        Ok(())
    }

    #[test]
    fn daily_verifier_rejects_unanchored_keyed_first_epoch_and_sequence_overflow(
    ) -> Result<(), String> {
        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "ab".repeat(32)))
            .map_err(|e| e.to_string())?;
        let epoch = ChainEpoch::keyed_successor(1, 7, 4, ChainHash([3; 32]), 0)
            .map_err(|e| e.to_string())?;
        let keyed = line(&epoch, keyring.get(7), 4, ChainHash([3; 32]));
        assert!(
            verify_sealed_partition(&[keyed], &keyring, &genesis_anchor())
                .is_err_and(|e| e.contains("authenticated checkpoint expectations"))
        );

        let legacy = ChainEpoch::legacy();
        let first = line(&legacy, None, u64::MAX, ChainHash([4; 32]));
        let second = line(
            &legacy,
            None,
            u64::MAX,
            parse_chain_hash(&first.chain_hash)?,
        );
        assert!(verify_sealed_partition(
            &[first, second],
            &LinkKeyring::default(),
            &genesis_anchor()
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn daily_verifier_rejects_epoch_gap_and_downgrade() -> Result<(), String> {
        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "ab".repeat(32)))
            .map_err(|e| e.to_string())?;
        let e0 = ChainEpoch::legacy();
        let first = line(&e0, None, 0, ChainHash::genesis());
        let e2 = ChainEpoch::keyed_successor(2, 7, 1, parse_chain_hash(&first.chain_hash)?, 1)
            .map_err(|e| e.to_string())?;
        let gap = line(&e2, keyring.get(7), 1, parse_chain_hash(&first.chain_hash)?);
        assert!(
            verify_sealed_partition(&[first.clone(), gap], &keyring, &genesis_anchor())
                .is_err_and(|e| e.contains("epoch sequence"))
        );

        let mut downgraded = first.clone();
        downgraded.sequence_number = 2;
        downgraded.prev_hash = first.chain_hash.clone();
        assert!(
            verify_sealed_partition(&[first, downgraded], &keyring, &genesis_anchor()).is_err()
        );
        Ok(())
    }

    fn checkpoint_fixture(keyring: &LinkKeyring) -> Result<PartitionCheckpoint, String> {
        let mut checkpoint = PartitionCheckpoint {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            tenant_id: "tenant-test".to_owned(),
            region: "weur".to_owned(),
            through_day: "2026-09-08".to_owned(),
            last_sequence: 3,
            last_chain_hash: "03".repeat(32),
            current_epoch_id: 1,
            current_link_key_id: Some(7),
            next_sequence: 4,
            next_prev_hash: "03".repeat(32),
            next_day: "2026-09-09".to_owned(),
            checkpoint_key_id: 7,
            previous_checkpoint_mac: "00".repeat(32),
            checkpoint_mac: String::new(),
        };
        checkpoint.checkpoint_mac = checkpoint_mac(&checkpoint, keyring)?;
        Ok(checkpoint)
    }

    #[test]
    fn authenticated_checkpoint_rejects_tamper_replay_wrong_key_and_partition() -> Result<(), String>
    {
        let keyring = LinkKeyring::parse_json(&format!(
            r#"{{"7":"{}","8":"{}"}}"#,
            "ab".repeat(32),
            "cd".repeat(32)
        ))
        .map_err(|e| e.to_string())?;
        let checkpoint = checkpoint_fixture(&keyring)?;
        validate_checkpoint(&checkpoint, "2026-09-09", &keyring)?;

        let mut tampered = checkpoint.clone();
        tampered.next_sequence = 5;
        assert!(validate_checkpoint(&tampered, "2026-09-09", &keyring).is_err());
        assert!(validate_checkpoint(&checkpoint, "2026-09-10", &keyring).is_err());
        let mut wrong_key = checkpoint.clone();
        wrong_key.checkpoint_key_id = 8;
        assert!(validate_checkpoint(&wrong_key, "2026-09-09", &keyring).is_err());

        let epoch = ChainEpoch::keyed_successor(1, 7, 4, ChainHash([3; 32]), 0)
            .map_err(|e| e.to_string())?;
        let mut first = line(&epoch, keyring.get(7), 4, ChainHash([3; 32]));
        first.region = "apac".to_owned();
        assert!(
            verify_sealed_partition(&[first], &keyring, &anchor_from_checkpoint(&checkpoint))
                .is_err_and(|e| e.contains("checkpoint expectations"))
        );
        Ok(())
    }

    #[test]
    fn checkpoint_publish_is_atomic_create_only_and_keyed_first_requires_input(
    ) -> Result<(), String> {
        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "ab".repeat(32)))
            .map_err(|e| e.to_string())?;
        let checkpoint = checkpoint_fixture(&keyring)?;
        let carry_args = CliArgs {
            keyring: keyring.clone(),
            verify_day: Some("2026-09-09".to_owned()),
            checkpoint_key_id: Some(7),
            checkpoint_out: Some(PathBuf::from("unused")),
            ..CliArgs::default()
        };
        let carried = carry_checkpoint(&checkpoint, &carry_args)?;
        assert_eq!(carried.last_sequence, checkpoint.last_sequence);
        assert_eq!(carried.previous_checkpoint_mac, checkpoint.checkpoint_mac);
        validate_checkpoint(&carried, "2026-09-10", &keyring)?;
        let dir =
            std::env::temp_dir().join(format!("corelink-checkpoint-test-{}", uuid::Uuid::now_v7()));
        let path = dir.join("checkpoint.ndjson");
        write_checkpoints_atomic(&path, std::iter::once(&checkpoint))?;
        assert!(write_checkpoints_atomic(&path, std::iter::once(&checkpoint)).is_err());
        let written = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(written.ends_with('\n'));
        let empty_path = dir.join("empty.ndjson");
        write_checkpoints_atomic(&empty_path, std::iter::empty::<&PartitionCheckpoint>())?;
        assert_eq!(
            std::fs::metadata(&empty_path)
                .map_err(|e| e.to_string())?
                .len(),
            0
        );
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;

        let epoch = ChainEpoch::keyed_successor(1, 7, 4, ChainHash([3; 32]), 0)
            .map_err(|e| e.to_string())?;
        let keyed = line(&epoch, keyring.get(7), 4, ChainHash([3; 32]));
        assert!(verify_sealed_partition(&[keyed], &keyring, &genesis_anchor()).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_rows_and_cross_day_or_wrong_object_copies_are_rejected() -> Result<(), String> {
        let first = line(&ChainEpoch::legacy(), None, 0, ChainHash::genesis());
        let mut conflicting = first.clone();
        conflicting.row_id = "conflicting-row".to_owned();
        conflicting.chain_hash = "ff".repeat(32);
        assert!(reject_duplicate_lines(&[first.clone(), conflicting])
            .is_err_and(|e| e.contains("duplicate sequence")));

        let mut repeated_row = line(
            &ChainEpoch::legacy(),
            None,
            1,
            parse_chain_hash(&first.chain_hash)?,
        );
        repeated_row.row_id = first.row_id.clone();
        assert!(reject_duplicate_lines(&[first.clone(), repeated_row])
            .is_err_and(|e| e.contains("duplicate row")));

        let correct = format!("staging/chunks/{}", sealed_chunk_key(&first));
        verify_archive_object_binding(
            &correct,
            std::slice::from_ref(&first),
            "1970-01-01",
            b"legacy-object",
        )?;
        assert!(verify_archive_object_binding(
            &correct,
            std::slice::from_ref(&first),
            "1970-01-02",
            b"legacy-object",
        )
        .is_err_and(|e| e.contains("cross-day")));
        assert!(verify_archive_object_binding(
            "staging/chunks/audit/1970/01/02/copied.ndjson",
            std::slice::from_ref(&first),
            "1970-01-01",
            b"legacy-object",
        )
        .is_err_and(|e| e.contains("authenticated row key")));

        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "ab".repeat(32)))
            .map_err(|e| e.to_string())?;
        let epoch = ChainEpoch::keyed_successor(1, 7, 0, ChainHash::genesis(), 0)
            .map_err(|e| e.to_string())?;
        let keyed = line(&epoch, keyring.get(7), 0, ChainHash::genesis());
        let keyed_bytes = b"exact-keyed-object-bytes";
        let keyed_path = format!(
            "staging/chunks/{}.epoch-1.{}",
            sealed_chunk_key(&keyed),
            domain_hash(&[], keyed_bytes)
        );
        verify_archive_object_binding(
            &keyed_path,
            std::slice::from_ref(&keyed),
            "1970-01-01",
            keyed_bytes,
        )?;
        assert!(verify_archive_object_binding(
            &format!("{}.epoch-1.{}", sealed_chunk_key(&keyed), "00".repeat(32)),
            &[keyed],
            "1970-01-01",
            keyed_bytes,
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn signed_witness_receipt_bootstrap_rejects_tamper_and_ahead_or_behind() -> Result<(), String> {
        use ed25519_dalek::{Signer, SigningKey};

        let signing = SigningKey::from_bytes(&[9_u8; 32]);
        let head = WitnessHeadV2 {
            epoch_id: 1,
            epoch_ledger_hash: "05".repeat(32),
            epoch_ledger_sequence: 1,
            head_hash: "03".repeat(32),
            head_message_version: 2,
            next_sequence: 4,
            region: "weur".to_owned(),
            signing_key_id: 9,
            tenant_id: "tenant-test".to_owned(),
        };
        let head_jcs = serde_jcs::to_vec(&head).map_err(|e| e.to_string())?;
        let head_signature = [8_u8; 64];
        let mut head_material = Vec::new();
        head_material.extend_from_slice(HEAD_RECORD_DOMAIN);
        head_material.extend_from_slice(&(head_jcs.len() as u64).to_be_bytes());
        head_material.extend_from_slice(&head_jcs);
        head_material.extend_from_slice(&head_signature);
        let record = WitnessRecord {
            head_message_b64: base64::engine::general_purpose::STANDARD.encode(&head_jcs),
            head_record_hash: domain_hash(&[], &head_material),
            head_signature_b64: base64::engine::general_purpose::STANDARD.encode(head_signature),
            previous_witness_hash: "00".repeat(32),
            region: "weur".to_owned(),
            tenant_id: "tenant-test".to_owned(),
            witness_sequence: 3,
            witness_version: 1,
        };
        let witness_jcs = serde_jcs::to_vec(&record).map_err(|e| e.to_string())?;
        let witness_record_hash = domain_hash(WITNESS_DOMAIN, &witness_jcs);
        let receipt = WitnessReceiptJcs {
            committed_at_ms: 1,
            head_record_hash: record.head_record_hash.clone(),
            previous_witness_hash: record.previous_witness_hash.clone(),
            receipt_version: 1,
            region: "weur".to_owned(),
            tenant_id: "tenant-test".to_owned(),
            witness_id: "security-witness".to_owned(),
            witness_key_id: 7,
            witness_record_hash: witness_record_hash.clone(),
            witness_sequence: 3,
        };
        let receipt_jcs = serde_jcs::to_vec(&receipt).map_err(|e| e.to_string())?;
        let mut signed = Vec::new();
        signed.extend_from_slice(RECEIPT_DOMAIN);
        signed.extend_from_slice(&(receipt_jcs.len() as u64).to_be_bytes());
        signed.extend_from_slice(&receipt_jcs);
        let stored = StoredWitnessReceipt {
            receipt_jcs_b64: base64::engine::general_purpose::STANDARD.encode(&receipt_jcs),
            receipt_signature_b64: base64::engine::general_purpose::STANDARD
                .encode(signing.sign(&signed).to_bytes()),
            witness_jcs_b64: base64::engine::general_purpose::STANDARD.encode(&witness_jcs),
            witness_key_id: 7,
            witness_record_hash,
            witness_sequence: 3,
            tenant_id: "tenant-test".to_owned(),
            region: "weur".to_owned(),
        };
        let dir =
            std::env::temp_dir().join(format!("corelink-bootstrap-test-{}", uuid::Uuid::now_v7()));
        let receipts = dir.join("receipts");
        std::fs::create_dir_all(&receipts).map_err(|e| e.to_string())?;
        let receipt_path = receipts.join("receipt.json");
        std::fs::write(
            &receipt_path,
            serde_jcs::to_vec(&stored).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let keys_path = dir.join("keys.json");
        std::fs::write(
            &keys_path,
            format!(
                r#"{{"7":"{}"}}"#,
                base64::engine::general_purpose::STANDARD
                    .encode(signing.verifying_key().to_bytes())
            ),
        )
        .map_err(|e| e.to_string())?;
        let args = CliArgs {
            witness_id: Some("security-witness".to_owned()),
            witness_public_keys: Some(keys_path),
            witness_receipts_dir: Some(receipts),
            ..CliArgs::default()
        };
        let anchors = load_bootstrap_anchors(&args)?;
        let anchor = &anchors
            .get(&("tenant-test".to_owned(), "weur".to_owned()))
            .ok_or("anchor absent")?[0];
        let mut first = line(&ChainEpoch::legacy(), None, 4, ChainHash([3; 32]));
        first.schema = SEALED_LINE_SCHEMA_V2.to_owned();
        first.algorithm_id = 1;
        first.epoch_id = 1;
        first.link_key_id = Some(7);
        assert!(anchor_matches_first(anchor, &first));
        first.sequence_number = 3;
        assert!(!anchor_matches_first(anchor, &first));
        first.sequence_number = 5;
        assert!(!anchor_matches_first(anchor, &first));
        let mut tampered = std::fs::read(&receipt_path).map_err(|e| e.to_string())?;
        let last = tampered.last_mut().ok_or("empty receipt")?;
        *last ^= 1;
        std::fs::write(&receipt_path, tampered).map_err(|e| e.to_string())?;
        assert!(load_bootstrap_anchors(&args).is_err());
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(())
    }
}
