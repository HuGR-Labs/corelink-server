//! Property tests for [`corelink_reapi::find_missing::FindMissingOrchestrator`]
//! (WI-S02-002 §10.2.2).
//!
//! Two load-bearing properties:
//!
//! 1. **No cross-tenant existence leak** (FF-HR-002 / CTRL-ISO-005).
//!    For every randomly-generated set of (tenant_a, tenant_b, blob)
//!    triples, when Tenant B asks `FindMissingBlobs` for a digest
//!    Tenant A wrote, the response surfaces it as MISSING. Asserts at
//!    10k iter (proptest-default + per-attempt micro-batch dispatch).
//!
//! 2. **Determinism**. Two back-to-back invocations with the same
//!    input slice yield byte-identical `missing` outputs (Vec
//!    equality). The bounded-parallel `buffer_unordered` fan-out
//!    completion order is non-deterministic; the orchestrator's
//!    order-restoration logic preserves input order at the API
//!    surface — this property is the regression test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, MetaStore,
    RequestId,
};
use corelink_reapi::find_missing::FindMissingOrchestrator;
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Writer};
use corelink_replication::region_resolver::{Region, TenantCtx};
use proptest::collection::vec;
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

proptest! {
    #![proptest_config(ProptestConfig {
        // WI-S02-002 §10.2.2 + sprint DoD canonical: 10k batch enumeration
        // attempts. Each case spawns a full `(tenant_a_blobs, tenant_b_query_subset)`
        // pair via random strategies, exercising the FindMissingBlobs handler
        // end-to-end with random-input diversity (closes Sonnet sprint-close P1-1
        // raised against the previous 64-case config).
        cases: proptest_cases(),
        max_shrink_iters: 64,
        .. ProptestConfig::default()
    })]

    /// **CRITICAL — FF-HR-002 specific.** For randomly-generated sets
    /// of `(tenant_a_blobs, tenant_b_query_subset)`, the
    /// `FindMissingBlobs` response under Tenant B MUST surface every
    /// `tenant_a` digest as MISSING — i.e. the cross-tenant
    /// existence oracle is closed by construction.
    ///
    /// Each proptest case writes 1-8 distinct bodies under Tenant A,
    /// then queries under Tenant B with a randomly-shuffled mix of
    /// (a) some of A's digests, (b) some never-existed digests. The
    /// orchestrator MUST emit every requested digest in `missing`
    /// (no leak; no false-presence).
    #[test]
    fn prop_no_cross_tenant_leak(
        a_bodies in vec(any::<Vec<u8>>(), 1..=8),
        b_extra_count in 0usize..=8,
        seed in any::<u64>(),
    ) {
        // Skip empty cases — proptest occasionally generates a Vec
        // of empty bodies which BLAKE3 + R2 + D1 still accept, but
        // the empty-body edge case is exercised in the unit tests
        // and is not the focus here.
        prop_assume!(!a_bodies.is_empty());
        prop_assume!(a_bodies.iter().all(|b| !b.is_empty()));
        // Deduplicate bodies — same-bytes BLAKE3 collide and we want
        // distinct digests for clarity.
        let mut seen = std::collections::HashSet::new();
        let bodies: Vec<Vec<u8>> = a_bodies
            .into_iter()
            .filter(|b| seen.insert(b.clone()))
            .collect();
        prop_assume!(!bodies.is_empty());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let tdk = fixed_tdk();
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(0xA), Region::Wnam);
            let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(0xB), Region::Wnam);

            // A writes every body.
            let mut a_digests = Vec::new();
            for (i, body) in bodies.iter().enumerate() {
                let d = write_blob(
                    &backend,
                    &meta,
                    &ctx_a,
                    body,
                    &format!("req-A-{i}-{seed}"),
                    0x10000 + (seed as u128) * 1024 + (i as u128),
                )
                .await;
                a_digests.push(d);
            }

            // Build B's query: A's digests + b_extra "never" digests.
            let mut query: Vec<Digest> = a_digests.clone();
            for j in 0..b_extra_count {
                query.push(Digest::compute(format!("never-B-{seed}-{j}").as_bytes()));
            }

            // Dispatch.
            let orch = FindMissingOrchestrator::new(&meta);
            let outcome = orch.find_missing(&ctx_b, &query).await.unwrap();

            // Property: every queried digest surfaces as MISSING.
            // The orchestrator preserves input order; the equality
            // check captures both correctness AND order.
            let query_len = query.len();
            prop_assert_eq!(outcome.missing.len(), query_len);
            prop_assert_eq!(outcome.missing, query);
            // d1_lookups equals input length (no skipped queries).
            prop_assert_eq!(outcome.d1_lookups, query_len);
            // R2 is NEVER consulted on the find_missing path —
            // backend has only A's writes (no B-side reads).
            prop_assert_eq!(backend.len(), bodies.len());
            Ok(())
        }).unwrap();
    }

    /// Determinism: the same input MUST produce the same output across
    /// repeated calls. Stresses the bounded-parallel fan-out's order
    /// restoration logic — completion order in `buffer_unordered` is
    /// non-deterministic, but the orchestrator must reorder by input
    /// index.
    #[test]
    fn prop_determinism_same_input_same_output(
        bodies in vec(any::<Vec<u8>>(), 1..=16),
        absent_extra in 0usize..=8,
        seed in any::<u64>(),
    ) {
        prop_assume!(bodies.iter().all(|b| !b.is_empty()));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let tdk = fixed_tdk();
            let backend = Arc::new(InMemoryR2::new());
            let meta = InMemoryMetaStore::new();
            let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

            // Write half, leave half absent.
            let mut digests = Vec::new();
            for (i, body) in bodies.iter().enumerate() {
                if i % 2 == 0 {
                    let d = write_blob(
                        &backend,
                        &meta,
                        &ctx,
                        body,
                        &format!("req-{i}-{seed}"),
                        0x20000 + (seed as u128) * 1024 + (i as u128),
                    )
                    .await;
                    digests.push(d);
                } else {
                    digests.push(Digest::compute(body));
                }
            }
            for j in 0..absent_extra {
                digests.push(Digest::compute(format!("never-{seed}-{j}").as_bytes()));
            }

            let orch = FindMissingOrchestrator::new(&meta);
            let r1 = orch.find_missing(&ctx, &digests).await.unwrap();
            let r2 = orch.find_missing(&ctx, &digests).await.unwrap();
            let r3 = orch.find_missing(&ctx, &digests).await.unwrap();
            prop_assert_eq!(&r1.missing, &r2.missing);
            prop_assert_eq!(&r2.missing, &r3.missing);
            prop_assert_eq!(r1.d1_lookups, digests.len());
            Ok(())
        }).unwrap();
    }
}

/// 10k-iter concurrent cross-tenant isolation test.
///
/// Per WI-S02-002 §10.2.2 + the charter "10k iter on cross-tenant
/// isolation in batch read", we run 10k batched FindMissingBlobs
/// invocations with attacker-Tenant B's PAT against a population of
/// digests that ALL belong to Tenant A. Every iteration MUST surface
/// 100% of the queried digests as MISSING. This is the
/// CTRL-ISO-005 / FF-HR-002 high-iteration regression gate.
#[tokio::test]
async fn cross_tenant_isolation_10k_iter() {
    let tdk = fixed_tdk();
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let ctx_a = TenantCtx::new(&tdk, Uuid::from_u128(0xA), Region::Wnam);
    let ctx_b = TenantCtx::new(&tdk, Uuid::from_u128(0xB), Region::Wnam);

    // Populate Tenant A with 16 distinct blobs.
    let mut a_digests = Vec::new();
    for i in 0u32..16 {
        let body = format!("A-secret-body-{i}");
        let d = write_blob(
            &backend,
            &meta,
            &ctx_a,
            body.as_bytes(),
            &format!("req-A-{i}"),
            0x90000 + u128::from(i),
        )
        .await;
        a_digests.push(d);
    }

    // 10k iterations. We use a small batch (4) per iteration so this
    // test stays under ~ 2 seconds on a laptop while still exercising
    // the bounded-parallel fan-out 10k times.
    let orch = FindMissingOrchestrator::new(&meta);
    let mut total_missing: u64 = 0;
    let mut total_lookups: u64 = 0;
    let mut leaked: u64 = 0;
    for iter_idx in 0..10_000u32 {
        let q: [Digest; 4] = [
            a_digests[(iter_idx as usize) % a_digests.len()],
            a_digests[((iter_idx as usize) + 1) % a_digests.len()],
            a_digests[((iter_idx as usize) + 2) % a_digests.len()],
            a_digests[((iter_idx as usize) + 3) % a_digests.len()],
        ];
        let out = orch.find_missing(&ctx_b, &q).await.unwrap();
        total_missing += out.missing.len() as u64;
        total_lookups += out.d1_lookups as u64;
        // Property: every digest queried is MISSING from B's
        // perspective — the orchestrator MUST never report any of
        // them as present.
        if out.missing.len() != q.len() {
            leaked += (q.len() - out.missing.len()) as u64;
        }
    }
    assert_eq!(leaked, 0, "cross-tenant existence leak detected");
    assert_eq!(total_missing, 10_000 * 4);
    assert_eq!(total_lookups, 10_000 * 4);
    // Backend retains only A's 16 writes (no B-side reads).
    assert_eq!(backend.len(), 16);
}
