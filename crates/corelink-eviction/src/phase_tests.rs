use super::*;
use crate::audit::InMemoryEvictionAuditSink;
use crate::blob_meta::{EvictionBlobDigest, InMemoryBlobMetaSoftDeleteStore};
use crate::metrics::InMemoryEvictionMetrics;
use crate::reachable::InMemoryAcReferenceProbe;
use crate::storage_state::{InMemoryTenantStorageStateStore, TenantStorageStateRow};

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

fn dig(seed: u8) -> EvictionBlobDigest {
    let bytes = [seed; 32];
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    EvictionBlobDigest::new(s).unwrap()
}

fn lru_row(seed: u8, last_accessed: u64, size: u64) -> BlobLruRow {
    BlobLruRow {
        digest: dig(seed),
        size_bytes: size,
        created_at_ms: 1,
        last_accessed_at_ms: last_accessed,
        deleted_at_ms: None,
    }
}

fn state_row(tenant: Uuid, region: EvictionRegion, used: u64, quota: u64) -> TenantStorageStateRow {
    TenantStorageStateRow {
        tenant_id: tenant,
        region,
        bytes_used: used,
        bytes_quota: quota,
        bytes_used_updated_at_ms: 1,
        last_synced_at_ms: 1,
        last_evict_at_ms: None,
        bytes_reclaimed_lifetime: 0,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

type Fixture = (
    InMemoryEvictionPhase<
        InMemoryBlobMetaSoftDeleteStore,
        InMemoryAcReferenceProbe,
        InMemoryTenantStorageStateStore,
        InMemoryEvictionAuditSink,
        InMemoryEvictionMetrics,
        CountingEvictionClock,
    >,
    Arc<InMemoryBlobMetaSoftDeleteStore>,
    Arc<InMemoryAcReferenceProbe>,
    Arc<InMemoryTenantStorageStateStore>,
    Arc<InMemoryEvictionAuditSink>,
    Arc<InMemoryEvictionMetrics>,
);

fn fresh(start_ms: u64, config: EvictionConfig) -> Fixture {
    let blob_meta = Arc::new(InMemoryBlobMetaSoftDeleteStore::new());
    let ac_probe = Arc::new(InMemoryAcReferenceProbe::new());
    let storage_state = Arc::new(InMemoryTenantStorageStateStore::new());
    let audit = Arc::new(InMemoryEvictionAuditSink::new());
    let metrics = Arc::new(InMemoryEvictionMetrics::new());
    let clock = Arc::new(CountingEvictionClock::new(start_ms));
    let phase = InMemoryEvictionPhase::new(
        Arc::clone(&blob_meta),
        Arc::clone(&ac_probe),
        Arc::clone(&storage_state),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
        config,
    );
    (phase, blob_meta, ac_probe, storage_state, audit, metrics)
}

// ---- canonical constants ---------------------------------------

#[test]
fn canonical_constants_pinned() {
    assert!((QUOTA_TRIGGER_THRESHOLD_PCT - 0.95).abs() < f64::EPSILON);
    assert!((QUOTA_TARGET_HEADROOM_PCT - 0.90).abs() < f64::EPSILON);
    assert_eq!(EVICTION_COOLDOWN_MS, 60 * 60 * 1000);
    assert_eq!(MAX_LRU_BATCH_SIZE, 250);
    assert_eq!(CANONICAL_EVICTION_PHASE_BUDGET_MS, 5 * 60 * 1000);
}

#[test]
fn config_default_for_free_tier() {
    let c = EvictionConfig::default_for_free_tier();
    assert_eq!(c.tier(), Tier::Free);
    assert_eq!(c.ttl_ms(), 7 * 86_400_000);
    assert_eq!(c.phase_budget_ms(), CANONICAL_EVICTION_PHASE_BUDGET_MS);
    assert_eq!(c.max_lru_batch_size(), MAX_LRU_BATCH_SIZE);
}

#[test]
fn config_enterprise_with_override_validates() {
    let c = EvictionConfig::new(Tier::Enterprise, Some(500)).unwrap();
    assert_eq!(c.ttl_ms(), 500 * 86_400_000);
}

#[test]
fn config_enterprise_above_cap_rejects_at_construction() {
    let err = EvictionConfig::new(Tier::Enterprise, Some(800)).unwrap_err();
    assert!(matches!(
        err,
        crate::tier::TierTtlOverrideError::ExceedsMaxTtl { .. }
    ));
}

// ---- step_candidate ---------------------------------------------

#[test]
fn step_already_soft_deleted_yields_skip_ttl() {
    let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
    let mut row = lru_row(1, 50, 1024);
    row.deleted_at_ms = Some(800);
    let d = phase
        .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
        .unwrap();
    assert!(matches!(d, EvictionDecision::SkipTtlNotExpired { .. }));
}

#[test]
fn step_within_ttl_window_yields_skip_ttl() {
    let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
    // last_accessed = 6000 >= cutoff = 5000 → skip TTL.
    let row = lru_row(1, 6000, 1024);
    let d = phase
        .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
        .unwrap();
    assert!(matches!(d, EvictionDecision::SkipTtlNotExpired { .. }));
}

#[test]
fn step_reachable_yields_skip_reachable() {
    let (phase, b, x, _s, _a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
    let row = lru_row(1, 100, 1024);
    b.push_row(ten_a(), row.clone()).unwrap();
    // Push an AC ref older than evict_started_at_ms = 10000.
    x.push_ac_row(ten_a(), "ac-1", vec![row.digest.clone()], 5000, None)
        .unwrap();
    let d = phase
        .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
        .unwrap();
    assert!(matches!(d, EvictionDecision::SkipReachable { .. }));
    assert_eq!(
        m.counter_total(crate::metrics::EvictionMetricKind::CascadePrevented),
        1
    );
}

#[test]
fn step_evict_path_emits_audit_and_metrics() {
    let (phase, b, _x, _s, a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
    let row = lru_row(1, 100, 4096);
    b.push_row(ten_a(), row.clone()).unwrap();
    let d = phase
        .step_candidate(ten_a(), EvictionRegion::Sam, &row, 10_000, 5_000)
        .unwrap();
    match d {
        EvictionDecision::Evict {
            bytes_reclaimed, ..
        } => {
            assert_eq!(bytes_reclaimed, 4096);
        }
        other => panic!("expected Evict, got {other:?}"),
    }
    assert_eq!(a.snapshot_of(EvictionEventType::Evicted).len(), 1);
    assert_eq!(
        m.counter_total(crate::metrics::EvictionMetricKind::BytesReclaimed),
        4096
    );
    assert_eq!(
        m.counter_total(crate::metrics::EvictionMetricKind::TtlExpired),
        1
    );
    assert_eq!(
        m.counter_total(crate::metrics::EvictionMetricKind::LruEvicted),
        1
    );
    // blob_meta now soft-deleted.
    let snap = b.snapshot(ten_a(), &row.digest).unwrap();
    assert!(snap.deleted_at_ms.is_some());
}

// ---- execute_daily ----------------------------------------------

#[test]
fn execute_daily_returns_storage_state_missing() {
    let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
    let err = phase
        .execute_daily(ten_a(), EvictionRegion::Sam)
        .unwrap_err();
    assert!(matches!(
        err,
        EvictionError::TenantStorageStateMissing { .. }
    ));
}

#[test]
fn execute_daily_below_threshold_skips_quota_ok() {
    let (phase, _b, _x, s, a, m) = fresh(1000, EvictionConfig::default_for_free_tier());
    s.push_row(state_row(ten_a(), EvictionRegion::Sam, 50, 100))
        .unwrap();
    let r = phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
    assert_eq!(r.skipped_quota_ok_count, 1);
    assert_eq!(r.blobs_evicted_count, 0);
    assert!(!r.quota_trigger_fired);
    assert_eq!(a.snapshot_of(EvictionEventType::SkippedQuotaOk).len(), 1);
    assert_eq!(
        m.counter_total(crate::metrics::EvictionMetricKind::CronFired),
        1
    );
}

#[test]
fn execute_daily_at_threshold_fires_trigger_and_evicts() {
    // Start clock far enough into the future that `cutoff_ms =
    // now - 7d` is well above 0 (otherwise saturating-sub
    // collapses cutoff to 0 and a row with last_accessed=1
    // doesn't satisfy `< cutoff`).
    let start = 30_u64 * 86_400_000; // 30d in ms
    let (phase, b, _x, s, a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
    // Tenant at 96% quota.
    s.push_row(state_row(ten_a(), EvictionRegion::Sam, 96, 100))
        .unwrap();
    // Push a cold blob: last_accessed = 1 << cutoff (start - 7d).
    let cold = lru_row(1, 1, 30);
    b.push_row(ten_a(), cold).unwrap();
    let r = phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
    assert!(r.quota_trigger_fired);
    assert_eq!(r.blobs_evicted_count, 1);
    assert_eq!(r.bytes_reclaimed, 30);
    // QuotaTriggerFired audit emit.
    assert_eq!(a.snapshot_of(EvictionEventType::QuotaTriggerFired).len(), 1);
    // Storage state row was reclaimed (96 - 30 = 66).
    let row = s.snapshot(ten_a(), EvictionRegion::Sam).unwrap();
    assert_eq!(row.bytes_used, 66);
    assert_eq!(row.bytes_reclaimed_lifetime, 30);
}

// ---- execute_quota_trigger --------------------------------------

#[test]
fn execute_quota_trigger_short_circuits_at_target() {
    let start = 30_u64 * 86_400_000;
    let (phase, b, _x, s, _a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
    s.push_row(state_row(ten_a(), EvictionRegion::Sam, 200, 100))
        .unwrap();
    // 5 cold blobs of 50 bytes each = 250 reclaimable.
    for i in 0..5 {
        b.push_row(ten_a(), lru_row(i, 1 + u64::from(i), 50))
            .unwrap();
    }
    // Target = 100 bytes — the loop should evict 2 blobs and stop.
    let r = phase
        .execute_quota_trigger(ten_a(), EvictionRegion::Sam, 100)
        .unwrap();
    assert!(r.bytes_reclaimed >= 100);
    // Should NOT have evicted all 5.
    assert!(r.blobs_evicted_count < 5);
}

// ---- tenant isolation -------------------------------------------

#[test]
fn tenant_isolation_eviction_does_not_touch_other_tenant() {
    let start = 30_u64 * 86_400_000;
    let (phase, b, _x, s, _a, _m) = fresh(start, EvictionConfig::default_for_free_tier());
    s.push_row(state_row(ten_a(), EvictionRegion::Sam, 100, 100))
        .unwrap();
    s.push_row(state_row(ten_b(), EvictionRegion::Sam, 50, 100))
        .unwrap();
    // Cold blob for A.
    let blob_a = lru_row(1, 1, 30);
    b.push_row(ten_a(), blob_a.clone()).unwrap();
    // Cold blob for B (would be eligible if scanned).
    let blob_b = lru_row(2, 1, 30);
    b.push_row(ten_b(), blob_b.clone()).unwrap();
    phase.execute_daily(ten_a(), EvictionRegion::Sam).unwrap();
    // Tenant B's blob is untouched.
    let snap_b = b.snapshot(ten_b(), &blob_b.digest).unwrap();
    assert!(snap_b.deleted_at_ms.is_none());
}

// ---- BLOB-only scope --------------------------------------------

#[test]
fn blob_only_scope_chunks_table_never_consulted() {
    // The InMemoryEvictionPhase has NO chunks-table dependency.
    // This test pins the wiring contract: the orchestrator's
    // type signature expects only blob_meta + ac_probe +
    // storage_state — adding a chunks-table store would be a
    // BREAKING change visible at compile time.
    let (phase, _b, _x, _s, _a, _m) = fresh(1000, EvictionConfig::default_for_free_tier());
    // Sanity: the config is the only field we expose.
    let c = phase.config();
    assert_eq!(c.tier(), Tier::Free);
}
