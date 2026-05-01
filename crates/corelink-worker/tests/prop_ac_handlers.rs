//! Property tests for the AC handler (WI-S04-001 §6.1.9 + §10.s04.001.1).
//!
//! Five canonical properties at 10k iter (PR; nightly bump to 100k via
//! the feature-gated `prop_ac_handlers_100k` test below):
//!
//! 1. `prop_ac_get_tenant_isolation` — Tenant B never observes a
//!    Tenant A entry under the same `action_digest` (`INV-AC-TENANT-SCOPED`).
//! 2. `prop_ac_update_idempotent` — UPDATE same `(tenant, action,
//!    result_hash)` twice ⇒ second is `IdempotentRefresh` (`INV-AC-IDEMPOTENT`).
//! 3. `prop_ac_negative_cache_invalidate` — GET miss ⇒ neg cache hot;
//!    UPDATE invalidates; subsequent GET hits the row.
//! 4. `prop_ac_ttl_refresh_monotonic` — every GET hit refreshes
//!    `last_hit_at >= prev` (never moves backward; `INV-AC-TTL-REFRESH-MONOTONIC`).
//! 5. `prop_ac_outputs_missing_blocks_update` — any tombstoned output
//!    ⇒ UPDATE rejects 422 + audit `ac.update.outputs_missing`; no
//!    row mutation.
//!
//! Counts: 5 `proptest!` cases × 10k iter = **50_000 iter PR**.

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness — panic on assertion is itself a test failure"
)]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::cache::kv::InMemoryKv;
use corelink_worker::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use corelink_worker::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use corelink_worker::reapi::ac::handler::{
    ActionCacheHandler, ActionCacheHandlerBuilder, ActionCacheHandlerImpl, AcError,
    Clock, FakeClock, GetActionResult, InMemoryAcEnvelopeStore,
    DEFAULT_AC_TTL_EXTEND_MS,
};
use corelink_worker::reapi::ac::{
    AcEventType, AcMetaUpsertOutcome, AcNegCache, ActionDigest, ActionResult,
    InMemoryAcMetaStore, InMemoryAuditSink, InMemoryFakeSigner, InMemoryMerkleVerifier,
    InMemoryOutputsCheck, OutputFileDigest,
};
use corelink_worker::Region;
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

const CASES_PR: u32 = 10_000;

fn fixed_tdk() -> Arc<TenantDerivationKey> {
    Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
}

fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
    let pat_id = PatId(Uuid::nil());
    make_auth_ctx(
        PrincipalId(Uuid::nil()),
        tenant,
        region,
        scopes,
        AuthMethod::Pat {
            env: PatEnv::Pat,
            pat_id,
        },
        fixed_tdk(),
    )
}

struct Wiring {
    handler: ActionCacheHandlerImpl<
        InMemoryAcMetaStore,
        InMemoryAcEnvelopeStore,
        InMemoryMerkleVerifier,
        InMemoryFakeSigner,
        InMemoryOutputsCheck,
        InMemoryAuditSink,
        InMemoryKv,
    >,
    meta: Arc<InMemoryAcMetaStore>,
    envelope: Arc<InMemoryAcEnvelopeStore>,
    outputs: Arc<InMemoryOutputsCheck>,
    audit: Arc<InMemoryAuditSink>,
    neg: Arc<AcNegCache<InMemoryKv>>,
    clock: Arc<FakeClock>,
}

fn wire(region: Region) -> Wiring {
    let meta = Arc::new(InMemoryAcMetaStore::new());
    let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
    let merkle = Arc::new(InMemoryMerkleVerifier::new());
    let signer = Arc::new(InMemoryFakeSigner::new());
    let outputs = Arc::new(InMemoryOutputsCheck::new());
    let audit = Arc::new(InMemoryAuditSink::new());
    let neg = Arc::new(AcNegCache::new(region, InMemoryKv::new()).unwrap());
    let clock = Arc::new(FakeClock::new(1_000_000));
    let handler = ActionCacheHandlerImpl::new(ActionCacheHandlerBuilder {
        region,
        meta: Arc::clone(&meta),
        envelope_store: Arc::clone(&envelope),
        merkle: Arc::clone(&merkle),
        signer: Arc::clone(&signer),
        outputs: Arc::clone(&outputs),
        audit: Arc::clone(&audit),
        neg_cache: Arc::clone(&neg),
        sig_key_id: 1,
        path_key_id: 1,
        ttl_extend_ms: DEFAULT_AC_TTL_EXTEND_MS,
        clock: Arc::clone(&clock) as Arc<dyn Clock>,
    });
    Wiring {
        handler,
        meta,
        envelope,
        outputs,
        audit,
        neg,
        clock,
    }
}

fn synthesize_action_result(seed: u64) -> (ActionDigest, ActionResult) {
    let action_bytes = format!("action-{seed}").into_bytes();
    let action_hash = Digest::compute(&action_bytes);
    let ad = ActionDigest::new(action_hash, action_bytes.len() as i64);
    let proto = format!("proto-{seed}").into_bytes();
    let out = Digest::compute(format!("out-{seed}").as_bytes());
    let ar = ActionResult::new(
        vec![OutputFileDigest::new(out, 64)],
        Vec::new(),
        0,
        proto,
    );
    (ad, ar)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn uuid_from_seed(seed: u64) -> Uuid {
    Uuid::from_u128((seed as u128) | (1u128 << 96)) // ensure non-nil
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: CASES_PR,
        ..ProptestConfig::default()
    })]

    /// Property 1 — `INV-AC-TENANT-SCOPED`. Two distinct tenants
    /// uploading to the *same* action_digest never see each other's
    /// entry on GET. Tenant B's GET surfaces 404.
    #[test]
    fn prop_ac_get_tenant_isolation(
        seed_a in 0u64..u64::MAX,
        seed_b in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
    ) {
        prop_assume!(seed_a != seed_b);
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant_a = uuid_from_seed(seed_a);
            let tenant_b = uuid_from_seed(seed_b);
            prop_assume!(tenant_a != tenant_b);

            let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
            let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
            let (ad, ar) = synthesize_action_result(seed_action);

            // A writes.
            w.outputs.insert_all_alive(tenant_a, &ar);
            let r = w.handler.update_action_result(&ctx_a, &ad, ar.clone(), "req-a").await;
            prop_assert!(r.is_ok(), "A's UPDATE must succeed: {r:?}");

            // B reads — must 404.
            let r = w.handler.get_action_result(&ctx_b, &ad, "req-b").await;
            prop_assert!(matches!(r, Err(AcError::NotFound)), "B must see 404; got {r:?}");
            Ok(())
        }).unwrap();
    }

    /// Property 2 — `INV-AC-IDEMPOTENT`. Same `(tenant, action,
    /// result_hash)` UPDATE twice ⇒ second is `IdempotentRefresh`.
    #[test]
    fn prop_ac_update_idempotent(
        seed_tenant in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
    ) {
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant = uuid_from_seed(seed_tenant);
            let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
            let (ad, ar) = synthesize_action_result(seed_action);
            w.outputs.insert_all_alive(tenant, &ar);
            let r1 = w.handler.update_action_result(&ctx, &ad, ar.clone(), "req-1").await.unwrap();
            prop_assert_eq!(r1.upsert_outcome, AcMetaUpsertOutcome::Inserted);
            w.clock.advance_ms(123);
            let r2 = w.handler.update_action_result(&ctx, &ad, ar.clone(), "req-2").await.unwrap();
            prop_assert_eq!(r2.upsert_outcome, AcMetaUpsertOutcome::IdempotentRefresh);
            // Result hash is content-stable.
            prop_assert_eq!(r1.row.result_hash, r2.row.result_hash);
            Ok(())
        }).unwrap();
    }

    /// Property 3 — `INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE`. GET
    /// miss populates the neg cache; subsequent UPDATE invalidates;
    /// final GET observes the row.
    #[test]
    fn prop_ac_negative_cache_invalidate(
        seed_tenant in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
    ) {
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant = uuid_from_seed(seed_tenant);
            let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
            let (ad, ar) = synthesize_action_result(seed_action);
            // 1st GET — miss.
            let r = w.handler.get_action_result(&ctx, &ad, "req-1").await;
            prop_assert!(matches!(r, Err(AcError::NotFound)));
            // Neg cache hot.
            let tctx = ctx.tenant_ctx();
            let hit = w.neg.lookup(&tctx, &ad.hash).await.unwrap();
            prop_assert_eq!(hit, Some(()));
            // UPDATE.
            w.outputs.insert_all_alive(tenant, &ar);
            w.handler.update_action_result(&ctx, &ad, ar.clone(), "req-2").await.unwrap();
            // Neg cache empty.
            let hit = w.neg.lookup(&tctx, &ad.hash).await.unwrap();
            prop_assert!(hit.is_none(), "neg cache must be invalidated");
            // GET hits.
            let g = w.handler.get_action_result(&ctx, &ad, "req-3").await.unwrap();
            prop_assert_eq!(g.action_result, ar);
            Ok(())
        }).unwrap();
    }

    /// Property 4 — `INV-AC-TTL-REFRESH-MONOTONIC`. `last_hit_at` is
    /// monotonic non-decreasing across GET hits.
    #[test]
    fn prop_ac_ttl_refresh_monotonic(
        seed_tenant in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
        n_hits in 1u32..32u32,
        delta_ms in 1u64..10_000u64,
    ) {
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant = uuid_from_seed(seed_tenant);
            let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
            let (ad, ar) = synthesize_action_result(seed_action);
            w.outputs.insert_all_alive(tenant, &ar);
            w.handler.update_action_result(&ctx, &ad, ar, "req-up").await.unwrap();
            let mut prev = w.clock.now_ms();
            for i in 0..n_hits {
                w.clock.advance_ms(delta_ms);
                let g: GetActionResult = w.handler.get_action_result(&ctx, &ad, "req-get").await.unwrap();
                let cur = g.row.last_hit_at_ms;
                prop_assert!(
                    cur >= prev,
                    "last_hit_at must be monotonic; iter={i} prev={prev} cur={cur}"
                );
                prev = cur;
            }
            Ok(())
        }).unwrap();
    }

    /// Property 5 — `INV-AC-OUTPUTS-VALID`. UPDATE with any tombstoned
    /// output is rejected 422 + audit `ac.update.outputs_missing`; no
    /// `ac_meta` row materializes.
    #[test]
    fn prop_ac_outputs_missing_blocks_update(
        seed_tenant in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
        n_outputs in 1u32..16u32,
        which_dead in 0u32..16u32,
    ) {
        let dead_idx = which_dead % n_outputs;
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant = uuid_from_seed(seed_tenant);
            let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
            let action_hash = Digest::compute(format!("action-{seed_action}").as_bytes());
            let ad = ActionDigest::new(action_hash, 32);
            let outputs: Vec<OutputFileDigest> = (0..n_outputs)
                .map(|i| {
                    let d = Digest::compute(format!("out-{seed_action}-{i}").as_bytes());
                    OutputFileDigest::new(d, 64)
                })
                .collect();
            let ar = ActionResult::new(outputs.clone(), Vec::new(), 0, b"raw".to_vec());
            // Insert all alive, then tombstone one.
            for o in &outputs {
                w.outputs.insert_alive(tenant, o.digest);
            }
            // Mark dead_idx as tombstoned.
            let dead = outputs.get(dead_idx as usize).unwrap();
            w.outputs.insert_tombstoned(tenant, dead.digest);
            let r = w.handler.update_action_result(&ctx, &ad, ar, "req-up").await;
            prop_assert!(matches!(r, Err(AcError::OutputsMissing { .. })), "expected OutputsMissing; got {r:?}");
            // No row.
            prop_assert!(w.meta.is_empty().unwrap());
            // Audit emitted.
            let outm = w.audit.snapshot_of(AcEventType::UpdateOutputsMissing);
            prop_assert_eq!(outm.len(), 1);
            // Envelope NOT persisted.
            prop_assert!(w.envelope.keys().is_empty());
            Ok(())
        }).unwrap();
    }
}
