// BYOK body transformation and Mode-B envelope lifecycle for `R2CasHandler`.
//
// This implementation block is kept separate from handler construction and
// key resolution so each policy surface stays within the B126 source cap.
impl R2CasHandler {
    /// Encrypt the body for a resolved plan. `Ok(None)` ⇒ store the plaintext
    /// unchanged; `Ok(Some(ct))` ⇒ store the ciphertext blob; `Err` ⇒ fail closed
    /// (the caller returns before any PUT — plaintext is NEVER stored).
    async fn encrypt_body(
        &self,
        plan: &ByokBodyPlan,
        plaintext: &[u8],
        allocation_id: Option<&str>,
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(None),
            ByokBodyPlan::Convergent { tcs, ctx } => {
                let stored = encrypt_cas_blob(plaintext, tcs, ctx)
                    .map_err(|e| CasHandlerError::Internal(format!("byok encrypt: {e}")))?;
                Ok(Some(stored))
            }
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                let stored = match allocation_id {
                    Some(allocation_id) => {
                        mode_b
                            .encrypt_for_allocation(plaintext, ctx, allocation_id)
                            .await
                    }
                    None => mode_b.encrypt(plaintext, ctx).await,
                }
                .map_err(|e| CasHandlerError::Internal(format!("byok mode-b encrypt: {e}")))?;
                Ok(Some(stored))
            }
        }
    }

    /// Decrypt the stored body for a resolved plan. `Plaintext` ⇒ return the
    /// stored bytes unchanged; otherwise decrypt (fail closed on any failure —
    /// raw stored bytes are NEVER served). The post-decrypt content-hash
    /// re-verify (audit C1) runs on the returned PLAINTEXT, in the caller.
    async fn decrypt_body(
        &self,
        plan: &ByokBodyPlan,
        stored: Vec<u8>,
        allocation_id: Option<&str>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(stored),
            ByokBodyPlan::Convergent { tcs, ctx } => decrypt_cas_blob(&stored, tcs, ctx)
                .map_err(|e| CasHandlerError::Internal(format!("byok decrypt: {e}"))),
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                match allocation_id {
                    Some(allocation_id) => {
                        mode_b
                            .decrypt_for_allocation(&stored, ctx, allocation_id)
                            .await
                    }
                    None => mode_b.decrypt(&stored, ctx).await,
                }
                .map_err(|e| CasHandlerError::Internal(format!("byok mode-b decrypt: {e}")))
            }
        }
    }

    /// BYOK Wave 4a — reclaim the Mode-B `byok_envelope` row for a resolved plan
    /// AFTER the R2 object has been deleted. ONLY Mode B (`Random`) writes an
    /// envelope row, so `Plaintext` / `Convergent` (Mode A) are no-ops.
    ///
    /// Ordering + fail-safety (frozen policy §3): the caller deletes the R2
    /// object FIRST, then calls this. A failed reclaim leaves an orphan
    /// wrapped-DEK row that now wraps NOTHING (the blob is already gone) — the
    /// SAFE-fail direction — so the caller WARNS and continues rather than fail
    /// the whole delete (which could leave a readable blob whose key was
    /// destroyed). `Ok(())` ⇒ nothing to reclaim or reclaim succeeded.
    async fn reclaim_byok_envelope(
        &self,
        plan: &ByokBodyPlan,
        allocation_id: Option<&str>,
    ) -> Result<(), String> {
        if let ByokBodyPlan::Random { ctx } = plan {
            if let Some(mode_b) = self.byok_mode_b.as_ref() {
                return match allocation_id {
                    Some(allocation_id) => mode_b.reclaim_for_allocation(ctx, allocation_id).await,
                    None => mode_b.reclaim(ctx).await,
                };
            }
        }
        Ok(())
    }

    /// BYOK write hook (test-facing): resolve + encrypt the body. The production
    /// `write` path resolves ONCE and calls [`Self::encrypt_body`] directly.
    #[cfg(test)]
    async fn byok_encrypt_for_write(
        &self,
        req: &CasWriteRequest,
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        let resolved = self
            .resolve_byok(&req.tenant, &req.claimed_hash, req.algo)
            .await?;
        self.encrypt_body(&resolved.plan, &req.bytes, None).await
    }

    /// BYOK read hook (test-facing): resolve + decrypt the stored body. The
    /// production `read` path resolves ONCE and calls [`Self::decrypt_body`].
    #[cfg(test)]
    async fn byok_decrypt_for_read(
        &self,
        req: &CasReadRequest,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
        self.decrypt_body(&resolved.plan, stored, None).await
    }

    /// BYOK delete-reclaim hook (test-facing): resolve + reclaim the Mode-B
    /// `byok_envelope` row, mirroring the post-R2-delete step in the production
    /// `delete` path (which WARNS on the returned `Err` and never fails the
    /// delete). Returns the reclaim `Result` so a test can assert the safe-fail
    /// direction.
    #[cfg(test)]
    async fn byok_reclaim_for_delete(
        &self,
        tenant: &str,
        digest: &str,
        algo: DigestAlgo,
    ) -> Result<(), String> {
        let resolved = self
            .resolve_byok(tenant, digest, algo)
            .await
            .map_err(|e| format!("resolve: {e}"))?;
        self.reclaim_byok_envelope(&resolved.plan, None).await
    }
}
