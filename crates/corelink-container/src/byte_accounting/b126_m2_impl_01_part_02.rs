/// Sentinel prefix carried in `CasHandlerError::Internal` / `AcHandlerError::Internal`
/// by an accounting decorator when a write is refused because it would push the
/// tenant past its storage cap. The route `map_err` maps this to HTTP **402**
/// (Payment Required — "storage quota exceeded"), distinct from a generic 500.
pub const OVER_CAP_SENTINEL: &str = "storage-over-cap: ";

/// Sentinel prefix carried in `…::Internal` by an accounting decorator when the
/// byte-accounting backend (D1) is unavailable. The route `map_err` maps this to
/// HTTP **503** — fail-CLOSED: a write we cannot account is refused, never
/// silently allowed (that would re-open the unbounded-storage hole).
pub const ACCT_UNAVAILABLE_SENTINEL: &str = "storage-accounting-unavailable: ";

/// Bridge an async accountant call onto the sync handler trait by blocking on the
/// current tokio runtime — the same `block_in_place` + `block_on` pattern the R2
/// handlers use for their async S3 I/O. `quota_seed` is the request's resolved
/// per-tier cap (used to seed a fresh `tenant_storage_state` row; see
/// [`ByteStore::check_and_accrue`]).
fn block_on_accrue(
    acc: &ByteAccountant,
    tenant: &str,
    bytes: i64,
    quota_seed: Option<i64>,
) -> Result<AccrueOutcome, String> {
    // This bridge is called from the synchronous accounting decorator. Keep
    // the phase strictly around the D1 future; the inner R2 handler is outside
    // this scope and records `ostore` independently.
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| handle.block_on(acc.accrue(tenant, bytes, quota_seed)))
}

/// Compute the **committed (stored) byte size** the accountant must reserve and
/// release for a write — the size that actually lands in R2, so reserve ==
/// release == the eventual delete-release (which measures the real R2 object)
/// and `bytes_used` can NEVER drift (audit C3 CRITICAL).
///
/// For a BYOK-ENCRYPTING tenant the stored object is the `CLB1` convergent blob
/// = `plaintext_len + BYOK_CLB1_OVERHEAD` (32 B). Such a tenant is detected via
/// the SAME [`ByokConfigCache`] the storage handlers use — engagement
/// [`ByokEngagement::Encrypt`] for a non-`_public` tenant — so a write that
/// stores ciphertext is accounted at its ciphertext size on BOTH the reserve and
/// (on rollback) the release.
///
/// - `cache == None` (today's production / tests) ⇒ plaintext size — byte-for-
///   byte the legacy behaviour, zero change for every non-BYOK deployment.
/// - `_public`, not configured, or `Plaintext` engagement ⇒ plaintext size.
/// - `FailClosed` engagement (Mode B / partial) ⇒ plaintext size: the inner
///   storage write fails closed and stores NOTHING, so the (plaintext-sized)
///   reservation is rolled back net-zero — no ciphertext is ever committed.
/// - `Encrypt` ⇒ `plaintext_len + BYOK_CLB1_OVERHEAD`.
///
/// FAIL-CLOSED: a config-cache read error returns `Err` — the caller maps it to
/// the 503 fail-closed sentinel. We must NEVER under-reserve an active tenant on
/// an undetermined config (and the shared cache means the inner handler would
/// fail closed on the same error anyway).
fn byok_committed_len(
    cache: Option<&Arc<ByokConfigCache>>,
    tenant: &str,
    plaintext_len: i64,
) -> Result<i64, String> {
    let Some(cache) = cache else {
        return Ok(plaintext_len);
    };
    // `_public` is deterministic public content — never encrypted (dedup), so it
    // is byte-identical to today (frozen policy: non-BYOK + `_public` unchanged).
    if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
        return Ok(plaintext_len);
    }
    let handle = tokio::runtime::Handle::current();
    let cfg = tokio::task::block_in_place(|| handle.block_on(cache.get(tenant)))
        .map_err(|e| format!("byok config read (accounting): {e}"))?;
    let Some(cfg) = cfg else {
        return Ok(plaintext_len);
    };
    match engagement_for(&cfg) {
        // Mode A (convergent) stores `CLB1` (+32 B); Mode B (random) stores `CLB2`
        // (+20 B — the nonce lives in `byok_envelope`, not inline). Account the
        // committed CIPHERTEXT size so the reservation matches the real R2 object.
        ByokEngagement::Encrypt(ByokCryptoMode::Convergent) => {
            Ok(plaintext_len.saturating_add(i64::try_from(BYOK_CLB1_OVERHEAD).unwrap_or(i64::MAX)))
        }
        ByokEngagement::Encrypt(ByokCryptoMode::Random) => {
            Ok(plaintext_len.saturating_add(i64::try_from(BYOK_CLB2_OVERHEAD).unwrap_or(i64::MAX)))
        }
        ByokEngagement::Plaintext | ByokEngagement::FailClosed(_) => Ok(plaintext_len),
    }
}

#[cfg(test)]
pub(crate) fn byok_committed_len_for_test(
    cache: Option<&Arc<ByokConfigCache>>,
    tenant: &str,
    plaintext_len: i64,
) -> Result<i64, String> {
    byok_committed_len(cache, tenant, plaintext_len)
}

/// Bridge an async release call onto the sync handler trait (see [`block_on_accrue`]).
fn block_on_release(acc: &ByteAccountant, tenant: &str, bytes: i64) {
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
    let handle = tokio::runtime::Handle::current();
    if let Err(e) = tokio::task::block_in_place(|| handle.block_on(acc.release(tenant, bytes))) {
        // A failed release over-counts the tenant (conservative — never widens
        // the cap), so log + continue rather than fail an already-committed op.
        tracing::warn!(error = %e, tenant = %tenant, bytes, "byte-accounting: release failed (counter over-counts; conservative)");
    }
}

fn block_on_record_liability(
    acc: &ByteAccountant,
    tenant: &str,
    logical_key: &str,
    bytes: i64,
    state: MutationLiabilityState,
    intent_id: Option<uuid::Uuid>,
) -> Result<(), String> {
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        handle.block_on(acc.record_mutation_liability(
            tenant,
            logical_key,
            bytes,
            state,
            intent_id,
        ))
    })
}

fn block_on_settle_liability(
    acc: &ByteAccountant,
    tenant: &str,
    logical_key: &str,
    resolution: MutationLiabilityResolution,
) -> Result<MutationLiabilitySettlement, String> {
    let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        handle.block_on(acc.settle_mutation_liability(tenant, logical_key, resolution))
    })
}

/// Classify one effect-aware CAS failure into its accounting disposition.
///
/// This is deliberately the only decision point for failed CAS writes:
/// `NotWritten` releases, while every other outcome retains a durable
/// liability. `unknown_intent_id` is generated once by the caller so the
/// ownership record remains stable throughout this attempt.
fn classify_mutation_failure(
    effect: &corelink_handler_cas::MutationEffect,
    unknown_intent_id: uuid::Uuid,
) -> (MutationLiabilityState, Option<uuid::Uuid>, bool) {
    match effect {
        corelink_handler_cas::MutationEffect::NotWritten => {
            (MutationLiabilityState::Released, None, true)
        }
        corelink_handler_cas::MutationEffect::Committed => {
            (MutationLiabilityState::Committed, None, false)
        }
        corelink_handler_cas::MutationEffect::Pending { intent_id } => {
            (MutationLiabilityState::Pending, Some(*intent_id), false)
        }
        corelink_handler_cas::MutationEffect::Unknown => {
            (MutationLiabilityState::Unknown, Some(unknown_intent_id), false)
        }
        _ => (MutationLiabilityState::Unknown, Some(unknown_intent_id), false),
    }
}

/// Number of per-`(tenant, hash)` serialization-lock shards held by an
/// [`AccountingCasHandler`].
///
/// rt-nuclear C2 (CAS write-vs-delete byte-accounting race): `delete_if_present`
/// in `r2_s3` serializes the HEAD+DELETE measure-and-delete **per key**, but that
/// lock covers delete-vs-delete ONLY. A concurrent overwrite-`write` of the SAME
/// content-addressed key takes NO part in it, so a `delete` of key K (size L) can
/// observe size L and `release` L while a racing `write` independently
/// reserves+commits its own bytes — the two operations' reserve/release no longer
/// net to the true on-disk total, UNDER-counting `bytes_used` by up to L when the
/// write wins (the blob is on disk but the counter was decremented) → a
/// storage-quota evasion.
///
/// We close that by serializing the FULL reserve→commit→release of a `write` and
/// the FULL delete→release of a `delete` against the SAME `(tenant, hash)` under
/// one lock — so their accounting sequences can never interleave for one key,
/// while distinct keys stay fully concurrent.
///
/// A FIXED, power-of-two shard array keeps the lock set **memory-bounded** (no
/// per-key map that grows with the live keyspace and needs pruning, unlike the
/// `r2_s3` delete map): every `(tenant, hash)` deterministically maps to one of
/// these shards. Distinct keys that collide on a shard serialize (a rare,
/// correctness-preserving false-share); the same key ALWAYS maps to the same
/// shard, which is the property the race requires. 256 shards keep cross-key
/// contention negligible for any realistic per-container concurrency.
const CAS_LOCK_SHARDS: usize = 256;

impl AccountingCasHandler {
    /// Acquire the per-`(tenant, hash)` serialization guard (the shard the key
    /// hashes to) and block on it via the SAME `block_in_place` + `block_on`
    /// bridge the R2 handlers use for their async I/O.
    ///
    /// Held by BOTH [`Self::write`] (across reserve→inner-PUT→release) and
    /// [`Self::delete`] (across inner-delete→release) so a write and a delete of
    /// the SAME content-addressed key cannot interleave their byte-accounting
    /// sequences (rt-nuclear C2). The returned guard must be held for the whole
    /// accounting sequence.
    ///
    /// Returns an [`tokio::sync::OwnedMutexGuard`] (the shard `Arc` is cloned so
    /// the guard owns its reference and need not borrow the array) — held by the
    /// caller across the entire reserve/commit/release sequence.
    fn lock_for(&self, tenant: &str, hash: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tenant.hash(&mut hasher);
        // A separator so `(a, bc)` and `(ab, c)` cannot collapse to one key.
        0u8.hash(&mut hasher);
        hash.hash(&mut hasher);
        // Map the key hash onto a shard. The modulo is correct for any shard
        // count; `CAS_LOCK_SHARDS` (256) is a power of two so the distribution is
        // uniform and the op is a single cheap division off a 64-bit hash.
        let idx = (hasher.finish() as usize) % self.key_locks.len();
        // `idx < len` by construction (modulo), so `get` is always `Some`; the
        // `unwrap_or_else` is unreachable totality that keeps clippy's
        // `indexing_slicing` happy without a panic path.
        let lock = self
            .key_locks
            .get(idx)
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())));
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(lock.lock_owned()))
    }
}

/// Storage-byte-accounting decorator over the CAS write + delete trait objects.
///
/// Holds the inner `R2CasHandler` (behind both trait objects) + the accountant.
/// Implements [`corelink_handler_cas::CasWriteHandler`] and
/// [`corelink_handler_cas::CasDeleteHandler`] with reserve→commit→release; see
/// the module-section comment above for the full discipline.
///
/// `key_locks` is a FIXED [`CAS_LOCK_SHARDS`]-wide array of per-`(tenant, hash)`
/// serialization locks (see [`CAS_LOCK_SHARDS`] for the rt-nuclear C2 rationale):
/// both `write` and `delete` acquire the shard their key maps to for their entire
/// reserve/commit/release sequence, so a write and a delete of the SAME key can
/// never interleave their accounting (which would under-count `bytes_used`).
#[non_exhaustive]
pub struct AccountingCasHandler {
    write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
    delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    accountant: Arc<ByteAccountant>,
    /// Fixed, memory-bounded shard array of per-`(tenant, hash)` async locks.
    key_locks: Arc<Vec<Arc<tokio::sync::Mutex<()>>>>,
    /// BYOK Wave 3b (GATED-INERT): the SAME per-tenant config cache the storage
    /// handlers use. `None` ⇒ plaintext-size accounting (today's behaviour). When
    /// `Some` AND a tenant is BYOK-`active`, the reserve/release size is the
    /// committed CIPHERTEXT size (`plaintext + BYOK_CLB1_OVERHEAD`) so it matches
    /// the on-disk object the delete path releases (audit C3 — no drift).
    byok_config_cache: Option<Arc<ByokConfigCache>>,
}

impl core::fmt::Debug for AccountingCasHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountingCasHandler")
            .field("accountant", &self.accountant)
            .field("key_lock_shards", &self.key_locks.len())
            .finish_non_exhaustive()
    }
}

impl AccountingCasHandler {
    /// Wrap the CAS write + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        write_inner: Arc<dyn corelink_handler_cas::CasWriteHandler>,
        delete_inner: Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        let key_locks = (0..CAS_LOCK_SHARDS)
            .map(|_| Arc::new(tokio::sync::Mutex::new(())))
            .collect::<Vec<_>>();
        Self {
            write_inner,
            delete_inner,
            accountant,
            key_locks: Arc::new(key_locks),
            byok_config_cache: None,
        }
    }

    /// Attach the BYOK Wave-3b config cache so a BYOK-`active` tenant is
    /// reserved/released at its committed CIPHERTEXT size (audit C3). Mirrors
    /// [`crate::storage::r2_s3::R2CasHandler::with_byok`]'s gating; pass the SAME
    /// `ByokConfigCache` Arc the storage handler holds so the active-ness lookup
    /// is a shared in-memory cache HIT (one D1 hop total). `None` (the default)
    /// keeps the exact plaintext-size accounting.
    #[must_use]
    pub fn with_byok(mut self, byok_config_cache: Arc<ByokConfigCache>) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self
    }

    #[cfg(test)]
    pub(crate) fn byok_config_cache_for_test(&self) -> Option<&Arc<ByokConfigCache>> {
        self.byok_config_cache.as_ref()
    }
}

impl corelink_handler_cas::CasWriteHandler for AccountingCasHandler {
    fn write(
        &self,
        req: corelink_handler_cas::CasWriteRequest,
    ) -> Result<corelink_handler_cas::CasWriteResponse, corelink_handler_cas::CasHandlerError> {
        self.write_with_effect(req).map_err(|failure| failure.cause)
    }

    fn write_with_effect(
        &self,
        req: corelink_handler_cas::CasWriteRequest,
    ) -> Result<corelink_handler_cas::CasWriteResponse, corelink_handler_cas::CasWriteFailure>
    {
        use corelink_handler_cas::{CasHandlerError, CasWriteFailure};
        // Reject a forged physical/accounting pairing before touching the
        // quota ledger.  The inner handler repeats the gate (and emits the
        // canonical denial audit), but the decorator must not even transiently
        // reserve another tenant's bytes for a malformed request.
        if !req.is_authorized_for_caller() {
            return Err(CasWriteFailure::not_written(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            }));
        }
        // `tenant` is the physical storage namespace (and BYOK namespace),
        // while `accounting_tenant` is the authenticated tenant whose quota
        // pays for this reservation.  Public moat writes intentionally have
        // `tenant == "_public"` and a real `accounting_tenant`.
        let storage_namespace = req.tenant.clone();
        let accounting_tenant = req.accounting_tenant.clone();
        let plaintext_len = i64::try_from(req.bytes.len()).unwrap_or(i64::MAX);
        // The resolved cap is always the caller's cap, including public writes.
        // A missing cap therefore fails closed for a fresh real tenant; `_public`
        // is never an implicit unlimited escape hatch.
        let quota_seed = req.storage_quota_bytes;
        // rt-nuclear C2: hold the per-`(accounting tenant, hash)` serialization
        // guard across
        // the WHOLE reserve→commit→release below, so a concurrent `delete` of the
        // SAME content-addressed key cannot interleave its delete→release with our
        // reserve/release and under-count `bytes_used`. Distinct keys map to other
        // shards and stay concurrent.
        let _key_guard = self.lock_for(&accounting_tenant, &req.claimed_hash);
        // BYOK Wave 3b (audit C3): account the COMMITTED (stored) size — for a
        // BYOK-`active` tenant the R2 object is the ciphertext blob (plaintext +
        // BYOK_CLB1_OVERHEAD), so reserve THAT size (the delete path already
        // releases the real R2 object size) → reserve == release, no drift. A
        // config read error fails CLOSED (503). `None` cache / non-BYOK tenant ⇒
        // `byte_len == plaintext_len`, byte-identical to today.
        let committed_len = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Accounting);
            byok_committed_len(
                self.byok_config_cache.as_ref(),
                &storage_namespace,
                plaintext_len,
            )
        };
        let byte_len = match committed_len {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(error = %e, "cas: byok committed-size lookup failed; failing closed");
                return Err(CasWriteFailure::not_written(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                ))));
            }
        };
        // RESERVE before the R2 PUT (cluster-C): an over-cap / indeterminate
        // reservation is rejected here, so the inner write — the durable R2 PUT
        // — NEVER runs and no uncounted blob is committed.
        match block_on_accrue(&self.accountant, &accounting_tenant, byte_len, quota_seed) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(CasWriteFailure::not_written(CasHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}cas write would exceed storage cap"
                ))));
            }
            Ok(AccrueOutcome::Indeterminate) => {
                // Fresh/unsynced tenant + no resolved cap → we refuse to seed an
                // uncapped row. Fail CLOSED (503) — absence is NOT unlimited.
                tracing::error!(
                    tenant = %accounting_tenant,
                    "cas: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return Err(CasWriteFailure::not_written(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}storage cap indeterminate (no row, no resolved cap)"
                ))));
            }
            Err(e) => {
                tracing::error!(error = %e, "cas: byte reservation failed; failing closed");
                return Err(CasWriteFailure::not_written(CasHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                ))));
            }
        }
        let logical_key = req.claimed_hash.clone();
        if let Err(e) = block_on_record_liability(
            &self.accountant,
            &accounting_tenant,
            &logical_key,
            byte_len,
            MutationLiabilityState::Reserved,
            None,
        ) {
            block_on_release(&self.accountant, &accounting_tenant, byte_len);
            return Err(CasWriteFailure::not_written(CasHandlerError::Internal(format!(
                "{ACCT_UNAVAILABLE_SENTINEL}mutation liability: {e}"
            ))));
        }
        match self.write_inner.write_with_effect(req) {
            Ok(resp) => {
                // An idempotent re-write stored NOTHING new (`durable == false`),
                // so roll the reservation back to avoid double-counting.
                if !resp.durable {
                    if let Err(e) = block_on_settle_liability(
                        &self.accountant,
                        &accounting_tenant,
                        &logical_key,
                        MutationLiabilityResolution::NotWritten,
                    ) {
                        return Err(CasWriteFailure::unknown(CasHandlerError::Internal(format!(
                            "{ACCT_UNAVAILABLE_SENTINEL}mutation liability: {e}"
                        ))));
                    }
                } else if let Err(e) = block_on_record_liability(
                    &self.accountant,
                    &accounting_tenant,
                    &logical_key,
                    byte_len,
                    MutationLiabilityState::Committed,
                    None,
                ) {
                    return Err(CasWriteFailure::committed(CasHandlerError::Internal(format!(
                        "{ACCT_UNAVAILABLE_SENTINEL}mutation liability: {e}"
                    ))));
                }
                Ok(resp)
            }
            Err(failure) => {
                let (state, intent_id, refund) =
                    classify_mutation_failure(&failure.effect, uuid::Uuid::new_v4());
                if refund {
                    if let Err(e) = block_on_settle_liability(
                        &self.accountant,
                        &accounting_tenant,
                        &logical_key,
                        MutationLiabilityResolution::NotWritten,
                    ) {
                        return Err(CasWriteFailure::unknown(CasHandlerError::Internal(format!(
                            "{ACCT_UNAVAILABLE_SENTINEL}mutation liability: {e}"
                        ))));
                    }
                } else if let Err(e) = block_on_record_liability(
                    &self.accountant,
                    &accounting_tenant,
                    &logical_key,
                    byte_len,
                    state,
                    intent_id,
                ) {
                    return Err(CasWriteFailure::unknown(CasHandlerError::Internal(format!(
                        "{ACCT_UNAVAILABLE_SENTINEL}mutation liability: {e}"
                    ))));
                }
                Err(failure)
            }
        }
    }
}

impl corelink_handler_cas::CasDeleteHandler for AccountingCasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, corelink_handler_cas::CasHandlerError>
    {
        let tenant = req.tenant.clone();
        // rt-nuclear C2: hold the SAME per-`(tenant, hash)` serialization guard the
        // write path uses, across the WHOLE inner-delete→release below, so a
        // concurrent overwrite-`write` of the SAME content-addressed key cannot
        // interleave its reserve/release with our delete→release (which would let
        // the delete release this key's bytes while the write re-commits them →
        // `bytes_used` under-count). Distinct keys hash to other shards (concurrent).
        let _key_guard = self.lock_for(&tenant, &req.hash);
        let resp = self.delete_inner.delete(req)?;
        // RELEASE the reclaimed bytes so a delete frees the tenant's headroom
        // (cluster-C: deletes that never decrement leak the cap forever).
        let reclaimed = i64::try_from(resp.reclaimed_bytes).unwrap_or(i64::MAX);
        if reclaimed > 0 {
            block_on_release(&self.accountant, &tenant, reclaimed);
        }
        Ok(resp)
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
