/// Validate the exact `RETURNING id` result from a seal chunk.  A missing,
/// duplicate, unexpected, or malformed id is an error; callers never advance
/// the chain head on an ambiguous D1 result.
fn validate_seal_chunk_result(
    result: &[crate::storage::d1_http::D1Row],
    expected: &[SealedRow],
) -> Result<(), String> {
    if result.len() != expected.len() {
        return Err(format!(
            "audit seal chunk updated {} rows, expected {}",
            result.len(),
            expected.len()
        ));
    }
    let expected_ids: std::collections::HashSet<&str> =
        expected.iter().map(|row| row.id.as_str()).collect();
    let mut seen = std::collections::HashSet::with_capacity(result.len());
    for returned in result {
        if returned.len() != 1 {
            return Err("audit seal RETURNING row has unexpected columns".to_owned());
        }
        let id = returned
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit seal RETURNING id missing/non-text")?;
        if !expected_ids.contains(id) || !seen.insert(id) {
            return Err(format!(
                "audit seal RETURNING id is unexpected/duplicate: {id}"
            ));
        }
    }
    Ok(())
}

/// Write one bounded sealed-row chunk atomically, guarded by `emitted_at IS
/// NULL`.  A partial/raced result is rejected before the caller advances the
/// signed head; the count predicate makes the UPDATE itself all-or-nothing.
async fn write_seal_chunk(d1: &D1HttpClient, rows: &[SealedRow], now: i64) -> Result<(), String> {
    let payload = seal_chunk_payload(rows, now)?;
    let result = d1.query(AUDIT_SEAL_CHUNK_SQL, &[payload]).await?;
    validate_seal_chunk_result(&result, rows)
}

/// Advance the `audit_chain_head` checkpoint with a compare-and-set on the value
/// we resumed from (single-writer anti-fork). Returns `true` when the head was
/// committed, `false` on drift (a concurrent drain advanced it first).
///
/// Uses `RETURNING` to detect whether the guarded write actually matched: a
/// guarded `UPDATE` (or an `INSERT OR IGNORE`) that hits no row / a conflict
/// returns zero rows ⇒ drift.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit, no shared config struct"
)]
async fn advance_head_cas(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    expected: Option<&HeadCheckpoint>,
    new_head_hex: &str,
    new_seq: u64,
    now: i64,
    // CF-6: the keyed-head columns persisted alongside the advance. `signature`
    // is `None` when no seed is configured (head advanced UNSIGNED / legacy).
    signature: Option<&str>,
    signed_at_ms: Option<i64>,
    signing_key_id: Option<u64>,
) -> Result<bool, String> {
    let new_seq_i = i64::try_from(new_seq).map_err(|_| "next_sequence exceeds i64")?;
    let signing_key_id_i = signing_key_id
        .map(|k| i64::try_from(k).map_err(|_| "signing_key_id exceeds i64"))
        .transpose()?;
    let rows = match expected {
        Some(cp) => {
            let exp_seq =
                i64::try_from(cp.next_sequence).map_err(|_| "next_sequence exceeds i64")?;
            let expected_key_id = cp
                .signing_key_id
                .map(|value| {
                    i64::try_from(value).map_err(|_| "expected signing_key_id exceeds i64")
                })
                .transpose()?;
            let expected_epoch_id = cp
                .epoch_id
                .map(|value| i64::try_from(value).map_err(|_| "expected epoch_id exceeds i64"))
                .transpose()?;
            let expected_head_version = cp.head_message_version.map(i64::from);
            let expected_ledger_sequence = cp
                .epoch_ledger_sequence
                .map(|value| {
                    i64::try_from(value).map_err(|_| "expected ledger sequence exceeds i64")
                })
                .transpose()?;
            let expected_witness_sequence = cp
                .head_witness_sequence
                .map(|value| {
                    i64::try_from(value).map_err(|_| "expected witness sequence exceeds i64")
                })
                .transpose()?;
            d1.query(
                "UPDATE audit_chain_head \
                 SET head_hash = ?1, next_sequence = ?2, updated_at = ?3, \
                     head_signature = ?8, head_signed_at_ms = ?9, signing_key_id = ?10 \
                 WHERE tenant_id = ?4 AND region = ?5 \
                   AND head_hash = ?6 AND next_sequence = ?7 \
                   AND head_signature IS ?11 AND signing_key_id IS ?12 \
                   AND epoch_id IS ?13 AND head_message_version IS ?14 \
                   AND epoch_ledger_sequence IS ?15 AND epoch_ledger_hash IS ?16 \
                   AND head_witness_sequence IS ?17 AND head_witness_hash IS ?18 \
                 RETURNING tenant_id",
                &[
                    json!(new_head_hex),
                    json!(new_seq_i),
                    json!(now),
                    json!(tenant_id),
                    json!(region),
                    json!(cp.head_hex),
                    json!(exp_seq),
                    json!(signature),
                    json!(signed_at_ms),
                    json!(signing_key_id_i),
                    json!(cp.head_signature.as_deref()),
                    json!(expected_key_id),
                    json!(expected_epoch_id),
                    json!(expected_head_version),
                    json!(expected_ledger_sequence),
                    json!(cp.epoch_ledger_hash.as_deref()),
                    json!(expected_witness_sequence),
                    json!(cp.head_witness_hash.as_deref()),
                ],
            )
            .await?
        }
        None => {
            // No checkpoint row (genesis, or a crashed drain never created one):
            // claim it. A concurrent drain that inserted first wins the PK; our
            // ON CONFLICT DO NOTHING then returns zero rows ⇒ drift.
            d1.query(
                "INSERT INTO audit_chain_head \
                     (tenant_id, region, head_hash, next_sequence, updated_at, \
                      head_signature, head_signed_at_ms, signing_key_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(tenant_id, region) DO NOTHING \
                 RETURNING tenant_id",
                &[
                    json!(tenant_id),
                    json!(region),
                    json!(new_head_hex),
                    json!(new_seq_i),
                    json!(now),
                    json!(signature),
                    json!(signed_at_ms),
                    json!(signing_key_id_i),
                ],
            )
            .await?
        }
    };
    Ok(!rows.is_empty())
}

/// Commit one already-witnessed v2 advance. Every sealed row, the exact
/// receipt and the guarded head CAS live in one D1 REST batch. The final
/// assertion INSERT deliberately violates a CHECK on any zero-row/partial
/// mutation, forcing SQLite to roll the complete batch back.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit cryptographic transaction boundary"
)]
async fn commit_v2_head_transaction(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    expected: &HeadCheckpoint,
    sealed: &[SealedRow],
    new_head_hex: &str,
    new_sequence: u64,
    now: i64,
    head_signature: &str,
    witness: &VerifiedWitness,
) -> Result<(), String> {
    if sealed.is_empty() {
        return Err("v2 witnessed transaction cannot commit an empty advance".to_owned());
    }
    let epoch_id = expected.epoch_id.ok_or("expected v2 epoch missing")?;
    let ledger_sequence = expected
        .epoch_ledger_sequence
        .ok_or("expected v2 ledger sequence missing")?;
    let ledger_hash = expected
        .epoch_ledger_hash
        .as_deref()
        .ok_or("expected v2 ledger hash missing")?;
    let expected_witness_sequence = expected
        .head_witness_sequence
        .ok_or("expected v2 witness sequence missing")?;
    let expected_witness_hash = expected
        .head_witness_hash
        .as_deref()
        .ok_or("expected v2 witness hash missing")?;
    let expected_signature = expected
        .head_signature
        .as_deref()
        .ok_or("expected v2 signature missing")?;
    let signing_key_id = expected
        .signing_key_id
        .ok_or("expected v2 signing key missing")?;
    if witness.record.witness_sequence != expected_witness_sequence.saturating_add(1)
        || witness.record.previous_witness_hash != expected_witness_hash
    {
        return Err("witness receipt is not the immediate D1 successor".to_owned());
    }
    let new_sequence_i = i64::try_from(new_sequence).map_err(|_| "new sequence exceeds i64")?;
    let expected_sequence_i =
        i64::try_from(expected.next_sequence).map_err(|_| "expected sequence exceeds i64")?;
    let epoch_id_i = i64::try_from(epoch_id).map_err(|_| "epoch id exceeds i64")?;
    let ledger_sequence_i =
        i64::try_from(ledger_sequence).map_err(|_| "ledger sequence exceeds i64")?;
    let signing_key_id_i =
        i64::try_from(signing_key_id).map_err(|_| "signing key id exceeds i64")?;
    let witness_sequence_i = i64::try_from(witness.record.witness_sequence)
        .map_err(|_| "witness sequence exceeds i64")?;
    let witness_key_id_i =
        i64::try_from(witness.witness_key_id).map_err(|_| "witness key id exceeds i64")?;
    let committed_at_i = i64::try_from(witness.receipt.committed_at_ms)
        .map_err(|_| "witness committed time exceeds i64")?;
    let witness_jcs_text = std::str::from_utf8(&witness.witness_jcs)
        .map_err(|_| "verified witness JCS is not UTF-8")?;
    let receipt_jcs_text = std::str::from_utf8(&witness.receipt_jcs)
        .map_err(|_| "verified witness receipt JCS is not UTF-8")?;
    let all_rows_payload = seal_chunk_payload(sealed, now)?;
    let mut statements =
        Vec::with_capacity(sealed.len().div_ceil(AUDIT_SEAL_ROWS_PER_STATEMENT) + 4);
    for chunk in sealed.chunks(AUDIT_SEAL_ROWS_PER_STATEMENT) {
        statements.push(D1BatchStatement::new(
            AUDIT_SEAL_CHUNK_SQL,
            vec![seal_chunk_payload(chunk, now)?],
        ));
    }
    statements.push(D1BatchStatement::new(
        "UPDATE audit_chain_head SET \
             head_hash=?1, next_sequence=?2, updated_at=?3, head_signature=?4, \
             head_signed_at_ms=?3, signing_key_id=?5, epoch_id=?6, head_message_version=2, \
             epoch_ledger_sequence=?7, epoch_ledger_hash=?8, \
             head_witness_sequence=?9, head_witness_hash=?10 \
         WHERE tenant_id=?11 AND region=?12 AND head_hash=?13 AND next_sequence=?14 \
           AND head_signature=?15 AND signing_key_id=?5 AND epoch_id=?6 \
           AND head_message_version=2 AND epoch_ledger_sequence=?7 AND epoch_ledger_hash=?8 \
           AND head_witness_sequence=?16 AND head_witness_hash=?17",
        vec![
            json!(new_head_hex),
            json!(new_sequence_i),
            json!(now),
            json!(head_signature),
            json!(signing_key_id_i),
            json!(epoch_id_i),
            json!(ledger_sequence_i),
            json!(ledger_hash),
            json!(witness_sequence_i),
            json!(witness.witness_record_hash),
            json!(tenant_id),
            json!(region),
            json!(expected.head_hex),
            json!(expected_sequence_i),
            json!(expected_signature),
            json!(i64::try_from(expected_witness_sequence)
                .map_err(|_| "expected witness sequence exceeds i64")?),
            json!(expected_witness_hash),
        ],
    ));
    statements.push(D1BatchStatement::new(
        "INSERT INTO audit_chain_witness_receipt \
             (tenant_id,region,witness_sequence,witness_record_hash,previous_witness_hash, \
              head_record_hash,witness_jcs,receipt_jcs,receipt_signature_b64,witness_id, \
              witness_key_id,committed_at_ms) \
         VALUES (?1,?2,?3,?4,?5,?6,CAST(?7 AS BLOB),CAST(?8 AS BLOB),?9,?10,?11,?12)",
        vec![
            json!(tenant_id),
            json!(region),
            json!(witness_sequence_i),
            json!(witness.witness_record_hash),
            json!(witness.record.previous_witness_hash),
            json!(witness.record.head_record_hash),
            json!(witness_jcs_text),
            json!(receipt_jcs_text),
            json!(witness.receipt_signature_b64),
            json!(witness.receipt.witness_id),
            json!(witness_key_id_i),
            json!(committed_at_i),
        ],
    ));
    let commit_id = uuid::Uuid::new_v4().to_string();
    statements.push(D1BatchStatement::new(
        "INSERT INTO audit_chain_v2_tx_assert(commit_id, assertion) \
         SELECT ?1, CASE WHEN \
           EXISTS (SELECT 1 FROM audit_chain_head WHERE tenant_id=?2 AND region=?3 \
             AND head_hash=?4 AND next_sequence=?5 AND head_signature=?6 \
             AND signing_key_id=?7 AND epoch_id=?8 AND head_message_version=2 \
             AND epoch_ledger_sequence=?9 AND epoch_ledger_hash=?10 \
             AND head_witness_sequence=?11 AND head_witness_hash=?12) \
           AND EXISTS (SELECT 1 FROM audit_chain_witness_receipt WHERE tenant_id=?2 AND region=?3 \
             AND witness_sequence=?11 AND witness_record_hash=?12 \
             AND receipt_signature_b64=?13) \
           AND (SELECT COUNT(*) FROM audit_outbox a JOIN json_each(?14) c \
                ON a.id=json_extract(c.value,'$.id') \
                WHERE a.tenant_id=?2 AND a.region=?3 \
                  AND a.sequence_number=json_extract(c.value,'$.sequence_number') \
                  AND a.prev_hash=json_extract(c.value,'$.prev_hash') \
                  AND a.chain_hash=json_extract(c.value,'$.chain_hash') \
                  AND a.canonical_jcs=json_extract(c.value,'$.canonical_jcs') \
                  AND a.emitted_at=json_extract(c.value,'$.sealed_at') \
                  AND a.algorithm_id=json_extract(c.value,'$.algorithm_id') \
                  AND a.epoch_id=json_extract(c.value,'$.epoch_id') \
                  AND a.link_key_id IS json_extract(c.value,'$.link_key_id'))=json_array_length(?14) \
         THEN 1 ELSE 0 END",
        vec![
            json!(commit_id), json!(tenant_id), json!(region), json!(new_head_hex),
            json!(new_sequence_i), json!(head_signature), json!(signing_key_id_i), json!(epoch_id_i),
            json!(ledger_sequence_i), json!(ledger_hash), json!(witness_sequence_i),
            json!(witness.witness_record_hash), json!(witness.receipt_signature_b64), all_rows_payload,
        ],
    ));
    statements.push(D1BatchStatement::new(
        "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1",
        vec![json!(commit_id)],
    ));
    d1.batch(statements).await.map(|_| ()).map_err(|error| {
        format!(
            "v2 D1 transaction rolled back at {:?}: {}",
            error.statement, error.message
        )
    })
}

/// CF-6 convergence: stamp the current-key signature onto a genuinely UNSIGNED
/// `audit_chain_head` IN PLACE — the SAME `(head_hash, next_sequence)`, no chain
/// mutation, no new row. Returns `Ok(true)` when a NULL head was signed, `Ok(false)`
/// when it was already signed (or a concurrent write signed it first).
///
/// SCOPE + SAFETY (hardened per the 2026-08-14 adversarial review):
/// - Only ever touches a head whose `head_signature IS NULL`. It NEVER re-signs a
///   head that already carries a signature — a current-key head with an INVALID
///   signature is left for `check_head_on_resume` to fail-CLOSE, never laundered, and
///   a foreign-key head is out of scope (key-rotation convergence is a separate,
///   old-signature-verifying operation, not this).
/// - The `… AND head_signature IS NULL` on the UPDATE makes the NULL→signed
///   transition ATOMIC: a concurrent normal drain (which changes head_hash/seq) or a
///   concurrent re-sign both lose the guard, so no signature is ever overwritten and
///   there is no double-sign race.
/// - This is NOT laundering-proof: under the operator's explicit
///   `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` window (the caller's gate) the current head
///   state IS the trust root — the same blindness the window already carries. The
///   value is that converging FAST lets the operator CLOSE the window sooner, which
///   MINIMISES total tamper-tolerant exposure vs. leaving it open for weeks of organic
///   per-partition re-signing.
async fn resign_unsigned_head(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    seed: &[u8; 32],
    signing_key_id: u64,
    now: i64,
) -> Result<bool, String> {
    let Some(cp) = read_checkpoint(d1, tenant_id, region).await? else {
        return Ok(false); // no head yet (genesis) — nothing to re-sign
    };
    if cp.head_signature.is_some() {
        return Ok(false); // already signed (any key) — never overwrite / launder
    }
    let signing_key_id_i =
        i64::try_from(signing_key_id).map_err(|_| "signing_key_id exceeds i64")?;
    let seq_i = i64::try_from(cp.next_sequence).map_err(|_| "next_sequence exceeds i64")?;
    let sig = sign_head(
        seed,
        signing_key_id,
        tenant_id,
        region,
        &cp.head_hex,
        cp.next_sequence,
    )?;
    // Atomic NULL→signed: the `AND head_signature IS NULL` guard means a row that a
    // concurrent write signed (or advanced) first is left untouched — no overwrite.
    let rows = d1
        .query(
            "UPDATE audit_chain_head \
             SET head_signature = ?1, head_signed_at_ms = ?2, signing_key_id = ?3 \
             WHERE tenant_id = ?4 AND region = ?5 \
               AND head_hash = ?6 AND next_sequence = ?7 \
               AND head_signature IS NULL \
             RETURNING tenant_id",
            &[
                json!(sig),
                json!(now),
                json!(signing_key_id_i),
                json!(tenant_id),
                json!(region),
                json!(cp.head_hex),
                json!(seq_i),
            ],
        )
        .await?;
    Ok(!rows.is_empty())
}
