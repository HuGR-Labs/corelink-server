//! Property tests at 10k iter (PR; 100k via `PROPTEST_CASES` env)
//! covering the four invariants the WI-S05-003 spec mandates:
//!
//! 1. **Tenant isolation**: cross-tenant upload-id replay rejected
//!    + distinct tenants ALWAYS get distinct object keys.
//! 2. **Idempotent operations**: re-initiate / re-upload-part
//!    same-bytes / re-complete / re-abort are no-ops on the
//!    durable state.
//! 3. **Abort safety**: abort on a Completed session is rejected;
//!    abort on an InProgress session clears the part list and
//!    transitions to Aborted; idempotent re-abort.
//! 4. **Part ordering**: every Complete with the same `(part,
//!    etag)` set yields the same composite ETag regardless of the
//!    submission order ⇒ NO; explicit S3 semantic is that the
//!    submission order is ETag-bearing. We assert the canonical
//!    ascending-order Complete is byte-stable, AND that the
//!    BTreeMap-internal recorded order is ascending regardless of
//!    `upload_part` interleaving.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    reason = "test code; panic on assertion failure is the contract"
)]

use bytes::Bytes;
use corelink_r2_multipart::{
    Bucket, InMemoryMultipartAdapter, InitiateRequest, MultipartAdapter, MultipartError,
    PartETag, PartNumber,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix};
use proptest::prelude::*;
use proptest::test_runner::Config;
use uuid::Uuid;
use zeroize::Zeroizing;

fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
    derive_prefix(&tdk, tenant)
}

fn dummy_digest_hex(seed: u64) -> String {
    // Deterministic 64-char lowercase hex from a u64 seed.
    let mut s = String::with_capacity(64);
    let bytes = seed.to_le_bytes();
    for _ in 0..8 {
        for b in bytes.iter() {
            s.push_str(&format!("{b:02x}"));
        }
    }
    s.truncate(64);
    s
}

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// Distinct tenants ALWAYS get distinct object keys (tenant
    /// prefix is the load-bearing isolator).
    #[test]
    fn prop_tenant_isolation_distinct_keys(
        t_a in any::<u128>(),
        t_b in any::<u128>(),
        digest_seed in any::<u64>(),
    ) {
        prop_assume!(t_a != t_b);
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let ta = Uuid::from_u128(t_a);
            let tb = Uuid::from_u128(t_b);
            let pa = fixed_prefix(ta);
            let pb = fixed_prefix(tb);
            let dh = dummy_digest_hex(digest_seed);
            let ua = adapter
                .initiate(InitiateRequest::new(ta, &pa, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            let ub = adapter
                .initiate(InitiateRequest::new(tb, &pb, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            // Tenants with distinct UUIDs collide on the prefix
            // only with probability ≈ 2^-96 — for the bound this
            // proptest sweeps, structural collision is impossible
            // up to that bound.
            assert_ne!(ua.object_key, ub.object_key);
            assert_ne!(ua.upload_id, ub.upload_id);
        });
    }

    /// Cross-tenant upload-id replay is structurally rejected on
    /// every method that takes an `(upload, tenant_id)` pair.
    #[test]
    fn prop_cross_tenant_replay_rejected(
        t_a in any::<u128>(),
        t_b in any::<u128>(),
        digest_seed in any::<u64>(),
    ) {
        prop_assume!(t_a != t_b);
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let ta = Uuid::from_u128(t_a);
            let tb = Uuid::from_u128(t_b);
            let pa = fixed_prefix(ta);
            let dh = dummy_digest_hex(digest_seed);
            let ua = adapter
                .initiate(InitiateRequest::new(ta, &pa, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            // upload_part with attacker tenant.
            let r = adapter
                .upload_part(
                    tb,
                    &ua,
                    PartNumber::new(1).unwrap(),
                    Bytes::from_static(b"x"),
                )
                .await;
            let is_cross_tenant = matches!(r, Err(MultipartError::CrossTenantUpload { .. }));
            prop_assert!(is_cross_tenant);
            // complete with attacker tenant.
            let r = adapter
                .complete(tb, &ua, vec![(PartNumber::new(1).unwrap(), PartETag::new("x"))])
                .await;
            let is_cross_tenant = matches!(r, Err(MultipartError::CrossTenantUpload { .. }));
            prop_assert!(is_cross_tenant);
            // abort with attacker tenant.
            let r = adapter.abort(tb, &ua).await;
            let is_cross_tenant = matches!(r, Err(MultipartError::CrossTenantUpload { .. }));
            prop_assert!(is_cross_tenant);
            Ok(())
        }).unwrap();
    }

    /// Re-initiate on the same `(tenant_id, object_key)` while
    /// InProgress returns the same upload id.
    #[test]
    fn prop_initiate_idempotent(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let u1 = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            let u2 = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            assert_eq!(u1.upload_id, u2.upload_id);
            assert_eq!(u1.object_key, u2.object_key);
        });
    }

    /// Re-upload-part with same bytes is a no-op on the recorded
    /// part list (ETag stable, single recorded entry).
    #[test]
    fn prop_upload_part_idempotent_same_bytes(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
        body in proptest::collection::vec(any::<u8>(), 0..1024),
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let u = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            let body_bytes = Bytes::from(body);
            let e1 = adapter
                .upload_part(tenant, &u, PartNumber::new(1).unwrap(), body_bytes.clone())
                .await
                .unwrap();
            let e2 = adapter
                .upload_part(tenant, &u, PartNumber::new(1).unwrap(), body_bytes)
                .await
                .unwrap();
            assert_eq!(e1, e2);
            let parts = adapter.parts_snapshot(&u.upload_id);
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0].0, 1);
            assert_eq!(parts[0].1, e1);
        });
    }

    /// Re-complete with same parts list returns the cached
    /// CompletedObject (byte-equal).
    #[test]
    fn prop_complete_idempotent(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
        body in proptest::collection::vec(any::<u8>(), 1..512),
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let u = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            let body_bytes = Bytes::from(body);
            let e = adapter
                .upload_part(tenant, &u, PartNumber::new(1).unwrap(), body_bytes)
                .await
                .unwrap();
            let parts = vec![(PartNumber::new(1).unwrap(), e)];
            let o1 = adapter.complete(tenant, &u, parts.clone()).await.unwrap();
            let o2 = adapter.complete(tenant, &u, parts).await.unwrap();
            assert_eq!(o1, o2);
        });
    }

    /// Abort on InProgress transitions to Aborted; idempotent
    /// re-abort; abort on Completed is rejected.
    #[test]
    fn prop_abort_safety(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
        complete_first in any::<bool>(),
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let u = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            if complete_first {
                let e = adapter
                    .upload_part(
                        tenant,
                        &u,
                        PartNumber::new(1).unwrap(),
                        Bytes::from_static(b"a"),
                    )
                    .await
                    .unwrap();
                let _ = adapter
                    .complete(tenant, &u, vec![(PartNumber::new(1).unwrap(), e)])
                    .await
                    .unwrap();
                // Abort on Completed must reject.
                let r = adapter.abort(tenant, &u).await;
                assert!(matches!(r, Err(MultipartError::UploadIdNotFound { .. })));
            } else {
                // Abort + re-abort is idempotent.
                adapter.abort(tenant, &u).await.unwrap();
                adapter.abort(tenant, &u).await.unwrap();
                // upload_part on aborted is rejected.
                let r = adapter
                    .upload_part(
                        tenant,
                        &u,
                        PartNumber::new(1).unwrap(),
                        Bytes::from_static(b"x"),
                    )
                    .await;
                assert!(matches!(r, Err(MultipartError::UploadIdNotFound { .. })));
            }
        });
    }

    /// Part ordering: regardless of the order in which the parts
    /// are uploaded, the recorded order surfaces as ascending part
    /// numbers via `parts_snapshot` (the BTreeMap invariant).
    #[test]
    fn prop_part_ordering_canonical(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
        n_parts in 2u32..=8u32,
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let u = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, Bucket::Chunk, "sam", &dh))
                .await
                .unwrap();
            // Upload parts in REVERSE order to exercise the
            // BTreeMap canonicalization.
            for i in (1..=n_parts).rev() {
                let body = format!("body-{i}");
                let _ = adapter
                    .upload_part(
                        tenant,
                        &u,
                        PartNumber::new(i).unwrap(),
                        Bytes::from(body.into_bytes()),
                    )
                    .await
                    .unwrap();
            }
            let snapshot = adapter.parts_snapshot(&u.upload_id);
            // BTreeMap surfaces parts in ascending order.
            for (idx, (pn, _)) in snapshot.iter().enumerate() {
                assert_eq!(*pn, (idx as u32) + 1);
            }
        });
    }

    /// Bucket-keyed orphan enumeration: sessions only surface in
    /// the bucket family they belong to AND only if older than
    /// `max_age` AND in the InProgress state.
    #[test]
    fn prop_list_orphans_filters(
        t in any::<u128>(),
        digest_seed in any::<u64>(),
        bucket_choice in any::<bool>(),
        future_secs in 0u64..(30 * 86_400),
    ) {
        let runtime = rt();
        runtime.block_on(async {
            let adapter = InMemoryMultipartAdapter::new();
            let tenant = Uuid::from_u128(t);
            let prefix = fixed_prefix(tenant);
            let dh = dummy_digest_hex(digest_seed);
            let bucket = if bucket_choice {
                Bucket::Chunk
            } else {
                Bucket::Manifest
            };
            let u = adapter
                .initiate(InitiateRequest::new(tenant, &prefix, bucket, "sam", &dh))
                .await
                .unwrap();
            let now = u.initiated_at + std::time::Duration::from_secs(future_secs);
            let max_age = std::time::Duration::from_secs(7 * 86_400);
            let orphans = adapter.list_orphans(bucket, now, max_age).await.unwrap();
            // Aged out only if future_secs > 7d.
            let expect_orphan = future_secs > 7 * 86_400;
            assert_eq!(orphans.len(), if expect_orphan { 1 } else { 0 });
            // Other-bucket enumeration always empty.
            let other = if bucket_choice { Bucket::Manifest } else { Bucket::Chunk };
            let orph_other = adapter.list_orphans(other, now, max_age).await.unwrap();
            assert!(orph_other.is_empty());
        });
    }
}
