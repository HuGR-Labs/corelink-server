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
        let mut byok_guard = self.acquire_byok_data(&req.tenant, DataOperation::Delete, None)?;

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
        let mut purge_plan = if let Some(guard) = byok_guard.as_ref() {
            if guard
                .intent()
                .map_err(AcHandlerError::Internal)?
                .catalog_generation()
                .is_some()
            {
                tokio::task::block_in_place(|| {
                    handle.block_on(guard.gate().tombstone_catalog(
                        guard.intent()?,
                        crate::storage::byok_generation_catalog::ByokObjectKind::Ac,
                        &req.action_digest,
                    ))
                })
                .map_err(AcHandlerError::Internal)?
            } else {
                None
            }
        } else {
            None
        };
        if byok_guard.as_ref().is_some_and(|guard| {
            guard
                .intent()
                .is_ok_and(|intent| intent.catalog_generation().is_some())
        }) && purge_plan.is_none()
        {
            self.finish_byok_mutation(byok_guard.as_mut())?;
            return Ok(AcDeleteResponse::with_reclaimed(false, 0));
        }
        let allocation_id = purge_plan
            .as_ref()
            .map(|plan| plan.object.allocation_id.clone());
        let key = match purge_plan
            .as_ref()
            .map(|plan| Ok(plan.object.physical_key.clone()))
            .unwrap_or_else(|| self.r2_key(&req.tenant, &resolved.physical_digest))
        {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true, elapsed_us(started));
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2AcHandler::delete");

        if let Some(plan) = purge_plan.as_mut() {
            tokio::task::block_in_place(|| {
                handle.block_on(
                    byok_guard
                        .as_ref()
                        .ok_or_else(|| "BYOK purge plan exists without data guard".to_owned())?
                        .gate()
                        .begin_purge_attempt(plan),
                )
            })
            .map_err(AcHandlerError::Internal)?;
        }

        let already_absent = purge_plan.as_ref().is_some_and(|plan| plan.r2_absent);
        let result = if already_absent {
            Ok(None)
        } else {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        let delete_error = result.as_ref().err().cloned();
        let head_result = if already_absent {
            Ok(None)
        } else {
            tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
        };
        let head_absent = matches!(&head_result, Ok(None));
        if let Some(plan) = purge_plan.as_ref() {
            let ledger_error = head_result
                .as_ref()
                .err()
                .map(String::as_str)
                .or_else(|| (!head_absent).then_some("R2 object remains after delete"));
            tokio::task::block_in_place(|| {
                handle.block_on(
                    byok_guard
                        .as_ref()
                        .ok_or_else(|| "BYOK purge plan exists without data guard".to_owned())?
                        .gate()
                        .finish_purge_attempt(plan, head_absent, ledger_error, false),
                )
            })
            .map_err(AcHandlerError::Internal)?;
        }

        if head_absent {
            let reclaimed = result.ok().flatten().unwrap_or(0);
            // BYOK Wave 4a: the R2 object is gone — reclaim the Mode-B
            // `byok_envelope` row (surface = `ac`) so a deleted AC entry does
            // not leave an orphan wrapped-DEK row. A durable purge plan keeps
            // the row at r2_absent and returns an error for retry when reclaim
            // fails; legacy non-catalog deletes retain warn-and-continue.
            let reclaim_error = tokio::task::block_in_place(|| {
                handle.block_on(
                    self.reclaim_byok_envelope(&resolved.plan, allocation_id.as_deref()),
                )
            })
            .err();
            if let Some(e) = reclaim_error {
                if let Some(plan) = purge_plan.as_ref() {
                    let _ = tokio::task::block_in_place(|| {
                        handle.block_on(
                            byok_guard
                                .as_ref()
                                .ok_or_else(|| {
                                    "BYOK purge plan exists without data guard".to_owned()
                                })?
                                .gate()
                                .finish_purge_attempt(plan, true, Some(&e), false),
                        )
                    });
                    self.emit_update_sli(true, elapsed_us(started));
                    return Err(AcHandlerError::Internal(format!(
                        "BYOK envelope reclaim: {e}"
                    )));
                }
                warn!(
                    error = %e, key = %key, tenant = %req.tenant,
                    "R2AcHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; entry already deleted)"
                );
            }
            if let Some(plan) = purge_plan.as_ref() {
                tokio::task::block_in_place(|| {
                    handle.block_on(
                        byok_guard
                            .as_ref()
                            .ok_or_else(|| "BYOK purge plan exists without data guard".to_owned())?
                            .gate()
                            .finish_purge_attempt(plan, true, None, true),
                    )
                })
                .map_err(AcHandlerError::Internal)?;
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
            self.finish_byok_mutation(byok_guard.as_mut())?;
            self.emit_update_sli(false, elapsed_us(started));
            Ok(AcDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
        } else {
            // Fail CLOSED on a storage fault: never a silent success.
            warn!(key = %key, "R2AcHandler::delete did not verify R2 absence");
            self.emit_update_sli(true, elapsed_us(started));
            Err(AcHandlerError::Internal(delete_error.unwrap_or_else(
                || "R2 delete HEAD verification failed".to_owned(),
            )))
        }
    }
}
