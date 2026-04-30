//! Pure-logic batch existence-check orchestration (WI-S02-002).
//!
//! [`FindMissingOrchestrator`] is the load-bearing seam invoked by both
//! the gRPC `ContentAddressableStorage::FindMissingBlobs` handler and
//! the HTTP `POST /v1/cas/find-missing` handler. Keeping it in a
//! pure-logic module (no tonic, no axum, no tokio reactor surface
//! beyond `Future` polymorphism) lets a future Cloudflare Workers
//! transport reuse the same orchestration without redoing the per-
//! digest D1 AuthZ-check dance.
//!
//! ## Surface
//!
//! Given a [`TenantCtx`] and an arbitrary list of [`Digest`]s, return
//! the SUBSET that the requesting tenant does NOT possess (alive +
//! visible under `(tenant_id, digest)` PK). Per ADR-0028 the result is
//! "missing" both when the digest never existed AND when it exists
//! under a DIFFERENT tenant — the trait surface is structurally unable
//! to distinguish, which is the load-bearing CTRL-ISO-005
//! ("no cross-tenant existence oracle") guarantee.
//!
//! ## Order is load-bearing (WI-S02-002 §6.1.4 + ADR-0028)
//!
//! 1. **Per-digest D1 AuthZ check** ([`MetaStore::get`]) under the
//!    requesting tenant's PK. Tombstoned rows surface as MISSING (per
//!    ADR-0028 §"tombstoned reads → uniform 404").
//! 2. **R2 NEVER consulted on the find-missing path.** D1 is the
//!    authoritative metadata index; the R2 orphan window (an alive
//!    blob_meta row whose R2 object disappeared) is repaired by the
//!    GC reconciler in S-06 and surfaces as a 404 on the read path.
//!    Honouring blob_meta as the existence oracle here means
//!    `FindMissingBlobs` is consistent with `BatchUpdateBlobs` (which
//!    drives the index) within the same write-then-find-missing
//!    transaction — a Bazel client that uploaded D in batch N MUST
//!    see D as "present" in batch N+1.
//! 3. **Bounded parallel dispatch.** With 1000 digests per request +
//!    a per-query D1 cost ≈ 5 ms, sequential dispatch costs 5 s; that
//!    blows the WI §14.2.4 latency budget (p99 ≤ 200 ms). We cap
//!    parallel `MetaStore::get` calls at [`MAX_PARALLEL_AUTHZ`] (default
//!    100) via `futures::stream::iter(...).buffer_unordered(N)` so the
//!    D1 connection pool is not oversubscribed under burst load.
//!
//! ## Negative cache integration (WI-S02-005 forward; out of scope)
//!
//! `FindMissingBlobs` MUST always fall through to D1 per
//! `remote_cache_product_profile.md §11.2 REG-NEGATIVE-001` — the
//! negative cache (KV) is a `GetBlob`-only short-circuit. If a future
//! WI-S02-005 layer wraps this orchestrator, it MUST NOT short-circuit
//! the D1 hit on a "previously missing" KV entry: a fresh upload
//! between the two calls would otherwise surface as a stale
//! "still missing" miss.
//!
//! ## Cross-tenant masking (WI-S02-002 §6.1.4 + ADR-0028)
//!
//! Tenant B asks `FindMissingBlobs([D])` where D is owned by Tenant A.
//! The MetaStore::get call uses `(B.tenant_id, D)` as the PK lookup key
//! — A's row is invisible. The orchestrator emits D in
//! `missing_blob_digests` exactly as if D had never been written
//! anywhere. From B's perspective, the response is structurally
//! indistinguishable from a never-existed digest.
//!
//! This is the canonical defense against the cross-tenant existence
//! oracle (CTRL-ISO-005); the property test
//! `prop_find_missing_no_cross_tenant_leak` exercises 10k iter at
//! the public API surface to enforce this at CI time.

use core::future::Future;

use corelink_hash::Digest;
use corelink_meta::{BlobMetaKey, MetaError, MetaStore};
use corelink_worker::TenantCtx;
use thiserror::Error;

/// Maximum digests per `FindMissingBlobs` request (REAPI v2 batch cap).
///
/// Larger batches stress Worker memory + the D1 connection pool. 1000 is
/// the canonical sweet spot per WI-S02-002 §9.2 — covers 99% of Bazel
/// client batch sizes, keeps p99 ≤ 200 ms tractable, and aligns with
/// REAPI v2 conformance recommendation. The handler MUST surface
/// `OUT_OF_RANGE` (gRPC code 11; HTTP 413) + `COR_CAS_BATCH_SIZE_EXCEEDED`
/// when this cap is exceeded — exposed as a `pub const` so the handler
/// reuses the same constant.
pub const MAX_FIND_MISSING_BATCH_SIZE: usize = 1000;

/// Maximum simultaneous `MetaStore::get` invocations during a single
/// batch dispatch. With 1000 digests per request and ~ 5 ms per D1
/// query, parallelism 100 brings batch p99 to ≈ 50 ms (10× speedup vs
/// sequential). Higher concurrency exhausts the Cloudflare D1 client
/// connection pool (typical limit ~ 6 concurrent queries per worker
/// runtime) — production binding adapter applies further back-pressure
/// internally; this cap is the orchestrator-side budget.
pub const MAX_PARALLEL_AUTHZ: usize = 100;

/// Errors surfaced by [`FindMissingOrchestrator::find_missing`].
///
/// "Digest is missing" is NOT modeled as an error — it is the
/// happy-path outcome (the entire purpose of the RPC). This enum
/// carries only **transport / programmer** failures.
#[derive(Debug, Error)]
pub enum FindMissingError {
    /// D1 (or fake) backend transport failure on a per-digest AuthZ
    /// check. Maps to `COR_SERVICE_DEGRADED` (gRPC `UNAVAILABLE` 14;
    /// HTTP 503).
    #[error("D1 AuthZ check failed: {0}")]
    Meta(#[from] MetaError),
}

/// Per-batch outcome.
#[derive(Clone, Debug)]
pub struct FindMissingOutcome {
    /// Digests that the requesting tenant does NOT possess. Order is
    /// preserved relative to the input `digests` slice (cf. WI-S02-002
    /// §6.1.4 — Bazel client correlates response order with request
    /// order).
    ///
    /// **Ambiguity warning** for size-aware callers: when the input
    /// contains the same hash with two different declared sizes
    /// (`[(H, S1), (H, S2)]`), the resulting `Vec<Digest>` cannot
    /// disambiguate which input *slot* was missing — the digest
    /// `H` appears once or twice in the missing vec but the wire
    /// response needs to know per-slot. Use [`Self::slot_is_missing`]
    /// for per-slot semantics (REAPI canonical).
    pub missing: Vec<Digest>,
    /// Per-input-slot missing flag. `slot_is_missing[i] == true` iff
    /// the i-th entry of the input slice is missing in the requesting
    /// tenant's scope under REAPI Digest identity (hash + declared
    /// size). REAPI handlers MUST use this field — not [`Self::missing`]
    /// — when reconstructing the wire response, because the
    /// hash-only `Vec<Digest>` form cannot distinguish a
    /// `[(H, real), (H, wrong)]` input where slot 0 is present and
    /// slot 1 is missing (codex round-2 P1 SEAL fix).
    pub slot_is_missing: Vec<bool>,
    /// Number of D1 lookups actually issued. Equal to `digests.len()`
    /// modulo client-side dedup; surfaced for metric emission +
    /// criterion benchmark observability.
    pub d1_lookups: usize,
}

/// Pure-logic batch existence-check orchestrator.
///
/// Holds **a reference** to the metadata store so per-request callers
/// borrow without churn — symmetric to
/// [`crate::read::CasReadOrchestrator`].
#[derive(Debug)]
pub struct FindMissingOrchestrator<'a, M: MetaStore> {
    meta: &'a M,
}

impl<'a, M: MetaStore> FindMissingOrchestrator<'a, M> {
    /// Construct an orchestrator borrowing the metadata store.
    #[must_use]
    pub const fn new(meta: &'a M) -> Self {
        Self { meta }
    }

    /// Per-batch existence-check orchestration.
    ///
    /// Returns a [`FindMissingOutcome`] whose `missing` field is the
    /// SUBSET of `digests` for which `(ctx.tenant_id(), digest)` is
    /// either absent OR tombstoned in `blob_meta`. Order in the
    /// response matches order in the input slice — duplicate digests
    /// in the request surface duplicate entries in the response (we
    /// do NOT collapse client-side dedup; a Bazel client that batches
    /// the same digest twice deserves the same answer twice).
    ///
    /// This API takes the raw [`Digest`] only (no declared size). For
    /// REAPI v2 wire conformance the handler MUST also enforce
    /// `(hash, size_bytes)` identity per-digest — see
    /// [`Self::find_missing_with_sizes`] (REAPI canonical) which
    /// additionally treats a size-mismatched-but-hash-present digest
    /// as missing.
    ///
    /// Bounded concurrency: at most [`MAX_PARALLEL_AUTHZ`] D1 lookups
    /// in flight.
    ///
    /// # Errors
    ///
    /// Returns [`FindMissingError::Meta`] when ANY per-digest D1
    /// lookup raises a transport fault. The orchestrator does NOT
    /// continue past the first error — partial responses would expose
    /// a non-deterministic surface (different `missing` sets across
    /// retries) which violates INV-CAS-IDEMPOTENCY at the read seam
    /// (ADR-0028 §"FindMissingBlobs is total-or-fail").
    pub fn find_missing<'r>(
        &'r self,
        ctx: &'r TenantCtx,
        digests: &'r [Digest],
    ) -> impl Future<Output = Result<FindMissingOutcome, FindMissingError>> + Send + 'r
    where
        'a: 'r,
    {
        // Delegate to the size-checking variant with `None` declared
        // sizes (skip the size cross-check). Keeping a thin wrapper
        // here means a future caller that only carries `Digest`s
        // (e.g. an internal GC reconciler that already trusts its own
        // metadata) can still call this method without forging
        // declared sizes.
        async move {
            let pairs: Vec<(Digest, Option<u64>)> =
                digests.iter().copied().map(|d| (d, None)).collect();
            self.find_missing_with_sizes(ctx, &pairs).await
        }
    }

    /// REAPI-canonical per-batch existence check.
    ///
    /// Per `bazelbuild/remote-apis` v2.12.0 the wire `Digest` is the
    /// `(hash, size_bytes)` pair; identity is BOTH fields. A request
    /// for `(hash=H, size_bytes=S)` where the server has
    /// `(H, S' != S)` MUST be treated as MISSING (codex round-1 P1
    /// fix; REAPI conformance) — otherwise a tooling bug that ships
    /// the wrong size silently surfaces as "present" and the client
    /// proceeds to trust a blob it could not verify.
    ///
    /// The `declared_size` is `Option<u64>`:
    /// - `Some(s)` → cross-check `row.size_bytes == s`; mismatch
    ///   surfaces the digest in `missing`.
    /// - `None` → skip the size check (legacy callers / internal
    ///   reconcilers that already trust their metadata).
    ///
    /// Order, dedup semantics, and concurrency bound are identical
    /// to [`Self::find_missing`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::find_missing`]: [`FindMissingError::Meta`]
    /// on any transport fault; first-error-aborts the batch.
    pub fn find_missing_with_sizes<'r>(
        &'r self,
        ctx: &'r TenantCtx,
        digests: &'r [(Digest, Option<u64>)],
    ) -> impl Future<Output = Result<FindMissingOutcome, FindMissingError>> + Send + 'r
    where
        'a: 'r,
    {
        async move {
            // Empty input — nothing to look up; return empty.
            if digests.is_empty() {
                return Ok(FindMissingOutcome {
                    missing: Vec::new(),
                    slot_is_missing: Vec::new(),
                    d1_lookups: 0,
                });
            }
            // Pre-condition: handler-side cap enforces ≤ 1000; defense
            // in depth by asserting the orchestrator-side bound here
            // would panic on misuse, which we do not do (a batched
            // 1001-digest request must surface as a structured
            // `OUT_OF_RANGE` from the handler, never a panic).
            // Per-index parallel dispatch with bounded concurrency.
            // Each future produces `(idx, Result<is_present, err>)`.
            // "Present" requires BOTH (a) row exists + alive, AND
            // (b) row.size_bytes matches declared (if declared is
            // Some).
            //
            // Tokens captured:
            // - `tenant_id` is `Copy` (UUID) so closures own it cheaply.
            // - The digest list is collected into an owned `Vec<(usize,
            //   Digest, Option<u64>)>` so the per-future closure does
            //   not capture the input slice's lifetime — the type
            //   system rejects `&'r [..]` in a `Send + 'static` future
            //   via `FnOnce` HRTB. `Digest` is 32 bytes Copy; the
            //   clone is negligible vs the D1 round-trip (~ 5 ms).
            use futures::stream::StreamExt;
            let n = digests.len();
            let mut presence: Vec<Option<bool>> = vec![None; n];
            let meta_ref: &M = self.meta;
            let tenant_id = ctx.tenant_id();
            let owned: Vec<(usize, Digest, Option<u64>)> = digests
                .iter()
                .copied()
                .enumerate()
                .map(|(idx, (d, s))| (idx, d, s))
                .collect();
            let mut stream = futures::stream::iter(owned)
                .map(move |(idx, digest, declared_size)| async move {
                    let key = BlobMetaKey::new(tenant_id, digest);
                    let row_opt = meta_ref.get(&key).await?;
                    // Present iff a row exists AND is alive AND (when
                    // a declared size was supplied) row.size_bytes
                    // matches. A `(hash, wrong_size)` request thus
                    // surfaces in `missing`, not as "present" — REAPI
                    // v2.12.0 Digest identity = `(hash, size_bytes)`.
                    let is_present = match row_opt {
                        None => false,
                        Some(row) => {
                            if !row.is_alive() {
                                false
                            } else {
                                match declared_size {
                                    Some(declared) => row.size_bytes == declared,
                                    None => true,
                                }
                            }
                        }
                    };
                    Ok::<(usize, bool), MetaError>((idx, is_present))
                })
                .buffer_unordered(MAX_PARALLEL_AUTHZ);
            while let Some(item) = stream.next().await {
                let (idx, is_present) = item?;
                if let Some(slot) = presence.get_mut(idx) {
                    *slot = Some(is_present);
                }
            }
            // Recompose `missing` + `slot_is_missing` in input order.
            // The per-slot Vec<bool> is the canonical wire-shape source
            // (codex round-2 P1 SEAL fix); `missing` is retained for
            // backward compatibility with internal callers (e.g. GC
            // reconcilers) that want a hash-only summary.
            let mut missing: Vec<Digest> = Vec::with_capacity(n);
            let mut slot_is_missing: Vec<bool> = Vec::with_capacity(n);
            for (idx, (digest, _)) in digests.iter().enumerate() {
                let present = presence
                    .get(idx)
                    .copied()
                    .flatten()
                    // Defensive: every index was filled by the stream;
                    // a `None` here would mean the stream completed
                    // without yielding for `idx`, which contradicts
                    // `buffer_unordered`'s contract (every input
                    // future yields exactly one output before the
                    // stream terminates). Treat as missing — the
                    // safer (no-leak) failure mode.
                    .unwrap_or(false);
                slot_is_missing.push(!present);
                if !present {
                    missing.push(*digest);
                }
            }
            Ok(FindMissingOutcome {
                missing,
                slot_is_missing,
                d1_lookups: n,
            })
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

    use bytes::Bytes;
    use corelink_meta::InMemoryMetaStore;
    use corelink_meta::{
        AuditEvent, AuditEventType, CommitPutRequest, CommitSoftDeleteRequest, RequestId,
    };
    use corelink_tenant_path::TenantDerivationKey;
    use corelink_worker::storage::r2::{InMemoryR2, R2Writer};
    use corelink_worker::Region;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_tdk() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    /// Drive a successful S-01 write (R2 + D1) so the find_missing
    /// tests have a pre-populated state.
    async fn write_blob(
        backend: &Arc<InMemoryR2>,
        meta: &InMemoryMetaStore,
        ctx: &TenantCtx,
        body: &[u8],
        request_id: &str,
        unique_id: u128,
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
                id: Uuid::from_u128(unique_id),
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
    async fn empty_input_returns_empty_outcome() {
        let meta = InMemoryMetaStore::new();
        let tdk = fixed_tdk();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[]).await.unwrap();
        assert!(out.missing.is_empty());
        assert_eq!(out.d1_lookups, 0);
    }

    #[tokio::test]
    async fn all_present_returns_empty_missing() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d1 = write_blob(&backend, &meta, &ctx, b"first", "req-1", 0x1).await;
        let d2 = write_blob(&backend, &meta, &ctx, b"second", "req-2", 0x2).await;
        let d3 = write_blob(&backend, &meta, &ctx, b"third", "req-3", 0x3).await;
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[d1, d2, d3]).await.unwrap();
        assert!(out.missing.is_empty());
        assert_eq!(out.d1_lookups, 3);
    }

    #[tokio::test]
    async fn all_absent_returns_full_input_in_input_order() {
        let tdk = fixed_tdk();
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d1 = Digest::compute(b"never-1");
        let d2 = Digest::compute(b"never-2");
        let d3 = Digest::compute(b"never-3");
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[d1, d2, d3]).await.unwrap();
        assert_eq!(out.missing, vec![d1, d2, d3]);
        assert_eq!(out.d1_lookups, 3);
    }

    #[tokio::test]
    async fn partial_present_subset_preserves_input_order() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d_present_1 = write_blob(&backend, &meta, &ctx, b"have-1", "req-h1", 0x10).await;
        let d_absent_1 = Digest::compute(b"missing-1");
        let d_present_2 = write_blob(&backend, &meta, &ctx, b"have-2", "req-h2", 0x11).await;
        let d_absent_2 = Digest::compute(b"missing-2");
        let d_absent_3 = Digest::compute(b"missing-3");
        let orch = FindMissingOrchestrator::new(&meta);
        let inputs = [d_present_1, d_absent_1, d_present_2, d_absent_2, d_absent_3];
        let out = orch.find_missing(&ctx, &inputs).await.unwrap();
        assert_eq!(out.missing, vec![d_absent_1, d_absent_2, d_absent_3]);
        assert_eq!(out.d1_lookups, 5);
    }

    #[tokio::test]
    async fn duplicate_input_yields_duplicate_response() {
        // Bazel/Buck2 client deduping is the client's job; if we
        // collapsed dedup on the server, an off-by-one would surface
        // as silent client confusion. Pin the contract: input N
        // copies → output N copies (when the digest is missing).
        let tdk = fixed_tdk();
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d = Digest::compute(b"never-existed");
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[d, d, d]).await.unwrap();
        assert_eq!(out.missing, vec![d, d, d]);
        assert_eq!(out.d1_lookups, 3);
    }

    /// **CRITICAL — FF-HR-002 specific.** Tenant B asks for digests
    /// that exist under Tenant A. AuthZ check returns None because the
    /// PK `(B.tenant_id, digest)` does not exist; the orchestrator
    /// returns the digest in `missing` — uniform with truly-never-
    /// existed. R2 is NEVER consulted.
    #[tokio::test]
    async fn cross_tenant_blobs_surface_as_missing() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(2), Region::Wnam);

        // A writes 3 blobs.
        let d_a1 = write_blob(&backend, &meta, &ctx_a, b"A-secret-1", "req-A1", 0x100).await;
        let d_a2 = write_blob(&backend, &meta, &ctx_a, b"A-secret-2", "req-A2", 0x101).await;
        let d_a3 = write_blob(&backend, &meta, &ctx_a, b"A-secret-3", "req-A3", 0x102).await;

        // B asks for them — all surface as missing.
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx_b, &[d_a1, d_a2, d_a3]).await.unwrap();
        assert_eq!(out.missing, vec![d_a1, d_a2, d_a3]);
        assert_eq!(out.d1_lookups, 3);
    }

    #[tokio::test]
    async fn tombstoned_blobs_surface_as_missing() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d = write_blob(&backend, &meta, &ctx, b"doomed", "req-d", 0x200).await;
        let key = BlobMetaKey::new(ctx.tenant_id(), d);
        meta.commit_soft_delete(CommitSoftDeleteRequest {
            key,
            now_ms: 1_700_000_001_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x201),
                request_id: RequestId::new("req-tomb"),
                event_type: AuditEventType::CasSoftDeleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[d]).await.unwrap();
        assert_eq!(out.missing, vec![d]);
    }

    #[tokio::test]
    async fn determinism_same_input_same_output_across_calls() {
        // Two back-to-back calls with the same input MUST yield the
        // same `missing` list (modulo Vec equality). Stresses the
        // bounded-parallel `buffer_unordered` fan-out's order-
        // restoration logic — completion order is not response
        // order.
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let mut digests = Vec::new();
        for i in 0..20u32 {
            let body = format!("body-{i}");
            if i % 3 == 0 {
                // Write some, leave others absent.
                let d = write_blob(
                    &backend,
                    &meta,
                    &ctx,
                    body.as_bytes(),
                    &format!("req-{i}"),
                    0x300 + u128::from(i),
                )
                .await;
                digests.push(d);
            } else {
                digests.push(Digest::compute(body.as_bytes()));
            }
        }
        let orch = FindMissingOrchestrator::new(&meta);
        let r1 = orch.find_missing(&ctx, &digests).await.unwrap();
        let r2 = orch.find_missing(&ctx, &digests).await.unwrap();
        assert_eq!(r1.missing, r2.missing);
        assert_eq!(r1.d1_lookups, r2.d1_lookups);
        assert_eq!(r1.d1_lookups, digests.len());
    }

    #[tokio::test]
    async fn batch_of_max_size_completes() {
        // Stress the bounded-parallel dispatcher with a batch at the
        // canonical cap (1000). All digests are absent — the
        // orchestrator should yield 1000 entries in `missing` in
        // input order.
        let tdk = fixed_tdk();
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let digests: Vec<Digest> = (0..MAX_FIND_MISSING_BATCH_SIZE)
            .map(|i| Digest::compute(format!("never-{i}").as_bytes()))
            .collect();
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &digests).await.unwrap();
        assert_eq!(out.missing.len(), MAX_FIND_MISSING_BATCH_SIZE);
        assert_eq!(out.missing, digests);
        assert_eq!(out.d1_lookups, MAX_FIND_MISSING_BATCH_SIZE);
        assert_eq!(out.slot_is_missing.len(), MAX_FIND_MISSING_BATCH_SIZE);
        assert!(out.slot_is_missing.iter().all(|m| *m));
    }

    /// **REAPI Digest identity = `(hash, size_bytes)`** (codex round-2
    /// P1 SEAL fix). Mixed-size same-hash inputs MUST be classified
    /// per slot — independence preserved across input orderings.
    #[tokio::test]
    async fn mixed_size_same_hash_per_slot_independence() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let body = b"r-only"; // 6 bytes
        let d = write_blob(&backend, &meta, &ctx, body, "req-mixed", 0x400).await;
        let real_size = body.len() as u64;

        // Order A: [(d, real), (d, wrong)] — only slot 1 missing.
        let orch = FindMissingOrchestrator::new(&meta);
        let pairs_a: Vec<(Digest, Option<u64>)> =
            vec![(d, Some(real_size)), (d, Some(999))];
        let out_a = orch
            .find_missing_with_sizes(&ctx, &pairs_a)
            .await
            .unwrap();
        assert_eq!(out_a.slot_is_missing, vec![false, true]);
        assert_eq!(out_a.missing, vec![d]); // only the wrong-size slot
        assert_eq!(out_a.d1_lookups, 2);

        // Order B: [(d, wrong), (d, real)] — only slot 0 missing.
        let pairs_b: Vec<(Digest, Option<u64>)> =
            vec![(d, Some(999)), (d, Some(real_size))];
        let out_b = orch
            .find_missing_with_sizes(&ctx, &pairs_b)
            .await
            .unwrap();
        assert_eq!(out_b.slot_is_missing, vec![true, false]);
        assert_eq!(out_b.missing, vec![d]);
        assert_eq!(out_b.d1_lookups, 2);
    }

    /// `find_missing(ctx, &[Digest])` (no declared sizes) MUST behave
    /// identically to legacy callers — present iff alive (no size
    /// cross-check).
    #[tokio::test]
    async fn find_missing_legacy_no_size_check_present_iff_alive() {
        let tdk = fixed_tdk();
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let d = write_blob(&backend, &meta, &ctx, b"legacy-body", "req-l", 0x500).await;
        let orch = FindMissingOrchestrator::new(&meta);
        let out = orch.find_missing(&ctx, &[d]).await.unwrap();
        assert!(out.missing.is_empty());
        assert_eq!(out.slot_is_missing, vec![false]);
    }
}
