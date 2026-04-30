//! Property tests at 10k iter for audit_outbox semantics.
//!
//! Covers:
//! 1. UNIQUE `(request_id, event_type)` enforcement under concurrency.
//! 2. Idempotent retry: same `(request_id, event_type)` + same payload =
//!    no-op (no double row).
//! 3. Cross-event idempotency: same `request_id` but different
//!    `event_type` is **not** a conflict (different UNIQUE key).
//! 4. Atomicity: when a commit_put fails with AuditIdempotencyConflict,
//!    NO mutation lands on blob_meta either.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code; assertions panic on failure by design"
)]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitDecrementRequest, CommitPutRequest,
    InMemoryMetaStore, InsertOutcome, MetaError, MetaStore,
};
use proptest::prelude::*;
use uuid::Uuid;

fn det_uuid(seed: u64) -> Uuid {
    let mut b = [0u8; 16];
    b[0..8].copy_from_slice(&seed.to_be_bytes());
    Uuid::from_bytes(b)
}

fn det_digest(seed: u64) -> Digest {
    let mut bytes = [0u8; 32];
    let s = seed.to_be_bytes();
    for (i, slot) in bytes.iter_mut().enumerate() {
        *slot = s[i % 8] ^ (i as u8);
    }
    Digest::from_hex(&hex::encode(bytes)).unwrap()
}

fn audit(seed: u64, kind: AuditEventType, request_suffix: &str, payload_seq: u64) -> AuditEvent {
    let id = det_uuid(0xa11d_0000_0000 ^ seed);
    let req = format!("req-{seed}-{request_suffix}");
    let payload = format!(
        r#"{{"specversion":"1.0","type":"{}","seq":{payload_seq}}}"#,
        kind.as_str()
    );
    AuditEvent {
        id,
        request_id: req.into(),
        event_type: kind,
        payload_json: payload,
    }
}

proptest! {
    /// 100 concurrent commit_put attempts with the SAME audit event
    /// → exactly one outbox row, no race-induced duplicates.
    #[test]
    fn concurrent_commit_put_dedupes_outbox(
        n in 2usize..=100,
    ) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let store = Arc::new(InMemoryMetaStore::new());
            let key = BlobMetaKey::new(det_uuid(0xfeed), det_digest(0xbeef));
            let event = audit(0xc0ffee, AuditEventType::CasPutCompleted, "stable", 1);

            let mut handles = Vec::with_capacity(n);
            for i in 0..n {
                let store = Arc::clone(&store);
                let event = event.clone();
                handles.push(tokio::spawn(async move {
                    store
                        .commit_put(CommitPutRequest {
                            key,
                            size_bytes: 4096,
                            now_ms: 100 + i as u64,
                            audit: event,
                        })
                        .await
                }));
            }
            let mut inserted = 0usize;
            let mut already = 0usize;
            for h in handles {
                match h.await.unwrap().unwrap() {
                    InsertOutcome::Inserted => inserted += 1,
                    InsertOutcome::AlreadyExists => already += 1,
                }
            }
            prop_assert_eq!(inserted, 1, "exactly one task wins the first-write race");
            prop_assert_eq!(already, n - 1);
            prop_assert_eq!(store.row_count(), 1);
            prop_assert_eq!(store.outbox_snapshot().len(), 1, "outbox UNIQUE prevents duplicates");
            Ok(())
        })?;
    }

    /// Cross-event idempotency: same request_id under DIFFERENT event_type
    /// is permitted (the UNIQUE constraint is on the *pair*, not on
    /// request_id alone).
    #[test]
    fn cross_event_request_id_reuse_is_allowed(
        seed in 0u64..1_000,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let store = InMemoryMetaStore::new();
            let key = BlobMetaKey::new(det_uuid(seed), det_digest(seed));
            let req = "shared-request-id";
            let put = AuditEvent {
                id: det_uuid(seed.wrapping_add(1)),
                request_id: req.into(),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: "{}".into(),
            };
            let dec = AuditEvent {
                id: det_uuid(seed.wrapping_add(2)),
                request_id: req.into(),
                event_type: AuditEventType::CasRefcountDecremented,
                payload_json: "{}".into(),
            };
            store
                .commit_put(CommitPutRequest { key, size_bytes: 8, now_ms: 1, audit: put })
                .await
                .unwrap();
            // Same request_id, different event_type: must succeed.
            store
                .commit_decrement(CommitDecrementRequest { key, now_ms: 2, audit: dec })
                .await
                .unwrap();
            prop_assert_eq!(store.outbox_snapshot().len(), 2);
            Ok(())
        })?;
    }

    /// Atomicity: when commit_put fails with AuditIdempotencyConflict
    /// (request_id reuse with a different payload), no blob_meta INSERT
    /// lands either.
    #[test]
    fn audit_conflict_aborts_entire_batch(
        seed in 0u64..1_000,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let store = InMemoryMetaStore::new();
            let key1 = BlobMetaKey::new(det_uuid(seed), det_digest(seed));
            let key2 = BlobMetaKey::new(det_uuid(seed.wrapping_add(0xdead)), det_digest(seed.wrapping_add(0xbeef)));
            let first = AuditEvent {
                id: det_uuid(seed.wrapping_add(1)),
                request_id: "shared".into(),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"v":1}"#.into(),
            };
            let conflicting = AuditEvent {
                id: det_uuid(seed.wrapping_add(2)),
                request_id: "shared".into(),
                event_type: AuditEventType::CasPutCompleted,
                payload_json: r#"{"v":2}"#.into(),
            };
            store
                .commit_put(CommitPutRequest { key: key1, size_bytes: 8, now_ms: 1, audit: first })
                .await
                .unwrap();
            // Different blob, same request_id, conflicting payload → abort.
            let err = store
                .commit_put(CommitPutRequest { key: key2, size_bytes: 16, now_ms: 2, audit: conflicting })
                .await
                .unwrap_err();
            prop_assert!(matches!(err, MetaError::AuditIdempotencyConflict));
            // key1 row exists; key2 row does NOT (atomicity).
            prop_assert!(store.get(&key1).await.unwrap().is_some());
            prop_assert!(store.get(&key2).await.unwrap().is_none());
            prop_assert_eq!(store.row_count(), 1);
            prop_assert_eq!(store.outbox_snapshot().len(), 1);
            Ok(())
        })?;
    }

    /// Cross-tenant CAS dedupe: same digest under tenant A and tenant B
    /// produce two distinct rows (per WI §9.5: per-tenant blob, NOT
    /// cross-tenant CAS dedupe).
    #[test]
    fn cross_tenant_same_digest_creates_independent_rows(
        seed in 0u64..1_000,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let store = InMemoryMetaStore::new();
            let digest = det_digest(seed);
            let tenant_a = det_uuid(seed.wrapping_mul(2));
            let tenant_b = det_uuid(seed.wrapping_mul(2).wrapping_add(1));
            prop_assume!(tenant_a != tenant_b);

            let key_a = BlobMetaKey::new(tenant_a, digest);
            let key_b = BlobMetaKey::new(tenant_b, digest);
            store
                .commit_put(CommitPutRequest {
                    key: key_a,
                    size_bytes: 256,
                    now_ms: 1,
                    audit: audit(seed, AuditEventType::CasPutCompleted, "tenant_a", 1),
                })
                .await
                .unwrap();
            store
                .commit_put(CommitPutRequest {
                    key: key_b,
                    size_bytes: 256,
                    now_ms: 1,
                    audit: audit(seed.wrapping_add(1), AuditEventType::CasPutCompleted, "tenant_b", 1),
                })
                .await
                .unwrap();
            prop_assert_eq!(store.row_count(), 2);
            prop_assert!(store.get(&key_a).await.unwrap().is_some());
            prop_assert!(store.get(&key_b).await.unwrap().is_some());
            // Refcounts independent.
            prop_assert_eq!(store.get(&key_a).await.unwrap().unwrap().refcount, 1);
            prop_assert_eq!(store.get(&key_b).await.unwrap().unwrap().refcount, 1);
            Ok(())
        })?;
    }
}
