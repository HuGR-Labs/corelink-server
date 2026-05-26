//! WI-S02-006 — consolidated cross-component read-path property tests.
//!
//! This file is the WI-S02-006 deliverable analogous to
//! `prop_cas.rs` (WI-S01-006 write-path consolidation): a single proptest
//! harness that exercises the **integrated** S-02 read-path stack
//! (`corelink-hash`, `corelink-tenant-path`, `corelink-worker`,
//! `corelink-meta`, `corelink-reapi::read`,
//! `corelink-reapi::find_missing`, `corelink-worker::cache::negative`)
//! against the load-bearing read-path invariants:
//!
//! - **INV-TENANT-ISOLATION** (CRITICAL) — cross-tenant reads NEVER
//!   surface another tenant's body; cross-tenant cache poisoning is
//!   impossible by construction.
//! - **INV-CAS-INTEGRITY** (CRITICAL) — round-trip same-tenant returns
//!   byte-identical body; metadata-recorded size matches body length.
//! - **INV-CAS-IDEMPOTENCY** (CRITICAL) — two back-to-back GETs of the
//!   same `(tenant, digest)` return byte-identical bodies.
//! - **INV-CAS-IMMUTABILITY** (CRITICAL) — tombstoned reads surface
//!   uniform 404 (`MissReason::Tombstoned`); tombstone is forensic-only
//!   on the wire (ADR-0028 uniform 404 freeze).
//! - **`FindMissingBlobs` no-existence-oracle** (CTRL-ISO-005) — a
//!   tenant asking for digests written by a different tenant ALWAYS
//!   sees them as missing; the response is indistinguishable from
//!   "truly missing".
//! - **Negative cache eventual convergence** (per WI-S02-005 §9.11) —
//!   no incorrect hot read surfaces a stale 200 OK across the
//!   write-then-read sequence; explicit
//!   `invalidate_on_write` clears the entry; transient stale 404s
//!   inside the TTL bound are acceptable per CF KV semantics.
//!
//! ## Round-Robin scheduling (WI §2 narrative)
//!
//! The flagship `cross_tenant_read_round_robin_100k` integration test
//! drives a **deterministic** round-robin schedule across 50 tenants ×
//! 1 000 random read ops = 50 000 single-pass attempts, executed twice
//! for a 100 000-iter total (per WI §10.6.1 + §13). Every distinct
//! `(actor_i, target_j)` pair with `i != j` is exercised ≥ 40 times in
//! the 100 000-iter budget (50 × 50 − 50 = 2 450 cross-pairs;
//! `100_000 / 2_450 ≈ 40.8` minimum reps per pair). Coverage is asserted
//! at the test surface — a regression that under-explored a (i, j) pair
//! would fail the coverage matrix check, not silently sample-skip.
//!
//! ## File-location rationale
//!
//! WI-S02-006 v1.0.0 §13 named the canonical path
//! `crates/corelink-worker/tests/prop_cas_read.rs`. The cross-component
//! scope dev-depends on `corelink-reapi::read` AND `corelink-reapi::find_missing`,
//! which `corelink-worker` cannot dev-depend on without creating a
//! workspace cycle (`corelink-reapi → corelink-worker → corelink-reapi`).
//! v1.1.0 §13 + §31 changelog routes the canonical deliverable to
//! `crates/corelink-reapi/tests/prop_cas_read.rs` — same path-routing
//! decision documented for WI-S01-006 (write-path consolidation).
//!
//! ## Iter counts and runtime budget
//!
//! Per WI §6.1 + §14.6.4:
//!
//! - `prop_get_blob_unary_isolation` — 10 000 cases (proptest).
//! - `prop_find_missing_no_existence_oracle` — 10 000 cases (proptest).
//! - `prop_negative_cache_correctness` — 10 000 cases (proptest).
//! - `prop_round_trip_same_tenant_idempotent_reads` — 10 000 cases.
//! - `cross_tenant_read_round_robin_100k` — explicit 100 000-iter loop
//!   over a 50-tenant × 1 000-op round-robin schedule (50 K serial pass
//!   ×2; coverage ≥ 40 reps/pair).
//!
//! All proptest properties run under
//! `tokio::runtime::Builder::new_current_thread()` per case (matches the
//! WI-S01-006 + prop_cross_tenant_read pattern). Failure seeds persist
//! into a sibling `prop_cas_read.proptest-regressions` file (proptest
//! convention — one file per `*.rs` test target).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test code: panics are test failures by design; integer casts are bounded by the test fixture"
)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, CommitSoftDeleteRequest,
    InMemoryMetaStore, MetaStore, RequestId,
};
use corelink_reapi::find_missing::FindMissingOrchestrator;
use corelink_reapi::read::{CasReadOrchestrator, MissReason as ReadMiss, ReadOutcome};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::cache::kv::{FakeClock, InMemoryKv};
use corelink_cas::cache::negative::{NegativeCache, DEFAULT_NEGATIVE_CACHE_TTL_SECS};
use corelink_cas::cache::MissReason as CacheMiss;
use corelink_cas::r2_storage::{CountingR2, InMemoryR2, R2Reader, R2Writer};
use corelink_worker::{Region, TenantCtx};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixed_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
}

fn ctx_for(tid: Uuid, region: Region) -> TenantCtx {
    TenantCtx::new(&fixed_tdk(), tid, region)
}

fn digest_from_seed(seed: &[u8; 32]) -> Digest {
    let mut hex = String::with_capacity(64);
    for &b in seed {
        hex.push_str(&format!("{b:02x}"));
    }
    Digest::from_hex(&hex).expect("64-char hex is valid")
}

/// Build a tenant UUID from a 64-bit seed in a way that keeps the value
/// space wide while remaining deterministic for round-robin coverage
/// matrix accounting.
fn tenant_uuid(seed: u64) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&seed.to_le_bytes());
    Uuid::from_bytes(bytes)
}

/// Drive a successful S-01 write so the read tests see populated state.
async fn write_blob(
    backend: &Arc<InMemoryR2>,
    meta: &InMemoryMetaStore,
    ctx: &TenantCtx,
    body: &[u8],
    request_id: &str,
    audit_id_seed: u128,
) -> Digest {
    let digest = Digest::compute(body);
    let writer = R2Writer::new(Region::Wnam, Arc::clone(backend));
    let vb = VerifiedBody::new(Bytes::copy_from_slice(body), digest).unwrap();
    writer.put(ctx, &vb).await.unwrap();
    let key = BlobMetaKey::new(ctx.tenant_id(), digest);
    meta.commit_put(CommitPutRequest {
        key,
        size_bytes: body.len() as u64,
        now_ms: 1_700_000_000_000,
        audit: AuditEvent {
            id: Uuid::from_u128(audit_id_seed),
            request_id: RequestId::new(request_id),
            event_type: AuditEventType::CasPutCompleted,
            payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
        },
    })
    .await
    .unwrap();
    digest
}

// ---------------------------------------------------------------------------
// Property A — GetBlob unary isolation @ 10 000 cases
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    })]

    /// **WI §8 `prop_get_blob_unary_isolation` — 10k iter.**
    ///
    /// Random `(tenant_actor, tenant_target, digest_owner)` triples.
    /// `tenant_target` writes a body. The orchestrator is queried once
    /// under `tenant_target` ctx (must Hit) AND once under
    /// `tenant_actor` ctx (must surface uniform 404 NeverExisted).
    /// CountingR2 asserts R2 GET was NEVER fired on the cross-tenant
    /// branch (CTRL-ISO-002 short-circuit at meta layer).
    #[test]
    fn prop_get_blob_unary_isolation(
        body in prop::collection::vec(any::<u8>(), 1..256usize),
        actor_seed in any::<u64>(),
        target_seed in any::<u64>(),
    ) {
        prop_assume!(actor_seed != target_seed);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let inner = Arc::new(InMemoryR2::new());
            let counting = Arc::new(CountingR2::new(Arc::clone(&inner)));
            let meta = InMemoryMetaStore::new();
            let ctx_target = ctx_for(tenant_uuid(target_seed), Region::Wnam);
            let ctx_actor = ctx_for(tenant_uuid(actor_seed), Region::Wnam);

            // Target writes through the counting backend.
            let writer = R2Writer::new(Region::Wnam, Arc::clone(&counting));
            let digest = Digest::compute(&body);
            let vb = VerifiedBody::new(Bytes::copy_from_slice(&body), digest).unwrap();
            writer.put(&ctx_target, &vb).await.unwrap();
            let key = BlobMetaKey::new(ctx_target.tenant_id(), digest);
            meta.commit_put(CommitPutRequest {
                key,
                size_bytes: body.len() as u64,
                now_ms: 1_700_000_000_000,
                audit: AuditEvent {
                    id: Uuid::from_u128(0x0193_84A0_FACE_7000_8000_0000_0000_0001 ^ u128::from(target_seed)),
                    request_id: RequestId::new("req-target"),
                    event_type: AuditEventType::CasPutCompleted,
                    payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
                },
            }).await.unwrap();

            let put_calls = counting.put_calls();
            let get_calls_baseline = counting.get_calls();
            prop_assert!(put_calls >= 1);
            prop_assert_eq!(get_calls_baseline, 0);

            let reader = R2Reader::new(Region::Wnam, Arc::clone(&counting));
            let orch = CasReadOrchestrator::new(&reader, &meta);

            // Same-tenant must Hit byte-identical.
            let out_target = orch.read_blob(&ctx_target, &digest).await.unwrap();
            match out_target {
                ReadOutcome::Hit { body: out, size_bytes } => {
                    prop_assert_eq!(out.as_ref(), body.as_slice());
                    prop_assert_eq!(size_bytes, body.len() as u64);
                }
                other => prop_assert!(false, "expected Hit, got {other:?}"),
            }
            let get_calls_after_target = counting.get_calls();
            prop_assert!(get_calls_after_target >= 1, "target-tenant Hit must invoke R2 GET");

            // Cross-tenant must surface uniform 404 NeverExisted.
            let out_actor = orch.read_blob(&ctx_actor, &digest).await.unwrap();
            prop_assert!(matches!(out_actor, ReadOutcome::NotFound(ReadMiss::NeverExisted)));

            // R2 GET must NOT fire on the cross-tenant branch — AuthZ
            // short-circuited at the meta layer.
            let get_calls_after_actor = counting.get_calls();
            prop_assert_eq!(
                get_calls_after_actor,
                get_calls_after_target,
                "cross-tenant GET fired R2 read; AuthZ short-circuit broken"
            );
            Ok(())
        })?;
    }

    /// **WI §8 `prop_round_trip_same_tenant_idempotent_reads` — 10k iter.**
    ///
    /// Same-tenant write then two GETs return byte-identical body twice
    /// (INV-CAS-IDEMPOTENCY at the read seam). Asserts metadata size
    /// matches body length both times.
    #[test]
    fn prop_round_trip_same_tenant_idempotent_reads(
        body in prop::collection::vec(any::<u8>(), 1..256usize),
        tenant_seed in any::<u64>(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx = ctx_for(tenant_uuid(tenant_seed), Region::Wnam);

            let digest = write_blob(
                &backend,
                &meta,
                &ctx,
                &body,
                "req-idem",
                0x0193_84A0_FACE_7000_8000_0000_0000_0002 ^ u128::from(tenant_seed),
            ).await;

            let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let r1 = orch.read_blob(&ctx, &digest).await.unwrap();
            let r2 = orch.read_blob(&ctx, &digest).await.unwrap();
            match (r1, r2) {
                (
                    ReadOutcome::Hit { body: b1, size_bytes: s1 },
                    ReadOutcome::Hit { body: b2, size_bytes: s2 },
                ) => {
                    prop_assert_eq!(b1.as_ref(), body.as_slice());
                    prop_assert_eq!(b2.as_ref(), body.as_slice());
                    prop_assert_eq!(s1, body.len() as u64);
                    prop_assert_eq!(s2, body.len() as u64);
                }
                (a, b) => prop_assert!(false, "expected two Hits, got {a:?}, {b:?}"),
            }
            Ok(())
        })?;
    }

    /// **WI §8 `prop_find_missing_no_existence_oracle` — 10k iter.**
    ///
    /// Tenant A writes one digest; Tenant B asks `FindMissingBlobs` for a
    /// batch containing `[tenant_a_digest, never_existed_digest]`. The
    /// response surfaces BOTH as missing. The output Vec is
    /// indistinguishable from "truly missing" (Tenant B cannot tell
    /// which entries are owned by Tenant A vs. genuinely never written).
    /// Codex round-2 P1 (WI-S02-002) is regression-pinned here:
    /// `slot_is_missing` must agree per-slot with the ownership flag.
    #[test]
    fn prop_find_missing_no_existence_oracle(
        body in prop::collection::vec(any::<u8>(), 1..128usize),
        actor_seed in any::<u64>(),
        target_seed in any::<u64>(),
        ghost_digest_seed in any::<[u8; 32]>(),
    ) {
        prop_assume!(actor_seed != target_seed);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx_target = ctx_for(tenant_uuid(target_seed), Region::Wnam);
            let ctx_actor = ctx_for(tenant_uuid(actor_seed), Region::Wnam);

            let target_digest = write_blob(
                &backend,
                &meta,
                &ctx_target,
                &body,
                "req-fmb-target",
                0x0193_84A0_FACE_7000_8000_0000_0000_0003 ^ u128::from(target_seed),
            ).await;

            let ghost_digest = digest_from_seed(&ghost_digest_seed);

            let orch = FindMissingOrchestrator::new(&meta);

            // Tenant B asks: should see BOTH as missing (target_digest is
            // owned by A; ghost_digest never existed).
            let actor_outcome = orch
                .find_missing(&ctx_actor, &[target_digest, ghost_digest])
                .await
                .unwrap();
            prop_assert_eq!(actor_outcome.slot_is_missing.len(), 2);
            prop_assert!(
                actor_outcome.slot_is_missing[0],
                "tenant A's digest must surface as missing under tenant B's ctx"
            );
            prop_assert!(
                actor_outcome.slot_is_missing[1],
                "never-existed digest must surface as missing"
            );

            // Tenant A asks the same batch: should see ONLY ghost_digest
            // as missing (their own digest is present).
            let target_outcome = orch
                .find_missing(&ctx_target, &[target_digest, ghost_digest])
                .await
                .unwrap();
            prop_assert_eq!(target_outcome.slot_is_missing.len(), 2);
            prop_assert!(
                !target_outcome.slot_is_missing[0],
                "tenant A's own digest must surface as present"
            );
            prop_assert!(
                target_outcome.slot_is_missing[1],
                "ghost_digest must always surface as missing"
            );

            // Indistinguishability: tenant B's response must surface
            // EXACTLY the same (`missing` Vec, `slot_is_missing` Vec)
            // shape as if `target_digest` had genuinely never been
            // written. We verify by running the same query against a
            // FRESH meta store (no writes at all) under tenant B and
            // asserting equality.
            let fresh_meta = InMemoryMetaStore::new();
            let fresh_orch = FindMissingOrchestrator::new(&fresh_meta);
            let fresh_outcome = fresh_orch
                .find_missing(&ctx_actor, &[target_digest, ghost_digest])
                .await
                .unwrap();
            prop_assert_eq!(
                actor_outcome.slot_is_missing,
                fresh_outcome.slot_is_missing,
                "FindMissingBlobs must NOT leak existence under cross-tenant query"
            );
            prop_assert_eq!(
                actor_outcome.missing.len(),
                fresh_outcome.missing.len(),
                "cross-tenant `missing` length must match the never-existed scenario"
            );
            Ok(())
        })?;
    }

    /// **WI §8 `prop_negative_cache_correctness` — 10k iter race
    /// scenarios** (aligned with WI-S02-005 §9.11 eventual-consistency
    /// model).
    ///
    /// Mixed sequence:
    /// 1. populate negative cache with `(tenant, digest, NotFound)` →
    /// 2. write blob (S-01 path) →
    /// 3. invalidate cache via `invalidate_on_write` →
    /// 4. lookup cache → MUST be None →
    /// 5. orchestrator-level read → MUST be Hit byte-identical body.
    ///
    /// Asserts: NO incorrect hot read (no 200 OK returning a wrong
    /// content; no cross-tenant cache poisoning); explicit invalidation
    /// is the load-bearing path; transient stale 404s INSIDE the TTL
    /// bound are acceptable per CF KV semantics (we do NOT assert "no
    /// stale negative ever observed" — that would contradict WI-005
    /// §9.11).
    #[test]
    fn prop_negative_cache_correctness(
        body in prop::collection::vec(any::<u8>(), 1..128usize),
        tenant_seed in any::<u64>(),
        attacker_seed in any::<u64>(),
    ) {
        prop_assume!(tenant_seed != attacker_seed);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
            let ctx = ctx_for(tenant_uuid(tenant_seed), Region::Wnam);
            let attacker_ctx = ctx_for(tenant_uuid(attacker_seed), Region::Wnam);

            let digest = Digest::compute(&body);

            // Step 1 — populate cache as missing.
            cache.put_miss(&ctx, &digest, CacheMiss::NotFound).await.unwrap();
            prop_assert_eq!(
                cache.lookup(&ctx, &digest).await.unwrap(),
                Some(CacheMiss::NotFound)
            );

            // Cross-tenant cache isolation (poisoning prevention).
            prop_assert_eq!(
                cache.lookup(&attacker_ctx, &digest).await.unwrap(),
                None,
                "cross-tenant cache poisoning detected"
            );

            // Step 2 — write blob.
            let _ = write_blob(
                &backend,
                &meta,
                &ctx,
                &body,
                "req-neg-cache",
                0x0193_84A0_FACE_7000_8000_0000_0000_0004 ^ u128::from(tenant_seed),
            ).await;

            // Step 3 — invalidate.
            cache.invalidate_on_write(&ctx, &digest).await.unwrap();

            // Step 4 — cache lookup must surface None post-invalidation.
            prop_assert_eq!(
                cache.lookup(&ctx, &digest).await.unwrap(),
                None,
                "invalidate_on_write did not clear the entry"
            );

            // Step 5 — orchestrator-level read MUST Hit (no stale 404).
            let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let out = orch.read_blob(&ctx, &digest).await.unwrap();
            match out {
                ReadOutcome::Hit { body: out_body, .. } => {
                    prop_assert_eq!(out_body.as_ref(), body.as_slice());
                }
                other => prop_assert!(false, "post-invalidate read must Hit; got {other:?}"),
            }

            // Cross-tenant must STILL see uniform 404 (cache invalidation
            // for tenant T does NOT leak to attacker tenant).
            let out_attacker = orch.read_blob(&attacker_ctx, &digest).await.unwrap();
            prop_assert!(matches!(
                out_attacker,
                ReadOutcome::NotFound(ReadMiss::NeverExisted)
            ));
            Ok(())
        })?;
    }

    /// **Tombstone read surfaces uniform 404 (`Tombstoned`).**
    ///
    /// INV-CAS-IMMUTABILITY at the read seam: a soft-deleted blob
    /// (`deleted_at IS NOT NULL`) MUST surface
    /// `ReadOutcome::NotFound(MissReason::Tombstoned)` and MUST NOT fire
    /// an R2 GET (forensic-only on the wire; the audit chain emits
    /// `corelink.cas.read_miss` with the variant for offline
    /// reclassification per ADR-0028).
    #[test]
    fn prop_tombstone_returns_uniform_not_found(
        body in prop::collection::vec(any::<u8>(), 1..128usize),
        tenant_seed in any::<u64>(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let inner = Arc::new(InMemoryR2::new());
            let counting = Arc::new(CountingR2::new(Arc::clone(&inner)));
            let meta = InMemoryMetaStore::new();
            let ctx = ctx_for(tenant_uuid(tenant_seed), Region::Wnam);

            let digest = Digest::compute(&body);
            let writer = R2Writer::new(Region::Wnam, Arc::clone(&counting));
            let vb = VerifiedBody::new(Bytes::copy_from_slice(&body), digest).unwrap();
            writer.put(&ctx, &vb).await.unwrap();
            let key = BlobMetaKey::new(ctx.tenant_id(), digest);
            meta.commit_put(CommitPutRequest {
                key,
                size_bytes: body.len() as u64,
                now_ms: 1_700_000_000_000,
                audit: AuditEvent {
                    id: Uuid::from_u128(0x0193_84A0_FACE_7000_8000_0000_0000_0005 ^ u128::from(tenant_seed)),
                    request_id: RequestId::new("req-tombstone-write"),
                    event_type: AuditEventType::CasPutCompleted,
                    payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
                },
            }).await.unwrap();

            // Soft-delete via the canonical schema path.
            meta.commit_soft_delete(CommitSoftDeleteRequest {
                key,
                now_ms: 1_700_000_001_000,
                audit: AuditEvent {
                    id: Uuid::from_u128(0x0193_84A0_FACE_7000_8000_0000_0000_0006 ^ u128::from(tenant_seed)),
                    request_id: RequestId::new("req-tombstone"),
                    event_type: AuditEventType::CasSoftDeleted,
                    payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
                },
            }).await.unwrap();

            let get_calls_baseline = counting.get_calls();
            let reader = R2Reader::new(Region::Wnam, Arc::clone(&counting));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let out = orch.read_blob(&ctx, &digest).await.unwrap();
            prop_assert!(matches!(
                out,
                ReadOutcome::NotFound(ReadMiss::Tombstoned)
            ));
            // Tombstoned read must NOT touch R2 — the meta-layer check
            // is the load-bearing short-circuit.
            prop_assert_eq!(
                counting.get_calls(),
                get_calls_baseline,
                "tombstoned read fired R2 GET; meta short-circuit broken"
            );
            Ok(())
        })?;
    }
}

// ---------------------------------------------------------------------------
// Round-Robin 100k cross-tenant attempt schedule
// ---------------------------------------------------------------------------

/// **WI §2 narrative + §10.6.1 `prop_tenant_isolation_read_path` —
/// 100k iter via deterministic round-robin schedule.**
///
/// 50 actor tenants × 50 target tenants × multiple rotations. Per WI
/// §2: every distinct `(actor_i, target_j)` pair with `i != j` MUST be
/// exercised ≥ 40 times. With 50 × 50 − 50 = 2 450 cross-pairs and
/// 100 000 iter total budget, the bound is `100_000 / 2_450 ≈ 40.8`
/// minimum reps per pair. We assert the coverage matrix at the end of
/// the loop so a regression that under-explored a (i, j) pair surfaces
/// as a coverage failure rather than silently sample-skip.
///
/// Schedule (per WI §2 narrative, deterministic):
/// - Iteration `i ∈ [0 .. 100_000)`.
/// - `actor_idx = i mod 50`.
/// - `op_within_iter` derived from a 50-deep sub-loop;
///   `target_idx = (i / 50 + op_within_iter) mod 50`.
/// - Skip the diagonal (`actor == target`, no cross-tenant) and emit
///   the schedule until the 100k budget is exhausted.
///
/// We use `tokio::test(flavor = "multi_thread")` to exercise the AuthZ
/// + R2 GET seam under concurrent dispatch — the coverage matrix is
/// computed AFTER all spawned tasks complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cross_tenant_read_round_robin_100k() {
    use futures::future::join_all;

    const TENANTS: usize = 50;
    const ITER: usize = 100_000;
    // Pair coverage budget: with `TENANTS * TENANTS - TENANTS` distinct
    // ordered cross-pairs and `ITER` total dispatches, the minimum reps
    // per pair is `ITER / (TENANTS * (TENANTS - 1))`.
    const CROSS_PAIRS: usize = TENANTS * (TENANTS - 1);
    let min_reps = ITER / CROSS_PAIRS;
    assert!(min_reps >= 40, "coverage bound dropped below WI §2 floor");

    // Seed all tenants + bodies once; every iteration reuses the
    // pre-populated state (write side is not under test in this
    // scenario; only the read seam).
    let backend = Arc::new(InMemoryR2::new());
    let counting = Arc::new(CountingR2::new(Arc::clone(&backend)));
    let meta = Arc::new(InMemoryMetaStore::new());

    let mut tenants: Vec<TenantCtx> = Vec::with_capacity(TENANTS);
    let mut digests: Vec<Digest> = Vec::with_capacity(TENANTS);
    for t in 0..TENANTS {
        // Use a wide spread so HMAC16 prefixes never collide.
        let ctx = ctx_for(tenant_uuid((t as u64) + 0xCAFE_F00D_0000_0000), Region::Wnam);
        let body: Vec<u8> = (0..128u8).map(|i| i.wrapping_add(t as u8)).collect();
        let digest = Digest::compute(&body);
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&counting));
        let vb = VerifiedBody::new(Bytes::copy_from_slice(&body), digest).unwrap();
        writer.put(&ctx, &vb).await.unwrap();
        let key = BlobMetaKey::new(ctx.tenant_id(), digest);
        meta.commit_put(CommitPutRequest {
            key,
            size_bytes: body.len() as u64,
            now_ms: 1_700_000_000_000,
            audit: AuditEvent {
                id: Uuid::from_u128(0x0193_84A0_FACE_7000_8000_0000_0000_0010 + t as u128),
                request_id: RequestId::new(format!("req-rr-victim-{t}")),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
            },
        })
        .await
        .unwrap();
        tenants.push(ctx);
        digests.push(digest);
    }
    assert_eq!(counting.put_calls(), TENANTS);
    let get_calls_baseline = counting.get_calls();

    // Build the round-robin schedule.
    let mut schedule: Vec<(usize, usize)> = Vec::with_capacity(ITER);
    let mut iter_idx = 0usize;
    while schedule.len() < ITER {
        let actor = iter_idx % TENANTS;
        for op in 0..TENANTS {
            let target = ((iter_idx / TENANTS) + op) % TENANTS;
            if actor == target {
                continue;
            }
            schedule.push((actor, target));
            if schedule.len() == ITER {
                break;
            }
        }
        iter_idx += 1;
    }
    assert_eq!(schedule.len(), ITER);

    // Coverage matrix: (actor, target) → count.
    let mut coverage: HashMap<(usize, usize), u32> = HashMap::with_capacity(CROSS_PAIRS);
    for &(a, t) in &schedule {
        *coverage.entry((a, t)).or_insert(0) += 1;
    }
    assert_eq!(
        coverage.len(),
        CROSS_PAIRS,
        "round-robin schedule must cover every cross-pair"
    );
    let min_actual = coverage.values().min().copied().unwrap_or(0);
    assert!(
        min_actual as usize >= min_reps,
        "minimum reps per pair fell below WI §2 floor: got {min_actual}, want ≥ {min_reps}"
    );

    // Concurrent dispatch via `join_all`. We chunk into 4 batches of
    // 25k to keep the JoinSet memory bounded.
    let reader = Arc::new(R2Reader::new(Region::Wnam, Arc::clone(&counting)));
    let chunk_size = ITER / 4;
    let mut leaks = 0usize;
    let mut ok_misses = 0usize;
    let mut other_misses = BTreeMap::<&'static str, usize>::new();
    for chunk in 0..4 {
        let start = chunk * chunk_size;
        let end = if chunk == 3 { ITER } else { start + chunk_size };
        let mut handles = Vec::with_capacity(end - start);
        for &(actor, target) in &schedule[start..end] {
            let actor_ctx = tenants[actor];
            let target_digest = digests[target];
            let reader = Arc::clone(&reader);
            let meta = Arc::clone(&meta);
            handles.push(tokio::spawn(async move {
                let orch = CasReadOrchestrator::new(reader.as_ref(), meta.as_ref());
                orch.read_blob(&actor_ctx, &target_digest).await
            }));
        }
        let outcomes = join_all(handles).await;
        for r in outcomes {
            let outcome = r.expect("task did not panic").expect("orchestrator must not error");
            match outcome {
                ReadOutcome::Hit { .. } => leaks += 1,
                ReadOutcome::NotFound(ReadMiss::NeverExisted) => ok_misses += 1,
                ReadOutcome::NotFound(ReadMiss::Tombstoned) => {
                    *other_misses.entry("tombstoned").or_insert(0) += 1;
                }
                ReadOutcome::NotFound(ReadMiss::R2OrphanRow) => {
                    *other_misses.entry("r2_orphan_row").or_insert(0) += 1;
                }
            }
        }
    }
    assert_eq!(
        leaks, 0,
        "cross-tenant read leak: {leaks}/{ITER} attempts surfaced a Hit"
    );
    assert_eq!(
        ok_misses, ITER,
        "all attempts must be uniform NeverExisted; other_misses={other_misses:?}"
    );
    // R2 GET MUST never fire on the cross-tenant branch (every actor
    // queries a digest stored under a different tenant's prefix; the
    // AuthZ check short-circuits at the meta layer).
    assert_eq!(
        counting.get_calls(),
        get_calls_baseline,
        "R2 GET fired during cross-tenant attempts; AuthZ short-circuit broken"
    );
}

// ---------------------------------------------------------------------------
// Negative cache TTL bound regression vector
// ---------------------------------------------------------------------------

/// **TTL bound regression**: the negative cache evicts at the exact TTL
/// boundary; a write+read sequence after TTL expiry returns the fresh
/// blob (no stale 404). This pin matches WI-S02-005 §9.11 +
/// `prop_neg_cache::ttl_boundary_exact_secs_evicts` but exercises the
/// integrated read-path stack rather than the cache adapter in
/// isolation.
#[tokio::test]
async fn negative_cache_ttl_boundary_does_not_block_post_write_read() {
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let clock = FakeClock::new(1_000_000);
    let kv = InMemoryKv::with_clock(clock);
    let cache = NegativeCache::new(Region::Wnam, kv);
    let ctx = ctx_for(tenant_uuid(0xDEAD_BEEF), Region::Wnam);
    let body = b"ttl-boundary-body".to_vec();
    let digest = Digest::compute(&body);

    // Populate negative; assert hit.
    cache
        .put_miss(&ctx, &digest, CacheMiss::NotFound)
        .await
        .unwrap();
    assert_eq!(
        cache.lookup(&ctx, &digest).await.unwrap(),
        Some(CacheMiss::NotFound)
    );

    // Advance clock past TTL → entry evicts.
    cache
        .backend()
        .clock()
        .advance(Duration::from_secs(DEFAULT_NEGATIVE_CACHE_TTL_SECS + 1));
    assert_eq!(
        cache.lookup(&ctx, &digest).await.unwrap(),
        None,
        "TTL-expired entry must evict"
    );

    // Now write the blob and read it; the read MUST Hit (the stale
    // negative is gone).
    let _ = write_blob(
        &backend,
        &meta,
        &ctx,
        &body,
        "req-ttl-boundary",
        0x0193_84A0_FACE_7000_8000_0000_0000_0007,
    )
    .await;

    let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
    let orch = CasReadOrchestrator::new(&reader, &meta);
    let out = orch.read_blob(&ctx, &digest).await.unwrap();
    match out {
        ReadOutcome::Hit { body: out_body, .. } => {
            assert_eq!(out_body.as_ref(), body.as_slice());
        }
        other => panic!("post-TTL read must Hit; got {other:?}"),
    }
}
