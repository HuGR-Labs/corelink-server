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
