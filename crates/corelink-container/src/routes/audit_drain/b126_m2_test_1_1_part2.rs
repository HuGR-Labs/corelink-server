/// THE LAUNDERING EXPLOIT (backend-audit §3), now CLOSED: an insider with D1
/// write rewrites the sealed rows + head and STRIPS the signature to NULL (they
/// lack the write-only seed, so they cannot re-sign). With the signing regime
/// active, a NULL signature is UNVERIFIABLE → FailClosed by default (was
/// silently `Proceed`, which let the honest drain re-sign the forged head).
#[test]
fn resume_fails_closed_on_stripped_signature() {
    let cp = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::FailClosed,
        "a NULL signature under an active signing regime must be tamper (fail-CLOSED)"
    );
}

/// A NULL / foreign-key-id head is tolerated ONLY under the explicit operator
/// migration escape (`trust_unsigned_resume = true`) — the pre-0080 legacy
/// bootstrap + coordinated seed-rotation re-sign path.
#[test]
fn resume_tolerates_unsigned_only_under_migration_escape() {
    let legacy = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&legacy), Some(&SEED_A), KID, TENANT, REGION, true),
        HeadResumeCheck::Proceed
    );
    // Foreign key id (rotation) also tolerated only under the escape.
    let rotated = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
    assert_eq!(
        check_head_on_resume(Some(&rotated), Some(&SEED_B), 2, TENANT, REGION, true),
        HeadResumeCheck::Proceed
    );
}

/// The legacy head is RE-SIGNED on the next advance: signing it with the
/// configured seed yields a signature that verifies.
#[test]
fn legacy_head_gets_signed_on_next_advance() {
    // Simulate the advance signing the new head computed from the legacy state.
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 10).unwrap();
    assert!(verify_head(
        &SEED_A,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        10,
        &sig
    ));
    // And a checkpoint carrying that fresh signature now Verifies on resume.
    let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 10);
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::Verified
    );
}

/// A head signed under a DIFFERENT key id (seed/key rotation) is TAMPER by
/// default — it cannot be verified with the current seed, so a foreign key id is
/// as much a laundering vector as a stripped signature. Legit rotation is the
/// explicit migration escape (see `resume_tolerates_unsigned_only_under_migration_escape`).
#[test]
fn resume_fails_closed_on_foreign_key_id() {
    let cp = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
    // Current deployment uses key id 2 + a different seed — cannot verify.
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_B), 2, TENANT, REGION, false),
        HeadResumeCheck::FailClosed
    );
}

/// A signed head with NO seed configured to verify it → fail-CLOSED (cannot
/// prove integrity of a head that claims to be signed).
#[test]
fn resume_fails_closed_when_signed_but_no_seed() {
    let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
    assert_eq!(
        check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
        HeadResumeCheck::FailClosed
    );
}

/// No signing regime (no seed) + a never-signed head → Proceed (dev/CI).
#[test]
fn resume_proceeds_unsigned_head_no_seed() {
    let cp = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
        HeadResumeCheck::Proceed
    );
}

/// No checkpoint (genesis) → proceed.
#[test]
fn resume_proceeds_with_no_checkpoint() {
    assert_eq!(
        check_head_on_resume(None, Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::Proceed
    );
}

/// `region_for_key` is irrelevant to the keypair: a macro region with no enum
/// (e.g. `apac`) still signs + verifies (the region is bound in the tuple,
/// not the key).
#[test]
fn macro_region_still_signs_and_verifies() {
    let sig = sign_head(&SEED_A, KID, TENANT, "apac", &head_hex(), 5).unwrap();
    assert!(verify_head(
        &SEED_A,
        KID,
        TENANT,
        "apac",
        &head_hex(),
        5,
        &sig
    ));
    // But it is region-bound: a different region must fail.
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        "afr",
        &head_hex(),
        5,
        &sig
    ));
}

// ---- B-038: partition lease + seal-loop fence ----

/// A batch of `n` deterministic sealed rows to drive the fenced loop.
fn sealed_rows(n: usize) -> Vec<SealedRow> {
    let payloads: Vec<Value> = (0..n).map(|i| json!({ "i": i })).collect();
    let (sealed, _, _) = seal_rows(ChainHash::genesis(), 0, &rows(&payloads)).unwrap();
    sealed
}

/// The fence decision is pure: it trips ONLY when the lease is enabled AND the
/// clock has reached the lease expiry. When the lease is OFF it is never `true`
/// regardless of the clock, so the seal loop always runs to completion.
#[test]
fn should_fence_only_when_enabled_and_expired() {
    // Lease OFF ⇒ never fence, even far past "expiry".
    assert!(!should_fence(1_000, 500, false));
    assert!(!should_fence(i64::MAX, 0, false));
    // Lease ON, before expiry ⇒ keep sealing.
    assert!(!should_fence(499, 500, true));
    // Lease ON, at/after expiry ⇒ fence (>= boundary is inclusive).
    assert!(should_fence(500, 500, true));
    assert!(should_fence(501, 500, true));
}

// ---- B-125: bounded JSON1 seal writes + fail-closed mutation semantics ----

/// The production mutation is one bounded JSON1 UPDATE per chunk, not one D1
/// request per row. Keep the SQL shape under test so a later edit cannot remove
/// the pending-row guard or turn the statement back into an unbounded write.
#[test]
fn seal_chunk_sql_is_bounded_and_guarded() {
    assert_eq!(AUDIT_SEAL_ROWS_PER_STATEMENT, 32);
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("json_each(?1)"));
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("emitted_at IS NULL"));
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("pending.enqueued_at <="));
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("collision.sequence_number"));
    assert!(
        !AUDIT_SEAL_CHUNK_SQL.contains("json_extract(candidate.value, '$.id') = audit_outbox.id")
    );
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("json_array_length(?1)"));
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("RETURNING id"));
}

#[test]
fn existing_sequence_collision_blocks_entire_chunk_without_partial_update() {
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
    conn.execute_batch(
        "CREATE TABLE audit_outbox (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            sequence_number INTEGER,
            prev_hash TEXT,
            chain_hash TEXT,
            canonical_jcs TEXT,
            chained_at INTEGER,
            emitted_at INTEGER,
            algorithm_id INTEGER,
            epoch_id INTEGER,
            link_key_id INTEGER,
            enqueued_at INTEGER NOT NULL
        );",
    )
    .expect("schema");
    conn.execute(
        "INSERT INTO audit_outbox (id,tenant_id,region,enqueued_at,emitted_at,sequence_number) VALUES (?1,'tenant','enam',100,NULL,NULL)",
        rusqlite::params!["candidate-10"],
    )
    .expect("candidate 10");
    conn.execute(
        "INSERT INTO audit_outbox (id,tenant_id,region,enqueued_at,emitted_at,sequence_number) VALUES (?1,'tenant','enam',100,NULL,NULL)",
        rusqlite::params!["candidate-11"],
    )
    .expect("candidate 11");
    conn.execute(
        "INSERT INTO audit_outbox (id,tenant_id,region,enqueued_at,emitted_at,sequence_number) VALUES (?1,'tenant','enam',1,200,11)",
        rusqlite::params!["existing-11"],
    )
    .expect("existing sequence 11");

    let payload = serde_json::json!([
        {"id":"candidate-10","sequence_number":10,"prev_hash":"00","chain_hash":"10","canonical_jcs":"{}","sealed_at":200,"algorithm_id":0,"epoch_id":0,"link_key_id":null},
        {"id":"candidate-11","sequence_number":11,"prev_hash":"10","chain_hash":"11","canonical_jcs":"{}","sealed_at":200,"algorithm_id":0,"epoch_id":0,"link_key_id":null}
    ])
    .to_string();
    let mut statement = conn
        .prepare(AUDIT_SEAL_CHUNK_SQL)
        .expect("prepare chunk SQL");
    let returned: Vec<String> = statement
        .query_map(rusqlite::params![payload], |row| row.get(0))
        .expect("execute chunk SQL")
        .collect::<Result<_, _>>()
        .expect("returning IDs");
    assert!(returned.is_empty(), "collision must block the whole chunk");

    let mut check = conn
        .prepare("SELECT sequence_number, emitted_at FROM audit_outbox WHERE id = ?1")
        .expect("prepare check");
    for id in ["candidate-10", "candidate-11"] {
        let (sequence, emitted): (Option<i64>, Option<i64>) = check
            .query_row(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("read candidate");
        assert_eq!(
            (sequence, emitted),
            (None, None),
            "{id} was partially updated"
        );
    }
}

#[test]
fn duplicate_sealed_tail_sequence_fails_closed() {
    let mut first = serde_json::Map::new();
    first.insert("sequence_number".to_owned(), json!(7));
    let mut second = serde_json::Map::new();
    second.insert("sequence_number".to_owned(), json!(7));

    let err = reject_duplicate_sealed_tail(&[first, second]).unwrap_err();
    assert!(err.contains("duplicate sequence_number 7"));
}

#[test]
fn distinct_sealed_tail_sequences_are_not_rejected() {
    let mut first = serde_json::Map::new();
    first.insert("sequence_number".to_owned(), json!(7));
    let mut second = serde_json::Map::new();
    second.insert("sequence_number".to_owned(), json!(6));

    assert!(reject_duplicate_sealed_tail(&[first, second]).is_ok());
}

fn sealed_tail_candidate(
    id: &str,
    sequence: u64,
    prev_byte: u8,
    canonical_jcs: &str,
    quarantined: bool,
    sequence_count: u64,
) -> crate::storage::d1_http::D1Row {
    let prev = ChainHash([prev_byte; 32]);
    let hash =
        link_for_epoch(&ChainEpoch::legacy(), &prev, canonical_jcs.as_bytes(), None).unwrap();
    let mut row = serde_json::Map::new();
    row.insert("id".to_owned(), json!(id));
    row.insert("sequence_number".to_owned(), json!(sequence));
    row.insert("prev_hash".to_owned(), json!(prev.to_hex()));
    row.insert("chain_hash".to_owned(), json!(hash.to_hex()));
    row.insert("canonical_jcs".to_owned(), json!(canonical_jcs));
    row.insert("sequence_count".to_owned(), json!(sequence_count));
    row.insert(
        "quarantined_at".to_owned(),
        if quarantined {
            json!(1_700_000_000_000i64)
        } else {
            Value::Null
        },
    );
    row.insert(
        "quarantine_reason".to_owned(),
        if quarantined {
            json!("sequence_fork:legacy")
        } else {
            Value::Null
        },
    );
    row.insert(
        "archived_at".to_owned(),
        if quarantined {
            Value::Null
        } else {
            json!(1_700_000_000_001i64)
        },
    );
    row.insert("algorithm_id".to_owned(), Value::Null);
    row.insert("epoch_id".to_owned(), Value::Null);
    row.insert("link_key_id".to_owned(), Value::Null);
    row
}

fn candidate_hash(row: &crate::storage::d1_http::D1Row) -> String {
    row.get("chain_hash").unwrap().as_str().unwrap().to_owned()
}

fn signed_tail_resolution(
    rows: &[crate::storage::d1_http::D1Row],
    selected: usize,
    checkpoint: &HeadCheckpoint,
) -> LegacyTailResolution {
    let mut resolution = LegacyTailResolution {
        candidate_count: rows.len() as u64,
        candidate_set_hash: legacy_tail_candidate_set_hash(rows).unwrap(),
        checkpoint_head_hash: checkpoint.head_hex.clone(),
        checkpoint_head_signature: checkpoint.head_signature.clone().unwrap(),
        checkpoint_next_sequence: checkpoint.next_sequence,
        created_at_ms: 1_700_000_000_000,
        resolution_signature: String::new(),
        resolution_version: 1,
        selected_chain_hash: candidate_hash(&rows[selected]),
        selected_row_id: rows[selected]["id"].as_str().unwrap().to_owned(),
        signing_key_id: KID,
        tail_sequence: rows[selected]["sequence_number"].as_u64().unwrap(),
    };
    let canonical = resolution.canonical_bytes(TENANT, REGION).unwrap();
    let key = ErasureSigningKey::from_seed(KID, region_for_key(REGION), 0, 0, SEED_A);
    resolution.resolution_signature = base64::engine::general_purpose::STANDARD
        .encode(key.signing_key.sign(&canonical).to_bytes());
    resolution
}

fn tail_resume_context<'a>(
    checkpoint: Option<&'a HeadCheckpoint>,
    head_check: HeadResumeCheck,
    signing_seed: Option<&'a [u8; 32]>,
    trust_unsigned_resume: bool,
) -> LegacyTailResumeContext<'a> {
    LegacyTailResumeContext {
        checkpoint,
        head_check,
        signing_seed,
        signing_key_id: KID,
        trust_unsigned_resume,
        tenant_id: TENANT,
        region: REGION,
    }
}

#[test]
fn unique_sealed_tail_needs_no_checkpoint_selection() {
    let row = sealed_tail_candidate("selected", 7, 0, "{}", false, 1);
    let expected = chain_hash_from_hex(row["chain_hash"].as_str().unwrap()).unwrap();
    let resolved = resolve_sealed_tail_rows(
        &[row],
        None,
        &tail_resume_context(None, HeadResumeCheck::Proceed, None, false),
    )
    .unwrap();
    assert_eq!(resolved, Some((expected, 7)));
}

#[test]
fn duplicate_tail_requires_exact_verified_signed_branch() {
    let rows = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("loser", 0, 1, "{\"fork\":true}", true, 2),
    ];
    let selected_hash = candidate_hash(&rows[0]);
    let checkpoint = signed_checkpoint(&SEED_A, KID, &selected_hash, 1);
    let resolution = signed_tail_resolution(&rows, 0, &checkpoint);
    let resolved = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap();
    assert_eq!(resolved, Some((checkpoint.head, 0)));

    let err = resolve_sealed_tail_rows(
        &rows,
        None,
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("without an explicit signed resolution"));

    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Proceed,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("without an exact verified checkpoint"));

    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            true,
        ),
    )
    .unwrap_err();
    assert!(err.contains("without an exact verified checkpoint"));
}

#[test]
fn duplicate_tail_rejects_candidate_set_or_signature_tamper() {
    let rows = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("loser", 0, 1, "{\"fork\":true}", true, 2),
    ];
    let checkpoint = signed_checkpoint(&SEED_A, KID, &candidate_hash(&rows[0]), 1);
    let mut resolution = signed_tail_resolution(&rows, 0, &checkpoint);
    resolution.candidate_set_hash = "00".repeat(32);
    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("commitment/signature does not verify"));
}

#[test]
fn duplicate_tail_rejects_two_rows_matching_signed_hash() {
    let rows = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("same-hash-loser", 0, 0, "{}", true, 2),
    ];
    let checkpoint = signed_checkpoint(&SEED_A, KID, &candidate_hash(&rows[0]), 1);
    let resolution = signed_tail_resolution(&rows, 0, &checkpoint);
    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("2 candidates matching"));
}

#[test]
fn duplicate_tail_rejects_loser_that_was_also_archived() {
    let mut rows = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("loser", 0, 1, "{\"fork\":true}", true, 2),
    ];
    rows[1].insert("archived_at".to_owned(), json!(1_700_000_000_002i64));
    let checkpoint = signed_checkpoint(&SEED_A, KID, &candidate_hash(&rows[0]), 1);
    let resolution = signed_tail_resolution(&rows, 0, &checkpoint);
    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("not exclusively quarantined"));
}

#[test]
fn duplicate_tail_rejects_ahead_head_and_unquarantined_loser() {
    let rows = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("loser", 0, 1, "{\"fork\":true}", true, 2),
    ];
    let selected_hash = candidate_hash(&rows[0]);
    let ahead = signed_checkpoint(&SEED_A, KID, &selected_hash, 2);
    let resolution = signed_tail_resolution(&rows, 0, &ahead);
    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&ahead),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("without an exact verified checkpoint"));

    let active_loser = vec![
        sealed_tail_candidate("selected", 0, 0, "{}", false, 2),
        sealed_tail_candidate("loser", 0, 1, "{\"fork\":true}", false, 2),
    ];
    let exact = signed_checkpoint(&SEED_A, KID, &candidate_hash(&active_loser[0]), 1);
    let active_resolution = signed_tail_resolution(&active_loser, 0, &exact);
    let err = resolve_sealed_tail_rows(
        &active_loser,
        Some(&active_resolution),
        &tail_resume_context(
            Some(&exact),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("not exclusively quarantined"));
}

#[test]
fn duplicate_tail_rejects_missing_signed_branch_and_hidden_third_candidate() {
    let rows = vec![
        sealed_tail_candidate("fork-a", 0, 1, "{}", true, 2),
        sealed_tail_candidate("fork-b", 0, 2, "{}", true, 2),
    ];
    let checkpoint = signed_checkpoint(&SEED_A, KID, &head_hex(), 1);
    let mut resolution = signed_tail_resolution(&rows, 0, &checkpoint);
    resolution.selected_chain_hash = checkpoint.head_hex.clone();
    resolution.selected_row_id = "absent".to_owned();
    let canonical = resolution.canonical_bytes(TENANT, REGION).unwrap();
    let key = ErasureSigningKey::from_seed(KID, region_for_key(REGION), 0, 0, SEED_A);
    resolution.resolution_signature = base64::engine::general_purpose::STANDARD
        .encode(key.signing_key.sign(&canonical).to_bytes());
    let err = resolve_sealed_tail_rows(
        &rows,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("0 candidates matching"));

    let hidden = vec![sealed_tail_candidate("selected", 0, 0, "{}", false, 3)];
    let err = resolve_sealed_tail_rows(
        &hidden,
        Some(&resolution),
        &tail_resume_context(
            Some(&checkpoint),
            HeadResumeCheck::Verified,
            Some(&SEED_A),
            false,
        ),
    )
    .unwrap_err();
    assert!(err.contains("has 3 candidates"));
}

#[test]
fn head_advance_cas_binds_the_verified_checkpoint_snapshot() {
    let source = include_str!("b126_m2_impl_02.rs");
    for guard in [
        "head_signature IS ?11",
        "signing_key_id IS ?12",
        "epoch_id IS ?13",
        "head_message_version IS ?14",
        "epoch_ledger_sequence IS ?15",
        "epoch_ledger_hash IS ?16",
        "head_witness_sequence IS ?17",
        "head_witness_hash IS ?18",
        "json!(cp.head_signature.as_deref())",
        "json!(cp.epoch_ledger_hash.as_deref())",
        "json!(cp.head_witness_hash.as_deref())",
    ] {
        assert!(
            source.contains(guard),
            "missing CAS snapshot guard/bind: {guard}"
        );
    }
}

#[test]
fn seal_chunk_payload_rejects_empty_and_duplicate_mutations() {
    assert!(seal_chunk_payload(&[], 1_700_000_000_000).is_err());

    let rows = sealed_rows(2);
    let duplicate = vec![rows[0].clone(), rows[0].clone()];
    let err = seal_chunk_payload(&duplicate, 1_700_000_000_000).unwrap_err();
    assert!(err.contains("duplicate id"));

    let mut same_sequence = rows[1].clone();
    same_sequence.sequence_number = rows[0].sequence_number;
    let err = seal_chunk_payload(&[rows[0].clone(), same_sequence], 1_700_000_000_000).unwrap_err();
    assert!(err.contains("duplicate sequence_number"));
}

#[test]
fn seal_chunk_payload_preserves_exact_seal_fields() {
    let rows = sealed_rows(2);
    let payload = seal_chunk_payload(&rows, 1_700_000_000_000).unwrap();
    let encoded = payload
        .as_str()
        .expect("JSON array encoded as one D1 value");
    let parsed: Value = serde_json::from_str(encoded).expect("valid JSON array payload");
    let values = parsed.as_array().expect("JSON array payload");
    assert_eq!(values.len(), rows.len());
    assert_eq!(values[0]["id"], rows[0].id);
    assert_eq!(values[0]["sequence_number"], json!(rows[0].sequence_number));
    assert_eq!(values[0]["prev_hash"], rows[0].prev_hash_hex);
    assert_eq!(values[0]["chain_hash"], rows[0].chain_hash_hex);
    assert_eq!(values[0]["canonical_jcs"], rows[0].canonical_jcs);
    assert_eq!(values[0]["sealed_at"], json!(1_700_000_000_000i64));
}

#[test]
fn v2_prefix_is_bounded_before_the_external_witness_moves() {
    assert_eq!(
        AUDIT_V2_MAX_ROWS_PER_TRANSACTION.div_ceil(AUDIT_SEAL_ROWS_PER_STATEMENT) + 4,
        AUDIT_V2_MAX_BATCH_STATEMENTS
    );

    let mut candidates = sealed_rows(2);
    for row in &mut candidates {
        row.canonical_jcs = "x".repeat(400 * 1024);
    }
    let (prefix, head, next_sequence) =
        bounded_v2_sealed_prefix(&candidates, 1_700_000_000_000).unwrap();
    assert_eq!(
        prefix.len(),
        1,
        "aggregate assertion value must shrink the prefix"
    );
    assert_eq!(head.to_hex(), prefix[0].chain_hash_hex);
    assert_eq!(next_sequence, prefix[0].sequence_number + 1);

    candidates[0].canonical_jcs = "\"\\".repeat(AUDIT_V2_MAX_SERIALIZED_ROW_BYTES / 2);
    assert!(bounded_v2_sealed_prefix(&candidates[..1], 1_700_000_000_000).is_err());
}

fn returning_id(id: &str) -> crate::storage::d1_http::D1Row {
    let mut row = serde_json::Map::new();
    row.insert("id".to_owned(), json!(id));
    row
}

#[test]
fn seal_chunk_result_requires_exact_returning_id_set() {
    let rows = sealed_rows(2);
    let good = vec![returning_id(&rows[0].id), returning_id(&rows[1].id)];
    assert!(validate_seal_chunk_result(&good, &rows).is_ok());

    // Any partial, unexpected, duplicate, extra-column, or non-text result is
    // ambiguous and must prevent head advancement.
    assert!(validate_seal_chunk_result(&good[..1], &rows).is_err());
    assert!(validate_seal_chunk_result(
        &[returning_id(&rows[0].id), returning_id("not-expected")],
        &rows
    )
    .is_err());
    assert!(validate_seal_chunk_result(
        &[returning_id(&rows[0].id), returning_id(&rows[0].id)],
        &rows
    )
    .is_err());
    let mut extra = returning_id(&rows[0].id);
    extra.insert("changed".to_owned(), json!(true));
    assert!(validate_seal_chunk_result(&[extra, returning_id(&rows[1].id)], &rows).is_err());
    let mut non_text = serde_json::Map::new();
    non_text.insert("id".to_owned(), json!(42));
    assert!(validate_seal_chunk_result(&[non_text, returning_id(&rows[1].id)], &rows).is_err());
}

#[test]
fn b125_smallest_above_budget_burst_is_bounded_complete_and_idempotent() {
    use std::time::Instant;

    // 513 is the smallest burst above the production per-call budget of 512.
    // Exercise the real JSON1 UPDATE in an isolated in-memory database: a live
    // production injection cannot be cleaned up once these append-only rows are
    // sealed, so it is deliberately forbidden by the B-125 owner packet.
    const BURST_ROWS: usize = 513;
    let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
    conn.execute_batch(
        "CREATE TABLE audit_outbox (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            sequence_number INTEGER,
            prev_hash TEXT,
            chain_hash TEXT,
            canonical_jcs TEXT,
            chained_at INTEGER,
            emitted_at INTEGER,
            algorithm_id INTEGER,
            epoch_id INTEGER,
            link_key_id INTEGER,
            enqueued_at INTEGER NOT NULL
        );",
    )
    .expect("schema");

    let sealed = sealed_rows(BURST_ROWS);
    for row in &sealed {
        conn.execute(
            "INSERT INTO audit_outbox (id,tenant_id,region,enqueued_at) VALUES (?1,'b125-synthetic','enam',100)",
            rusqlite::params![row.id],
        )
        .expect("synthetic row");
    }

    let started = Instant::now();
    let mut statement_sizes = Vec::new();
    for chunk in sealed.chunks(AUDIT_SEAL_ROWS_PER_STATEMENT) {
        let payload = seal_chunk_payload(chunk, 200).expect("chunk payload");
        let payload = payload.as_str().expect("encoded JSON1 string");
        let mut statement = conn
            .prepare(AUDIT_SEAL_CHUNK_SQL)
            .expect("prepare chunk SQL");
        let returned: Vec<crate::storage::d1_http::D1Row> = statement
            .query_map(rusqlite::params![payload], |row| {
                let mut out = serde_json::Map::new();
                out.insert("id".to_owned(), json!(row.get::<_, String>(0)?));
                Ok(out)
            })
            .expect("execute chunk SQL")
            .collect::<Result<_, _>>()
            .expect("returning rows");
        validate_seal_chunk_result(&returned, chunk).expect("exact returned id set");
        statement_sizes.push(returned.len());
    }
    let elapsed = started.elapsed();

    assert_eq!(statement_sizes.len(), 17, "sixteen full chunks plus one row");
    assert_eq!(&statement_sizes[..16], &[32; 16]);
    assert_eq!(statement_sizes[16], 1);
    assert_eq!(statement_sizes.iter().sum::<usize>(), BURST_ROWS);
    let (sealed_count, distinct_sequences, pending): (usize, usize, usize) = conn
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT sequence_number), SUM(emitted_at IS NULL) FROM audit_outbox",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("population readback");
    assert_eq!((sealed_count, distinct_sequences, pending), (513, 513, 0));

    // Replay control: an already-sealed chunk updates zero rows. The route's
    // pending scan therefore has no work and cannot duplicate the chain.
    let replay_payload = seal_chunk_payload(&sealed[..32], 300).expect("replay payload");
    let replay_payload = replay_payload.as_str().expect("encoded replay JSON1 string");
    let replayed = conn
        .prepare(AUDIT_SEAL_CHUNK_SQL)
        .expect("prepare replay")
        .query_map(rusqlite::params![replay_payload], |row| row.get::<_, String>(0))
        .expect("execute replay")
        .collect::<Result<Vec<_>, _>>()
        .expect("replay rows");
    assert!(replayed.is_empty(), "replay must mutate zero sealed rows");

    tracing::info!(
        "B125 isolated burst: rows={BURST_ROWS} statements={} elapsed_us={} rows_per_second={:.2}",
        statement_sizes.len(),
        elapsed.as_micros(),
        BURST_ROWS as f64 / elapsed.as_secs_f64()
    );
}

#[tokio::test]
async fn chunked_fence_commits_only_bounded_prefix_and_reports_it() {
    let sealed = sealed_rows(AUDIT_SEAL_ROWS_PER_STATEMENT * 2 + 1);
    let mut clock_calls = 0;
    let clock = move || {
        clock_calls += 1;
        if clock_calls >= 4 {
            1_000
        } else {
            900
        }
    };
    let mut chunk_lengths = Vec::new();
    let outcome = run_chunked_fenced_seal_loop(&sealed, true, 1_000, clock, |chunk| {
        chunk_lengths.push(chunk.len());
        async { Ok(()) }
    })
    .await
    .unwrap();

    assert_eq!(chunk_lengths, vec![32, 32]);
    assert_eq!(outcome, FencedSeal::Fenced(64));
}

#[tokio::test]
async fn chunked_seal_write_error_stops_before_next_chunk() {
    let sealed = sealed_rows(AUDIT_SEAL_ROWS_PER_STATEMENT + 1);
    let mut attempted = Vec::new();
    let err = run_chunked_fenced_seal_loop(
        &sealed,
        false,
        0,
        || 0,
        |chunk| {
            attempted.push(chunk.len());
            async { Err("atomic chunk failed".to_owned()) }
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err, "atomic chunk failed");
    assert_eq!(attempted, vec![AUDIT_SEAL_ROWS_PER_STATEMENT]);
}

/// Sweep plumbing: a `Fenced(n)` outcome is accounted like a truncated batch —
/// its `n` rows count into `rows_sealed` and it forces `incomplete = true`,
/// while a `Leased` outcome only increments `partitions_leased`. This mirrors
/// the exact arithmetic the `handle_drain` match arms perform (kept in lock-step
/// with them) without needing a live D1.
#[test]
fn sweep_accounts_fenced_and_leased_outcomes() {
    // Replicate the match-arm accounting over a synthetic outcome stream.
    let outcomes = [
        PartitionOutcome::Sealed(4),
        PartitionOutcome::Leased,
        PartitionOutcome::Fenced(3),
        PartitionOutcome::Leased,
        PartitionOutcome::Drift,
        PartitionOutcome::Empty,
    ];
    let mut rows_sealed: u64 = 0;
    let mut partitions_drained: u64 = 0;
    let mut partitions_leased: u64 = 0;
    let mut partitions_drifted: u64 = 0;
    let mut incomplete = false;
    for o in outcomes {
        match o {
            PartitionOutcome::Sealed(n) => {
                partitions_drained += 1;
                rows_sealed += n;
            }
            PartitionOutcome::Fenced(n) => {
                rows_sealed += n;
                incomplete = true;
            }
            PartitionOutcome::Leased => partitions_leased += 1,
            PartitionOutcome::Drift => partitions_drifted += 1,
            PartitionOutcome::Empty => {}
        }
    }
    assert_eq!(rows_sealed, 7); // 4 sealed + 3 fenced prefix
    assert_eq!(partitions_drained, 1);
    assert_eq!(partitions_leased, 2);
    assert_eq!(partitions_drifted, 1);
    assert!(incomplete, "a fenced partition must force a re-drain");
}

/// Two lease holders minted for the same partition are DISTINCT, so a stealer's
/// holder-scoped release can never delete the other holder's lease.
#[test]
fn lease_holders_are_unique_per_invocation() {
    let a = new_lease_holder();
    let b = new_lease_holder();
    assert_ne!(a, b);
    assert_eq!(a.len(), 36, "uuid v4 hyphenated form is 36 chars");
}

/// The fence boundary equals the lease's written `expires_ms` (acquired_ms +
/// TTL), so a holder stops writing exactly when its lease becomes stealable —
/// never after. (The acquire SQL writes the same `acquired_ms + TTL`.)
#[test]
fn fence_expiry_matches_lease_ttl() {
    let acquired_ms = 1_700_000_000_000;
    let my_lease_expires_ms = acquired_ms + AUDIT_DRAIN_LEASE_TTL_MS;
    // At exactly the written expiry the holder fences (does not write past it).
    assert!(should_fence(my_lease_expires_ms, my_lease_expires_ms, true));
    // One ms before, it keeps going.
    assert!(!should_fence(
        my_lease_expires_ms - 1,
        my_lease_expires_ms,
        true
    ));
}

// ---- WP-L MED-21: `ok` is `partitions_failed == 0` ------------------
//
// The pre-fix shape returned `ok: true` regardless of how many
// partitions actually failed — masking a chronically-stuck chain
// under a green-looking response. The fix inverts that.

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();
