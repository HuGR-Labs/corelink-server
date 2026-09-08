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

        // ListAttempted audit and the R2 enumeration. The durable attempted
        // audit commits first; a failure prevents any storage dispatch.
        let handle = tokio::runtime::Handle::current();
        if let Err(e) = self.audit.emit(AcAuditEvent::new(
            AcAuditEventKind::ListAttempted,
            req.tenant.clone(),
            String::new(),
            req.principal.clone(),
            req.at_unix_ms,
        )) {
            self.emit_list_sli(true, elapsed_us(started));
            return Err(AcHandlerError::AuditFailed(e));
        }

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
        let (rows, next_cursor) = match result {
            Ok(ok) => ok,
            Err(e) => {
                warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                self.emit_list_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
            }
        };
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
}
