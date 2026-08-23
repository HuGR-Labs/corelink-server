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
//! ## Line format (`corelink.audit.sealed.v1`)
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

use serde::{Deserialize, Serialize};

use crate::chain::link_chain_hash_from_canonical;
use crate::event::ChainHash;
use crate::sink::canonical_date_yyyy_mm_dd;

/// Schema tag every archive line carries, so a reader can tell a sealed-row
/// line from the legacy typed-`AuditEvent` line without guessing.
pub const SEALED_LINE_SCHEMA: &str = "corelink.audit.sealed.v1";

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
    /// Schema discriminator — always [`SEALED_LINE_SCHEMA`] on write. Defaults
    /// on construction and on deserialization so no call site can forget it;
    /// the verifier still checks the value it actually reads.
    #[serde(default = "default_schema")]
    pub schema: String,
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

/// Verify a slice of lines as one contiguous chunk of a single partition.
///
/// # Errors
///
/// Any [`SealedArchiveError`]; every arm means "do not archive this".
pub fn verify_chunk(lines: &[SealedArchiveLine]) -> Result<(), SealedArchiveError> {
    let Some(first) = lines.first() else {
        return Err(SealedArchiveError::EmptyChunk);
    };
    let partition = format!("{}/{}", first.tenant_id, first.region);
    let mut expected_seq = first.sequence_number;
    // `None` for the first line: a chunk may start mid-chain, so its `prev_hash`
    // is only checkable against the row's own link hash, not against a
    // predecessor we do not hold.
    let mut running_head: Option<ChainHash> = None;

    for line in lines {
        let found_partition = format!("{}/{}", line.tenant_id, line.region);
        if found_partition != partition {
            return Err(SealedArchiveError::MixedPartition {
                expected: partition,
                found: found_partition,
            });
        }
        if line.sequence_number != expected_seq {
            return Err(SealedArchiveError::SequenceGap {
                expected: expected_seq,
                found: line.sequence_number,
            });
        }
        let prev = parse_hash(&line.prev_hash).ok_or(SealedArchiveError::MalformedHash {
            sequence_number: line.sequence_number,
            field: "prev_hash",
        })?;
        let claimed = parse_hash(&line.chain_hash).ok_or(SealedArchiveError::MalformedHash {
            sequence_number: line.sequence_number,
            field: "chain_hash",
        })?;
        if let Some(head) = running_head {
            if head != prev {
                return Err(SealedArchiveError::ChainHeadDiscontinuity {
                    sequence_number: line.sequence_number,
                });
            }
        }
        let recomputed = link_chain_hash_from_canonical(&prev, line.canonical_jcs.as_bytes());
        if recomputed != claimed {
            return Err(SealedArchiveError::LinkHashMismatch {
                sequence_number: line.sequence_number,
            });
        }
        running_head = Some(claimed);
        expected_seq = expected_seq.saturating_add(1);
    }
    Ok(())
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
mod tests {
    use super::*;

    /// Build a chain of `n` verifying lines starting at `start_seq`, each one
    /// linked off the previous, exactly the way the drain seals them.
    fn chain(
        tenant: &str,
        region: &str,
        start_seq: u64,
        start_prev: ChainHash,
        enqueued_at_ms: i64,
        n: u64,
    ) -> Vec<SealedArchiveLine> {
        let mut head = start_prev;
        let mut out = Vec::new();
        for i in 0..n {
            let seq = start_seq + i;
            let jcs = format!("{{\"data\":{{\"n\":{seq}}},\"id\":\"row-{seq}\"}}");
            let link = link_chain_hash_from_canonical(&head, jcs.as_bytes());
            out.push(SealedArchiveLine {
                schema: default_schema(),
                tenant_id: tenant.to_owned(),
                region: region.to_owned(),
                sequence_number: seq,
                prev_hash: head.to_hex(),
                chain_hash: link.to_hex(),
                enqueued_at_ms,
                row_id: format!("row-{seq}"),
                canonical_jcs: jcs,
            });
            head = link;
        }
        out
    }

    #[test]
    fn verifies_a_genesis_chain() {
        let lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 5);
        verify_chunk(&lines).expect("honest chain must verify");
    }

    #[test]
    fn verifies_a_chunk_that_starts_mid_chain() {
        // A chunk is a WINDOW of the chain; it must not require seq 0.
        let all = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 10);
        verify_chunk(&all[4..]).expect("mid-chain window must verify");
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 3);
        lines[1].canonical_jcs = "{\"data\":{\"n\":999}}".to_owned();
        assert_eq!(
            verify_chunk(&lines),
            Err(SealedArchiveError::LinkHashMismatch { sequence_number: 1 })
        );
    }

    #[test]
    fn rejects_a_rewritten_prev_hash() {
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 3);
        // Re-seal line 2 self-consistently off a FORGED head: the line alone
        // verifies, so only the running-head check catches the splice.
        let forged = ChainHash([7u8; 32]);
        let relink = link_chain_hash_from_canonical(&forged, lines[2].canonical_jcs.as_bytes());
        lines[2].prev_hash = forged.to_hex();
        lines[2].chain_hash = relink.to_hex();
        assert_eq!(
            verify_chunk(&lines),
            Err(SealedArchiveError::ChainHeadDiscontinuity { sequence_number: 2 })
        );
    }

    #[test]
    fn rejects_a_dropped_row() {
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 4);
        lines.remove(2);
        assert_eq!(
            verify_chunk(&lines),
            Err(SealedArchiveError::SequenceGap {
                expected: 2,
                found: 3
            })
        );
    }

    #[test]
    fn rejects_a_cross_partition_chunk() {
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 2);
        let other = chain("t2", "enam", 2, ChainHash::genesis(), 1_787_517_615_093, 1);
        lines.extend(other);
        assert!(matches!(
            verify_chunk(&lines),
            Err(SealedArchiveError::MixedPartition { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_chunk() {
        assert_eq!(verify_chunk(&[]), Err(SealedArchiveError::EmptyChunk));
        assert_eq!(serialize_chunk(&[]), Err(SealedArchiveError::EmptyChunk));
    }

    #[test]
    fn rejects_an_uppercase_hash() {
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 1);
        lines[0].chain_hash = lines[0].chain_hash.to_uppercase();
        assert_eq!(
            verify_chunk(&lines),
            Err(SealedArchiveError::MalformedHash {
                sequence_number: 0,
                field: "chain_hash"
            })
        );
    }

    #[test]
    fn serializes_one_json_object_per_line_and_round_trips() {
        let lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 3);
        let bytes = serialize_chunk(&lines).expect("honest chunk serializes");
        let text = String::from_utf8(bytes).expect("NDJSON is UTF-8");
        assert!(!text.ends_with('\n'), "no trailing newline");
        let parsed: Vec<SealedArchiveLine> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("each line is one JSON object"))
            .collect();
        assert_eq!(parsed, lines);
        assert!(parsed.iter().all(|l| l.schema == SEALED_LINE_SCHEMA));
        verify_chunk(&parsed).expect("round-tripped chunk still verifies");
    }

    #[test]
    fn key_carries_the_partition_so_two_partitions_cannot_collide() {
        let a = chain("t1", "enam", 42, ChainHash::genesis(), 1_787_517_615_093, 1);
        let b = chain("t2", "weur", 42, ChainHash::genesis(), 1_787_517_615_093, 1);
        let ka = sealed_chunk_key(&a[0]);
        let kb = sealed_chunk_key(&b[0]);
        assert_ne!(ka, kb, "distinct partitions MUST NOT share a key");
        assert_eq!(ka, "audit/2026/08/23/t1/enam/00000042.ndjson");
        assert!(kb.starts_with("audit/2026/08/23/"), "same day prefix: {kb}");
    }

    #[test]
    fn key_day_comes_from_the_event_not_from_now() {
        // 2020-01-02T00:00:00Z — a backfilled row must file under its own day.
        let old = chain("t1", "enam", 7, ChainHash::genesis(), 1_577_923_200_000, 1);
        assert_eq!(
            sealed_chunk_key(&old[0]),
            "audit/2020/01/02/t1/enam/00000007.ndjson"
        );
    }

    #[test]
    fn split_never_lets_a_chunk_cross_midnight() {
        // 2026-08-23T23:59:59.500Z and 2026-08-24T00:00:00.500Z.
        let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_529_599_500, 1);
        lines.extend(chain(
            "t1",
            "enam",
            1,
            parse_hash(&lines[0].chain_hash).expect("hex"),
            1_787_529_600_500,
            1,
        ));
        let chunks = split_into_chunks(lines, 1000, 1 << 20);
        assert_eq!(chunks.len(), 2, "midnight is a hard split");
        assert!(sealed_chunk_key(&chunks[0][0]).contains("/08/23/"));
        assert!(sealed_chunk_key(&chunks[1][0]).contains("/08/24/"));
    }

    #[test]
    fn split_respects_the_line_cap_and_loses_nothing() {
        let lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 25);
        let chunks = split_into_chunks(lines.clone(), 10, 1 << 20);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 25);
        for c in &chunks {
            assert!(c.len() <= 10);
            verify_chunk(c).expect("every split chunk still verifies");
        }
        // Every chunk key is distinct because the first sequence differs.
        let keys: std::collections::BTreeSet<String> =
            chunks.iter().map(|c| sealed_chunk_key(&c[0])).collect();
        assert_eq!(keys.len(), chunks.len());
    }

    #[test]
    fn split_of_nothing_is_nothing() {
        assert!(split_into_chunks(Vec::new(), 10, 10).is_empty());
    }

    #[test]
    fn zero_caps_do_not_hang_or_drop() {
        let lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 3);
        let chunks = split_into_chunks(lines, 0, 0);
        assert_eq!(chunks.len(), 3, "clamped to one line per chunk");
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 3);
    }
}
