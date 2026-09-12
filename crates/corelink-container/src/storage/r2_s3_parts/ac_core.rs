impl R2CasHandler {
    /// The tenant-scoped LIST prefix: `<region>/<tenant_prefix>/`. Every
    /// key returned under this prefix belongs to exactly this tenant
    /// (layer 5 of `INV-TENANT-ISOLATION`); the trailing slash bounds
    /// the prefix so one tenant's prefix can never be a prefix of
    /// another's. Enumeration / delete NEVER widen beyond this.
    fn r2_list_prefix(&self, tenant: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(format!("{}/{}/", self.cas_region, prefix))
    }
}

fn cas_catalog_next_cursor(
    rows: &[crate::storage::byok_generation_catalog::PublishedObject],
    limit: u32,
) -> Option<String> {
    (rows.len() == limit as usize)
        .then(|| rows.last().map(|row| row.logical_key.clone()))
        .flatten()
}

impl CasDeleteHandler for R2CasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::CasDeleteResponse;

        // Delete folds availability into the PUT (mutation) SLI bucket.
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
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::DeleteDenied,
                    req.tenant.clone(),
                    req.hash.clone(),
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

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::DeleteAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        let mut byok_guard = self.acquire_byok_data(&req.tenant, DataOperation::Delete)?;

        // CRITICAL — `block_in_place`: see `read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `write`/`read` stored. Non-BYOK
        // tenants resolve to the raw digest (unchanged); an active-but-unresolvable
        // tenant fails CLOSED rather than delete the wrong (or no) key. BYOK Wave
        // 4a: the resolved plan also drives the Mode-B `byok_envelope` reclaim
        // performed AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok_with_guard(
                &req.tenant,
                &req.hash,
                DigestAlgo::Blake3,
                byok_guard.as_ref(),
            ))
        }) {
            Ok(r) => r,
            Err(e) => {
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. DELETE is native-only
        // (the Bazel REAPI bridge exposes no delete surface), so the BLAKE3
        // keyspace applies.
        let mut purge_plan = if let Some(guard) = byok_guard.as_ref() {
            if guard
                .intent()
                .map_err(CasHandlerError::Internal)?
                .catalog_generation()
                .is_some()
            {
                tokio::task::block_in_place(|| {
                    handle.block_on(guard.gate().tombstone_catalog(
                        guard.intent()?,
                        crate::storage::byok_generation_catalog::ByokObjectKind::Cas,
                        &format!("blake3:{}", req.hash),
                    ))
                })
                .map_err(CasHandlerError::Internal)?
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
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::DeleteCommitted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            self.finish_byok_mutation(byok_guard.as_mut())?;
            return Ok(CasDeleteResponse::with_reclaimed(false, 0));
        }
        let allocation_id = purge_plan
            .as_ref()
            .map(|plan| plan.object.allocation_id.clone());
        let key = match purge_plan
            .as_ref()
            .map(|plan| Ok(plan.object.physical_key.clone()))
            .unwrap_or_else(|| {
                self.r2_key(&req.tenant, &resolved.physical_digest, DigestAlgo::Blake3)
            }) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::delete");

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
            .map_err(CasHandlerError::Internal)?;
        }

        // S3 DeleteObject is idempotent: deleting an absent key succeeds.
        // `existed` is best-effort (S3 does not report prior presence on a
        // plain DeleteObject); we report `true` on a clean delete so the
        // diagnostic is monotone, never a silent success on a transport
        // error. CRITICAL — `block_in_place`: see `read` above.
        //
        // Storage byte-accounting (finding #1 / cluster-C) + concurrent
        // double-DELETE over-release (rt-nuclear #6/#10/#14): the size measurement
        // and the delete are serialized per-key by `delete_if_present`, which
        // returns the reclaimed size to AT MOST ONE racer — every other racer sees
        // the object already gone and gets `None` (releases 0). This makes the
        // release reflect what THIS request actually removed, so two racing
        // deletes can never both credit the same bytes. CRITICAL — `block_in_place`:
        // see `read` above.
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
            .map_err(CasHandlerError::Internal)?;
        }

        if head_absent {
            let reclaimed = result.ok().flatten().unwrap_or(0);
            // BYOK Wave 4a: the R2 object is now gone — reclaim the Mode-B
            // `byok_envelope` row so the deleted blob's wrapped DEK does not
            // linger (orphan key-material). A durable purge plan keeps the
            // row at r2_absent and returns an error for retry when reclaim
            // fails; legacy non-catalog deletes retain the warn-and-continue
            // safe-fail behavior.
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
                    emit(true);
                    return Err(CasHandlerError::Internal(format!(
                        "BYOK envelope reclaim: {e}"
                    )));
                }
                warn!(
                    error = %e, key = %key, tenant = %req.tenant,
                    "R2CasHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; blob already deleted)"
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
                .map_err(CasHandlerError::Internal)?;
            }
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::DeleteCommitted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            self.finish_byok_mutation(byok_guard.as_mut())?;
            emit(false);
            Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
        } else {
            // Fail CLOSED on a storage fault: never a silent success.
            warn!(key = %key, "R2CasHandler::delete did not verify R2 absence");
            emit(true);
            Err(CasHandlerError::Internal(delete_error.unwrap_or_else(
                || "R2 delete HEAD verification failed".to_owned(),
            )))
        }
    }
}

impl CasListHandler for R2CasHandler {
    fn list(
        &self,
        req: corelink_handler_cas::CasListRequest,
    ) -> Result<corelink_handler_cas::CasListResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::{CasBlobEntry, CasListResponse};

        // List folds availability into the GET (read) SLI bucket.
        // B-057: the window the latency SLI reports. Started at handler
        // entry so it covers the same work the availability SLI counts.
        let started = Instant::now();
        let emit = |is_error: bool| {
            self.emit_sli(
                Sli::AvailCasGet,
                Sli::LatencyCasGetP99,
                is_error,
                elapsed_us(started),
            );
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::ListDenied,
                    req.tenant.clone(),
                    String::new(),
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

        // ListAttempted audit and the R2 enumeration.
        //
        // Enumeration is bounded to the tenant's derived prefix — cross-tenant
        // keys cannot appear in the result. Fail CLOSED if the prefix is not
        // derivable: an empty prefix would list a SHARED keyspace across every
        // non-derivable tenant. Both branches below log their own R2 error
        // (with whatever prefix they had, if any) in the storage branch.
        let handle = tokio::runtime::Handle::current();
        let audit_result = self.audit.emit(AuditEvent::new(
            AuditEventKind::ListAttempted,
            req.tenant.clone(),
            String::new(),
            req.principal.clone(),
            req.at_unix_ms,
        ));
        let mut byok_guard = if audit_result.is_ok() {
            self.acquire_byok_data(&req.tenant, DataOperation::Read)?
        } else {
            None
        };
        let store_result = if audit_result.is_err() {
            Ok((Vec::new(), None))
        } else if let Some(guard) = byok_guard.as_ref().filter(|guard| {
            guard
                .intent()
                .is_ok_and(|intent| intent.catalog_generation().is_some())
        }) {
            // Catalog cursors are opaque full logical identities. Keeping the
            // algorithm prefix is required to resume correctly after SHA-256
            // rows in the shared CAS catalog ordering.
            let after = req.cursor.as_deref();
            let rows = tokio::task::block_in_place(|| {
                handle.block_on(guard.gate().list_catalog(
                    guard.intent()?,
                    crate::storage::byok_generation_catalog::ByokObjectKind::Cas,
                    req.limit,
                    after,
                ))
            })
            .map_err(CasHandlerError::Internal)?;
            let next_cursor = cas_catalog_next_cursor(&rows, req.limit);
            Ok((
                rows.into_iter()
                    .map(|row| {
                        (
                            row.logical_key
                                .split_once(':')
                                .map_or(row.logical_key.as_str(), |(_, hash)| hash)
                                .to_owned(),
                            row.size_bytes,
                            row.published_at_ms.to_string(),
                        )
                    })
                    .collect(),
                next_cursor,
            ))
        } else {
            let prefix = match self.r2_list_prefix(&req.tenant) {
                Ok(p) => p,
                Err(e) => {
                    emit(true);
                    return Err(CasHandlerError::Internal(e));
                }
            };
            debug!(prefix = %prefix, "R2CasHandler::list");
            let result = {
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
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
                    warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                    emit(true);
                    return Err(CasHandlerError::Internal(e));
                }
            }
        };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        match store_result {
            Ok((rows, next_cursor)) => {
                self.validate_byok_return(byok_guard.as_mut())?;
                let blobs = rows
                    .into_iter()
                    .map(|(key, size, last_modified)| {
                        // Strip `<region>/<tenant_prefix>/` to recover the
                        // bare digest; never leak the storage key layout.
                        let hash = key.rsplit('/').next().unwrap_or(&key).to_owned();
                        CasBlobEntry::new(hash, size, last_modified)
                    })
                    .collect();
                emit(false);
                Ok(CasListResponse::new(blobs, next_cursor))
            }
            Err(e) => {
                // Already `warn!`-logged (with prefix, where one was
                // available) at the point the error was produced above.
                emit(true);
                Err(e)
            }
        }
    }
}
