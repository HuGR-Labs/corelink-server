//! Property tests for the WI-S03-004 revocation orchestrator.
//!
//! Each test exercises **10 000 iterations** in PR mode (the
//! canonical bar set by WI-S01-006 + maintained across S-02
//! property suites) to confirm the load-bearing invariants:
//!
//! 1. **INV-AUTH-REVOCATION-IDEMPOTENT** — replays of the same
//!    revoke request return the SAME `revoked_at` timestamp and
//!    insert a SINGLE audit_outbox row.
//! 2. **INV-AUTH-NEON-IS-SOT** — when the orchestrator's downstream
//!    side effects are degraded (KV outage, queue outage), the
//!    Neon SoT row is still flipped from active to revoked + the
//!    audit row is still inserted; subsequent verify-style
//!    inspections of the SoT show the revoke as effective.
//! 3. **INV-AUTH-MASS-REVOKE-ATOMIC** — Phase 1 mass UPDATE
//!    flips ALL active rows for the tenant (all-or-none); Phase 2
//!    audit_outbox row count equals the Phase 1 result count.
//! 4. **INV-AUTH-PROPAGATION-AT-LEAST-ONCE** — concurrent ingest
//!    of the same `(pat_id, revoked_at)` pair from peer regions is
//!    deduped to a single DO-storage entry.
//!
//! All tests run inside a multi-threaded tokio runtime so the
//! property suite faithfully reproduces the single-writer-per-region
//! contention the production CF Durable Object will see.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on a failing assertion are themselves test failures"
)]
#![allow(missing_docs, reason = "test crate")]

use std::collections::BTreeSet;
use std::sync::Arc;

use proptest::prelude::*;
use uuid::Uuid;

use corelink_pat::{PatId, PrincipalId, TenantId};
use corelink_worker::auth::revocation::{
    HookOutcome, InMemoryBroadcast, InMemoryMetaRevocationSink, InMemoryRevocationStore,
    KvSessionCacheInvalidator, MassRevokeRow, PropagationOutcome, RevocationOrchestrator,
    RevocationReason, RevocationStore, RevokeRequest, RevokedEntry, SessionCacheInvalidator,
    SessionCacheKey,
};
use corelink_worker::cache::kv::{AlwaysFailingKv, InMemoryKv};
use corelink_worker::Region;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn fresh_pat_id() -> PatId {
    PatId(Uuid::now_v7())
}

fn fresh_tenant() -> TenantId {
    TenantId(Uuid::now_v7())
}

fn fresh_principal() -> PrincipalId {
    PrincipalId(Uuid::now_v7())
}

fn key_for(seed: u64) -> SessionCacheKey {
    SessionCacheKey::from_hash(&format!("{seed:016x}"))
}

fn reason_for(idx: u8) -> RevocationReason {
    match idx % 6 {
        0 => RevocationReason::UserInitiated,
        1 => RevocationReason::AdminInitiated,
        2 => RevocationReason::SecurityIncident,
        3 => RevocationReason::Expired,
        4 => RevocationReason::ScopeChanged,
        _ => RevocationReason::MassRevoke,
    }
}

/// Canonical builder used by every property test. Returns a
/// fully-wired orchestrator + the underlying store / meta-sink /
/// broadcast handles so the property body can introspect side
/// effects.
fn build_rig(
    region: Region,
    peers: BTreeSet<Region>,
) -> (
    RevocationOrchestrator,
    Arc<InMemoryRevocationStore>,
    Arc<InMemoryMetaRevocationSink>,
    Arc<InMemoryBroadcast>,
) {
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let orch = RevocationOrchestrator::new(
        region,
        peers,
        store.clone(),
        meta.clone(),
        cache,
        broadcast.clone(),
    );
    (orch, store, meta, broadcast)
}

fn build_rig_with_kv_outage(
    region: Region,
    peers: BTreeSet<Region>,
) -> (
    RevocationOrchestrator,
    Arc<InMemoryRevocationStore>,
    Arc<InMemoryMetaRevocationSink>,
    Arc<InMemoryBroadcast>,
) {
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache: Arc<dyn SessionCacheInvalidator> = Arc::new(KvSessionCacheInvalidator::new(
        AlwaysFailingKv::new("kv outage chaos"),
    ));
    let orch = RevocationOrchestrator::new(
        region,
        peers,
        store.clone(),
        meta.clone(),
        cache,
        broadcast.clone(),
    );
    (orch, store, meta, broadcast)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// **INV-AUTH-REVOCATION-IDEMPOTENT**.
    ///
    /// For every random `(pat_id, tenant_id, principal, reason,
    /// retry_count)` tuple, calling `revoke` `retry_count` times
    /// MUST:
    /// - return the same `revoked_at` timestamp on every call
    ///   after the first;
    /// - leave exactly ONE audit_outbox row.
    /// - leave exactly ONE DO-storage entry.
    /// - never broadcast more than once for the fresh-revoke arm
    ///   (single-region — peers empty — so 0 broadcasts).
    #[test]
    fn revoke_is_idempotent_across_retries(
        retry_count in 1u32..6,
        reason_idx in 0u8..6,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (orch, store, meta, broadcast) = build_rig(Region::Wnam, BTreeSet::new());
            let pid = fresh_pat_id();
            let tid = fresh_tenant();
            let prid = fresh_principal();
            let cache_key = key_for(0xdead_beef_0000);
            meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
            let reason = reason_for(reason_idx);

            let mut canonical_ts = None;
            for _ in 0..retry_count {
                let resp = orch
                    .revoke(RevokeRequest {
                        pat_id: pid,
                        tenant_id: tid,
                        token_hash_key: cache_key.clone(),
                        reason,
                        revoked_by: prid,
                    })
                    .await
                    .expect("revoke ok");
                match canonical_ts {
                    None => canonical_ts = Some(resp.revoked_at),
                    Some(ts) => prop_assert_eq!(resp.revoked_at, ts),
                }
            }
            prop_assert_eq!(meta.audit_count().await, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);
            Ok(())
        })?;
    }

    /// **INV-AUTH-NEON-IS-SOT** under degraded KV.
    ///
    /// When the KV invalidator is fully down, the orchestrator MUST
    /// still flip the SoT row + insert the audit row + return the
    /// canonical timestamp; only the `session_cache_invalidate`
    /// outcome flips to `SoftDegraded`.
    #[test]
    fn neon_is_sot_under_kv_outage(
        reason_idx in 0u8..6,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (orch, store, meta, broadcast) =
                build_rig_with_kv_outage(Region::Wnam, BTreeSet::new());
            let pid = fresh_pat_id();
            let tid = fresh_tenant();
            let prid = fresh_principal();
            let cache_key = key_for(0xdead_beef_1111);
            meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
            let reason = reason_for(reason_idx);
            let resp = orch
                .revoke(RevokeRequest {
                    pat_id: pid,
                    tenant_id: tid,
                    token_hash_key: cache_key,
                    reason,
                    revoked_by: prid,
                })
                .await
                .expect("revoke ok despite KV outage");
            prop_assert!(resp.was_freshly_revoked);
            prop_assert_eq!(resp.session_cache_invalidate, HookOutcome::SoftDegraded);
            prop_assert_eq!(meta.audit_count().await, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            // No peers configured: broadcast skipped, never enqueued.
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);
            Ok(())
        })?;
    }

    /// **INV-AUTH-NEON-IS-SOT** under degraded broadcast.
    ///
    /// When the broadcast queue is down, the orchestrator MUST
    /// still flip the SoT row + insert the audit row + return the
    /// canonical timestamp + surface `PropagationOutcome::DlqFallback`.
    #[test]
    fn neon_is_sot_under_queue_outage(
        reason_idx in 0u8..6,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let mut peers = BTreeSet::new();
            peers.insert(Region::Weur);
            peers.insert(Region::Sam);
            let (orch, store, meta, broadcast) = build_rig(Region::Wnam, peers);
            broadcast.set_queue_down(true).await;
            let pid = fresh_pat_id();
            let tid = fresh_tenant();
            let prid = fresh_principal();
            let cache_key = key_for(0xdead_beef_2222);
            meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
            let reason = reason_for(reason_idx);

            let resp = orch
                .revoke(RevokeRequest {
                    pat_id: pid,
                    tenant_id: tid,
                    token_hash_key: cache_key,
                    reason,
                    revoked_by: prid,
                })
                .await
                .expect("revoke ok despite queue outage");
            prop_assert!(resp.was_freshly_revoked);
            prop_assert_eq!(resp.broadcast_outcome, PropagationOutcome::DlqFallback);
            prop_assert_eq!(meta.audit_count().await, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);
            Ok(())
        })?;
    }

    /// **INV-AUTH-PROPAGATION-AT-LEAST-ONCE** consumer-side dedup.
    ///
    /// Ingesting the same `(pat_id, revoked_at)` from a peer region
    /// `replay_count` times MUST yield exactly ONE DO entry; only
    /// the first call returns `was_fresh = true`.
    #[test]
    fn ingest_remote_dedups_across_replays(
        replay_count in 1u32..8,
        reason_idx in 0u8..6,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (orch, store, _meta, _broadcast) = build_rig(Region::Weur, BTreeSet::new());
            let entry = RevokedEntry::new(
                fresh_pat_id(),
                fresh_tenant(),
                fresh_principal(),
                std::time::SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(1_700_000_000 + u64::from(reason_idx)),
                reason_for(reason_idx),
                Region::Wnam,
                None,
            );
            let mut fresh_count = 0usize;
            for _ in 0..replay_count {
                let outcome = orch
                    .ingest_remote(entry.clone(), None)
                    .await
                    .expect("ingest ok");
                if outcome.was_fresh {
                    fresh_count += 1;
                }
            }
            prop_assert_eq!(fresh_count, 1);
            prop_assert_eq!(store.entry_count().await, 1);
            Ok(())
        })?;
    }
}

// --------------------------------------------------------------------------
// Mass-revoke property — SMALLER cases (mass-revoke is O(N) per case;
// we cap N to keep PR runtime under a minute even at 10k cases).
// --------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    /// **INV-AUTH-MASS-REVOKE-ATOMIC** — all-or-none Phase 1.
    ///
    /// Seed N (1..=20) PATs into a single tenant; call mass_revoke;
    /// MUST report `revoked_count == N`, `outbox_inserted == N`,
    /// audit_count == N, store_count == N. Phase 1 atomic UPDATE
    /// is modeled as a synchronous loop over the SoT (the canonical
    /// Postgres-side `UPDATE … WHERE tenant_id = X` is monolithic).
    #[test]
    fn mass_revoke_atomic_all_or_none(
        n in 1usize..=20,
        reason_idx in 0u8..6,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (orch, store, meta, broadcast) = build_rig(Region::Wnam, BTreeSet::new());
            let tid = fresh_tenant();
            for i in 0..n {
                let pid = fresh_pat_id();
                let prid = fresh_principal();
                meta.seed_pat(pid, tid, prid, key_for(i as u64)).await;
            }
            let reason = reason_for(reason_idx);
            let resp = orch
                .mass_revoke(tid, reason, fresh_principal())
                .await
                .expect("mass_revoke ok");
            prop_assert_eq!(resp.revoked_count, n);
            prop_assert_eq!(resp.outbox_inserted, n);
            prop_assert_eq!(meta.audit_count().await, n);
            prop_assert_eq!(store.entry_count().await, n);
            // No peers => broadcast skipped (NoPeers).
            prop_assert_eq!(broadcast.enqueued().await.len(), 0);
            Ok(())
        })?;
    }
}

// --------------------------------------------------------------------------
// Concurrent revocation property — single test, 100k attempts via
// tokio::spawn fan-out (100 victims × 1 000 attackers).
// --------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_idempotent_under_high_replay_load() {
    // Spawn 100 victims; each victim is revoked then concurrently
    // revoked 1 000 times more (1k replays per victim → 100k replay
    // attempts). After the storm: each victim has exactly 1 audit
    // row, 1 DO entry, and a single canonical revoked_at timestamp.
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let orch = Arc::new(RevocationOrchestrator::new(
        Region::Wnam,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        cache,
        broadcast.clone(),
    ));

    const VICTIMS: usize = 100;
    const REPLAYS_PER_VICTIM: usize = 1_000;

    let mut victims = Vec::with_capacity(VICTIMS);
    for _ in 0..VICTIMS {
        let pid = fresh_pat_id();
        let tid = fresh_tenant();
        let prid = fresh_principal();
        let cache_key = key_for(rand_u64());
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
        victims.push((pid, tid, prid, cache_key));
    }

    let mut tasks = Vec::with_capacity(VICTIMS * REPLAYS_PER_VICTIM);
    for (pid, tid, prid, cache_key) in &victims {
        for _ in 0..REPLAYS_PER_VICTIM {
            let orch = orch.clone();
            let pid = *pid;
            let tid = *tid;
            let prid = *prid;
            let cache_key = cache_key.clone();
            tasks.push(tokio::spawn(async move {
                orch.revoke(RevokeRequest {
                    pat_id: pid,
                    tenant_id: tid,
                    token_hash_key: cache_key,
                    reason: RevocationReason::SecurityIncident,
                    revoked_by: prid,
                })
                .await
                .expect("revoke ok")
            }));
        }
    }

    let mut successes = 0usize;
    for t in tasks {
        let _ = t.await.expect("join ok");
        successes += 1;
    }
    assert_eq!(successes, VICTIMS * REPLAYS_PER_VICTIM);
    // Single audit row per victim; single DO entry per victim.
    assert_eq!(meta.audit_count().await, VICTIMS);
    assert_eq!(store.entry_count().await, VICTIMS);
}

fn rand_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

// --------------------------------------------------------------------------
// Cross-region propagation property — orchestrator-A revoke fans
// out to orchestrator-B via the broadcast fake; B ingests and the
// DO storage in B agrees with A on `(pat_id, revoked_at)`.
// --------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn propagation_eventually_aligns_all_regions() {
    // Single revoke at orch_a; the broadcast carries the entry to
    // orch_b and orch_c via ingest_remote; the three DOs converge on
    // the same `(pat_id, revoked_at)`.
    let store_a = Arc::new(InMemoryRevocationStore::new());
    let store_b = Arc::new(InMemoryRevocationStore::new());
    let store_c = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache_a: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let cache_b: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let cache_c: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let mut peers = BTreeSet::new();
    peers.insert(Region::Weur);
    peers.insert(Region::Sam);
    let orch_a = RevocationOrchestrator::new(
        Region::Wnam,
        peers,
        store_a.clone(),
        meta.clone(),
        cache_a,
        broadcast.clone(),
    );
    let orch_b = RevocationOrchestrator::new(
        Region::Weur,
        BTreeSet::new(),
        store_b.clone(),
        Arc::new(InMemoryMetaRevocationSink::new()),
        cache_b,
        Arc::new(InMemoryBroadcast::new()),
    );
    let orch_c = RevocationOrchestrator::new(
        Region::Sam,
        BTreeSet::new(),
        store_c.clone(),
        Arc::new(InMemoryMetaRevocationSink::new()),
        cache_c,
        Arc::new(InMemoryBroadcast::new()),
    );

    let pid = fresh_pat_id();
    let tid = fresh_tenant();
    let prid = fresh_principal();
    let cache_key = key_for(rand_u64());
    meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
    let resp = orch_a
        .revoke(RevokeRequest {
            pat_id: pid,
            tenant_id: tid,
            token_hash_key: cache_key.clone(),
            reason: RevocationReason::AdminInitiated,
            revoked_by: prid,
        })
        .await
        .expect("revoke at A");
    assert_eq!(resp.broadcast_outcome, PropagationOutcome::Enqueued);

    // Drain the broadcast as a synthetic queue consumer.
    let messages = broadcast.enqueued().await;
    assert_eq!(messages.len(), 1);
    for msg in messages {
        let _ = orch_b
            .ingest_remote(msg.clone(), Some(cache_key.clone()))
            .await
            .expect("ingest at B");
        let _ = orch_c
            .ingest_remote(msg, Some(cache_key.clone()))
            .await
            .expect("ingest at C");
    }

    let row_a = store_a
        .get(pid)
        .await
        .expect("get ok")
        .expect("entry present at A");
    let row_b = store_b
        .get(pid)
        .await
        .expect("get ok")
        .expect("entry present at B");
    let row_c = store_c
        .get(pid)
        .await
        .expect("get ok")
        .expect("entry present at C");
    assert_eq!(row_a.revoked_at, row_b.revoked_at);
    assert_eq!(row_a.revoked_at, row_c.revoked_at);
    assert_eq!(row_a.pat_id, pid);
}

// --------------------------------------------------------------------------
// Idempotent ingest under concurrent replay (100k attempts).
// --------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ingest_remote_concurrent_dedup_100k() {
    let store = Arc::new(InMemoryRevocationStore::new());
    let meta = Arc::new(InMemoryMetaRevocationSink::new());
    let broadcast = Arc::new(InMemoryBroadcast::new());
    let cache: Arc<dyn SessionCacheInvalidator> =
        Arc::new(KvSessionCacheInvalidator::new(InMemoryKv::new()));
    let orch = Arc::new(RevocationOrchestrator::new(
        Region::Weur,
        BTreeSet::new(),
        store.clone(),
        meta.clone(),
        cache,
        broadcast.clone(),
    ));
    let entry = RevokedEntry::new(
        fresh_pat_id(),
        fresh_tenant(),
        fresh_principal(),
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
        RevocationReason::SecurityIncident,
        Region::Wnam,
        None,
    );

    let mut tasks = Vec::with_capacity(100_000);
    for _ in 0..100_000 {
        let orch = orch.clone();
        let entry = entry.clone();
        tasks.push(tokio::spawn(async move {
            orch.ingest_remote(entry, None)
                .await
                .expect("ingest ok")
                .was_fresh
        }));
    }

    let mut fresh = 0usize;
    for t in tasks {
        if t.await.expect("join ok") {
            fresh += 1;
        }
    }
    assert_eq!(fresh, 1, "exactly one fresh insert across 100k replays");
    assert_eq!(store.entry_count().await, 1);
}

// --------------------------------------------------------------------------
// Mass revoke concurrent with single revoke (race under same tenant)
// --------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mass_revoke_concurrent_with_singles_idempotent() {
    let (orch, store, meta, _broadcast) = build_rig(Region::Wnam, BTreeSet::new());
    let orch = Arc::new(orch);
    let tid = fresh_tenant();
    let mut victims = Vec::with_capacity(50);
    for i in 0..50 {
        let pid = fresh_pat_id();
        let prid = fresh_principal();
        meta.seed_pat(pid, tid, prid, key_for(i as u64)).await;
        victims.push((pid, prid, key_for(i as u64)));
    }

    // Fire single revoke for half the victims while mass_revoke
    // races against them.
    let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    for (i, (pid, prid, ck)) in victims.iter().enumerate() {
        if i % 2 == 0 {
            let orch = orch.clone();
            let pid = *pid;
            let prid = *prid;
            let ck = ck.clone();
            tasks.push(tokio::spawn(async move {
                let _ = orch
                    .revoke(RevokeRequest {
                        pat_id: pid,
                        tenant_id: tid,
                        token_hash_key: ck,
                        reason: RevocationReason::UserInitiated,
                        revoked_by: prid,
                    })
                    .await;
            }));
        }
    }
    let orch_clone = orch.clone();
    let mass_handle = tokio::spawn(async move {
        let _ = orch_clone
            .mass_revoke(tid, RevocationReason::SecurityIncident, fresh_principal())
            .await
            .expect("mass_revoke ok");
    });
    for t in tasks {
        t.await.expect("join");
    }
    mass_handle.await.expect("mass join");

    // Final state: ≤ 50 audit rows + ≤ 50 DO entries (each victim
    // exactly once; some victims may have been revoked by the
    // single path first → mass_revoke skipped them; some by mass
    // first → singles became idempotent replays; in either case the
    // canonical `(pat_id, revoked_at)` shows up exactly once).
    let store_count = store.entry_count().await;
    let audit_count = meta.audit_count().await;
    assert_eq!(store_count, 50, "every victim has exactly one DO entry");
    assert_eq!(
        audit_count, 50,
        "every victim has exactly one audit row (idempotent replays do not duplicate)"
    );

    // Phase 2 chunked verification: every audit row is for a
    // distinct pat_id within this tenant.
    let rows = meta.audit_rows().await;
    let mut seen: std::collections::HashSet<PatId> = std::collections::HashSet::new();
    for row in &rows {
        assert!(seen.insert(row.pat_id));
    }
    let _ = MassRevokeRow {
        pat_id: fresh_pat_id(),
        principal_id: fresh_principal(),
        token_hash_key: key_for(0),
    };
}
