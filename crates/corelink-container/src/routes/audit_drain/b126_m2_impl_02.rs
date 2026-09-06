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

/// CF-6 convergence sweep: stamp the current-key signature onto genuinely UNSIGNED
/// (`head_signature IS NULL`) heads IN PLACE so the operator can turn
/// `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` back OFF WITHOUT waiting weeks for organic
/// per-partition traffic to advance each head — converging fast MINIMISES the total
/// tamper-tolerant exposure window. SELF-GATED on the toggle being ON (the operator's
/// explicit "these legacy heads are trusted") AND a seed being configured; outside the
/// window a legacy NULL head is TAMPER (fail-CLOSED) and is NEVER auto-blessed. Idle
/// heads have no pending rows, so the pending-partition drain loop never visits them —
/// this scans `audit_chain_head` directly, bounded by `batch_limit` per call (a large
/// legacy backlog converges over repeated calls, like the row drain). Returns
/// `(heads_resigned, incomplete)`. Pure D1 orchestration — excluded from mutation
/// testing (no in-CI test can drive live D1; see `.cargo/mutants.toml`).
async fn converge_unsigned_heads(
    d1: &D1HttpClient,
    signing_seed: Option<&[u8; 32]>,
    trust_unsigned_resume: bool,
    signing_key_id: u64,
    batch_limit: i64,
    now: i64,
) -> (u64, bool) {
    let Some(seed) = signing_seed else {
        return (0, false);
    };
    if !trust_unsigned_resume {
        return (0, false);
    }
    let unsigned = match read_unsigned_head_partitions(d1, batch_limit).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "audit/drain: unsigned-head scan failed");
            return (0, false);
        }
    };
    let scanned = unsigned.len() as i64;
    let mut heads_resigned: u64 = 0;
    for (tenant_id, region) in unsigned {
        match resign_unsigned_head(d1, &tenant_id, &region, seed, signing_key_id, now).await {
            Ok(true) => {
                heads_resigned = heads_resigned.saturating_add(1);
                tracing::warn!(
                    region = %region,
                    key_id = signing_key_id,
                    "audit/drain: CF-6 legacy UNSIGNED head re-signed in place \
                     (AUDIT_CHAIN_TRUST_UNSIGNED_RESUME migration)"
                );
            }
            Ok(false) => {}
            Err(e) => tracing::error!(
                error = %e, region = %region,
                "audit/drain: legacy head re-sign failed"
            ),
        }
    }
    // A full page ⇒ more unsigned heads remain; a caller must re-drain.
    (heads_resigned, scanned >= batch_limit)
}

/// Drain a single `(tenant_id, region)` partition.
///
/// B-038 lease wrapper. When `lease_enabled` is OFF this is a thin pass-through to
/// [`drain_partition_inner`] with the fence disabled — EXACTLY the pre-lease
/// behavior (no lease acquire/release, no fence). When ON it acquires the
/// per-partition lease FIRST (before any read): if another drain holds it we
/// return [`PartitionOutcome::Leased`] without touching the partition; otherwise
/// we run the body and, on EVERY exit path (`Sealed`/`Drift`/`Empty`/`Fenced` and
/// the CF-6 `FailClosed` `Err`), release the lease before returning. Single-exit
/// so no path can leak a held lease (a missed release still self-heals on TTL).
#[allow(
    clippy::too_many_arguments,
    reason = "explicit, no shared config struct"
)]
async fn drain_partition(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    now: i64,
    signing_seed: Option<&[u8; 32]>,
    signing_key_id: u64,
    trust_unsigned_resume: bool,
    batch_limit: i64,
    lease_enabled: bool,
) -> Result<PartitionOutcome, String> {
    if !lease_enabled {
        // Lease regime OFF: no lease, no fence — the drain runs exactly as it did
        // before B-038. `i64::MAX` expiry makes `should_fence` a permanent `false`.
        return drain_partition_inner(
            d1,
            tenant_id,
            region,
            now,
            signing_seed,
            signing_key_id,
            trust_unsigned_resume,
            batch_limit,
            false,
            i64::MAX,
        )
        .await;
    }

    let holder = new_lease_holder();
    // Acquire against a FRESH clock read (not the shared sweep `now`, which may be
    // seconds stale for a late partition) so the TTL bounds THIS call, and derive
    // the fence expiry from the SAME instant — the fence boundary must equal the
    // lease's written `expires_ms` so a holder stops writing exactly when its lease
    // becomes stealable, never after.
    let acquired_ms = now_ms();
    let acquired = acquire_lease(
        d1,
        tenant_id,
        region,
        &holder,
        acquired_ms,
        AUDIT_DRAIN_LEASE_TTL_MS,
    )
    .await?;
    if !acquired {
        tracing::debug!(
            region = %region,
            "audit/drain: partition lease held by another drain — skipping (backpressure, not drift)"
        );
        return Ok(PartitionOutcome::Leased);
    }
    let my_lease_expires_ms = acquired_ms.saturating_add(AUDIT_DRAIN_LEASE_TTL_MS);

    // Run the body, then ALWAYS release (single exit). The CF-6 `FailClosed` Err
    // return happens INSIDE the body, so it too passes through the release.
    let outcome = drain_partition_inner(
        d1,
        tenant_id,
        region,
        now,
        signing_seed,
        signing_key_id,
        trust_unsigned_resume,
        batch_limit,
        true,
        my_lease_expires_ms,
    )
    .await;
    if let Err(e) = release_lease(d1, tenant_id, region, &holder).await {
        tracing::warn!(
            error = %e,
            region = %region,
            "audit/drain: lease release failed (best-effort; the lease self-heals on TTL expiry)"
        );
    }
    outcome
}

/// The single-writer drain body for one partition. Correct for a single writer;
/// B-038's [`drain_partition`] wrapper provides that precondition via the lease,
/// and the seal-loop fence (`lease_enabled` + `my_lease_expires_ms`) guarantees a
/// holder stops writing at its own lease expiry.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit, no shared config struct"
)]
async fn drain_partition_inner(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    now: i64,
    signing_seed: Option<&[u8; 32]>,
    signing_key_id: u64,
    trust_unsigned_resume: bool,
    batch_limit: i64,
    lease_enabled: bool,
    my_lease_expires_ms: i64,
) -> Result<PartitionOutcome, String> {
    let checkpoint = read_checkpoint(d1, tenant_id, region).await?;

    // B-054: this route still owns only the proven legacy/v1 drain.  Once a
    // partition has a versioned epoch head, the witnessed runtime must own the
    // seal; treating it as E0 would silently downgrade its links.
    reject_unwired_epoch_checkpoint(checkpoint.as_ref())?;

    // CF-6: verify the keyed head signature of the checkpoint we resume from. A
    // non-NULL signature signed under the current key id that does NOT verify (or
    // a signed head with no seed to verify it) is tampering — refuse to extend the
    // chain from a forged head (fail-CLOSED, SEV-1).
    match check_head_on_resume(
        checkpoint.as_ref(),
        signing_seed,
        signing_key_id,
        tenant_id,
        region,
        trust_unsigned_resume,
    ) {
        HeadResumeCheck::FailClosed => {
            tracing::error!(
                tenant_id = %tenant_id,
                region = %region,
                signing_key_id = signing_key_id,
                severity = "SEV-1",
                tamper_detected = true,
                "audit/drain: audit_chain_head signature does NOT verify — TAMPER DETECTED; \
                 refusing to extend the chain from a forged head (fail-CLOSED, CF-6)"
            );
            return Err(
                "audit_chain_head signature verification failed (tamper-detected): refusing to \
                 extend the chain from a forged head"
                    .to_string(),
            );
        }
        HeadResumeCheck::Verified => {
            tracing::debug!(
                tenant_id = %tenant_id,
                region = %region,
                "audit/drain: resumed-head signature verified (CF-6)"
            );
        }
        HeadResumeCheck::Proceed => {}
    }

    let sealed_tail = read_sealed_tail(d1, tenant_id, region).await?;

    // Resume the builder honestly via resume()/new() (GENESIS when there is no
    // prior state at all). The resume point is crash-safe (sealed-tail authoritative).
    let resume = resolve_resume(
        checkpoint.as_ref().map(|c| (c.head, c.next_sequence)),
        sealed_tail,
    );
    let builder = match resume {
        Some((head, seq)) => HashChainBuilder::resume(head, seq),
        None => HashChainBuilder::new(),
    };

    let rows = read_pending_rows(d1, tenant_id, region, batch_limit).await?;
    if rows.is_empty() {
        return Ok(PartitionOutcome::Empty);
    }

    let (sealed, new_head, new_seq) = seal_rows(*builder.head(), builder.next_sequence(), &rows)?;

    // Seal the rows FIRST (deterministic + idempotent: guarded by emitted_at IS
    // NULL). A crash here leaves correctly-sealed rows the next drain resumes from
    // (sealed-tail authoritative) — never a gap.
    //
    // B-038 SELF-FENCE (load-bearing): before each write, if the lease regime is
    // ON and our lease has expired (`now_ms() >= my_lease_expires_ms`), STOP —
    // seal only the pre-expiry prefix and do NOT advance the head. NOT advancing
    // is crash-safe: the next drain resumes from the now-ahead sealed tail and
    // continues (the SAME crash-recovery path). This makes a holder provably stop
    // writing at its own expiry, so it cannot still be sealing after a stealer
    // takes the now-expired lease — closing the residual fork window a bare TTL
    // lease leaves open. When the lease is OFF the fence never trips and the loop
    // runs to completion exactly as before.
    let fence = run_fenced_seal_loop(
        &sealed,
        lease_enabled,
        my_lease_expires_ms,
        now_ms,
        |row| async move { write_seal(d1, &row, now).await },
    )
    .await?;
    if let FencedSeal::Fenced(prefix) = fence {
        tracing::warn!(
            region = %region,
            prefix_sealed = prefix,
            "audit/drain: lease expired mid-seal — FENCED after the prefix, head NOT advanced; \
             a re-drain resumes from the sealed tail (B-038)"
        );
        return Ok(PartitionOutcome::Fenced(prefix));
    }

    // CF-6: sign the canonical head tuple we are about to commit (when a seed is
    // configured). No seed ⇒ advance UNSIGNED (NULL columns, legacy/tolerated).
    let new_head_hex = new_head.to_hex();
    let (signature, signed_at_ms, key_id_col) = match signing_seed {
        Some(seed) => {
            let sig = sign_head(
                seed,
                signing_key_id,
                tenant_id,
                region,
                &new_head_hex,
                new_seq,
            )?;
            (Some(sig), Some(now), Some(signing_key_id))
        }
        None => (None, None, None),
    };

    // Advance the checkpoint with a compare-and-set on the resumed value.
    let committed = advance_head_cas(
        d1,
        tenant_id,
        region,
        checkpoint.as_ref(),
        &new_head_hex,
        new_seq,
        now,
        signature.as_deref(),
        signed_at_ms,
        key_id_col,
    )
    .await?;

    if committed {
        Ok(PartitionOutcome::Sealed(sealed.len() as u64))
    } else {
        // Drift: a concurrent drain advanced the head first. The CAS gates only
        // the head advance, so this branch guards the SEQUENTIAL crash-then-resume
        // race — where the resumed rows ARE genuinely identical (same single
        // writer, same tail, same head). The CONCURRENT-overlap fork the CAS can
        // NOT catch (it runs after the seals hit disk) is closed upstream by the
        // B-038 partition lease + seal-loop fence, which enforce the single-writer
        // precondition this no-op always needed; we simply do not double-advance.
        Ok(PartitionOutcome::Drift)
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
