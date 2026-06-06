//! Chaos suite (WI-S05-003 §15) — 5 in-scope scenarios that the
//! pure-logic adapter can simulate without a live R2 binding:
//!
//! 1. Client disconnect mid-UploadPart → adapter.abort invoked.
//! 2. R2 5xx mid-Complete → BackendError surfaces.
//! 3. 10001 parts attempt → MaxPartsExceeded.
//! 4. Cross-tenant upload_id replay → CrossTenantUpload.
//! 5. Concurrency limit (semaphore caps at 8) — `try_acquire`
//!    trips ConcurrencyLimitReached.
//!
//! The remaining §15 scenarios (Wrangler binding misconfigured,
//! R2 quota exceeded, region fail-over, upload_id TTL expiry, D1
//! partial UNIQUE, ListMultipartUploads pagination edge) require
//! the real R2 binding shim + D1 schema + sweeper wiring; they
//! land alongside WI-S05-006.

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
    AlwaysFailingMultipartAdapter, Bucket, InMemoryMultipartAdapter, InitiateRequest,
    MultipartAdapter, MultipartError, PartETag, PartNumber, SessionState,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix};
use uuid::Uuid;
use zeroize::Zeroizing;

fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
    derive_prefix(&tdk, tenant)
}

fn dummy_digest() -> String {
    "0123456789abcdef".repeat(4)
}

#[tokio::test]
async fn chaos_client_disconnect_mid_upload() {
    // Scenario: the handler observes a client disconnect after
    // some parts have been uploaded; it invokes `abort` on the
    // session. The abort path must clear the recorded parts and
    // transition the session to Aborted.
    let adapter = InMemoryMultipartAdapter::new();
    let tenant = Uuid::nil();
    let prefix = fixed_prefix(tenant);
    let dh = dummy_digest();
    let u = adapter
        .initiate(InitiateRequest::new(
            tenant,
            &prefix,
            Bucket::Chunk,
            "sam",
            &dh,
        ))
        .await
        .unwrap();
    // Upload 3 parts.
    for i in 1u32..=3 {
        let _ = adapter
            .upload_part(
                tenant,
                &u,
                PartNumber::new(i).unwrap(),
                Bytes::from(vec![i as u8; 32]),
            )
            .await
            .unwrap();
    }
    // Client disconnects → handler invokes abort.
    adapter.abort(tenant, &u).await.unwrap();
    assert_eq!(
        adapter.session_state(&u.upload_id),
        Some(SessionState::Aborted)
    );
    let parts = adapter.parts_snapshot(&u.upload_id);
    assert!(
        parts.is_empty(),
        "abort must clear recorded part list (FM-060 cleanup)"
    );
    // upload_part on aborted session yields UploadIdNotFound.
    let r = adapter
        .upload_part(
            tenant,
            &u,
            PartNumber::new(1).unwrap(),
            Bytes::from_static(b"z"),
        )
        .await;
    assert!(matches!(r, Err(MultipartError::UploadIdNotFound { .. })));
}

#[tokio::test]
async fn chaos_r2_backend_failure_surfaces() {
    // Scenario: R2 5xx; every adapter method propagates the
    // backend error verbatim so the handler can map it to 503 +
    // COR_MULTIPART_BACKEND_UNAVAILABLE.
    let adapter = AlwaysFailingMultipartAdapter::new("R2 5xx mid-complete");
    let tenant = Uuid::nil();
    let prefix = fixed_prefix(tenant);
    let dh = dummy_digest();
    let r = adapter
        .initiate(InitiateRequest::new(
            tenant,
            &prefix,
            Bucket::Chunk,
            "sam",
            &dh,
        ))
        .await;
    match r {
        Err(MultipartError::Backend(s)) => assert_eq!(s, "R2 5xx mid-complete"),
        other => panic!("expected Backend error; got {other:?}"),
    }
}

#[tokio::test]
async fn chaos_max_parts_exceeded() {
    // Scenario: handler tries to upload part 10_001; PartNumber
    // ctor rejects at the type level.
    let r = PartNumber::new(10_001);
    match r {
        Err(MultipartError::MaxPartsExceeded { part_number, max }) => {
            assert_eq!(part_number, 10_001);
            assert_eq!(max, 10_000);
        }
        other => panic!("expected MaxPartsExceeded; got {other:?}"),
    }
}

#[tokio::test]
async fn chaos_cross_tenant_upload_id_replay() {
    // Scenario: tenant B captures upload_id from tenant A and
    // tries to upload a part. Adapter rejects.
    let adapter = InMemoryMultipartAdapter::new();
    let ta = Uuid::from_u128(1);
    let tb = Uuid::from_u128(2);
    let pa = fixed_prefix(ta);
    let dh = dummy_digest();
    let u = adapter
        .initiate(InitiateRequest::new(ta, &pa, Bucket::Chunk, "sam", &dh))
        .await
        .unwrap();
    // Tenant B replays.
    let r = adapter
        .upload_part(
            tb,
            &u,
            PartNumber::new(1).unwrap(),
            Bytes::from_static(b"x"),
        )
        .await;
    assert!(matches!(r, Err(MultipartError::CrossTenantUpload { .. })));

    // The legitimate tenant can still finish their work.
    let e = adapter
        .upload_part(
            ta,
            &u,
            PartNumber::new(1).unwrap(),
            Bytes::from_static(b"x"),
        )
        .await
        .unwrap();
    let _ = adapter
        .complete(ta, &u, vec![(PartNumber::new(1).unwrap(), e)])
        .await
        .unwrap();
}

#[tokio::test]
async fn chaos_concurrency_limit_reached() {
    // Scenario: handler tries to acquire 9 concurrent permits for
    // tenant A with a budget of 8. The 9th `try_acquire` trips.
    let adapter = InMemoryMultipartAdapter::with_concurrency(8);
    let tenant = Uuid::nil();
    let mut held = Vec::new();
    for _ in 0..8 {
        held.push(adapter.semaphore().try_acquire(tenant).unwrap());
    }
    let r = adapter.semaphore().try_acquire(tenant);
    match r {
        Err(MultipartError::ConcurrencyLimitReached { limit, .. }) => assert_eq!(limit, 8),
        other => panic!("expected ConcurrencyLimitReached; got {other:?}"),
    }
    // Drop 1 permit → next try succeeds.
    held.pop();
    let _ = adapter.semaphore().try_acquire(tenant).unwrap();
}

#[tokio::test]
async fn chaos_orphan_sweeper_consumes_list_orphans() {
    // Scenario: WI-S05-006 sweeper consumes `list_orphans` to
    // discover sessions older than 7 days and aborts them via
    // the same trait surface.
    let adapter = InMemoryMultipartAdapter::new();
    let tenant = Uuid::nil();
    let prefix = fixed_prefix(tenant);
    let dh = dummy_digest();
    let u = adapter
        .initiate(InitiateRequest::new(
            tenant,
            &prefix,
            Bucket::Chunk,
            "sam",
            &dh,
        ))
        .await
        .unwrap();
    // Advance the sweeper's `now` 8 days into the future.
    let future_now = u.initiated_at + std::time::Duration::from_secs(8 * 86_400);
    let max_age = std::time::Duration::from_secs(7 * 86_400);
    let orphans = adapter
        .list_orphans(Bucket::Chunk, future_now, max_age)
        .await
        .unwrap();
    assert_eq!(orphans.len(), 1);
    // Sweeper aborts each orphan.
    let synthesised = corelink_r2_multipart::MultipartUpload::new(
        orphans[0].upload_id.clone(),
        orphans[0].tenant_id,
        Bucket::Chunk,
        orphans[0].object_key.clone(),
        orphans[0].initiated_at,
    );
    adapter.abort(tenant, &synthesised).await.unwrap();
    // After abort, no orphans surface.
    let orphans = adapter
        .list_orphans(Bucket::Chunk, future_now, max_age)
        .await
        .unwrap();
    assert!(orphans.is_empty());
}

#[tokio::test]
async fn chaos_complete_part_etag_mismatch() {
    // Scenario: client submits Complete with a fabricated ETag
    // that doesn't match what the adapter recorded. Reject with
    // PartMissing(expected, actual) so the client can surface a
    // helpful diagnostic.
    let adapter = InMemoryMultipartAdapter::new();
    let tenant = Uuid::nil();
    let prefix = fixed_prefix(tenant);
    let dh = dummy_digest();
    let u = adapter
        .initiate(InitiateRequest::new(
            tenant,
            &prefix,
            Bucket::Chunk,
            "sam",
            &dh,
        ))
        .await
        .unwrap();
    let _ = adapter
        .upload_part(
            tenant,
            &u,
            PartNumber::new(1).unwrap(),
            Bytes::from_static(b"real"),
        )
        .await
        .unwrap();
    let r = adapter
        .complete(
            tenant,
            &u,
            vec![(PartNumber::new(1).unwrap(), PartETag::new("fabricated"))],
        )
        .await;
    match r {
        Err(MultipartError::PartMissing {
            part_number,
            expected,
            actual,
        }) => {
            assert_eq!(part_number, 1);
            assert!(!expected.is_empty());
            assert_eq!(actual, "fabricated");
        }
        other => panic!("expected PartMissing; got {other:?}"),
    }
}
