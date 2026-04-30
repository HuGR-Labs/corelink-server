//! WI-S01-003 §13 integration tests: end-to-end PUT→GET cycles, the
//! `BlobStoreWrite` type-driven seam, error-taxonomy mapping, and the
//! per-region binding contract.
//!
//! Coverage map:
//! - WI §6.1.1 `R2Writer::put` happy path → [`put_then_get_round_trip`].
//! - WI §6.1.1 + §6.1.3 `BlobStoreWrite` seam → [`scoped_writer_implements_blob_store_write`].
//! - WI §6.1.1 `If-None-Match: *` idempotent → [`idempotent_duplicate_via_blob_store_write`].
//! - WI §6.1.1 size cap → [`oversize_blob_rejected_with_taxonomy_code`].
//! - WI §6.1.6 error mapping → [`backend_5xx_maps_to_service_degraded`].
//! - WI §6.1.4 per-region binding → [`per_region_binding_isolates_buckets`].
//! - WI §15 chaos #4 concurrent duplicate writes → [`concurrent_duplicate_writes_idempotent`].

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_docs_in_private_items,
    clippy::print_stderr,
    clippy::print_stdout,
    missing_docs,
    reason = "test code; panic on assertion mismatch is the contract"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::{BlobStoreWrite, Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::error::R2Error;
use corelink_worker::storage::r2::{
    AlwaysFailingR2, InMemoryR2, PutOutcome, R2Reader, R2Writer, SINGLE_BLOB_LIMIT_BYTES,
};
use corelink_worker::{Region, TenantCtx};
use uuid::Uuid;
use zeroize::Zeroizing;

const FIXTURE_TDK_BYTES: [u8; 32] = *b"fixture-tdk-32-bytes-constant!ok";

fn fixture_ctx(region: Region, suffix: u128) -> TenantCtx {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES));
    let tid = Uuid::from_u128(suffix);
    TenantCtx::new(&tdk, tid, region)
}

#[tokio::test]
async fn put_then_get_round_trip() {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader = R2Reader::new(Region::Wnam, backend);

    let ctx = fixture_ctx(Region::Wnam, 1);
    let body = Bytes::from_static(b"hello world");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    let outcome = writer.put(&ctx, &vb).await.expect("put ok");
    assert_eq!(outcome, PutOutcome::Fresh);

    let got = reader.get(&ctx, &claimed).await.expect("get ok");
    assert_eq!(got, body);
}

#[tokio::test]
async fn scoped_writer_implements_blob_store_write() {
    // The BlobStoreWrite seam (corelink-hash WI-S01-002) is the type-driven
    // contract: any code path that wants to write to R2 *must* go through
    // VerifiedBody::new. This test exercises the impl through the trait.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Sam, Arc::clone(&backend));

    let ctx = fixture_ctx(Region::Sam, 7);
    let body = Bytes::from_static(b"trait-driven body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    fn assert_blob_store<S: BlobStoreWrite>(_s: &S) {}
    let scoped = writer.for_tenant(&ctx);
    assert_blob_store(&scoped);

    scoped.put_verified(&vb).await.expect("trait put ok");
    assert_eq!(backend.len(), 1);
}

#[tokio::test]
async fn idempotent_duplicate_via_blob_store_write() {
    // Through the BlobStoreWrite trait, Duplicate maps to Ok(()) — this is
    // INV-CAS-IDEMPOTENCY enforced at the trait surface. Callers that need
    // the Fresh/Duplicate distinction call `writer.put` directly.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Weur, Arc::clone(&backend));
    let ctx = fixture_ctx(Region::Weur, 9);

    let body = Bytes::from_static(b"idempotent body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let scoped = writer.for_tenant(&ctx);

    scoped.put_verified(&vb).await.expect("put 1 ok");
    scoped
        .put_verified(&vb)
        .await
        .expect("put 2 ok (Duplicate→Ok)");
    scoped
        .put_verified(&vb)
        .await
        .expect("put 3 ok (Duplicate→Ok)");
    assert_eq!(backend.len(), 1, "only one physical object stored");
}

#[tokio::test]
async fn oversize_blob_rejected_with_taxonomy_code() {
    // Blobs > 5 MiB belong on the multipart path (WI-S05-003). The single-
    // blob writer must reject them rather than silently passing through.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, backend);
    let ctx = fixture_ctx(Region::Wnam, 11);

    let oversize = Bytes::from(vec![0u8; SINGLE_BLOB_LIMIT_BYTES + 1]);
    let claimed = Digest::compute(&oversize);
    let vb = VerifiedBody::new(oversize, claimed).expect("verifies");

    let err = writer.put(&ctx, &vb).await.expect_err("must reject");
    match &err {
        R2Error::BlobTooLarge { size, limit } => {
            assert_eq!(*size, SINGLE_BLOB_LIMIT_BYTES + 1);
            assert_eq!(*limit, SINGLE_BLOB_LIMIT_BYTES);
        }
        other => panic!("expected BlobTooLarge, got {other:?}"),
    }
    assert_eq!(err.taxonomy_code(), "COR_CAS_BLOB_TOO_LARGE");
}

#[tokio::test]
async fn boundary_blob_at_exactly_5_mib_accepted() {
    // 5 MiB exactly is permitted; only > 5 MiB rejects.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let ctx = fixture_ctx(Region::Wnam, 13);

    let exactly_limit = Bytes::from(vec![0u8; SINGLE_BLOB_LIMIT_BYTES]);
    let claimed = Digest::compute(&exactly_limit);
    let vb = VerifiedBody::new(exactly_limit, claimed).expect("verifies");

    writer
        .put(&ctx, &vb)
        .await
        .expect("5 MiB exactly must accept");
    assert_eq!(backend.len(), 1);
}

#[tokio::test]
async fn backend_5xx_maps_to_service_degraded() {
    // A backend that always fails surfaces R2Error::Backend to the caller,
    // which carries the COR_SERVICE_DEGRADED taxonomy code (HTTP 503).
    let backend = Arc::new(AlwaysFailingR2::new("synthetic 5xx"));
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));
    let ctx = fixture_ctx(Region::Wnam, 17);

    let body = Bytes::from_static(b"will fail");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");

    let err = writer.put(&ctx, &vb).await.expect_err("must fail");
    match &err {
        R2Error::Backend(msg) => assert!(msg.contains("synthetic 5xx")),
        other => panic!("expected Backend, got {other:?}"),
    }
    assert_eq!(err.taxonomy_code(), "COR_SERVICE_DEGRADED");

    // Same fault path on the reader (`get`).
    let read_err = reader.get(&ctx, &claimed).await.expect_err("must fail");
    match &read_err {
        R2Error::Backend(msg) => assert!(msg.contains("synthetic 5xx")),
        other => panic!("expected Backend on get, got {other:?}"),
    }
    assert_eq!(read_err.taxonomy_code(), "COR_SERVICE_DEGRADED");
}

#[tokio::test]
async fn always_failing_backend_head_propagates_fault() {
    use corelink_worker::storage::r2::R2Backend;

    let backend = AlwaysFailingR2::new("synthetic head fault");
    let head_err = backend.head("anything").await.expect_err("must fail");
    match &head_err {
        R2Error::Backend(msg) => assert!(msg.contains("synthetic head fault")),
        other => panic!("expected Backend, got {other:?}"),
    }
}

#[tokio::test]
async fn read_miss_returns_not_found_with_taxonomy_code() {
    let backend = Arc::new(InMemoryR2::new());
    let reader = R2Reader::new(Region::Wnam, backend);
    let ctx = fixture_ctx(Region::Wnam, 19);
    let claimed = Digest::compute(b"never-stored");

    let err = reader.get(&ctx, &claimed).await.expect_err("must miss");
    assert!(matches!(err, R2Error::NotFound));
    assert_eq!(err.taxonomy_code(), "COR_CAS_BLOB_NOT_FOUND");
}

#[tokio::test]
async fn per_region_binding_isolates_buckets() {
    // Tenant A in WNAM and WEUR — same tenant, two writers, two regions.
    // Keys must carry the right `cas-<region>` prefix per writer; the
    // backend (acting as the shared R2 admin namespace) must hold both.
    let shared_backend = Arc::new(InMemoryR2::new());
    let writer_wnam = R2Writer::new(Region::Wnam, Arc::clone(&shared_backend));
    let writer_weur = R2Writer::new(Region::Weur, Arc::clone(&shared_backend));

    let body = Bytes::from_static(b"per-region body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx_wnam = fixture_ctx(Region::Wnam, 1);
    let ctx_weur = fixture_ctx(Region::Weur, 1); // Same tenant!

    writer_wnam.put(&ctx_wnam, &vb).await.expect("put wnam ok");
    writer_weur.put(&ctx_weur, &vb).await.expect("put weur ok");

    let keys = shared_backend.keys_snapshot();
    assert_eq!(keys.len(), 2, "two distinct keys expected");
    assert!(keys.iter().any(|k| k.starts_with("cas-wnam/")));
    assert!(keys.iter().any(|k| k.starts_with("cas-weur/")));
}

#[tokio::test]
async fn concurrent_same_tenant_duplicate_writes_idempotent() {
    // 100 concurrent PUTs of the same digest under the **same** tenant must
    // result in exactly one physical write; outcomes are a mix of Fresh
    // (one) and Duplicate (the rest), no Backend errors. Same-tenant
    // contention is the immutability layer of chaos #4.
    let backend = Arc::new(InMemoryR2::new());
    let writer = Arc::new(R2Writer::new(Region::Wnam, Arc::clone(&backend)));

    let body = Bytes::from_static(b"hot contention");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx = fixture_ctx(Region::Wnam, 23);

    let mut joins = Vec::with_capacity(100);
    for _ in 0..100 {
        let writer_clone = Arc::clone(&writer);
        let vb_clone = vb.clone();
        joins.push(tokio::spawn(async move {
            writer_clone.put(&ctx, &vb_clone).await
        }));
    }

    let mut fresh = 0;
    let mut dup = 0;
    for join in joins {
        match join.await.expect("task ok").expect("put ok") {
            PutOutcome::Fresh => fresh += 1,
            PutOutcome::Duplicate => dup += 1,
        }
    }
    assert_eq!(fresh + dup, 100);
    assert_eq!(fresh, 1, "exactly one Fresh outcome");
    assert_eq!(dup, 99, "remaining 99 must be Duplicate");
    assert_eq!(backend.len(), 1, "one physical object");
}

#[tokio::test]
async fn chaos_4_concurrent_writes_across_distinct_tenants() {
    // WI-S01-003 §15 chaos #4 (per the WI text): "100 concurrent PUT same
    // digest different tenants → expect 1 success per tenant (paths
    // distinct)". Each tenant must land its own physical object — paths
    // are disjoint by HMAC isolation, so no contention between tenants.
    let backend = Arc::new(InMemoryR2::new());
    let writer = Arc::new(R2Writer::new(Region::Wnam, Arc::clone(&backend)));

    let body = Bytes::from_static(b"shared digest body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");

    // 100 distinct tenants under the same TDK + region.
    let mut joins = Vec::with_capacity(100);
    for tid_seed in 1_000u128..1_100u128 {
        let writer_clone = Arc::clone(&writer);
        let vb_clone = vb.clone();
        let ctx = fixture_ctx(Region::Wnam, tid_seed);
        joins.push(tokio::spawn(async move {
            writer_clone.put(&ctx, &vb_clone).await
        }));
    }

    let mut fresh = 0;
    let mut dup = 0;
    for join in joins {
        match join.await.expect("task ok").expect("put ok") {
            PutOutcome::Fresh => fresh += 1,
            PutOutcome::Duplicate => dup += 1,
        }
    }
    assert_eq!(fresh, 100, "every tenant must land its own object");
    assert_eq!(dup, 0, "paths are disjoint across tenants — no Duplicate");
    assert_eq!(
        backend.len(),
        100,
        "one physical object per tenant (cross-tenant disjointness)"
    );

    // Each key must carry a distinct HMAC16 prefix.
    let keys = backend.keys_snapshot();
    let prefixes: std::collections::HashSet<&str> =
        keys.iter().filter_map(|k| k.split('/').nth(1)).collect();
    assert_eq!(prefixes.len(), 100, "100 distinct HMAC16 prefixes");
}

#[tokio::test]
async fn cross_region_keys_distinct_for_same_tenant_and_digest() {
    // Even though region-pinning happens at the writer level (residency),
    // the *key* itself encodes the region. This makes a misrouted blob
    // detectable in offline forensics: a `cas-weur/...` key found in the
    // `cas-wnam` bucket is itself the smoking gun.
    let backend = Arc::new(InMemoryR2::new());
    let body = Bytes::from_static(b"region forensics");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");

    for region in [Region::Wnam, Region::Weur, Region::Sam] {
        let writer = R2Writer::new(region, Arc::clone(&backend));
        let ctx = fixture_ctx(region, 29);
        writer.put(&ctx, &vb).await.expect("put ok");
    }

    let keys = backend.keys_snapshot();
    assert_eq!(keys.len(), 3, "three distinct keys (one per region)");
}

#[tokio::test]
async fn region_mismatch_writer_rejects_with_internal() {
    // Dispatcher hands a WEUR ctx to a WNAM-pinned writer — adapter
    // refuses rather than silently writing to the wrong bucket. This is
    // the residency invariant (INV-DATA-RESIDENCY) at the storage seam.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, backend);
    let ctx_weur = fixture_ctx(Region::Weur, 41);
    let body = Bytes::from_static(b"misrouted");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");

    let err = writer.put(&ctx_weur, &vb).await.expect_err("must reject");
    match &err {
        R2Error::RegionMismatch { adapter: a, ctx: c } => {
            assert_eq!(*a, Region::Wnam);
            assert_eq!(*c, Region::Weur);
        }
        other => panic!("expected RegionMismatch, got {other:?}"),
    }
    assert_eq!(err.taxonomy_code(), "COR_INTERNAL");
}

#[tokio::test]
async fn region_mismatch_reader_rejects_with_internal() {
    let backend = Arc::new(InMemoryR2::new());
    let reader = R2Reader::new(Region::Sam, backend);
    let ctx_wnam = fixture_ctx(Region::Wnam, 43);
    let claimed = Digest::compute(b"anything");

    let err = reader
        .get(&ctx_wnam, &claimed)
        .await
        .expect_err("must reject");
    assert!(matches!(err, R2Error::RegionMismatch { .. }));
    assert_eq!(err.taxonomy_code(), "COR_INTERNAL");
}

#[tokio::test]
async fn metrics_observer_records_fresh_duplicate_and_get_outcomes() {
    use corelink_worker::storage::metrics::{
        GetResultLabel, InMemoryMetrics, MetricsObserver, PutResultLabel,
    };

    let metrics = Arc::new(InMemoryMetrics::new());
    let observer: Arc<dyn MetricsObserver> = Arc::clone(&metrics) as _;
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::with_metrics(Region::Wnam, Arc::clone(&backend), Arc::clone(&observer));
    let reader = R2Reader::with_metrics(Region::Wnam, Arc::clone(&backend), Arc::clone(&observer));
    let ctx = fixture_ctx(Region::Wnam, 47);

    let body = Bytes::from_static(b"metrics body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    writer.put(&ctx, &vb).await.expect("put 1 ok");
    writer.put(&ctx, &vb).await.expect("put 2 ok (dup)");
    let _ = reader.get(&ctx, &claimed).await.expect("get ok");
    let _ = reader
        .get(&ctx, &Digest::compute(b"never"))
        .await
        .expect_err("get miss");

    assert_eq!(metrics.put_count(Region::Wnam, PutResultLabel::Ok), 1);
    assert_eq!(
        metrics.put_count(Region::Wnam, PutResultLabel::ConflictDuplicate),
        1
    );
    assert_eq!(metrics.put_count(Region::Wnam, PutResultLabel::Error), 0);
    assert_eq!(metrics.get_count(Region::Wnam, GetResultLabel::Ok), 1);
    assert_eq!(metrics.get_count(Region::Wnam, GetResultLabel::NotFound), 1);
    assert_eq!(metrics.get_count(Region::Wnam, GetResultLabel::Error), 0);
    // Duration histograms recorded too — small body lands in the Le1KiB bucket.
    use corelink_worker::storage::metrics::PutSizeBucket;
    assert_eq!(
        metrics.put_duration_observations(Region::Wnam, PutSizeBucket::Le1KiB),
        2,
        "Le1KiB bucket recorded for both PUT calls (Fresh + Duplicate)"
    );
    assert_eq!(
        metrics.get_duration_observations(Region::Wnam),
        2,
        "GET duration recorded on both Ok and NotFound"
    );
}

#[tokio::test]
async fn put_result_get_result_labels_have_canonical_strings() {
    // Lock the metric label string contract — these strings are part of
    // the dashboard panel queries (S-09) so any drift is a P0 ingest break.
    use corelink_worker::storage::metrics::{GetResultLabel, PutResultLabel};

    assert_eq!(PutResultLabel::Ok.as_str(), "ok");
    assert_eq!(
        PutResultLabel::ConflictDuplicate.as_str(),
        "conflict_duplicate"
    );
    assert_eq!(PutResultLabel::Error.as_str(), "error");

    assert_eq!(GetResultLabel::Ok.as_str(), "ok");
    assert_eq!(GetResultLabel::NotFound.as_str(), "not_found");
    assert_eq!(GetResultLabel::Error.as_str(), "error");
}

#[tokio::test]
async fn put_size_bucket_boundaries_are_inclusive() {
    // Lock down the canonical boundary points; codex round-3 P2 caught a
    // pre-fix off-by-one (size==1024 falling into Le16KiB, oversize folded
    // into Le5MiB). The current contract is inclusive Le<N> + Oversize for
    // anything past 5 MiB.
    use corelink_worker::storage::metrics::PutSizeBucket;

    assert_eq!(PutSizeBucket::from_bytes(0), PutSizeBucket::Le1KiB);
    assert_eq!(PutSizeBucket::from_bytes(1023), PutSizeBucket::Le1KiB);
    assert_eq!(PutSizeBucket::from_bytes(1024), PutSizeBucket::Le1KiB);
    assert_eq!(PutSizeBucket::from_bytes(1025), PutSizeBucket::Le16KiB);

    assert_eq!(PutSizeBucket::from_bytes(16 * 1024), PutSizeBucket::Le16KiB);
    assert_eq!(
        PutSizeBucket::from_bytes(16 * 1024 + 1),
        PutSizeBucket::Le256KiB
    );

    assert_eq!(
        PutSizeBucket::from_bytes(256 * 1024),
        PutSizeBucket::Le256KiB
    );
    assert_eq!(
        PutSizeBucket::from_bytes(256 * 1024 + 1),
        PutSizeBucket::Le1MiB
    );

    assert_eq!(
        PutSizeBucket::from_bytes(1024 * 1024),
        PutSizeBucket::Le1MiB
    );
    assert_eq!(
        PutSizeBucket::from_bytes(1024 * 1024 + 1),
        PutSizeBucket::Le5MiB
    );

    assert_eq!(
        PutSizeBucket::from_bytes(5 * 1024 * 1024),
        PutSizeBucket::Le5MiB
    );
    assert_eq!(
        PutSizeBucket::from_bytes(5 * 1024 * 1024 + 1),
        PutSizeBucket::Oversize
    );
    assert_eq!(
        PutSizeBucket::from_bytes(usize::MAX),
        PutSizeBucket::Oversize
    );

    // Stable label round-trip.
    assert_eq!(PutSizeBucket::Le1KiB.as_str(), "le1kib");
    assert_eq!(PutSizeBucket::Le16KiB.as_str(), "le16kib");
    assert_eq!(PutSizeBucket::Le256KiB.as_str(), "le256kib");
    assert_eq!(PutSizeBucket::Le1MiB.as_str(), "le1mib");
    assert_eq!(PutSizeBucket::Le5MiB.as_str(), "le5mib");
    assert_eq!(PutSizeBucket::Oversize.as_str(), "oversize");
}

#[tokio::test]
async fn put_size_bucket_observed_per_payload_size() {
    // Each canonical bucket increments its own histogram cell when an
    // actual PUT lands a body of the corresponding size (drives the live
    // wiring path between R2Writer::put and MetricsObserver::record_put_duration).
    use corelink_worker::storage::metrics::{InMemoryMetrics, MetricsObserver, PutSizeBucket};

    let metrics = Arc::new(InMemoryMetrics::new());
    let observer: Arc<dyn MetricsObserver> = Arc::clone(&metrics) as _;
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::with_metrics(Region::Wnam, backend, observer);

    // One body in each canonical bucket. Each body has a distinct digest
    // so the writes don't collide on If-None-Match.
    let bodies: &[(usize, PutSizeBucket)] = &[
        (1024, PutSizeBucket::Le1KiB),
        (16 * 1024, PutSizeBucket::Le16KiB),
        (256 * 1024, PutSizeBucket::Le256KiB),
        (1024 * 1024, PutSizeBucket::Le1MiB),
        (5 * 1024 * 1024, PutSizeBucket::Le5MiB),
    ];

    for (i, (size, _bucket)) in bodies.iter().enumerate() {
        let mut body = vec![0u8; *size];
        // Distinct digests per body via tail byte.
        if let Some(last) = body.last_mut() {
            *last = u8::try_from(i).unwrap_or(0);
        }
        let body = Bytes::from(body);
        let claimed = Digest::compute(&body);
        let vb = VerifiedBody::new(body, claimed).expect("verifies");
        let ctx = fixture_ctx(Region::Wnam, 80 + (i as u128));
        writer.put(&ctx, &vb).await.expect("put ok");
    }

    for (_size, bucket) in bodies {
        assert_eq!(
            metrics.put_duration_observations(Region::Wnam, *bucket),
            1,
            "bucket {bucket:?} must have exactly 1 observation"
        );
    }
}

#[tokio::test]
async fn metrics_observer_records_error_labels_on_oversize_and_backend_fault() {
    use corelink_worker::storage::metrics::{InMemoryMetrics, MetricsObserver, PutResultLabel};

    let metrics = Arc::new(InMemoryMetrics::new());
    let observer: Arc<dyn MetricsObserver> = Arc::clone(&metrics) as _;

    // Backend fault path → Error counter increments.
    let failing = Arc::new(AlwaysFailingR2::new("burn"));
    let writer = R2Writer::with_metrics(Region::Wnam, failing, Arc::clone(&observer));
    let ctx = fixture_ctx(Region::Wnam, 67);
    let body = Bytes::from_static(b"will fail");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    writer.put(&ctx, &vb).await.expect_err("must fail");

    // Oversize path → Error counter increments.
    let backend = Arc::new(InMemoryR2::new());
    let writer2 = R2Writer::with_metrics(Region::Wnam, backend, Arc::clone(&observer));
    let big = Bytes::from(vec![0u8; SINGLE_BLOB_LIMIT_BYTES + 1]);
    let claimed2 = Digest::compute(&big);
    let vb2 = VerifiedBody::new(big, claimed2).expect("verifies");
    writer2
        .put(&ctx, &vb2)
        .await
        .expect_err("must reject oversize");

    // Region mismatch path → Error counter increments.
    let backend3 = Arc::new(InMemoryR2::new());
    let writer3 = R2Writer::with_metrics(Region::Wnam, backend3, Arc::clone(&observer));
    let ctx_wrong = fixture_ctx(Region::Sam, 71);
    let body3 = Bytes::from_static(b"misrouted");
    let claimed3 = Digest::compute(&body3);
    let vb3 = VerifiedBody::new(body3, claimed3).expect("verifies");
    writer3
        .put(&ctx_wrong, &vb3)
        .await
        .expect_err("must reject region mismatch");

    // 3 distinct error paths must increment the WNAM Error counter exactly 3
    // times.
    assert_eq!(metrics.put_count(Region::Wnam, PutResultLabel::Error), 3);
}

#[tokio::test]
async fn blob_store_constructor_rejects_region_mismatch() {
    use corelink_worker::storage::blob_store::R2BlobStore;

    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader_wrong = R2Reader::new(Region::Weur, backend);
    let err =
        R2BlobStore::new(writer, reader_wrong).expect_err("writer wnam + reader weur must reject");
    assert_eq!(err.writer, Region::Wnam);
    assert_eq!(err.reader, Region::Weur);
}

#[tokio::test]
async fn blob_store_trait_round_trip() {
    // Exercises the unified BlobStore (read + write) trait abstraction
    // promised by WI-S01-003 §6.1.3 — a single seam that the REAPI handler
    // / read path / GC sweeper depend on; concrete impl is `R2BlobStore`.
    use corelink_worker::storage::blob_store::R2BlobStore;
    use corelink_worker::storage::BlobStore;

    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader = R2Reader::new(Region::Wnam, backend);
    let store = R2BlobStore::new(writer, reader).expect("matching regions");

    fn assert_blob_store<S: BlobStore>(_s: &S) {}
    assert_blob_store(&store);

    let ctx = fixture_ctx(Region::Wnam, 53);
    let body = Bytes::from_static(b"BlobStore body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    store.put_verified(&ctx, &vb).await.expect("put ok");
    let got = store.get(&ctx, &claimed).await.expect("get ok");
    assert_eq!(got, body);
}

// Compile-time check: ScopedR2Writer's put_verified future is Send.
// Static assertion via fn signature.
#[tokio::test]
async fn scoped_writer_future_is_send() {
    fn assert_send<T: Send>(_t: T) {}
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, backend);
    let ctx = fixture_ctx(Region::Wnam, 31);
    let body = Bytes::from_static(b"send check");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let scoped = writer.for_tenant(&ctx);
    let fut = scoped.put_verified(&vb);
    assert_send(fut);
}

#[tokio::test]
async fn single_blob_limit_bytes_is_exactly_5_mib() {
    // Lock the canonical 5 MiB constant — mutants like "* → +" in the
    // const expression `5 * 1024 * 1024` are caught by this assertion.
    assert_eq!(SINGLE_BLOB_LIMIT_BYTES, 5_242_880);
    assert_eq!(SINGLE_BLOB_LIMIT_BYTES, 5 * 1024 * 1024);
}

#[tokio::test]
async fn region_display_matches_bucket_suffix() {
    use std::fmt::Write as _;
    let mut buf = String::new();
    write!(&mut buf, "{}", Region::Wnam).expect("display");
    assert_eq!(buf, "wnam");
    buf.clear();
    write!(&mut buf, "{}", Region::Weur).expect("display");
    assert_eq!(buf, "weur");
    buf.clear();
    write!(&mut buf, "{}", Region::Sam).expect("display");
    assert_eq!(buf, "sam");
}

#[tokio::test]
async fn in_memory_r2_len_is_empty_consistent() {
    let backend = InMemoryR2::new();
    assert_eq!(backend.len(), 0);
    assert!(backend.is_empty());

    let writer = R2Writer::new(Region::Wnam, Arc::new(InMemoryR2::new()));
    assert_eq!(writer.region(), Region::Wnam);

    // Drive a put through a fresh backend to bump len().
    let backend2 = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend2));
    let ctx = fixture_ctx(Region::Wnam, 71);
    let body = Bytes::from_static(b"len check");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    writer.put(&ctx, &vb).await.expect("put ok");
    assert_eq!(backend2.len(), 1);
    assert!(!backend2.is_empty());
}

#[tokio::test]
async fn in_memory_r2_head_reflects_storage_state() {
    use corelink_worker::storage::r2::R2Backend;
    let backend = InMemoryR2::new();
    let key = "test-key";
    assert!(!backend.head(key).await.expect("head ok"));
    backend
        .put_if_none_match(key, Bytes::from_static(b"x"))
        .await
        .expect("put ok");
    assert!(backend.head(key).await.expect("head ok"));
}
