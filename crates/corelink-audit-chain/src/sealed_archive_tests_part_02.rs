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
            SealedArchiveError::SequenceOverflow {
                sequence_number: u64::MAX,
            },
            "sequence_overflow:seq=18446744073709551615",
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
