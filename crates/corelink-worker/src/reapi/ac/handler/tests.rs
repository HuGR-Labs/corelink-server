//! Unit tests for the pure-logic AC handler.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantDerivationKey;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::super::audit::{AcEventType, InMemoryAuditSink};
use super::super::merkle::InMemoryMerkleVerifier;
use super::super::meta::{AcMetaUpsertOutcome, InMemoryAcMetaStore};
use super::super::outputs::InMemoryOutputsCheck;
use super::super::sig::InMemoryFakeSigner;
use super::super::types::{ActionDigest, ActionResult, OutputFileDigest};
use super::builder::{
    ActionCacheHandlerBuilder, ActionCacheHandlerImpl, Clock, FakeClock,
};
use super::envelope_store::InMemoryAcEnvelopeStore;
use super::errors::{AcError, DEFAULT_AC_TTL_EXTEND_MS};
use super::handler_trait::ActionCacheHandler;
use super::super::neg_cache::AcNegCache;
use crate::cache::kv::InMemoryKv;
use crate::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use crate::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use crate::Region;

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

#[allow(dead_code, reason = "merkle/signer fields kept on Arc for shared-handle lifecycle even when tests do not borrow them")]
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
    merkle: Arc<InMemoryMerkleVerifier>,
    signer: Arc<InMemoryFakeSigner>,
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
        merkle,
        signer,
        outputs,
        audit,
        neg,
        clock,
    }
}

fn fresh_action_result() -> (ActionDigest, ActionResult) {
    let proto_bytes = b"canonical-action-proto-1".to_vec();
    let action_hash = Digest::compute(b"action-key-1");
    let action_digest = ActionDigest::new(action_hash, 32);
    let result = ActionResult::new(
        vec![OutputFileDigest::new(Digest::compute(b"out1"), 100)],
        Vec::new(),
        0,
        proto_bytes,
    );
    (action_digest, result)
}

#[tokio::test]
async fn update_then_get_happy_path() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    // Pre-populate outputs alive.
    w.outputs.insert_all_alive(tenant, &ar);
    let upd = w
        .handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-update-1")
        .await
        .unwrap();
    assert_eq!(upd.upsert_outcome, AcMetaUpsertOutcome::Inserted);
    // GET should now succeed.
    let get = w
        .handler
        .get_action_result(&ctx, &ad, "req-get-1")
        .await
        .unwrap();
    assert_eq!(get.action_result, ar);
    // Last hit advanced.
    assert!(get.row.last_hit_at_ms >= w.clock.now_ms());
}

#[tokio::test]
async fn cross_tenant_get_returns_not_found() {
    let w = wire(Region::Wnam);
    let tenant_a = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let tenant_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
    let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    w.outputs.insert_all_alive(tenant_a, &ar);
    w.handler
        .update_action_result(&ctx_a, &ad, ar.clone(), "req-up-a")
        .await
        .unwrap();
    // B asks for the same digest under their own ctx.
    let err = w
        .handler
        .get_action_result(&ctx_b, &ad, "req-get-b")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::NotFound));
}

#[tokio::test]
async fn missing_scope_rejected_403() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::empty());
    let (ad, _) = fresh_action_result();
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-noscope")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        AcError::ScopeInsufficient {
            required: SCOPE_CACHE_R
        }
    ));
    assert_eq!(err.cor_code(), "COR_AUTH_SCOPE_INSUFFICIENT");
}

#[tokio::test]
async fn merkle_invalid_rejected_pre_persist() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let (ad, mut ar) = fresh_action_result();
    // Inject all-zero digest sentinel ⇒ MalformedTree.
    ar.output_files.push(OutputFileDigest::new(
        Digest::from_hex(&"00".repeat(32)).unwrap(),
        1,
    ));
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar, "req-merkle")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::MerkleInvalid { .. }));
    assert!(w.meta.is_empty().unwrap());
}

#[tokio::test]
async fn outputs_missing_rejected_pre_persist() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let (ad, ar) = fresh_action_result();
    // outputs NOT inserted — missing.
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar, "req-outputs")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::OutputsMissing { count: 1 }));
    assert!(w.meta.is_empty().unwrap());
}

#[tokio::test]
async fn idempotent_re_update_refreshes_last_hit() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    w.outputs.insert_all_alive(tenant, &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-1")
        .await
        .unwrap();
    w.clock.advance_ms(5_000);
    let again = w
        .handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-2")
        .await
        .unwrap();
    assert_eq!(
        again.upsert_outcome,
        AcMetaUpsertOutcome::IdempotentRefresh
    );
}

#[tokio::test]
async fn result_hash_mismatch_rejected_409() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let (ad, ar1) = fresh_action_result();
    w.outputs.insert_all_alive(tenant, &ar1);
    w.handler
        .update_action_result(&ctx, &ad, ar1, "req-1")
        .await
        .unwrap();
    // Same action_digest, different proto bytes ⇒ different result_hash.
    let ar2 = ActionResult::new(
        vec![OutputFileDigest::new(Digest::compute(b"out1"), 100)],
        Vec::new(),
        0,
        b"DIFFERENT-PROTO".to_vec(),
    );
    w.outputs.insert_all_alive(tenant, &ar2);
    let err = w
        .handler
        .update_action_result(&ctx, &ad, ar2, "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::ResultHashMismatch { .. }));
    assert_eq!(err.cor_code(), "COR_AC_RESULT_HASH_MISMATCH");
}

#[tokio::test]
async fn neg_cache_invalidated_on_update() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    // First GET — miss + populate negative cache.
    let _ = w
        .handler
        .get_action_result(&ctx, &ad, "req-get-miss")
        .await
        .unwrap_err();
    // Negative cache now hot.
    let tenant_ctx = ctx.tenant_ctx();
    let hit = w.neg.lookup(&tenant_ctx, &ad.hash).await.unwrap();
    assert_eq!(hit, Some(()));
    // UPDATE invalidates.
    w.outputs.insert_all_alive(tenant, &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-upd")
        .await
        .unwrap();
    let hit = w.neg.lookup(&tenant_ctx, &ad.hash).await.unwrap();
    assert!(hit.is_none(), "neg cache must be invalidated post-UPDATE");
    // Subsequent GET — hit.
    let got = w
        .handler
        .get_action_result(&ctx, &ad, "req-get-hit")
        .await
        .unwrap();
    assert_eq!(got.action_result, ar);
}

#[tokio::test]
async fn region_mismatch_returns_internal() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Weur, PatScopes::single(SCOPE_CACHE_R));
    let (ad, _) = fresh_action_result();
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-mis")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::RegionMismatch { .. }));
}

#[tokio::test]
async fn audit_emitted_on_get_ok() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    w.outputs.insert_all_alive(tenant, &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar.clone(), "req-up")
        .await
        .unwrap();
    let _ = w.handler.get_action_result(&ctx, &ad, "req-get").await.unwrap();
    let ok = w.audit.snapshot_of(AcEventType::GetOk);
    assert_eq!(ok.len(), 1);
    let upd = w.audit.snapshot_of(AcEventType::UpdateOk);
    assert_eq!(upd.len(), 1);
}

#[tokio::test]
async fn ttl_expired_returns_410() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    w.outputs.insert_all_alive(tenant, &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap();
    // Advance past TTL.
    w.clock.advance_ms(DEFAULT_AC_TTL_EXTEND_MS + 1);
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-get")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::Expired));
    assert_eq!(err.cor_code(), "COR_AC_TTL_EXPIRED");
}

#[tokio::test]
async fn envelope_tampering_detected_via_sig_invalid() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
    let (ad, ar) = fresh_action_result();
    w.outputs.insert_all_alive(tenant, &ar);
    w.handler
        .update_action_result(&ctx, &ad, ar, "req-up")
        .await
        .unwrap();
    // Tamper the envelope sig byte directly.
    let prefix = *ctx.tenant_prefix();
    let hex = ad.hash.to_hex();
    assert!(w.envelope.tamper_for_test(Region::Wnam, &prefix, &hex));
    let err = w
        .handler
        .get_action_result(&ctx, &ad, "req-get")
        .await
        .unwrap_err();
    assert!(matches!(err, AcError::SigInvalid));
    // Audit emitted.
    let sig = w.audit.snapshot_of(AcEventType::GetSigInvalid);
    assert_eq!(sig.len(), 1);
}
