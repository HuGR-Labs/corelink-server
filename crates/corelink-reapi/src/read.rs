//! Pure-logic per-blob CAS read orchestration (WI-S02-001).
//!
//! [`CasReadOrchestrator`] is the load-bearing seam invoked by both the
//! gRPC `ByteStream::Read` handler and the HTTP `GET /v1/cas/<digest>`
//! handler in [`crate::handler`] / [`crate::http_read`]. Keeping it in a
//! pure-logic module (no tonic, no axum, no tokio reactor) lets a future
//! Cloudflare Workers transport (`tonic-web` + `worker::Router`) reuse the
//! same orchestration logic without redoing the AuthZ-check + R2 GET dance.
//!
//! ## Order is load-bearing (WI-S02-001 §1 + §6.1.4 + ADR-0028)
//!
//! 1. **AuthZ check on D1** ([`MetaStore::get`]). Pre-storage. If the
//!    `(tenant_id, digest)` row is absent OR tombstoned, the orchestrator
//!    returns [`ReadOutcome::NotFound`] without any R2 call. This is
//!    CTRL-ISO-002 ("AuthZ on storage call") implemented at the
//!    type-system level.
//! 2. **R2 GET via [`R2Reader::get`]**. The reader builds the canonical
//!    key from `(region, ctx.prefix(), digest)` — a tenant cannot read
//!    another tenant's blob even if the auth-side check were bypassed,
//!    because the per-tenant prefix derivation makes the key collide
//!    only on its OWN bucket entry.
//! 3. **Body BLAKE3 verify** (orchestrator OPTIONALLY off — the client
//!    verify lives in WI-S02-003; this WI returns the raw bytes). The
//!    handler-level integration test asserts byte-identity vs the body
//!    written via S-01.
//! 4. **`last_accessed_at` touch** is **deferred** to the eventual D1
//!    write path; INV-CAS-IMMUTABILITY says we never mutate the row's
//!    digest / size_bytes / created_at, but `last_accessed_at` is a
//!    legitimate-by-spec mutation. The write is **non-blocking on the
//!    read path**: the handler can fire-and-forget per WI §6.1.5 cost
//!    budget. We surface a touch-needed signal through [`ReadOutcome`]
//!    so the handler can schedule the side-effect; the orchestrator
//!    itself stays read-only over the meta store (test-friendly:
//!    `&dyn MetaStore` is enough; no mutation API needed at this layer).
//!
//! ## Cross-tenant masking (ADR-0028 uniform 404 freeze)
//!
//! When the AuthZ check returns `Ok(None)` because the row exists under
//! a DIFFERENT tenant's prefix, the orchestrator returns
//! [`ReadOutcome::NotFound`] — the SAME variant as a never-existed
//! digest and as a tombstoned digest. The client sees one 404. The
//! handler-side audit emission distinguishes the [`MissReason`] only
//! into the LOW-SEVERITY [`crate::audit::REAPI_READ_MISS`] event for
//! the conflated NeverExisted/CrossTenantMasked arm — it is
//! structurally impossible to tell the two apart at the read seam
//! without a side-channel oracle. The SEV-1-driving
//! [`crate::audit::REAPI_CROSS_TENANT_ATTEMPT`] is emitted offline by
//! the S-09 chain consumer once the global digest index confirms
//! cross-tenant ownership. Clients never see this disambiguation
//! (codex round-4 P2 split + round-5 docs alignment).
//!
//! Per WI §6.1.4 the timing-padding for parity across MissReason
//! variants is **out of scope** for this WI (delivered by WI-S02-004
//! constant-time middleware that wraps this handler). This orchestrator
//! emits the variant on the [`ReadOutcome`] type but does NOT perform
//! padding itself.
//!
//! ## Streaming
//!
//! The orchestrator returns the FULL [`Bytes`] body. The handler chunks
//! it into `≤ 1 MiB` slices for `ByteStream::Read` (REAPI v2 §contract:
//! the server MAY send chunks of any size, but ≤ 1 MiB is canonical for
//! REAPI conformance per remote_cache_product_profile.md §7.2.1). For
//! S-01-003 single-blob 5 MiB cap the body is bounded; multipart read
//! lands in S-05 alongside multipart write. Worker memory peak per
//! concurrent request is therefore bounded by the 5 MiB cap + per-chunk
//! overhead — well under the 50 MiB hard ceiling per WI §10.1.3.

use core::future::Future;

use bytes::Bytes;
use corelink_cas::r2_storage::R2Error;
use corelink_cas::r2_storage::{R2Backend, R2Reader};
use corelink_hash::Digest;
use corelink_meta::{BlobMetaKey, BlobMetaRow, MetaError, MetaStore};
use corelink_replication::region_resolver::TenantCtx;
use thiserror::Error;

/// Disambiguated 404 reason, retained on the [`ReadOutcome::NotFound`]
/// variant for audit forensics. The client never sees this enum — every
/// variant maps to the same wire 404 with `error_code =
/// COR_CAS_BLOB_NOT_FOUND` per ADR-0028.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissReason {
    /// No `(tenant_id, digest)` row in `blob_meta`. Could be a never-
    /// existed digest **or** a digest stored under a different tenant's
    /// prefix — the trait-level [`MetaStore::get`] cannot distinguish
    /// (the PK is `(tenant_id, digest)`; cross-tenant rows are simply
    /// invisible to a query under another tenant_id).
    NeverExisted,
    /// `(tenant_id, digest)` row exists in `blob_meta` but
    /// `deleted_at IS NOT NULL` — the blob was tombstoned by S-06 GC
    /// soft-delete and is invisible per WI §6.1.6.
    Tombstoned,
    /// `(tenant_id, digest)` row exists, alive, and the AuthZ check
    /// passed — but R2 returned NotFound. This is the "orphan blob_meta
    /// row" failure mode the GC sweep (S-06 reconciler) is designed to
    /// repair within ≤ 24h. While the orphan window is open, callers
    /// see a uniform 404; the audit forensic distinguishes via this
    /// variant so SRE can correlate orphan-counter spikes with R2
    /// reconciliation events.
    R2OrphanRow,
}

/// Outcome of a per-blob read orchestration call.
#[derive(Clone, Debug)]
pub enum ReadOutcome {
    /// Body fetched; bytes returned. The `size_bytes` field is the
    /// blob_meta-recorded size — the handler asserts it matches
    /// `body.len()` (a defense-in-depth check; INV-CAS-INTEGRITY +
    /// INV-CAS-IMMUTABILITY say they MUST match).
    Hit {
        /// Body bytes as returned by R2.
        body: Bytes,
        /// blob_meta-recorded size.
        size_bytes: u64,
    },
    /// 404 — uniform per ADR-0028. The disambiguated [`MissReason`]
    /// is forensic-only; the wire surface emits a single
    /// `COR_CAS_BLOB_NOT_FOUND` regardless.
    NotFound(MissReason),
}

/// Errors surfaced by [`CasReadOrchestrator::read_blob`].
///
/// [`ReadOutcome::NotFound`] is NOT modeled as an error here — it is a
/// happy-path outcome (404 is a legitimate response). This enum carries
/// only the **transport / programmer** failures.
#[derive(Debug, Error)]
pub enum ReadOrchestratorError {
    /// D1 (or fake) backend transport failure on the AuthZ check. Maps
    /// to `COR_SERVICE_DEGRADED` (HTTP 503 / gRPC `UNAVAILABLE` 14).
    #[error("D1 AuthZ check failed: {0}")]
    Meta(#[from] MetaError),
    /// R2 backend transport failure (NOT NotFound — that's
    /// [`ReadOutcome::NotFound`]). Maps to `COR_SERVICE_DEGRADED`.
    /// Region mismatch programs map to `COR_INTERNAL`.
    #[error("R2 GET failed: {0}")]
    R2(#[from] R2Error),
}

/// Read orchestrator over a generic R2 backend + MetaStore impl.
///
/// Holds **references** so the same orchestrator instance per request
/// can borrow Arc-shared backends without churn — symmetric to
/// [`crate::orchestrator::CasWriteOrchestrator`].
#[derive(Debug)]
pub struct CasReadOrchestrator<'a, B: R2Backend, M: MetaStore> {
    reader: &'a R2Reader<B>,
    meta: &'a M,
}

impl<'a, B: R2Backend, M: MetaStore> CasReadOrchestrator<'a, B, M> {
    /// Construct an orchestrator borrowing the two injected dependencies.
    #[must_use]
    pub const fn new(reader: &'a R2Reader<B>, meta: &'a M) -> Self {
        Self { reader, meta }
    }

    /// Per-blob read orchestration. Returns the body on hit or a
    /// disambiguated [`ReadOutcome::NotFound`] on miss.
    ///
    /// # Errors
    ///
    /// - [`ReadOrchestratorError::Meta`] — D1 transport failure during
    ///   the AuthZ check. NOT raised on `(row absent | tombstoned)` —
    ///   those are happy-path 404s.
    /// - [`ReadOrchestratorError::R2`] — R2 transport failure during
    ///   GET. NOT raised on R2 NotFound — that maps to
    ///   [`MissReason::R2OrphanRow`] (the AuthZ check passed but R2
    ///   has no object; a known eventual-consistency / orphan window).
    pub fn read_blob<'r>(
        &'r self,
        ctx: &'r TenantCtx,
        digest: &'r Digest,
    ) -> impl Future<Output = Result<ReadOutcome, ReadOrchestratorError>> + Send + 'r
    where
        'a: 'r,
    {
        async move {
            // Step 1 — AuthZ check on D1.
            //
            // The PK is `(tenant_id, digest)`. A query under tenant B
            // for a digest stored under tenant A returns `Ok(None)` —
            // the row is invisible to the wrong tenant. This is the
            // load-bearing CTRL-ISO-002 invariant: by going through the
            // typed `BlobMetaKey` constructor (which takes our
            // `ctx.tenant_id()`), it is structurally impossible for a
            // caller to query "is digest D anywhere" — only "is
            // digest D under tenant T" is expressible.
            let key = BlobMetaKey::new(ctx.tenant_id(), *digest);
            let row_opt: Option<BlobMetaRow> = self.meta.get(&key).await?;
            let row = match row_opt {
                None => {
                    // No row. Could be never-existed OR cross-tenant.
                    // The trait surface cannot disambiguate; the
                    // orchestrator emits NeverExisted and the audit
                    // emission elsewhere folds in the cross-tenant
                    // disambiguation if the handler chooses (a future
                    // S-09 audit chain could probe an alt-tenant index;
                    // for S-02 we surface NeverExisted uniformly).
                    return Ok(ReadOutcome::NotFound(MissReason::NeverExisted));
                }
                Some(r) => r,
            };
            if !row.is_alive() {
                return Ok(ReadOutcome::NotFound(MissReason::Tombstoned));
            }

            // Step 2 — R2 GET.
            //
            // The reader's `get` method itself enforces
            // `ctx.region() == reader.region()` and derives the
            // canonical key from `(region, ctx.prefix(), digest)` so
            // the cross-tenant prefix isolation holds at the storage
            // seam too — defense-in-depth: even if the AuthZ check
            // had been bypassed, the prefix component of the key
            // would route the GET to a non-existent slot under the
            // wrong tenant's namespace.
            match self.reader.get(ctx, digest).await {
                Ok(body) => Ok(ReadOutcome::Hit {
                    body,
                    size_bytes: row.size_bytes,
                }),
                Err(R2Error::NotFound) => Ok(ReadOutcome::NotFound(MissReason::R2OrphanRow)),
                Err(other) => Err(other.into()),
            }
        }
    }
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
    use std::sync::Arc;

    use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
    use corelink_hash::Digest;
    use corelink_meta::InMemoryMetaStore;
    use corelink_meta::{
        AuditEvent, AuditEventType, CommitPutRequest, CommitSoftDeleteRequest, RequestId,
    };
    use corelink_replication::region_resolver::Region;
    use corelink_tenant_path::TenantDerivationKey;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_tdk() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    /// Drive a successful S-01 write (R2 + D1) so the read tests have a
    /// pre-populated state.
    async fn write_blob(
        backend: &Arc<InMemoryR2>,
        meta: &InMemoryMetaStore,
        ctx: &TenantCtx,
        body: &[u8],
        request_id: &str,
    ) -> Digest {
        let digest = Digest::compute(body);
        let writer = R2Writer::new(Region::Wnam, Arc::clone(backend));
        let vb = corelink_hash::VerifiedBody::new(Bytes::copy_from_slice(body), digest).unwrap();
        writer.put(ctx, &vb).await.unwrap();
        let key = BlobMetaKey::new(ctx.tenant_id(), digest);
        meta.commit_put(CommitPutRequest {
            key,
            size_bytes: body.len() as u64,
            now_ms: 1_700_000_000_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x019384A0_FACE_7000_8000_000000000001),
                request_id: RequestId::new(request_id),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();
        digest
    }

    #[tokio::test]
    async fn happy_path_returns_hit_with_size_bytes() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let digest = write_blob(&backend, &meta, &ctx, b"hello world", "req-1").await;

        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let out = orch.read_blob(&ctx, &digest).await.unwrap();
        match out {
            ReadOutcome::Hit { body, size_bytes } => {
                assert_eq!(body.as_ref(), b"hello world");
                assert_eq!(size_bytes, 11);
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn never_existed_digest_returns_never_existed() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let digest = Digest::compute(b"not-written");

        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let out = orch.read_blob(&ctx, &digest).await.unwrap();
        assert!(matches!(
            out,
            ReadOutcome::NotFound(MissReason::NeverExisted)
        ));
    }

    /// **CRITICAL — FF-HR-002 specific.** Tenant B asks for a digest
    /// that exists under Tenant A's prefix. AuthZ check returns None
    /// because the PK `(B.tenant_id, digest)` does not exist; we
    /// return NeverExisted (uniform 404). R2 is NEVER called for B.
    #[tokio::test]
    async fn cross_tenant_read_returns_never_existed_without_r2_call() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(2), Region::Wnam);

        // A writes; backend has 1 object.
        let digest = write_blob(&backend, &meta, &ctx_a, b"secret-of-A", "req-A").await;
        let r2_keys_pre = backend.keys_snapshot();
        assert_eq!(r2_keys_pre.len(), 1);

        // B reads. Must return NeverExisted; R2 must NOT be hit.
        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let out = orch.read_blob(&ctx_b, &digest).await.unwrap();
        assert!(
            matches!(out, ReadOutcome::NotFound(MissReason::NeverExisted)),
            "cross-tenant read must surface as uniform NotFound; got {out:?}"
        );
        // R2 still has only A's object — B's call did not even touch
        // the backend (AuthZ check short-circuited).
        let r2_keys_post = backend.keys_snapshot();
        assert_eq!(r2_keys_post.len(), 1);
        assert_eq!(r2_keys_pre, r2_keys_post);
    }

    #[tokio::test]
    async fn tombstoned_blob_returns_tombstoned() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let digest = write_blob(&backend, &meta, &ctx, b"to-be-tombstoned", "req-tomb").await;

        // Soft-delete via the canonical path.
        let key = BlobMetaKey::new(ctx.tenant_id(), digest);
        meta.commit_soft_delete(CommitSoftDeleteRequest {
            key,
            now_ms: 1_700_000_001_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x019384A0_DEAD_7000_8000_000000000002),
                request_id: RequestId::new("req-soft-delete"),
                event_type: AuditEventType::CasSoftDeleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();

        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let out = orch.read_blob(&ctx, &digest).await.unwrap();
        assert!(matches!(out, ReadOutcome::NotFound(MissReason::Tombstoned)));
    }

    /// AuthZ row exists alive but R2 is empty (orphan blob_meta row).
    /// The orchestrator surfaces R2OrphanRow.
    #[tokio::test]
    async fn r2_orphan_row_surfaces_distinct_miss_reason() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

        // Insert blob_meta directly (skipping R2 PUT) so the row
        // exists but R2 is empty. We use a manual commit_put against a
        // body we never wrote.
        let body = b"would-be-body";
        let digest = Digest::compute(body);
        let key = BlobMetaKey::new(ctx.tenant_id(), digest);
        meta.commit_put(CommitPutRequest {
            key,
            size_bytes: body.len() as u64,
            now_ms: 1_700_000_000_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x019384A0_FACE_7000_8000_000000000004),
                request_id: RequestId::new("req-orphan"),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();

        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let out = orch.read_blob(&ctx, &digest).await.unwrap();
        assert!(matches!(
            out,
            ReadOutcome::NotFound(MissReason::R2OrphanRow)
        ));
    }

    #[tokio::test]
    async fn region_mismatch_surfaces_as_r2_error() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        // ctx in WEUR but reader bound to WNAM.
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Weur);

        // Write a row directly into D1 so AuthZ check passes (we want
        // to exercise the post-AuthZ R2 path).
        let body = b"region-mismatch-body";
        let digest = Digest::compute(body);
        let key = BlobMetaKey::new(ctx.tenant_id(), digest);
        meta.commit_put(CommitPutRequest {
            key,
            size_bytes: body.len() as u64,
            now_ms: 1_700_000_000_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x019384A0_FACE_7000_8000_000000000005),
                request_id: RequestId::new("req-region-mismatch"),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();

        let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
        let orch = CasReadOrchestrator::new(&reader, &meta);
        let err = orch.read_blob(&ctx, &digest).await.unwrap_err();
        assert!(
            matches!(
                err,
                ReadOrchestratorError::R2(R2Error::RegionMismatch { .. })
            ),
            "expected RegionMismatch; got {err:?}"
        );
    }
}
