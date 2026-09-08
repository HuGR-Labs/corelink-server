/// A long staleness window so a freshly-loaded tenant bloom stays "fresh"
/// for the whole test (the just-reloaded fall-through fires only on the
/// FIRST touch of a tenant).
const LONG_WINDOW: Duration = Duration::from_secs(3600);

/// Warm a tenant's bloom past the one-shot just-reloaded epoch so the fast
/// path is armed: touch any digest once (that first touch falls through),
/// after which fresh `definitely-absent` lookups skip the inner store.
async fn warm(store: &BloomTombstoneStore, tenant: &str) {
    let _ = store.is_tombstoned(tenant, "warm-up-digest").await;
}

/// (a) A non-tombstoned digest → `Ok(false)` with ZERO inner calls on the
/// fast path (after the tenant bloom is warmed past its first touch).
#[tokio::test]
async fn bloom_fast_path_skips_inner_for_absent_digest() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await; // first touch falls through (1 inner read)
    let before = inner.reads();
    let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
    assert!(!r, "absent digest ⇒ Ok(false)");
    assert_eq!(
        inner.reads(),
        before,
        "fast path must NOT touch the inner store"
    );
}

/// (b) A digest tombstoned THROUGH the wrapper → `Ok(true)` and stays true.
#[tokio::test]
async fn bloom_through_write_is_true_and_stays_true() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await;
    assert!(store.upsert(TENANT, DIGEST, "dsr", 1).await.is_ok());
    // Bloom hit (we wrote it) ⇒ falls through to inner, which is authoritative.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "tombstoned ⇒ true"
    );
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "stays true"
    );
}

/// (c) NO FALSE NEGATIVE: a digest tombstoned DIRECTLY in the inner store
/// (another instance) is correctly reported `true` AFTER a refresh — and
/// the bounded-window behaviour is asserted: with a ZERO window EVERY
/// lookup is stale ⇒ always falls through ⇒ always authoritative.
#[tokio::test]
async fn bloom_no_false_negative_cross_instance_after_refresh() {
    let inner = Arc::new(CountingInner::new());
    // Zero window ⇒ every tenant_bloom call re-stamps ⇒ just_reloaded=true
    // ⇒ every lookup falls through to the authoritative inner store. This
    // is the worst case (window→0 = the wrapper is a pass-through, never a
    // false negative) and the boundary the bound is measured against.
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        Duration::ZERO,
    );
    // Another instance writes the tombstone directly to D1 (not via bloom).
    inner.seed_inner(TENANT, DIGEST);
    // The wrapper must NEVER report this absent.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "cross-instance tombstone must read true after refresh"
    );

    // And the WITHIN-window staleness bound: with a long window, the FIRST
    // touch of a tenant falls through (catches the cross-instance write);
    // subsequent fast-path lookups of OTHER absent digests skip D1, but a
    // cross-instance write that lands AFTER the bloom was loaded is only
    // guaranteed visible after the window elapses (next reload). Assert the
    // first-touch catch:
    let inner2 = Arc::new(CountingInner::new());
    let store2 = BloomTombstoneStore::with_params(
        inner2.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    inner2.seed_inner(TENANT, DIGEST); // present before first touch
    assert!(
        store2.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "first-touch reload catches the cross-instance tombstone"
    );
}

/// (c2) F-010 REGRESSION — WITHIN-WINDOW false-negative is closed by the D1
/// reload-seed. A digest tombstoned by a SEPARATE writer (the erase route's
/// own `D1TombstoneStore`, which does NOT share this bloom) is present in D1
/// before the bloom's first (re)load. With a LONG window the bloom is
/// (re)loaded exactly once on first touch; the OLD code seeded the fresh bloom
/// from in-process writes ONLY, so the FIRST read fell through (410) but
/// STAMPED an EMPTY bloom — every subsequent read within the ~30s window then
/// hit the empty-bloom fast path and returned `Ok(false)` WITHOUT consulting
/// D1 (the 410→404/200 GDPR downgrade). The fix seeds the reload from the
/// authoritative D1 set, so the SECOND (and every) within-window read still
/// sees the digest in the bloom → falls through → stays `true`. This asserts
/// the in-code invariant #1 ("NO FALSE NEGATIVE … within at most one window").
#[tokio::test]
async fn bloom_no_within_window_false_negative_for_cross_writer_tombstone() {
    let inner = Arc::new(CountingInner::new());
    // LONG window: the bloom is (re)loaded ONCE, so the only thing standing
    // between a second read and a false negative is the reload SEED (F-010).
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    // A SEPARATE writer (not via this bloom) tombstones the digest in D1.
    inner.seed_inner(TENANT, DIGEST);
    // First read: triggers the one-shot (re)load → seeds the bloom from D1 →
    // falls through → true.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q1"),
        "first within-window read of a cross-writer tombstone must be true"
    );
    // SECOND read, still WITHIN the (long) window — the regression point. The
    // bloom is NOT re-stamped (window not elapsed), so the fast path runs; it
    // MUST still see the seeded bit and fall through to D1 → true. Pre-fix
    // this returned Ok(false) (false negative / GDPR 410 bypass).
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q2"),
        "F-010: second within-window read MUST NOT be a false negative"
    );
    // And a THIRD, to be thorough.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q3"),
        "F-010: stays true for every within-window read"
    );
    // A genuinely-absent OTHER digest still fast-paths to false (the seed did
    // not over-set membership for unrelated digests).
    assert!(
        !store.is_tombstoned(TENANT, "00absent00").await.expect("q4"),
        "an un-tombstoned digest still reads absent"
    );
}

/// (d) False-positive path: a bloom HIT on a digest that is NOT actually
/// tombstoned falls through to the inner store and returns its authoritative
/// `Ok(false)` — never a wrongful 410. We force a "hit" by writing the
/// digest through the wrapper (sets the bits) but NOT into the inner set
/// (simulating a bloom bit set with no real tombstone, e.g. an upsert whose
/// inner write later rolled back). Here we use the erroring-free inner and
/// assert the authoritative answer wins.
#[tokio::test]
async fn bloom_false_positive_falls_through_to_authoritative_inner() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await;
    // Set the bloom bit directly (a "false positive": bit set, no inner row).
    let tb = store
        .fresh_tenant_bloom(TENANT)
        .expect("warmed bloom is fresh");
    tb.bloom
        .insert(&BloomTombstoneStore::bloom_key(TENANT, DIGEST));
    let before = inner.reads();
    let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
    assert!(
        !r,
        "bloom false-positive must defer to the authoritative inner Ok(false)"
    );
    assert_eq!(
        inner.reads(),
        before + 1,
        "maybe-present must consult the inner store exactly once"
    );
}

/// (e) Inner error propagates UNCHANGED on the maybe-present path
/// (preserves today's fail-OPEN semantics — invariant 3).
#[tokio::test]
async fn bloom_inner_error_propagates_on_maybe_path() {
    let inner: Arc<dyn TombstoneStore> = Arc::new(ErroringInner);
    let store = BloomTombstoneStore::with_params(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await; // first touch also errors, fine
                                // Write through to set a bloom bit → guarantees a maybe-present probe.
    let _ = store.upsert(TENANT, DIGEST, "r", 1).await;
    let err = store.is_tombstoned(TENANT, DIGEST).await;
    assert_eq!(
        err,
        Err("d1 fault".to_owned()),
        "inner Err must propagate unchanged"
    );
}

/// (f) Concurrency: many tasks read + write concurrently; no panic, no torn
/// state, and every through-the-wrapper write is observed true afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bloom_concurrent_reads_and_writes() {
    let inner = Arc::new(CountingInner::new());
    let store = Arc::new(BloomTombstoneStore::with_params(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    ));
    let mut handles = Vec::new();
    for i in 0..64u32 {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            let d = format!("digest-{i:04}");
            // half write, all read.
            if i % 2 == 0 {
                let _ = s.upsert(TENANT, &d, "r", i64::from(i)).await;
            }
            let _ = s.is_tombstoned(TENANT, &d).await;
        }));
    }
    for h in handles {
        h.await.expect("task");
    }
    // Every even digest was written THROUGH the wrapper ⇒ must read true.
    for i in (0..64u32).step_by(2) {
        let d = format!("digest-{i:04}");
        assert!(
            store.is_tombstoned(TENANT, &d).await.expect("q"),
            "written digest {d} must be tombstoned"
        );
    }
}

/// Bloom unit: one-sided error guarantee — once inserted, `contains` is true.
#[test]
fn bloom_unit_no_false_negative_and_bounded() {
    let b = Bloom::new(DEFAULT_BLOOM_BITS, DEFAULT_BLOOM_HASHES);
    for i in 0..1000 {
        b.insert(&format!("k{i}"));
    }
    for i in 0..1000 {
        assert!(
            b.contains(&format!("k{i}")),
            "inserted key must always be present"
        );
    }
    // Bounded: the bit-array size is fixed regardless of element count.
    assert_eq!(b.words.len(), DEFAULT_BLOOM_BITS / 64);
}

// ──────────────────────────────────────────────────────────────────────
// F-004 — TombstoneGatedCasHandler (the centralized shared-seam erase gate)
// ──────────────────────────────────────────────────────────────────────

use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasReadHandler,
    CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest, CasWriteResponse,
};

/// Minimal CAS handler fake: read/exists always succeed, write always
/// commits, delete always succeeds — so any 404/410/503 in the tests below
/// can ONLY come from the tombstone gate, not from the underlying handler.
#[derive(Debug, Default)]
struct AlwaysOkCas {
    wrote: Mutex<Vec<(String, String)>>,
    deleted: Mutex<Vec<(String, String)>>,
}
impl CasReadHandler for AlwaysOkCas {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        Ok(CasReadResponse::new(b"live-bytes".to_vec(), req.hash))
    }
}
impl CasWriteHandler for AlwaysOkCas {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        lock_or_recover(&self.wrote).push((req.tenant.clone(), req.claimed_hash.clone()));
        Ok(CasWriteResponse::new(req.claimed_hash, true))
    }
}
impl CasDeleteHandler for AlwaysOkCas {
    fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        lock_or_recover(&self.deleted).push((req.tenant.clone(), req.hash.clone()));
        Ok(CasDeleteResponse::new(true))
    }
}

fn gated_with(tombstones: Arc<dyn TombstoneStore>) -> (TombstoneGatedCasHandler, Arc<AlwaysOkCas>) {
    let backing = Arc::new(AlwaysOkCas::default());
    let gated = TombstoneGatedCasHandler::new(
        backing.clone() as Arc<dyn CasReadHandler>,
        backing.clone() as Arc<dyn CasWriteHandler>,
        backing.clone() as Arc<dyn CasDeleteHandler>,
        tombstones,
    );
    (gated, backing)
}

fn read_req() -> CasReadRequest {
    CasReadRequest::new(
        TENANT.to_owned(),
        DIGEST.to_owned(),
        format!("anon@{TENANT}"),
        TENANT.to_owned(),
        0,
    )
}
fn write_req() -> CasWriteRequest {
    CasWriteRequest::new(
        TENANT.to_owned(),
        DIGEST.to_owned(),
        b"resurrect".to_vec(),
        format!("anon@{TENANT}"),
        TENANT.to_owned(),
        0,
    )
}

/// A tombstoned read is refused at the SHARED seam (NotFound) — so NO
/// surface (cargo/brew/npm/pip/bazel/turbo/oci) can serve erased bytes,
/// even though only the native route has the inline 410 gate. (F-004 read.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_read_of_tombstoned_is_notfound() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
    let (gated, _backing) = gated_with(ts);
    let r = gated.read(read_req());
    assert!(
        matches!(r, Err(CasHandlerError::NotFound { .. })),
        "tombstoned read must be refused at the shared seam, got {r:?}"
    );
    // exists likewise reports absent.
    assert!(!gated.exists(read_req()).expect("exists"));
}

/// A non-tombstoned read delegates to the backing handler (live bytes).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_read_of_live_blob_delegates() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    let (gated, _backing) = gated_with(ts);
    let resp = gated.read(read_req()).expect("live read");
    assert_eq!(resp.bytes, b"live-bytes".to_vec());
    assert!(gated.exists(read_req()).expect("exists"));
}

/// A re-PUT of a tombstoned (tenant, hash) is REFUSED at the shared seam with
/// the GONE sentinel — so erased bytes cannot be resurrected at the same
/// content address via ANY write surface (the prior write path was ungated).
/// (F-004 write — the resurrection vector.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_of_tombstoned_is_refused_gone() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
    let (gated, backing) = gated_with(ts);
    let r = gated.write(write_req());
    match r {
        Err(CasHandlerError::Internal(msg)) => {
            assert!(
                msg.starts_with(TOMBSTONE_GONE_SENTINEL),
                "re-PUT must carry the GONE sentinel, got {msg}"
            );
        }
        other => panic!("re-PUT of erased blob must be refused, got {other:?}"),
    }
    // The backing handler was NEVER reached — no resurrection.
    assert!(
        lock_or_recover(&backing.wrote).is_empty(),
        "no bytes were written"
    );
}

/// A write to a LIVE (non-tombstoned) hash delegates and commits normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_of_live_hash_delegates() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    let (gated, backing) = gated_with(ts);
    let resp = gated.write(write_req()).expect("live write");
    assert!(resp.durable);
    assert_eq!(lock_or_recover(&backing.wrote).len(), 1);
}

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();
