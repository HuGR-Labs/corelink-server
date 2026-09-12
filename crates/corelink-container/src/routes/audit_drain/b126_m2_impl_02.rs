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

fn resolution_u64(row: &Value, field: &str) -> Result<u64, String> {
    row.get(field)
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| format!("legacy tail resolution {field} missing/non-negative-integer"))
}

fn resolution_text(row: &Value, field: &str) -> Result<String, String> {
    row.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("legacy tail resolution {field} missing/non-text"))
}

fn parse_legacy_tail_resolution(
    row: crate::storage::d1_http::D1Row,
) -> Result<LegacyTailResolution, String> {
    let row = Value::Object(row);
    Ok(LegacyTailResolution {
        candidate_count: resolution_u64(&row, "candidate_count")?,
        candidate_set_hash: resolution_text(&row, "candidate_set_hash")?,
        checkpoint_head_hash: resolution_text(&row, "checkpoint_head_hash")?,
        checkpoint_head_signature: resolution_text(&row, "checkpoint_head_signature")?,
        checkpoint_next_sequence: resolution_u64(&row, "checkpoint_next_sequence")?,
        created_at_ms: resolution_u64(&row, "created_at_ms")?,
        resolution_signature: resolution_text(&row, "resolution_signature")?,
        resolution_version: u8::try_from(resolution_u64(&row, "resolution_version")?)
            .map_err(|_| "legacy tail resolution version exceeds u8".to_owned())?,
        selected_chain_hash: resolution_text(&row, "selected_chain_hash")?,
        selected_row_id: resolution_text(&row, "selected_row_id")?,
        signing_key_id: resolution_u64(&row, "signing_key_id")?,
        tail_sequence: resolution_u64(&row, "tail_sequence")?,
    })
}

async fn read_legacy_tail_resolution(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<LegacyTailResolution>, String> {
    let mut rows = d1
        .query(
            "SELECT resolution_version, tail_sequence, selected_row_id, selected_chain_hash, \
                    checkpoint_head_hash, checkpoint_next_sequence, checkpoint_head_signature, \
                    signing_key_id, candidate_count, candidate_set_hash, resolution_signature, \
                    created_at_ms \
               FROM audit_chain_legacy_tail_resolution \
              WHERE tenant_id = ?1 AND region = ?2",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    if rows.len() > 1 {
        return Err("multiple legacy tail resolutions for one partition".to_owned());
    }
    rows.pop().map(parse_legacy_tail_resolution).transpose()
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

/// Load the sole active epoch projection. Its ledger pointer is trusted only
/// after [`verify_v2_checkpoint_witness`] proves the signed/witnessed head binds
/// the same values.
async fn read_active_epoch(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<ActiveEpoch, String> {
    let rows = d1
        .query(
            "SELECT e.epoch_id, e.algorithm_id, e.link_key_id, e.start_sequence, \
                    e.start_prev_hash, e.predecessor_epoch_id, e.open_ledger_sequence, \
                    e.open_ledger_hash, k.key_commitment_hex, l.entry_jcs, \
                    l.signature_b64 AS ledger_signature_b64, \
                    l.signing_key_id AS ledger_signing_key_id, l.ledger_hash AS stored_ledger_hash \
             FROM audit_chain_epoch e \
             LEFT JOIN audit_chain_link_key_registry k ON k.link_key_id = e.link_key_id \
             JOIN audit_chain_epoch_ledger l ON l.tenant_id=e.tenant_id AND l.region=e.region \
                  AND l.ledger_sequence=e.open_ledger_sequence \
             WHERE e.tenant_id = ?1 AND e.region = ?2 AND e.state = 'active' LIMIT 2",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    if rows.len() != 1 {
        return Err("v2 partition must have exactly one active epoch".to_owned());
    }
    let row = Value::Object(rows.into_iter().next().ok_or("active epoch disappeared")?);
    let unsigned = |field: &str| -> Result<u64, String> {
        row.get(field)
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| format!("active epoch {field} missing/invalid"))
    };
    let epoch_id = unsigned("epoch_id")?;
    let algorithm_id = unsigned("algorithm_id")?;
    let start_sequence = unsigned("start_sequence")?;
    let start_prev_hash_text = row
        .get("start_prev_hash")
        .and_then(Value::as_str)
        .ok_or("active epoch start_prev_hash missing")?;
    let start_prev_hash = chain_hash_from_hex(start_prev_hash_text)
        .ok_or("active epoch start_prev_hash malformed")?;
    let ledger_sequence = unsigned("open_ledger_sequence")?;
    let ledger_hash = row
        .get("open_ledger_hash")
        .and_then(Value::as_str)
        .filter(|hash| chain_hash_from_hex(hash).is_some())
        .ok_or("active epoch open_ledger_hash malformed")?
        .to_owned();
    if row.get("stored_ledger_hash").and_then(Value::as_str) != Some(ledger_hash.as_str()) {
        return Err("active epoch ledger index/hash mismatch".to_owned());
    }
    let ledger_jcs = decode_d1_blob(
        row.get("entry_jcs")
            .ok_or("active epoch entry_jcs missing")?,
        "active epoch entry_jcs",
        16 * 1024,
    )?;
    let ledger_signature_b64 = row
        .get("ledger_signature_b64")
        .and_then(Value::as_str)
        .ok_or("active epoch ledger signature missing/non-text")?
        .to_owned();
    let ledger_signing_key_id = unsigned("ledger_signing_key_id")?;
    let link_key_id = checkpoint_nullable_u64(&row, "link_key_id")?;
    let predecessor = checkpoint_nullable_u64(&row, "predecessor_epoch_id")?;
    let (epoch, key_commitment) = match (epoch_id, algorithm_id, link_key_id, predecessor) {
        (0, 0, None, None) if start_sequence == 0 && start_prev_hash == ChainHash::genesis() => {
            (ChainEpoch::legacy(), None)
        }
        (id, 1, Some(key_id), Some(predecessor)) if id > 0 => {
            let epoch = ChainEpoch::keyed_successor(
                id,
                key_id,
                start_sequence,
                start_prev_hash,
                predecessor,
            )
            .map_err(|error| format!("active keyed epoch invalid: {error}"))?;
            let commitment = row
                .get("key_commitment_hex")
                .and_then(Value::as_str)
                .and_then(chain_hash_from_hex)
                .ok_or("active keyed epoch lacks a valid registered key commitment")?;
            (epoch, Some(commitment))
        }
        _ => return Err("active epoch projection has invalid algorithm/identity".to_owned()),
    };
    Ok(ActiveEpoch {
        epoch,
        ledger_sequence,
        ledger_hash,
        key_commitment,
        ledger_jcs,
        ledger_signature_b64,
        ledger_signing_key_id,
    })
}

/// A v2 partition's sealed tail must have one row at its maximum sequence.
///
/// V2 commits rows and their signed checkpoint atomically. More than one row at
/// the maximum sequence therefore represents a fork, and must never be resolved
/// through the exceptional legacy signed-resolution path below.
fn reject_duplicate_sealed_tail(rows: &[crate::storage::d1_http::D1Row]) -> Result<(), String> {
    if rows.len() < 2 {
        return Ok(());
    }
    let first = rows
        .first()
        .and_then(|row| row.get("sequence_number").and_then(Value::as_i64))
        .ok_or("audit_outbox.sequence_number negative/non-integer on sealed tail")?;
    let second = rows
        .get(1)
        .and_then(|row| row.get("sequence_number").and_then(Value::as_i64))
        .ok_or("audit_outbox.sequence_number negative/non-integer on sealed tail")?;
    if first == second {
        return Err(format!(
            "audit_outbox sealed tail has duplicate sequence_number {first}; refusing ambiguous resume"
        ));
    }
    Ok(())
}

/// Resolve a durable sealed tail without laundering a historical fork.
///
/// A duplicated maximum sequence is normally ambiguous and therefore a hard
/// stop. There is one narrowly safe legacy recovery: the already-verified
/// Ed25519 checkpoint names exactly one unquarantined candidate, its
/// `next_sequence` is exactly one past the duplicated sequence, every competing
/// candidate is quarantined, and the query proves the complete bounded duplicate
/// population (at most 32 rows). In that case no branch is invented or
/// selected by row order: the pre-existing signed checkpoint is the
/// cryptographic branch decision.
///
/// If any predicate is absent (the production `bba0ff1d` residue has no tail
/// matching its signed, ahead-of-tail checkpoint), recovery requires a new
/// independently witnessed epoch. The legacy drain must not synthesize one;
/// B-054 deliberately keeps that writer gated until witness wiring exists.
#[derive(Serialize)]
struct LegacyTailCandidateCommitment<'a> {
    archived_at: Option<i64>,
    canonical_jcs_hash: String,
    chain_hash: &'a str,
    id: &'a str,
    prev_hash: &'a str,
    quarantined_at: Option<i64>,
    quarantine_reason: Option<&'a str>,
    sequence_number: u64,
}

fn legacy_tail_candidate_set_hash(
    rows: &[crate::storage::d1_http::D1Row],
) -> Result<String, String> {
    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.id missing/non-text on ambiguous sealed tail")?;
        let sequence_number = row
            .get("sequence_number")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or("audit_outbox.sequence_number negative/non-integer on sealed tail")?;
        let prev_hash = row
            .get("prev_hash")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.prev_hash missing/non-text on ambiguous sealed tail")?;
        let chain_hash = row
            .get("chain_hash")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.chain_hash missing/non-text on ambiguous sealed tail")?;
        let canonical_jcs = row
            .get("canonical_jcs")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.canonical_jcs missing/non-text on ambiguous sealed tail")?;
        let quarantined_at = match row.get("quarantined_at") {
            Some(Value::Null) => None,
            Some(Value::Number(value)) => Some(
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or("audit_outbox.quarantined_at negative/non-integer")?,
            ),
            _ => return Err("audit_outbox.quarantined_at missing/malformed".to_owned()),
        };
        let quarantine_reason = match row.get("quarantine_reason") {
            Some(Value::Null) => None,
            Some(Value::String(value)) if !value.trim().is_empty() => Some(value.as_str()),
            _ => return Err("audit_outbox.quarantine_reason missing/malformed".to_owned()),
        };
        let archived_at = match row.get("archived_at") {
            Some(Value::Null) => None,
            Some(Value::Number(value)) => Some(
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or("audit_outbox.archived_at negative/non-integer")?,
            ),
            _ => return Err("audit_outbox.archived_at missing/malformed".to_owned()),
        };
        candidates.push(LegacyTailCandidateCommitment {
            archived_at,
            canonical_jcs_hash: blake3::hash(canonical_jcs.as_bytes()).to_hex().to_string(),
            chain_hash,
            id,
            prev_hash,
            quarantined_at,
            quarantine_reason,
            sequence_number,
        });
    }
    candidates.sort_by(|left, right| left.id.cmp(right.id));
    let canonical = serde_jcs::to_vec(&candidates)
        .map_err(|error| format!("JCS canonicalize legacy tail candidate set: {error}"))?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

struct LegacyTailResumeContext<'a> {
    checkpoint: Option<&'a HeadCheckpoint>,
    head_check: HeadResumeCheck,
    signing_seed: Option<&'a [u8; 32]>,
    signing_key_id: u64,
    trust_unsigned_resume: bool,
    tenant_id: &'a str,
    region: &'a str,
}

fn resolve_sealed_tail_rows(
    rows: &[crate::storage::d1_http::D1Row],
    resolution: Option<&LegacyTailResolution>,
    context: &LegacyTailResumeContext<'_>,
) -> Result<Option<(ChainHash, u64)>, String> {
    let Some(first_row) = rows.first() else {
        return Ok(None);
    };
    let first_sequence = first_row
        .get("sequence_number")
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or("audit_outbox.sequence_number negative/non-integer on sealed tail")?;
    let first_hash = first_row
        .get("chain_hash")
        .and_then(Value::as_str)
        .and_then(chain_hash_from_hex)
        .ok_or("audit_outbox.chain_hash is not a 64-hex BLAKE3 digest")?;
    let duplicate_count = first_row
        .get("sequence_count")
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or("audit_outbox sealed-tail sequence_count missing/non-integer")?;

    if duplicate_count == 1 {
        return Ok(Some((first_hash, first_sequence)));
    }
    if !(2..=32).contains(&duplicate_count) || rows.len() != duplicate_count {
        return Err(format!(
            "audit_outbox sealed tail sequence {first_sequence} has {duplicate_count} candidates; independently witnessed epoch recovery required"
        ));
    }
    if rows.iter().any(|row| {
        row.get("sequence_number")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            != Some(first_sequence)
    }) {
        return Err(
            "audit_outbox sealed-tail population count disagrees with returned candidates"
                .to_owned(),
        );
    }

    let Some(checkpoint) = context.checkpoint else {
        return Err(format!(
            "audit_outbox sealed tail has duplicate sequence_number {first_sequence}; independently witnessed epoch recovery required"
        ));
    };
    let Some(resolution) = resolution else {
        return Err(format!(
            "audit_outbox sealed tail has duplicate sequence_number {first_sequence} without an explicit signed resolution"
        ));
    };
    let Some(signing_seed) = context.signing_seed else {
        return Err(
            "legacy tail resolution cannot be verified without the signing seed".to_owned(),
        );
    };
    if context.trust_unsigned_resume
        || context.head_check != HeadResumeCheck::Verified
        || checkpoint.next_sequence != first_sequence.saturating_add(1)
    {
        return Err(format!(
            "audit_outbox sealed tail has duplicate sequence_number {first_sequence} without an exact verified checkpoint; independently witnessed epoch recovery required"
        ));
    }
    if first_sequence != 0 {
        return Err(format!(
            "ambiguous legacy tail sequence {first_sequence} lacks a complete genesis/independently-witnessed prefix proof"
        ));
    }

    let checkpoint_is_strict_legacy = checkpoint.epoch_id.is_none()
        && checkpoint.head_message_version.is_none()
        && checkpoint.epoch_ledger_sequence.is_none()
        && checkpoint.epoch_ledger_hash.is_none()
        && checkpoint.head_witness_sequence.is_none()
        && checkpoint.head_witness_hash.is_none();
    let checkpoint_signature = checkpoint
        .head_signature
        .as_deref()
        .ok_or("verified checkpoint unexpectedly lacks its signature")?;
    if !checkpoint_is_strict_legacy
        || resolution.resolution_version != 1
        || resolution.tail_sequence != first_sequence
        || resolution.checkpoint_next_sequence != checkpoint.next_sequence
        || resolution.checkpoint_head_hash != checkpoint.head_hex
        || resolution.checkpoint_head_signature != checkpoint_signature
        || resolution.signing_key_id != context.signing_key_id
        || checkpoint.signing_key_id != Some(context.signing_key_id)
        || resolution.candidate_count != u64::try_from(duplicate_count).unwrap_or(u64::MAX)
        || resolution.selected_chain_hash != resolution.checkpoint_head_hash
    {
        return Err("legacy tail resolution does not match the exact live checkpoint".to_owned());
    }
    let candidate_set_hash = legacy_tail_candidate_set_hash(rows)?;
    if resolution.candidate_set_hash != candidate_set_hash
        || !verify_legacy_tail_resolution(
            resolution,
            context.tenant_id,
            context.region,
            signing_seed,
        )
    {
        return Err(
            "legacy tail resolution candidate commitment/signature does not verify".to_owned(),
        );
    }

    let mut signed_candidate = None;
    let mut checkpoint_hash_matches = 0usize;
    for row in rows {
        let metadata = parse_sealed_epoch_metadata(&Value::Object(row.clone()))?;
        if metadata != (0, 0, None) {
            return Err(
                "ambiguous sealed-tail recovery is restricted to legacy epoch rows".to_owned(),
            );
        }
        let hash = row
            .get("chain_hash")
            .and_then(Value::as_str)
            .and_then(chain_hash_from_hex)
            .ok_or("audit_outbox.chain_hash is not a 64-hex BLAKE3 digest")?;
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.id missing/non-text on ambiguous sealed tail")?;
        let prev_hash = row
            .get("prev_hash")
            .and_then(Value::as_str)
            .and_then(chain_hash_from_hex)
            .ok_or("audit_outbox.prev_hash is not a 64-hex BLAKE3 digest")?;
        let canonical_jcs = row
            .get("canonical_jcs")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.canonical_jcs missing/non-text")?;
        let recomputed = link_for_epoch(
            &ChainEpoch::legacy(),
            &prev_hash,
            canonical_jcs.as_bytes(),
            None,
        )
        .map_err(|error| format!("re-hash ambiguous legacy tail: {error}"))?;
        if recomputed != hash {
            return Err("ambiguous sealed-tail candidate hash does not verify".to_owned());
        }
        let quarantined = match row.get("quarantined_at") {
            Some(Value::Null) => false,
            Some(Value::Number(value)) if value.as_i64().is_some_and(|v| v >= 0) => true,
            _ => {
                return Err(
                    "audit_outbox.quarantined_at malformed on ambiguous sealed tail".to_owned(),
                )
            }
        };
        let quarantine_reason = row.get("quarantine_reason").and_then(Value::as_str);
        let archived = row
            .get("archived_at")
            .and_then(Value::as_i64)
            .is_some_and(|value| value >= 0);
        if hash == checkpoint.head {
            checkpoint_hash_matches = checkpoint_hash_matches.saturating_add(1);
        }
        if hash == checkpoint.head
            && id == resolution.selected_row_id
            && hash.to_hex() == resolution.selected_chain_hash
        {
            if quarantined
                || quarantine_reason.is_some()
                || !archived
                || prev_hash != ChainHash::genesis()
                || signed_candidate.replace(hash).is_some()
            {
                return Err(format!(
                    "audit_outbox sealed tail sequence {first_sequence} lacks one unique unquarantined signed branch"
                ));
            }
        } else if archived
            || !quarantined
            || quarantine_reason.map_or(true, |reason| reason.trim().is_empty())
        {
            return Err(format!(
                "audit_outbox sealed tail sequence {first_sequence} has a losing branch that is not exclusively quarantined"
            ));
        }
    }
    if checkpoint_hash_matches != 1 {
        return Err(format!(
            "audit_outbox sealed tail sequence {first_sequence} has {checkpoint_hash_matches} candidates matching the signed checkpoint; exactly one is required"
        ));
    }
    signed_candidate
        .map(|hash| Some((hash, first_sequence)))
        .ok_or_else(|| {
            format!(
                "audit_outbox sealed tail sequence {first_sequence} has no branch selected by the signed checkpoint; independently witnessed epoch recovery required"
            )
        })
}

/// Read the durable sealed-rows tail (MAX sequence sealed row) for a partition.
async fn read_sealed_tail(
    d1: &D1HttpClient,
    context: &LegacyTailResumeContext<'_>,
) -> Result<Option<(ChainHash, u64)>, String> {
    let rows = d1
        .query(
            "SELECT id, sequence_number, prev_hash, chain_hash, canonical_jcs, archived_at, \
                    quarantined_at, quarantine_reason, \
                    algorithm_id, epoch_id, link_key_id, \
                    (SELECT COUNT(*) FROM audit_outbox AS sibling \
                      WHERE sibling.tenant_id = audit_outbox.tenant_id \
                        AND sibling.region = audit_outbox.region \
                        AND sibling.emitted_at IS NOT NULL \
                        AND sibling.sequence_number = audit_outbox.sequence_number) AS sequence_count \
               FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND sequence_number IS NOT NULL \
               AND sequence_number = (SELECT MAX(tail.sequence_number) FROM audit_outbox AS tail \
                    WHERE tail.tenant_id = ?1 AND tail.region = ?2 \
                      AND tail.emitted_at IS NOT NULL AND tail.sequence_number IS NOT NULL) \
             ORDER BY id LIMIT 33",
            &[json!(context.tenant_id), json!(context.region)],
        )
        .await?;
    let resolution = if rows
        .first()
        .and_then(|row| row.get("sequence_count"))
        .and_then(Value::as_i64)
        .is_some_and(|count| count > 1)
    {
        read_legacy_tail_resolution(d1, context.tenant_id, context.region).await?
    } else {
        None
    };
    let resolved = resolve_sealed_tail_rows(&rows, resolution.as_ref(), context)?;
    let Some((chain, seq)) = resolved else {
        return Ok(None);
    };
    let row = rows
        .iter()
        .find(|row| {
            row.get("chain_hash")
                .and_then(Value::as_str)
                .and_then(chain_hash_from_hex)
                == Some(chain)
        })
        .ok_or("resolved sealed tail is missing its selected metadata row")?;
    // Keep the parser's Value-based malformed-metadata contract while adapting
    // the D1 client's object-map row representation at this boundary.
    let row = Value::Object(row.clone());
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

/// V2 transactions update rows and head atomically, so the durable tail must
/// exactly equal the signed checkpoint. Metadata is verified separately from
/// the authenticated active epoch and each newly sealed row.
async fn read_any_sealed_tail(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<(ChainHash, u64)>, String> {
    let rows = d1.query(
        "SELECT sequence_number, chain_hash FROM audit_outbox \
         WHERE tenant_id=?1 AND region=?2 AND emitted_at IS NOT NULL AND sequence_number IS NOT NULL \
         ORDER BY sequence_number DESC LIMIT 2",
        &[json!(tenant_id), json!(region)],
    ).await?;
    reject_duplicate_sealed_tail(&rows)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let sequence = row
        .get("sequence_number")
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or("v2 sealed tail sequence malformed")?;
    let head = row
        .get("chain_hash")
        .and_then(Value::as_str)
        .and_then(chain_hash_from_hex)
        .ok_or("v2 sealed tail hash malformed")?;
    Ok(Some((head, sequence)))
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

/// Maximum number of sealed rows in one JSON1 UPDATE statement.  This keeps
/// each D1 value bounded while removing the one-HTTP-request-per-row drain
/// bottleneck.  The global `AUDIT_DRAIN_BATCH_LIMIT` still bounds the total
/// rows handled by one request.
const AUDIT_SEAL_ROWS_PER_STATEMENT: usize = 32;

/// D1 accepts at most 250 statements in one batch. Four statements are
/// reserved for head CAS, receipt, assertion and assertion cleanup.
const AUDIT_V2_MAX_BATCH_STATEMENTS: usize = 250;
const AUDIT_V2_MAX_ROWS_PER_TRANSACTION: usize =
    (AUDIT_V2_MAX_BATCH_STATEMENTS - 4) * AUDIT_SEAL_ROWS_PER_STATEMENT;

/// The row array occurs twice in the serialized batch request (seal chunks plus
/// assertion). Bound the *outer JSON-escaped representation*, not the inner
/// string: quote/backslash-heavy canonical evidence can nearly double during
/// request serialization. Two copies below this ceiling leave explicit room
/// beneath 2,000,000 bytes for SQL, receipts and envelope overhead.
const AUDIT_V2_MAX_SERIALIZED_ROW_BYTES: usize = 750 * 1024;

/// One atomic UPDATE for a bounded JSON1 row set.  The count predicate is
/// intentional: if any expected row was already sealed/raced, *none* of this
/// chunk is updated and the caller fails closed before advancing the head.
const AUDIT_SEAL_CHUNK_SQL: &str = "UPDATE audit_outbox \
    SET sequence_number = (SELECT json_extract(value, '$.sequence_number') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        prev_hash = (SELECT json_extract(value, '$.prev_hash') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        chain_hash = (SELECT json_extract(value, '$.chain_hash') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        canonical_jcs = (SELECT json_extract(value, '$.canonical_jcs') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        chained_at = (SELECT json_extract(value, '$.sealed_at') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        emitted_at = (SELECT json_extract(value, '$.sealed_at') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        algorithm_id = (SELECT json_extract(value, '$.algorithm_id') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        epoch_id = (SELECT json_extract(value, '$.epoch_id') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id), \
        link_key_id = (SELECT json_extract(value, '$.link_key_id') FROM json_each(?1) WHERE json_extract(value, '$.id') = audit_outbox.id) \
    WHERE id IN (SELECT json_extract(value, '$.id') FROM json_each(?1)) \
      AND emitted_at IS NULL \
      AND (SELECT COUNT(*) FROM audit_outbox AS pending \
           WHERE pending.id IN (SELECT json_extract(value, '$.id') FROM json_each(?1)) \
             AND pending.emitted_at IS NULL) = json_array_length(?1) \
      AND (SELECT COUNT(*) FROM audit_outbox AS pending \
           WHERE pending.id IN (SELECT json_extract(value, '$.id') FROM json_each(?1)) \
             AND pending.emitted_at IS NULL \
             AND pending.enqueued_at <= (SELECT MIN(CAST(json_extract(value, '$.sealed_at') AS INTEGER)) FROM json_each(?1))) = json_array_length(?1) \
      AND NOT EXISTS (SELECT 1 FROM audit_outbox AS collision, json_each(?1) AS candidate \
           WHERE collision.id <> json_extract(candidate.value, '$.id') \
             AND collision.tenant_id = audit_outbox.tenant_id \
             AND collision.region = audit_outbox.region \
             AND collision.sequence_number = json_extract(candidate.value, '$.sequence_number') \
             AND collision.emitted_at IS NOT NULL) \
    RETURNING id";

/// Build the single JSON parameter consumed by [`AUDIT_SEAL_CHUNK_SQL`].
/// Duplicate IDs are rejected before the statement is sent because the SQL
/// count guard compares the array length, not the number of distinct IDs.
fn seal_chunk_payload(rows: &[SealedRow], now: i64) -> Result<Value, String> {
    if rows.is_empty() {
        return Err("audit seal chunk must not be empty".to_owned());
    }
    let mut ids = std::collections::HashSet::with_capacity(rows.len());
    let mut sequence_numbers = std::collections::HashSet::with_capacity(rows.len());
    let mut payload = Vec::with_capacity(rows.len());
    for row in rows {
        if !ids.insert(&row.id) {
            return Err(format!("audit seal chunk contains duplicate id {}", row.id));
        }
        if !sequence_numbers.insert(row.sequence_number) {
            return Err(format!(
                "audit seal chunk contains duplicate sequence_number {}",
                row.sequence_number
            ));
        }
        let sequence_number =
            i64::try_from(row.sequence_number).map_err(|_| "sequence_number exceeds i64")?;
        let epoch_id = i64::try_from(row.epoch_id).map_err(|_| "epoch_id exceeds i64")?;
        let link_key_id = row
            .link_key_id
            .map(i64::try_from)
            .transpose()
            .map_err(|_| "link_key_id exceeds i64")?;
        payload.push(json!({
            "id": row.id,
            "sequence_number": sequence_number,
            "prev_hash": row.prev_hash_hex,
            "chain_hash": row.chain_hash_hex,
            "canonical_jcs": row.canonical_jcs,
            "sealed_at": now,
            "algorithm_id": i64::from(row.algorithm_id),
            "epoch_id": epoch_id,
            "link_key_id": link_key_id,
        }));
    }
    serde_json::to_string(&Value::Array(payload))
        .map(Value::String)
        .map_err(|e| format!("audit seal chunk payload serialize: {e}"))
}

/// Select the largest prefix whose complete transaction assertion parameter
/// is legal for D1. This runs before the external witness linearization point.
/// A single oversized audit row stops closed without moving either system.
fn bounded_v2_sealed_prefix(
    sealed: &[SealedRow],
    now: i64,
) -> Result<(&[SealedRow], ChainHash, u64), String> {
    let upper = sealed.len().min(AUDIT_V2_MAX_ROWS_PER_TRANSACTION);
    if upper == 0 {
        return Err("v2 witnessed transaction cannot select an empty prefix".to_owned());
    }
    let fits = |count: usize| -> Result<bool, String> {
        let prefix = sealed
            .get(..count)
            .ok_or("v2 bounded prefix count exceeds pending rows")?;
        let payload = seal_chunk_payload(prefix, now)?;
        let serialized = serde_json::to_vec(&payload)
            .map_err(|error| format!("serialize outer v2 D1 parameter: {error}"))?;
        Ok(serialized.len() <= AUDIT_V2_MAX_SERIALIZED_ROW_BYTES)
    };
    if !fits(1)? {
        return Err(format!(
            "first pending audit row exceeds safe serialized D1 transaction bound ({AUDIT_V2_MAX_SERIALIZED_ROW_BYTES} bytes)"
        ));
    }
    let (mut low, mut high) = (1, upper);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle)? {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let prefix = sealed
        .get(..low)
        .ok_or("v2 bounded prefix disappeared")?;
    let last = prefix.last().ok_or("bounded v2 prefix disappeared")?;
    let head = chain_hash_from_hex(&last.chain_hash_hex)
        .ok_or("bounded v2 prefix produced a malformed chain hash")?;
    let next_sequence = last
        .sequence_number
        .checked_add(1)
        .ok_or("bounded v2 prefix sequence overflow")?;
    Ok((prefix, head, next_sequence))
}

include!("b126_m2_impl_02_part3.rs");
