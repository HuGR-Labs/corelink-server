//! Unit tests for the revocation orchestrator + in-memory fakes.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1).
//! Test bodies are verbatim copies of the original inline `mod tests`
//! block.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

use crate::cache::kv::KvBackend;
use crate::region::Region;

use super::in_memory_broadcast::InMemoryBroadcast;
use super::in_memory_meta::InMemoryMetaRevocationSink;
use super::in_memory_store::InMemoryRevocationStore;
use super::orchestrator::RevocationOrchestrator;
use super::traits::{KvSessionCacheInvalidator, SessionCacheInvalidator};
use super::types::{
    HookOutcome, PropagationOutcome, RevocationError, RevocationReason, RevokeRequest,
    RevokedEntry, SessionCacheKey,
};

fn pat_id() -> PatId {
    PatId(Uuid::now_v7())
}

fn tenant() -> PatTenantId {
    PatTenantId(Uuid::now_v7())
}

fn principal() -> PatPrincipalId {
    PatPrincipalId(Uuid::now_v7())
}

fn key(suffix: &str) -> SessionCacheKey {
    SessionCacheKey::from_hash(suffix)
}

fn build_orchestrator<K: KvBackend + Send + Sync + 'static>(
    region: Region,
    peers: BTreeSet<Region>,
    store: Arc<InMemoryRevocationStore>,
    meta: Arc<InMemoryMetaRevocationSink>,
    kv: K,
    broadcast: Arc<InMemoryBroadcast>,
) -> RevocationOrchestrator {
    let cache: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(kv));
    RevocationOrchestrator::new(region, peers, store, meta, cache, broadcast)
}

#[tokio::test]
async fn revoke_happy_path_writes_all_planes() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let mut peers = BTreeSet::new();
    peers.insert(Region::Weur);
    peers.insert(Region::Sam);
    let orch = build_orchestrator(
        Region::Wnam,
        peers,
        store.clone(),
        meta.clone(),
        kv,
        broadcast.clone(),
    );
    let pid = pat_id();
    let tid = tenant();
    let prid = principal();
    let cache_key = key("aaaaaaaaaaaaaaaa");
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

    let resp = orch
        .revoke(RevokeRequest {
            pat_id: pid,
            tenant_id: tid,
            token_hash_key: cache_key.clone(),
            reason: RevocationReason::UserInitiated,
            revoked_by: prid,
        })
        .await
        .expect("revoke ok");
    assert!(resp.was_freshly_revoked);
    assert_eq!(resp.session_cache_invalidate, HookOutcome::Ok);
    assert_eq!(resp.broadcast_outcome, PropagationOutcome::Enqueued);
    assert_eq!(resp.origin_region, Region::Wnam);
    assert_eq!(meta.audit_count().await, 1);
    assert_eq!(store.entry_count().await, 1);
    assert_eq!(broadcast.enqueued_count(pid).await, 1);
}

#[tokio::test]
async fn revoke_idempotent_replay_no_dup_audit_no_dup_broadcast() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        kv,
        broadcast.clone(),
    );
    let pid = pat_id();
    let tid = tenant();
    let prid = principal();
    let cache_key = key("bbbbbbbbbbbbbbbb");
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

    let mut prior_revoked_at = None;
    for _ in 0..5 {
        let resp = orch
            .revoke(RevokeRequest {
                pat_id: pid,
                tenant_id: tid,
                token_hash_key: cache_key.clone(),
                reason: RevocationReason::UserInitiated,
                revoked_by: prid,
            })
            .await
            .expect("revoke ok");
        if let Some(ts) = prior_revoked_at {
            assert_eq!(resp.revoked_at, ts, "idempotent replay returns same ts");
            assert!(!resp.was_freshly_revoked);
        }
        prior_revoked_at = Some(resp.revoked_at);
    }
    // Single audit row for 5 retries.
    assert_eq!(meta.audit_count().await, 1);
    // Single DO entry.
    assert_eq!(store.entry_count().await, 1);
    // No peers: broadcast was never called for the fresh path.
    assert_eq!(broadcast.enqueued().await.len(), 0);
}

#[tokio::test]
async fn revoke_unknown_pat_returns_not_found() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        kv,
        broadcast,
    );
    let result = orch
        .revoke(RevokeRequest {
            pat_id: pat_id(),
            tenant_id: tenant(),
            token_hash_key: key("cccccccccccccccc"),
            reason: RevocationReason::UserInitiated,
            revoked_by: principal(),
        })
        .await;
    match result {
        Err(RevocationError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
    assert_eq!(meta.audit_count().await, 0);
    assert_eq!(store.entry_count().await, 0);
}

#[tokio::test]
async fn revoke_kv_outage_soft_degrades() {
    use crate::cache::kv::AlwaysFailingKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = AlwaysFailingKv::new("kv outage");
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        kv,
        broadcast.clone(),
    );
    let pid = pat_id();
    let tid = tenant();
    let prid = principal();
    let cache_key = key("dddddddddddddddd");
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

    let resp = orch
        .revoke(RevokeRequest {
            pat_id: pid,
            tenant_id: tid,
            token_hash_key: cache_key,
            reason: RevocationReason::UserInitiated,
            revoked_by: prid,
        })
        .await
        .expect("revoke succeeds despite KV outage");
    assert!(resp.was_freshly_revoked);
    assert_eq!(resp.session_cache_invalidate, HookOutcome::SoftDegraded);
    // Neon SoT + DO upsert succeeded.
    assert_eq!(meta.audit_count().await, 1);
    assert_eq!(store.entry_count().await, 1);
}

#[tokio::test]
async fn revoke_queue_outage_dlq_fallback() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    broadcast.set_queue_down(true).await;
    let mut peers = BTreeSet::new();
    peers.insert(Region::Weur);
    let orch = build_orchestrator(
        Region::Wnam,
        peers,
        store.clone(),
        meta.clone(),
        kv,
        broadcast.clone(),
    );
    let pid = pat_id();
    let tid = tenant();
    let prid = principal();
    let cache_key = key("eeeeeeeeeeeeeeee");
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

    let resp = orch
        .revoke(RevokeRequest {
            pat_id: pid,
            tenant_id: tid,
            token_hash_key: cache_key,
            reason: RevocationReason::SecurityIncident,
            revoked_by: prid,
        })
        .await
        .expect("revoke ok despite queue outage");
    assert!(resp.was_freshly_revoked);
    assert_eq!(resp.broadcast_outcome, PropagationOutcome::DlqFallback);
    // Local revocation effective imediato.
    assert_eq!(meta.audit_count().await, 1);
    assert_eq!(store.entry_count().await, 1);
}

#[tokio::test]
async fn mass_revoke_atomicity_and_chunking() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let mut peers = BTreeSet::new();
    peers.insert(Region::Weur);
    let orch = build_orchestrator(
        Region::Wnam,
        peers,
        store.clone(),
        meta.clone(),
        kv,
        broadcast.clone(),
    );
    let tid = tenant();
    // Seed 2_500 PATs (multiple Phase 2 chunks at chunk size 1000).
    let mut ids = Vec::with_capacity(2_500);
    for i in 0..2_500u32 {
        let pid = pat_id();
        ids.push(pid);
        meta.seed_pat(
            pid,
            tid,
            principal(),
            key(&format!("{i:016x}")),
        )
        .await;
    }
    let resp = orch
        .mass_revoke(tid, RevocationReason::SecurityIncident, principal())
        .await
        .expect("mass_revoke ok");
    assert_eq!(resp.revoked_count, 2_500);
    assert_eq!(resp.outbox_inserted, 2_500);
    assert_eq!(meta.audit_count().await, 2_500);
    // All 2_500 entries broadcast (chunked into batches of 100).
    assert_eq!(broadcast.enqueued().await.len(), 2_500);
    assert_eq!(resp.broadcast_outcome, PropagationOutcome::Enqueued);
}

#[tokio::test]
async fn ingest_remote_idempotent_dedup() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Weur,
        BTreeSet::new(),
        store.clone(),
        meta,
        kv,
        broadcast,
    );
    let entry = RevokedEntry {
        pat_id: pat_id(),
        tenant_id: tenant(),
        revoked_by: principal(),
        revoked_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        reason: RevocationReason::UserInitiated,
        origin_region: Region::Wnam,
        mass_revoke_id: None,
    };
    let first = orch
        .ingest_remote(entry.clone(), None)
        .await
        .expect("ingest ok");
    let second = orch
        .ingest_remote(entry.clone(), None)
        .await
        .expect("ingest ok");
    assert!(first.was_fresh);
    assert!(!second.was_fresh);
    assert_eq!(store.entry_count().await, 1);
}

#[tokio::test]
async fn reconcile_clean_view_is_clean() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        kv,
        broadcast,
    );
    let summary = orch.reconcile(&[]).await.expect("reconcile ok");
    assert!(summary.is_clean());
}

#[tokio::test]
async fn reconcile_detects_neon_only_drift() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta,
        kv,
        broadcast,
    );
    let pid = pat_id();
    let neon = vec![(pid, SystemTime::UNIX_EPOCH + Duration::from_secs(123))];
    let summary = orch.reconcile(&neon).await.expect("reconcile ok");
    assert_eq!(summary.neon_only, vec![pid]);
    assert!(summary.do_only.is_empty());
    assert!(summary.timestamp_drift.is_empty());
}

#[tokio::test]
async fn reconcile_detects_timestamp_drift() {
    use crate::cache::kv::InMemoryKv;
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let kv = InMemoryKv::new();
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let orch = build_orchestrator(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        kv,
        broadcast,
    );
    let pid = pat_id();
    let tid = tenant();
    let prid = principal();
    let cache_key = key("ffffffffffffffff");
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
    let resp = orch
        .revoke(RevokeRequest {
            pat_id: pid,
            tenant_id: tid,
            token_hash_key: cache_key,
            reason: RevocationReason::UserInitiated,
            revoked_by: prid,
        })
        .await
        .expect("revoke ok");
    let bogus_neon_ts = resp.revoked_at + Duration::from_secs(60);
    let summary = orch.reconcile(&[(pid, bogus_neon_ts)]).await.expect("ok");
    assert_eq!(summary.timestamp_drift.len(), 1);
    let drift = summary
        .timestamp_drift
        .first()
        .expect("timestamp drift row must be present");
    assert_eq!(drift.pat_id, pid);
}

#[tokio::test]
async fn session_cache_key_parse_round_trip() {
    let raw = "auth:session:abc123";
    let key = SessionCacheKey::parse(raw).expect("parse ok");
    assert_eq!(key.as_str(), raw);
    assert_eq!(SessionCacheKey::parse("nope"), None);
    assert_eq!(SessionCacheKey::parse("auth:session:"), None);
}

#[tokio::test]
async fn revocation_reason_wire_round_trip() {
    let cases = [
        RevocationReason::UserInitiated,
        RevocationReason::AdminInitiated,
        RevocationReason::SecurityIncident,
        RevocationReason::Expired,
        RevocationReason::ScopeChanged,
        RevocationReason::MassRevoke,
    ];
    for c in cases {
        let s = c.as_wire();
        assert!(!s.is_empty());
    }
}
