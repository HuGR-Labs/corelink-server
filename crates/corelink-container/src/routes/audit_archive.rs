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
//! - different  → HARD FAILURE, rows are NOT marked. Two different chunks under
//!   one key means either a key collision or a rewritten chain, and both are
//!   incidents. Never "resolve" this by overwriting or by skipping.
//!
//! ## Fail loud, never fail open
//!
//! Every error arm leaves the rows unarchived and surfaces a non-2xx, so the
//! backlog and the cron's own logs both show the gap. Silently reporting
//! success on an unwritten chunk is the failure mode this whole item exists to
//! remove.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use corelink_audit_chain::{
    sealed_chunk_key, serialize_chunk, split_into_chunks, SealedArchiveLine,
    DEFAULT_SEALED_MAX_BYTES_PER_CHUNK, DEFAULT_SEALED_MAX_LINES_PER_CHUNK, SEALED_LINE_SCHEMA,
};

use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

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
    let batch_limit = std::env::var("AUDIT_ARCHIVE_BATCH_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(2000);
    Some(AuditArchiveState {
        internal_auth_key,
        d1,
        r2,
        bucket,
        batch_limit,
        max_lines_per_chunk: DEFAULT_SEALED_MAX_LINES_PER_CHUNK,
        max_bytes_per_chunk: DEFAULT_SEALED_MAX_BYTES_PER_CHUNK,
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
}

/// Every partition that still owns sealed-but-unarchived rows.
async fn read_unarchived_partitions(d1: &D1HttpClient) -> Result<Vec<(String, String)>, String> {
    let rows = d1
        .query(
            "SELECT DISTINCT tenant_id, region FROM audit_outbox \
             WHERE emitted_at IS NOT NULL AND archived_at IS NULL \
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
/// `ORDER BY sequence_number` over the unarchived set yields the ordered PREFIX
/// of what is left, so the batch is always chain-contiguous and the next call
/// resumes exactly where this one stopped.
async fn read_unarchived_rows(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    limit: i64,
) -> Result<Vec<SealedArchiveLine>, String> {
    let rows = d1
        .query(
            "SELECT id, sequence_number, prev_hash, chain_hash, enqueued_at, canonical_jcs \
             FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND archived_at IS NULL \
               AND sequence_number IS NOT NULL AND canonical_jcs IS NOT NULL \
             ORDER BY sequence_number \
             LIMIT ?3",
            &[json!(tenant_id), json!(region), json!(limit)],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
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
        out.push(SealedArchiveLine {
            schema: SEALED_LINE_SCHEMA.to_owned(),
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
) -> Result<ChunkWrite, String> {
    match r2.put_if_absent(key, bytes.to_vec()).await {
        Ok(true) => Ok(ChunkWrite::Created),
        Ok(false) => {
            let existing = r2
                .get(key)
                .await?
                // A key that rejected the conditional PUT but then reads absent
                // is a consistency fault, not an idempotent replay — refuse it
                // rather than mark rows archived against nothing.
                .ok_or_else(|| {
                    format!("archive key {key} rejected create-if-absent but reads absent")
                })?;
            if existing == bytes {
                Ok(ChunkWrite::AlreadyIdentical)
            } else {
                Err(format!(
                    "AUDIT_ARCHIVE_KEY_CONFLICT key={key} existing_bytes={} new_bytes={} — \
                     refusing to overwrite an immutable archive object",
                    existing.len(),
                    bytes.len()
                ))
            }
        }
        Err(e) => Err(e),
    }
}

/// Mark one chunk's rows archived, guarded by `archived_at IS NULL` so a re-run
/// (or a concurrent archiver) never double-counts.
async fn mark_archived(
    d1: &D1HttpClient,
    lines: &[SealedArchiveLine],
    now: i64,
) -> Result<(), String> {
    for line in lines {
        d1.query(
            "UPDATE audit_outbox SET archived_at = ?1 \
             WHERE id = ?2 AND emitted_at IS NOT NULL AND archived_at IS NULL",
            &[json!(now), json!(line.row_id)],
        )
        .await?;
    }
    Ok(())
}

/// Outcome of archiving a single partition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PartitionOutcome {
    rows: u64,
    chunks_created: u64,
    chunks_already_present: u64,
    /// The batch limit truncated this partition's tail.
    truncated: bool,
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
    let truncated = i64::try_from(lines.len()).unwrap_or(i64::MAX) >= limit;
    let chunks = split_into_chunks(lines, state.max_lines_per_chunk, state.max_bytes_per_chunk);
    let mut outcome = PartitionOutcome {
        truncated,
        ..PartitionOutcome::default()
    };
    for chunk in chunks {
        // `split_into_chunks` never emits an empty chunk, but the key derives
        // from the FIRST line and an empty chunk would have no key at all --
        // ask for the line rather than index and assume.
        let Some(first) = chunk.first() else {
            continue;
        };
        // `serialize_chunk` re-verifies every link from the persisted bytes
        // before producing any output, so a chain break in D1 stops the archive
        // here instead of being copied offsite as if it were evidence.
        let body = serialize_chunk(&chunk).map_err(|e| e.to_string())?;
        let key = sealed_chunk_key(first);
        match put_chunk_if_absent(&state.r2, &key, &body).await? {
            ChunkWrite::Created => {
                outcome.chunks_created = outcome.chunks_created.saturating_add(1)
            }
            ChunkWrite::AlreadyIdentical => {
                outcome.chunks_already_present = outcome.chunks_already_present.saturating_add(1);
            }
        }
        // R2 first, D1 second — see the module docs.
        mark_archived(&state.d1, &chunk, now).await?;
        outcome.rows = outcome.rows.saturating_add(chunk.len() as u64);
    }
    Ok(outcome)
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
                tracing::error!(
                    error = %e,
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
    fn default_bucket_is_the_provisioned_one() {
        // Guards the regression that kept this control dead: a default naming a
        // bucket that does not exist.
        assert_eq!(DEFAULT_AUDIT_BUCKET, "corelink-audit-weur");
    }
}
