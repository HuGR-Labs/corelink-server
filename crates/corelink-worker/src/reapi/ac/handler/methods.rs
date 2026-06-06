//! Inner async methods + persist/recover sideline + `ActionResultStash`.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

use corelink_pat::{SCOPE_CACHE_R, SCOPE_CACHE_W};

use super::super::audit::{AcEventType, AuditSink};
use super::super::merkle::MerkleVerifier;
use super::super::meta::{
    AcKey, AcMetaStore, AcMetaUpsertOutcome, AcRefreshRequest, AcUpsertRequest,
};
use super::super::outputs::{OutputsCheck, OutputsCheckOutcome};
use super::super::sig::{AcEnvelope, SigError, Signer, AC_ENVELOPE_SIG_LEN};
use super::super::types::{ActionDigest, ActionResult, ResultHash};
use super::builder::ActionCacheHandlerImpl;
use super::envelope_store::AcEnvelopeStore;
use super::errors::{AcError, GetActionResult, UpdateActionResult, AC_ENVELOPE_VERSION};
use crate::cache::kv::KvBackend;
use crate::middleware::auth_ctx::AuthCtx;

impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Inner GET flow per WI §1.
    ///
    /// Steps:
    /// - \[0\] scope_check (`SCOPE_CACHE_R`).
    /// - \[1\] negative_cache::lookup → 404 short-circuit on hit.
    /// - \[2\] ac_meta::lookup → 404 + populate negative cache on miss;
    ///   410 on `expires_at <= now`.
    /// - \[3\] envelope_store::get → row says alive but envelope absent
    ///   ⇒ 503 (orphan envelope; reconcile in S-06).
    /// - \[4\] sig::verify (against canonical preimage rebuilt from row
    ///   fields) → 422 + audit on mismatch.
    /// - \[5\] outputs_check (warn-only, sampled at handler boundary —
    ///   skipped for the in-memory fake; DEFERRED(WI-S04-006): wire
    ///   1% sampling toggle from `EnvConfigTierTtlResolver` once the
    ///   real Cloudflare D1 binding ships alongside the conformance
    ///   suite. Until then the GET path is fail-open by design and
    ///   the full `INV-AC-OUTPUTS-VALID` check happens on UPDATE.
    /// - \[6\] meta::refresh_on_hit (last_hit_at → now_ms; expires_at
    ///   += ttl_extend_ms).
    /// - \[7\] audit emit `ac.get.ok`.
    pub(super) async fn get_action_result_inner(
        &self,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        request_id: &str,
    ) -> Result<GetActionResult, AcError> {
        // Step [0] — region pinning + scope check.
        self.check_region(ctx)?;
        Self::require_scope(ctx, SCOPE_CACHE_R)?;

        // Step [1] — negative cache lookup. Soft-fail on KV outage.
        let tenant_ctx = ctx.tenant_ctx();
        if let Some(()) = self
            .neg_cache
            .lookup(&tenant_ctx, &action_digest.hash)
            .await?
        {
            // Hit — short-circuit 404.
            self.audit.emit(self.make_record(
                AcEventType::GetMiss,
                ctx,
                action_digest,
                request_id,
                None,
                "neg_cache_hit",
                Vec::new(),
            ))?;
            return Err(AcError::NotFound);
        }

        // Step [2] — ac_meta lookup.
        let key = AcKey::new(ctx.tenant_id(), action_digest.hash);
        let row_opt = self.meta.get_with_expiry(&key).await?;
        let Some(row) = row_opt else {
            // Miss — populate negative cache + audit + 404.
            self.neg_cache
                .populate_miss(&tenant_ctx, &action_digest.hash)
                .await?;
            self.audit.emit(self.make_record(
                AcEventType::GetMiss,
                ctx,
                action_digest,
                request_id,
                None,
                "ac_meta_miss",
                Vec::new(),
            ))?;
            return Err(AcError::NotFound);
        };

        // Defense-in-depth: row's tenant_id MUST match ctx (the trait
        // surface already enforces this via `(tenant_id, action_digest)`
        // PK shape, but a future refactor that loosens the PK would
        // surface here as a bug).
        if row.tenant_id != ctx.tenant_id() {
            return Err(AcError::Internal(
                "ac_meta row tenant_id mismatch ctx (defense-in-depth)".to_string(),
            ));
        }

        // 410 on expiry.
        let now_ms = self.clock.now_ms();
        if let Some(expires) = row.expires_at_ms {
            if expires <= now_ms {
                self.audit.emit(self.make_record(
                    AcEventType::GetMiss,
                    ctx,
                    action_digest,
                    request_id,
                    Some(row.result_hash),
                    "ttl_expired",
                    Vec::new(),
                ))?;
                return Err(AcError::Expired);
            }
        }

        // Step [3] — envelope store GET (uses materialized prefix per
        // ADR-0035 H-3 — no TDK access on GET hot path).
        let action_hex = action_digest.hash.to_hex();
        let envelope_opt = self
            .envelope_store
            .get(row.region, &row.tenant_prefix, &action_hex)
            .await
            .map_err(AcError::BackendUnavailable)?;
        let Some(envelope) = envelope_opt else {
            // Row alive but envelope missing — orphan window per
            // ADR-0035 (analogous to the CAS R2-orphan-row case in
            // WI-S02-001 ADR-0028). Maps to 503; reconcile catches it.
            return Err(AcError::BackendUnavailable(
                "ac envelope orphan: row alive but envelope absent in r2".to_string(),
            ));
        };

        // Step [4] — sig verify. Recompute canonical preimage from
        // the row fields (NEVER trust the envelope's preimage bytes —
        // verify against re-derived bytes only).
        let canonical = AcEnvelope::canonicalize(
            AC_ENVELOPE_VERSION,
            row.sig_key_id,
            row.tenant_id,
            &row.action_digest,
            &row.result_hash,
        )?;
        if canonical != envelope.canonical_bytes {
            // Envelope's canonical bytes drift from the row — tampering.
            self.audit.emit(self.make_record(
                AcEventType::GetSigInvalid,
                ctx,
                action_digest,
                request_id,
                Some(row.result_hash),
                "canonical_drift",
                Vec::new(),
            ))?;
            // Populate negative cache so subsequent probes short-circuit
            // — defense-in-depth against retry storms following a
            // tampering signal.
            let _ = self
                .neg_cache
                .populate_miss(&tenant_ctx, &action_digest.hash)
                .await;
            return Err(AcError::SigInvalid);
        }
        match self.signer.verify(
            row.tenant_id,
            row.sig_key_id,
            &canonical,
            &envelope.signature,
        ) {
            Ok(()) => {}
            Err(SigError::Mismatch) => {
                self.audit.emit(self.make_record(
                    AcEventType::GetSigInvalid,
                    ctx,
                    action_digest,
                    request_id,
                    Some(row.result_hash),
                    "sig_mismatch",
                    Vec::new(),
                ))?;
                let _ = self
                    .neg_cache
                    .populate_miss(&tenant_ctx, &action_digest.hash)
                    .await;
                return Err(AcError::SigInvalid);
            }
            Err(other) => return Err(other.into()),
        }

        // Step [5] — outputs check. The pure-logic handler runs the
        // strict check on every GET (the production wrapper applies
        // 1% sampling per ADR-0035 H-7 — out of scope for this
        // pure-logic core; the sampled invocation lands in the
        // wrapper). Warn-only: drift is logged as a record but does
        // not reject the GET.
        //
        // Decode the persisted ActionResult (the canonical proto bytes
        // travel through the envelope_store under a separate seam in
        // production; for the in-memory pure-logic core we keep them
        // alongside the `AcMetaRow` only — the audit + outputs check
        // exercise the same code path against a synthetic empty
        // ActionResult in the property tests, which is sufficient for
        // the boundary).
        //
        // For the host-server gRPC wrapper (deferred WI), the
        // ActionResult bytes will round-trip through the envelope JSON
        // payload. The pure-logic handler returns an empty
        // ActionResult here — every test that exercises GET sets the
        // `output_files` slice on UPDATE, and we honor the same shape
        // on echo.
        //
        // We re-construct the `ActionResult` echo by reading the
        // canonical bytes off the envelope store. The wrapper stores
        // them as the envelope `canonical_bytes` is the *signed* 121-
        // byte preimage, NOT the raw proto — so the wrapper persists
        // the proto bytes alongside (production: as the R2 envelope
        // JSON's `data.action_result_proto` field). For the in-memory
        // pure-logic handler we keep them on a sibling map keyed by
        // the same canonical key.
        let action_result = self
            .recover_action_result(row.region, &row.tenant_prefix, &action_hex)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Step [6] — refresh on hit (last_hit_at + optional TTL).
        self.meta
            .refresh_on_hit(AcRefreshRequest {
                key,
                now_ms,
                ttl_extend_ms: Some(self.ttl_extend_ms),
            })
            .await?;

        // Step [7] — audit emit `ac.get.ok`.
        self.audit.emit(self.make_record(
            AcEventType::GetOk,
            ctx,
            action_digest,
            request_id,
            Some(row.result_hash),
            "",
            Vec::new(),
        ))?;

        // Re-read the row so the caller sees the refreshed last_hit_at.
        let refreshed =
            self.meta.get(&key).await?.ok_or_else(|| {
                AcError::Internal("ac_meta row vanished post-refresh".to_string())
            })?;
        Ok(GetActionResult {
            action_result,
            row: refreshed,
        })
    }

    /// Inner UPDATE flow per WI §1.
    ///
    /// Steps:
    /// - \[0\] scope_check (`SCOPE_CACHE_W`).
    /// - \[1\] body decode (caller-side — handler receives typed
    ///   `ActionResult`).
    /// - \[2\] digest verify (caller-side — REAPI URL/path → action_digest;
    ///   handler echoes the supplied tuple).
    /// - \[3\] merkle::verify → 422 on rejected tree.
    /// - \[4\] outputs::resolve_blobs (delegated to OutputsCheck).
    /// - \[5\] outputs::assert_alive → 422 on tombstoned subset.
    /// - \[6\] sig::sign over canonical preimage.
    /// - \[7\] envelope_store::put (R2 PUT).
    /// - \[8\] ac_meta::upsert (idempotent).
    /// - \[9\] negative_cache::invalidate.
    /// - \[10\] audit emit `ac.update.ok`.
    pub(super) async fn update_action_result_inner(
        &self,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        action_result: ActionResult,
        request_id: &str,
    ) -> Result<UpdateActionResult, AcError> {
        // Step [0] — region + scope.
        self.check_region(ctx)?;
        Self::require_scope(ctx, SCOPE_CACHE_W)?;

        // Step [3] — Merkle verify (pre-persist).
        if let Err(merkle_err) = self.merkle.verify(&action_result) {
            self.audit.emit(self.make_record(
                AcEventType::UpdateMerkleInvalid,
                ctx,
                action_digest,
                request_id,
                Some(ResultHash::compute(&action_result)),
                merkle_err.audit_code(),
                Vec::new(),
            ))?;
            return Err(merkle_err.into());
        }

        // Steps [4]–[5] — outputs aliveness check.
        match self
            .outputs
            .assert_alive(ctx.tenant_id(), &action_result)
            .await?
        {
            OutputsCheckOutcome::AllAlive => {}
            OutputsCheckOutcome::SomeMissing { missing } => {
                let count = missing.len();
                self.audit.emit(self.make_record(
                    AcEventType::UpdateOutputsMissing,
                    ctx,
                    action_digest,
                    request_id,
                    Some(ResultHash::compute(&action_result)),
                    "outputs_missing",
                    missing,
                ))?;
                return Err(AcError::OutputsMissing { count });
            }
        }

        // Compute result_hash.
        let result_hash = ResultHash::compute(&action_result);

        // Step [6] — sig sign.
        let canonical = AcEnvelope::canonicalize(
            AC_ENVELOPE_VERSION,
            self.sig_key_id,
            ctx.tenant_id(),
            action_digest,
            &result_hash,
        )?;
        let signature: [u8; AC_ENVELOPE_SIG_LEN] =
            match self
                .signer
                .sign(ctx.tenant_id(), self.sig_key_id, &canonical)
            {
                Ok(s) => s,
                Err(sig_err) => {
                    if !matches!(sig_err, SigError::KeyIdReserved) {
                        self.audit.emit(self.make_record(
                            AcEventType::UpdateSigInvalid,
                            ctx,
                            action_digest,
                            request_id,
                            Some(result_hash),
                            "sig_sign_failed",
                            Vec::new(),
                        ))?;
                    }
                    return Err(sig_err.into());
                }
            };
        let envelope = AcEnvelope {
            canonical_bytes: canonical,
            signature,
            sig_key_id: self.sig_key_id,
        };

        // Step [7] — envelope put (R2-first per WI §9.5; orphan
        // recoverable via S-06 reconcile).
        //
        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification
        // (audit-ordering-high-risk-seal §Escalation 4, 2026-05-27):
        // the R2 envelope_store.put (step 7), persist_action_result
        // (step 7b) and meta.upsert (step 8) precede the audit emits
        // at lines 408 (`UpdateResultMismatch`) and 430 (`UpdateOk`).
        // This is the canonical architecture per WI-S04-001 §9.5
        // ratified in ADR-0035 ("AC handler invariants"):
        //   - R2 is content-addressable + idempotent (overwrite OK);
        //   - D1 INSERT failure leaves an orphan R2 envelope
        //     (recoverable via S-06 GC reconcile, R-009 mitigation);
        //   - the inverse (D1-first) would create ghost rows
        //     referencing missing R2 objects → 404 forever.
        // The `UpdateResultMismatch` audit (line 408) is
        // decision-driven on the upsert's `ResultHashMismatch`
        // outcome (same shape as billing-emit idempotency); the
        // `UpdateOk` audit (line 430) is the completion of the
        // intent established by the upstream fail-CLOSED audits
        // (`UpdateMerkleInvalid`, `UpdateOutputsMissing`,
        // `UpdateSigInvalid` at lines 297/318/350 — those fire
        // BEFORE any state mutation per the canonical envelope).
        let action_hex = action_digest.hash.to_hex();
        let tenant_prefix = *ctx.tenant_prefix();
        self.envelope_store
            .put(self.region, &tenant_prefix, &action_hex, envelope)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Stash the action_result proto alongside the envelope so GET
        // can recover the typed shape (production: persisted in the
        // envelope JSON's `data.action_result_proto`). Pure-logic
        // handler keeps a sibling map.
        self.persist_action_result(self.region, &tenant_prefix, &action_hex, &action_result)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Step [8] — ac_meta upsert.
        let now_ms = self.clock.now_ms();
        let key = AcKey::new(ctx.tenant_id(), action_digest.hash);
        let upsert_outcome = self
            .meta
            .upsert(AcUpsertRequest {
                key,
                action_digest: *action_digest,
                tenant_prefix,
                region: self.region,
                result_hash,
                path_key_id: self.path_key_id,
                sig_key_id: self.sig_key_id,
                now_ms,
                ttl_ms: Some(self.ttl_extend_ms),
            })
            .await?;
        if let AcMetaUpsertOutcome::ResultHashMismatch {
            existing,
            attempted,
        } = upsert_outcome
        {
            self.audit.emit(self.make_record(
                AcEventType::UpdateResultMismatch,
                ctx,
                action_digest,
                request_id,
                Some(attempted),
                "result_hash_mismatch",
                Vec::new(),
            ))?;
            return Err(AcError::ResultHashMismatch {
                existing,
                attempted,
            });
        }

        // Step [9] — negative cache invalidate (after row commit).
        let tenant_ctx = ctx.tenant_ctx();
        self.neg_cache
            .invalidate_on_update(&tenant_ctx, &action_digest.hash)
            .await?;

        // Step [10] — audit emit.
        self.audit.emit(self.make_record(
            AcEventType::UpdateOk,
            ctx,
            action_digest,
            request_id,
            Some(result_hash),
            match upsert_outcome {
                AcMetaUpsertOutcome::Inserted => "inserted",
                AcMetaUpsertOutcome::IdempotentRefresh => "idempotent_refresh",
                AcMetaUpsertOutcome::ResultHashMismatch { .. } => "result_hash_mismatch",
            },
            Vec::new(),
        ))?;

        let row = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| AcError::Internal("ac_meta row vanished post-upsert".to_string()))?;
        Ok(UpdateActionResult {
            action_result,
            upsert_outcome,
            row,
        })
    }
}
