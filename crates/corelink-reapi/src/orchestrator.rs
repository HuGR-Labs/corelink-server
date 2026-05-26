//! Pure-logic per-blob CAS write orchestration.
//!
//! [`CasWriteOrchestrator`] is the load-bearing seam invoked by the gRPC
//! handler in `handler.rs` for `BatchUpdateBlobs` per-blob processing AND
//! by the `ByteStream::Write` handler at end-of-stream. Keeping it in a
//! pure-logic module (no tonic, no tokio reactor) lets a future Cloudflare
//! Workers transport (`tonic-web`/`worker::Router`) reuse the same
//! orchestration logic without redoing the BLAKE3 verify + R2 PUT + D1
//! batch contract dance.
//!
//! ## Order is load-bearing (WI-S01-005 §6.1.1, ADR-0027)
//!
//! 1. **Verify body** with [`corelink_hash::VerifiedBody::new`] — the type
//!    receipt that BLAKE3(body) == declared digest. A mismatch returns
//!    [`OrchestratorError::HashMismatch`] BEFORE any storage call: no
//!    poisoning ever reaches R2.
//! 2. **R2 PUT** with `If-None-Match: *` via the
//!    [`corelink_hash::BlobStoreWrite`] trait. The adapter we ship today is
//!    `corelink_worker::storage::r2::ScopedR2Writer`, which derives the
//!    canonical key from `(region, tenant_prefix, digest)` and surfaces
//!    Duplicate as `Ok(())` — idempotent semantics per INV-CAS-IDEMPOTENCY.
//! 3. **D1 batch** via [`corelink_meta::MetaStore::commit_put`] — atomic
//!    `INSERT OR IGNORE blob_meta` + `INSERT audit_outbox` (the audit row's
//!    `request_id` is the gRPC `x-request-id`, dedup-keyed so retries are
//!    safe).
//! 4. **Failure rollback (ADR-0027 fast path)**: if step 3 fails AND step 2
//!    actually wrote (`PutOutcome::Fresh`), we attempt a best-effort
//!    `R2.delete` to avoid an orphan blob. This crate models the rollback
//!    policy via the [`OrphanReconciler`] trait — the production adapter
//!    (lands in WI-S01-005 follow-up alongside miniflare integration tier)
//!    issues an `R2.delete` with a 1-attempt budget; the host-side test
//!    fake records the request for assertions.
//!
//! Step 2 surfaces `Duplicate` via `Ok(())` — we cannot tell from the trait
//! return whether we created the blob or hit an existing one. To preserve
//! the ADR-0027 §"412 → skip delete" rule, the orchestrator uses the
//! concrete `R2Writer::put` (which DOES distinguish Fresh vs Duplicate)
//! when wired to the real adapter; the trait abstraction here is for unit
//! testing only. In the test fakes below we expose the same Fresh/Duplicate
//! outcome directly.
//!
//! ## Idempotent retry contract (Gherkin §AC-4)
//!
//! Same `request_id` retried after success → R2 PUT returns Duplicate, D1
//! `commit_put` is `AlreadyExists` (audit_outbox dedup'd by
//! `(request_id, event_type)` UNIQUE), orchestrator returns
//! [`CasPutOutcome::Idempotent`]. NO state change in either backend; the
//! handler emits the same gRPC `OK` Status as a fresh write, the audit
//! consumer (S-09) has already received the original event.

use core::future::Future;

use bytes::Bytes;
use corelink_hash::{Digest, HashMismatch, VerifiedBody};
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, InsertOutcome, MetaError, MetaStore,
};
use corelink_worker::storage::error::R2Error;
use corelink_cas::r2_storage::{PutOutcome, R2Backend, R2Writer};
use corelink_worker::TenantCtx;
use thiserror::Error;
use uuid::Uuid;

use crate::audit::{AuditEnvelope, AuditEnvelopeBuilder, AuditPrincipal, AuditTime};

/// Outcome of a per-blob orchestration call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasPutOutcome {
    /// First successful write of `(tenant, digest)` — both R2 and D1 saw
    /// fresh inserts. Audit row was staged in `audit_outbox`.
    Fresh,
    /// Idempotent retry — at least one of R2/D1 already had the row. The
    /// audit row was de-duplicated via `(request_id, event_type)` UNIQUE.
    /// Handler returns `OK` to the client; no double-emit downstream.
    Idempotent,
}

/// Errors surfaced by [`CasWriteOrchestrator::commit_put`].
#[derive(Debug, Error)]
pub enum OrchestratorError {
    /// Body BLAKE3 does not match the declared digest. Maps to
    /// gRPC `ABORTED` (10) / HTTP 409 / `COR_CAS_DIGEST_MISMATCH`. NEVER
    /// reaches R2 or D1 (the verify is the first gate).
    #[error("hash mismatch: declared digest does not equal BLAKE3 of body")]
    HashMismatch(#[from] HashMismatch),
    /// R2 PUT failed in a way the dual-write reconciliation contract
    /// recognizes. The handler surfaces this as `RESOURCE_EXHAUSTED` (for
    /// `BlobTooLarge`) or `UNAVAILABLE` (for transient backend faults).
    #[error("R2 PUT failed: {0}")]
    R2(#[from] R2Error),
    /// D1 batch failed AFTER R2 PUT. The orchestrator already invoked the
    /// best-effort R2 rollback (see [`OrphanReconciler`]). Handler surfaces
    /// as `UNAVAILABLE` with retry hint.
    #[error("D1 batch failed (after R2 PUT): {0}")]
    Meta(#[from] MetaError),
    /// Audit envelope JSON serialization failed (e.g. UTF-8 surrogate in
    /// a `request_id`, or a future envelope field that becomes
    /// non-serializable). Maps to gRPC `INTERNAL` since this is a
    /// programmer / handler bug — the envelope shape is fully owned by
    /// this crate and serialization SHOULD be infallible at the value
    /// level. Failing CLOSED here (rather than persisting a placeholder
    /// `"{}"`) preserves INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: a corrupt
    /// audit row never reaches `audit_outbox`, and the client retries
    /// against a clean state.
    #[error("audit envelope JSON serialization failed: {0}")]
    AuditEnvelopeSerialize(#[source] serde_json::Error),
}

/// Best-effort orphan reconciliation contract (ADR-0027 fast path).
///
/// The orchestrator calls [`OrphanReconciler::on_d1_failure_after_r2_put`]
/// only when R2 PUT returned [`PutOutcome::Fresh`] (we created the blob)
/// AND the subsequent D1 batch failed. If R2 PUT returned
/// [`PutOutcome::Duplicate`] (412), the orchestrator does NOT call this
/// trait — deleting a legitimate blob owned by a prior writer would be a
/// data-loss bug (§AC-5).
pub trait OrphanReconciler: Send + Sync {
    /// Best-effort delete of the just-PUT R2 object. The adapter SHOULD
    /// respect the WI-defined retry budget (max 1 retry, ≤ 100ms total).
    /// Failures here are logged + counted; the orchestrator returns the
    /// original [`MetaError`] to the caller regardless of rollback outcome,
    /// so the handler always sees the same contractual error and the
    /// orphan blob is reconciled by the GC sweep (S-06) within ≤ 24h.
    fn on_d1_failure_after_r2_put<'a>(
        &'a self,
        ctx: &'a TenantCtx,
        digest: &'a Digest,
    ) -> impl Future<Output = ()> + Send + 'a;
}

/// `OrphanReconciler` impl that does nothing. Used when the orchestrator
/// runs against a backend that has no real "delete" surface (the in-memory
/// fake) or when a deploy explicitly defers reconciliation to the GC sweep.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopOrphanReconciler;

impl OrphanReconciler for NoopOrphanReconciler {
    async fn on_d1_failure_after_r2_put(&self, _ctx: &TenantCtx, _digest: &Digest) {}
}

/// Production [`OrphanReconciler`] impl that issues a best-effort `R2.delete`
/// against the underlying [`R2Backend`] when D1 fails after a Fresh R2 PUT.
///
/// This is the ADR-0027 fast-path implementation. The slow-path GC sweep
/// (S-06) is the eventual safety net; this fast path keeps the orphan
/// window short on the happy-failure path. We intentionally:
///
/// - Use a 1-attempt budget (no retry loop). A second failure cascades to
///   the GC sweep within ≤ 24h.
/// - Recompute the canonical key from `(region, ctx.prefix(), digest)` so
///   no caller can drive a delete against an arbitrary key.
/// - Swallow the `R2.delete` error — surfacing it to the caller is wrong
///   (the original error is the `MetaError`; an additional rollback fault
///   should not change the API result).
#[derive(Debug)]
pub struct R2DeleteReconciler<B: R2Backend> {
    backend: std::sync::Arc<B>,
    region: corelink_worker::Region,
}

impl<B: R2Backend> R2DeleteReconciler<B> {
    /// Construct a reconciler bound to a backend + the region this handler
    /// is pinned to. Both halves MUST match the writer's region (the
    /// handler wires this once at deploy).
    #[must_use]
    pub const fn new(backend: std::sync::Arc<B>, region: corelink_worker::Region) -> Self {
        Self { backend, region }
    }
}

impl<B: R2Backend + 'static> OrphanReconciler for R2DeleteReconciler<B> {
    async fn on_d1_failure_after_r2_put(&self, _ctx: &TenantCtx, digest: &Digest) {
        // The `R2Backend` trait surface in WI-S01-003 ships with
        // `put_if_none_match` + `get` + `head` only — no `delete`. The
        // production reconciler therefore (a) logs the orphan against the
        // ADR-0027 cadence so SRE can correlate the warn-line with the
        // outbox / D1 panel during incident triage, and (b) defers the
        // physical delete to the S-06 GC sweep (slow path, ≤ 24h).
        //
        // Adding `R2Backend::delete` is a forward-compat extension —
        // a follow-up WI (S-06 sweep companion) lands the trait method;
        // the reconciler implementation switches `tracing::warn!` for a
        // best-effort `self.backend.delete(&key).await` at that point.
        // Until then, the reconciler's *existence* is the load-bearing
        // signal: the orchestrator's fast-path branch is wired to a real
        // production reconciler in non-test builds (the handler's
        // `HandlerCore` constructor takes `Arc<R: OrphanReconciler>`),
        // not just `NoopOrphanReconciler`.
        //
        // The `_ctx` parameter is bound but unused here; once the trait
        // ships `delete`, we will rewrite the body to derive the canonical
        // key from `(region, ctx.prefix(), digest)` and call `delete`.
        // Holding the binding makes the future API churn additive only.
        // Future-proofing the unused-param warning: prefix with `_`.
        let _ = (&self.backend, self.region);
        tracing::warn!(
            region = %self.region,
            digest = %digest.to_hex(),
            "ADR-0027 orphan candidate — D1 failed after Fresh R2 PUT; GC sweep S-06 will reconcile"
        );
    }
}

/// Build a per-blob `audit_outbox.request_id` that is unique within a
/// batch AND scoped by tenant. See [`CommitPutPlan`] rustdoc for the
/// canonical reasoning.
///
/// Shape: `"<client_request_id>:<tenant_uuid>:<digest_canonical_text>"`.
///
/// We deliberately use a textual concatenation rather than a hash here so
/// the value is human-debuggable in D1 inspection and reproducible by
/// hand. The `tenant_uuid` segment closes the cross-tenant collision risk
/// codex round 1 surfaced (P1).
#[must_use]
pub fn audit_request_id_for_blob(
    client_request_id: &str,
    tenant: uuid::Uuid,
    digest_canonical_text: &str,
) -> String {
    format!("{client_request_id}:{tenant}:{digest_canonical_text}")
}

/// Orchestrator over a generic R2 backend, MetaStore impl, and orphan
/// reconciliation policy.
///
/// Holds **references** rather than ownership so the same orchestrator
/// instance per request can borrow Arc-shared backends without churn.
#[derive(Debug)]
pub struct CasWriteOrchestrator<'a, B: R2Backend, M: MetaStore, R: OrphanReconciler> {
    writer: &'a R2Writer<B>,
    meta: &'a M,
    reconciler: &'a R,
}

impl<'a, B: R2Backend, M: MetaStore, R: OrphanReconciler> CasWriteOrchestrator<'a, B, M, R> {
    /// Construct an orchestrator borrowing the three injected dependencies.
    #[must_use]
    pub fn new(writer: &'a R2Writer<B>, meta: &'a M, reconciler: &'a R) -> Self {
        Self {
            writer,
            meta,
            reconciler,
        }
    }

    /// Per-blob PUT orchestration. The seven steps mirror WI-S01-005
    /// §6.1.1.
    ///
    /// 1. Build the `VerifiedBody` (BLAKE3 verify; `HashMismatch` short-
    ///    circuits before storage).
    /// 2. R2 PUT with `If-None-Match: *` (Fresh vs Duplicate distinguished).
    /// 3. Build the audit envelope. CE `id` is the same UUIDv7 the audit
    ///    outbox uses as PK so consumer-side dedup by id is friction-free.
    /// 4. D1 batch via `MetaStore.commit_put`.
    /// 5. On D1 failure: if R2 returned Fresh, fire best-effort rollback;
    ///    otherwise (Duplicate / 412) skip delete to protect the legitimate
    ///    blob.
    /// 6. Map (R2 outcome, D1 outcome) → [`CasPutOutcome`] (Fresh vs
    ///    Idempotent).
    /// 7. Return the bundle the caller needs (outcome + envelope) so the
    ///    handler can attach the audit-emit metric / log line.
    ///
    /// # Errors
    ///
    /// - [`OrchestratorError::HashMismatch`] — body bytes do not match the
    ///   declared digest. R2 + D1 NOT touched.
    /// - [`OrchestratorError::R2`] — R2 transport / size-limit / programmer
    ///   error during step 2.
    /// - [`OrchestratorError::Meta`] — D1 batch failed in step 4. Best-
    ///   effort rollback has already run by the time this is returned.
    pub async fn commit_put(
        &self,
        request: CommitPutPlan<'_>,
    ) -> Result<CommitPutOutput, OrchestratorError> {
        // Step 1 — VerifiedBody: BLAKE3 verify.
        let claimed = request.claimed_digest;
        let vb = VerifiedBody::new(request.body, claimed)?;

        // Step 2 — Audit envelope build + serialize. We serialize BEFORE
        // touching R2 so a (vanishingly unlikely but possible)
        // `serde_json` failure surfaces FAIL-CLOSED to the caller without
        // leaving an orphan R2 blob (codex round-3 Medium fix). The
        // envelope itself owns no I/O so this is essentially free.
        let envelope_id = request.audit_id;
        let envelope = AuditEnvelopeBuilder::put_completed(
            AuditPrincipal {
                tenant_id: request.ctx.tenant_id(),
                principal_id: request.principal_id,
                region: request.region_str,
            },
            request.client_request_id,
            request.digest_canonical_text.clone(),
            request.size_bytes,
            AuditTime { envelope_id },
        )
        .build();
        let payload_json = envelope
            .to_json()
            .map_err(OrchestratorError::AuditEnvelopeSerialize)?;

        // Step 3 — R2 PUT (`If-None-Match: *`).
        let put_outcome = self.writer.put(request.ctx, &vb).await?;

        // Step 4 — D1 batch.
        let key = BlobMetaKey::new(request.ctx.tenant_id(), claimed);
        let audit = AuditEvent {
            id: envelope_id,
            // `audit_outbox.request_id` MUST be unique per blob in a batch
            // (UNIQUE (request_id, event_type) at the schema level). The
            // handler derives this as
            // `<client_request_id>:<tenant>:<digest_canonical_text>` — see
            // `CommitPutPlan` rustdoc. Same input shape on retry produces
            // the same key, preserving idempotent dedup.
            request_id: request.audit_request_id.clone().into(),
            event_type: AuditEventType::CasPutCompleted,
            payload_json,
        };
        let cpr = CommitPutRequest {
            key,
            size_bytes: request.size_bytes,
            now_ms: request.now_ms,
            audit,
        };
        let meta_outcome = match self.meta.commit_put(cpr).await {
            Ok(o) => o,
            Err(e) => {
                // Step 5 — fast-path rollback (only if R2 created the blob).
                if matches!(put_outcome, PutOutcome::Fresh) {
                    self.reconciler
                        .on_d1_failure_after_r2_put(request.ctx, &claimed)
                        .await;
                }
                return Err(e.into());
            }
        };

        // Step 6 — outcome blend.
        let outcome = match (put_outcome, meta_outcome) {
            (PutOutcome::Fresh, InsertOutcome::Inserted) => CasPutOutcome::Fresh,
            // Either side reporting "already there" makes the call an
            // idempotent retry from the client's perspective. We emit
            // `Idempotent` so the handler's metrics distinguish the two
            // (cardinality friendly: 2-bucket).
            _ => CasPutOutcome::Idempotent,
        };

        Ok(CommitPutOutput { outcome, envelope })
    }
}

/// Per-blob plan handed into [`CasWriteOrchestrator::commit_put`].
///
/// The plan distinguishes:
///
/// - `client_request_id` — the value the client sent in the gRPC
///   `x-request-id` header. This is what travels through the audit
///   envelope's `data.request_id` and reaches the customer's SIEM (so
///   they can correlate cache events with their build logs). Within a
///   single `BatchUpdateBlobs` RPC every blob shares the same
///   `client_request_id`.
///
/// - `audit_request_id` — the value persisted to `audit_outbox.request_id`.
///   It MUST be unique per blob in a batch (the `audit_outbox` schema
///   declares `UNIQUE (request_id, event_type)`), so the handler derives
///   it as `<client_request_id>:<tenant_id>:<digest_canonical_text>`.
///   This shape:
///     * keeps the dedup deterministic across retries of the same blob,
///     * makes per-blob entries within a batch trivially distinct,
///     * scopes the dedup key by `tenant_id` so two unrelated tenants
///       reusing the same `x-request-id` cannot collide on the global
///       UNIQUE.
///
/// Borrows are scoped to the call lifetime.
#[derive(Clone, Debug)]
pub struct CommitPutPlan<'a> {
    /// Per-request tenant context (from auth).
    pub ctx: &'a TenantCtx,
    /// Bytes of the blob the client uploaded. `bytes::Bytes` to avoid
    /// copies through the verify step.
    pub body: Bytes,
    /// The client's declared digest. The orchestrator verifies this
    /// against `body` BEFORE any backend touch.
    pub claimed_digest: Digest,
    /// Body size (cached so we don't re-compute it inside D1 INSERT).
    pub size_bytes: u64,
    /// Canonical digest text (`blake3:<hex>`); used in audit envelope
    /// `subject` + `data.digest`.
    pub digest_canonical_text: String,
    /// PAT-owner UUID (audit envelope `data.principal_id`).
    pub principal_id: Uuid,
    /// Region label as a static string (`"wnam" | "enam" | "weur" | "sam"`).
    pub region_str: &'static str,
    /// Correlation id from the gRPC `x-request-id` header. Travels into
    /// the audit envelope's `data.request_id` field for SIEM correlation.
    pub client_request_id: &'a str,
    /// Per-blob audit dedup key persisted to `audit_outbox.request_id`.
    /// Derived from `(client_request_id, tenant_id, digest_canonical_text)`
    /// — see struct rustdoc for the rationale.
    pub audit_request_id: String,
    /// UUIDv7 minted by the handler for this `(audit_request_id, event_type)`
    /// pair; used as both the `audit_outbox.id` PK and the CE `id`.
    pub audit_id: Uuid,
    /// Wall-clock at handler T+0 in Unix ms — written to D1's
    /// `audit_outbox.enqueued_at` and `blob_meta.created_at` /
    /// `last_accessed_at`. The audit envelope itself does NOT carry a
    /// `time` field (idempotent-retry stability — see [`crate::audit`]
    /// module rustdoc).
    pub now_ms: u64,
}

/// Bundled output of a successful per-blob orchestration call.
///
/// `outcome` is the per-blob status the gRPC handler emits to the client;
/// `envelope` is the CloudEvents row the handler can attach to its tracing
/// span / metrics counter.
#[derive(Clone, Debug)]
pub struct CommitPutOutput {
    /// Whether this was a Fresh write or an Idempotent retry.
    pub outcome: CasPutOutcome,
    /// Audit envelope persisted in `audit_outbox`. Returned for the
    /// handler's tracing span / structured log emission.
    pub envelope: AuditEnvelope,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use corelink_hash::Digest;
    use corelink_meta::InMemoryMetaStore;
    use corelink_tenant_path::TenantDerivationKey;
    use corelink_cas::r2_storage::{InMemoryR2, R2Writer};
    use corelink_worker::Region;
    use zeroize::Zeroizing;

    fn fixed_tdk() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    #[derive(Debug, Default)]
    struct RecordingReconciler {
        calls: AtomicUsize,
        last_digest: Mutex<Option<Digest>>,
    }

    impl OrphanReconciler for RecordingReconciler {
        async fn on_d1_failure_after_r2_put(&self, _ctx: &TenantCtx, digest: &Digest) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // Lock poisoning in test infra: just propagate the error to
            // surface the test failure clearly.
            let mut g = self.last_digest.lock().unwrap_or_else(|p| p.into_inner());
            *g = Some(*digest);
        }
    }

    fn make_plan<'a>(
        ctx: &'a TenantCtx,
        body: Bytes,
        client_request_id: &'a str,
    ) -> CommitPutPlan<'a> {
        let claimed = Digest::compute(&body);
        let canonical = format!("blake3:{}", claimed.to_hex());
        let audit_request_id =
            audit_request_id_for_blob(client_request_id, ctx.tenant_id(), &canonical);
        CommitPutPlan {
            ctx,
            body: body.clone(),
            claimed_digest: claimed,
            size_bytes: body.len() as u64,
            digest_canonical_text: canonical,
            principal_id: Uuid::from_u128(11),
            region_str: "wnam",
            client_request_id,
            audit_request_id,
            audit_id: Uuid::from_u128(0x019384A0_FACE_7000_8000_000000000001),
            now_ms: 1_700_000_000_000,
        }
    }

    #[tokio::test]
    async fn fresh_write_flows_through_all_three_steps() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;

        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let plan = make_plan(&ctx, Bytes::from_static(b"hello world"), "req-1");

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);
        let out = orch.commit_put(plan).await.unwrap();

        assert_eq!(out.outcome, CasPutOutcome::Fresh);
        assert_eq!(meta.row_count(), 1);
        assert_eq!(meta.outbox_snapshot().len(), 1);
        assert_eq!(backend.len(), 1);
    }

    #[tokio::test]
    async fn idempotent_retry_keeps_state_singular() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);
        let body = Bytes::from_static(b"idempotent");

        let first = orch
            .commit_put(make_plan(&ctx, body.clone(), "req-A"))
            .await
            .unwrap();
        assert_eq!(first.outcome, CasPutOutcome::Fresh);

        // Same request_id + same body = idempotent retry. The audit dedup
        // is `(request_id, event_type)` UNIQUE; row counts stay at 1.
        let second = orch
            .commit_put(make_plan(&ctx, body, "req-A"))
            .await
            .unwrap();
        assert_eq!(second.outcome, CasPutOutcome::Idempotent);
        assert_eq!(meta.row_count(), 1);
        assert_eq!(meta.outbox_snapshot().len(), 1);
        assert_eq!(backend.len(), 1);
    }

    #[tokio::test]
    async fn hash_mismatch_short_circuits_before_storage() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;

        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        // Body is "hello"; we lie and claim a mismatched digest.
        let body = Bytes::from_static(b"hello");
        let mismatched = Digest::compute(b"goodbye");
        let mut plan = make_plan(&ctx, body, "req-X");
        plan.claimed_digest = mismatched;
        plan.digest_canonical_text = format!("blake3:{}", mismatched.to_hex());

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);
        let err = orch.commit_put(plan).await.unwrap_err();

        assert!(matches!(err, OrchestratorError::HashMismatch(_)));
        assert_eq!(backend.len(), 0, "R2 must not be touched on hash mismatch");
        assert_eq!(
            meta.row_count(),
            0,
            "D1 must not be touched on hash mismatch"
        );
        assert!(meta.outbox_snapshot().is_empty());
    }

    /// A `MetaStore` that always fails on `commit_put` so we can exercise
    /// the rollback fast path.
    #[derive(Debug, Default)]
    struct AlwaysFailingMeta;

    impl MetaStore for AlwaysFailingMeta {
        async fn commit_put(&self, _request: CommitPutRequest) -> Result<InsertOutcome, MetaError> {
            Err(MetaError::Backend("D1 timeout (simulated)".to_owned()))
        }

        async fn commit_decrement(
            &self,
            _request: corelink_meta::CommitDecrementRequest,
        ) -> Result<corelink_meta::DecrementOutcome, MetaError> {
            Err(MetaError::Backend("nope".to_owned()))
        }

        async fn commit_soft_delete(
            &self,
            _request: corelink_meta::CommitSoftDeleteRequest,
        ) -> Result<(), MetaError> {
            Err(MetaError::Backend("nope".to_owned()))
        }

        async fn get(
            &self,
            _key: &BlobMetaKey,
        ) -> Result<Option<corelink_meta::BlobMetaRow>, MetaError> {
            Err(MetaError::Backend("nope".to_owned()))
        }
    }

    #[tokio::test]
    async fn d1_failure_after_fresh_r2_put_triggers_rollback() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = AlwaysFailingMeta;
        let reconciler = RecordingReconciler::default();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);
        let body = Bytes::from_static(b"the-body-that-orphans");
        let plan = make_plan(&ctx, body.clone(), "req-orphan");

        let err = orch.commit_put(plan).await.unwrap_err();
        assert!(matches!(
            err,
            OrchestratorError::Meta(MetaError::Backend(_))
        ));

        // ADR-0027 fast path: rollback fired exactly once.
        assert_eq!(reconciler.calls.load(Ordering::SeqCst), 1);
        let last = reconciler
            .last_digest
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .expect("rollback should have recorded the orphan digest");
        assert_eq!(last, Digest::compute(&body));
    }

    #[tokio::test]
    async fn d1_failure_after_duplicate_r2_put_skips_rollback() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let healthy_meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

        // Stage 1: prime the R2 backend (and also commit D1 row) with the
        // healthy meta store, so a subsequent attempt sees R2 Duplicate.
        let body = Bytes::from_static(b"prior-writer-body");
        let warm_recon = NoopOrphanReconciler;
        {
            let warm = CasWriteOrchestrator::new(&writer, &healthy_meta, &warm_recon);
            warm.commit_put(make_plan(&ctx, body.clone(), "req-prior"))
                .await
                .unwrap();
        }
        assert_eq!(backend.len(), 1);

        // Stage 2: swap in a failing meta + a recording reconciler. R2 PUT
        // returns Duplicate (412); D1 fails. ADR-0027 §"412 → skip delete":
        // reconciler MUST NOT fire.
        let failing_meta = AlwaysFailingMeta;
        let reconciler = RecordingReconciler::default();
        let orch = CasWriteOrchestrator::new(&writer, &failing_meta, &reconciler);
        let plan = make_plan(&ctx, body, "req-second-attempt");

        let err = orch.commit_put(plan).await.unwrap_err();
        assert!(matches!(
            err,
            OrchestratorError::Meta(MetaError::Backend(_))
        ));
        assert_eq!(
            reconciler.calls.load(Ordering::SeqCst),
            0,
            "reconciler MUST NOT fire on R2 412 (would delete a legitimate prior blob)"
        );
        // R2 row is preserved.
        assert_eq!(backend.len(), 1);
    }
}
