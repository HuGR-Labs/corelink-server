/// Storage-byte-accounting decorator over the AC update + delete trait objects.
///
/// Same reserve→commit→release discipline as [`AccountingCasHandler`], over the
/// `AcUpdateHandler` / `AcDeleteHandler` surface (Bazel AC writes + native AC).
///
/// `key_locks` is a FIXED [`CAS_LOCK_SHARDS`]-wide array of per-`(tenant,
/// action_digest)` serialization locks — the EXACT mirror of
/// [`AccountingCasHandler`]'s (see [`CAS_LOCK_SHARDS`] for the rt-nuclear C2
/// rationale). AC entries are mutable (a result payload's size can change), so a
/// concurrent AC `update` + `delete` of the SAME key has the identical
/// write-vs-delete byte-accounting race the CAS plane already closed: the delete
/// releases a stale `reclaimed_bytes` while the update independently
/// reserves/commits → `bytes_used` UNDER-count (storage-quota evasion). Both
/// `update` and `delete` acquire the shard their `action_digest` maps to for
/// their entire reserve/commit/release sequence, so a write and a delete of the
/// SAME key can never interleave their accounting; distinct keys map to other
/// shards and stay fully concurrent.
#[non_exhaustive]
pub struct AccountingAcHandler {
    update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
    delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
    accountant: Arc<ByteAccountant>,
    /// Fixed, memory-bounded shard array of per-`(tenant, action_digest)` async
    /// locks (mirrors [`AccountingCasHandler::key_locks`]).
    key_locks: Arc<Vec<Arc<tokio::sync::Mutex<()>>>>,
    /// BYOK Wave 3b (GATED-INERT): the SAME per-tenant config cache the AC
    /// storage handler uses — see [`AccountingCasHandler::byok_config_cache`].
    /// `None` ⇒ plaintext-size accounting (today's behaviour).
    byok_config_cache: Option<Arc<ByokConfigCache>>,
}

impl core::fmt::Debug for AccountingAcHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountingAcHandler")
            .field("accountant", &self.accountant)
            .field("key_lock_shards", &self.key_locks.len())
            .finish_non_exhaustive()
    }
}

impl AccountingAcHandler {
    /// Wrap the AC update + delete handlers with byte accounting.
    #[must_use]
    pub fn new(
        update_inner: Arc<dyn corelink_handler_ac::AcUpdateHandler>,
        delete_inner: Arc<dyn corelink_handler_ac::AcDeleteHandler>,
        accountant: Arc<ByteAccountant>,
    ) -> Self {
        let key_locks = (0..CAS_LOCK_SHARDS)
            .map(|_| Arc::new(tokio::sync::Mutex::new(())))
            .collect::<Vec<_>>();
        Self {
            update_inner,
            delete_inner,
            accountant,
            key_locks: Arc::new(key_locks),
            byok_config_cache: None,
        }
    }

    /// Attach the BYOK Wave-3b config cache so a BYOK-`active` tenant is
    /// reserved/released at its committed CIPHERTEXT size (audit C3); mirror of
    /// [`AccountingCasHandler::with_byok`]. `None` (the default) keeps the exact
    /// plaintext-size accounting.
    #[must_use]
    pub fn with_byok(mut self, byok_config_cache: Arc<ByokConfigCache>) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self
    }

    /// Acquire the per-`(tenant, action_digest)` serialization guard (the shard
    /// the key hashes to) and block on it via the SAME `block_in_place` +
    /// `block_on` bridge the R2 handlers use for their async I/O.
    ///
    /// Byte-identical to [`AccountingCasHandler::lock_for`]. Held by BOTH
    /// [`Self::update`] (across reserve→inner-update→release) and [`Self::delete`]
    /// (across inner-delete→release) so an update and a delete of the SAME AC key
    /// cannot interleave their byte-accounting sequences (rt-nuclear C2 sibling).
    /// The returned guard must be held for the whole accounting sequence.
    ///
    /// Returns an [`tokio::sync::OwnedMutexGuard`] (the shard `Arc` is cloned so
    /// the guard owns its reference and need not borrow the array).
    fn lock_for(&self, tenant: &str, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tenant.hash(&mut hasher);
        // A separator so `(a, bc)` and `(ab, c)` cannot collapse to one key.
        0u8.hash(&mut hasher);
        key.hash(&mut hasher);
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

impl corelink_handler_ac::AcUpdateHandler for AccountingAcHandler {
    fn update(
        &self,
        req: corelink_handler_ac::AcUpdateRequest,
    ) -> Result<corelink_handler_ac::AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        let tenant = req.tenant.clone();
        let plaintext_len = i64::try_from(req.result_payload.len()).unwrap_or(i64::MAX);
        // Worker-resolved per-tier cap (seeds a FRESH row; `None` ⇒ fail-closed).
        let quota_seed = req.storage_quota_bytes;
        // rt-nuclear C2 sibling (AC plane): hold the per-`(tenant, action_digest)`
        // serialization guard across the WHOLE reserve→commit→release below, so a
        // concurrent `delete` of the SAME AC key cannot interleave its
        // delete→release with our reserve/release and under-count `bytes_used`.
        // Distinct keys map to other shards and stay concurrent.
        let _key_guard = self.lock_for(&tenant, &req.action_digest);
        // BYOK Wave 3b (audit C3): account the COMMITTED (stored) size — a
        // BYOK-`active` tenant's AC object is the ciphertext blob (plaintext +
        // BYOK_CLB1_OVERHEAD); reserve THAT so it matches the real R2 object the
        // delete path releases → no drift. Config error ⇒ fail CLOSED (503).
        // `None` cache / non-BYOK ⇒ `byte_len == plaintext_len` (unchanged).
        let byte_len = match byok_committed_len(
            self.byok_config_cache.as_ref(),
            &tenant,
            plaintext_len,
        ) {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(error = %e, "ac: byok committed-size lookup failed; failing closed");
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        };
        match block_on_accrue(&self.accountant, &tenant, byte_len, quota_seed) {
            Ok(AccrueOutcome::Accrued) => {}
            Ok(AccrueOutcome::OverCap) => {
                return Err(AcHandlerError::Internal(format!(
                    "{OVER_CAP_SENTINEL}ac write would exceed storage cap"
                )));
            }
            Ok(AccrueOutcome::Indeterminate) => {
                tracing::error!(
                    tenant = %tenant,
                    "ac: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}storage cap indeterminate (no row, no resolved cap)"
                )));
            }
            Err(e) => {
                tracing::error!(error = %e, "ac: byte reservation failed; failing closed");
                return Err(AcHandlerError::Internal(format!(
                    "{ACCT_UNAVAILABLE_SENTINEL}{e}"
                )));
            }
        }
        match self.update_inner.update(req) {
            Ok(resp) => {
                // Idempotent / divergent-refused AC writes that stored nothing
                // new (`durable == false`) roll the reservation back.
                if !resp.durable {
                    block_on_release(&self.accountant, &tenant, byte_len);
                }
                Ok(resp)
            }
            Err(e) => {
                block_on_release(&self.accountant, &tenant, byte_len);
                Err(e)
            }
        }
    }
}

impl corelink_handler_ac::AcDeleteHandler for AccountingAcHandler {
    fn delete(
        &self,
        req: corelink_handler_ac::AcDeleteRequest,
    ) -> Result<corelink_handler_ac::AcDeleteResponse, corelink_handler_ac::AcHandlerError> {
        let tenant = req.tenant.clone();
        // rt-nuclear C2 sibling (AC plane): hold the SAME per-`(tenant,
        // action_digest)` serialization guard the update path uses, across the
        // WHOLE inner-delete→release below, so a concurrent `update` of the SAME
        // AC key cannot interleave its reserve/release with our delete→release
        // (which would let the delete release this key's bytes while the update
        // re-commits them → `bytes_used` under-count). Distinct keys hash to other
        // shards (concurrent).
        let _key_guard = self.lock_for(&tenant, &req.action_digest);
        let resp = self.delete_inner.delete(req)?;
        let reclaimed = i64::try_from(resp.reclaimed_bytes).unwrap_or(i64::MAX);
        if reclaimed > 0 {
            block_on_release(&self.accountant, &tenant, reclaimed);
        }
        Ok(resp)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
pub(crate) mod testing {
    include!("b126_m2_test_1_1.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR];
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_2_1.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_2_1_REANCHOR];
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod decorator_tests {
    include!("b126_m2_test_3_1.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_3_1_REANCHOR];
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod byok_accounting_tests {
    include!("b126_m2_test_4_1.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_4_1_REANCHOR];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
