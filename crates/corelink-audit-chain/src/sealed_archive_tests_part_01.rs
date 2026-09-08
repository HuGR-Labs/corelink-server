use super::*;
use crate::chain::link_chain_hash_from_canonical;
use crate::epoch::{ChainEpoch, LinkKey};

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
            algorithm_id: 0,
            epoch_id: 0,
            link_key_id: None,
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
fn legacy_verifier_rejects_partial_or_keyed_metadata() {
    let mut lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 2);
    lines[0].link_key_id = Some(7);
    assert_eq!(
        verify_chunk(&lines),
        Err(SealedArchiveError::LinkKeyMismatch {
            expected: None,
            found: Some(7)
        })
    );
    lines[0].link_key_id = None;
    lines[0].algorithm_id = 1;
    assert_eq!(
        verify_chunk(&lines),
        Err(SealedArchiveError::AlgorithmMismatch {
            expected: 0,
            found: 1
        })
    );
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

// ---------------------------------------------------------------------
// `split_verifying_prefix` — partial progress over a FORKED partition.
//
// The fixture reproduces the measured 2026-08-14 defect rather than a
// synthetic gap: within one `(tenant_id, region)` partition the seal path
// forked, so a sequence number appears TWICE carrying two DIFFERENT
// `prev_hash` values (two branches, not a relabelling) and the sequence
// that should have followed is missing.
// ---------------------------------------------------------------------

/// `_public`/`wnam`-shaped fixture: a clean run 0..=`dup_at`, then a SECOND
/// row re-using sequence `dup_at` off a DIFFERENT (forked) head, then the
/// branch continues at `dup_at + 2` — i.e. `dup_at + 1` is missing.
fn forked_partition(dup_at: u64, tail: u64) -> Vec<SealedArchiveLine> {
    let mut lines = chain(
        "_public",
        "wnam",
        0,
        ChainHash::genesis(),
        1_787_517_615_093,
        dup_at + 1,
    );
    // The forked branch: same sequence number, different prev_hash.
    let forked_head = ChainHash([0x5au8; 32]);
    let mut dup = chain("_public", "wnam", dup_at, forked_head, 1_787_517_615_093, 1);
    // Give the duplicate its own payload so it is unmistakably a second
    // row, not a copy of the archived one.
    let jcs = format!("{{\"data\":{{\"n\":{dup_at}}},\"id\":\"row-{dup_at}-fork\"}}");
    let link = link_chain_hash_from_canonical(&forked_head, jcs.as_bytes());
    if let Some(d) = dup.first_mut() {
        d.row_id = format!("row-{dup_at}-fork");
        d.canonical_jcs = jcs;
        d.chain_hash = link.to_hex();
    }
    let branch_start = dup_at + 2;
    let branch = chain(
        "_public",
        "wnam",
        branch_start,
        link,
        1_787_517_615_093,
        tail,
    );
    lines.extend(dup);
    lines.extend(branch);
    lines
}
