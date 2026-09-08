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
