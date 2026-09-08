/// A tombstone-gate transport FAULT fails CLOSED on both read and write
/// (UNAVAILABLE sentinel → 503) — never serve/commit when the erasure gate
/// cannot be consulted (PEN-2/REV-S1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_gate_fault_fails_closed() {
    let ts: Arc<dyn TombstoneStore> = Arc::new(ErroringInner);
    let (gated, backing) = gated_with(ts);
    match gated.read(read_req()) {
        Err(CasHandlerError::Internal(msg)) => {
            assert!(
                msg.starts_with(TOMBSTONE_UNAVAILABLE_SENTINEL),
                "read fail-closed, got {msg}"
            );
        }
        other => panic!("read must fail closed on gate fault, got {other:?}"),
    }
    match gated.write(write_req()) {
        Err(CasHandlerError::Internal(msg)) => {
            assert!(
                msg.starts_with(TOMBSTONE_UNAVAILABLE_SENTINEL),
                "write fail-closed, got {msg}"
            );
        }
        other => panic!("write must fail closed on gate fault, got {other:?}"),
    }
    assert!(
        lock_or_recover(&backing.wrote).is_empty(),
        "no write on gate fault"
    );
}

/// DELETE is a pass-through even for a tombstoned blob (idempotent erase
/// must never be blocked by its own tombstone).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_delete_passes_through_even_when_tombstoned() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
    let (gated, backing) = gated_with(ts);
    let req = CasDeleteRequest::new(
        TENANT.to_owned(),
        DIGEST.to_owned(),
        format!("anon@{TENANT}"),
        TENANT.to_owned(),
        0,
    );
    gated.delete(req).expect("delete passes through");
    assert_eq!(lock_or_recover(&backing.deleted).len(), 1);
}

// ──────────────────────────────────────────────────────────────────────
// H3 — resurrection guard (authoritative write gate) + bloom-map eviction
// ──────────────────────────────────────────────────────────────────────

/// H3 (resurrection): after a SEPARATE writer (another container instance /
/// region) erases a hash STRAIGHT to D1 — invisible to THIS instance's warm
/// bloom within the staleness window — a re-PUT of that hash MUST be REFUSED
/// (GONE), never silently resurrected. The READ fast-path is (tolerably)
/// within-window blind (the bytes are already gone from R2), but the WRITE
/// gate is authoritative. FAILS before the fix (write trusted the stale
/// bloom `Ok(false)` → resurrection); PASSES after.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_after_cross_writer_erase_within_window_is_refused() {
    let inner = Arc::new(CountingInner::new());
    // LONG window: once warm, the bloom is NOT re-stamped, so a cross-writer
    // erase that lands AFTER the load is invisible to the fast path — the
    // exact H3 resurrection window.
    let bloom = Arc::new(BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    ));
    // Warm the tenant bloom past its one-shot just-reloaded epoch (the bloom
    // was seeded while the inner set was still empty).
    warm(&bloom, TENANT).await;
    // Another instance erases the hash DIRECTLY in D1 (never through this
    // bloom), AFTER this bloom was loaded.
    inner.seed_inner(TENANT, DIGEST);
    // The READ fast path is fooled within the window (the tolerable case:
    // R2 bytes already gone, a slipped GET 404s).
    assert!(
        !bloom.is_tombstoned(TENANT, DIGEST).await.expect("read q"),
        "read fast-path is within-window blind (expected)"
    );
    // The AUTHORITATIVE write gate is NOT fooled — the re-PUT is refused GONE.
    let (gated, backing) = gated_with(bloom);
    match gated.write(write_req()) {
        Err(CasHandlerError::Internal(msg)) => assert!(
            msg.starts_with(TOMBSTONE_GONE_SENTINEL),
            "H3: re-PUT after a cross-writer erase must be GONE, got {msg}"
        ),
        other => {
            panic!("H3: re-PUT of a cross-writer-erased blob must be refused, got {other:?}")
        }
    }
    assert!(
        lock_or_recover(&backing.wrote).is_empty(),
        "H3: erased bytes must NOT be resurrected"
    );
}

/// H3 (resurrection, control): a genuinely-LIVE hash still writes normally
/// through the authoritative gate (the fix does not block legitimate PUTs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_of_live_hash_through_bloom_still_commits() {
    let inner = Arc::new(CountingInner::new());
    let bloom = Arc::new(BloomTombstoneStore::with_params(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    ));
    warm(&bloom, TENANT).await;
    let (gated, backing) = gated_with(bloom);
    let resp = gated.write(write_req()).expect("live write commits");
    assert!(resp.durable);
    assert_eq!(lock_or_recover(&backing.wrote).len(), 1);
}

/// H3 (OOM): the per-tenant bloom map is LRU-BOUNDED. Touching far more
/// distinct tenants than the cap never grows the map past the cap, and the
/// live-count gauge tracks it. Pre-fix the map grew one 128 KiB bloom per
/// tenant forever (≈1.3 GB @ 10k tenants ⇒ OOM on the capped container).
#[tokio::test]
async fn bloom_map_is_lru_bounded_under_many_tenants() {
    const CAP: usize = 8;
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params_capped(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
        CAP,
    );
    // Touch 200 distinct tenants — each first touch mints (reloads) a bloom.
    for i in 0..200u32 {
        let tenant = format!("tenant-{i:04}");
        let _ = store.is_tombstoned(&tenant, DIGEST).await.expect("q");
    }
    // The map (and its gauge) never exceeds the cap despite 200 tenants.
    assert!(
        store.tenant_bloom_count() <= CAP as u64,
        "bloom map must stay LRU-bounded: {} > {CAP}",
        store.tenant_bloom_count()
    );
    // The gauge is live and the map fills exactly to the cap (evict-one-per
    // -new-insert steady state).
    assert_eq!(
        store.tenant_bloom_count(),
        CAP as u64,
        "map fills to — and holds at — the cap"
    );
}

/// Adversarial tuning cannot bypass the shared 2048-tenant/128 KiB
/// geometry: oversized constructor values are clamped before allocation.
#[test]
fn bloom_tuning_is_clamped_to_declared_capacity() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params_capped(
        inner,
        usize::MAX,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
        usize::MAX,
    );
    assert_eq!(store.bits, DEFAULT_BLOOM_BITS);
    assert_eq!(store.max_tenants, DEFAULT_MAX_TENANT_BLOOMS);
}

/// H3 (eviction correctness): evicting a tenant's bloom is safe — a
/// subsequent lookup for an evicted tenant reloads AUTHORITATIVELY from D1,
/// so a tombstone written before eviction is still reported `true` (no
/// false-negative introduced by eviction; invariant 1 preserved).
#[tokio::test]
async fn evicted_tenant_reload_still_sees_its_tombstone() {
    const CAP: usize = 2;
    let inner = Arc::new(CountingInner::new());
    // A tombstone for TENANT lives durably in D1 (any writer).
    inner.seed_inner(TENANT, DIGEST);
    let store = BloomTombstoneStore::with_params_capped(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
        CAP,
    );
    // Prime TENANT (loads its bloom, seeded from D1 → reports true).
    assert!(store.is_tombstoned(TENANT, DIGEST).await.expect("q0"));
    // Churn other tenants past the cap to force TENANT's bloom out.
    for i in 0..8u32 {
        let t = format!("other-{i:04}");
        let _ = store.is_tombstoned(&t, DIGEST).await.expect("q");
    }
    assert_eq!(store.tenant_bloom_count(), CAP as u64, "still bounded");
    // TENANT's bloom was evicted; the next lookup reloads from D1 and MUST
    // still report the tombstone (authoritative — no eviction false-negative).
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q1"),
        "evicted tenant's durable tombstone must survive reload"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_1_2_REANCHOR: () = ();
