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
        let mut byok_guard = self.acquire_byok_data(&req.tenant, DataOperation::Write)?;

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<R2AcHandler as AcLookupHandler>::lookup` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — §4-hardened physical key (audit H-4) + the
        // body crypto plan. Fail CLOSED for an active-but-unresolvable tenant.
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok_with_guard(
                &req.tenant,
                &req.action_digest,
                byok_guard.as_ref(),
            ))
        }) {
            Ok(r) => r,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let catalog_generation = byok_guard
            .as_ref()
            .and_then(|guard| guard.intent().ok())
            .and_then(|intent| intent.catalog_generation());
        let (key, staged, envelope_allocation_id) =
            if let (Some(guard), Some(generation)) = (byok_guard.as_ref(), catalog_generation) {
                match tokio::task::block_in_place(|| {
                    handle.block_on(guard.gate().resolve_catalog(
                        guard.intent()?,
                        crate::storage::byok_generation_catalog::ByokObjectKind::Ac,
                        &req.action_digest,
                    ))
                }) {
                    Ok(Some(published)) => {
                        (published.physical_key, None, Some(published.allocation_id))
                    }
                    Ok(None) => {
                        let allocation_id = Uuid::new_v4().to_string();
                        let physical_key = self
                            .generation_r2_key(
                                &req.tenant,
                                generation,
                                &allocation_id,
                                &resolved.physical_digest,
                            )
                            .map_err(AcHandlerError::Internal)?;
                        let staged = tokio::task::block_in_place(|| {
                            handle.block_on(guard.gate().allocate_catalog(
                                guard.intent()?,
                                crate::storage::byok_generation_catalog::ByokObjectKind::Ac,
                                &req.action_digest,
                                &allocation_id,
                                &physical_key,
                                req.result_payload.len() as u64,
                            ))
                        })
                        .map_err(AcHandlerError::Internal)?;
                        (physical_key, Some(staged), Some(allocation_id))
                    }
                    Err(error) => return Err(AcHandlerError::Internal(error)),
                }
            } else {
                let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
                    Ok(k) => k,
                    Err(e) => {
                        self.emit_update_sli(true, elapsed_us(started));
                        return Err(AcHandlerError::Internal(e));
                    }
                };
                (key, None, None)
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
            handle.block_on(self.encrypt_body(
                &resolved.plan,
                &req.result_payload,
                envelope_allocation_id.as_deref(),
            ))
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
                self.finish_byok_mutation(byok_guard.as_mut())?;
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
            if let (Some(guard), Some(_)) = (byok_guard.as_ref(), staged.as_ref()) {
                tokio::task::block_in_place(|| {
                    handle.block_on(guard.validate_for_storage_dispatch())
                })
                .map_err(AcHandlerError::Internal)?;
            }
            tokio::task::block_in_place(|| {
                handle.block_on(self.client.put_if_absent(&key, payload))
            })
        };

        match created {
            Ok(true) => {
                if let (Some(guard), Some(staged)) = (byok_guard.as_ref(), staged.as_ref()) {
                    let published = tokio::task::block_in_place(|| {
                        handle.block_on(guard.gate().publish_catalog(guard.intent()?, staged))
                    })
                    .map_err(AcHandlerError::Internal)?;
                    if !published {
                        return Err(AcHandlerError::Internal(
                            "BYOK catalog rejected stale PUT; object left unreachable".to_owned(),
                        ));
                    }
                }
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
                self.finish_byok_mutation(byok_guard.as_mut())?;
                self.emit_update_sli(false, elapsed_us(started));
                // This caller created the object → durable=true.
                Ok(AcUpdateResponse::new(req.action_digest, true))
            }
            Ok(false) => {
                if staged.is_some() {
                    return Err(AcHandlerError::Internal("generation-qualified AC PUT unexpectedly collided; staged object remains unpublished".to_owned()));
                }
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
                        self.finish_byok_mutation(byok_guard.as_mut())?;
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
