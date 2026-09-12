/// Write sealed rows in bounded JSON1 chunks.  The lease fence is checked on
/// both sides of each statement: if the lease expires after a chunk commits,
/// the head is deliberately left unadvanced and the sealed prefix is resumed
/// from its durable tail by the next drain.
async fn run_chunked_fenced_seal_loop<C, W, Fut>(
    sealed: &[SealedRow],
    lease_enabled: bool,
    my_lease_expires_ms: i64,
    mut clock: C,
    mut write_chunk: W,
) -> Result<FencedSeal, String>
where
    C: FnMut() -> i64,
    W: FnMut(Vec<SealedRow>) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let mut written = 0u64;
    for chunk in sealed.chunks(AUDIT_SEAL_ROWS_PER_STATEMENT) {
        if should_fence(clock(), my_lease_expires_ms, lease_enabled) {
            return Ok(FencedSeal::Fenced(written));
        }
        let chunk = chunk.to_vec();
        let chunk_len = chunk.len();
        write_chunk(chunk).await?;
        written = written
            .checked_add(u64::try_from(chunk_len).map_err(|_| "seal chunk length exceeds u64")?)
            .ok_or("sealed row count overflow")?;
        if should_fence(clock(), my_lease_expires_ms, lease_enabled) {
            return Ok(FencedSeal::Fenced(written));
        }
    }
    Ok(FencedSeal::Complete(written))
}

/// Acquire the per-partition drain lease atomically (B-038). One SQLite statement
/// — `INSERT … ON CONFLICT DO UPDATE … WHERE expires_ms < now RETURNING holder` —
/// so there is no read-then-write race: a fresh row INSERTs, an EXPIRED lease is
/// stolen by the guarded UPDATE, and a LIVE lease fails the `WHERE` so the
/// `RETURNING` yields zero rows. Acquired iff a row is returned (mirrors the
/// `advance_head_cas` RETURNING pattern). Only called when the lease flag is ON.
async fn acquire_lease(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    holder: &str,
    now_ms: i64,
    ttl_ms: i64,
) -> Result<bool, String> {
    let expires_ms = now_ms.saturating_add(ttl_ms);
    let rows = d1
        .query(
            "INSERT INTO audit_drain_lease \
                 (tenant_id, region, holder, acquired_ms, expires_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(tenant_id, region) DO UPDATE \
                SET holder = excluded.holder, \
                    acquired_ms = excluded.acquired_ms, \
                    expires_ms = excluded.expires_ms \
                WHERE audit_drain_lease.expires_ms < ?4 \
             RETURNING holder",
            &[
                json!(tenant_id),
                json!(region),
                json!(holder),
                json!(now_ms),
                json!(expires_ms),
            ],
        )
        .await?;
    Ok(!rows.is_empty())
}

/// Release the per-partition drain lease (B-038), holder-scoped so we only ever
/// delete a lease we still hold. Best-effort: a missed release (crash/panic) is
/// harmless — the next drain steals the lease once it expires.
async fn release_lease(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    holder: &str,
) -> Result<(), String> {
    d1.query(
        "DELETE FROM audit_drain_lease \
         WHERE tenant_id = ?1 AND region = ?2 AND holder = ?3",
        &[json!(tenant_id), json!(region), json!(holder)],
    )
    .await?;
    Ok(())
}

/// Parse a 64-char lowercase-hex BLAKE3 digest into a [`ChainHash`]. `None` on
/// any malformed value (wrong length / non-hex).
fn chain_hash_from_hex(s: &str) -> Option<ChainHash> {
    let mut out = [0u8; 32];
    hex::decode_to_slice(s.trim(), &mut out).ok()?;
    Some(ChainHash(out))
}

/// Read a nullable checkpoint integer without allowing SQLite/JSON coercions.
/// `NULL` is the explicit legacy value; a present non-integer or negative value
/// is malformed evidence and must stop the drain before resume decisions.
fn checkpoint_nullable_u64(row: &Value, field: &str) -> Result<Option<u64>, String> {
    match row.get(field) {
        Some(Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_i64()
                .ok_or_else(|| format!("audit_chain_head.{field} non-integer"))?;
            u64::try_from(value)
                .map(Some)
                .map_err(|_| format!("audit_chain_head.{field} negative"))
        }
        None => Err(format!("audit_chain_head.{field} missing")),
    }
}

/// Read a nullable checkpoint text value with the same legacy/malformed split.
fn checkpoint_nullable_text(row: &Value, field: &str) -> Result<Option<String>, String> {
    match row.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("audit_chain_head.{field} non-text")),
        None => Err(format!("audit_chain_head.{field} missing")),
    }
}

/// Decode sealed-row epoch metadata without collapsing NULL or negative
/// values into legacy E0. Only all-NULL legacy, explicit E0, and complete
/// positive keyed metadata are valid durable shapes.
fn parse_sealed_epoch_metadata(row: &Value) -> Result<(u8, u64, Option<u64>), String> {
    fn nullable_i64(row: &Value, field: &str) -> Result<Option<i64>, String> {
        match row.get(field) {
            Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| format!("audit_outbox.{field} missing/non-integer")),
            None => Err(format!("audit_outbox.{field} missing")),
        }
    }

    let algorithm = nullable_i64(row, "algorithm_id")?;
    let epoch = nullable_i64(row, "epoch_id")?;
    let key = nullable_i64(row, "link_key_id")?;
    match (algorithm, epoch, key) {
        (None, None, None) | (Some(0), Some(0), None) => Ok((0, 0, None)),
        (Some(1), Some(epoch), Some(key)) if epoch > 0 && key > 0 => {
            Ok((1, epoch as u64, Some(key as u64)))
        }
        _ => Err(
            "audit_outbox sealed epoch metadata is partial, negative, or a downgrade".to_owned(),
        ),
    }
}

/// The persisted per-partition chain checkpoint (`audit_chain_head` row).
#[derive(Clone, Debug)]
struct HeadCheckpoint {
    head: ChainHash,
    /// The EXACT stored hex (used verbatim in the compare-and-set WHERE clause).
    head_hex: String,
    next_sequence: u64,
    /// CF-6: base64 Ed25519 signature over the canonical head tuple (`None` for a
    /// pre-0080 / legacy head — tolerated, re-signed on the next advance).
    head_signature: Option<String>,
    /// CF-6: the key id the stored signature was produced under (lets a
    /// seed/key rotation be told apart from tampering on resume).
    signing_key_id: Option<u64>,
    /// B-054 authenticated epoch metadata.  Legacy NULLs are interpreted only
    /// as E0 by the compatibility path; a partially populated v2 head is never
    /// silently downgraded.
    epoch_id: Option<u64>,
    head_message_version: Option<u8>,
    epoch_ledger_sequence: Option<u64>,
    epoch_ledger_hash: Option<String>,
    head_witness_sequence: Option<u64>,
    head_witness_hash: Option<String>,
}

/// Authenticated-by-the-current-v2-head active epoch projection. The head
/// binds `ledger_sequence/hash`; the runtime separately checks that the D1
/// projection and configured link-key commitment agree before sealing.
#[derive(Clone, Debug)]
struct ActiveEpoch {
    epoch: ChainEpoch,
    ledger_sequence: u64,
    ledger_hash: String,
    key_commitment: Option<ChainHash>,
    ledger_jcs: Vec<u8>,
    ledger_signature_b64: String,
    ledger_signing_key_id: u64,
}

/// One `audit_outbox` row sealed into the BLAKE3 chain. Pure value — the seal is
/// computed in memory, then written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SealedRow {
    id: String,
    sequence_number: u64,
    prev_hash_hex: String,
    chain_hash_hex: String,
    /// The EXACT RFC-8785 JCS bytes that were hashed (what the verifier re-hashes).
    canonical_jcs: String,
    /// Versioned B-054 metadata persisted beside the link.  E0 is explicit in
    /// newly written rows; old rows remain nullable in D1 for compatibility.
    algorithm_id: u8,
    epoch_id: u64,
    link_key_id: Option<u64>,
}

/// Outcome of draining a single partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartitionOutcome {
    /// `n` rows sealed and the head advanced.
    Sealed(u64),
    /// The stored head drifted under us (a concurrent drain advanced it) — the
    /// partition was aborted to avoid forking the chain.
    Drift,
    /// No pending rows (raced away between the partition scan and the read).
    Empty,
    /// Another drain holds this partition's lease (`AUDIT_DRAIN_LEASE_ENABLED`
    /// ON). We did not touch it — neither an error nor drift, just normal
    /// backpressure. Counted into `partitions_leased`.
    Leased,
    /// The seal-loop self-fence tripped: `n` rows were sealed (the pre-expiry
    /// PREFIX) and the loop stopped at the lease expiry, so the head was NOT
    /// advanced. A re-drain resumes from the now-ahead sealed tail and continues
    /// (the existing crash-recovery path). Treated like a truncated batch by the
    /// sweep: `n` counts into `rows_sealed` and forces `incomplete = true`.
    Fenced(u64),
}

/// Pure chain-seal computation. Given the resume `(start_head, start_seq)` and the
/// pending rows in deterministic order, compute the sealed link for each row.
/// Returns `(sealed_rows, new_head, new_next_sequence)`.
///
/// Fully deterministic: identical inputs yield identical outputs — the basis of
/// the concurrent-drain fork-freedom (two drains compute byte-identical seals).
///
/// # Errors
/// Aborts the whole partition (returns `Err`) if any row's payload cannot be
/// JCS-canonicalized (would break sequence contiguity if skipped). Our own sinks
/// only ever write valid JSON, so this is defensive.
fn seal_rows(
    start_head: ChainHash,
    start_seq: u64,
    rows: &[(String, Value)],
) -> Result<(Vec<SealedRow>, ChainHash, u64), String> {
    seal_rows_for_epoch(start_head, start_seq, rows, &ChainEpoch::legacy(), None)
}

/// Version-aware seal computation.  The legacy `seal_rows` wrapper preserves
/// existing behaviour, while keyed callers must supply an authenticated epoch
/// and key; there is no fallback from a keyed request to algorithm 0.
fn seal_rows_for_epoch(
    start_head: ChainHash,
    start_seq: u64,
    rows: &[(String, Value)],
    epoch: &ChainEpoch,
    key: Option<&LinkKey>,
) -> Result<(Vec<SealedRow>, ChainHash, u64), String> {
    let mut prev = start_head;
    let mut seq = start_seq;
    let mut out = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        let jcs =
            serde_jcs::to_vec(payload).map_err(|e| format!("JCS canonicalize row {id}: {e}"))?;
        // Hash using the explicit epoch algorithm.  A missing key, invalid
        // epoch, or algorithm mismatch aborts before any row is written.
        let chain = link_for_epoch(epoch, &prev, &jcs, key)
            .map_err(|e| format!("audit-chain epoch seal row {id}: {e}"))?;
        let canonical_jcs =
            String::from_utf8(jcs).map_err(|e| format!("JCS bytes not UTF-8 for row {id}: {e}"))?;
        out.push(SealedRow {
            id: id.clone(),
            sequence_number: seq,
            prev_hash_hex: prev.to_hex(),
            chain_hash_hex: chain.to_hex(),
            canonical_jcs,
            algorithm_id: epoch.algorithm().id(),
            epoch_id: epoch.epoch_id(),
            link_key_id: epoch.link_key_id(),
        });
        prev = chain;
        seq = seq.saturating_add(1);
    }
    Ok((out, prev, seq))
}

/// Resolve the authoritative resume `(head, next_sequence)` for a partition.
/// Prefers the durable sealed-rows tail when it is AHEAD of the checkpoint (a
/// prior drain crashed after sealing rows but before advancing the head); else
/// the `audit_chain_head` checkpoint; else GENESIS (`None`).
fn resolve_resume(
    checkpoint: Option<(ChainHash, u64)>,
    sealed_tail: Option<(ChainHash, u64)>,
) -> Option<(ChainHash, u64)> {
    // sealed_tail carries (chain_hash, sequence_number) of the MAX sealed row; the
    // next sequence is that + 1.
    let from_tail = sealed_tail.map(|(h, seq)| (h, seq.saturating_add(1)));
    match (checkpoint, from_tail) {
        (Some((_, cp_seq)), Some((th, t_seq))) if t_seq > cp_seq => Some((th, t_seq)),
        (Some(cp), _) => Some(cp),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

/// Outcome of the CF-6 head-signature check on resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadResumeCheck {
    /// Safe to resume + (re-)sign on advance: genesis (no checkpoint); no signing
    /// regime configured and the head was never signed (dev/CI); or an explicit
    /// operator migration window (`trust_unsigned_resume`) is tolerating a NULL /
    /// foreign-key-id head during a legacy-bootstrap or seed-rotation re-sign.
    Proceed,
    /// A signature signed under the CURRENT key id verified OK.
    Verified,
    /// A NON-NULL signature signed under the current key id did NOT verify, OR a
    /// signed head is present but no seed is configured to verify it: TAMPER /
    /// fail-CLOSED — refuse to extend the chain from this head.
    FailClosed,
}

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
