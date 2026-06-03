//! End-to-end Gherkin scenarios for the AC handler (WI-S04-001 §8).
//!
//! Each Gherkin scenario maps to a `#[tokio::test]` exercising the
//! full handler trait surface end-to-end against the InMemory wiring.
//! These tests are the canonical behavioral spec — every refactor
//! that touches the handler MUST keep this file green.

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
    AcEnvelopeStore, AcError, ActionCacheHandler, ActionCacheHandlerBuilder,
    ActionCacheHandlerImpl, Clock, FakeClock, InMemoryAcEnvelopeStore, DEFAULT_AC_TTL_EXTEND_MS,
};
use corelink_worker::reapi::ac::{
    AcEventType, AcMetaUpsertOutcome, AcNegCache, ActionDigest, ActionResult, InMemoryAcMetaStore,
    InMemoryAuditSink, InMemoryFakeSigner, InMemoryMerkleVerifier, InMemoryOutputsCheck,
    OutputFileDigest,
};
use corelink_worker::Region;
use uuid::Uuid;
use zeroize::Zeroizing;

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

fn fixed_action() -> (ActionDigest, ActionResult) {
    let action_hash = Digest::compute(b"e2e-action");
    let ad = ActionDigest::new(action_hash, 100);
    let ar = ActionResult::new(
        vec![
            OutputFileDigest::new(Digest::compute(b"out-1"), 10),
            OutputFileDigest::new(Digest::compute(b"out-2"), 20),
        ],
        Vec::new(),
        0,
        b"e2e-proto-bytes".to_vec(),
    );
    (ad, ar)
}

fn fixed_tenant_a() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

fn fixed_tenant_b() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
}

/// Gherkin: GetActionResult hit (warm path).
#[tokio::test]
async fn gherkin_get_action_result_hit_warm() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    let upd = w
        .handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-1")
        .await
        .unwrap();
    assert_eq!(upd.upsert_outcome, AcMetaUpsertOutcome::Inserted);
    let g = w
        .handler
        .get_action_result(&ctx, &ad, "req-2")
        .await
        .unwrap();
    assert_eq!(g.action_result, ar);
    assert!(g.row.last_hit_at_ms >= w.clock.now_ms());
    // Audit: GetOk emitted.
    assert_eq!(w.audit.snapshot_of(AcEventType::GetOk).len(), 1);
}

/// Gherkin: GetActionResult miss (cold path; populate negative cache).
#[tokio::test]
async fn gherkin_get_action_result_miss_populates_neg_cache() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_R),
    );
    let (ad, _) = fixed_action();
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-miss")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::NotFound));
    assert_eq!(err.cor_code(), "COR_AC_ACTION_NOT_FOUND");
    // Neg cache populated.
    let tctx = ctx.tenant_ctx();
    assert_eq!(w.neg.lookup(&tctx, &ad.hash).await.unwrap(), Some(()));
    // Audit GetMiss emitted.
    assert_eq!(w.audit.snapshot_of(AcEventType::GetMiss).len(), 1);
}

/// Gherkin: GetActionResult short-circuits on negative cache hit.
#[tokio::test]
async fn gherkin_get_action_result_neg_cache_short_circuits() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_R),
    );
    let (ad, _) = fixed_action();
    // Populate neg cache directly.
    let tctx = ctx.tenant_ctx();
    w.neg.populate_miss(&tctx, &ad.hash).await.unwrap();
    // GET returns 404 without touching ac_meta.
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-neg")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::NotFound));
    // Snapshot the audit: only one GetMiss with reason=neg_cache_hit.
    let miss = w.audit.snapshot_of(AcEventType::GetMiss);
    assert_eq!(miss.len(), 1);
    assert_eq!(miss[0].reason, "neg_cache_hit");
}

/// Gherkin: GetActionResult tenant isolation (cross-tenant 404).
#[tokio::test]
async fn gherkin_cross_tenant_get_returns_not_found() {
    let w = wire(Region::Wnam);
    let ctx_a = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let ctx_b = make_ctx(
        fixed_tenant_b(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_R),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    w.handler
        .update_action_result(&ctx_a, &ad, ar.clone(), "req-a")
        .await
        .unwrap();
    let err = w
        .handler
        .get_action_result(&ctx_b, &ad, "req-b")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::NotFound));
    // Audit GetMiss for B carries B's tenant id, NEVER A's.
    let miss = w.audit.snapshot_of(AcEventType::GetMiss);
    assert!(!miss.is_empty());
    for r in &miss {
        assert_eq!(r.tenant_id, fixed_tenant_b());
    }
}

/// Gherkin: GetActionResult signature invalid (envelope tampering).
#[tokio::test]
async fn gherkin_envelope_tampering_returns_sig_invalid() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap();
    let prefix = *ctx.tenant_prefix();
    let hex = ad.hash.to_hex();
    assert!(w.envelope.tamper_for_test(Region::Wnam, &prefix, &hex));
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-get")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::SigInvalid));
    assert_eq!(err.cor_code(), "COR_AC_SIG_INVALID");
    // Audit emitted.
    assert_eq!(w.audit.snapshot_of(AcEventType::GetSigInvalid).len(), 1);
    // Negative cache populated (defense-in-depth against retry storms).
    let tctx = ctx.tenant_ctx();
    assert_eq!(w.neg.lookup(&tctx, &ad.hash).await.unwrap(), Some(()));
}

/// Gherkin: GetActionResult expired (TTL exceeded).
#[tokio::test]
async fn gherkin_get_action_result_expired_returns_410() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap();
    w.clock.advance_ms(DEFAULT_AC_TTL_EXTEND_MS + 1);
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-get")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::Expired));
    assert_eq!(err.cor_code(), "COR_AC_TTL_EXPIRED");
}

/// Gherkin: UpdateActionResult happy path.
#[tokio::test]
async fn gherkin_update_action_result_happy_path() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    let r = w
        .handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-up")
        .await
        .unwrap();
    assert_eq!(r.upsert_outcome, AcMetaUpsertOutcome::Inserted);
    assert_eq!(r.action_result, ar);
    // Envelope persisted.
    let prefix = *ctx.tenant_prefix();
    let hex = ad.hash.to_hex();
    let env = w.envelope.get(Region::Wnam, &prefix, &hex).await.unwrap();
    assert!(env.is_some());
    // ac_meta row materialized.
    assert_eq!(w.meta.len().unwrap(), 1);
    // Audit UpdateOk emitted.
    assert_eq!(w.audit.snapshot_of(AcEventType::UpdateOk).len(), 1);
}

/// Gherkin: UpdateActionResult Merkle invalid (rejected pre-persist).
#[tokio::test]
async fn gherkin_update_merkle_invalid_rejected_pre_persist() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W),
    );
    let (ad, mut ar) = fixed_action();
    // Inject all-zero digest sentinel ⇒ MalformedTree.
    ar.output_files.push(OutputFileDigest::new(
        Digest::from_hex(&"00".repeat(32)).unwrap(),
        1,
    ));
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::MerkleInvalid { .. }));
    assert_eq!(err.cor_code(), "COR_AC_MERKLE_INVALID");
    // No row, no envelope.
    assert!(w.meta.is_empty().unwrap());
    assert!(w.envelope.keys().is_empty());
    // Audit emitted.
    assert_eq!(
        w.audit.snapshot_of(AcEventType::UpdateMerkleInvalid).len(),
        1
    );
}

/// Gherkin: UpdateActionResult outputs missing (tombstoned blob).
#[tokio::test]
async fn gherkin_update_outputs_missing_rejected() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W),
    );
    let (ad, ar) = fixed_action();
    // Mark first output as tombstoned.
    let dead = ar.output_files[0].digest;
    w.outputs.insert_tombstoned(fixed_tenant_a(), dead);
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::OutputsMissing { count: 2 }));
    assert_eq!(err.cor_code(), "COR_AC_OUTPUTS_MISSING");
    // No row, no envelope.
    assert!(w.meta.is_empty().unwrap());
    assert!(w.envelope.keys().is_empty());
    // Audit emitted with the missing digest.
    let outm = w.audit.snapshot_of(AcEventType::UpdateOutputsMissing);
    assert_eq!(outm.len(), 1);
    assert!(!outm[0].missing_outputs.is_empty());
}

/// Gherkin: UpdateActionResult idempotent.
#[tokio::test]
async fn gherkin_update_idempotent() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W),
    );
    let (ad, ar) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar);
    let r1 = w
        .handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-1")
        .await
        .unwrap();
    let r2 = w
        .handler
        .update_action_result(&ctx, &ad, ar, "req-2")
        .await
        .unwrap();
    assert_eq!(r1.upsert_outcome, AcMetaUpsertOutcome::Inserted);
    assert_eq!(r2.upsert_outcome, AcMetaUpsertOutcome::IdempotentRefresh);
    assert_eq!(w.meta.len().unwrap(), 1, "only one row materialized");
    // Two UpdateOk audits.
    assert_eq!(w.audit.snapshot_of(AcEventType::UpdateOk).len(), 2);
}

/// Gherkin: UpdateActionResult result_hash mismatch (suspicious; 409).
#[tokio::test]
async fn gherkin_update_result_hash_mismatch_409() {
    let w = wire(Region::Wnam);
    let ctx = make_ctx(
        fixed_tenant_a(),
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W),
    );
    let (ad, ar1) = fixed_action();
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar1);
    w.handler
        .update_action_result(&ctx, &ad, ar1, "req-1")
        .await
        .unwrap();
    let ar2 = ActionResult::new(
        vec![OutputFileDigest::new(Digest::compute(b"out-1"), 10)],
        Vec::new(),
        0,
        b"DIFFERENT-PROTO".to_vec(),
    );
    w.outputs.insert_all_alive(fixed_tenant_a(), &ar2);
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar2, "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::ResultHashMismatch { .. }));
    assert_eq!(err.cor_code(), "COR_AC_RESULT_HASH_MISMATCH");
    // Audit emitted.
    assert_eq!(
        w.audit.snapshot_of(AcEventType::UpdateResultMismatch).len(),
        1
    );
}

/// Gherkin: scope check rejects request without `cache:r` / `cache:w`.
#[tokio::test]
async fn gherkin_scope_insufficient_rejects_403() {
    let w = wire(Region::Wnam);
    let ctx_no_scope = make_ctx(fixed_tenant_a(), Region::Wnam, PatScopes::empty());
    let (ad, ar) = fixed_action();
    let err = w
        .handler
        .get_action_result(&ctx_no_scope, &ad, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        AcError::ScopeInsufficient {
            required: SCOPE_CACHE_R
        }
    ));
    let err = w
        .handler
        .update_action_result(&ctx_no_scope, &ad, ar, "req-2")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        AcError::ScopeInsufficient {
            required: SCOPE_CACHE_W
        }
    ));
}

/// Gherkin: AcError 10-variant taxonomy maps 1:1 to canonical
/// `COR_AC_*` codes.
#[test]
fn gherkin_canonical_error_code_mapping() {
    use corelink_worker::reapi::ac::handler::AcError;
    assert_eq!(AcError::NotFound.cor_code(), "COR_AC_ACTION_NOT_FOUND");
    assert_eq!(AcError::Expired.cor_code(), "COR_AC_TTL_EXPIRED");
    assert_eq!(AcError::SigInvalid.cor_code(), "COR_AC_SIG_INVALID");
    assert_eq!(
        AcError::MerkleInvalid {
            reason: "depth_exceeded"
        }
        .cor_code(),
        "COR_AC_MERKLE_INVALID"
    );
    assert_eq!(
        AcError::OutputsMissing { count: 2 }.cor_code(),
        "COR_AC_OUTPUTS_MISSING"
    );
    let h1 = corelink_worker::reapi::ac::ResultHash::compute(&ActionResult::new(
        Vec::new(),
        Vec::new(),
        0,
        b"a".to_vec(),
    ));
    let h2 = corelink_worker::reapi::ac::ResultHash::compute(&ActionResult::new(
        Vec::new(),
        Vec::new(),
        0,
        b"b".to_vec(),
    ));
    assert_eq!(
        AcError::ResultHashMismatch {
            existing: h1,
            attempted: h2
        }
        .cor_code(),
        "COR_AC_RESULT_HASH_MISMATCH"
    );
    assert_eq!(
        AcError::ScopeInsufficient {
            required: SCOPE_CACHE_R
        }
        .cor_code(),
        "COR_AUTH_SCOPE_INSUFFICIENT"
    );
    assert_eq!(
        AcError::BackendUnavailable("x".into()).cor_code(),
        "COR_AC_BACKEND_UNAVAILABLE"
    );
}
