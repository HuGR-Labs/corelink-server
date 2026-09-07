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
