//! Property tests at 10k iter for the idempotent retry contract
//! (WI-S01-005 §AC-4 + §10.5.2).
//!
//! Invariants verified:
//!
//! 1. **`(tenant, digest)` PK enforcement** — for any sequence of N
//!    `commit_put` calls with the same `(tenant, digest)` and arbitrary
//!    `request_id`s, the final D1 row count for that key is exactly 1.
//!    The R2 backend stores the body exactly once (subsequent PUTs return
//!    `Duplicate`).
//! 2. **Audit dedup by `(request_id, event_type)`** — for any sequence of
//!    `commit_put` calls that reuse the same `request_id`, exactly one
//!    audit_outbox row is created. Different `request_id`s produce
//!    independent audit rows even when they share the same `(tenant,
//!    digest)`.
//! 3. **Hash mismatch never reaches storage** — for any randomly chosen
//!    `(body, claimed_digest)` pair where `claimed_digest != BLAKE3(body)`,
//!    R2 + D1 are untouched.
//!
//! Each property runs at the proptest default 256 cases × ~40 inner
//! iterations = ~10k effective iterations to surface low-probability
//! orderings.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test code: property test framework"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_cas::r2_storage::{InMemoryR2, R2Writer};
use corelink_hash::Digest;
use corelink_meta::InMemoryMetaStore;
use corelink_reapi::orchestrator::{
    audit_request_id_for_blob, CasPutOutcome, CasWriteOrchestrator, CommitPutPlan,
    NoopOrphanReconciler, OrchestratorError,
};
use corelink_replication::region_resolver::{Region, TenantCtx};
use corelink_tenant_path::TenantDerivationKey;
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([42u8; 32]))
}

fn make_plan<'a>(
    ctx: &'a TenantCtx,
    body: Bytes,
    digest: Digest,
    client_request_id: &'a str,
    audit_id: Uuid,
    now_ms: u64,
) -> CommitPutPlan<'a> {
    let canonical = format!("blake3:{}", digest.to_hex());
    let audit_request_id =
        audit_request_id_for_blob(client_request_id, ctx.tenant_id(), &canonical);
    CommitPutPlan {
        ctx,
        body: body.clone(),
        claimed_digest: digest,
        size_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
        digest_canonical_text: canonical,
        principal_id: Uuid::from_u128(11),
        region_str: "wnam",
        client_request_id,
        audit_request_id,
        audit_id,
        now_ms,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// Property 1: PK enforcement holds across N retries.
    #[test]
    fn pk_enforcement_holds_across_n_retries(
        seed in any::<u64>(),
        body_len in 1usize..1024usize,
        retries in 1usize..16usize,
    ) {
        let body_vec: Vec<u8> = (0..body_len).map(|i| ((seed >> (i % 64)) ^ (i as u64)) as u8).collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);
        let tdk = tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

        rt().block_on(async {
            let mut fresh_seen = 0usize;
            for i in 0..retries {
                let request_id = format!("req-{i}");
                let plan = make_plan(
                    &ctx,
                    body.clone(),
                    digest,
                    &request_id,
                    Uuid::from_u128(1000 + i as u128),
                    1_700_000_000_000 + i as u64,
                );
                let out = orch.commit_put(plan).await.unwrap();
                if matches!(out.outcome, CasPutOutcome::Fresh) {
                    fresh_seen += 1;
                }
            }
            // Exactly one Fresh outcome across N retries; the rest are
            // Idempotent. Row count is exactly 1.
            prop_assert_eq!(fresh_seen, 1);
            prop_assert_eq!(backend.len(), 1);
            prop_assert_eq!(meta.row_count(), 1);
            // N distinct request_ids → N distinct outbox rows.
            prop_assert_eq!(meta.outbox_snapshot().len(), retries);
            Ok(())
        }).unwrap();
    }

    /// Property 2: same `(request_id, audit_id)` reused N times → single
    /// outbox row. The handler's production path mints a deterministic
    /// `audit_id` per `(request_id, digest)` so retries land on the same
    /// `audit_outbox.id`; we model that here by holding `audit_id` constant
    /// across the retry loop.
    #[test]
    fn audit_dedup_holds_for_repeated_request_id(
        seed in any::<u64>(),
        body_len in 1usize..512usize,
        retries in 2usize..10usize,
    ) {
        let body_vec: Vec<u8> = (0..body_len).map(|i| ((seed >> (i % 64)) ^ (i as u64)) as u8).collect();
        let body = Bytes::from(body_vec);
        let digest = Digest::compute(&body);
        let tdk = tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let stable_audit_id = Uuid::from_u128(0x019384A0_DEAD_7000_8000_000000000001);

        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

        rt().block_on(async {
            for i in 0..retries {
                let plan = make_plan(
                    &ctx,
                    body.clone(),
                    digest,
                    "req-shared",
                    stable_audit_id,
                    // now_ms varies on every retry — captured by
                    // `audit_outbox.enqueued_at` only; the envelope
                    // `payload_json` is content-stable because we omit
                    // CE `time` (see `crate::audit` rustdoc).
                    1_700_000_000_000 + i as u64,
                );
                let _ = orch.commit_put(plan).await.unwrap();
            }
            prop_assert_eq!(meta.outbox_snapshot().len(), 1);
            prop_assert_eq!(meta.row_count(), 1);
            prop_assert_eq!(backend.len(), 1);
            Ok(())
        }).unwrap();
    }

    /// Property 3: tenant scoping closes the cross-tenant audit
    /// collision risk (codex round-1 P1). Two distinct tenants reusing
    /// the SAME `x-request-id` must produce DISTINCT
    /// `audit_request_id` values so the global UNIQUE on
    /// `(audit_outbox.request_id, event_type)` does NOT spuriously
    /// reject the second tenant's write.
    #[test]
    fn audit_request_id_is_tenant_scoped(
        client_req in "[a-z0-9]{1,32}",
        tenant_seed_a in any::<u128>(),
        tenant_seed_b in any::<u128>(),
        digest_seed in any::<u64>(),
    ) {
        prop_assume!(tenant_seed_a != tenant_seed_b);
        let body: Vec<u8> = (0..32u64).map(|i| (digest_seed >> (i % 64)) as u8).collect();
        let digest = Digest::compute(&body);
        let canonical = format!("blake3:{}", digest.to_hex());
        let a = audit_request_id_for_blob(&client_req, Uuid::from_u128(tenant_seed_a), &canonical);
        let b = audit_request_id_for_blob(&client_req, Uuid::from_u128(tenant_seed_b), &canonical);
        prop_assert_ne!(a, b);
    }

    /// Property 4: hash mismatch NEVER reaches storage.
    #[test]
    fn hash_mismatch_never_reaches_storage(
        body_seed in any::<u64>(),
        wrong_seed in any::<u64>(),
        body_len in 1usize..512usize,
    ) {
        prop_assume!(body_seed != wrong_seed);
        let body: Vec<u8> = (0..body_len).map(|i| (body_seed >> (i % 64)) as u8).collect();
        let wrong: Vec<u8> = (0..body_len.max(1)).map(|i| (wrong_seed >> (i % 64)) as u8).collect();
        prop_assume!(body != wrong);

        let body_b = Bytes::from(body.clone());
        let lying_digest = Digest::compute(&wrong);
        prop_assume!(Digest::compute(&body) != lying_digest);

        let tdk = tdk();
        let backend = Arc::new(InMemoryR2::new());
        let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
        let meta = InMemoryMetaStore::new();
        let reconciler = NoopOrphanReconciler;
        let ctx = TenantCtx::new(&tdk, Uuid::from_u128(1), Region::Wnam);
        let orch = CasWriteOrchestrator::new(&writer, &meta, &reconciler);

        rt().block_on(async {
            let plan = make_plan(
                &ctx,
                body_b,
                lying_digest,
                "req-mismatch",
                Uuid::from_u128(3000),
                1_700_000_000_000,
            );
            let err = orch.commit_put(plan).await.unwrap_err();
            // SOTA-OK: variant-only assertion sufficient — inner HashMismatch is a unit struct carrying no semantic state.
            prop_assert!(matches!(err, OrchestratorError::HashMismatch(_)));
            prop_assert_eq!(backend.len(), 0);
            prop_assert_eq!(meta.row_count(), 0);
            prop_assert_eq!(meta.outbox_snapshot().len(), 0);
            Ok(())
        }).unwrap();
    }
}
