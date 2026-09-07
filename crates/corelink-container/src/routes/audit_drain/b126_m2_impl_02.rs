/// Verify the stored head signature of the checkpoint we resume from (CF-6).
///
/// A checkpoint carrying a non-NULL signature signed under the CURRENT key id MUST
/// verify against the canonical head tuple + the seed-derived public key. A NULL
/// signature (legacy), a signature under a different key id (rotation), or no
/// checkpoint at all are tolerated ([`HeadResumeCheck::Proceed`]); the head is
/// (re-)signed on advance. A signature that fails to verify — or a signed head
/// with no seed available to verify it — is [`HeadResumeCheck::FailClosed`].
fn check_head_on_resume(
    checkpoint: Option<&HeadCheckpoint>,
    signing_seed: Option<&[u8; 32]>,
    current_key_id: u64,
    tenant_id: &str,
    region: &str,
    trust_unsigned_resume: bool,
) -> HeadResumeCheck {
    let Some(cp) = checkpoint else {
        // Genesis — no prior head to verify.
        return HeadResumeCheck::Proceed;
    };
    let Some(seed) = signing_seed else {
        // No signing regime configured (dev/CI). Nothing to verify against: a
        // never-signed (NULL) head is tolerated; a head that CLAIMS a signature we
        // cannot check is still fail-CLOSED (can't prove integrity).
        return if cp.head_signature.is_none() {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    };
    // Signing regime ACTIVE. Every legit advance signs the head under the current
    // key, so a NULL signature or a foreign `signing_key_id` is UNVERIFIABLE. An
    // insider with D1 write (but no seed) strips/rotates the signature to launder a
    // chain rewrite — so by default that is TAMPER (fail-CLOSED). A genuine
    // legacy/rotation migration is an EXPLICIT, logged operator opt-in
    // (`trust_unsigned_resume`), never a silent tolerance.
    let Some(sig) = cp.head_signature.as_deref() else {
        return if trust_unsigned_resume {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    };
    if cp.signing_key_id != Some(current_key_id) {
        return if trust_unsigned_resume {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    }
    // Signed under the current key id ⇒ MUST verify. A failure here is an active
    // forgery (head rewritten while keeping a current-key signature) and is ALWAYS
    // tamper — never softened by the migration escape.
    if verify_head(
        seed,
        current_key_id,
        tenant_id,
        region,
        &cp.head_hex,
        cp.next_sequence,
        sig,
    ) {
        HeadResumeCheck::Verified
    } else {
        HeadResumeCheck::FailClosed
    }
}

/// Do not let the still-legacy D1 drain reinterpret a B-054 v2 checkpoint.
/// Until the external witness/ledger transaction is wired, a v2 (or partially
/// populated) head is an explicit fail-closed stop, never a downgrade to the
/// unkeyed formula or an automatic E0 bootstrap.
fn reject_unwired_epoch_checkpoint(checkpoint: Option<&HeadCheckpoint>) -> Result<(), String> {
    let Some(cp) = checkpoint else {
        return Ok(());
    };
    let fields = [
        cp.epoch_id.is_some(),
        cp.head_message_version.is_some(),
        cp.epoch_ledger_sequence.is_some(),
        cp.epoch_ledger_hash.is_some(),
        cp.head_witness_sequence.is_some(),
        cp.head_witness_hash.is_some(),
    ];
    let any = fields.iter().copied().any(|present| present);
    let all = fields.iter().copied().all(|present| present);
    if any {
        return Err(if all && cp.head_message_version == Some(2) {
            "B-054 v2 epoch checkpoint requires the witnessed epoch runtime; refusing legacy drain downgrade"
                .to_owned()
        } else {
            "B-054 epoch checkpoint metadata is incomplete; refusing legacy drain downgrade"
                .to_owned()
        });
    }
    Ok(())
}

/// Read the `(tenant_id, region)` partitions with pending (unsealed) rows.
async fn read_pending_partitions(d1: &D1HttpClient) -> Result<Vec<(String, String)>, String> {
    let rows = d1
        .query(
            "SELECT DISTINCT tenant_id, region FROM audit_outbox WHERE emitted_at IS NULL",
            &[],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let t = r.get("tenant_id").and_then(Value::as_str)?.to_owned();
            let region = r.get("region").and_then(Value::as_str)?.to_owned();
            Some((t, region))
        })
        .collect())
}

/// Read up to `limit` `(tenant_id, region)` partitions whose `audit_chain_head` is
/// genuinely UNSIGNED (`head_signature IS NULL`, legacy pre-0080 / seed-was-broken).
///
/// DELIBERATELY NULL-only — NOT foreign-key (`signing_key_id != current`). Re-signing
/// a foreign-key head without verifying its old signature is a broader trust action
/// (and an attacker with D1 write could set a non-current key id to force a re-sign),
/// so key-rotation convergence is out of scope here; this sweep only heals the exact
/// legacy-NULL case the CF-6-seed-bug produced. `LIMIT` bounds the per-call work so a
/// large backlog cannot make one drain call exceed the edge subrequest timeout.
async fn read_unsigned_head_partitions(
    d1: &D1HttpClient,
    limit: i64,
) -> Result<Vec<(String, String)>, String> {
    let rows = d1
        .query(
            "SELECT tenant_id, region FROM audit_chain_head \
             WHERE head_signature IS NULL LIMIT ?1",
            &[json!(limit)],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let t = r.get("tenant_id").and_then(Value::as_str)?.to_owned();
            let region = r.get("region").and_then(Value::as_str)?.to_owned();
            Some((t, region))
        })
        .collect())
}

/// Read the `audit_chain_head` checkpoint for a partition.
async fn read_checkpoint(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<HeadCheckpoint>, String> {
    let rows = d1
        .query(
            "SELECT head_hash, next_sequence, head_signature, signing_key_id, \
                    epoch_id, head_message_version, epoch_ledger_sequence, \
                    epoch_ledger_hash, head_witness_sequence, head_witness_hash \
             FROM audit_chain_head \
             WHERE tenant_id = ?1 AND region = ?2",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    // D1 HTTP rows are object maps; the nullable checkpoint helpers also serve
    // the JSON-value test seam, so normalize this query row at the boundary.
    let row = Value::Object(row);
    let head_hex = row
        .get("head_hash")
        .and_then(Value::as_str)
        .ok_or("audit_chain_head.head_hash missing/non-text")?
        .to_owned();
    let head = chain_hash_from_hex(&head_hex)
        .ok_or("audit_chain_head.head_hash is not a 64-hex BLAKE3 digest")?;
    let next_sequence = row
        .get("next_sequence")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_chain_head.next_sequence missing/negative/non-integer")?;
    // CF-6: nullable signature columns (0080). A NULL is a legacy head.
    let head_signature = checkpoint_nullable_text(&row, "head_signature")?;
    let signing_key_id = checkpoint_nullable_u64(&row, "signing_key_id")?;
    let epoch_id = checkpoint_nullable_u64(&row, "epoch_id")?;
    let head_message_version = checkpoint_nullable_u64(&row, "head_message_version")?
        .map(|value| {
            u8::try_from(value)
                .map_err(|_| "audit_chain_head.head_message_version exceeds u8".to_owned())
        })
        .transpose()?;
    let epoch_ledger_sequence = checkpoint_nullable_u64(&row, "epoch_ledger_sequence")?;
    let epoch_ledger_hash = checkpoint_nullable_text(&row, "epoch_ledger_hash")?;
    let head_witness_sequence = checkpoint_nullable_u64(&row, "head_witness_sequence")?;
    let head_witness_hash = checkpoint_nullable_text(&row, "head_witness_hash")?;
    Ok(Some(HeadCheckpoint {
        head,
        head_hex,
        next_sequence,
        head_signature,
        signing_key_id,
        epoch_id,
        head_message_version,
        epoch_ledger_sequence,
        epoch_ledger_hash,
        head_witness_sequence,
        head_witness_hash,
    }))
}

/// Read the durable sealed-rows tail (MAX sequence sealed row) for a partition.
async fn read_sealed_tail(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<(ChainHash, u64)>, String> {
    let rows = d1
        .query(
            "SELECT sequence_number, chain_hash, algorithm_id, epoch_id, link_key_id FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND sequence_number IS NOT NULL \
             ORDER BY sequence_number DESC LIMIT 1",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    // Keep the parser's Value-based malformed-metadata contract while adapting
    // the D1 client's object-map row representation at this boundary.
    let row = Value::Object(row);
    let seq = row
        .get("sequence_number")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_outbox.sequence_number negative/non-integer on a sealed row")?;
    let chain_hex = row
        .get("chain_hash")
        .and_then(Value::as_str)
        .ok_or("audit_outbox.chain_hash missing on a sealed row")?;
    let chain = chain_hash_from_hex(chain_hex)
        .ok_or("audit_outbox.chain_hash is not a 64-hex BLAKE3 digest")?;
    // B-054: the legacy drain may never resume a keyed/epoch-versioned tail.
    // This is a hard stop, not a downgrade to E0; the witnessed epoch runtime
    // must validate its ledger/key before continuing.
    let (algorithm_id, epoch_id, link_key_id) = parse_sealed_epoch_metadata(&row)?;
    if algorithm_id != 0 || epoch_id != 0 || link_key_id.is_some() {
        return Err(
            "B-054 versioned sealed tail requires the witnessed epoch runtime; refusing downgrade"
                .to_owned(),
        );
    }
    Ok(Some((chain, seq)))
}

/// Read up to `limit` pending rows of a partition in deterministic seal order.
///
/// The `LIMIT` seals only the ordered PREFIX of the unsealed tail; the next call
/// resumes from the advanced head (`emitted_at IS NULL` excludes the rows this
/// call sealed, so the same `ORDER BY` picks up exactly where we left off). This
/// keeps the hash chain intact — a prefix of a deterministic order is still a
/// deterministic order — while bounding the work per call.
async fn read_pending_rows(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    limit: i64,
) -> Result<Vec<(String, Value)>, String> {
    let rows = d1
        .query(
            "SELECT id, payload_json FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 AND emitted_at IS NULL \
             ORDER BY enqueued_at, id \
             LIMIT ?3",
            &[json!(tenant_id), json!(region), json!(limit)],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.id missing/non-text")?
            .to_owned();
        let payload_str = row
            .get("payload_json")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.payload_json missing/non-text")?;
        let payload: Value = serde_json::from_str(payload_str)
            .map_err(|e| format!("audit_outbox.payload_json row {id} is not valid JSON: {e}"))?;
        out.push((id, payload));
    }
    Ok(out)
}

/// Write the sealed columns for one row, guarded by `emitted_at IS NULL` so a
/// re-run (or a concurrent drain) never double-seals.
async fn write_seal(d1: &D1HttpClient, row: &SealedRow, now: i64) -> Result<(), String> {
    let seq = i64::try_from(row.sequence_number).map_err(|_| "sequence_number exceeds i64")?;
    let epoch_id = i64::try_from(row.epoch_id).map_err(|_| "epoch_id exceeds i64")?;
    let link_key_id = row
        .link_key_id
        .map(i64::try_from)
        .transpose()
        .map_err(|_| "link_key_id exceeds i64")?;
    d1.query(
        "UPDATE audit_outbox \
         SET sequence_number = ?1, prev_hash = ?2, chain_hash = ?3, \
             canonical_jcs = ?4, chained_at = ?5, emitted_at = ?5, \
             algorithm_id = ?6, epoch_id = ?7, link_key_id = ?8 \
         WHERE id = ?9 AND emitted_at IS NULL",
        &[
            json!(seq),
            json!(row.prev_hash_hex),
            json!(row.chain_hash_hex),
            json!(row.canonical_jcs),
            json!(now),
            json!(i64::from(row.algorithm_id)),
            json!(epoch_id),
            json!(link_key_id),
            json!(row.id),
        ],
    )
    .await?;
    Ok(())
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
            d1.query(
                "UPDATE audit_chain_head \
                 SET head_hash = ?1, next_sequence = ?2, updated_at = ?3, \
                     head_signature = ?8, head_signed_at_ms = ?9, signing_key_id = ?10 \
                 WHERE tenant_id = ?4 AND region = ?5 \
                   AND head_hash = ?6 AND next_sequence = ?7 \
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
