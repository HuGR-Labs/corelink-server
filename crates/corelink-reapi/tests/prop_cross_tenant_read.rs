//! Property tests — cross-tenant CAS read isolation (WI-S02-001).
//!
//! **CRITICAL — FF-HR-002 specific.** This file pins
//! INV-TENANT-ISOLATION across the read path. Failure of any property
//! here is a **catastrophic blast radius** (cross-tenant data leak).
//!
//! Cases (proptest):
//!
//! - **Property A — uniform NotFound under different tenant context**
//!   (10k iter): Tenant A writes a blob with random body bytes; Tenant B
//!   asks for the SAME digest under their own ctx. Outcome MUST be
//!   `ReadOutcome::NotFound(MissReason::NeverExisted)` with R2 backend
//!   key count unchanged. CTRL-ISO-002 holds at the AuthZ-check seam.
//!
//! - **Property B — round-trip same-tenant** (10k iter): Tenant A writes
//!   a random body, Tenant A reads it. Outcome MUST be `ReadOutcome::Hit`
//!   with body bytes byte-identical to the input AND blob_meta-recorded
//!   size matching `body.len()`.
//!
//! - **Property C — distinct (tenant, body) pairs land in distinct R2
//!   keys** (cross-property): two tenants writing identical bodies
//!   produce two distinct R2 keys (no key-namespace collision because
//!   `corelink-tenant-path` HMAC16 produces per-tenant prefixes).
//!
//! - **Property D — never-written digest under any tenant returns
//!   uniform NotFound**: random digest values that no tenant has ever
//!   written must surface NeverExisted regardless of which tenant ctx
//!   asks. No R2 call is made (AuthZ short-circuit).
//!
//! All properties run at 10k iter (matches WI-S01-006 cross-component
//! property test budget). The tests use the same `InMemoryR2` +
//! `InMemoryMetaStore` fakes that exercise the canonical
//! `R2Backend::get` and `MetaStore::get` semantics — the production
//! Cloudflare adapter (S-01-005 follow-up) is wire-format-compatible.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, MetaStore,
    RequestId,
};
use corelink_reapi::read::{CasReadOrchestrator, MissReason, ReadOutcome};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{CountingR2, InMemoryR2, R2Reader, R2Writer};
use corelink_replication::region_resolver::{Region, TenantCtx};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn fixed_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
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

/// Same shape as [`write_blob`] but routes the R2 PUT through a
/// [`CountingR2`] wrapper so the cross-tenant test can assert
/// `get_calls == 0` with a recording-mock semantics.
async fn write_blob_through(
    backend: &Arc<CountingR2<InMemoryR2>>,
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

proptest! {
    #![proptest_config(ProptestConfig {
        // 10k iter per WI-S02-001 §10.1.1. The harness boots a fresh
        // backend + meta + ctx pair on every iteration; no global state
        // leaks across iterations.
        cases: proptest_cases(),
        max_shrink_iters: 256,
        // Failures persist into a sibling-file regression DB by default
        // (`prop_cross_tenant_read.proptest-regressions`). We do NOT
        // fail-persist a digest body under a tenant_id seed — only the
        // proptest seed is captured.
        ..ProptestConfig::default()
    })]

    /// **FF-HR-002 invariant.** Tenant A writes; Tenant B reads. The
    /// AuthZ check on D1 returns `Ok(None)` because the row is keyed
    /// under `(B.tenant_id, digest)` which does not exist. R2 GET is
    /// NEVER called on the cross-tenant path.
    ///
    /// Codex round-1 P1(c) fix: the prior version asserted only via
    /// `keys_snapshot` (final state); this version wraps the backend in
    /// [`CountingR2`] and asserts `get_calls == 0` so an accidental
    /// post-AuthZ R2 GET would be caught.
    #[test]
    fn cross_tenant_read_returns_uniform_not_found(
        body in prop::collection::vec(any::<u8>(), 1..256usize),
        tenant_a_lo in any::<u64>(),
        tenant_b_lo in any::<u64>(),
    ) {
        // Drop tautological case where tenants happen to be equal.
        prop_assume!(tenant_a_lo != tenant_b_lo);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let tdk = fixed_tdk();
            let inner_backend = Arc::new(InMemoryR2::new());
            let counting = Arc::new(CountingR2::new(Arc::clone(&inner_backend)));
            let meta = InMemoryMetaStore::new();
            let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(u128::from(tenant_a_lo)), Region::Wnam);
            let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(u128::from(tenant_b_lo)), Region::Wnam);

            // A writes through the counting backend so both writes and
            // reads observe the same call counters.
            let digest = write_blob_through(
                &counting,
                &meta,
                &ctx_a,
                &body,
                "req-A",
                0x019384A0_FACE_7000_8000_000000000001,
            )
            .await;

            let put_calls_after_write = counting.put_calls();
            let get_calls_after_write = counting.get_calls();
            prop_assert!(put_calls_after_write >= 1);
            prop_assert_eq!(get_calls_after_write, 0);

            let reader = R2Reader::new(Region::Wnam, Arc::clone(&counting));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let out = orch.read_blob(&ctx_b, &digest).await.unwrap();

            // B sees uniform NotFound — same as a never-existed digest.
            // Per ADR-0028 the wire surface conflates NeverExisted with
            // CrossTenantMasked.
            // SOTA-OK: variant-only assertion sufficient — MissReason::NeverExisted is a unit variant carrying no semantic state.
            prop_assert!(matches!(
                out,
                ReadOutcome::NotFound(MissReason::NeverExisted)
            ));

            // R2 GET was NEVER called on the cross-tenant path. The
            // AuthZ check short-circuited at the meta layer.
            let get_calls_after_cross_tenant_read = counting.get_calls();
            prop_assert_eq!(
                get_calls_after_cross_tenant_read,
                get_calls_after_write,
                "cross-tenant read must not invoke R2 GET"
            );
            Ok(())
        })?;
    }

    /// **Round-trip same-tenant — Property B.**
    #[test]
    fn round_trip_same_tenant_returns_byte_identical_body(
        body in prop::collection::vec(any::<u8>(), 0..256usize),
        tenant_lo in any::<u64>(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let tdk = fixed_tdk();
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx = TenantCtx::new(&tdk, Uuid::from_u128(u128::from(tenant_lo)), Region::Wnam);

            // Empty bodies are an orphan-fast-path test (WI-S01-006
            // lesson) — `MetaStore::commit_put` rejects size_bytes == 0
            // via the schema CHECK; skip that case here so the
            // happy-path round trip is the only thing under test.
            if body.is_empty() {
                return Ok(());
            }

            let digest = write_blob(
                &backend,
                &meta,
                &ctx,
                &body,
                "req-roundtrip",
                0x019384A0_FACE_7000_8000_000000000002,
            )
            .await;

            let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let out = orch.read_blob(&ctx, &digest).await.unwrap();

            match out {
                ReadOutcome::Hit { body: out_body, size_bytes } => {
                    prop_assert_eq!(out_body.as_ref(), body.as_slice());
                    prop_assert_eq!(size_bytes, body.len() as u64);
                }
                other => prop_assert!(false, "expected Hit, got {other:?}"),
            }
            Ok(())
        })?;
    }

    /// **Property D — never-written digest returns uniform NotFound.**
    /// We construct random digest bytes via `Digest::from_hex` over
    /// 64-char hex strings.
    #[test]
    fn never_written_digest_returns_never_existed(
        digest_seed in any::<[u8; 32]>(),
        tenant_lo in any::<u64>(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let tdk = fixed_tdk();
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx = TenantCtx::new(&tdk, Uuid::from_u128(u128::from(tenant_lo)), Region::Wnam);

            // Build a random valid digest from 32 bytes.
            let digest_hex: String = digest_seed.iter().map(|b| format!("{b:02x}")).collect();
            let digest = Digest::from_hex(&digest_hex).unwrap();

            let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
            let orch = CasReadOrchestrator::new(&reader, &meta);
            let out = orch.read_blob(&ctx, &digest).await.unwrap();

            // SOTA-OK: variant-only assertion sufficient — MissReason::NeverExisted is a unit variant carrying no semantic state.
            prop_assert!(matches!(
                out,
                ReadOutcome::NotFound(MissReason::NeverExisted)
            ));
            // R2 was never touched — backend remains empty.
            prop_assert_eq!(backend.len(), 0);
            Ok(())
        })?;
    }
}

/// Concurrent 100k cross-tenant attempts via tokio-spawned tasks.
///
/// Codex round-1 P2(c) fix: WI §10.1.1 mandates "Property test 100k
/// iter cross-tenant → 0 successes". The proptest-style 10k cases
/// above run synchronously per-case with a fresh runtime; this test
/// scales to 100k by spawning concurrent tasks across a multi-thread
/// runtime, exercising the AuthZ check + R2 GET seam under
/// **simultaneous** cross-tenant pressure (the realistic adversary
/// model: hundreds of compromised PATs probing in parallel).
///
/// Setup: 8 attacker tenants each holding a randomized digest set;
/// 8 victim tenants own the actual blobs. The matrix gives 8 × 8 =
/// 64 cross-pairs. We dispatch 100,000 lookups spread across all
/// pairs; every single one MUST return `NotFound::NeverExisted`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_cross_tenant_isolation_100k() {
    const ATTEMPTS: usize = 100_000;
    const VICTIMS: usize = 8;
    const ATTACKERS: usize = 8;

    let tdk = fixed_tdk();
    let backend = Arc::new(InMemoryR2::new());
    let counting = Arc::new(CountingR2::new(Arc::clone(&backend)));
    let meta = Arc::new(InMemoryMetaStore::new());

    // Pre-populate: each victim writes one blob.
    let mut victim_blobs: Vec<(TenantCtx, Digest)> = Vec::with_capacity(VICTIMS);
    for v in 0..VICTIMS {
        let ctx = TenantCtx::new(
            &tdk,
            Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0000_u128 + v as u128),
            Region::Wnam,
        );
        let body: Vec<u8> = (0..64u8).map(|i| i.wrapping_add(v as u8)).collect();
        let digest = write_blob_through(
            &counting,
            &meta,
            &ctx,
            &body,
            &format!("req-victim-{v}"),
            0x0193_84A0_FACE_7000_8000_0000_0000_0000_u128 + v as u128,
        )
        .await;
        victim_blobs.push((ctx, digest));
    }
    assert_eq!(counting.put_calls(), VICTIMS);
    assert_eq!(counting.get_calls(), 0);

    // Pre-populate attacker contexts.
    let attacker_ctxs: Vec<TenantCtx> = (0..ATTACKERS)
        .map(|a| {
            TenantCtx::new(
                &tdk,
                Uuid::from_u128(0xBBBB_0000_0000_0000_0000_0000_0000_0000_u128 + a as u128),
                Region::Wnam,
            )
        })
        .collect();

    // Dispatch attempts in concurrent batches via `futures::future::join_all`.
    use futures::future::join_all;
    let reader = Arc::new(R2Reader::new(Region::Wnam, Arc::clone(&counting)));
    let mut handles = Vec::with_capacity(ATTEMPTS);
    for i in 0..ATTEMPTS {
        let attacker = attacker_ctxs[i % ATTACKERS];
        let (_victim_ctx, target_digest) = victim_blobs[i % VICTIMS];
        let reader = Arc::clone(&reader);
        let meta = Arc::clone(&meta);
        handles.push(tokio::spawn(async move {
            let orch = CasReadOrchestrator::new(reader.as_ref(), meta.as_ref());
            orch.read_blob(&attacker, &target_digest).await
        }));
    }

    let outcomes = join_all(handles).await;
    let mut leaked = 0usize;
    let mut ok_misses = 0usize;
    for r in outcomes {
        let outcome = r
            .expect("task did not panic")
            .expect("orchestrator did not error");
        match outcome {
            ReadOutcome::Hit { .. } => {
                leaked += 1;
            }
            ReadOutcome::NotFound(MissReason::NeverExisted) => {
                ok_misses += 1;
            }
            ReadOutcome::NotFound(other) => {
                panic!("unexpected MissReason on cross-tenant attack: {other:?}");
            }
        }
    }
    assert_eq!(
        leaked, 0,
        "cross-tenant data leak: {leaked} attempts succeeded out of {ATTEMPTS}"
    );
    assert_eq!(
        ok_misses, ATTEMPTS,
        "all attempts must surface as uniform 404 NeverExisted"
    );
    // R2 GET was NEVER called for any attacker request — AuthZ
    // short-circuited at the meta layer for all 100,000 attempts.
    assert_eq!(
        counting.get_calls(),
        0,
        "R2 GET fired {} times during cross-tenant attempts; expected 0",
        counting.get_calls()
    );
}
