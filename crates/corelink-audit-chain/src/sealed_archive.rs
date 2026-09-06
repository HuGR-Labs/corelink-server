//! Offsite NDJSON archive of the LIVE sealed audit chain.
//!
//! ## Why this module exists next to [`crate::archive_producer`]
//!
//! [`crate::archive_producer`] archives the crate's own typed
//! [`crate::AuditEvent`] stream. The chain that actually runs in production is
//! NOT that stream: `POST /_internal/audit/drain` seals GENERIC CloudEvents
//! rows out of the D1 `audit_outbox` table and links them with
//! [`crate::link_chain_hash_from_canonical`] over the persisted RFC-8785 JCS
//! bytes, because those payloads are written by several sinks and do not
//! deserialize into `AuditEvent` (their `id` is a composite dedup string, not a
//! UUID). An archive of the live chain therefore cannot be a stream of
//! `AuditEvent` JSON — it has to carry the persisted JCS bytes verbatim, which
//! is exactly what the architecture wiki already describes: "the verifier never
//! re-canonicalizes off the wire archive; it reads the persisted JCS NDJSON
//! bytes and recomputes BLAKE3 directly, so the persisted bytes are
//! load-bearing" (`docs/knowledge/compliance/audit-chain.md`).
//!
//! This module is that line format plus the pure chunking/verification logic
//! around it. The I/O — reading D1, writing R2 — lives in
//! `corelink-container::routes::audit_archive`.
//!
//! ## Line format (`corelink.audit.sealed.v1` / `.v2`)
//!
//! One JSON object per line, no trailing newline on the final line:
//!
//! ```text
//! {"schema":"corelink.audit.sealed.v1","tenant_id":…,"region":…,
//!  "sequence_number":…,"prev_hash":<64-hex>,"chain_hash":<64-hex>,
//!  "enqueued_at_ms":…,"row_id":…,"canonical_jcs":<the exact JCS string>}
//! ```
//!
//! `canonical_jcs` is the byte-for-byte string that was hashed at seal time
//! (`audit_outbox.canonical_jcs`). Re-deriving it is forbidden: JCS-canonical
//! form drift across a `serde_jcs` bump would silently invalidate every link,
//! so the archive carries the bytes rather than a recipe for them.
//!
//! ## Key shape
//!
//! ```text
//! audit/<YYYY>/<MM>/<DD>/<tenant_id>/<region>/<8-digit-sequence>.ndjson
//! ```
//!
//! The `<tenant_id>/<region>` segments are NOT decoration. The live chain is
//! partitioned per `(tenant_id, region)` and each partition numbers its own
//! sequence from 0, so [`crate::archive_chunk_key`]'s
//! `audit/<Y>/<M>/<D>/<seq>.ndjson` shape COLLIDES between any two partitions
//! that seal the same sequence number on the same day. Under a bucket with
//! Object Lock — where a create-if-absent writer treats an existing key as
//! already-archived — that collision would silently drop a whole partition's
//! chunk. Partitioning the key removes the collision by construction.
//!
//! The day slot comes from the FIRST line's `enqueued_at_ms` (when the event
//! happened), never from wall-clock-now, so a backfill lands each chunk in the
//! day it belongs to and re-running the archiver is stable.
//!
//! The daily verifier lists `audit/<YYYY>/<MM>/<DD>/` with no delimiter, so the
//! deeper key still appears in that listing.
//!
//! ## Fail-CLOSED
//!
//! [`serialize_chunk`] re-verifies every link it is about to write —
//! contiguous sequence numbers, `prev_hash` matching the preceding
//! `chain_hash`, and `BLAKE3(prev_hash || canonical_jcs) == chain_hash` — and
//! refuses to produce bytes otherwise. The archive is evidence; writing a chunk
//! that does not verify would manufacture a chain break in the offsite copy of
//! a chain that is intact in D1 (or, worse, launder one that is not).
//!
//! ## Partial progress — [`split_verifying_prefix`]
//!
//! Fail-CLOSED per CHUNK is correct. Fail-CLOSED per PARTITION was not: a
//! partition that carries ONE historical chain break could never archive ANY of
//! its rows, so it retried on every hourly tick forever and nothing said so.
//! (Measured 2026-08-14: a 17-minute seal fork put 1,505 excess rows on
//! duplicated `sequence_number`s across 8 of 360 partitions.)
//!
//! [`split_verifying_prefix`] is the seam. It walks a partition's rows and
//! returns the LONGEST leading run that [`verify_chunk`] accepts, plus a
//! [`PrefixBreak`] naming exactly where and why the chain stops. The caller
//! archives the prefix normally and QUARANTINES the rest — sealed rows are the
//! evidence, so they are never re-sequenced, re-hashed, or deleted; they are
//! marked unarchivable with a machine-readable reason and taken out of the work
//! queue so the backlog can reach zero and the absence monitor can tell a stuck
//! archiver from a permanently broken chain segment.

use serde::{Deserialize, Serialize};

use crate::epoch::{link_for_epoch, ChainEpoch, LinkKey};
use crate::event::ChainHash;
use crate::sink::canonical_date_yyyy_mm_dd;

/// Schema tag every archive line carries, so a reader can tell a sealed-row
/// line from the legacy typed-`AuditEvent` line without guessing.
pub const SEALED_LINE_SCHEMA: &str = "corelink.audit.sealed.v1";
/// Versioned archive-line schema for epoch metadata and keyed links.
pub const SEALED_LINE_SCHEMA_V2: &str = "corelink.audit.sealed.v2";

/// Default per-chunk line cap for the sealed archive.
pub const DEFAULT_SEALED_MAX_LINES_PER_CHUNK: usize = 1000;

/// Default per-chunk byte cap for the sealed archive (1 MiB of NDJSON).
pub const DEFAULT_SEALED_MAX_BYTES_PER_CHUNK: usize = 1024 * 1024;

/// Default value for [`SealedArchiveLine::schema`].
fn default_schema() -> String {
    SEALED_LINE_SCHEMA.to_owned()
}

/// One sealed `audit_outbox` row, as archived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedArchiveLine {
    /// Schema discriminator — [`SEALED_LINE_SCHEMA`] for legacy E0 or
    /// [`SEALED_LINE_SCHEMA_V2`] for keyed epochs. Defaults to the legacy
    /// schema on construction/deserialization; the version-aware verifier
    /// checks the value it actually reads.
    #[serde(default = "default_schema")]
    pub schema: String,
    /// Versioned link algorithm (`0` is the historical unkeyed formula).
    /// Missing in legacy archives means algorithm `0`.
    #[serde(default)]
    pub algorithm_id: u8,
    /// Immutable epoch selected by the signed epoch ledger.
    /// Missing in legacy archives means E0.
    #[serde(default)]
    pub epoch_id: u64,
    /// Registered link-key identity for keyed epochs.  Key material is never
    /// serialized into an archive line.
    #[serde(default)]
    pub link_key_id: Option<u64>,
    /// Chain partition tenant (`audit_outbox.tenant_id`).
    pub tenant_id: String,
    /// Chain partition region (`audit_outbox.region`).
    pub region: String,
    /// Position within the `(tenant_id, region)` chain.
    pub sequence_number: u64,
    /// 64-hex BLAKE3 chain head BEFORE this link (64 zeros at genesis).
    pub prev_hash: String,
    /// 64-hex BLAKE3 link = `BLAKE3(prev_hash || canonical_jcs)`.
    pub chain_hash: String,
    /// `audit_outbox.enqueued_at` — the instant the event happened; the day
    /// slot of the archive key derives from the chunk's FIRST line.
    pub enqueued_at_ms: i64,
    /// `audit_outbox.id` — the deterministic dedup key of the source row.
    pub row_id: String,
    /// The EXACT RFC-8785 JCS bytes that were hashed at seal time.
    pub canonical_jcs: String,
}

/// Why a chunk was refused. Every arm is fail-CLOSED — the caller must NOT
/// write the chunk and must NOT mark its rows archived.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SealedArchiveError {
    /// A chunk with zero lines has no key and no meaning.
    EmptyChunk,
    /// `prev_hash` / `chain_hash` was not 64 lowercase hex characters.
    MalformedHash {
        /// Sequence number of the offending line.
        sequence_number: u64,
        /// Which field failed (`"prev_hash"` / `"chain_hash"`).
        field: &'static str,
    },
    /// Lines in one chunk crossed a `(tenant_id, region)` partition boundary.
    MixedPartition {
        /// The partition the chunk started in.
        expected: String,
        /// The partition the offending line belongs to.
        found: String,
    },
    /// Sequence numbers were not contiguous ascending.
    SequenceGap {
        /// The sequence number the next line had to carry.
        expected: u64,
        /// What it actually carried.
        found: u64,
    },
    /// A line's `prev_hash` did not equal the preceding line's `chain_hash`.
    ChainHeadDiscontinuity {
        /// Sequence number of the offending line.
        sequence_number: u64,
    },
    /// `BLAKE3(prev_hash || canonical_jcs)` did not reproduce `chain_hash` —
    /// the row does not verify against its own persisted bytes.
    LinkHashMismatch {
        /// Sequence number of the offending line.
        sequence_number: u64,
    },
    /// A line carries a different immutable epoch than the verifier selected.
    EpochMismatch {
        /// Expected epoch from the authenticated epoch ledger.
        expected: u64,
        /// Epoch recorded on the line.
        found: u64,
    },
    /// A line carries a different link algorithm than its epoch descriptor.
    AlgorithmMismatch {
        /// Expected algorithm id from the epoch descriptor.
        expected: u8,
        /// Algorithm recorded on the line.
        found: u8,
    },
    /// A line carries a different registered key id than its epoch descriptor.
    LinkKeyMismatch {
        /// Expected key id (if any) from the epoch descriptor.
        expected: Option<u64>,
        /// Key id recorded on the line.
        found: Option<u64>,
    },
    /// Archive schema discriminator does not match the selected epoch.
    SchemaMismatch {
        /// Expected schema discriminator.
        expected: String,
        /// Schema discriminator present on the line.
        found: String,
    },
    /// Key material was unavailable for a keyed epoch.
    MissingLinkKey {
        /// Epoch that requires the unavailable key.
        epoch_id: u64,
    },
    /// The authenticated epoch descriptor itself was malformed.
    InvalidEpoch(String),
    /// The NDJSON could not be produced (serialization fault).
    Serialization(String),
}

impl core::fmt::Display for SealedArchiveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyChunk => write!(f, "sealed archive chunk is empty"),
            Self::MalformedHash {
                sequence_number,
                field,
            } => write!(
                f,
                "sealed archive line seq={sequence_number}: {field} is not 64 lowercase hex chars"
            ),
            Self::MixedPartition { expected, found } => write!(
                f,
                "sealed archive chunk mixes partitions: expected {expected}, found {found}"
            ),
            Self::SequenceGap { expected, found } => write!(
                f,
                "sealed archive chunk sequence gap: expected {expected}, found {found}"
            ),
            Self::ChainHeadDiscontinuity { sequence_number } => write!(
                f,
                "sealed archive line seq={sequence_number}: prev_hash != preceding chain_hash"
            ),
            Self::LinkHashMismatch { sequence_number } => write!(
                f,
                "sealed archive line seq={sequence_number}: BLAKE3(prev_hash || canonical_jcs) != chain_hash"
            ),
            Self::EpochMismatch { expected, found } => write!(
                f,
                "sealed archive epoch mismatch: expected {expected}, found {found}"
            ),
            Self::AlgorithmMismatch { expected, found } => write!(
                f,
                "sealed archive algorithm mismatch: expected {expected}, found {found}"
            ),
            Self::LinkKeyMismatch { expected, found } => write!(
                f,
                "sealed archive link-key mismatch: expected {expected:?}, found {found:?}"
            ),
            Self::SchemaMismatch { expected, found } => write!(
                f,
                "sealed archive schema mismatch: expected {expected}, found {found}"
            ),
            Self::MissingLinkKey { epoch_id } => write!(
                f,
                "sealed archive epoch {epoch_id} requires unavailable link key"
            ),
            Self::InvalidEpoch(reason) => write!(f, "sealed archive epoch is invalid: {reason}"),
            Self::Serialization(e) => write!(f, "sealed archive serialization failed: {e}"),
        }
    }
}

impl std::error::Error for SealedArchiveError {}

/// Parse 64 lowercase-hex characters into a [`ChainHash`].
fn parse_hash(hex: &str) -> Option<ChainHash> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // Reject uppercase: the seal writes lowercase, and accepting both would let
    // two spellings of one hash compare unequal as strings while comparing equal
    // as bytes.
    if hex.bytes().any(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(ChainHash(out))
}

/// Canonical archive key for a chunk whose FIRST line is `first`.
///
/// See the module docs for why `tenant_id` + `region` are in the key.
#[must_use]
pub fn sealed_chunk_key(first: &SealedArchiveLine) -> String {
    // `enqueued_at` is an epoch-ms INTEGER column and is non-negative for every
    // row the drain seals; clamp defensively so a corrupt negative value lands
    // in the epoch day rather than panicking the archiver.
    let ms = u64::try_from(first.enqueued_at_ms).unwrap_or(0);
    let ymd = canonical_date_yyyy_mm_dd(ms);
    let (y, rest) = ymd.split_at(ymd.find('-').unwrap_or(ymd.len()));
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let (m, rest2) = rest.split_at(rest.find('-').unwrap_or(rest.len()));
    let d = rest2.strip_prefix('-').unwrap_or(rest2);
    format!(
        "audit/{}/{}/{}/{}/{}/{:08}.ndjson",
        y, m, d, first.tenant_id, first.region, first.sequence_number
    )
}

/// Where a partition's archivable prefix stops, and why.
///
/// Returned by [`split_verifying_prefix`] alongside the prefix itself. It names
/// the FIRST line that cannot join the chunk; every line from `index` onward is
/// unarchivable until a human resolves the break, because the chain's own
/// linkage — not a policy choice — says so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefixBreak {
    /// Index into the slice handed to [`split_verifying_prefix`] of the first
    /// line that could not be archived.
    pub index: usize,
    /// `sequence_number` carried by that line. This is the LOW bound of what
    /// the caller quarantines — note that on a duplicated sequence it equals a
    /// sequence the prefix already archived, which is correct: the archived
    /// copy is already marked and the duplicate is the row being refused.
    pub sequence_number: u64,
    /// The verification failure that stopped the prefix.
    pub error: SealedArchiveError,
}

impl PrefixBreak {
    /// Short machine-readable reason string, for
    /// `audit_outbox.quarantine_reason`.
    ///
    /// Deliberately grep-able and stable: an operator triaging a quarantined
    /// row must be able to `GROUP BY quarantine_reason` and see the shape of
    /// the damage without reading a prose log line that may have rotated away.
    #[must_use]
    pub fn reason_code(&self) -> String {
        match &self.error {
            SealedArchiveError::EmptyChunk => "empty_chunk".to_owned(),
            SealedArchiveError::MalformedHash {
                sequence_number,
                field,
            } => format!("malformed_hash:seq={sequence_number},field={field}"),
            SealedArchiveError::MixedPartition { expected, found } => {
                format!("mixed_partition:expected={expected},found={found}")
            }
            SealedArchiveError::SequenceGap { expected, found } => {
                format!("sequence_gap:expected={expected},found={found}")
            }
            SealedArchiveError::ChainHeadDiscontinuity { sequence_number } => {
                format!("chain_head_discontinuity:seq={sequence_number}")
            }
            SealedArchiveError::LinkHashMismatch { sequence_number } => {
                format!("link_hash_mismatch:seq={sequence_number}")
            }
            SealedArchiveError::EpochMismatch { expected, found } => {
                format!("epoch_mismatch:expected={expected},found={found}")
            }
            SealedArchiveError::AlgorithmMismatch { expected, found } => {
                format!("algorithm_mismatch:expected={expected},found={found}")
            }
            SealedArchiveError::LinkKeyMismatch { expected, found } => {
                format!("link_key_mismatch:expected={expected:?},found={found:?}")
            }
            SealedArchiveError::SchemaMismatch { expected, found } => {
                format!("schema_mismatch:expected={expected},found={found}")
            }
            SealedArchiveError::MissingLinkKey { epoch_id } => {
                format!("missing_link_key:epoch={epoch_id}")
            }
            SealedArchiveError::InvalidEpoch(reason) => {
                format!("invalid_epoch:{reason}")
            }
            SealedArchiveError::Serialization(_) => "serialization".to_owned(),
        }
    }
}

/// Split an in-order run of one partition's lines into the longest leading run
/// that [`verify_chunk`] accepts, plus the break that ended it.
///
/// `(prefix, None)` means the whole slice verifies. `(prefix, Some(break))`
/// means `prefix` is archivable AS A CHUNK WINDOW and everything from
/// `break.index` onward is not.
///
/// This function is PURE and BORROWS: it never mutates a line, so no chain
/// column can be rewritten on the quarantine path. Re-sequencing a sealed row
/// would destroy the evidence the chain exists to produce.
///
/// An empty slice yields an empty prefix and no break — there is nothing to
/// archive and nothing to quarantine.
#[must_use]
pub fn split_verifying_prefix(
    lines: &[SealedArchiveLine],
) -> (&[SealedArchiveLine], Option<PrefixBreak>) {
    split_verifying_prefix_for_epoch(lines, &ChainEpoch::legacy(), None)
}

/// Version-aware counterpart to [`split_verifying_prefix`].  It verifies a
/// contiguous archive prefix using the authenticated epoch descriptor.  A
/// keyed epoch cannot silently fall back to the legacy formula: missing key,
/// wrong algorithm, wrong epoch, and wrong key id all stop the prefix and are
/// reported as a quarantine reason.
#[must_use]
pub fn split_verifying_prefix_for_epoch<'a>(
    lines: &'a [SealedArchiveLine],
    epoch: &ChainEpoch,
    key: Option<&LinkKey>,
) -> (&'a [SealedArchiveLine], Option<PrefixBreak>) {
    let epoch_error = epoch.validate().err();
    if let Some(error) = epoch_error {
        return (
            lines.get(..0).unwrap_or(&[]),
            lines.first().map(|line| PrefixBreak {
                index: 0,
                sequence_number: line.sequence_number,
                error: SealedArchiveError::InvalidEpoch(error.to_string()),
            }),
        );
    }
    let Some(first) = lines.first() else {
        return (lines, None);
    };
    let partition = format!("{}/{}", first.tenant_id, first.region);
    let expected_schema = if epoch.algorithm().id() == 0 {
        SEALED_LINE_SCHEMA
    } else {
        SEALED_LINE_SCHEMA_V2
    };
    let mut expected_seq = first.sequence_number;
    // `None` for the first line: a chunk may start mid-chain, so its `prev_hash`
    // is only checkable against the row's own link hash, not against a
    // predecessor we do not hold.
    let mut running_head: Option<ChainHash> = None;

    for (index, line) in lines.iter().enumerate() {
        let found_partition = format!("{}/{}", line.tenant_id, line.region);
        let failure = if found_partition != partition {
            Some(SealedArchiveError::MixedPartition {
                expected: partition.clone(),
                found: found_partition,
            })
        } else if line.sequence_number != expected_seq {
            Some(SealedArchiveError::SequenceGap {
                expected: expected_seq,
                found: line.sequence_number,
            })
        } else if line.schema != expected_schema {
            Some(SealedArchiveError::SchemaMismatch {
                expected: expected_schema.to_owned(),
                found: line.schema.clone(),
            })
        } else if line.epoch_id != epoch.epoch_id() {
            Some(SealedArchiveError::EpochMismatch {
                expected: epoch.epoch_id(),
                found: line.epoch_id,
            })
        } else if line.algorithm_id != epoch.algorithm().id() {
            Some(SealedArchiveError::AlgorithmMismatch {
                expected: epoch.algorithm().id(),
                found: line.algorithm_id,
            })
        } else if line.link_key_id != epoch.link_key_id() {
            Some(SealedArchiveError::LinkKeyMismatch {
                expected: epoch.link_key_id(),
                found: line.link_key_id,
            })
        } else if matches!(epoch.algorithm(), crate::epoch::LinkAlgorithm::KeyedV2) && key.is_none()
        {
            Some(SealedArchiveError::MissingLinkKey {
                epoch_id: epoch.epoch_id(),
            })
        } else {
            match (parse_hash(&line.prev_hash), parse_hash(&line.chain_hash)) {
                (None, _) => Some(SealedArchiveError::MalformedHash {
                    sequence_number: line.sequence_number,
                    field: "prev_hash",
                }),
                (_, None) => Some(SealedArchiveError::MalformedHash {
                    sequence_number: line.sequence_number,
                    field: "chain_hash",
                }),
                (Some(prev), Some(claimed)) => {
                    if running_head.is_some_and(|head| head != prev) {
                        Some(SealedArchiveError::ChainHeadDiscontinuity {
                            sequence_number: line.sequence_number,
                        })
                    } else if link_for_epoch(epoch, &prev, line.canonical_jcs.as_bytes(), key)
                        != Ok(claimed)
                    {
                        Some(SealedArchiveError::LinkHashMismatch {
                            sequence_number: line.sequence_number,
                        })
                    } else {
                        running_head = Some(claimed);
                        expected_seq = expected_seq.saturating_add(1);
                        None
                    }
                }
            }
        };
        if let Some(error) = failure {
            return (
                // `index` is always in bounds for a prefix range; `get` rather
                // than a slice expression because the workspace denies
                // `indexing_slicing` and a silent panic here would take the
                // whole archive sweep down.
                lines.get(..index).unwrap_or(&[]),
                Some(PrefixBreak {
                    index,
                    sequence_number: line.sequence_number,
                    error,
                }),
            );
        }
    }
    (lines, None)
}

/// Verify a slice of lines as one contiguous chunk of a single partition.
///
/// # Errors
///
/// Any [`SealedArchiveError`]; every arm means "do not archive this".
pub fn verify_chunk(lines: &[SealedArchiveLine]) -> Result<(), SealedArchiveError> {
    if lines.is_empty() {
        return Err(SealedArchiveError::EmptyChunk);
    }
    // ONE implementation of the link rules, shared with the partial-progress
    // path, so the prefix the archiver writes and the chunk the verifier
    // accepts can never drift apart.
    match split_verifying_prefix(lines) {
        (_, None) => Ok(()),
        (_, Some(brk)) => Err(brk.error),
    }
}

/// Verify a chunk against an authenticated epoch and its in-memory key.
/// Legacy callers should use [`verify_chunk`], which remains byte-for-byte
/// compatible with E0 archives.  Unknown or unavailable epoch evidence is
/// always an error; this function never downgrades to unkeyed verification.
pub fn verify_chunk_for_epoch(
    lines: &[SealedArchiveLine],
    epoch: &ChainEpoch,
    key: Option<&LinkKey>,
) -> Result<(), SealedArchiveError> {
    if lines.is_empty() {
        return Err(SealedArchiveError::EmptyChunk);
    }
    match split_verifying_prefix_for_epoch(lines, epoch, key) {
        (_, None) => Ok(()),
        (_, Some(brk)) => Err(brk.error),
    }
}

/// Verify then serialize a chunk to NDJSON bytes.
///
/// # Errors
///
/// Any [`SealedArchiveError`] — the chunk is not written.
pub fn serialize_chunk(lines: &[SealedArchiveLine]) -> Result<Vec<u8>, SealedArchiveError> {
    verify_chunk(lines)?;
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let encoded = serde_json::to_string(line)
            .map_err(|e| SealedArchiveError::Serialization(e.to_string()))?;
        out.push_str(&encoded);
    }
    Ok(out.into_bytes())
}

/// Verify then serialize a versioned archive chunk.  Serialization is refused
/// unless every line uses the selected epoch and algorithm.
pub fn serialize_chunk_for_epoch(
    lines: &[SealedArchiveLine],
    epoch: &ChainEpoch,
    key: Option<&LinkKey>,
) -> Result<Vec<u8>, SealedArchiveError> {
    verify_chunk_for_epoch(lines, epoch, key)?;
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let encoded = serde_json::to_string(line)
            .map_err(|e| SealedArchiveError::Serialization(e.to_string()))?;
        out.push_str(&encoded);
    }
    Ok(out.into_bytes())
}

/// Split an in-order run of one partition's lines into chunks that each fit the
/// caps AND stay inside one calendar day.
///
/// The day boundary is a hard split, not a cap: the key encodes a day, so a
/// chunk that straddled midnight would file half its lines under the wrong one.
#[must_use]
pub fn split_into_chunks(
    lines: Vec<SealedArchiveLine>,
    max_lines: usize,
    max_bytes: usize,
) -> Vec<Vec<SealedArchiveLine>> {
    let max_lines = max_lines.max(1);
    let max_bytes = max_bytes.max(1);
    let mut chunks: Vec<Vec<SealedArchiveLine>> = Vec::new();
    let mut current: Vec<SealedArchiveLine> = Vec::new();
    let mut current_bytes: usize = 0;
    let mut current_day: Option<String> = None;

    for line in lines {
        let day = canonical_date_yyyy_mm_dd(u64::try_from(line.enqueued_at_ms).unwrap_or(0));
        // Approximate the encoded size with the JCS payload plus a fixed
        // envelope allowance; an exact re-encode per line would cost a
        // serialization for every candidate boundary and the cap is a budget,
        // not an invariant.
        let line_bytes = line.canonical_jcs.len() + line.row_id.len() + 256;
        let day_changed = current_day.as_ref().is_some_and(|d| *d != day);
        let full = current.len() >= max_lines || current_bytes + line_bytes > max_bytes;
        if !current.is_empty() && (day_changed || full) {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        if current.is_empty() {
            current_day = Some(day);
        }
        current_bytes += line_bytes;
        current.push(line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod sealed_archive_tests;
