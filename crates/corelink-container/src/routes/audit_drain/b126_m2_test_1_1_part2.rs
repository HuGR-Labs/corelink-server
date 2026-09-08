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

/// THE FENCE TEST (reaches the residual fork window). Drive the REAL seal loop
/// with a clock that crosses `my_lease_expires_ms` partway through and assert:
/// only the pre-expiry PREFIX was written, and the outcome is `Fenced(prefix)`
/// — so the caller does NOT advance the head. This is the path the prod probe
/// structurally cannot reach.
#[tokio::test]
async fn fence_stops_seal_loop_at_expiry_writing_only_the_prefix() {
    let sealed = sealed_rows(5);
    let expires = 1_000;
    // Clock: rows 0,1,2 see now < expiry (900); row 3 sees now == expiry
    // (1000) → fence BEFORE writing row 3. `should_fence` reads the clock once
    // per row, before each write, so 4 readings are consumed (rows 0..=3).
    let ticks = [900, 900, 900, 1_000, 1_000];
    let mut tick = ticks.into_iter();
    let clock = move || tick.next().unwrap_or(i64::MAX);

    let mut written: Vec<u64> = Vec::new();
    let outcome = run_fenced_seal_loop(&sealed, true, expires, clock, |row| {
        written.push(row.sequence_number);
        async { Ok(()) }
    })
    .await
    .unwrap();

    // Fenced after the 3-row prefix (sequences 0,1,2); rows 3,4 were NOT written.
    assert_eq!(outcome, FencedSeal::Fenced(3));
    assert_eq!(written, vec![0, 1, 2]);
}

/// With the lease OFF the fence is inert: the loop seals every row and reports
/// `Complete`, even when the clock is past a (would-be) expiry — proving the
/// flag-OFF default writes the whole batch exactly as before B-038.
#[tokio::test]
async fn lease_disabled_seals_whole_batch_completely() {
    let sealed = sealed_rows(4);
    let mut written: Vec<u64> = Vec::new();
    let outcome = run_fenced_seal_loop(
        &sealed,
        false,       // lease OFF
        0,           // expiry already in the past — must be ignored
        || i64::MAX, // clock way past expiry
        |row| {
            written.push(row.sequence_number);
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome, FencedSeal::Complete(4));
    assert_eq!(written, vec![0, 1, 2, 3]);
}

/// Lease ON but the clock never reaches expiry ⇒ the whole batch is sealed and
/// the head may advance (Complete). The fence only bites when the lease truly
/// expires mid-seal.
#[tokio::test]
async fn lease_enabled_but_not_expired_completes() {
    let sealed = sealed_rows(3);
    let mut written = 0u64;
    let outcome = run_fenced_seal_loop(
        &sealed,
        true,
        i64::MAX,
        || 0,
        |_row| {
            written += 1;
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome, FencedSeal::Complete(3));
    assert_eq!(written, 3);
}

/// A write error propagates (the loop is not swallowed by the fence).
#[tokio::test]
async fn seal_loop_propagates_write_error() {
    let sealed = sealed_rows(2);
    let err = run_fenced_seal_loop(
        &sealed,
        true,
        i64::MAX,
        || 0,
        |_row| async { Err("boom".to_string()) },
    )
    .await;
    assert_eq!(err, Err("boom".to_string()));
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
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("json_array_length(?1)"));
    assert!(AUDIT_SEAL_CHUNK_SQL.contains("RETURNING id"));
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
