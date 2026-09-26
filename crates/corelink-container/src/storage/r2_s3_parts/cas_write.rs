impl R2CasHandler {
    fn write_with_byok_operation_context(
        &self,
        req: CasWriteRequest,
        context: Option<&dyn corelink_handler_cas::CasWriteOperationContext>,
    ) -> Result<CasWriteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        // B-057: the window the latency SLI reports. Started at handler
        // entry so it covers the same work the availability SLI counts.
        let started = Instant::now();
        let emit = |is_error: bool| {
            self.emit_sli(
                Sli::AvailCasPut,
                Sli::LatencyCasPutP99,
                is_error,
                elapsed_us(started),
            );
        };

        // Cross-tenant denial — audit BEFORE returning.
        if !req.is_authorized_for_caller() {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::WriteDenied,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // WriteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::WriteAttempted,
                req.tenant.clone(),
                req.claimed_hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        // Content-addressing enforcement (INV-CAS-INTEGRITY):
        // the durable store MUST NOT persist bytes that do not hash to
        // the claimed digest — otherwise the CAS guarantee is a lie and
        // any client (or a buggy uploader) can poison the cache for
        // every subsequent reader of that digest. Verify BEFORE the
        // PUT; on mismatch emit `CorrectnessViolation` + a
        // `CorrectnessCas` SLI failure and reject with 422
        // `HashMismatch`. Nothing is written.
        if let Err(actual) = verify_content_hash(req.algo, &req.claimed_hash, &req.bytes) {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::CorrectnessViolation,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            self.sli
                .observe(SliObservation::new(Sli::CorrectnessCas, true, 0));
            emit(true);
            return Err(CasHandlerError::HashMismatch {
                claimed: req.claimed_hash,
                actual,
            });
        }
        let staging_context = match crate::storage::cas_write_fence::staging_admission_context(context) {
            Ok(context) => context,
            Err(error) => {
                emit(true);
                return Err(CasHandlerError::Internal(format!(
                    "CAS staging write context rejected: {error}"
                )));
            }
        };
        if staging_context.is_some()
            && (req.tenant == crate::adapter_cache::PUBLIC_NAMESPACE
                || self.cas_write_fence.is_none())
        {
            emit(true);
            return Err(CasHandlerError::Internal(
                "CAS staging ownership fence unavailable".to_owned(),
            ));
        }
        let mut byok_guard = self.acquire_byok_data(&req.tenant, DataOperation::Write, context)?;

        // B071: claim the D1 writer lease before resolving keys or touching
        // R2. GC acquisition excludes this lease, and the metadata commit
        // consumes it atomically. `_public` is a shared physical namespace,
        // not a tenant-owned `blob_meta` row, so assigning it to one caller
        // would leak GC ownership across tenants.
        let fence_lease = if req.tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            None
        } else if let Some(fence) = self.cas_write_fence.as_ref() {
            match fence.begin(
                &req.tenant,
                &canonical_meta_digest(&req.claimed_hash),
                req.at_unix_ms,
                staging_context,
            ) {
                Ok(lease) => Some(lease),
                Err(e) => {
                    emit(true);
                    return Err(CasHandlerError::Internal(format!(
                        "CAS write D1 fence: {e}"
                    )));
                }
            }
        } else {
            None
        };
        let abort_fence = |lease: &Option<crate::storage::cas_write_fence::CasWriteLease>| {
            if let (Some(fence), Some(lease)) = (self.cas_write_fence.as_ref(), lease) {
                if let Err(e) = fence.abort(lease) {
                    warn!(error = %e, "CAS write D1 fence abort failed; lease expiry will recover");
                }
            }
        };

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<Self as CasReadHandler>::read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — the §4-hardened physical key digest (audit
        // H-4) + the body crypto plan. Non-BYOK tenants resolve to the RAW digest
        // (byte-identical to today); an active-but-unresolvable tenant fails
        // CLOSED here (never PUTs plaintext).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok_with_guard(
                &req.tenant,
                &req.claimed_hash,
                req.algo,
                byok_guard.as_ref(),
            ))
        }) {
            Ok(r) => r,
            Err(e) => {
                abort_fence(&fence_lease);
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let logical_key = format!(
            "{}:{}",
            match req.algo {
                DigestAlgo::Blake3 => "blake3",
                DigestAlgo::Sha256 => "sha256",
            },
            req.claimed_hash
        );
        let catalog_generation = byok_guard
            .as_ref()
            .and_then(|guard| guard.intent().ok())
            .and_then(|intent| intent.catalog_generation());
        let (key, staged, envelope_allocation_id) = if let (Some(guard), Some(generation)) =
            (byok_guard.as_ref(), catalog_generation)
        {
            let catalog_result = tokio::task::block_in_place(|| {
                handle.block_on(guard.gate().resolve_catalog(
                    guard.intent()?,
                    crate::storage::byok_generation_catalog::ByokObjectKind::Cas,
                    &logical_key,
                ))
            });
            match catalog_result {
                Ok(Some(published)) => {
                    (published.physical_key, None, Some(published.allocation_id))
                }
                Ok(None) => {
                    let allocation_id = Uuid::new_v4().to_string();
                    let physical_digest =
                        crate::storage::byok_generation_catalog::generation_qualified_digest(
                            generation,
                            &allocation_id,
                            &resolved.physical_digest,
                        )
                        .map_err(CasHandlerError::Internal)?;
                    let physical_key = self
                        .r2_key(&req.tenant, &physical_digest, req.algo)
                        .map_err(CasHandlerError::Internal)?;
                    let staged = tokio::task::block_in_place(|| {
                        handle.block_on(guard.gate().allocate_catalog(
                            guard.intent()?,
                            crate::storage::byok_generation_catalog::ByokObjectKind::Cas,
                            &logical_key,
                            &allocation_id,
                            &physical_key,
                            req.bytes.len() as u64,
                        ))
                    })
                    .map_err(CasHandlerError::Internal)?;
                    (physical_key, Some(staged), Some(allocation_id))
                }
                Err(error) => return Err(CasHandlerError::Internal(error)),
            }
        } else {
            let legacy_key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
                Ok(k) => k,
                Err(e) => {
                    abort_fence(&fence_lease);
                    emit(true);
                    return Err(CasHandlerError::Internal(e));
                }
            };
            (legacy_key, None, None)
        };
        debug!(key = %key, bytes = req.bytes.len(), "R2CasHandler::write");
        let request_bytes_len = req.bytes.len() as u64;

        // Idempotent-rewrite detection (rt-nuclear #13 — byte double-charge).
        // CAS is content-addressed: the key already embeds the verified content
        // hash, so an object that already exists under this key holds the SAME
        // bytes (the hash was verified above). HEAD before PUT (mirrors the AC
        // path's GET-and-compare, but for CAS a presence HEAD suffices): if the
        // blob is already present we SKIP the re-PUT and return `durable=false`,
        // so the `AccountingCasHandler` decorator does NOT charge the bytes a
        // second time on an idempotent re-write. A HEAD error fails CLOSED to the
        // PUT path (correctness over the accounting optimisation — a transient
        // HEAD blip must never drop a write); a duplicate PUT is harmless (same
        // bytes) and the worst case is the legacy double-charge, never data loss.
        //
        // BYOK Wave 3c — the dedup HEAD-skip is gated to the convergent/plaintext
        // path ONLY. Mode B (random DEK) is deliberately NOT deduped (audit C2):
        // the §4-hardened key + random DEK + the `byok_envelope` idempotency
        // (reuse-the-row, deterministic ciphertext) own the no-orphan guarantee,
        // so a Mode-B write always proceeds to the PUT below.
        let dedup_eligible = !matches!(resolved.plan, ByokBodyPlan::Random { .. });
        if dedup_eligible {
            let head_result = {
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
            };
            match head_result {
                Ok(Some(_)) => {
                    // Already durable under this content-addressed key → idempotent
                    // no-op; do not re-PUT and do not re-charge the bytes.
                    if let (Some(fence), Some(lease)) =
                        (self.cas_write_fence.as_ref(), fence_lease.as_ref())
                    {
                        if let Err(e) = fence.commit(
                            lease,
                            request_bytes_len,
                            req.at_unix_ms,
                            staging_context,
                        ) {
                            abort_fence(&fence_lease);
                            emit(true);
                            return Err(CasHandlerError::Internal(format!(
                                "CAS write metadata commit: {e}"
                            )));
                        }
                        abort_fence(&fence_lease);
                    }
                    self.audit
                        .emit(AuditEvent::new(
                            AuditEventKind::WriteCommitted,
                            req.tenant.clone(),
                            req.claimed_hash.clone(),
                            req.principal.clone(),
                            req.at_unix_ms,
                        ))
                        .map_err(CasHandlerError::AuditFailed)?;
                    self.sli
                        .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                    self.finish_byok_mutation(byok_guard.as_mut())?;
                    emit(false);
                    return Ok(CasWriteResponse::new(req.claimed_hash, false));
                }
                Ok(None) => { /* absent — fall through to the PUT below */ }
                Err(e) => {
                    // Ambiguous prior state: fall through to the PUT (fail-CLOSED to
                    // a durable write) rather than risk dropping a fresh blob.
                    warn!(error = %e, key = %key, "R2CasHandler::write pre-PUT HEAD error; PUTting");
                }
            }
        }

        // BYOK Wave 3a/3c (GATED-INERT): for an `active` tenant, encrypt at rest
        // AFTER the plaintext integrity verify + the (gated) dedup HEAD check,
        // replacing the stored bytes with the ciphertext blob (convergent CLB1 or
        // Mode-B CLB2). FAIL-CLOSED: an active tenant whose encryptor/KMS/Tcs/
        // envelope-store is unavailable returns Err here — plaintext is NEVER PUT
        // for an active tenant. `None` ⇒ the plaintext path (req.bytes),
        // byte-identical to today for every non-BYOK tenant.
        let payload = match tokio::task::block_in_place(|| {
            handle.block_on(self.encrypt_body(
                &resolved.plan,
                &req.bytes,
                envelope_allocation_id.as_deref(),
            ))
        }) {
            Ok(Some(ciphertext)) => ciphertext,
            Ok(None) => req.bytes,
            Err(e) => {
                abort_fence(&fence_lease);
                emit(true);
                return Err(e);
            }
        };

        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            if let (Some(guard), Some(_)) = (byok_guard.as_ref(), staged.as_ref()) {
                tokio::task::block_in_place(|| {
                    handle.block_on(guard.validate_for_storage_dispatch())
                })
                .map_err(CasHandlerError::Internal)?;
            }
            tokio::task::block_in_place(|| {
                if dedup_eligible {
                    handle
                        .block_on(self.client.put_if_absent(&key, payload))
                        .map(|fresh| (fresh, ()))
                } else {
                    handle
                        .block_on(self.client.put(&key, payload))
                        .map(|()| (true, ()))
                }
            })
        };

        match result {
            Ok((r2_fresh, ())) => {
                if let (Some(guard), Some(staged)) = (byok_guard.as_ref(), staged.as_ref()) {
                    let published = tokio::task::block_in_place(|| {
                        handle.block_on(guard.gate().publish_catalog(guard.intent()?, staged))
                    })
                    .map_err(CasHandlerError::Internal)?;
                    if !published {
                        return Err(CasHandlerError::Internal(
                            "BYOK catalog rejected stale PUT; object left unreachable".to_owned(),
                        ));
                    }
                }
                let metadata_result = if let (Some(fence), Some(lease)) =
                    (self.cas_write_fence.as_ref(), fence_lease.as_ref())
                {
                    Some(fence.commit(
                        lease,
                        request_bytes_len,
                        req.at_unix_ms,
                        staging_context,
                    ))
                } else {
                    None
                };
                if let Some(Err(e)) = metadata_result {
                    // Mode B uses a blind PUT because every write has a fresh
                    // random envelope.  If D1 metadata loses the race, the
                    // object and envelope are intentionally preserved and a
                    // durable reconciliation intent is recorded; deleting
                    // either side here would make the outcome unrecoverable.
                    if let ByokBodyPlan::Random { ctx } = &resolved.plan {
                        if let Some(mode_b) = self.byok_mode_b.as_ref() {
                            if let Err(reconcile_error) = tokio::task::block_in_place(|| {
                                handle.block_on(mode_b.record_reconciliation_intent(
                                    ctx,
                                    &key,
                                    "mode-b metadata commit failed",
                                    req.at_unix_ms,
                                ))
                            }) {
                                warn!(
                                    error = %reconcile_error,
                                    "Mode-B metadata failure could not persist reconciliation intent"
                                );
                                abort_fence(&fence_lease);
                                emit(true);
                                return Err(CasHandlerError::Internal(format!(
                                    "CAS write metadata commit: {e}; reconciliation intent: {reconcile_error}"
                                )));
                            }
                        }
                    }
                    // Only a conditional PUT that returned `true` is ours to
                    // compensate. Blind Mode-B PUTs may have replaced an
                    // existing ciphertext and are therefore never deleted.
                    if dedup_eligible && r2_fresh {
                        let rollback = tokio::task::block_in_place(|| {
                            handle.block_on(self.client.delete(&key))
                        });
                        if let Err(rollback_error) = rollback {
                            warn!(error = %rollback_error, key = %key, "CAS write R2 compensation failed after D1 metadata rejection");
                        }
                    }
                    abort_fence(&fence_lease);
                    emit(true);
                    return Err(CasHandlerError::Internal(format!(
                        "CAS write metadata commit: {e}"
                    )));
                }
                abort_fence(&fence_lease);
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::WriteCommitted,
                        req.tenant.clone(),
                        req.claimed_hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                self.finish_byok_mutation(byok_guard.as_mut())?;
                emit(false);
                Ok(CasWriteResponse::new(req.claimed_hash, r2_fresh))
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2CasHandler::write error");
                abort_fence(&fence_lease);
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}

impl CasWriteHandler for R2CasHandler {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        self.write_with_byok_operation_context(req, None)
    }

    fn write_with_effect_and_context(
        &self,
        req: CasWriteRequest,
        context: Option<Arc<dyn corelink_handler_cas::CasWriteOperationContext>>,
    ) -> Result<CasWriteResponse, corelink_handler_cas::CasWriteFailure> {
        self.write_with_byok_operation_context(req, context.as_deref())
            .map_err(corelink_handler_cas::CasWriteFailure::unknown)
    }
}
