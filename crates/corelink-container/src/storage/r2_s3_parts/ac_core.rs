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

        // CRITICAL — `block_in_place`: see `read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `write`/`read` stored. Non-BYOK
        // tenants resolve to the raw digest (unchanged); an active-but-unresolvable
        // tenant fails CLOSED rather than delete the wrong (or no) key. BYOK Wave
        // 4a: the resolved plan also drives the Mode-B `byok_envelope` reclaim
        // performed AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.hash, DigestAlgo::Blake3))
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
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest, DigestAlgo::Blake3) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::delete");

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
        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                // BYOK Wave 4a: the R2 object is now gone — reclaim the Mode-B
                // `byok_envelope` row so the deleted blob's wrapped DEK does not
                // linger (orphan key-material). A reclaim failure is the SAFE-fail
                // direction (the DEK wraps nothing now), so WARN + continue — the
                // delete still SUCCEEDS and the R2 delete is NOT rolled back.
                if let Err(e) = tokio::task::block_in_place(|| {
                    handle.block_on(self.reclaim_byok_envelope(&resolved.plan))
                }) {
                    warn!(
                        error = %e, key = %key, tenant = %req.tenant,
                        "R2CasHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; blob already deleted)"
                    );
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
                emit(false);
                Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
            }
            Err(e) => {
                // Fail CLOSED on a storage fault: never a silent success.
                warn!(error = %e, key = %key, "R2CasHandler::delete error");
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
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
        // (with whatever prefix they had, if any) INSIDE the branch, so the
        // shared post-join code only has to deal in `CasHandlerError`.
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(Vec<(String, u64, String)>, Option<String>), CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                //
                // The mandatory `ListAttempted` audit write and the R2
                // `ListObjectsV2` call now run CONCURRENTLY — one
                // `tokio::join!` under a single `block_in_place` +
                // `block_on`, instead of two serial round trips (measured:
                // ~236ms total on IAD, ~30-36ms `ostore` + ~108-120ms
                // `oother` dominated by this blocking D1 INSERT — see the
                // PR).
                //
                // FAIL-CLOSED IS PRESERVED. The two calls are dispatched
                // together, but NEITHER result is used — and no bytes are
                // served — until BOTH have completed and are checked BELOW,
                // in the SAME order the old serial code checked them: the
                // audit result first (`AuditFailed` short-circuits exactly
                // as it did when the calls were serial), the store result
                // second. What changes is that a failing audit no longer
                // PREVENTS the R2 call from having been issued (it can no
                // longer prevent it — they started together); what does NOT
                // change is that a failing audit still prevents any R2
                // result from ever reaching the caller. "No bytes served
                // without the audit row" holds; "no R2 request issued
                // without the audit row" is the guarantee traded away for
                // the latency win, and it was never a stated invariant —
                // only "audit before enumeration" was, and enumeration
                // (returning rows to the caller) still cannot happen without
                // the audit row.
                //
                // Cross-tenant denial (above) is UNAFFECTED — it returns
                // before this point and never joins anything.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let prefix = self
                        .r2_list_prefix(&req.tenant)
                        .map_err(CasHandlerError::Internal)?;
                    debug!(prefix = %prefix, "R2CasHandler::list");
                    self.client
                        .list_objects_page(&prefix, req.limit, req.cursor.as_deref())
                        .await
                        .map_err(|e| {
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            CasHandlerError::Internal(e)
                        })
                };
                // ONE `Phase::Store` scope wraps the ENTIRE joined window —
                // NOT two separate scopes (one per future) — so the
                // overlapping wall time is attributed exactly once.
                // `oaudit` (`Phase::Audit`) is deliberately NOT entered
                // here: `append_async` (which this drives) skips its own
                // scope for exactly this reason. See `origin_timing.rs`'s
                // "Concurrent native-plane list seam" note — without this,
                // the two phases would double-count the same wall-clock
                // window and the `Σ(phases) ≤ total` invariant would break.
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior. Taken whenever `audit_async` is unset (every
                // test handler in this module, and any future `AuditSink`
                // impl that is not the durable D1 sink).
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching the prefix or
                    // R2 — never compute or evaluate the store side here.
                    Ok((Vec::new(), None))
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
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        match store_result {
            Ok((rows, next_cursor)) => {
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
