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

        // LookupAttempted audit and the (resolve_byok -> R2 GET) chain. The
        // durable attempted audit commits first; a failure prevents any
        // storage dispatch. The SECOND audit write (`LookupHit`/`LookupMiss`)
        // stays exactly where it was — after the outcome is known, still
        // fully synchronous.
        //
        // CRITICAL — `block_in_place` rationale: this sync trait method
        // is invoked from inside an async axum handler on the tokio
        // multi-thread runtime; a bare `handle.block_on(future)` from
        // inside a running future on the SAME runtime hangs forever
        // (observed: 60s curl timeout in prod before this fix).
        let handle = tokio::runtime::Handle::current();
        if let Err(e) = self.audit.emit(AcAuditEvent::new(
            AcAuditEventKind::LookupAttempted,
            req.tenant.clone(),
            req.action_digest.clone(),
            req.principal.clone(),
            req.at_unix_ms,
        )) {
            self.emit_lookup_sli(true, elapsed_us(started));
            return Err(AcHandlerError::AuditFailed(e));
        }

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
        let stored_opt = {
            let _scope = crate::origin_timing::PhaseScope::enter(
                crate::origin_timing::Phase::Store,
            );
            tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
        };
        let stored_opt = match stored_opt {
            Ok(got) => got,
            Err(e) => {
                warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                self.emit_lookup_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
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
