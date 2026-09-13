//! Offsite archive of the sealed audit chain — `POST /_internal/audit/archive`.
//!
//! ## What this closes
//!
//! `POST /_internal/audit/drain` seals `audit_outbox` rows into the BLAKE3
//! chain, but the seal lands in D1 and D1 is mutable. S-09 calls for an
//! immutable offsite copy: NDJSON chunks in an R2 bucket under a 7-year Object
//! Lock. The producer for that copy existed in source
//! (`corelink_audit_chain::archive_producer`) and the daily verifier cron
//! existed in CI, but nothing ever connected them — the producer's only caller
//! sat behind a Cargo feature no build passes, in a crate the live Worker never
//! imports. This route is the missing middle.
//!
//! ## Why it is a SEPARATE endpoint, not a write inside the drain
//!
//! The drain's job is to make the trail tamper-evident, and it must not acquire
//! a new way to fail. If the archive write lived inside the drain transaction,
//! an R2 outage would either abort the seal (leaving rows PLAIN and mutable in
//! the pre-seal window — strictly worse than no archive) or be swallowed
//! (silently producing an incomplete archive). Splitting them means R2 can be
//! down for a week and the chain still seals on schedule; the archive simply
//! catches up. It also means the archiver can walk BACKWARD over rows sealed
//! long before it existed, which is how the pre-existing sealed backlog gets
//! archived at all.
//!
//! ## Ordering — the only ordering that is safe
//!
//! For each chunk: **write R2 first, mark `archived_at` second.** A crash
//! between the two re-writes byte-identical bytes on the next run (the chunk is
//! a pure function of its rows), which the create-if-absent path treats as
//! success. The reverse order would mark rows archived that were never written,
//! and nothing would ever revisit them.
//!
//! ## Idempotency under Object Lock
//!
//! `corelink-audit-weur` carries a bucket-wide 7-year retention rule, so an
//! existing object CANNOT be overwritten. The writer therefore does a
//! conditional `PutObject` with `If-None-Match: *`; a `412` means the key is
//! already there, and the writer then GETs it and compares bytes:
//!
//! - identical  → SUCCESS (this chunk was already archived; mark the rows)
//! - different  → parse and verify the existing object. A valid semantic
//!   PREFIX is a recoverable R2-first/D1-bookkeeping crash window: mark only
//!   the rows represented by that immutable object, then continue under the
//!   next sequence key. Any non-prefix or unverifiable object is a HARD
//!   FAILURE; rows are NOT marked. Never "resolve" that case by overwriting or
//!   by skipping.
//!
//! ## Fail loud, never fail open
//!
//! Every error arm leaves the rows unarchived and surfaces a non-2xx, so the
//! backlog and the cron's own logs both show the gap. Silently reporting
//! success on an unwritten chunk is the failure mode this whole item exists to
//! remove.
//!
//! ## Clean prefix + quarantine — partial progress over a forked partition
//!
//! Refusing a chunk that does not verify is correct. Refusing a whole PARTITION
//! forever was not. A 17-minute seal fork on 2026-08-14 left 8 of 360
//! partitions with duplicated `sequence_number`s (each duplicate pair carrying
//! a DIFFERENT `prev_hash` — two branches) and the following sequence missing,
//! so those partitions could never satisfy the verifier and retried on every
//! hourly tick, invisibly, forever.
//!
//! Per partition the archiver therefore:
//!
//! 1. takes the LONGEST CONTIGUOUS VERIFYING PREFIX
//!    ([`corelink_audit_chain::split_verifying_prefix_for_epoch`]) of the sealed,
//!    unarchived, not-yet-quarantined rows and archives it exactly as before —
//!    R2 first, `archived_at` second;
//! 2. QUARANTINES the rest of the partition's currently-sealed set: sets
//!    `quarantined_at` + a machine-readable `quarantine_reason` naming the
//!    break (`sequence_gap:expected=11,found=10`), which takes those rows out
//!    of the work queue;
//! 3. resumes on later rows as a NEW chunk window — legal because
//!    [`corelink_audit_chain::verify_chunk_for_epoch`] checks a window's first row
//!    against its OWN link only, never against a predecessor it does not hold.
//!
//! **Re-sequencing is FORBIDDEN.** The sealed rows are the evidence. Nothing
//! here ever UPDATEs `sequence_number`, `prev_hash`, `chain_hash`,
//! `canonical_jcs`, `emitted_at` or `chained_at`; the only columns this module
//! writes are `archived_at`, `quarantined_at` and `quarantine_reason`, and a
//! test in this file re-reads its own source to prove it.
//!
//! **Quarantine is LOUD, not a drawer.** The response body carries
//! `rows_quarantined` / `partitions_quarantined`, the cron logs them, the
//! absence monitor excludes quarantined rows from its pending clause so it
//! neither pages forever nor goes quiet, and
//! `specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md` says how to list them. A row
//! that is silently unarchivable forever is the defect class this endpoint was
//! built to remove.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine as _;
use ed25519_dalek::Signature;
#[cfg(test)]
use ed25519_dalek::Verifier as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use corelink_audit_chain::{
    key_matches_commitment, sealed_chunk_key, serialize_chunk_for_epoch, split_into_chunks,
    split_verifying_prefix_for_epoch, ChainEpoch, ChainHash, LinkKey, LinkKeyring,
    SealedArchiveLine, DEFAULT_SEALED_MAX_BYTES_PER_CHUNK, DEFAULT_SEALED_MAX_LINES_PER_CHUNK,
    SEALED_LINE_SCHEMA, SEALED_LINE_SCHEMA_V2,
};

use crate::routes::audit_drain::{
    b054_archive_authenticate_link_registry, b054_archive_authenticate_signing_registry,
    b054_archive_load_manifest_signer, b054_archive_parse_trust_roots, b054_load_witness_client,
    B054ArchiveManifestSigner, B054ArchiveSigningKey, B054ArchiveTrustRoots,
    B054StoredWitnessReceiptInput, B054WitnessAnchor, WitnessClient,
};
use crate::storage::d1_http::{D1BatchStatement, D1HttpClient, D1Row};
use crate::storage::r2_s3::{CappedGet, R2S3Client};

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";
const ARCHIVE_MANIFEST_DOMAIN: &[u8] = b"corelink/audit-chain/archive-manifest/v1\0";
const ARCHIVE_LEDGER_DOMAIN: &[u8] = b"corelink/audit-chain/epoch-ledger/v1\0";
const MAX_ARCHIVE_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_ARCHIVE_MANIFEST_ENVELOPE_BYTES: usize = 512 * 1024;
const JS_SAFE_MAX: u64 = 9_007_199_254_740_991;

/// Decode the three nullable B-054 columns without turning malformed metadata
/// into E0. Only the complete legacy spelling, explicit E0, or a complete
/// positive keyed tuple is admissible; every partial/downgrade shape stops the
/// archive before it can manufacture an E0 line.
fn parse_sealed_epoch_metadata(row: &Value) -> Result<(u8, u64, Option<u64>), String> {
    fn nullable_i64(row: &Value, field: &str) -> Result<Option<i64>, String> {
        match row.get(field) {
            Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| format!("audit_outbox.{field} missing/non-integer")),
            None => Err(format!("audit_outbox.{field} missing")),
        }
    }

    let algorithm = nullable_i64(row, "algorithm_id")?;
    let epoch = nullable_i64(row, "epoch_id")?;
    let key = nullable_i64(row, "link_key_id")?;
    match (algorithm, epoch, key) {
        (None, None, None) | (Some(0), Some(0), None) => Ok((0, 0, None)),
        (Some(1), Some(epoch), Some(key)) if epoch > 0 && key > 0 => {
            Ok((1, epoch as u64, Some(key as u64)))
        }
        _ => Err(
            "audit_outbox sealed epoch metadata is partial, negative, or a downgrade".to_owned(),
        ),
    }
}

/// Bucket the archive lands in when `R2_AUDIT_BUCKET` is unset.
///
/// This is a REAL, provisioned bucket that already carries the 7-year Object
/// Lock rule (`corelink-audit-7y-retention`, `maxAgeSeconds=220924800`) and
/// already holds the erasure attestations. The default deliberately does NOT
/// name a bucket that has to be created first — an archive whose default target
/// does not exist is how this control stayed dead for a year.
pub const DEFAULT_AUDIT_BUCKET: &str = "corelink-audit-weur";

/// Shared state for the archive route.
#[derive(Clone)]
pub struct AuditArchiveState {
    internal_auth_key: String,
    d1: Arc<D1HttpClient>,
    r2: Arc<R2S3Client>,
    bucket: String,
    /// Rows read per partition per call. Bounds one call's work so a cold
    /// backlog drains over repeated calls instead of exceeding the edge
    /// subrequest timeout. A truncated call returns `incomplete: true`.
    batch_limit: i64,
    max_lines_per_chunk: usize,
    max_bytes_per_chunk: usize,
    /// Forwarded write-only keyring held in zeroizing memory. Keyed archive
    /// verification consumes it only after authenticated ledger, witness and
    /// manifest/object checks complete.
    link_keyring: Option<Arc<LinkKeyring>>,
    archive_trust_roots: Option<Arc<B054ArchiveTrustRoots>>,
    archive_signer: Option<Arc<B054ArchiveManifestSigner>>,
    witness: Option<Arc<WitnessClient>>,
}

impl std::fmt::Debug for AuditArchiveState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditArchiveState")
            .field("internal_auth_key", &"<redacted>")
            .field("d1", &"[D1HttpClient]")
            .field("bucket", &self.bucket)
            .field("batch_limit", &self.batch_limit)
            .field("max_lines_per_chunk", &self.max_lines_per_chunk)
            .field("max_bytes_per_chunk", &self.max_bytes_per_chunk)
            .field(
                "link_keyring",
                &self.link_keyring.as_ref().map(|keyring| keyring.len()),
            )
            .field("archive_trust_roots", &self.archive_trust_roots.is_some())
            .field("archive_signer", &self.archive_signer)
            .field("witness", &self.witness)
            .finish()
    }
}

/// Build the route state from the environment, mirroring
/// [`crate::routes::audit_drain::build_state_from_env`]: the DEDICATED
/// `CORELINK_ERASE_AUTH_KEY` only (no shared fallback), ≥32 chars, plus D1 and
/// R2 from [`crate::storage::StorageEnv`]. Any missing piece leaves the route
/// UNMOUNTED (fail-CLOSED) rather than mounted-and-broken.
pub async fn build_state_from_env() -> Option<AuditArchiveState> {
    let internal_auth_key = match crate::routes::admin::erase_auth_key_from_env() {
        Some(k) if k.len() >= 32 => k.to_string(),
        _ => {
            tracing::warn!(
                "no usable CORELINK_ERASE_AUTH_KEY (dedicated; NO shared fallback) \
                 (< 32 chars); /_internal/audit/archive NOT mounted (fail-CLOSED)"
            );
            return None;
        }
    };
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&storage_env).ok()?);
    let bucket = std::env::var("R2_AUDIT_BUCKET")
        .ok()
        .map(|s| s.trim().to_owned())
        // The DO forward-list sends every key as `this.env.X ?? ""`, so an unset
        // var arrives as `Some("")` — treat empty as absent or the client would
        // be built against a nameless bucket.
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_AUDIT_BUCKET.to_owned());
    let r2 = match R2S3Client::new(&storage_env, bucket.clone()).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::warn!(
                error = %e,
                bucket = %bucket,
                "audit/archive: R2 client build failed; route NOT mounted (fail-CLOSED)"
            );
            return None;
        }
    };
    let link_keyring = match std::env::var("AUDIT_CHAIN_LINK_KEYS_JSON") {
        Ok(raw) if !raw.trim().is_empty() => {
            let raw = Zeroizing::new(raw);
            match LinkKeyring::parse_json(&raw) {
                Ok(keyring) => Some(Arc::new(keyring)),
                Err(error) => {
                    tracing::warn!(error = %error, "audit/archive: malformed link keyring; route NOT mounted (fail-CLOSED)");
                    return None;
                }
            }
        }
        _ => None,
    };
    let batch_limit = std::env::var("AUDIT_ARCHIVE_BATCH_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(2000);
    let archive_trust_roots = match std::env::var("AUDIT_CHAIN_TRUST_ROOT_PUBLIC_KEYS_JSON") {
        Ok(raw) if !raw.trim().is_empty() => match b054_archive_parse_trust_roots(&raw) {
            Ok(roots) => Some(Arc::new(roots)),
            Err(error) => {
                tracing::warn!(error = %error, "audit/archive: malformed OOB trust roots; route NOT mounted");
                return None;
            }
        },
        _ => None,
    };
    let archive_signer = match b054_archive_load_manifest_signer() {
        Ok(signer) => signer,
        Err(error) => {
            tracing::warn!(error = %error, "audit/archive: invalid manifest signer; route NOT mounted");
            return None;
        }
    };
    let witness = match b054_load_witness_client() {
        Ok(witness) => witness,
        Err(error) => {
            tracing::warn!(error = %error, "audit/archive: invalid witness configuration; route NOT mounted");
            return None;
        }
    };
    Some(AuditArchiveState {
        internal_auth_key,
        d1,
        r2,
        bucket,
        batch_limit,
        max_lines_per_chunk: DEFAULT_SEALED_MAX_LINES_PER_CHUNK,
        max_bytes_per_chunk: DEFAULT_SEALED_MAX_BYTES_PER_CHUNK,
        link_keyring,
        archive_trust_roots,
        archive_signer,
        witness,
    })
}

/// Mount `POST /_internal/audit/archive`.
pub fn router(state: AuditArchiveState) -> Router {
    Router::new()
        .route("/_internal/audit/archive", post(handle_archive))
        .with_state(state)
}

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(i64::MAX)
}

/// Constant-time internal-auth check — mirrors
/// [`crate::routes::audit_drain`] byte-for-byte so the internal-auth surface
/// has ONE behaviour, not one per module.
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let Some(provided) = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let provided = provided.as_bytes();
    if provided.len() != expected.len() {
        return false;
    }
    provided.ct_eq(expected).into()
}

/// What happened to one chunk's R2 write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkWrite {
    /// The object did not exist and was created by this call.
    Created,
    /// The object already existed with byte-identical content — a prior run (or
    /// a concurrent one) wrote it. Idempotent success.
    AlreadyIdentical,
    /// The immutable object is a valid semantic prefix of the current chunk.
    /// This happens when a prior run completed R2 PUT but crashed before D1
    /// bookkeeping, and newer rows later joined the same day/key window.
    ExistingPrefix(usize),
}

/// Read an immutable archive object and return its semantic lines.
///
/// Archive objects are NDJSON emitted by `serialize_chunk`.  Parse and verify
/// the object before using it to advance D1: a malformed or unverifiable
/// pre-existing object is a hard conflict, never evidence that rows may be
/// marked archived.
fn parse_existing_chunk(
    bytes: &[u8],
    epoch: &ChainEpoch,
    link_key: Option<&LinkKey>,
) -> Result<Vec<SealedArchiveLine>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| format!("existing archive object is not UTF-8: {e}"))?;
    if text.is_empty() {
        return Err("existing archive object is empty".to_owned());
    }
    // Historical writers may have emitted the conventional single NDJSON
    // terminator. Accept exactly one trailing newline; a second trailing
    // newline or any empty line in the middle remains malformed evidence.
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        return Err("existing archive object has no lines".to_owned());
    }
    let lines = text
        .split('\n')
        .map(|line| {
            serde_json::from_str::<SealedArchiveLine>(line)
                .map_err(|e| format!("existing archive object has invalid NDJSON: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if lines.is_empty() {
        return Err("existing archive object has no lines".to_owned());
    }
    corelink_audit_chain::verify_chunk_for_epoch(&lines, epoch, link_key)
        .map_err(|e| format!("existing archive object does not verify: {e}"))?;
    Ok(lines)
}

/// Return the number of candidate lines represented by an existing immutable
/// object, when that object is a non-empty semantic prefix of the candidate.
fn existing_prefix_len(
    existing: &[SealedArchiveLine],
    candidate: &[SealedArchiveLine],
) -> Option<usize> {
    (!existing.is_empty()
        && existing.len() <= candidate.len()
        && candidate.get(..existing.len()) == Some(existing))
    .then_some(existing.len())
}

/// Classify bytes already present under an immutable archive key.
///
/// Keep this independent from the conditional PUT: an Object Lock gateway may
/// reject a write to an existing WORM key without preserving a classifiable
/// S3 `412` in the SDK error. Reading the known key first lets idempotent and
/// prefix recovery proceed without attempting an impossible write. The absent
/// path remains race-safe because it still uses `If-None-Match: *`.
fn classify_existing_chunk(
    key: &str,
    existing: &[u8],
    candidate: &[u8],
    epoch: &ChainEpoch,
    link_key: Option<&LinkKey>,
) -> Result<ChunkWrite, String> {
    if existing == candidate {
        return Ok(ChunkWrite::AlreadyIdentical);
    }
    let existing_lines = parse_existing_chunk(existing, epoch, link_key).map_err(|reason| {
        format!(
            "AUDIT_ARCHIVE_KEY_CONFLICT key={key} existing_bytes={} new_bytes={} ({reason}) — \
             refusing to overwrite an immutable archive object",
            existing.len(),
            candidate.len()
        )
    })?;
    let candidate_lines = parse_existing_chunk(candidate, epoch, link_key).map_err(|reason| {
        format!("archive candidate for key={key} failed self-parse ({reason})")
    })?;
    let Some(prefix_len) = existing_prefix_len(&existing_lines, &candidate_lines) else {
        return Err(format!(
            "AUDIT_ARCHIVE_KEY_CONFLICT key={key} existing_bytes={} new_bytes={} — \
             existing object is not the current candidate's semantic prefix; \
             refusing to overwrite an immutable archive object",
            existing.len(),
            candidate.len()
        ));
    };
    Ok(ChunkWrite::ExistingPrefix(prefix_len))
}

/// Bound the read-before-PUT probe to one candidate object plus the one
/// conventional NDJSON terminator accepted by `parse_existing_chunk`.
fn max_existing_chunk_bytes(candidate: &[u8]) -> u64 {
    u64::try_from(candidate.len())
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}

/// Every partition that still owns sealed-but-unarchived rows.
async fn read_unarchived_partitions(d1: &D1HttpClient) -> Result<Vec<(String, String)>, String> {
    let rows = d1
        .query(
            // The quarantine predicate matches the partial work-queue index
            // added by migration 0100: a partition whose only remaining rows are
            // quarantined has no work left and must drop out of the sweep
            // entirely, or the archiver walks a permanently-refused tail on
            // every hourly tick.
            "SELECT DISTINCT tenant_id, region FROM audit_outbox \
             WHERE emitted_at IS NOT NULL AND archived_at IS NULL \
               AND quarantined_at IS NULL \
               AND sequence_number IS NOT NULL AND canonical_jcs IS NOT NULL",
            &[],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let tenant_id = row
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.tenant_id missing/non-text")?
            .to_owned();
        let region = row
            .get("region")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.region missing/non-text")?
            .to_owned();
        out.push((tenant_id, region));
    }
    Ok(out)
}

/// Read up to `limit` sealed-but-unarchived rows of one partition, in chain
/// order.
///
/// `ORDER BY sequence_number` over the unarchived, un-quarantined set yields
/// the ordered PREFIX of what is left, so the next call resumes exactly where
/// this one stopped.
///
/// It does NOT guarantee the batch is chain-contiguous — that was the wrong
/// assumption. The 2026-08-14 seal fork left duplicated sequence numbers with
/// the following sequence missing, so the ORDERED batch can still contain a
/// break. [`split_verifying_prefix`] is what decides how much of it is
/// archivable; this function only decides what to look at.
async fn read_unarchived_rows(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    limit: i64,
) -> Result<Vec<SealedArchiveLine>, String> {
    let rows = d1
        .query(
            "SELECT id, sequence_number, prev_hash, chain_hash, enqueued_at, canonical_jcs, \
                    algorithm_id, epoch_id, link_key_id \
             FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND archived_at IS NULL \
               AND quarantined_at IS NULL \
               AND sequence_number IS NOT NULL AND canonical_jcs IS NOT NULL \
             ORDER BY sequence_number \
             LIMIT ?3",
            &[json!(tenant_id), json!(region), json!(limit)],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // D1 returns object maps; normalize once so the shared metadata parser
        // keeps its Value-based test seam and fail-closed behavior.
        let row = Value::Object(row);
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.id missing/non-text")?;
        let sequence_number = row
            .get("sequence_number")
            .and_then(Value::as_i64)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or("audit_outbox.sequence_number negative/non-integer on a sealed row")?;
        let prev_hash = row
            .get("prev_hash")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.prev_hash missing on a sealed row")?;
        let chain_hash = row
            .get("chain_hash")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.chain_hash missing on a sealed row")?;
        let enqueued_at = row
            .get("enqueued_at")
            .and_then(Value::as_i64)
            .ok_or("audit_outbox.enqueued_at missing/non-integer")?;
        let canonical_jcs = row
            .get("canonical_jcs")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.canonical_jcs missing on a sealed row")?;
        let (algorithm_id, epoch_id, link_key_id) = parse_sealed_epoch_metadata(&row)?;
        out.push(SealedArchiveLine {
            schema: if algorithm_id == 0 {
                SEALED_LINE_SCHEMA
            } else {
                SEALED_LINE_SCHEMA_V2
            }
            .to_owned(),
            algorithm_id,
            epoch_id,
            link_key_id,
            tenant_id: tenant_id.to_owned(),
            region: region.to_owned(),
            sequence_number,
            prev_hash: prev_hash.to_owned(),
            chain_hash: chain_hash.to_owned(),
            enqueued_at_ms: enqueued_at,
            row_id: id.to_owned(),
            canonical_jcs: canonical_jcs.to_owned(),
        });
    }
    Ok(out)
}

/// Write one chunk create-if-absent. See the module docs for why a differing
/// pre-existing object is a hard failure and not an overwrite.
async fn put_chunk_if_absent(
    r2: &R2S3Client,
    key: &str,
    bytes: &[u8],
    epoch: &ChainEpoch,
    link_key: Option<&LinkKey>,
) -> Result<ChunkWrite, String> {
    // The common R2-first/D1-second recovery case already has a durable WORM
    // object. Do not PUT at that key merely to discover it: inspect and verify
    // it first. A concurrent creator after a 404 is still handled by the
    // conditional PUT loser arm below.
    let max_existing_bytes = max_existing_chunk_bytes(bytes);
    match r2.get_capped(key, max_existing_bytes).await? {
        CappedGet::Found(existing) => {
            return classify_existing_chunk(key, &existing, bytes, epoch, link_key);
        }
        CappedGet::Missing => {}
        CappedGet::TooLarge { actual_bytes } => {
            return Err(format!(
                "AUDIT_ARCHIVE_KEY_CONFLICT key={key} existing object exceeds candidate byte bound {max_existing_bytes} (reported_bytes={actual_bytes:?}) — refusing to buffer or overwrite immutable archive object",
            ));
        }
    }
    match r2.put_if_absent(key, bytes.to_vec()).await {
        Ok(true) => {
            let ceiling = u64::try_from(bytes.len())
                .map_err(|_| format!("archive candidate for key={key} exceeds u64"))?;
            match r2.get_capped(key, ceiling).await? {
                CappedGet::Found(readback) if readback == bytes => Ok(ChunkWrite::Created),
                CappedGet::Found(_) => Err(format!(
                    "AUDIT_ARCHIVE_KEY_CONFLICT key={key}: created object readback differs"
                )),
                CappedGet::Missing => Err(format!(
                    "archive key {key} accepted create-if-absent but reads absent"
                )),
                CappedGet::TooLarge { actual_bytes } => Err(format!(
                    "archive key {key} created object exceeds bounded bytes={} actual_bytes={actual_bytes:?}",
                    bytes.len()
                )),
            }
        }
        Ok(false) => {
            let existing = match r2.get_capped(key, max_existing_bytes).await? {
                CappedGet::Found(existing) => existing,
                // A key that rejected the conditional PUT but then reads absent
                // is a consistency fault, not an idempotent replay — refuse it
                // rather than mark rows archived against nothing.
                CappedGet::Missing => {
                    return Err(format!(
                        "archive key {key} rejected create-if-absent but reads absent"
                    ));
                }
                CappedGet::TooLarge { actual_bytes } => {
                    return Err(format!(
                        "AUDIT_ARCHIVE_KEY_CONFLICT key={key} race winner exceeds candidate byte bound {max_existing_bytes} (reported_bytes={actual_bytes:?}) — refusing to buffer or overwrite immutable archive object",
                    ));
                }
            };
            classify_existing_chunk(key, &existing, bytes, epoch, link_key)
        }
        Err(e) => Err(e),
    }
}

/// The one and only archive-bookkeeping UPDATE. JSON1 expands the bounded row
/// id array inside SQLite, and the exact-set CTE makes the update an all-or-
/// nothing operation: a concurrent archiver that claims even one requested row
/// causes this statement to return no rows and update nothing.
const MARK_ARCHIVED_SQL: &str = "WITH requested AS (
       SELECT CAST(value AS TEXT) AS id FROM json_each(?2)
    ), eligible AS (
       SELECT o.id FROM audit_outbox o
       JOIN requested r ON r.id = o.id
       WHERE o.emitted_at IS NOT NULL AND o.archived_at IS NULL
    ), exact AS (
       SELECT 1
       WHERE (SELECT COUNT(*) FROM requested) = CAST(?3 AS INTEGER)
         AND (SELECT COUNT(*) FROM requested) =
             (SELECT COUNT(DISTINCT id) FROM requested)
         AND (SELECT COUNT(*) FROM eligible) = CAST(?3 AS INTEGER)
         AND (SELECT COUNT(*) FROM eligible) =
             (SELECT COUNT(DISTINCT id) FROM eligible)
    )
    UPDATE audit_outbox
    SET archived_at = CAST(?1 AS INTEGER)
    WHERE id IN (SELECT id FROM requested)
      AND emitted_at IS NOT NULL AND archived_at IS NULL
      AND EXISTS (SELECT 1 FROM exact)
    RETURNING id";

fn validate_archived_returned_ids(
    lines: &[SealedArchiveLine],
    returned: &[D1Row],
) -> Result<(), String> {
    let expected = lines
        .iter()
        .map(|line| line.row_id.clone())
        .collect::<BTreeSet<_>>();
    if expected.len() != lines.len() {
        return Err("archive watermark input contains duplicate row ids".to_owned());
    }

    let mut actual = BTreeSet::new();
    for row in returned {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("archive watermark RETURNING row has missing/non-text id")?;
        if !actual.insert(id.to_owned()) {
            return Err("archive watermark RETURNING contained duplicate row ids".to_owned());
        }
    }
    if actual != expected {
        return Err(format!(
            "archive watermark returned {} rows for {} requested rows; refusing partial/unexpected update",
            actual.len(),
            expected.len()
        ));
    }
    Ok(())
}

/// Mark one chunk's rows archived in one atomic, exact-set UPDATE. A re-run
/// (or a concurrent archiver) is accepted only when this call owns every row.
async fn mark_archived(
    d1: &D1HttpClient,
    lines: &[SealedArchiveLine],
    now: i64,
) -> Result<(), String> {
    if lines.is_empty() {
        return Err("archive watermark refuses an empty chunk".to_owned());
    }
    if lines.len() > DEFAULT_SEALED_MAX_LINES_PER_CHUNK {
        return Err(format!(
            "archive watermark chunk exceeds {}-row bound",
            DEFAULT_SEALED_MAX_LINES_PER_CHUNK
        ));
    }
    let row_ids = lines
        .iter()
        .map(|line| Value::String(line.row_id.clone()))
        .collect::<Vec<_>>();
    let row_ids_json = serde_json::to_string(&row_ids)
        .map_err(|e| format!("archive watermark row-id JSON encoding failed: {e}"))?;
    let returned = d1
        .query(
            MARK_ARCHIVED_SQL,
            &[json!(now), Value::String(row_ids_json), json!(lines.len())],
        )
        .await?;
    validate_archived_returned_ids(lines, &returned)
}

/// Census of what a quarantine sweep is about to take out of the work queue.
///
/// `count` is read with the SAME predicate the UPDATE uses, immediately before
/// it, so the number reported to the operator is the number of rows actually
/// marked rather than an estimate. Both the epoch identity and
/// `max_sequence` bound the UPDATE. Without both, a duplicate sequence at an
/// epoch boundary could quarantine a valid keyed row as legacy evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuarantineCensus {
    count: u64,
    max_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArchiveEpochIdentity {
    algorithm_id: u8,
    epoch_id: u64,
    link_key_id: Option<u64>,
}

impl From<&SealedArchiveLine> for ArchiveEpochIdentity {
    fn from(line: &SealedArchiveLine) -> Self {
        Self {
            algorithm_id: line.algorithm_id,
            epoch_id: line.epoch_id,
            link_key_id: line.link_key_id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuarantineBounds {
    from_sequence: u64,
    through_sequence: u64,
}

/// The one and only quarantine write. Sets ONLY `quarantined_at` and
/// `quarantine_reason` — never a chain column. See
/// `no_update_ever_touches_a_chain_column` in this file's tests.
///
/// The D1 REST API binds JSON numbers as REAL, so every integer bind that is
/// COMPARED against an INTEGER column is wrapped in `CAST(?N AS INTEGER)`.
const QUARANTINE_SQL: &str = "UPDATE audit_outbox \
     SET quarantined_at = CAST(?1 AS INTEGER), quarantine_reason = ?2 \
     WHERE tenant_id = ?3 AND region = ?4 \
       AND emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL \
       AND sequence_number IS NOT NULL AND canonical_jcs IS NOT NULL \
       AND ((CAST(?5 AS INTEGER) = 0 \
             AND ((algorithm_id IS NULL AND epoch_id IS NULL AND link_key_id IS NULL) \
                  OR (algorithm_id = 0 AND epoch_id = 0 AND link_key_id IS NULL))) \
            OR (CAST(?5 AS INTEGER) = 1 AND algorithm_id = 1 \
                AND epoch_id = CAST(?6 AS INTEGER) AND link_key_id = CAST(?7 AS INTEGER))) \
       AND sequence_number >= CAST(?8 AS INTEGER) \
       AND sequence_number <= CAST(?9 AS INTEGER)";

/// Count the rows a quarantine would take, and find the upper bound.
async fn quarantine_census(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    epoch: ArchiveEpochIdentity,
    bounds: QuarantineBounds,
) -> Result<Option<QuarantineCensus>, String> {
    let rows = d1
        .query(
            "SELECT COUNT(*) AS n, MAX(sequence_number) AS max_seq FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL \
               AND sequence_number IS NOT NULL AND canonical_jcs IS NOT NULL \
               AND ((CAST(?3 AS INTEGER) = 0 \
                     AND ((algorithm_id IS NULL AND epoch_id IS NULL AND link_key_id IS NULL) \
                          OR (algorithm_id = 0 AND epoch_id = 0 AND link_key_id IS NULL))) \
                    OR (CAST(?3 AS INTEGER) = 1 AND algorithm_id = 1 \
                        AND epoch_id = CAST(?4 AS INTEGER) \
                        AND link_key_id = CAST(?5 AS INTEGER))) \
               AND sequence_number >= CAST(?6 AS INTEGER) \
               AND sequence_number <= CAST(?7 AS INTEGER)",
            &[
                json!(tenant_id),
                json!(region),
                json!(epoch.algorithm_id),
                json!(epoch.epoch_id),
                json!(epoch.link_key_id),
                json!(bounds.from_sequence),
                json!(bounds.through_sequence),
            ],
        )
        .await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let count = row
        .get("n")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_outbox quarantine census: COUNT(*) missing/non-integer")?;
    if count == 0 {
        return Ok(None);
    }
    // MAX over a non-empty set is never NULL, but read it as an Option rather
    // than assume: quarantining against an unknown upper bound is exactly the
    // unbounded UPDATE this census exists to prevent.
    let max_sequence = row
        .get("max_seq")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_outbox quarantine census: MAX(sequence_number) missing/non-integer")?;
    Ok(Some(QuarantineCensus {
        count,
        max_sequence,
    }))
}

/// Mark the unarchivable tail of one epoch segment, never crossing into a row
/// with different epoch metadata. Returns how many rows left the work queue.
async fn quarantine_tail(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    epoch: ArchiveEpochIdentity,
    bounds: QuarantineBounds,
    reason: &str,
    now: i64,
) -> Result<u64, String> {
    let Some(census) = quarantine_census(d1, tenant_id, region, epoch, bounds).await? else {
        return Ok(0);
    };
    d1.query(
        QUARANTINE_SQL,
        &[
            json!(now),
            json!(reason),
            json!(tenant_id),
            json!(region),
            json!(epoch.algorithm_id),
            json!(epoch.epoch_id),
            json!(epoch.link_key_id),
            json!(bounds.from_sequence),
            json!(census.max_sequence),
        ],
    )
    .await?;
    // WARN, not INFO: a quarantined row is permanently outside the offsite
    // evidence copy. It must appear in the container log even when the sweep
    // as a whole returns 200.
    tracing::warn!(
        tenant_id = %tenant_id,
        region = %region,
        rows = census.count,
        from_sequence = bounds.from_sequence,
        to_sequence = census.max_sequence,
        reason = %reason,
        "audit/archive: QUARANTINED an unarchivable chain segment — \
         rows stay sealed in D1 but will NEVER reach the R2 archive \
         (runbook RB-AUDIT-ARCHIVE-ABSENT §5)"
    );
    Ok(census.count)
}

/// Outcome of archiving a single partition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PartitionOutcome {
    rows: u64,
    chunks_created: u64,
    chunks_already_present: u64,
    /// Rows this call ruled permanently unarchivable.
    rows_quarantined: u64,
    /// The batch limit truncated this partition's tail.
    truncated: bool,
}

/// Exact signed archive-manifest wire shape from WI-S09-007. Unknown or
/// omitted fields are rejected before a D1 index row can become authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveManifestObjectJcs {
    blake3_hash: String,
    byte_length: u64,
    object_key: String,
    record_count: u64,
    start_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveManifestJcs {
    algorithm_id: u8,
    end_head_hash: String,
    end_head_witness_hash: String,
    end_head_witness_sequence: u64,
    end_sequence_exclusive: u64,
    epoch_id: u64,
    epoch_ledger_hash: String,
    epoch_ledger_sequence: u64,
    is_empty: bool,
    link_key_id: Option<u64>,
    manifest_type: String,
    manifest_version: u8,
    objects: Vec<ArchiveManifestObjectJcs>,
    record_count: u64,
    region: String,
    signing_key_id: u64,
    start_prev_hash: String,
    start_sequence: u64,
    tenant_id: String,
}

/// Columns duplicated outside `manifest_jcs`. D1 is only an index, so every
/// one is compared to the authenticated canonical document.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ArchiveManifestIndex {
    tenant_id: String,
    region: String,
    epoch_id: u64,
    start_sequence: u64,
    end_sequence_exclusive: u64,
    record_count: u64,
    is_empty: bool,
    algorithm_id: u8,
    link_key_id: Option<u64>,
    start_prev_hash: String,
    end_head_hash: String,
    end_head_witness_sequence: u64,
    end_head_witness_hash: String,
    epoch_ledger_sequence: u64,
    epoch_ledger_hash: String,
    manifest_version: u8,
    manifest_hash: String,
    signing_key_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveEpochGenesisJcs {
    algorithm_id: u8,
    checkpoint_type: String,
    checkpoint_version: u8,
    epoch_id: u64,
    ledger_sequence: u64,
    ledger_version: u8,
    link_key_id: Option<u64>,
    previous_ledger_hash: String,
    region: String,
    signing_key_id: u64,
    start_prev_hash: String,
    start_sequence: u64,
    tenant_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveEpochTransitionJcs {
    algorithm_id: u8,
    checkpoint_type: String,
    checkpoint_version: u8,
    epoch_id: u64,
    from_epoch_id: u64,
    from_head_hash: String,
    from_next_sequence: u64,
    ledger_sequence: u64,
    ledger_version: u8,
    link_key_id: u64,
    previous_ledger_hash: String,
    region: String,
    signing_key_id: u64,
    tenant_id: String,
    to_start_prev_hash: String,
    to_start_sequence: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum ArchiveEpochLedgerJcs {
    Genesis(ArchiveEpochGenesisJcs),
    Transition(ArchiveEpochTransitionJcs),
}

impl ArchiveEpochLedgerJcs {
    fn common(&self) -> (u64, u64, u8, Option<u64>, &str, &str, &str, u64) {
        match self {
            Self::Genesis(value) => (
                value.ledger_sequence,
                value.epoch_id,
                value.algorithm_id,
                value.link_key_id,
                &value.previous_ledger_hash,
                &value.tenant_id,
                &value.region,
                value.signing_key_id,
            ),
            Self::Transition(value) => (
                value.ledger_sequence,
                value.epoch_id,
                value.algorithm_id,
                Some(value.link_key_id),
                &value.previous_ledger_hash,
                &value.tenant_id,
                &value.region,
                value.signing_key_id,
            ),
        }
    }

    fn epoch(&self) -> Result<ChainEpoch, String> {
        match self {
            Self::Genesis(value)
                if value.epoch_id == 0
                    && value.ledger_sequence == 0
                    && value.algorithm_id == 0
                    && value.link_key_id.is_none()
                    && value.checkpoint_type == "epoch-genesis"
                    && value.checkpoint_version == 1
                    && value.ledger_version == 1
                    && value.start_sequence == 0
                    && value.start_prev_hash == "0".repeat(64) =>
            {
                Ok(ChainEpoch::legacy())
            }
            Self::Transition(value)
                if value.checkpoint_type == "epoch-transition"
                    && value.checkpoint_version == 1
                    && value.ledger_version == 1
                    && value.algorithm_id == 1
                    && value.from_head_hash == value.to_start_prev_hash
                    && value.from_next_sequence == value.to_start_sequence
                    && is_lower_hash(&value.from_head_hash) =>
            {
                ChainEpoch::keyed_successor(
                    value.epoch_id,
                    value.link_key_id,
                    value.to_start_sequence,
                    archive_chain_hash(&value.to_start_prev_hash)?,
                    value.from_epoch_id,
                )
                .map_err(|error| format!("epoch ledger transition invalid: {error}"))
            }
            Self::Genesis(_) | Self::Transition(_) => {
                Err("epoch ledger genesis contract mismatch".to_owned())
            }
        }
    }
}

/// Public verification authorities are represented by authenticated values,
/// not flags. These constructors are fed by the trust-root, full-ledger and
/// challenged-latest loaders; mutable D1 cannot mint them.
#[derive(Clone)]
#[cfg(test)]
struct AuthenticatedSigningKey {
    key_id: u64,
    verifying_key: ed25519_dalek::VerifyingKey,
}

trait ArchiveSignatureVerifier {
    fn archive_key_id(&self) -> u64;
    fn verify_archive_signature(&self, payload: &[u8], signature_b64: &str) -> Result<(), String>;
}

#[cfg(test)]
impl ArchiveSignatureVerifier for AuthenticatedSigningKey {
    fn archive_key_id(&self) -> u64 {
        self.key_id
    }

    fn verify_archive_signature(&self, payload: &[u8], signature_b64: &str) -> Result<(), String> {
        self.verifying_key
            .verify(payload, &decode_canonical_signature(signature_b64)?)
            .map_err(|_| "archive manifest signature verification failed".to_owned())
    }
}

impl ArchiveSignatureVerifier for B054ArchiveSigningKey {
    fn archive_key_id(&self) -> u64 {
        self.key_id()
    }

    fn verify_archive_signature(&self, payload: &[u8], signature_b64: &str) -> Result<(), String> {
        self.verify_exact_signature(payload, signature_b64)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthenticatedLedgerAnchor {
    tenant_id: String,
    region: String,
    epoch_id: u64,
    ledger_sequence: u64,
    ledger_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthenticatedWitnessAnchor {
    tenant_id: String,
    region: String,
    witness_sequence: u64,
    witness_record_hash: String,
    head_hash: String,
    head_next_sequence: u64,
    epoch_id: u64,
    epoch_ledger_sequence: u64,
    epoch_ledger_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
struct AuthenticatedArchiveObject {
    object_key: String,
    bytes: Vec<u8>,
}

trait ArchiveObjectBytes {
    fn object_key(&self) -> &str;
    fn bytes(&self) -> &[u8];
}

#[cfg(test)]
impl ArchiveObjectBytes for AuthenticatedArchiveObject {
    fn object_key(&self) -> &str {
        &self.object_key
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Canonical immutable object prepared entirely before the first R2 request.
/// Keeping preparation separate from publication makes the "zero R2 before
/// authentication/preflight" boundary mechanically reviewable.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedArchiveObject {
    manifest: ArchiveManifestObjectJcs,
    bytes: Vec<u8>,
    row_ids: Vec<String>,
}

impl ArchiveObjectBytes for PreparedArchiveObject {
    fn object_key(&self) -> &str {
        &self.manifest.object_key
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedArchiveManifestEnvelope {
    manifest_hash: String,
    manifest_jcs_b64: String,
    signature_b64: String,
}

struct PreparedSignedArchive {
    index: ArchiveManifestIndex,
    manifest_jcs: Vec<u8>,
    signature_b64: String,
    envelope_key: String,
    envelope_bytes: Vec<u8>,
    objects: Vec<PreparedArchiveObject>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthenticatedArchiveManifest {
    manifest_hash: String,
    epoch_id: u64,
    algorithm_id: u8,
    link_key_id: Option<u64>,
    key_commitment: Option<ChainHash>,
}

fn is_lower_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn archive_chain_hash(value: &str) -> Result<ChainHash, String> {
    if !is_lower_hash(value) {
        return Err("archive chain hash is not canonical lower hex".to_owned());
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index.saturating_mul(2);
        *byte = u8::from_str_radix(
            value
                .get(start..start.saturating_add(2))
                .ok_or("archive chain hash slice failed")?,
            16,
        )
        .map_err(|_| "archive chain hash decode failed")?;
    }
    Ok(ChainHash(bytes))
}

fn canonical_archive_partition(tenant_id: &str, region: &str) -> bool {
    uuid::Uuid::parse_str(tenant_id).ok().is_some_and(|id| {
        id.get_version_num() == 7
            && id.get_variant() == uuid::Variant::RFC4122
            && id.hyphenated().to_string() == tenant_id
    }) && matches!(region, "wnam" | "enam" | "weur" | "sam" | "apac" | "afr")
}

fn hash_domain_payload(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(payload);
    hasher.finalize().to_hex().to_string()
}

fn archive_manifest_object_key(
    tenant_id: &str,
    region: &str,
    epoch_id: u64,
    manifest_hash: &str,
) -> String {
    format!("audit/manifests/{tenant_id}/{region}/epoch-{epoch_id:08}/{manifest_hash}.jcs.json")
}

/// Publish immutable bytes and prove the exact readback under a caller-supplied
/// memory ceiling. A successful PUT is read back too: the HTTP acknowledgement
/// alone is not evidence that the bytes retained by the Object-Lock bucket are
/// the bytes the signed manifest names.
async fn put_exact_object_last(
    r2: &R2S3Client,
    key: &str,
    bytes: &[u8],
    ceiling: u64,
) -> Result<bool, String> {
    let byte_len = u64::try_from(bytes.len())
        .map_err(|_| format!("archive object key={key} byte length exceeds u64"))?;
    if byte_len == 0 || byte_len > ceiling {
        return Err(format!(
            "archive object key={key} violates publication byte ceiling"
        ));
    }
    let created = r2.put_if_absent(key, bytes.to_vec()).await?;
    match r2.get_capped(key, ceiling).await? {
        CappedGet::Found(readback) if readback == bytes => Ok(created),
        CappedGet::Found(_) => Err(format!(
            "AUDIT_ARCHIVE_KEY_CONFLICT key={key}: immutable readback differs from authenticated bytes"
        )),
        CappedGet::Missing => Err(format!(
            "archive key={key} accepted/rejected publication but reads absent"
        )),
        CappedGet::TooLarge { actual_bytes } => Err(format!(
            "archive key={key} readback exceeds ceiling={ceiling} actual_bytes={actual_bytes:?}"
        )),
    }
}

fn prepare_archive_objects(
    lines: &[SealedArchiveLine],
    epoch: &ChainEpoch,
    link_key: Option<&LinkKey>,
    max_lines: usize,
    max_bytes: usize,
) -> Result<Vec<PreparedArchiveObject>, String> {
    if lines.is_empty() {
        return Err("keyed live archive refuses an empty row range".to_owned());
    }
    let chunks = split_into_chunks(lines.to_vec(), max_lines, max_bytes);
    let mut prepared = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let first = chunk
            .first()
            .ok_or("archive chunk splitter returned an empty chunk")?;
        let bytes = serialize_chunk_for_epoch(&chunk, epoch, link_key)
            .map_err(|error| format!("archive chunk verification failed: {error}"))?;
        if bytes.is_empty() || bytes.len() > max_bytes {
            return Err("archive chunk serialization violated configured byte bound".to_owned());
        }
        let content_hash = hash_domain_payload(&[], &bytes);
        prepared.push(PreparedArchiveObject {
            manifest: ArchiveManifestObjectJcs {
                blake3_hash: content_hash.clone(),
                byte_length: u64::try_from(bytes.len())
                    .map_err(|_| "archive object byte length exceeds u64")?,
                object_key: format!(
                    "{}.epoch-{}.{}",
                    sealed_chunk_key(first),
                    epoch.epoch_id(),
                    content_hash
                ),
                record_count: u64::try_from(chunk.len())
                    .map_err(|_| "archive object row count exceeds u64")?,
                start_sequence: first.sequence_number,
            },
            row_ids: chunk.iter().map(|line| line.row_id.clone()).collect(),
            bytes,
        });
    }
    if prepared.windows(2).any(|pair| {
        pair.first()
            .map(|object| object.manifest.object_key.as_bytes())
            >= pair
                .get(1)
                .map(|object| object.manifest.object_key.as_bytes())
    }) {
        return Err("archive object keys are not strictly ordered with chain sequence".to_owned());
    }
    Ok(prepared)
}

#[allow(clippy::too_many_arguments)]
fn prepare_signed_archive(
    lines: &[SealedArchiveLine],
    objects: Vec<PreparedArchiveObject>,
    epoch: &ChainEpoch,
    ledger: &AuthenticatedLedgerAnchor,
    witness: &AuthenticatedWitnessAnchor,
    signing_key_id: u64,
    signature_b64: impl FnOnce(&[u8]) -> Result<String, String>,
) -> Result<PreparedSignedArchive, String> {
    let first = lines
        .first()
        .ok_or("signed archive manifest refuses an empty row range")?;
    let last = lines
        .last()
        .ok_or("signed archive manifest lost its final row")?;
    let end_sequence_exclusive = last
        .sequence_number
        .checked_add(1)
        .ok_or("archive manifest end sequence overflow")?;
    if witness.tenant_id != first.tenant_id
        || witness.region != first.region
        || witness.head_next_sequence != end_sequence_exclusive
        || witness.head_hash != last.chain_hash
        || witness.epoch_id != epoch.epoch_id()
        || witness.epoch_ledger_sequence != ledger.ledger_sequence
        || witness.epoch_ledger_hash != ledger.ledger_hash
    {
        return Err("greatest witnessed boundary does not bind selected archive rows".to_owned());
    }
    let manifest = ArchiveManifestJcs {
        algorithm_id: epoch.algorithm().id(),
        end_head_hash: witness.head_hash.clone(),
        end_head_witness_hash: witness.witness_record_hash.clone(),
        end_head_witness_sequence: witness.witness_sequence,
        end_sequence_exclusive,
        epoch_id: epoch.epoch_id(),
        epoch_ledger_hash: ledger.ledger_hash.clone(),
        epoch_ledger_sequence: ledger.ledger_sequence,
        is_empty: false,
        link_key_id: epoch.link_key_id(),
        manifest_type: "audit-chain-archive-manifest".to_owned(),
        manifest_version: 1,
        objects: objects
            .iter()
            .map(|object| object.manifest.clone())
            .collect(),
        record_count: u64::try_from(lines.len()).map_err(|_| "archive row count exceeds u64")?,
        region: first.region.clone(),
        signing_key_id,
        start_prev_hash: first.prev_hash.clone(),
        start_sequence: first.sequence_number,
        tenant_id: first.tenant_id.clone(),
    };
    let manifest_jcs = serde_jcs::to_vec(&manifest)
        .map_err(|error| format!("archive manifest canonicalization failed: {error}"))?;
    if manifest_jcs.len() > MAX_ARCHIVE_MANIFEST_BYTES {
        return Err("archive manifest exceeds bounded canonical size".to_owned());
    }
    let manifest_hash = hash_domain_payload(ARCHIVE_MANIFEST_DOMAIN, &manifest_jcs);
    let signature_b64 = signature_b64(&manifest_jcs)?;
    decode_canonical_signature(&signature_b64)?;
    let index = ArchiveManifestIndex {
        tenant_id: manifest.tenant_id.clone(),
        region: manifest.region.clone(),
        epoch_id: manifest.epoch_id,
        start_sequence: manifest.start_sequence,
        end_sequence_exclusive: manifest.end_sequence_exclusive,
        record_count: manifest.record_count,
        is_empty: false,
        algorithm_id: manifest.algorithm_id,
        link_key_id: manifest.link_key_id,
        start_prev_hash: manifest.start_prev_hash.clone(),
        end_head_hash: manifest.end_head_hash.clone(),
        end_head_witness_sequence: manifest.end_head_witness_sequence,
        end_head_witness_hash: manifest.end_head_witness_hash.clone(),
        epoch_ledger_sequence: manifest.epoch_ledger_sequence,
        epoch_ledger_hash: manifest.epoch_ledger_hash.clone(),
        manifest_version: 1,
        manifest_hash: manifest_hash.clone(),
        signing_key_id,
    };
    let envelope = SignedArchiveManifestEnvelope {
        manifest_hash: manifest_hash.clone(),
        manifest_jcs_b64: base64::engine::general_purpose::STANDARD.encode(&manifest_jcs),
        signature_b64: signature_b64.clone(),
    };
    let envelope_bytes = serde_jcs::to_vec(&envelope)
        .map_err(|error| format!("archive manifest envelope canonicalization failed: {error}"))?;
    if envelope_bytes.len() > MAX_ARCHIVE_MANIFEST_ENVELOPE_BYTES {
        return Err("archive manifest envelope exceeds bounded size".to_owned());
    }
    let envelope_key = archive_manifest_object_key(
        &manifest.tenant_id,
        &manifest.region,
        manifest.epoch_id,
        &manifest_hash,
    );
    Ok(PreparedSignedArchive {
        index,
        manifest_jcs,
        signature_b64,
        envelope_key,
        envelope_bytes,
        objects,
    })
}

fn decode_canonical_signature(value: &str) -> Result<Signature, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| "archive manifest signature is malformed base64")?;
    if decoded.len() != 64 || base64::engine::general_purpose::STANDARD.encode(&decoded) != value {
        return Err("archive manifest signature is not canonical padded base64".to_owned());
    }
    let bytes: [u8; 64] = decoded
        .try_into()
        .map_err(|_| "archive manifest signature has wrong length")?;
    Ok(Signature::from_bytes(&bytes))
}

/// Authenticate the complete immutable-manifest boundary using already
/// authenticated trust-root/ledger/witness values and bytes read back from R2.
/// This helper cannot be satisfied with booleans or D1 projections.
#[allow(clippy::too_many_arguments)]
fn authenticate_archive_manifest<T: ArchiveObjectBytes>(
    index: &ArchiveManifestIndex,
    manifest_jcs: &[u8],
    signature_b64: &str,
    signer: &impl ArchiveSignatureVerifier,
    ledger: &AuthenticatedLedgerAnchor,
    witness: &AuthenticatedWitnessAnchor,
    objects: &[T],
    epoch: &ChainEpoch,
    link_key: Option<&LinkKey>,
    key_commitment: Option<&ChainHash>,
) -> Result<AuthenticatedArchiveManifest, String> {
    if manifest_jcs.is_empty() || manifest_jcs.len() > MAX_ARCHIVE_MANIFEST_BYTES {
        return Err("archive manifest JCS is empty or exceeds the bounded size".to_owned());
    }
    let manifest: ArchiveManifestJcs =
        serde_json::from_slice(manifest_jcs).map_err(|_| "archive manifest JCS is malformed")?;
    let canonical =
        serde_jcs::to_vec(&manifest).map_err(|_| "archive manifest canonicalization failed")?;
    if canonical != manifest_jcs {
        return Err("archive manifest is not exact RFC-8785 JCS".to_owned());
    }
    if manifest.manifest_type != "audit-chain-archive-manifest"
        || manifest.manifest_version != 1
        || manifest.signing_key_id == 0
        || manifest.signing_key_id > JS_SAFE_MAX
        || manifest.epoch_id > JS_SAFE_MAX
        || manifest.start_sequence > JS_SAFE_MAX
        || manifest.end_sequence_exclusive > JS_SAFE_MAX
        || manifest.record_count > JS_SAFE_MAX
        || manifest.end_head_witness_sequence > JS_SAFE_MAX
        || manifest.epoch_ledger_sequence > JS_SAFE_MAX
        || !is_lower_hash(&manifest.start_prev_hash)
        || !is_lower_hash(&manifest.end_head_hash)
        || !is_lower_hash(&manifest.end_head_witness_hash)
        || !is_lower_hash(&manifest.epoch_ledger_hash)
        || !matches!(manifest.algorithm_id, 0 | 1)
        || !canonical_archive_partition(&manifest.tenant_id, &manifest.region)
        || !((manifest.algorithm_id == 0
            && manifest.epoch_id == 0
            && manifest.link_key_id.is_none())
            || (manifest.algorithm_id == 1
                && manifest.epoch_id > 0
                && manifest
                    .link_key_id
                    .is_some_and(|key_id| key_id > 0 && key_id <= JS_SAFE_MAX)))
    {
        return Err("archive manifest security fields violate the v1 contract".to_owned());
    }
    epoch
        .validate()
        .map_err(|error| format!("archive manifest epoch descriptor is invalid: {error}"))?;
    if epoch.epoch_id() != manifest.epoch_id
        || epoch.algorithm().id() != manifest.algorithm_id
        || epoch.link_key_id() != manifest.link_key_id
        || match (manifest.algorithm_id, link_key, key_commitment) {
            (0, None, None) => false,
            (1, Some(key), Some(commitment)) => !key_matches_commitment(key, commitment),
            _ => true,
        }
    {
        return Err(
            "archive manifest algorithm/key differs from authenticated epoch registry".to_owned(),
        );
    }
    let computed_hash = hash_domain_payload(ARCHIVE_MANIFEST_DOMAIN, manifest_jcs);
    if !is_lower_hash(&index.manifest_hash)
        || computed_hash != index.manifest_hash
        || index.tenant_id != manifest.tenant_id
        || index.region != manifest.region
        || index.epoch_id != manifest.epoch_id
        || index.start_sequence != manifest.start_sequence
        || index.end_sequence_exclusive != manifest.end_sequence_exclusive
        || index.record_count != manifest.record_count
        || index.is_empty != manifest.is_empty
        || index.algorithm_id != manifest.algorithm_id
        || index.link_key_id != manifest.link_key_id
        || index.start_prev_hash != manifest.start_prev_hash
        || index.end_head_hash != manifest.end_head_hash
        || index.end_head_witness_sequence != manifest.end_head_witness_sequence
        || index.end_head_witness_hash != manifest.end_head_witness_hash
        || index.epoch_ledger_sequence != manifest.epoch_ledger_sequence
        || index.epoch_ledger_hash != manifest.epoch_ledger_hash
        || index.manifest_version != manifest.manifest_version
        || index.signing_key_id != manifest.signing_key_id
    {
        return Err("archive manifest D1 index differs from signed JCS".to_owned());
    }
    if signer.archive_key_id() != manifest.signing_key_id {
        return Err("archive manifest names a different authenticated signing key".to_owned());
    }
    signer.verify_archive_signature(manifest_jcs, signature_b64)?;
    if ledger.tenant_id != manifest.tenant_id
        || ledger.region != manifest.region
        || ledger.epoch_id != manifest.epoch_id
        || ledger.ledger_sequence != manifest.epoch_ledger_sequence
        || ledger.ledger_hash != manifest.epoch_ledger_hash
    {
        return Err("archive manifest differs from authenticated epoch ledger".to_owned());
    }
    if witness.tenant_id != manifest.tenant_id
        || witness.region != manifest.region
        || witness.witness_sequence != manifest.end_head_witness_sequence
        || witness.witness_record_hash != manifest.end_head_witness_hash
        || witness.head_hash != manifest.end_head_hash
        || witness.head_next_sequence != manifest.end_sequence_exclusive
        || witness.epoch_id != manifest.epoch_id
        || witness.epoch_ledger_sequence != manifest.epoch_ledger_sequence
        || witness.epoch_ledger_hash != manifest.epoch_ledger_hash
    {
        return Err("archive manifest differs from authenticated witness head".to_owned());
    }

    let mut supplied = BTreeMap::new();
    for object in objects {
        if supplied
            .insert(object.object_key(), object.bytes())
            .is_some()
        {
            return Err("archive verifier received a duplicate object key".to_owned());
        }
    }
    let mut prior_key: Option<&[u8]> = None;
    let mut prior_chain_head: Option<String> = None;
    let mut next_sequence = manifest.start_sequence;
    let mut total_records = 0_u64;
    for object in &manifest.objects {
        if object.object_key.is_empty()
            || object
                .object_key
                .as_bytes()
                .first()
                .is_some_and(|byte| *byte == b'/')
            || object
                .object_key
                .as_bytes()
                .windows(2)
                .any(|window| window == b"..")
            || !is_lower_hash(&object.blake3_hash)
            || object.byte_length > JS_SAFE_MAX
            || object.record_count == 0
            || object.record_count > JS_SAFE_MAX
            || object.start_sequence > JS_SAFE_MAX
            || object.start_sequence != next_sequence
            || prior_key.is_some_and(|prior| prior >= object.object_key.as_bytes())
        {
            return Err(
                "archive manifest object list is malformed, unsorted, or non-contiguous".to_owned(),
            );
        }
        let bytes = supplied
            .remove(object.object_key.as_str())
            .ok_or("archive manifest names an unavailable object")?;
        if u64::try_from(bytes.len()).ok() != Some(object.byte_length)
            || hash_domain_payload(&[], bytes) != object.blake3_hash
        {
            return Err("archive object bytes differ from signed manifest".to_owned());
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| "archive manifest object is not UTF-8")?;
        let lines = text
            .split('\n')
            .map(serde_json::from_str::<SealedArchiveLine>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "archive manifest object is malformed NDJSON")?;
        corelink_audit_chain::verify_chunk_for_epoch(&lines, epoch, link_key)
            .map_err(|error| format!("archive manifest object chain does not verify: {error}"))?;
        if u64::try_from(lines.len()).ok() != Some(object.record_count)
            || lines.first().map(|line| line.sequence_number) != Some(object.start_sequence)
            || lines
                .last()
                .and_then(|line| line.sequence_number.checked_add(1))
                != object.start_sequence.checked_add(object.record_count)
            || lines.iter().enumerate().any(|(offset, line)| {
                line.sequence_number
                    != object
                        .start_sequence
                        .saturating_add(u64::try_from(offset).unwrap_or(u64::MAX))
                    || line.tenant_id != manifest.tenant_id
                    || line.region != manifest.region
                    || line.epoch_id != manifest.epoch_id
                    || line.algorithm_id != manifest.algorithm_id
                    || line.link_key_id != manifest.link_key_id
                    || line.schema
                        != if manifest.algorithm_id == 0 {
                            SEALED_LINE_SCHEMA
                        } else {
                            SEALED_LINE_SCHEMA_V2
                        }
            })
            || lines.first().map(|line| line.prev_hash.as_str())
                != Some(
                    prior_chain_head
                        .as_deref()
                        .unwrap_or(manifest.start_prev_hash.as_str()),
                )
        {
            return Err("archive object rows differ from signed manifest range/epoch".to_owned());
        }
        for pair in lines.windows(2) {
            if pair.get(1).map(|line| line.prev_hash.as_str())
                != pair.first().map(|line| line.chain_hash.as_str())
            {
                return Err("archive object rows have a chain-head discontinuity".to_owned());
            }
        }
        next_sequence = next_sequence
            .checked_add(object.record_count)
            .ok_or("archive manifest range overflow")?;
        total_records = total_records
            .checked_add(object.record_count)
            .ok_or("archive manifest record count overflow")?;
        prior_key = Some(object.object_key.as_bytes());
        prior_chain_head = lines.last().map(|line| line.chain_hash.clone());
    }
    if !supplied.is_empty()
        || total_records != manifest.record_count
        || next_sequence != manifest.end_sequence_exclusive
        || manifest.is_empty != manifest.objects.is_empty()
        || (!manifest.is_empty
            && prior_chain_head.as_deref() != Some(manifest.end_head_hash.as_str()))
        || (manifest.is_empty
            && (manifest.record_count != 0
                || manifest.start_sequence != manifest.end_sequence_exclusive
                || manifest.start_prev_hash != manifest.end_head_hash))
        || (!manifest.is_empty
            && (manifest.record_count == 0
                || manifest.start_sequence >= manifest.end_sequence_exclusive))
    {
        return Err(
            "archive manifest coverage is incomplete, overlapping, or has extra objects".to_owned(),
        );
    }
    Ok(AuthenticatedArchiveManifest {
        manifest_hash: computed_hash,
        epoch_id: manifest.epoch_id,
        algorithm_id: manifest.algorithm_id,
        link_key_id: manifest.link_key_id,
        key_commitment: key_commitment.copied(),
    })
}

/// Epoch facts that have already passed the authenticated trust-root,
/// full-ledger-signature, challenged witness receipt/latest and signed immutable
/// manifest/object checks.
///
/// Keeping this type private makes the trust boundary explicit: mutable D1
/// projection rows are not sufficient to construct an archive verifier. The
/// live keyed path constructs this only after the authenticated loader
/// completes; repository wiring does not claim deployment or production proof.
#[allow(dead_code)]
struct AuthenticatedArchiveEpoch {
    epoch: ChainEpoch,
    key_commitment: ChainHash,
    manifest: AuthenticatedArchiveManifest,
}

struct ArchiveEpochContext<'a> {
    epoch: ChainEpoch,
    link_key: Option<&'a LinkKey>,
}

/// Bind authenticated public epoch facts to the configured secret key. This
/// is the bounded prerequisite for enabling keyed archival: a matching key id
/// alone is insufficient because deployment configuration can contain stale
/// or substituted material.
#[allow(dead_code)]
fn context_from_authenticated_epoch<'a>(
    evidence: &AuthenticatedArchiveEpoch,
    keyring: &'a LinkKeyring,
) -> Result<ArchiveEpochContext<'a>, String> {
    evidence
        .epoch
        .validate()
        .map_err(|e| format!("authenticated archive epoch descriptor is invalid: {e}"))?;
    if evidence.manifest.epoch_id != evidence.epoch.epoch_id()
        || evidence.manifest.algorithm_id != evidence.epoch.algorithm().id()
        || evidence.manifest.link_key_id != evidence.epoch.link_key_id()
        || evidence.manifest.key_commitment != Some(evidence.key_commitment)
        || !is_lower_hash(&evidence.manifest.manifest_hash)
    {
        return Err("authenticated archive manifest does not bind the selected epoch".to_owned());
    }
    let key_id = evidence
        .epoch
        .link_key_id()
        .ok_or("authenticated keyed archive epoch has no link_key_id")?;
    let link_key = keyring.get(key_id).ok_or_else(|| {
        format!("configured audit link key {key_id} is missing; archive stopped closed")
    })?;
    if !key_matches_commitment(link_key, &evidence.key_commitment) {
        return Err(format!(
            "configured audit link key {key_id} does not match authenticated registry commitment"
        ));
    }
    Ok(ArchiveEpochContext {
        epoch: evidence.epoch.clone(),
        link_key: Some(link_key),
    })
}

fn same_epoch(left: &SealedArchiveLine, right: &SealedArchiveLine) -> bool {
    left.algorithm_id == right.algorithm_id
        && left.epoch_id == right.epoch_id
        && left.link_key_id == right.link_key_id
}

fn first_epoch_segment(lines: &[SealedArchiveLine]) -> &[SealedArchiveLine] {
    let Some(first) = lines.first() else {
        return lines;
    };
    let len = lines
        .iter()
        .take_while(|line| same_epoch(first, line))
        .count();
    lines.get(..len).unwrap_or(&[])
}

fn archive_d1_blob(value: &Value, field: &str, max: usize) -> Result<Vec<u8>, String> {
    let bytes = match value {
        Value::String(encoded) => base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| format!("{field} is not canonical D1 base64"))?,
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| format!("{field} contains a non-byte"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(format!("{field} is not a D1 blob")),
    };
    if bytes.is_empty() || bytes.len() > max {
        return Err(format!("{field} length outside archive bound"));
    }
    Ok(bytes)
}

async fn load_archive_signing_key_by_id(
    state: &AuditArchiveState,
    signing_key_id: u64,
) -> Result<B054ArchiveSigningKey, String> {
    let roots = state
        .archive_trust_roots
        .as_deref()
        .ok_or("B054 archive OOB trust roots unavailable")?;
    let rows = state
        .d1
        .query(
            "SELECT registry_jcs,registry_signature_b64 FROM audit_chain_signing_key_registry WHERE signing_key_id=?1",
            &[json!(signing_key_id)],
        )
        .await?;
    if rows.len() != 1 {
        return Err("B054 archive signing registry absent or ambiguous".to_owned());
    }
    let row = rows
        .first()
        .ok_or("B054 archive signing registry disappeared")?;
    let authenticated = b054_archive_authenticate_signing_registry(
        roots,
        archive_d1_blob(
            row.get("registry_jcs")
                .ok_or("B054 archive signing registry JCS missing")?,
            "signing registry JCS",
            MAX_ARCHIVE_MANIFEST_BYTES,
        )?,
        row.get("registry_signature_b64")
            .and_then(Value::as_str)
            .ok_or("B054 archive signing registry signature missing")?
            .to_owned(),
    )?;
    if authenticated.key_id() != signing_key_id {
        return Err("B054 archive signing registry identity mismatch".to_owned());
    }
    Ok(authenticated)
}

async fn load_archive_link_commitment(
    state: &AuditArchiveState,
    signing: &B054ArchiveSigningKey,
    link_key_id: u64,
) -> Result<ChainHash, String> {
    let rows = state
        .d1
        .query(
            "SELECT registry_jcs,registry_signature_b64 FROM audit_chain_link_key_registry WHERE link_key_id=?1",
            &[json!(link_key_id)],
        )
        .await?;
    if rows.len() != 1 {
        return Err("B054 archive link registry absent or ambiguous".to_owned());
    }
    let row = rows
        .first()
        .ok_or("B054 archive link registry disappeared")?;
    let link = b054_archive_authenticate_link_registry(
        signing,
        archive_d1_blob(
            row.get("registry_jcs")
                .ok_or("B054 archive link registry JCS missing")?,
            "link registry JCS",
            MAX_ARCHIVE_MANIFEST_BYTES,
        )?,
        row.get("registry_signature_b64")
            .and_then(Value::as_str)
            .ok_or("B054 archive link registry signature missing")?
            .to_owned(),
    )?;
    if link.key_id() != link_key_id || link.signing_key_id() != signing.key_id() {
        return Err("B054 archive link registry identity mismatch".to_owned());
    }
    archive_chain_hash(link.key_commitment_hex())
}

async fn load_authenticated_archive_ledger(
    state: &AuditArchiveState,
    tenant_id: &str,
    region: &str,
    target_epoch: u64,
) -> Result<(ChainEpoch, AuthenticatedLedgerAnchor, B054ArchiveSigningKey), String> {
    let mut previous_hash = "0".repeat(64);
    let mut selected: Option<(ChainEpoch, AuthenticatedLedgerAnchor, B054ArchiveSigningKey)> = None;
    let mut signing_keys = BTreeMap::<u64, B054ArchiveSigningKey>::new();
    let mut expected_sequence = 0_u64;
    while expected_sequence <= target_epoch {
        let rows = state
            .d1
            .query(
                "SELECT ledger_sequence,epoch_id,previous_ledger_hash,ledger_hash,entry_jcs,signature_b64,signing_key_id FROM audit_chain_epoch_ledger WHERE tenant_id=?1 AND region=?2 AND ledger_sequence>=?3 AND ledger_sequence<=?4 ORDER BY ledger_sequence LIMIT 128",
                &[json!(tenant_id), json!(region), json!(expected_sequence), json!(target_epoch)],
            )
            .await?;
        if rows.is_empty() {
            return Err("B054 archive ledger has a gap or missing epoch".to_owned());
        }
        for row in &rows {
            let jcs = archive_d1_blob(
                row.get("entry_jcs")
                    .ok_or("B054 archive ledger JCS missing")?,
                "epoch ledger JCS",
                MAX_ARCHIVE_MANIFEST_BYTES,
            )?;
            let parsed: ArchiveEpochLedgerJcs =
                serde_json::from_slice(&jcs).map_err(|_| "B054 archive ledger JCS malformed")?;
            if serde_jcs::to_vec(&parsed).map_err(|_| "B054 archive ledger JCS failure")? != jcs {
                return Err("B054 archive ledger is not exact RFC-8785 JCS".to_owned());
            }
            let signature = row
                .get("signature_b64")
                .and_then(Value::as_str)
                .ok_or("B054 archive ledger signature missing")?;
            let (
                sequence,
                epoch_id,
                _algorithm_id,
                _link_key_id,
                previous,
                tenant,
                row_region,
                key_id,
            ) = parsed.common();
            let signing = if let Some(signing) = signing_keys.get(&key_id) {
                signing.clone()
            } else {
                let signing = load_archive_signing_key_by_id(state, key_id).await?;
                signing_keys.insert(key_id, signing.clone());
                signing
            };
            signing.verify_exact_signature(&jcs, signature)?;
            let ledger_hash = hash_domain_payload(ARCHIVE_LEDGER_DOMAIN, &jcs);
            if sequence != expected_sequence
                || epoch_id != sequence
                || previous != previous_hash
                || tenant != tenant_id
                || row_region != region
                || row.get("ledger_sequence").and_then(Value::as_u64) != Some(sequence)
                || row.get("epoch_id").and_then(Value::as_u64) != Some(epoch_id)
                || row.get("previous_ledger_hash").and_then(Value::as_str) != Some(previous)
                || row.get("ledger_hash").and_then(Value::as_str) != Some(ledger_hash.as_str())
                || row.get("signing_key_id").and_then(Value::as_u64) != Some(key_id)
            {
                return Err("B054 archive ledger index/contiguity/hash mismatch".to_owned());
            }
            let epoch = parsed.epoch()?;
            previous_hash = ledger_hash.clone();
            selected = Some((
                epoch,
                AuthenticatedLedgerAnchor {
                    tenant_id: tenant_id.to_owned(),
                    region: region.to_owned(),
                    epoch_id,
                    ledger_sequence: sequence,
                    ledger_hash,
                },
                signing,
            ));
            expected_sequence = expected_sequence
                .checked_add(1)
                .ok_or("B054 archive ledger sequence overflow")?;
        }
    }
    selected.ok_or("B054 archive authenticated ledger is empty".to_owned())
}

async fn authenticate_witnessed_audit_head(
    state: &AuditArchiveState,
    signing_keys: &mut BTreeMap<u64, B054ArchiveSigningKey>,
    anchor: &B054WitnessAnchor,
) -> Result<(), String> {
    let signing = if let Some(signing) = signing_keys.get(&anchor.signing_key_id) {
        signing.clone()
    } else {
        let signing = load_archive_signing_key_by_id(state, anchor.signing_key_id).await?;
        signing_keys.insert(anchor.signing_key_id, signing.clone());
        signing
    };
    signing.verify_exact_signature(&anchor.head_jcs, &anchor.head_signature_b64)
}

async fn load_witnessed_archive_boundary(
    state: &AuditArchiveState,
    tenant_id: &str,
    region: &str,
    epoch_id: u64,
    ledger: &AuthenticatedLedgerAnchor,
    segment: &[SealedArchiveLine],
) -> Result<AuthenticatedWitnessAnchor, String> {
    let client = state
        .witness
        .as_deref()
        .ok_or("B054 archive witness client unavailable")?;
    // The challenged external latest fixes the upper bound. Mutable D1 cannot
    // choose a convenient historical suffix or omit a witnessed receipt.
    let latest = client
        .latest(tenant_id, region)
        .await?
        .ok_or("B054 archive witness latest is empty")?;
    let latest_anchor = latest.archive_anchor()?;
    let mut signing_keys = BTreeMap::<u64, B054ArchiveSigningKey>::new();
    authenticate_witnessed_audit_head(state, &mut signing_keys, &latest_anchor).await?;
    let mut cursor: i64 = -1;
    let mut prior_sequence: Option<u64> = None;
    let mut prior_hash: Option<String> = None;
    let mut historical_latest = None;
    let mut selected = None;
    let first_sequence = segment
        .first()
        .map(|line| line.sequence_number)
        .ok_or("B054 archive witness selection received empty segment")?;
    let loaded_end = segment
        .last()
        .and_then(|line| line.sequence_number.checked_add(1))
        .ok_or("B054 archive loaded segment end overflow")?;
    loop {
        let page = state
            .d1
            .query(
                "SELECT witness_jcs,receipt_jcs,receipt_signature_b64,witness_key_id,witness_record_hash,witness_sequence FROM audit_chain_witness_receipt WHERE tenant_id=?1 AND region=?2 AND witness_sequence>?3 AND witness_sequence<=?4 ORDER BY witness_sequence LIMIT 256",
                &[json!(tenant_id), json!(region), json!(cursor), json!(latest_anchor.witness_sequence)],
            )
            .await?;
        if page.is_empty() {
            break;
        }
        let mut stored_page = Vec::with_capacity(page.len());
        for row in &page {
            let witness_sequence = row
                .get("witness_sequence")
                .and_then(Value::as_i64)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or("B054 archive witness sequence invalid")?;
            stored_page.push(B054StoredWitnessReceiptInput {
                witness_jcs: archive_d1_blob(
                    row.get("witness_jcs")
                        .ok_or("B054 archive witness JCS missing")?,
                    "witness JCS",
                    32 * 1024,
                )?,
                receipt_jcs: archive_d1_blob(
                    row.get("receipt_jcs")
                        .ok_or("B054 archive receipt JCS missing")?,
                    "witness receipt JCS",
                    32 * 1024,
                )?,
                receipt_signature_b64: row
                    .get("receipt_signature_b64")
                    .and_then(Value::as_str)
                    .ok_or("B054 archive witness receipt signature missing")?
                    .to_owned(),
                witness_key_id: row
                    .get("witness_key_id")
                    .and_then(Value::as_u64)
                    .ok_or("B054 archive witness key id invalid")?,
                witness_record_hash: row
                    .get("witness_record_hash")
                    .and_then(Value::as_str)
                    .ok_or("B054 archive witness record hash missing")?
                    .to_owned(),
                witness_sequence,
            });
        }
        for verified in client.verify_historical_receipt_chain(stored_page, tenant_id, region)? {
            let anchor = verified.archive_anchor()?;
            authenticate_witnessed_audit_head(state, &mut signing_keys, &anchor).await?;
            if anchor.tenant_id != tenant_id || anchor.region != region {
                return Err("B054 archive witness history crosses partitions".to_owned());
            }
            match (prior_sequence, prior_hash.as_deref()) {
                (None, None)
                    if anchor.witness_sequence == 0
                        && anchor.previous_witness_hash == "0".repeat(64) => {}
                (Some(sequence), Some(hash))
                    if sequence.checked_add(1) == Some(anchor.witness_sequence)
                        && anchor.previous_witness_hash == hash => {}
                _ => return Err("B054 archive witness history is not contiguous".to_owned()),
            }
            prior_sequence = Some(anchor.witness_sequence);
            prior_hash = Some(anchor.witness_record_hash.clone());
            if anchor.epoch_id == epoch_id
                && anchor.epoch_ledger_sequence == ledger.ledger_sequence
                && anchor.epoch_ledger_hash == ledger.ledger_hash
                && anchor.head_next_sequence > first_sequence
                && anchor.head_next_sequence <= loaded_end
            {
                selected = Some(AuthenticatedWitnessAnchor {
                    tenant_id: anchor.tenant_id.clone(),
                    region: anchor.region.clone(),
                    witness_sequence: anchor.witness_sequence,
                    witness_record_hash: anchor.witness_record_hash.clone(),
                    head_hash: anchor.head_hash.clone(),
                    head_next_sequence: anchor.head_next_sequence,
                    epoch_id: anchor.epoch_id,
                    epoch_ledger_sequence: anchor.epoch_ledger_sequence,
                    epoch_ledger_hash: anchor.epoch_ledger_hash.clone(),
                });
            }
            cursor = i64::try_from(anchor.witness_sequence)
                .map_err(|_| "B054 archive witness cursor exceeds i64")?;
            historical_latest = Some(anchor);
        }
        if page.len() < 256 {
            break;
        }
    }
    if historical_latest.as_ref() != Some(&latest_anchor) {
        return Err(
            "B054 archive witness history does not span genesis through challenged latest"
                .to_owned(),
        );
    }
    let selected = selected
        .ok_or("B054 archive loaded segment contains no authenticated witnessed end boundary")?;
    let expected_head = segment
        .iter()
        .find(|line| line.sequence_number.checked_add(1) == Some(selected.head_next_sequence))
        .map(|line| line.chain_hash.as_str());
    if expected_head != Some(selected.head_hash.as_str()) {
        return Err("B054 archive witnessed boundary hash differs from loaded rows".to_owned());
    }
    Ok(selected)
}

async fn finalize_signed_archive(
    state: &AuditArchiveState,
    archive: &PreparedSignedArchive,
    lines: &[SealedArchiveLine],
    published_at: i64,
) -> Result<(), String> {
    let index = &archive.index;
    let row_facts = serde_json::to_string(
        &lines
            .iter()
            .map(|line| {
                json!({
                    "algorithm_id": line.algorithm_id,
                    "canonical_jcs": line.canonical_jcs,
                    "chain_hash": line.chain_hash,
                "epoch_id": line.epoch_id,
                "enqueued_at": line.enqueued_at_ms,
                    "id": line.row_id,
                    "link_key_id": line.link_key_id,
                    "prev_hash": line.prev_hash,
                    "sequence_number": line.sequence_number,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|_| "B054 archive row-id encoding failed")?;
    let exact = "tenant_id=?1 AND region=?2 AND epoch_id=?3 AND start_sequence=?4 AND end_sequence_exclusive=?5 AND record_count=?6 AND is_empty=0 AND algorithm_id=?7 AND link_key_id=?8 AND start_prev_hash=?9 AND end_head_hash=?10 AND end_head_witness_sequence=?11 AND end_head_witness_hash=?12 AND epoch_ledger_sequence=?13 AND epoch_ledger_hash=?14 AND manifest_version=1 AND manifest_hash=?15 AND manifest_jcs=CAST(?16 AS BLOB) AND signature_b64=?17 AND signing_key_id=?18";
    let params = vec![
        json!(index.tenant_id),
        json!(index.region),
        json!(index.epoch_id),
        json!(index.start_sequence),
        json!(index.end_sequence_exclusive),
        json!(index.record_count),
        json!(index.algorithm_id),
        json!(index.link_key_id),
        json!(index.start_prev_hash),
        json!(index.end_head_hash),
        json!(index.end_head_witness_sequence),
        json!(index.end_head_witness_hash),
        json!(index.epoch_ledger_sequence),
        json!(index.epoch_ledger_hash),
        json!(index.manifest_hash),
        Value::String(
            String::from_utf8(archive.manifest_jcs.clone())
                .map_err(|_| "manifest JCS not UTF-8")?,
        ),
        json!(archive.signature_b64),
        json!(index.signing_key_id),
        json!(published_at),
    ];
    let mut statements = vec![D1BatchStatement::new(
        format!("INSERT INTO audit_chain_archive_manifest (tenant_id,region,epoch_id,start_sequence,end_sequence_exclusive,record_count,is_empty,algorithm_id,link_key_id,start_prev_hash,end_head_hash,end_head_witness_sequence,end_head_witness_hash,epoch_ledger_sequence,epoch_ledger_hash,manifest_version,manifest_hash,manifest_jcs,signature_b64,signing_key_id,published_at_ms) SELECT ?1,?2,?3,?4,?5,?6,0,?7,?8,?9,?10,?11,?12,?13,?14,1,?15,CAST(?16 AS BLOB),?17,?18,?19 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_archive_manifest WHERE {exact})"),
        params.clone(),
    )];
    statements.push(D1BatchStatement::new(
        "UPDATE audit_outbox SET archived_at=?1 WHERE id IN (SELECT CAST(json_extract(value,'$.id') AS TEXT) FROM json_each(?2)) AND tenant_id=?3 AND region=?4 AND epoch_id=?5 AND sequence_number>=?6 AND sequence_number<?7 AND emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL",
        vec![json!(published_at), Value::String(row_facts.clone()), json!(index.tenant_id), json!(index.region), json!(index.epoch_id), json!(index.start_sequence), json!(index.end_sequence_exclusive)],
    ));
    let commit_id = format!("b054-archive-{}", index.manifest_hash);
    statements.push(D1BatchStatement::new(
        format!("INSERT INTO audit_chain_v2_tx_assert(commit_id,assertion) SELECT ?20,CASE WHEN EXISTS (SELECT 1 FROM audit_chain_archive_manifest WHERE {exact}) AND (SELECT COUNT(*) FROM json_each(?21))=?6 AND (SELECT COUNT(DISTINCT CAST(json_extract(value,'$.id') AS TEXT)) FROM json_each(?21))=?6 AND (SELECT COUNT(*) FROM audit_outbox WHERE tenant_id=?1 AND region=?2 AND epoch_id=?3 AND sequence_number>=?4 AND sequence_number<?5)=?6 AND NOT EXISTS (SELECT 1 FROM json_each(?21) f LEFT JOIN audit_outbox o ON o.id=CAST(json_extract(f.value,'$.id') AS TEXT) WHERE o.id IS NULL OR o.tenant_id<>?1 OR o.region<>?2 OR o.sequence_number<>CAST(json_extract(f.value,'$.sequence_number') AS INTEGER) OR o.enqueued_at<>CAST(json_extract(f.value,'$.enqueued_at') AS INTEGER) OR o.prev_hash<>json_extract(f.value,'$.prev_hash') OR o.chain_hash<>json_extract(f.value,'$.chain_hash') OR o.canonical_jcs<>json_extract(f.value,'$.canonical_jcs') OR o.algorithm_id<>CAST(json_extract(f.value,'$.algorithm_id') AS INTEGER) OR o.epoch_id<>CAST(json_extract(f.value,'$.epoch_id') AS INTEGER) OR o.link_key_id<>CAST(json_extract(f.value,'$.link_key_id') AS INTEGER) OR o.emitted_at IS NULL OR o.archived_at IS NULL OR o.quarantined_at IS NOT NULL) THEN 1 ELSE 0 END"),
        params.into_iter().chain([json!(commit_id), Value::String(row_facts)]).collect(),
    ));
    statements.push(D1BatchStatement::new(
        "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1",
        vec![json!(commit_id)],
    ));
    state
        .d1
        .batch(statements)
        .await
        .map(|_| ())
        .map_err(|error| {
            format!(
                "B054 archive exact D1 transaction rolled back at {:?}: {}",
                error.statement, error.message
            )
        })
}

/// B054_ARCHIVE_LIVE_BOUNDARY
async fn archive_keyed_partition(
    state: &AuditArchiveState,
    segment: &[SealedArchiveLine],
    now: i64,
) -> Result<PartitionOutcome, String> {
    let first = segment
        .first()
        .ok_or("B054 keyed archive received an empty segment")?;
    let link_key_id = first
        .link_key_id
        .ok_or("B054 keyed archive row has no link key id")?;

    // B054_ARCHIVE_AUTH_TRUST_ROOT
    let signer = state
        .archive_signer
        .as_deref()
        .ok_or("B054 archive manifest signer unavailable")?;
    let manifest_signing = load_archive_signing_key_by_id(state, signer.key_id()).await?;
    // B054_ARCHIVE_AUTH_LEDGER_CONTIGUOUS
    let (epoch, ledger, epoch_signing) =
        load_authenticated_archive_ledger(state, &first.tenant_id, &first.region, first.epoch_id)
            .await?;
    let commitment = load_archive_link_commitment(state, &epoch_signing, link_key_id).await?;
    // B054_ARCHIVE_AUTH_LEDGER_HASH
    if epoch.algorithm().id() != first.algorithm_id || epoch.link_key_id() != first.link_key_id {
        return Err("B054 keyed rows differ from authenticated epoch ledger".to_owned());
    }
    let keyring = state
        .link_keyring
        .as_deref()
        .ok_or("B054 keyed archive link keyring unavailable")?;
    let link_key = keyring
        .get(link_key_id)
        .ok_or("B054 keyed archive configured link key unavailable")?;
    if !key_matches_commitment(link_key, &commitment) {
        return Err("B054 keyed archive link key differs from authenticated commitment".to_owned());
    }
    let (verified_prefix, brk) = split_verifying_prefix_for_epoch(segment, &epoch, Some(link_key));
    if brk.is_some() || verified_prefix.len() != segment.len() {
        return Err(
            "B054 keyed archive chain gap/hash mismatch; rows remain unarchived and unquarantined"
                .to_owned(),
        );
    }
    // B054_ARCHIVE_AUTH_WITNESS_CONTIGUOUS
    let witness = load_witnessed_archive_boundary(
        state,
        &first.tenant_id,
        &first.region,
        first.epoch_id,
        &ledger,
        segment,
    )
    .await?;
    // B054_ARCHIVE_AUTH_WITNESS_LATEST
    let selected_len = witness
        .head_next_sequence
        .checked_sub(first.sequence_number)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or("B054 witnessed archive range underflow/overflow")?;
    let selected = segment
        .get(..selected_len)
        .ok_or("B054 witnessed archive range exceeds loaded segment")?;
    // B054_ARCHIVE_MANIFEST_OBJECT_REQUIRED
    let objects = prepare_archive_objects(
        selected,
        &epoch,
        Some(link_key),
        state.max_lines_per_chunk,
        state.max_bytes_per_chunk,
    )?;
    // B054_ARCHIVE_MANIFEST_OBJECT_HASH
    if objects.is_empty() {
        return Err("B054 non-empty witnessed range produced no archive objects".to_owned());
    }
    // B054_ARCHIVE_MANIFEST_OBJECT_RANGE
    // B054_ARCHIVE_MANIFEST_OBJECT_EXTRA
    let archive = prepare_signed_archive(
        selected,
        objects,
        &epoch,
        &ledger,
        &witness,
        manifest_signing.key_id(),
        |payload| signer.sign_exact(payload, &first.region),
    )?;
    manifest_signing.verify_exact_signature(&archive.manifest_jcs, &archive.signature_b64)?;
    let authenticated_manifest = authenticate_archive_manifest(
        &archive.index,
        &archive.manifest_jcs,
        &archive.signature_b64,
        &manifest_signing,
        &ledger,
        &witness,
        &archive.objects,
        &epoch,
        Some(link_key),
        Some(&commitment),
    )?;
    let authenticated_epoch = AuthenticatedArchiveEpoch {
        epoch: epoch.clone(),
        key_commitment: commitment,
        manifest: authenticated_manifest,
    };
    let authenticated_context = context_from_authenticated_epoch(&authenticated_epoch, keyring)?;
    if authenticated_context.epoch != epoch
        || authenticated_context.link_key.map(LinkKey::as_bytes) != Some(link_key.as_bytes())
    {
        return Err("B054 authenticated archive context drifted before publication".to_owned());
    }

    // B054_ARCHIVE_R2_WRITE_BOUNDARY
    let mut created = 0_u64;
    let mut present = 0_u64;
    for object in &archive.objects {
        if put_exact_object_last(
            &state.r2,
            &object.manifest.object_key,
            &object.bytes,
            u64::try_from(state.max_bytes_per_chunk).unwrap_or(u64::MAX),
        )
        .await?
        {
            created = created.saturating_add(1);
        } else {
            present = present.saturating_add(1);
        }
    }
    // The signed, content-addressed manifest is always published LAST.
    if put_exact_object_last(
        &state.r2,
        &archive.envelope_key,
        &archive.envelope_bytes,
        u64::try_from(MAX_ARCHIVE_MANIFEST_ENVELOPE_BYTES).unwrap_or(u64::MAX),
    )
    .await?
    {
        created = created.saturating_add(1);
    } else {
        present = present.saturating_add(1);
    }

    // B054_ARCHIVE_D1_EXACT_CAS
    finalize_signed_archive(state, &archive, selected, now).await?;
    Ok(PartitionOutcome {
        rows: u64::try_from(selected.len()).unwrap_or(u64::MAX),
        chunks_created: created,
        chunks_already_present: present,
        rows_quarantined: 0,
        truncated: selected.len() < segment.len(),
    })
}

fn archive_epoch_context<'a>(
    state: &'a AuditArchiveState,
    first: &SealedArchiveLine,
) -> Result<ArchiveEpochContext<'a>, String> {
    if first.algorithm_id == 0 && first.epoch_id == 0 && first.link_key_id.is_none() {
        return Ok(ArchiveEpochContext {
            epoch: ChainEpoch::legacy(),
            link_key: None,
        });
    }

    let _ = state;
    Err("non-legacy archive segment reached the legacy-only path".to_owned())
}

async fn archive_partition(
    state: &AuditArchiveState,
    tenant_id: &str,
    region: &str,
    limit: i64,
    now: i64,
) -> Result<PartitionOutcome, String> {
    let lines = read_unarchived_rows(&state.d1, tenant_id, region, limit).await?;
    if lines.is_empty() {
        return Ok(PartitionOutcome::default());
    }
    // Never mix epoch algorithms in one verification or immutable object. A
    // later epoch remains untouched until this segment is durably archived;
    // missing or invalid authenticated evidence fails closed and leaves rows
    // unmodified.
    let segment = first_epoch_segment(&lines);
    let first = segment
        .first()
        .ok_or("archive epoch segmentation produced an empty segment")?;
    if first.algorithm_id == 1 {
        let mut outcome = archive_keyed_partition(state, segment, now).await?;
        outcome.truncated = outcome.truncated
            || i64::try_from(lines.len()).unwrap_or(i64::MAX) >= limit
            || segment.len() < lines.len();
        return Ok(outcome);
    }
    let context = archive_epoch_context(state, first)?;
    let truncated =
        i64::try_from(lines.len()).unwrap_or(i64::MAX) >= limit || segment.len() < lines.len();
    // The ordered batch is NOT necessarily chain-contiguous (the 2026-08-14
    // fork). Archive the longest verifying prefix; the break, if any, tells us
    // exactly where the quarantine starts.
    let (prefix, brk) = split_verifying_prefix_for_epoch(segment, &context.epoch, context.link_key);
    let mut outcome = PartitionOutcome {
        truncated,
        ..PartitionOutcome::default()
    };
    let mut offset = 0usize;
    while offset < prefix.len() {
        let Some(remaining) = prefix.get(offset..) else {
            break;
        };
        let chunks = split_into_chunks(
            remaining.to_vec(),
            state.max_lines_per_chunk,
            state.max_bytes_per_chunk,
        );
        let Some(chunk) = chunks.first() else {
            break;
        };
        // `split_into_chunks` never emits an empty chunk, but the key derives
        // from the FIRST line and an empty chunk would have no key at all --
        // ask for the line rather than index and assume.
        let Some(first) = chunk.first() else {
            continue;
        };
        // `serialize_chunk_for_epoch` re-verifies every link from persisted bytes
        // before producing any output, so a chain break in D1 stops the archive
        // here instead of being copied offsite as if it were evidence.
        let body = serialize_chunk_for_epoch(chunk, &context.epoch, context.link_key)
            .map_err(|e| e.to_string())?;
        let key = sealed_chunk_key(first);
        match put_chunk_if_absent(&state.r2, &key, &body, &context.epoch, context.link_key).await? {
            ChunkWrite::Created => {
                outcome.chunks_created = outcome.chunks_created.saturating_add(1)
            }
            ChunkWrite::AlreadyIdentical => {
                outcome.chunks_already_present = outcome.chunks_already_present.saturating_add(1);
            }
            ChunkWrite::ExistingPrefix(rows) => {
                if rows == 0 || rows > chunk.len() {
                    return Err(format!(
                        "archive key {key} existing prefix has invalid line count {rows}"
                    ));
                }
                outcome.chunks_already_present = outcome.chunks_already_present.saturating_add(1);
                let Some(existing_prefix) = chunk.get(..rows) else {
                    return Err(format!(
                        "archive key {key} existing prefix has invalid line count {rows}"
                    ));
                };
                mark_archived(&state.d1, existing_prefix, now)
                    .await
                    .map_err(|e| format!("ARCHIVE_D1_MARK: {e}"))?;
                outcome.rows = outcome.rows.saturating_add(rows as u64);
                offset = offset.saturating_add(rows);
                continue;
            }
        }
        // R2 first, D1 second — see the module docs.
        mark_archived(&state.d1, chunk, now)
            .await
            .map_err(|e| format!("ARCHIVE_D1_MARK: {e}"))?;
        outcome.rows = outcome.rows.saturating_add(chunk.len() as u64);
        offset = offset.saturating_add(chunk.len());
    }
    // Quarantine LAST, and only after every prefix chunk is durable in R2 and
    // marked. A `?` above returns before this line, so a partial R2 failure
    // leaves the tail un-quarantined and retryable — the archive's ordering
    // rule (R2 first, D1 bookkeeping second) applied to the second kind of
    // bookkeeping.
    if let Some(brk) = brk {
        let through_sequence = segment
            .last()
            .map(|line| line.sequence_number)
            .ok_or("archive quarantine refuses an empty epoch segment")?;
        outcome.rows_quarantined = quarantine_tail(
            &state.d1,
            tenant_id,
            region,
            ArchiveEpochIdentity::from(first),
            QuarantineBounds {
                from_sequence: brk.sequence_number,
                through_sequence,
            },
            &brk.reason_code(),
            now,
        )
        .await?;
    }
    Ok(outcome)
}

/// Only these fixed labels cross the container/Worker boundary. The detailed
/// error may contain an immutable object key or provider response, so it stays
/// in the container log and must never be copied into the cron response.
fn archive_failure_code(error: &str) -> &'static str {
    if error.starts_with("ARCHIVE_D1_MARK: ") {
        "d1_mark"
    } else if error.starts_with("AUDIT_ARCHIVE_KEY_CONFLICT") {
        "immutable_key_conflict"
    } else if error.starts_with("R2 conditional put failed") {
        "r2_conditional_put"
    } else if error.starts_with("R2 get failed")
        || error.starts_with("R2 body read failed")
        || error.starts_with("R2 head failed")
    {
        "r2_read"
    } else if error.starts_with("archive key ") {
        "r2_consistency"
    } else if error.starts_with("archive watermark") {
        "d1_mark"
    } else if error.starts_with("D1 ") {
        "d1_query"
    } else if error.starts_with("audit_outbox.") {
        "row_decode"
    } else if error.starts_with("B054 ") {
        "epoch_evidence"
    } else if error.starts_with("archive candidate") {
        "candidate_invalid"
    } else {
        "unclassified"
    }
}

async fn handle_archive(State(state): State<AuditArchiveState>, headers: HeaderMap) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let partitions = match read_unarchived_partitions(&state.d1).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "audit/archive: partition scan failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "archive scan failed").into_response();
        }
    };
    let now = now_ms();
    let mut rows_archived: u64 = 0;
    let mut chunks_created: u64 = 0;
    let mut chunks_already_present: u64 = 0;
    let mut partitions_archived: u64 = 0;
    let mut partitions_failed: u64 = 0;
    let mut failure_codes = BTreeSet::new();
    let mut rows_quarantined: u64 = 0;
    let mut partitions_quarantined: u64 = 0;
    let mut incomplete = false;

    for (tenant_id, region) in partitions {
        match archive_partition(&state, &tenant_id, &region, state.batch_limit, now).await {
            Ok(o) => {
                if o.rows > 0 {
                    partitions_archived = partitions_archived.saturating_add(1);
                }
                rows_archived = rows_archived.saturating_add(o.rows);
                chunks_created = chunks_created.saturating_add(o.chunks_created);
                chunks_already_present =
                    chunks_already_present.saturating_add(o.chunks_already_present);
                if o.rows_quarantined > 0 {
                    partitions_quarantined = partitions_quarantined.saturating_add(1);
                    rows_quarantined = rows_quarantined.saturating_add(o.rows_quarantined);
                }
                if o.truncated {
                    incomplete = true;
                }
            }
            Err(e) => {
                // One partition's failure must not abort the sweep — the other
                // partitions' rows are independently archivable, and stopping
                // would let a single bad partition hold the whole estate's
                // compliance copy hostage. The call still reports failure.
                partitions_failed = partitions_failed.saturating_add(1);
                let failure_code = archive_failure_code(&e);
                failure_codes.insert(failure_code);
                tracing::error!(
                    error = %e,
                    failure_code,
                    region = %region,
                    "audit/archive: partition failed — rows left UNARCHIVED"
                );
            }
        }
    }

    let body = json!({
        "bucket": state.bucket,
        "rows_archived": rows_archived,
        "chunks_created": chunks_created,
        "chunks_already_present": chunks_already_present,
        "partitions_archived": partitions_archived,
        "partitions_failed": partitions_failed,
        "failure_codes": failure_codes,
        // Quarantine is reported, never merely logged. A row that is
        // unarchivable BY DESIGN still has to be counted somewhere an operator
        // reads, or it becomes the silent backlog this endpoint replaced.
        "rows_quarantined": rows_quarantined,
        "partitions_quarantined": partitions_quarantined,
        "incomplete": incomplete,
    });
    // A partial failure is NOT a 200. The cron logs the status; reporting
    // success while rows sit unarchived is exactly the silent-success failure
    // this endpoint exists to eliminate.
    let status = if partitions_failed > 0 {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    };
    (status, Json(body)).into_response()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use ed25519_dalek::Signer as _;

    #[test]
    fn archive_failure_codes_are_fixed_and_never_reflect_keys_or_provider_text() {
        let cases = [
            (
                "AUDIT_ARCHIVE_KEY_CONFLICT key=secret",
                "immutable_key_conflict",
            ),
            (
                "R2 conditional put failed for key secret: 403",
                "r2_conditional_put",
            ),
            ("R2 get failed for key secret: 403", "r2_read"),
            (
                "archive key secret rejected create-if-absent",
                "r2_consistency",
            ),
            (
                "archive watermark returned 0 rows for 4 requested rows",
                "d1_mark",
            ),
            ("ARCHIVE_D1_MARK: D1 HTTP 403: secret", "d1_mark"),
            ("D1 HTTP 403: secret", "d1_query"),
            ("audit_outbox.id missing/non-text", "row_decode"),
            ("B054 archive witness unavailable", "epoch_evidence"),
            (
                "archive candidate for key=secret failed self-parse",
                "candidate_invalid",
            ),
            ("secret-without-known-prefix", "unclassified"),
        ];
        for (detail, expected) in cases {
            let code = archive_failure_code(detail);
            assert_eq!(code, expected);
            assert!(!code.contains("secret"));
        }
    }

    fn verifying_lines(count: u64) -> Vec<SealedArchiveLine> {
        let mut head = corelink_audit_chain::ChainHash::genesis();
        (0..count)
            .map(|sequence_number| {
                let canonical_jcs = format!(r#"{{"n":{sequence_number}}}"#);
                let chain_hash = corelink_audit_chain::link_chain_hash_from_canonical(
                    &head,
                    canonical_jcs.as_bytes(),
                );
                let line = SealedArchiveLine {
                    schema: SEALED_LINE_SCHEMA.to_owned(),
                    algorithm_id: 0,
                    epoch_id: 0,
                    link_key_id: None,
                    tenant_id: "00000000-0000-7000-8000-00000000aaaa".to_owned(),
                    region: "enam".to_owned(),
                    sequence_number,
                    prev_hash: head.to_hex(),
                    chain_hash: chain_hash.to_hex(),
                    enqueued_at_ms: 1_787_824_088_488,
                    row_id: format!("row-{sequence_number}"),
                    canonical_jcs,
                };
                head = chain_hash;
                line
            })
            .collect()
    }

    fn headers_with(key: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            INTERNAL_AUTH_HEADER,
            HeaderValue::from_str(key).expect("test key is a valid header value"),
        );
        h
    }

    #[test]
    fn auth_accepts_the_exact_key_only() {
        let expected = "k".repeat(32);
        assert!(internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&expected)
        ));
        assert!(!internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&"k".repeat(31))
        ));
        assert!(!internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&format!("{expected}x"))
        ));
        assert!(!internal_auth_ok(expected.as_bytes(), &HeaderMap::new()));
    }

    #[test]
    fn sealed_metadata_parser_is_fail_closed() {
        let legacy = json!({"algorithm_id": null, "epoch_id": null, "link_key_id": null});
        assert_eq!(parse_sealed_epoch_metadata(&legacy).unwrap(), (0, 0, None));
        let explicit_e0 = json!({"algorithm_id": 0, "epoch_id": 0, "link_key_id": null});
        assert_eq!(
            parse_sealed_epoch_metadata(&explicit_e0).unwrap(),
            (0, 0, None)
        );
        let keyed = json!({"algorithm_id": 1, "epoch_id": 1, "link_key_id": 7});
        assert_eq!(
            parse_sealed_epoch_metadata(&keyed).unwrap(),
            (1, 1, Some(7))
        );

        for partial in [
            json!({"algorithm_id": 0, "epoch_id": null, "link_key_id": null}),
            json!({"algorithm_id": null, "epoch_id": 0, "link_key_id": null}),
            json!({"algorithm_id": 1, "epoch_id": 1, "link_key_id": null}),
            json!({"algorithm_id": 1, "epoch_id": 0, "link_key_id": 7}),
            json!({"algorithm_id": 0, "epoch_id": 0, "link_key_id": 7}),
            json!({"algorithm_id": 1, "epoch_id": -1, "link_key_id": 7}),
            json!({"algorithm_id": 1, "epoch_id": 1, "link_key_id": 0}),
        ] {
            assert!(
                parse_sealed_epoch_metadata(&partial).is_err(),
                "partial/downgrade metadata must not become E0: {partial}"
            );
        }
    }

    #[test]
    fn archive_work_is_split_at_the_first_epoch_boundary() {
        let mut lines = verifying_lines(3);
        let keyed = lines.get_mut(2).expect("test fixture has three lines");
        keyed.schema = SEALED_LINE_SCHEMA_V2.to_owned();
        keyed.algorithm_id = 1;
        keyed.epoch_id = 1;
        keyed.link_key_id = Some(7);

        let segment = first_epoch_segment(&lines);
        assert_eq!(segment.len(), 2);
        assert!(segment.iter().all(|line| line.algorithm_id == 0));
    }

    #[test]
    fn authenticated_epoch_context_requires_the_committed_key() {
        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "11".repeat(32)))
            .expect("test keyring");
        let epoch = ChainEpoch::keyed_successor(1, 7, 2, ChainHash::genesis(), 0)
            .expect("valid keyed successor");
        let commitment = keyring.get(7).expect("configured key").commitment();
        let manifest = AuthenticatedArchiveManifest {
            manifest_hash: "22".repeat(32),
            epoch_id: 1,
            algorithm_id: 1,
            link_key_id: Some(7),
            key_commitment: Some(commitment),
        };
        let matching = AuthenticatedArchiveEpoch {
            epoch: epoch.clone(),
            key_commitment: commitment,
            manifest: manifest.clone(),
        };
        let context = context_from_authenticated_epoch(&matching, &keyring)
            .expect("commitment must bind configured key");
        assert_eq!(context.epoch, epoch);
        assert!(context.link_key.is_some());

        let mismatched_commitment = ChainHash::genesis();
        let mismatched = AuthenticatedArchiveEpoch {
            epoch,
            key_commitment: mismatched_commitment,
            manifest: AuthenticatedArchiveManifest {
                key_commitment: Some(mismatched_commitment),
                ..manifest
            },
        };
        assert!(context_from_authenticated_epoch(&mismatched, &keyring).is_err());
    }

    #[test]
    fn signed_manifest_binds_index_witness_ledger_and_exact_object_bytes() {
        let lines = verifying_lines(2);
        let body = serialize_chunk_for_epoch(&lines, &ChainEpoch::legacy(), None)
            .expect("valid archive body");
        let object_key = sealed_chunk_key(lines.first().expect("fixture line"));
        let signer = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
        let signing_key_id = 9;
        let end_head_hash = lines.last().expect("fixture line").chain_hash.clone();
        let witness_hash = "33".repeat(32);
        let ledger_hash = "44".repeat(32);
        let manifest = ArchiveManifestJcs {
            algorithm_id: 0,
            end_head_hash: end_head_hash.clone(),
            end_head_witness_hash: witness_hash.clone(),
            end_head_witness_sequence: 12,
            end_sequence_exclusive: 2,
            epoch_id: 0,
            epoch_ledger_hash: ledger_hash.clone(),
            epoch_ledger_sequence: 0,
            is_empty: false,
            link_key_id: None,
            manifest_type: "audit-chain-archive-manifest".to_owned(),
            manifest_version: 1,
            objects: vec![ArchiveManifestObjectJcs {
                blake3_hash: hash_domain_payload(&[], &body),
                byte_length: body.len() as u64,
                object_key: object_key.clone(),
                record_count: 2,
                start_sequence: 0,
            }],
            record_count: 2,
            region: "enam".to_owned(),
            signing_key_id,
            start_prev_hash: ChainHash::genesis().to_hex(),
            start_sequence: 0,
            tenant_id: "00000000-0000-7000-8000-00000000aaaa".to_owned(),
        };
        let manifest_jcs = serde_jcs::to_vec(&manifest).expect("manifest JCS");
        let manifest_hash = hash_domain_payload(ARCHIVE_MANIFEST_DOMAIN, &manifest_jcs);
        let signature_b64 =
            base64::engine::general_purpose::STANDARD.encode(signer.sign(&manifest_jcs).to_bytes());
        let index = ArchiveManifestIndex {
            tenant_id: manifest.tenant_id.clone(),
            region: manifest.region.clone(),
            epoch_id: manifest.epoch_id,
            start_sequence: manifest.start_sequence,
            end_sequence_exclusive: manifest.end_sequence_exclusive,
            record_count: manifest.record_count,
            is_empty: manifest.is_empty,
            algorithm_id: manifest.algorithm_id,
            link_key_id: manifest.link_key_id,
            start_prev_hash: manifest.start_prev_hash.clone(),
            end_head_hash: manifest.end_head_hash.clone(),
            end_head_witness_sequence: manifest.end_head_witness_sequence,
            end_head_witness_hash: manifest.end_head_witness_hash.clone(),
            epoch_ledger_sequence: manifest.epoch_ledger_sequence,
            epoch_ledger_hash: manifest.epoch_ledger_hash.clone(),
            manifest_version: manifest.manifest_version,
            manifest_hash,
            signing_key_id,
        };
        let authenticated_signer = AuthenticatedSigningKey {
            key_id: signing_key_id,
            verifying_key: signer.verifying_key(),
        };
        let ledger = AuthenticatedLedgerAnchor {
            tenant_id: "00000000-0000-7000-8000-00000000aaaa".to_owned(),
            region: "enam".to_owned(),
            epoch_id: 0,
            ledger_sequence: 0,
            ledger_hash,
        };
        let witness = AuthenticatedWitnessAnchor {
            tenant_id: "00000000-0000-7000-8000-00000000aaaa".to_owned(),
            region: "enam".to_owned(),
            witness_sequence: 12,
            witness_record_hash: witness_hash,
            head_hash: end_head_hash,
            head_next_sequence: 2,
            epoch_id: 0,
            epoch_ledger_sequence: 0,
            epoch_ledger_hash: "44".repeat(32),
        };
        let object = AuthenticatedArchiveObject {
            object_key,
            bytes: body,
        };
        authenticate_archive_manifest(
            &index,
            &manifest_jcs,
            &signature_b64,
            &authenticated_signer,
            &ledger,
            &witness,
            std::slice::from_ref(&object),
            &ChainEpoch::legacy(),
            None,
            None,
        )
        .expect("all independently authenticated bindings agree");

        let mut altered = object;
        altered.bytes.push(b'\n');
        assert!(authenticate_archive_manifest(
            &index,
            &manifest_jcs,
            &signature_b64,
            &authenticated_signer,
            &ledger,
            &witness,
            &[altered],
            &ChainEpoch::legacy(),
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn existing_object_can_recover_a_legacy_serialized_prefix() {
        let candidate = verifying_lines(3);
        let candidate_prefix = candidate.get(..2).expect("test fixture has two lines");
        let legacy_prefix = candidate_prefix
            .iter()
            .map(|line| {
                json!({
                    "schema": &line.schema,
                    "tenant_id": &line.tenant_id,
                    "region": &line.region,
                    "sequence_number": line.sequence_number,
                    "prev_hash": &line.prev_hash,
                    "chain_hash": &line.chain_hash,
                    "enqueued_at_ms": line.enqueued_at_ms,
                    "row_id": &line.row_id,
                    "canonical_jcs": &line.canonical_jcs,
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let existing = parse_existing_chunk(legacy_prefix.as_bytes(), &ChainEpoch::legacy(), None)
            .expect("legacy object");
        assert_eq!(existing, candidate_prefix);
        assert_eq!(existing_prefix_len(&existing, &candidate), Some(2));

        let candidate_bytes = serialize_chunk_for_epoch(&candidate, &ChainEpoch::legacy(), None)
            .expect("candidate serializes");
        assert_eq!(
            classify_existing_chunk(
                "immutable-key",
                legacy_prefix.as_bytes(),
                &candidate_bytes,
                &ChainEpoch::legacy(),
                None,
            ),
            Ok(ChunkWrite::ExistingPrefix(2)),
            "a legacy WORM prefix must be handled from its GET bytes without relying on a PUT error"
        );
        let legacy_with_terminator = format!("{legacy_prefix}\n");
        assert_eq!(
            classify_existing_chunk(
                "immutable-key",
                legacy_with_terminator.as_bytes(),
                &candidate_bytes,
                &ChainEpoch::legacy(),
                None,
            ),
            Ok(ChunkWrite::ExistingPrefix(2)),
            "one conventional trailing NDJSON newline remains recoverable"
        );
        assert!(
            parse_existing_chunk(
                format!("{legacy_prefix}\n\n").as_bytes(),
                &ChainEpoch::legacy(),
                None,
            )
            .is_err(),
            "two trailing newlines must not weaken the no-empty-lines contract"
        );
        let internal_empty = legacy_prefix.replacen('\n', "\n\n", 1);
        assert!(
            parse_existing_chunk(internal_empty.as_bytes(), &ChainEpoch::legacy(), None).is_err(),
            "an internal empty line is malformed evidence"
        );
    }

    #[test]
    fn existing_object_classification_preserves_idempotency_and_conflict_fail_closed() {
        let candidate = verifying_lines(3);
        let candidate_bytes = serialize_chunk_for_epoch(&candidate, &ChainEpoch::legacy(), None)
            .expect("candidate serializes");
        assert_eq!(
            classify_existing_chunk(
                "immutable-key",
                &candidate_bytes,
                &candidate_bytes,
                &ChainEpoch::legacy(),
                None,
            ),
            Ok(ChunkWrite::AlreadyIdentical)
        );

        let mut divergent = candidate.clone();
        divergent.get_mut(1).expect("second line").row_id = "different-row".to_owned();
        let divergent_bytes = serialize_chunk_for_epoch(&divergent, &ChainEpoch::legacy(), None)
            .expect("divergent chunk still verifies");
        let error = classify_existing_chunk(
            "immutable-key",
            &divergent_bytes,
            &candidate_bytes,
            &ChainEpoch::legacy(),
            None,
        )
        .expect_err("a non-prefix immutable object remains a hard conflict");
        assert!(error.contains("AUDIT_ARCHIVE_KEY_CONFLICT"));

        let complete_with_terminator = format!(
            "{}\n",
            String::from_utf8(candidate_bytes.clone()).expect("archive bytes are UTF-8")
        );
        assert_eq!(
            max_existing_chunk_bytes(&candidate_bytes),
            u64::try_from(candidate_bytes.len())
                .expect("test bytes fit in u64")
                .saturating_add(1),
            "the capped probe must admit one conventional NDJSON terminator"
        );
        assert_eq!(
            classify_existing_chunk(
                "immutable-key",
                complete_with_terminator.as_bytes(),
                &candidate_bytes,
                &ChainEpoch::legacy(),
                None,
            ),
            Ok(ChunkWrite::ExistingPrefix(candidate.len())),
            "a complete immutable object with one trailing newline remains recoverable"
        );
        assert!(
            parse_existing_chunk(
                format!("{complete_with_terminator}\n").as_bytes(),
                &ChainEpoch::legacy(),
                None,
            )
            .is_err(),
            "a second trailing newline remains malformed evidence"
        );
    }

    #[test]
    fn existing_object_must_be_an_exact_nonempty_prefix() {
        let candidate = verifying_lines(3);
        assert_eq!(existing_prefix_len(&[], &candidate), None);
        let candidate_prefix = candidate.get(..2).expect("test fixture has two lines");
        assert_eq!(existing_prefix_len(&candidate, candidate_prefix), None);
        let mut different = candidate_prefix.to_vec();
        different
            .get_mut(1)
            .expect("test fixture has two lines")
            .row_id = "not-the-same-row".to_owned();
        assert_eq!(existing_prefix_len(&different, &candidate), None);
    }

    fn returned_ids(ids: &[&str]) -> Vec<D1Row> {
        ids.iter()
            .map(|id| {
                let mut row = D1Row::new();
                row.insert("id".to_owned(), json!(id));
                row
            })
            .collect()
    }

    #[test]
    fn archive_watermark_accepts_the_exact_returned_id_set() {
        let lines = verifying_lines(2);
        assert!(validate_archived_returned_ids(&lines, &returned_ids(&["row-1", "row-0"])).is_ok());
    }

    #[test]
    fn archive_watermark_rejects_partial_or_unexpected_returned_ids() {
        let lines = verifying_lines(2);
        assert!(validate_archived_returned_ids(&lines, &returned_ids(&["row-0"])).is_err());
        assert!(
            validate_archived_returned_ids(&lines, &returned_ids(&["row-0", "other"])).is_err()
        );
    }

    #[test]
    fn archive_watermark_rejects_duplicate_returned_or_requested_ids() {
        let lines = verifying_lines(2);
        assert!(
            validate_archived_returned_ids(&lines, &returned_ids(&["row-0", "row-0"])).is_err()
        );
        let mut duplicate_input = lines.clone();
        let first_row_id = duplicate_input
            .first()
            .expect("test fixture has rows")
            .row_id
            .clone();
        duplicate_input
            .get_mut(1)
            .expect("test fixture has two rows")
            .row_id = first_row_id;
        assert!(validate_archived_returned_ids(&duplicate_input, &returned_ids(&[])).is_err());
    }

    #[test]
    fn archive_watermark_sql_is_one_bounded_exact_set_update() {
        assert!(MARK_ARCHIVED_SQL.contains("json_each(?2)"));
        assert!(MARK_ARCHIVED_SQL.contains("COUNT(DISTINCT id)"));
        assert!(MARK_ARCHIVED_SQL.contains("CAST(?1 AS INTEGER)"));
        assert!(MARK_ARCHIVED_SQL.contains("emitted_at IS NOT NULL AND archived_at IS NULL"));
        assert!(MARK_ARCHIVED_SQL.contains("RETURNING id"));
    }

    /// The columns this module is ALLOWED to write. Anything else in a `SET`
    /// clause is a re-sequencing bug.
    const WRITABLE_COLUMNS: [&str; 3] = ["archived_at", "quarantined_at", "quarantine_reason"];

    /// The evidence. Rewriting any of these to make the verifier happy destroys
    /// the exact property the audit chain exists to prove.
    const CHAIN_COLUMNS: [&str; 6] = [
        "sequence_number",
        "prev_hash",
        "chain_hash",
        "canonical_jcs",
        "emitted_at",
        "chained_at",
    ];

    #[test]
    fn no_update_ever_touches_a_chain_column() {
        // Reads this module's OWN source and inspects every `UPDATE
        // audit_outbox` statement in it. A unit test over the SQL constants
        // would only prove the statements I remembered to list; scanning the
        // file catches a new one a future change adds.
        //
        // The needle is split so this test does not match itself.
        let needle = concat!("UPDATE ", "audit_outbox");
        let source = include_str!("audit_archive.rs");
        let statements: Vec<&str> = source
            .match_indices(needle)
            .filter_map(|(i, _)| source.get(i..))
            .collect();
        assert_eq!(
            statements.len(),
            3,
            "expected exactly three UPDATE statements (mark_archived, QUARANTINE_SQL, keyed exact CAS); \
             a new one must be reviewed against the immutability rule"
        );
        for stmt in statements {
            let set_clause = stmt
                .split("WHERE")
                .next()
                .expect("split always yields at least one element");
            for chain_column in CHAIN_COLUMNS {
                assert!(
                    !set_clause.contains(chain_column),
                    "an UPDATE writes the chain column `{chain_column}`; \
                     sealed rows are evidence and MUST NOT be re-sequenced or re-hashed. \
                     SET clause: {set_clause}"
                );
            }
            // And positively: every assignment names a permitted column.
            let assignments = set_clause.matches('=').count();
            let permitted: usize = WRITABLE_COLUMNS
                .iter()
                .map(|c| set_clause.matches(c).count())
                .sum();
            assert!(
                assignments <= permitted,
                "an UPDATE assigns something outside {WRITABLE_COLUMNS:?}. SET clause: {set_clause}"
            );
        }
    }

    #[test]
    fn quarantine_sql_writes_only_the_quarantine_columns_and_is_bounded() {
        assert!(QUARANTINE_SQL
            .contains("SET quarantined_at = CAST(?1 AS INTEGER), quarantine_reason = ?2"));
        // Bounded above by the census's MAX: an unbounded `sequence_number >=`
        // would swallow rows sealed a millisecond later, which are exactly the
        // rows resumption depends on.
        assert!(QUARANTINE_SQL.contains("sequence_number <= CAST(?9 AS INTEGER)"));
        assert!(QUARANTINE_SQL.contains("algorithm_id = 1"));
        assert!(QUARANTINE_SQL.contains("algorithm_id IS NULL"));
        // Never re-quarantines and never touches an already-archived row.
        assert!(QUARANTINE_SQL.contains("archived_at IS NULL AND quarantined_at IS NULL"));
    }

    #[test]
    fn the_work_queue_excludes_quarantined_rows() {
        // Both reads must carry the predicate, or the archiver walks a
        // permanently-refused tail on every hourly tick — the exact cost this
        // change removes.
        let source = include_str!("audit_archive.rs");
        // Count in the PRODUCTION half only — this test module quotes the same
        // predicate, and a test that counts its own assertions proves nothing.
        let production = source
            .split("\n#[cfg(test)]\n#[allow(")
            .next()
            .expect("split always yields at least one element");
        let occurrences = production.matches("quarantined_at IS NULL").count();
        assert_eq!(
            occurrences, 5,
            "expected the quarantine predicate in the partition scan, the row read, \
             the census, the legacy UPDATE guard and keyed exact CAS; found {occurrences}"
        );
    }

    #[test]
    fn default_bucket_is_the_provisioned_one() {
        // Guards the regression that kept this control dead: a default naming a
        // bucket that does not exist.
        assert_eq!(DEFAULT_AUDIT_BUCKET, "corelink-audit-weur");
    }
}
