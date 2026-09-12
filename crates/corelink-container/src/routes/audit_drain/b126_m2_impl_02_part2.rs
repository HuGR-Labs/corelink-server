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
    witness: Option<&WitnessClient>,
    link_keyring: Option<&LinkKeyring>,
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
            witness,
            link_keyring,
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
        witness,
        link_keyring,
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
    witness: Option<&WitnessClient>,
    link_keyring: Option<&LinkKeyring>,
) -> Result<PartitionOutcome, String> {
    let checkpoint = read_checkpoint(d1, tenant_id, region).await?;

    if checkpoint
        .as_ref()
        .is_some_and(|head| head.head_message_version.is_some())
    {
        return drain_partition_v2(
            d1,
            tenant_id,
            region,
            now,
            signing_seed,
            signing_key_id,
            batch_limit,
            checkpoint.as_ref().ok_or("v2 checkpoint disappeared")?,
            witness,
            link_keyring,
        )
        .await;
    }

    // B-054: this route still owns only the proven legacy/v1 drain.  Once a
    // partition has a versioned epoch head, the witnessed runtime must own the
    // seal; treating it as E0 would silently downgrade its links.
    reject_unwired_epoch_checkpoint(checkpoint.as_ref())?;

    // CF-6: verify the keyed head signature of the checkpoint we resume from. A
    // non-NULL signature signed under the current key id that does NOT verify (or
    // a signed head with no seed to verify it) is tampering — refuse to extend the
    // chain from a forged head (fail-CLOSED, SEV-1).
    let head_check = check_head_on_resume(
        checkpoint.as_ref(),
        signing_seed,
        signing_key_id,
        tenant_id,
        region,
        trust_unsigned_resume,
    );
    match head_check {
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

    let tail_resume_context = LegacyTailResumeContext {
        checkpoint: checkpoint.as_ref(),
        head_check,
        signing_seed,
        signing_key_id,
        trust_unsigned_resume,
        tenant_id,
        region,
    };
    let sealed_tail = read_sealed_tail(d1, &tail_resume_context).await?;

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
    // B-038 SELF-FENCE (load-bearing): before and after each bounded JSON1
    // statement, if the lease regime is ON and our lease has expired
    // (`now_ms() >= my_lease_expires_ms`), STOP — seal only the committed prefix
    // and do NOT advance the head. A next drain resumes from the now-ahead
    // sealed tail. The chunk is bounded to keep the post-check's race window
    // finite; when the lease is OFF the fence remains inert.
    let fence = run_chunked_fenced_seal_loop(
        &sealed,
        lease_enabled,
        my_lease_expires_ms,
        now_ms,
        |chunk| async move { write_seal_chunk(d1, &chunk, now).await },
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

#[allow(
    clippy::too_many_arguments,
    reason = "explicit v2 trust-boundary inputs"
)]
async fn drain_partition_v2(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    now: i64,
    signing_seed: Option<&[u8; 32]>,
    current_signing_key_id: u64,
    batch_limit: i64,
    checkpoint: &HeadCheckpoint,
    witness_client: Option<&WitnessClient>,
    link_keyring: Option<&LinkKeyring>,
) -> Result<PartitionOutcome, String> {
    let signing_seed = signing_seed.ok_or("v2 drain requires configured head signing seed")?;
    let witness_client =
        witness_client.ok_or("v2 drain requires complete independent witness config")?;
    if checkpoint.signing_key_id != Some(current_signing_key_id) {
        return Err(
            "v2 head signing key differs from configured key; explicit rotation required"
                .to_owned(),
        );
    }
    let latest = witness_client
        .latest(tenant_id, region)
        .await?
        .ok_or("external witness latest is null for existing v2 D1 head")?;
    verify_v2_checkpoint_witness(checkpoint, &latest, signing_seed, tenant_id, region)?;

    let active = read_active_epoch(d1, tenant_id, region).await?;
    authenticate_active_epoch(
        &active,
        signing_seed,
        current_signing_key_id,
        tenant_id,
        region,
    )?;
    if checkpoint.epoch_id != Some(active.epoch.epoch_id())
        || checkpoint.epoch_ledger_sequence != Some(active.ledger_sequence)
        || checkpoint.epoch_ledger_hash.as_deref() != Some(active.ledger_hash.as_str())
    {
        return Err("active epoch projection differs from signed v2 head".to_owned());
    }
    let tail = read_any_sealed_tail(d1, tenant_id, region).await?;
    match (checkpoint.next_sequence, tail) {
        (0, None) => {}
        (next, Some((tail_hash, tail_sequence)))
            if tail_sequence.checked_add(1) == Some(next) && tail_hash == checkpoint.head => {}
        _ => return Err("sealed D1 tail differs from signed v2 checkpoint".to_owned()),
    }
    let key = match active.epoch.link_key_id() {
        None => None,
        Some(key_id) => {
            let key = link_keyring
                .and_then(|keys| keys.get(key_id))
                .ok_or("active keyed epoch key is unavailable")?;
            let commitment = active
                .key_commitment
                .as_ref()
                .ok_or("active keyed epoch commitment is unavailable")?;
            if !key_matches_commitment(key, commitment) {
                return Err("configured link key does not match registered commitment".to_owned());
            }
            Some(key)
        }
    };
    let rows = read_pending_rows(d1, tenant_id, region, batch_limit).await?;
    if rows.is_empty() {
        return Ok(PartitionOutcome::Empty);
    }
    let (sealed, _, _) = seal_rows_for_epoch(
        checkpoint.head,
        checkpoint.next_sequence,
        &rows,
        &active.epoch,
        key,
    )?;
    let (sealed, new_head, new_sequence) = bounded_v2_sealed_prefix(&sealed, now)?;
    let new_head_hex = new_head.to_hex();
    let head_signature = sign_head_v2(
        signing_seed,
        current_signing_key_id,
        tenant_id,
        region,
        &new_head_hex,
        new_sequence,
        active.epoch.epoch_id(),
        active.ledger_sequence,
        &active.ledger_hash,
    )?;
    let head_jcs = canonical_head_v2_bytes(
        tenant_id,
        region,
        &new_head_hex,
        new_sequence,
        active.epoch.epoch_id(),
        active.ledger_sequence,
        &active.ledger_hash,
        current_signing_key_id,
    )?;

    // Linearization point occurs outside D1. A timeout/conflict is commit
    // unknown and freezes this partition; ordinary drain never auto-recovers.
    let witnessed = witness_client
        .append(&head_jcs, &head_signature, tenant_id, region, &latest)
        .await?;
    commit_v2_head_transaction(
        d1,
        tenant_id,
        region,
        checkpoint,
        sealed,
        &new_head_hex,
        new_sequence,
        now,
        &head_signature,
        &witnessed,
    )
    .await?;
    Ok(PartitionOutcome::Sealed(sealed.len() as u64))
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
