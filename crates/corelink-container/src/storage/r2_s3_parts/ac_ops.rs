impl corelink_handler_ac::AcLookupHandler for R2AcHandler {
    fn lookup(
        &self,
        req: corelink_handler_ac::AcLookupRequest,
    ) -> Result<corelink_handler_ac::AcLookupResponse, corelink_handler_ac::AcHandlerError> {
        let started = Instant::now();
        use corelink_handler_ac::{
            AcHandlerError, AcLookupResponse, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::LookupDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(|e| {
                    self.emit_lookup_sli(true, elapsed_us(started));
                    AcHandlerError::AuditFailed(e)
                })?;
            self.emit_lookup_sli(true, elapsed_us(started));
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // LookupAttempted audit and the (resolve_byok -> R2 GET) chain —
        // AC counterpart of `R2CasHandler::read`'s concurrent seam; see
        // there for the full fail-CLOSED rationale (audit result checked
        // first, in the same order the old serial code checked it;
        // store-side error already `warn!`-logged at the point it is
        // produced). The SECOND audit write (`LookupHit`/`LookupMiss`)
        // stays exactly where it was — after the outcome is known, still
        // fully synchronous.
        //
        // CRITICAL — `block_in_place` rationale: this sync trait method
        // is invoked from inside an async axum handler on the tokio
        // multi-thread runtime; a bare `handle.block_on(future)` from
        // inside a running future on the SAME runtime hangs forever
        // (observed: 60s curl timeout in prod before this fix).
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(ByokResolved, Option<Vec<u8>>), AcHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH — see `R2CasHandler::read` for the full
                // rationale; this mirrors it exactly for the AC surface.
                let audit_fut = audit_async.emit_ac_async(AcAuditEvent::new(
                    AcAuditEventKind::LookupAttempted,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let resolved = self.resolve_byok(&req.tenant, &req.action_digest).await?;
                    let key = self
                        .r2_key(&req.tenant, &resolved.physical_digest)
                        .map_err(AcHandlerError::Internal)?;
                    debug!(key = %key, "R2AcHandler::lookup");
                    let got = self.client.get(&key).await.map_err(|e| {
                        warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                        AcHandlerError::Internal(e)
                    })?;
                    Ok((resolved, got))
                };
                // ONE `Phase::Store` scope for the whole joined window —
                // see `R2CasHandler::list` for why (never double-count
                // against `oaudit`).
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior (every test handler in this module today).
                let audit_result = self.audit.emit(AcAuditEvent::new(
                    AcAuditEventKind::LookupAttempted,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching resolve_byok or
                    // R2. This filler is never read (audit_result is
                    // checked first, below).
                    Ok((
                        ByokResolved {
                            physical_digest: String::new(),
                            plan: ByokBodyPlan::Plaintext,
                        },
                        None,
                    ))
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            self.emit_lookup_sli(true, elapsed_us(started));
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
                        Ok(k) => k,
                        Err(e) => {
                            self.emit_lookup_sli(true, elapsed_us(started));
                            return Err(AcHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2AcHandler::lookup");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
                    };
                    match result {
                        Ok(got) => Ok((resolved, got)),
                        Err(e) => {
                            warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                            self.emit_lookup_sli(true, elapsed_us(started));
                            return Err(AcHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            self.emit_lookup_sli(true, elapsed_us(started));
            return Err(AcHandlerError::AuditFailed(e));
        }

        let (resolved, stored_opt) = match store_result {
            Ok(ok) => ok,
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                self.emit_lookup_sli(true, elapsed_us(started));
                return Err(e);
            }
        };

        match stored_opt {
            Some(bytes) => {
                // BYOK Wave 3b/3c (GATED-INERT): for an `active` tenant decrypt the
                // stored blob to plaintext before serving (surface `"ac"`).
                // FAIL-CLOSED: any decrypt/unwrap failure returns Err — the raw
                // stored bytes are NEVER served. `Plaintext`-plan tenants get
                // their bytes back unchanged (byte-identical to today).
                let bytes = match tokio::task::block_in_place(|| {
                    handle.block_on(self.decrypt_body(&resolved.plan, bytes))
                }) {
                    Ok(pt) => pt,
                    Err(e) => {
                        self.emit_lookup_sli(true, elapsed_us(started));
                        return Err(e);
                    }
                };
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::LookupHit,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(|e| {
                        self.emit_lookup_sli(true, elapsed_us(started));
                        AcHandlerError::AuditFailed(e)
                    })?;
                self.emit_lookup_sli(false, elapsed_us(started));
                Ok(AcLookupResponse::new(req.action_digest, bytes))
            }
            None => {
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::LookupMiss,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(|e| {
                        self.emit_lookup_sli(true, elapsed_us(started));
                        AcHandlerError::AuditFailed(e)
                    })?;
                // Miss is NOT an availability error — handler served
                // correctly (mirrors `InMemoryAcHandler::lookup`).
                self.emit_lookup_sli(false, elapsed_us(started));
                Err(AcHandlerError::Miss {
                    tenant: req.tenant,
                    action_digest: req.action_digest,
                })
            }
        }
    }
}

impl corelink_handler_ac::AcUpdateHandler for R2AcHandler {
    fn update(
        &self,
        req: corelink_handler_ac::AcUpdateRequest,
    ) -> Result<corelink_handler_ac::AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
        let started = Instant::now();
        use corelink_handler_ac::{
            AcHandlerError, AcUpdateResponse, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::UpdateDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(|e| {
                    self.emit_update_sli(true, elapsed_us(started));
                    AcHandlerError::AuditFailed(e)
                })?;
            self.emit_update_sli(true, elapsed_us(started));
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // UpdateAttempted audit BEFORE mutation.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::UpdateAttempted,
                req.tenant.clone(),
                req.action_digest.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(|e| {
                self.emit_update_sli(true, elapsed_us(started));
                AcHandlerError::AuditFailed(e)
            })?;

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<R2AcHandler as AcLookupHandler>::lookup` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — §4-hardened physical key (audit H-4) + the
        // body crypto plan. Fail CLOSED for an active-but-unresolvable tenant.
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
        }) {
            Ok(r) => r,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(
            key = %key,
            bytes = req.result_payload.len(),
            "R2AcHandler::update"
        );

        // BYOK Wave 3b/3c (GATED-INERT): compute the bytes we WOULD store — the
        // ciphertext for an `active` tenant (surface `"ac"`), else the plaintext
        // payload. Encrypting BEFORE the divergent-body compare is LOAD-BEARING:
        // for an active tenant the stored prior is ciphertext and the encryption
        // is deterministic (convergent CLB1, or Mode-B CLB2 under the persisted
        // per-(tenant,action_digest) DEK), so comparing the prior against the
        // would-be-stored CIPHERTEXT keeps the immutability + idempotency contract
        // exact (an identical payload re-PUT is byte-identical → no-op; a divergent
        // payload yields divergent ciphertext → DivergentBody). FAIL-CLOSED: an
        // active tenant whose encryptor/KMS/Tcs/envelope-store is unavailable
        // returns Err here — plaintext is NEVER PUT for an active tenant. The
        // non-BYOK path computes nothing (`None`) and stores `req.result_payload`
        // verbatim, byte-identical to today.
        let encrypted: Option<Vec<u8>> = match tokio::task::block_in_place(|| {
            handle.block_on(self.encrypt_body(&resolved.plan, &req.result_payload))
        }) {
            Ok(maybe_ct) => maybe_ct,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(e);
            }
        };

        // AC IMMUTABILITY INVARIANT (F5): the Action Cache maps an
        // `action_digest` (hash of the build *action*, not its result)
        // to a result payload. AC bytes are therefore NOT
        // self-verifying — a divergent re-PUT must be REFUSED, never
        // silently overwritten, or any write-capable token can poison a
        // proven cache result for every subsequent build that hits the
        // same digest (supply-chain compromise).
        //
        // GET-and-compare BEFORE any PUT (mirrors
        // `InMemoryAcHandler::update`), against the WOULD-BE-STORED bytes
        // (`stored_view`: ciphertext for an active tenant, else the plaintext):
        //   - existing != stored_view → `DivergentBody` (409); NO PUT.
        //   - existing == stored_view → idempotent no-op (`durable=false`).
        //   - absent                  → PUT (`durable=true`).
        //   - ambiguous GET error     → fail CLOSED (`Internal`); never
        //     blind-overwrite on an unknown prior state.
        // Owned copy: the compare view must outlive the move of
        // `encrypted`/`result_payload` into the conditional put, because a
        // lost race re-compares against it.
        let stored_view: Vec<u8> = encrypted
            .as_deref()
            .unwrap_or(req.result_payload.as_slice())
            .to_vec();
        let existing = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
        };
        let pre_put_state = match existing {
            Ok(prior) => classify_prior(prior.as_deref(), &stored_view),
            Err(e) => {
                // Ambiguous prior state: fail closed rather than risk a
                // blind overwrite of a proven result.
                warn!(error = %e, key = %key, "R2AcHandler::update pre-PUT GET error");
                self.emit_update_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
            }
        };
        match pre_put_state {
            PriorState::Divergent => {
                warn!(
                    key = %key,
                    "R2AcHandler::update divergent body — refusing to overwrite a \
                     proven AC result (INV-AC-RESULT-HASH-IMMUTABLE)"
                );
                self.emit_update_sli(false, elapsed_us(started));
                return Err(AcHandlerError::DivergentBody {
                    tenant: req.tenant,
                    action_digest: req.action_digest,
                });
            }
            PriorState::Identical => {
                // Byte-identical re-PUT → idempotent no-op. The proven
                // bytes are already durable; do not re-PUT.
                self.emit_update_sli(false, elapsed_us(started));
                return Ok(AcUpdateResponse::new(req.action_digest, false));
            }
            PriorState::Absent => { /* absent — fall through to the PUT below */ }
        }

        // Store the ciphertext (active) or the plaintext payload (non-BYOK) —
        // the same bytes the compare above proved are non-divergent.
        //
        // CONDITIONAL PUT (WP-C): the GET-compare above is NOT atomic with
        // the write. Two concurrent writers with divergent bodies can both
        // observe `Ok(None)` and both fire an unconditional `put` —
        // last-write-wins over a PROVEN result (intra-tenant AC poisoning
        // window). `put_if_absent` closes it server-side (`If-None-Match: *`):
        //   - Ok(true)  → this caller created the object (durable=true).
        //   - Ok(false) → another writer won the race. Re-GET and apply the
        //     SAME classifier as the pre-PUT compare: identical bytes →
        //     idempotent no-op (durable=false); divergent → `DivergentBody`.
        //     The winner's bytes are NEVER overwritten on this path.
        //   - Err       → fail CLOSED (same as the pre-PUT GET error arm).
        let payload = encrypted.unwrap_or(req.result_payload);
        let created = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| {
                handle.block_on(self.client.put_if_absent(&key, payload))
            })
        };

        match created {
            Ok(true) => {
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::UpdateCommitted,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(|e| {
                        self.emit_update_sli(true, elapsed_us(started));
                        AcHandlerError::AuditFailed(e)
                    })?;
                self.emit_update_sli(false, elapsed_us(started));
                // This caller created the object → durable=true.
                Ok(AcUpdateResponse::new(req.action_digest, true))
            }
            Ok(false) => {
                // Lost the race: someone else's bytes are (or were being)
                // stored under this digest. Re-GET and compare against OUR
                // would-be-stored view.
                let prior = {
                    let _scope =
                        crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                    tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
                };
                let lost_race_state = match prior {
                    Ok(bytes) => classify_prior(bytes.as_deref(), &stored_view),
                    Err(e) => {
                        // Ambiguous post-race state: fail closed rather
                        // than report success on bytes we cannot prove.
                        warn!(error = %e, key = %key, "R2AcHandler::update lost-race re-GET error");
                        self.emit_update_sli(true, elapsed_us(started));
                        return Err(AcHandlerError::Internal(e));
                    }
                };
                match lost_race_state {
                    PriorState::Identical => {
                        self.emit_update_sli(false, elapsed_us(started));
                        Ok(AcUpdateResponse::new(req.action_digest, false))
                    }
                    PriorState::Divergent => {
                        warn!(
                            key = %key,
                            "R2AcHandler::update lost conditional-PUT race with a DIVERGENT \
                             body — preserving the proven AC result \
                             (INV-AC-RESULT-HASH-IMMUTABLE)"
                        );
                        self.emit_update_sli(false, elapsed_us(started));
                        Err(AcHandlerError::DivergentBody {
                            tenant: req.tenant,
                            action_digest: req.action_digest,
                        })
                    }
                    PriorState::Absent => {
                        // We lost a conditional put yet the object reads
                        // absent — ambiguous state (concurrent delete?).
                        // Fail closed rather than report success on bytes we
                        // cannot prove are stored.
                        warn!(key = %key, "R2AcHandler::update lost-race re-GET absent");
                        self.emit_update_sli(true, elapsed_us(started));
                        Err(AcHandlerError::Internal(
                            "conditional-PUT lost the race but re-GET found the key absent"
                                .to_owned(),
                        ))
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2AcHandler::update error");
                self.emit_update_sli(true, elapsed_us(started));
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

impl R2AcHandler {
    /// The tenant-scoped LIST prefix for AC refs:
    /// `<region>/<tenant_prefix>/`. Same isolation guarantee as
    /// [`R2CasHandler::r2_list_prefix`]; enumeration / delete never
    /// widen beyond this tenant's derived prefix.
    fn r2_list_prefix(&self, tenant: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(format!("{}/{}/", self.ac_region, prefix))
    }
}

/// The relationship between an EXISTING stored object and the bytes a
/// caller would store (`stored_view`): the AC immutability decision space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriorState {
    /// No object stored — the caller may create it.
    Absent,
    /// Stored bytes are byte-identical to the would-be-stored view — an
    /// idempotent no-op (durable=false).
    Identical,
    /// Stored bytes diverge — refuse with `DivergentBody` (409).
    Divergent,
}

/// Classify a prior GET result against the bytes that would be stored.
///
/// Shared by the pre-PUT compare AND the lost-race re-GET after a failed
/// conditional put, so both sites can never diverge in their decision.
fn classify_prior(existing: Option<&[u8]>, stored_view: &[u8]) -> PriorState {
    match existing {
        None => PriorState::Absent,
        Some(prior) if prior == stored_view => PriorState::Identical,
        Some(_) => PriorState::Divergent,
    }
}

impl corelink_handler_ac::AcDeleteHandler for R2AcHandler {
    fn delete(
        &self,
        req: corelink_handler_ac::AcDeleteRequest,
    ) -> Result<corelink_handler_ac::AcDeleteResponse, corelink_handler_ac::AcHandlerError> {
        let started = Instant::now();
        use corelink_handler_ac::{
            AcDeleteResponse, AcHandlerError, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::DeleteDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(|e| {
                    self.emit_update_sli(true, elapsed_us(started));
                    AcHandlerError::AuditFailed(e)
                })?;
            self.emit_update_sli(true, elapsed_us(started));
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::DeleteAttempted,
                req.tenant.clone(),
                req.action_digest.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(|e| {
                self.emit_update_sli(true, elapsed_us(started));
                AcHandlerError::AuditFailed(e)
            })?;

        // Byte-accounting (finding #1 / cluster-C) + concurrent double-DELETE
        // over-release (rt-nuclear #6/#10/#14): `delete_if_present` serializes the
        // measure-and-delete per key and returns the reclaimed size to AT MOST ONE
        // racer, so two racing deletes can never both credit the same bytes.
        // CRITICAL — `block_in_place`.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `update`/`lookup` stored.
        // Fail CLOSED for an active-but-unresolvable tenant. BYOK Wave 4a: the
        // resolved plan also drives the Mode-B `byok_envelope` reclaim performed
        // AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
        }) {
            Ok(r) => r,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2AcHandler::delete");

        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                // BYOK Wave 4a: the R2 object is gone — reclaim the Mode-B
                // `byok_envelope` row (surface = `ac`) so a deleted AC entry does
                // not leave an orphan wrapped-DEK row. WARN + continue on failure
                // (safe-fail direction); the delete still SUCCEEDS.
                if let Err(e) = tokio::task::block_in_place(|| {
                    handle.block_on(self.reclaim_byok_envelope(&resolved.plan))
                }) {
                    warn!(
                        error = %e, key = %key, tenant = %req.tenant,
                        "R2AcHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; entry already deleted)"
                    );
                }
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::DeleteCommitted,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(|e| {
                        self.emit_update_sli(true, elapsed_us(started));
                        AcHandlerError::AuditFailed(e)
                    })?;
                self.emit_update_sli(false, elapsed_us(started));
                Ok(AcDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
            }
            Err(e) => {
                // Fail CLOSED on a storage fault: never a silent success.
                warn!(error = %e, key = %key, "R2AcHandler::delete error");
                self.emit_update_sli(true, elapsed_us(started));
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

impl corelink_handler_ac::AcListHandler for R2AcHandler {
    fn list(
        &self,
        req: corelink_handler_ac::AcListRequest,
    ) -> Result<corelink_handler_ac::AcListResponse, corelink_handler_ac::AcHandlerError> {
        let started = Instant::now();
        use corelink_handler_ac::{
            AcHandlerError, AcListResponse, AcRefEntry, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::ListDenied,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(|e| {
                    self.emit_list_sli(true, elapsed_us(started));
                    AcHandlerError::AuditFailed(e)
                })?;
            self.emit_list_sli(true, elapsed_us(started));
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // ListAttempted audit and the R2 enumeration — AC counterpart of
        // `R2CasHandler::list`'s concurrent seam; see that doc for the full
        // fail-CLOSED rationale (audit result checked first, in the same
        // order as the old serial code; store-side error already
        // `warn!`-logged at the point it is produced).
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(Vec<(String, u64, String)>, Option<String>), AcHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                // See `R2CasHandler::list` for the full rationale — this
                // mirrors it exactly for the AC surface.
                let audit_fut = audit_async.emit_ac_async(AcAuditEvent::new(
                    AcAuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let prefix = self
                        .r2_list_prefix(&req.tenant)
                        .map_err(AcHandlerError::Internal)?;
                    debug!(prefix = %prefix, "R2AcHandler::list");
                    self.client
                        .list_objects_page(&prefix, req.limit, req.cursor.as_deref())
                        .await
                        .map_err(|e| {
                            warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                            AcHandlerError::Internal(e)
                        })
                };
                // ONE `Phase::Store` scope for the whole joined window — see
                // `R2CasHandler::list` for why (never double-count against
                // `oaudit`).
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior (every test handler in this module today).
                let audit_result = self.audit.emit(AcAuditEvent::new(
                    AcAuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result = if audit_result.is_err() {
                    Ok((Vec::new(), None))
                } else {
                    let prefix = match self.r2_list_prefix(&req.tenant) {
                        Ok(p) => p,
                        Err(e) => {
                            self.emit_list_sli(true, elapsed_us(started));
                            return Err(AcHandlerError::Internal(e));
                        }
                    };
                    debug!(prefix = %prefix, "R2AcHandler::list");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| {
                            handle.block_on(self.client.list_objects_page(
                                &prefix,
                                req.limit,
                                req.cursor.as_deref(),
                            ))
                        })
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                            self.emit_list_sli(true, elapsed_us(started));
                            return Err(AcHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            self.emit_list_sli(true, elapsed_us(started));
            return Err(AcHandlerError::AuditFailed(e));
        }

        match store_result {
            Ok((rows, next_cursor)) => {
                let refs = rows
                    .into_iter()
                    .map(|(key, size, last_modified)| {
                        let ref_key = key.rsplit('/').next().unwrap_or(&key).to_owned();
                        AcRefEntry::new(ref_key, last_modified, size)
                    })
                    .collect();
                self.emit_list_sli(false, elapsed_us(started));
                Ok(AcListResponse::new(refs, next_cursor))
            }
            Err(e) => {
                // Already `warn!`-logged (with prefix, where available) at
                // the point the error was produced above.
                self.emit_list_sli(true, elapsed_us(started));
                Err(e)
            }
        }
    }
}

/// Build an `R2AcHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryAcHandler`.
///
/// Mirrors [`build_r2_cas_handler_from_env`] exactly; only the bucket
/// + region defaults and the handler type differ.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_ac_handler_from_env(
    bucket: &str,
    ac_region: &str,
) -> Option<Result<R2AcHandler, String>> {
    let env = super::StorageEnv::from_env()?;
    // FAIL CLOSED: see `build_r2_cas_handler_from_env`. The AC key space
    // is NOT content-addressed (AC bytes are not self-verifying), so a
    // predictable-prefix collision is even more dangerous here (F1/F5):
    // a same-ms prefix collision lets one tenant poison another's
    // ActionResult. The secret TDK is mandatory on the production path.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 AC handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: DURABLE audit trail is mandatory on the
    // production data plane — see `build_r2_cas_handler_from_env`. The AC key
    // space is not content-addressed, so a lost/forged audit row is even more
    // dangerous. Wire the D1 `audit_outbox` sink or REFUSE (route mounts the
    // fail-CLOSED handler, never a volatile in-memory fallback).
    // Kept concrete for `with_async_audit` too — see the matching comment in
    // `build_r2_cas_handler_from_env`.
    let audit_concrete = match ac_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable AC audit sink unavailable with storage creds present; \
                 refusing to mount the R2 AC handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn corelink_handler_ac::AuditSink> = audit_concrete.clone();
    // B-057, AC twin of the CAS builder above: constant-memory aggregate
    // instead of a Vec that grew for the life of the container.
    let sli = crate::sli_aggregate::shared();
    Some(Ok(R2AcHandler::new(
        client,
        ac_region,
        Some(tdk_bytes),
        audit,
        sli,
    )
    .with_async_audit(audit_concrete)))
}

/// Load the tenant derivation key from `R2_TDK_HEX` env var (64 hex chars =
/// 32 bytes). Returns `None` when unset/invalid; on the production storage
/// path a `None` here makes `build_r2_*_handler_from_env` FAIL CLOSED (the
/// handler is not constructed and the route does not mount) rather than
/// degrade to a public-prefix scheme (F1/F2).
fn load_tdk_from_env() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        warn!(
            len = hex_str.len(),
            "R2_TDK_HEX has wrong length; ignoring TDK"
        );
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        warn!("R2_TDK_HEX is not valid hex; ignoring TDK");
        return None;
    }
    Some(bytes)
}
