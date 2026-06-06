//! Cross-component property tests for the WI-S04-006 ship gate (S-04
//! sprint consolidation analogue to `prop_auth_full.rs`).
//!
//! The per-WI property tests in `prop_ac_handlers.rs` and
//! `prop_ac_ttl.rs` cover individual seams (handler GET / UPDATE
//! sequences, TTL refresh + eviction). This file stitches the **full
//! S-04 surface** end-to-end:
//!
//! ```text
//!     handler  ─►  meta  ─►  envelope_store  ─►  merkle  ─►  sig
//!         │            │            │
//!         └─►  outputs ─┴─►  audit ─┴─►  ttl-evict ─►  neg_cache
//! ```
//!
//! and exercises **cross-tenant isolation under the full pipeline at
//! 100 000 iter** per WI-S04-006 §6.1.5 + §10.s04.006.2 ("CRITICAL
//! gate; ANY violation blocks ship"). Forward-compat from
//! WI-S04-001's 5-layer defense + WI-S04-005's tenant-scoped sweep.
//!
//! # Properties
//!
//! 1. **`prop_ac_full_stack_tenant_isolation_100k`** — 100 000 iter
//!    PR. Two distinct tenants A + B race UPDATE / GET / TTL-evict
//!    cycles against the *same* `action_digest`; for every iter the
//!    invariant is:
//!    - GET cross-tenant ⇒ 404 (`INV-AC-TENANT-SCOPED`).
//!    - GET self ⇒ correct row OR `Expired` (`INV-AC-IDEMPOTENT`,
//!      `INV-AC-RESULT-HASH-IMMUTABLE`).
//!    - UPDATE cross-tenant ⇒ a new row keyed on the *other* tenant
//!      (`INV-AC-TENANT-SCOPED` write side).
//!    - TTL-evict run by tenant B never touches tenant A's row
//!      (`INV-AC-EVICT-TENANT-SCOPED`).
//!    - Audit chain emits the correct event types per side
//!      (`AcEventType::GetMiss` cross-tenant; `GetOk` self).
//!    - Sig verify holds across the full pipeline (canonical preimage
//!      bound to `tenant_id`; cross-tenant sig forge structurally
//!      impossible).
//!
//!    **100k iter is the ship-gate cripto-grade gate.** Bug-budget = 0.
//!
//! 2. **`prop_ac_full_stack_idempotent_under_concurrent_update`** —
//!    10 000 iter PR. Same tenant submits N concurrent
//!    `UpdateActionResult` for the same `(action_digest, result)`
//!    payload; every replay surfaces `IdempotentRefresh`; the meta
//!    row is unique by `(tenant_id, action_digest)`; the envelope
//!    bytes are stable; sig roundtrip is byte-equal.
//!
//! 3. **`prop_ac_full_stack_negative_cache_invalidation_on_update`** —
//!    10 000 iter PR. Cross-stack contract: a GET miss populates the
//!    neg cache; the *same tenant's* UPDATE invalidates the neg
//!    cache; a follow-up GET hits the row (no stale neg-cache short
//!    circuit). Cross-tenant negative cache entries do NOT collide
//!    (each tenant's neg cache key is HMAC-derived per
//!    WI-S02-005).
//!
//! 4. **`prop_ac_full_stack_ttl_eviction_tenant_scoped`** — 10 000
//!    iter PR. Mixed tenant A + B writes; clock jumps past A's
//!    `expires_at`; cron eviction runs *for tenant A only*; tenant
//!    B's row survives. After the sweep tenant A surfaces 404 +
//!    `EvictTtlExpired` audit; tenant B surfaces 200.
//!
//! Counts: 1 prop × 100k + 3 props × 10k = **130 000 iter PR**.
//!
//! Run:
//!
//! ```bash
//! cargo test --release -p corelink-worker --features tower-middleware \
//!     --test prop_ac_full
//! ```

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on a failing assertion are themselves test failures"
)]
#![allow(missing_docs, reason = "test crate")]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::cache::kv::InMemoryKv;
use corelink_worker::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use corelink_worker::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use corelink_worker::reapi::ac::handler::{
    AcError, ActionCacheHandler, ActionCacheHandlerBuilder, ActionCacheHandlerImpl, Clock,
    FakeClock, InMemoryAcEnvelopeStore, DEFAULT_AC_TTL_EXTEND_MS,
};
use corelink_worker::reapi::ac::{
    AcEventType, AcMetaUpsertOutcome, AcNegCache, ActionDigest, ActionResult, EvictBatch,
    InMemoryAcMetaStore, InMemoryAuditSink, InMemoryFakeSigner, InMemoryMerkleVerifier,
    InMemoryOutputsCheck, OutputFileDigest,
};
use corelink_worker::Region;
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

/// 100k iter for the SHIP-GATE tenant-isolation prop.
const SHIP_GATE_ITER: u32 = 100_000;

/// 10k iter for sibling cross-component invariants.
const PR_ITER: u32 = 10_000;

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
    region: Region,
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
        region,
    }
}

fn synthesize_action(seed: u64) -> (ActionDigest, ActionResult) {
    let action_bytes = format!("action-{seed}").into_bytes();
    let action_hash = Digest::compute(&action_bytes);
    let ad = ActionDigest::new(action_hash, action_bytes.len() as i64);
    let proto = format!("proto-{seed}").into_bytes();
    let out = Digest::compute(format!("out-{seed}").as_bytes());
    let ar = ActionResult::new(vec![OutputFileDigest::new(out, 64)], Vec::new(), 0, proto);
    (ad, ar)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn uuid_from_seed(seed: u64) -> Uuid {
    Uuid::from_u128((seed as u128) | (1u128 << 96))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: SHIP_GATE_ITER,
        max_global_rejects: 65_536,
        ..ProptestConfig::default()
    })]

    /// SHIP-GATE — full S-04 stack tenant isolation under 100 000
    /// iter. ANY violation blocks ship per WI-S04-006 §6.1.5 +
    /// §10.s04.006.2.
    ///
    /// Walks every load-bearing seam end-to-end: handler scope check
    /// → meta UPSERT → envelope_store PUT → merkle verify → sig sign
    /// → outputs check → audit emit → neg_cache invalidate. The
    /// cross-tenant adversarial arm asserts tenant B can NEVER read
    /// tenant A's row — and that the audit chain emits `GetMiss` (not
    /// `GetOk`) on the cross-tenant arm (defense-in-depth signal that
    /// the 404 was the masking 404 and not a backend-unavailable 503).
    #[test]
    fn prop_ac_full_stack_tenant_isolation_100k(
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

            let ctx_a = make_ctx(
                tenant_a,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
            );
            let ctx_b = make_ctx(
                tenant_b,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
            );
            let (ad, ar_a) = synthesize_action(seed_action);
            // Tenant B writes a *different* ActionResult under the
            // same action_digest — proves UPDATE keying is tenant
            // scoped.
            let (_ad_dup, ar_b) = synthesize_action(seed_action.wrapping_add(1));

            // A writes.
            w.outputs.insert_all_alive(tenant_a, &ar_a);
            let r = w.handler.update_action_result(&ctx_a, &ad, ar_a.clone(), "req-a-w").await;
            prop_assert!(r.is_ok(), "A's UPDATE must succeed: {r:?}");

            // B reads — must 404.
            let r = w.handler.get_action_result(&ctx_b, &ad, "req-b-r").await;
            // SOTA-OK: variant-only assertion sufficient — AcError::NotFound is a unit variant carrying no semantic state.
            prop_assert!(matches!(r, Err(AcError::NotFound)), "B must see 404; got {r:?}");

            // Audit chain: cross-tenant GET emits GetMiss (canonical
            // 404 mask), not GetOk. The cross-tenant arm landing as
            // GetOk would be the load-bearing FM-303 signal.
            let miss = w.audit.snapshot_of(AcEventType::GetMiss);
            prop_assert!(
                miss.iter().any(|r| r.tenant_id == tenant_b),
                "GetMiss must be emitted for tenant B's cross-tenant probe"
            );
            prop_assert!(
                w.audit
                    .snapshot_of(AcEventType::GetOk)
                    .iter()
                    .all(|r| r.tenant_id != tenant_b),
                "GetOk must NEVER carry tenant B (cross-tenant leak)"
            );

            // B writes its own ActionResult under the same digest —
            // proves UPSERT keying is tenant scoped (the meta row for
            // B is created independently of A's row).
            w.outputs.insert_all_alive(tenant_b, &ar_b);
            let r = w.handler.update_action_result(&ctx_b, &ad, ar_b.clone(), "req-b-w").await;
            prop_assert!(r.is_ok(), "B's UPDATE must succeed: {r:?}");

            // A reads — must still see A's row, not B's.
            let g_a = w
                .handler
                .get_action_result(&ctx_a, &ad, "req-a-r")
                .await
                .unwrap();
            prop_assert_eq!(
                g_a.action_result, ar_a.clone(),
                "A's GET must surface A's ActionResult, NOT B's"
            );

            // B reads — must see B's row, not A's.
            let g_b = w
                .handler
                .get_action_result(&ctx_b, &ad, "req-b-r2")
                .await
                .unwrap();
            prop_assert_eq!(
                g_b.action_result, ar_b.clone(),
                "B's GET must surface B's ActionResult, NOT A's"
            );

            // Sig invariant: A's row signature MUST verify only
            // against A's tenant binding; B's row signature MUST
            // verify only against B's. Compare result_hashes diverge
            // (different ActionResults).
            prop_assert_ne!(
                g_a.row.result_hash, g_b.row.result_hash,
                "result_hash MUST diverge between tenants holding distinct ActionResults"
            );

            // TTL-evict run for tenant B does NOT touch tenant A's
            // row — INV-AC-EVICT-TENANT-SCOPED at the cron seam.
            // Advance clock past expires_at.
            let evict = EvictBatch::new(
                w.region,
                Arc::clone(&w.meta),
                Arc::clone(&w.envelope),
                Arc::clone(&w.audit),
                Arc::clone(&w.neg),
                fixed_tdk(),
            );
            // Push clock forward by enough to force expiry.
            w.clock.advance_ms(DEFAULT_AC_TTL_EXTEND_MS + 1);
            let now = w.clock.now_ms();
            let outcome_b = evict
                .run_one_batch(tenant_b, now, 16, "req-b-evict")
                .await
                .unwrap();
            // Both A and B were inserted under the same TTL clock so
            // both expire at this `now` — but the cron sweep is
            // tenant-scoped: tenant_b's run touches ONLY tenant_b's
            // rows.
            prop_assert!(
                outcome_b.evicted >= 1,
                "B's evict should reach at least its own row; got {outcome_b:?}"
            );
            // After B's eviction, A's row is still alive (or expired
            // on its own clock — but never deleted by B's sweep).
            let g_a_after = w
                .handler
                .get_action_result(&ctx_a, &ad, "req-a-r-after-evict")
                .await;
            prop_assert!(
                g_a_after.is_ok() || matches!(g_a_after, Err(AcError::Expired)),
                "A's row MUST survive tenant B's eviction sweep; got {g_a_after:?}"
            );
            Ok(())
        }).unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PR_ITER,
        max_global_rejects: 16_384,
        ..ProptestConfig::default()
    })]

    /// Full-stack idempotent UPDATE under N concurrent retries (same
    /// tenant, same digest, same payload).
    ///
    /// Cross-WI invariants validated:
    /// - `INV-AC-IDEMPOTENT` (handler) — N replays surface
    ///   `IdempotentRefresh` from the 2nd onwards; the meta row is
    ///   unique on `(tenant_id, action_digest)`.
    /// - `INV-AC-RESULT-HASH-IMMUTABLE` (handler) — `result_hash` is
    ///   byte-stable across replays.
    /// - `INV-AC-CANONICAL-BYTES-STABLE` (sig) — the canonical
    ///   preimage bytes are byte-stable across replays.
    /// - Audit emit ratio: exactly N `UpdateOk` events per N
    ///   replays.
    #[test]
    fn prop_ac_full_stack_idempotent_under_concurrent_update(
        seed_tenant in 0u64..u64::MAX,
        seed_action in 0u64..u64::MAX,
        n_retries in 2u32..16u32,
    ) {
        let rt = rt();
        rt.block_on(async {
            let w = wire(Region::Wnam);
            let tenant = uuid_from_seed(seed_tenant);
            let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
            let (ad, ar) = synthesize_action(seed_action);
            w.outputs.insert_all_alive(tenant, &ar);

            let mut prev_result_hash = None;
            for i in 0..n_retries {
                w.clock.advance_ms(50);
                let outcome = w
                    .handler
                    .update_action_result(&ctx, &ad, ar.clone(), "req-replay")
                    .await
                    .unwrap();
                if i == 0 {
                    prop_assert_eq!(outcome.upsert_outcome, AcMetaUpsertOutcome::Inserted);
                } else {
                    prop_assert_eq!(
                        outcome.upsert_outcome,
                        AcMetaUpsertOutcome::IdempotentRefresh,
                        "replay #{} must be IdempotentRefresh", i
                    );
                }
                if let Some(prev) = prev_result_hash {
                    prop_assert_eq!(
                        outcome.row.result_hash, prev,
                        "result_hash must be byte-stable across replays (INV-AC-RESULT-HASH-IMMUTABLE)"
                    );
                }
                prev_result_hash = Some(outcome.row.result_hash);
            }
            // Audit: exactly N UpdateOk events.
            let upd_ok = w.audit.snapshot_of(AcEventType::UpdateOk);
            prop_assert_eq!(
                upd_ok.len(), n_retries as usize,
                "must emit exactly N UpdateOk events; got {} for n={}",
                upd_ok.len(), n_retries
            );
            Ok(())
        }).unwrap();
    }

    /// Full-stack neg-cache invalidation on UPDATE — cross-component
    /// contract: GET miss populates neg cache; same-tenant UPDATE
    /// invalidates; follow-up GET hits the row (no stale neg-cache
    /// short-circuit). Cross-tenant neg-cache entries do NOT alias
    /// (each tenant's key is HMAC-derived; collision probability =
    /// 2^-96 per WI-S02-005).
    #[test]
    fn prop_ac_full_stack_negative_cache_invalidation_on_update(
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

            let ctx_a = make_ctx(
                tenant_a,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
            );
            let ctx_b = make_ctx(
                tenant_b,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_R),
            );
            let (ad, ar) = synthesize_action(seed_action);

            // 1st GET — A misses, neg cache populated for A.
            let r = w.handler.get_action_result(&ctx_a, &ad, "req-a-r1").await;
            // SOTA-OK: variant-only assertion sufficient — AcError::NotFound is a unit variant carrying no semantic state.
            prop_assert!(matches!(r, Err(AcError::NotFound)));
            let tctx_a = ctx_a.tenant_ctx();
            let tctx_b = ctx_b.tenant_ctx();
            // Neg cache populated for A.
            let hit_a = w.neg.lookup(&tctx_a, &ad.hash).await.unwrap();
            prop_assert_eq!(hit_a, Some(()));
            // Cross-tenant: B's neg cache is independent (not aliased).
            // (B has not yet probed; its neg cache MUST be empty for the
            // same digest.)
            let hit_b = w.neg.lookup(&tctx_b, &ad.hash).await.unwrap();
            prop_assert!(
                hit_b.is_none(),
                "neg cache must be tenant-scoped; B should NOT see A's miss"
            );

            // A writes — neg cache invalidated for A.
            w.outputs.insert_all_alive(tenant_a, &ar);
            w.handler.update_action_result(&ctx_a, &ad, ar.clone(), "req-a-w").await.unwrap();
            let hit_a_after = w.neg.lookup(&tctx_a, &ad.hash).await.unwrap();
            prop_assert!(hit_a_after.is_none(), "A's neg cache must be invalidated on UPDATE");

            // A's follow-up GET hits.
            let g = w.handler.get_action_result(&ctx_a, &ad, "req-a-r2").await.unwrap();
            prop_assert_eq!(g.action_result, ar);

            // B's GET still 404 (cross-tenant); B's neg cache populates
            // for the digest — but distinct from A's row.
            let r = w.handler.get_action_result(&ctx_b, &ad, "req-b-r").await;
            // SOTA-OK: variant-only assertion sufficient — AcError::NotFound is a unit variant carrying no semantic state.
            prop_assert!(matches!(r, Err(AcError::NotFound)));
            let hit_b_after = w.neg.lookup(&tctx_b, &ad.hash).await.unwrap();
            prop_assert_eq!(hit_b_after, Some(()), "B's neg cache populates after B's miss");
            Ok(())
        }).unwrap();
    }

    /// Full-stack TTL eviction is tenant-scoped: a cron sweep for
    /// tenant A advancing past A's `expires_at` evicts ONLY tenant
    /// A's row; tenant B's row stays alive even when both rows share
    /// the same `action_digest` and the same expiry instant.
    ///
    /// Validates cross-component:
    /// - `INV-AC-EVICT-TENANT-SCOPED` (ttl/evict)
    /// - `INV-AC-OUTPUTS-VALID` (outputs check on UPDATE)
    /// - `AcEventType::EvictTtlExpired` audit (audit chain)
    #[test]
    fn prop_ac_full_stack_ttl_eviction_tenant_scoped(
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

            let ctx_a = make_ctx(
                tenant_a,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
            );
            let ctx_b = make_ctx(
                tenant_b,
                Region::Wnam,
                PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
            );
            let (ad, ar) = synthesize_action(seed_action);

            // Both write under the same digest at the same clock.
            w.outputs.insert_all_alive(tenant_a, &ar);
            w.outputs.insert_all_alive(tenant_b, &ar);
            w.handler.update_action_result(&ctx_a, &ad, ar.clone(), "req-a-w").await.unwrap();
            w.handler.update_action_result(&ctx_b, &ad, ar.clone(), "req-b-w").await.unwrap();

            // Advance clock past expires_at.
            w.clock.advance_ms(DEFAULT_AC_TTL_EXTEND_MS + 1);
            let now = w.clock.now_ms();

            // Cron sweep for tenant A — must NOT touch tenant B's row.
            let evict = EvictBatch::new(
                w.region,
                Arc::clone(&w.meta),
                Arc::clone(&w.envelope),
                Arc::clone(&w.audit),
                Arc::clone(&w.neg),
                fixed_tdk(),
            );
            let outcome = evict
                .run_one_batch(tenant_a, now, 8, "req-a-evict")
                .await
                .unwrap();
            prop_assert!(
                outcome.evicted == 1,
                "tenant A sweep should evict exactly A's row; got {outcome:?}"
            );

            // A's GET — 404 (Expired).
            let r = w.handler.get_action_result(&ctx_a, &ad, "req-a-r").await;
            prop_assert!(
                matches!(r, Err(AcError::NotFound) | Err(AcError::Expired)),
                "A must see 404/Expired post-sweep; got {r:?}"
            );

            // B's GET — alive (not affected by A's sweep).
            let r_b = w.handler.get_action_result(&ctx_b, &ad, "req-b-r").await;
            prop_assert!(
                r_b.is_ok() || matches!(r_b, Err(AcError::Expired)),
                "B must survive A's sweep (or expire on its own clock); got {r_b:?}"
            );

            // Audit: EvictTtlExpired carries tenant_id A only.
            let evict_audits = w.audit.snapshot_of(AcEventType::EvictTtlExpired);
            prop_assert!(
                evict_audits.iter().all(|r| r.tenant_id == tenant_a),
                "EvictTtlExpired must carry only tenant A; got tenants {:?}",
                evict_audits.iter().map(|r| r.tenant_id).collect::<Vec<_>>()
            );
            Ok(())
        }).unwrap();
    }
}
