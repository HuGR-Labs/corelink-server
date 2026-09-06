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

#[test]
fn prefix_stops_immediately_before_a_duplicated_sequence() {
    // Duplicate at 10, so 11 is what the chunk needed next and 11 is
    // missing from the branch — the exact `_public`/`wnam` shape.
    let lines = forked_partition(10, 4);
    let (prefix, brk) = split_verifying_prefix(&lines);
    assert_eq!(prefix.len(), 11, "sequences 0..=10 archive normally");
    verify_chunk(prefix).expect("the prefix must be an archivable chunk");
    let brk = brk.expect("the fork must be reported, never swallowed");
    assert_eq!(brk.index, 11);
    assert_eq!(brk.sequence_number, 10);
    assert_eq!(
        brk.error,
        SealedArchiveError::SequenceGap {
            expected: 11,
            found: 10
        }
    );
    // The classification names the expected/found pair, machine-readably.
    assert_eq!(brk.reason_code(), "sequence_gap:expected=11,found=10");
}

#[test]
fn a_clean_partition_is_entirely_prefix_and_quarantines_nothing() {
    let lines = chain("t1", "enam", 0, ChainHash::genesis(), 1_787_517_615_093, 12);
    let (prefix, brk) = split_verifying_prefix(&lines);
    assert_eq!(prefix.len(), lines.len());
    assert!(brk.is_none(), "a healthy partition has no break");
    verify_chunk(prefix).expect("whole partition verifies");
}

#[test]
fn a_break_on_the_very_first_row_yields_an_empty_prefix_and_no_loop() {
    // The archiver resumes at the first UNARCHIVED sequence. If that row is
    // itself the break, there is nothing to archive at all — the caller
    // must quarantine and move on rather than spin.
    let mut lines = chain("t1", "enam", 5, ChainHash::genesis(), 1_787_517_615_093, 3);
    if let Some(l) = lines.first_mut() {
        // Corrupt the first row's own link: it no longer verifies against
        // its persisted bytes, so not even a window can start here.
        l.canonical_jcs = "{\"data\":{\"n\":999}}".to_owned();
    }
    let (prefix, brk) = split_verifying_prefix(&lines);
    assert!(prefix.is_empty(), "nothing is archivable");
    let brk = brk.expect("the break must be reported");
    assert_eq!(brk.index, 0);
    assert_eq!(brk.sequence_number, 5);
    assert_eq!(brk.reason_code(), "link_hash_mismatch:seq=5");
    // An empty prefix produces no chunk, so nothing is written and nothing
    // is marked archived — the caller quarantines and the partition leaves
    // the work queue instead of being retried forever.
    assert_eq!(verify_chunk(prefix), Err(SealedArchiveError::EmptyChunk));
}

#[test]
fn archiving_resumes_after_a_break_as_a_new_mid_chain_window() {
    // Rows sealed AFTER the quarantined segment form their own window. A
    // window starts wherever it starts and carries whatever `prev_hash` its
    // first row holds; `verify_chunk` checks that row against its own link
    // only (`running_head` starts `None`), so resumption is legal.
    let lines = forked_partition(10, 4);
    let (_, brk) = split_verifying_prefix(&lines);
    assert!(brk.is_some());
    // The branch that continues at sequence 12 with the forked head.
    let resumed = lines.get(12..).expect("branch rows exist");
    assert_eq!(resumed.len(), 4);
    assert_eq!(
        resumed.first().map(|l| l.sequence_number),
        Some(12),
        "the resumed window starts mid-chain, not at 0"
    );
    let (prefix, brk2) = split_verifying_prefix(resumed);
    assert!(brk2.is_none(), "the resumed window has no break of its own");
    assert_eq!(prefix.len(), 4);
    verify_chunk(resumed).expect("a mid-chain window verifies as a chunk");
    serialize_chunk(resumed).expect("and therefore serializes");
}

#[test]
fn classifying_a_break_never_rewrites_a_chain_column() {
    // The quarantine decision is derived from the rows and MUST NOT touch
    // them: sequence_number / prev_hash / chain_hash / canonical_jcs are
    // the evidence. `split_verifying_prefix` borrows, so this is a property
    // of the signature — this test pins it against a future refactor that
    // takes ownership and "normalizes" a row on the way past.
    let before = forked_partition(10, 4);
    let subject = before.clone();
    let (prefix, brk) = split_verifying_prefix(&subject);
    let reason = brk.expect("break present").reason_code();
    assert!(!reason.is_empty());
    assert_eq!(prefix.len(), 11);
    assert_eq!(
        subject, before,
        "not one byte of any sealed row may change on the quarantine path"
    );
    // And specifically the chain columns, named, so the assertion reads as
    // the invariant it enforces rather than as a generic equality.
    for (after, orig) in subject.iter().zip(before.iter()) {
        assert_eq!(after.sequence_number, orig.sequence_number);
        assert_eq!(after.prev_hash, orig.prev_hash);
        assert_eq!(after.chain_hash, orig.chain_hash);
        assert_eq!(after.canonical_jcs, orig.canonical_jcs);
        assert_eq!(after.enqueued_at_ms, orig.enqueued_at_ms);
    }
}

#[test]
fn reason_codes_are_stable_and_greppable() {
    // `quarantine_reason` is queried by operators with GROUP BY; the shape
    // is part of the contract, not a log string.
    let cases = [
        (
            SealedArchiveError::SequenceGap {
                expected: 11,
                found: 10,
            },
            "sequence_gap:expected=11,found=10",
        ),
        (
            SealedArchiveError::ChainHeadDiscontinuity { sequence_number: 7 },
            "chain_head_discontinuity:seq=7",
        ),
        (
            SealedArchiveError::LinkHashMismatch { sequence_number: 7 },
            "link_hash_mismatch:seq=7",
        ),
        (
            SealedArchiveError::MalformedHash {
                sequence_number: 7,
                field: "prev_hash",
            },
            "malformed_hash:seq=7,field=prev_hash",
        ),
        (
            SealedArchiveError::MixedPartition {
                expected: "a/enam".to_owned(),
                found: "b/enam".to_owned(),
            },
            "mixed_partition:expected=a/enam,found=b/enam",
        ),
    ];
    for (error, expected) in cases {
        let brk = PrefixBreak {
            index: 0,
            sequence_number: 0,
            error,
        };
        assert_eq!(brk.reason_code(), expected);
    }
}

#[test]
fn prefix_of_nothing_is_nothing() {
    let (prefix, brk) = split_verifying_prefix(&[]);
    assert!(prefix.is_empty());
    assert!(brk.is_none(), "an empty read is not a chain break");
}

fn keyed_chain() -> (ChainEpoch, LinkKey, Vec<SealedArchiveLine>) {
    let key = LinkKey::from_bytes([0x37; 32]);
    let epoch = ChainEpoch::keyed_successor(1, 9, 2, ChainHash([0x11; 32]), 0)
        .expect("valid keyed successor");
    let mut head = *epoch.start_prev_hash();
    let mut lines = Vec::new();
    for seq in 2..5 {
        let jcs = format!("{{\"data\":{{\"n\":{seq}}}}}");
        let link = link_for_epoch(&epoch, &head, jcs.as_bytes(), Some(&key)).expect("keyed link");
        lines.push(SealedArchiveLine {
            schema: SEALED_LINE_SCHEMA_V2.to_owned(),
            algorithm_id: epoch.algorithm().id(),
            epoch_id: epoch.epoch_id(),
            link_key_id: epoch.link_key_id(),
            tenant_id: "t1".to_owned(),
            region: "enam".to_owned(),
            sequence_number: seq,
            prev_hash: head.to_hex(),
            chain_hash: link.to_hex(),
            enqueued_at_ms: 1_787_517_615_093,
            row_id: format!("row-{seq}"),
            canonical_jcs: jcs,
        });
        head = link;
    }
    (epoch, key, lines)
}

#[test]
fn keyed_epoch_archive_verifies_only_with_matching_key_and_metadata() {
    let (epoch, key, lines) = keyed_chain();
    verify_chunk_for_epoch(&lines, &epoch, Some(&key)).expect("keyed chain verifies");
    assert_eq!(
        verify_chunk(&lines),
        Err(SealedArchiveError::SchemaMismatch {
            expected: SEALED_LINE_SCHEMA.to_owned(),
            found: SEALED_LINE_SCHEMA_V2.to_owned()
        })
    );
    assert_eq!(
        verify_chunk_for_epoch(&lines, &epoch, None),
        Err(SealedArchiveError::MissingLinkKey { epoch_id: 1 })
    );
    let wrong = LinkKey::from_bytes([0x38; 32]);
    assert!(matches!(
        verify_chunk_for_epoch(&lines, &epoch, Some(&wrong)),
        Err(SealedArchiveError::LinkHashMismatch { .. })
    ));
}

#[test]
fn keyed_epoch_downgrade_or_epoch_splice_is_rejected() {
    let (epoch, key, mut lines) = keyed_chain();
    lines[1].algorithm_id = 0;
    assert_eq!(
        verify_chunk_for_epoch(&lines, &epoch, Some(&key)),
        Err(SealedArchiveError::AlgorithmMismatch {
            expected: 1,
            found: 0
        })
    );
    lines[1].algorithm_id = 1;
    lines[1].epoch_id = 0;
    assert_eq!(
        verify_chunk_for_epoch(&lines, &epoch, Some(&key)),
        Err(SealedArchiveError::EpochMismatch {
            expected: 1,
            found: 0
        })
    );
    lines[1].epoch_id = 1;
    lines[1].link_key_id = None;
    assert_eq!(
        verify_chunk_for_epoch(&lines, &epoch, Some(&key)),
        Err(SealedArchiveError::LinkKeyMismatch {
            expected: Some(9),
            found: None
        })
    );
    lines[1].link_key_id = Some(9);
    lines[1].algorithm_id = 0;
    lines[1].epoch_id = 0;
    assert_eq!(
        verify_chunk_for_epoch(&lines, &epoch, Some(&key)),
        Err(SealedArchiveError::EpochMismatch {
            expected: 1,
            found: 0
        })
    );
}
