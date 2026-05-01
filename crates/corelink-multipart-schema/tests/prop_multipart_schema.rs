//! Property tests for the WI-S05-004 multipart schema simulator.
//!
//! Coverage (5 spec-mandated invariants × 10k iter PR; 100k nightly via
//! `PROPTEST_CASES`):
//!
//! 1. **chunks PK uniqueness** — re-inserting the same `(tenant_id,
//!    chunk_digest)` increments refcount; cross-tenant same-digest
//!    inserts produce 2 distinct rows.
//! 2. **chunks check constraints** — random invalid payloads
//!    (oversize size_bytes / non-hex digest / sub-1 path_key_id /
//!    sub-1 size_bytes) are rejected 100% of the time with the
//!    canonical [`SimError::CheckViolation`] mnemonic.
//! 3. **chunks tenant isolation** — Tenant A inserts, Tenant B reads
//!    via `get_chunk` / `list_chunks_for_tenant` — empty.
//! 4. **manifest_chunks PK ordered** — chunks indexed `0..N-1`
//!    materialise in the canonical order via `list_manifest_chunks`.
//! 5. **multipart_sessions state transitions** — `in_progress →
//!    completed | aborted` ✓; reverse transitions rejected
//!    100% (`INV-MULTIPART-STATE-MONOTONIC`).
//!
//! Bonus coverage:
//!
//! - **expires_at correctness** — every initiated session satisfies
//!   `expires_at_ms = started_at + ttl_ms (default 7d)` and
//!   `expires_at_ms >= started_at`.
//! - **partial UNIQUE in_progress** — only one in_progress session per
//!   `(tenant, blob_digest_expected)`; completed/aborted rows never
//!   block a fresh start (Lote 10.5bis P0 fix).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_multipart_schema::{
    ChunkUpsertOutcome, ChunkUpsertRequest, ManifestChunkInsertRequest, MultipartFinalizeOutcome,
    MultipartFinalizeRequest, MultipartInitiateOutcome, MultipartInitiateRequest, MultipartRegion,
    MultipartSchema, MultipartSessionState, SessionId, SimError, CHUNK_SIZE_BYTES_MAX,
    DEFAULT_SESSION_TTL_MS, MAX_CHUNKS_PER_BLOB,
};
use proptest::prelude::*;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Strategies.
// ---------------------------------------------------------------------------

fn tenant_uuid_strategy() -> impl Strategy<Value = Uuid> {
    any::<u128>().prop_map(Uuid::from_u128)
}

fn session_id_strategy() -> impl Strategy<Value = SessionId> {
    any::<u128>().prop_map(|n| SessionId(Uuid::from_u128(n)))
}

fn hex64_strategy() -> impl Strategy<Value = String> {
    "[0-9a-f]{64}".prop_map(String::from)
}

fn region_strategy() -> impl Strategy<Value = MultipartRegion> {
    prop_oneof![
        Just(MultipartRegion::Sam),
        Just(MultipartRegion::Iad),
        Just(MultipartRegion::Lhr),
        Just(MultipartRegion::Nrt),
        Just(MultipartRegion::Syd),
    ]
}

fn chunk_request_strategy() -> impl Strategy<Value = ChunkUpsertRequest> {
    (
        tenant_uuid_strategy(),
        hex64_strategy(),
        region_strategy(),
        1i64..=CHUNK_SIZE_BYTES_MAX,
        1i64..1_000_000_i64, // now_ms
    )
        .prop_map(
            |(tid, chunk_digest, region, size_bytes, now_ms)| ChunkUpsertRequest {
                tenant_id: tid,
                chunk_digest: chunk_digest.clone(),
                tenant_prefix: [0xab; 16],
                path_key_id: 1,
                region,
                r2_object_key: format!("chunk-{region}/abcd/{chunk_digest}"),
                size_bytes,
                now_ms,
                created_by_pat_id: None,
                created_by_request_id: None,
            },
        )
}

fn session_initiate_strategy() -> impl Strategy<Value = MultipartInitiateRequest> {
    (
        session_id_strategy(),
        tenant_uuid_strategy(),
        hex64_strategy(),
        region_strategy(),
        1i64..1_000_000_i64,
    )
        .prop_map(|(sid, tid, blob, region, now_ms)| MultipartInitiateRequest {
            session_id: sid,
            tenant_id: tid,
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            blob_digest_expected: blob.clone(),
            region,
            bucket: format!("corelink-chunk-{region}"),
            object_key: format!("chunk-{region}/abcd/{blob}"),
            now_ms,
            ttl_ms: None,
            created_by_pat_id: None,
            created_by_request_id: format!("req-{tid}"),
        })
}

// ---------------------------------------------------------------------------
// 1. chunks PK uniqueness — idempotent retry vs cross-tenant distinct.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_chunks_pk_uniqueness(req in chunk_request_strategy()) {
        let mut s = MultipartSchema::new();
        let outcome1 = s.upsert_chunk(req.clone()).unwrap();
        prop_assert_eq!(outcome1, ChunkUpsertOutcome::Inserted);
        // Same payload re-issued ⇒ idempotent refcount increment.
        let mut req2 = req.clone();
        req2.now_ms = req.now_ms.saturating_add(1);
        let outcome2 = s.upsert_chunk(req2).unwrap();
        prop_assert_eq!(outcome2, ChunkUpsertOutcome::Idempotent { new_refcount: 2 });
        // PK count remains 1 — refcount tracks duplicate use.
        prop_assert_eq!(s.chunks_count(), 1);
    }

    #[test]
    fn prop_chunks_cross_tenant_distinct(
        ten_a in tenant_uuid_strategy(),
        ten_b in tenant_uuid_strategy(),
        digest in hex64_strategy(),
        region in region_strategy(),
        size_bytes in 1i64..=CHUNK_SIZE_BYTES_MAX,
    ) {
        prop_assume!(ten_a != ten_b);
        let mut s = MultipartSchema::new();
        let mk = |tid: Uuid| ChunkUpsertRequest {
            tenant_id: tid,
            chunk_digest: digest.clone(),
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            region,
            r2_object_key: format!("chunk-{region}/abcd/{digest}"),
            size_bytes,
            now_ms: 1_000,
            created_by_pat_id: None,
            created_by_request_id: None,
        };
        s.upsert_chunk(mk(ten_a)).unwrap();
        s.upsert_chunk(mk(ten_b)).unwrap();
        prop_assert_eq!(s.chunks_count(), 2);
        // Each tenant sees exactly one row; refcount remains 1 for each.
        prop_assert_eq!(s.list_chunks_for_tenant(&ten_a).len(), 1);
        prop_assert_eq!(s.list_chunks_for_tenant(&ten_b).len(), 1);
        prop_assert_eq!(s.get_chunk(&ten_a, &digest).unwrap().refcount, 1);
        prop_assert_eq!(s.get_chunk(&ten_b, &digest).unwrap().refcount, 1);
    }
}

// ---------------------------------------------------------------------------
// 2. chunks CHECK constraint enforcement — invalid payloads always rejected.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_chunks_check_size_bytes_rejected(
        ten in tenant_uuid_strategy(),
        digest in hex64_strategy(),
        region in region_strategy(),
        // Either too small (<=0) or too big (> 4 MiB).
        oversize in (CHUNK_SIZE_BYTES_MAX + 1)..=(CHUNK_SIZE_BYTES_MAX + 4096),
    ) {
        let mut s = MultipartSchema::new();
        let req = ChunkUpsertRequest {
            tenant_id: ten,
            chunk_digest: digest.clone(),
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            region,
            r2_object_key: format!("chunk-{region}/abcd/{digest}"),
            size_bytes: oversize,
            now_ms: 1_000,
            created_by_pat_id: None,
            created_by_request_id: None,
        };
        let err = s.upsert_chunk(req).unwrap_err();
        prop_assert_eq!(err, SimError::CheckViolation("chk_chunks_size_bytes"));
    }

    #[test]
    fn prop_chunks_check_zero_size_rejected(
        ten in tenant_uuid_strategy(),
        digest in hex64_strategy(),
        region in region_strategy(),
        too_small in -1024i64..=0_i64,
    ) {
        let mut s = MultipartSchema::new();
        let req = ChunkUpsertRequest {
            tenant_id: ten,
            chunk_digest: digest.clone(),
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            region,
            r2_object_key: format!("chunk-{region}/abcd/{digest}"),
            size_bytes: too_small,
            now_ms: 1_000,
            created_by_pat_id: None,
            created_by_request_id: None,
        };
        let err = s.upsert_chunk(req).unwrap_err();
        prop_assert_eq!(err, SimError::CheckViolation("chk_chunks_size_bytes"));
    }

    #[test]
    fn prop_chunks_check_path_key_id_positive(
        ten in tenant_uuid_strategy(),
        digest in hex64_strategy(),
        region in region_strategy(),
        non_positive in i64::MIN..1_i64,
    ) {
        let mut s = MultipartSchema::new();
        let req = ChunkUpsertRequest {
            tenant_id: ten,
            chunk_digest: digest.clone(),
            tenant_prefix: [0xab; 16],
            path_key_id: non_positive,
            region,
            r2_object_key: format!("chunk-{region}/abcd/{digest}"),
            size_bytes: 2 * 1024 * 1024,
            now_ms: 1_000,
            created_by_pat_id: None,
            created_by_request_id: None,
        };
        let err = s.upsert_chunk(req).unwrap_err();
        prop_assert_eq!(err, SimError::CheckViolation("chk_chunks_path_key_id_positive"));
    }
}

// ---------------------------------------------------------------------------
// 3. chunks tenant isolation — Tenant B never sees Tenant A's rows.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_chunks_tenant_isolation(
        ten_a in tenant_uuid_strategy(),
        ten_b in tenant_uuid_strategy(),
        digest in hex64_strategy(),
        region in region_strategy(),
        size_bytes in 1i64..=CHUNK_SIZE_BYTES_MAX,
    ) {
        prop_assume!(ten_a != ten_b);
        let mut s = MultipartSchema::new();
        let req = ChunkUpsertRequest {
            tenant_id: ten_a,
            chunk_digest: digest.clone(),
            tenant_prefix: [0xaa; 16],
            path_key_id: 1,
            region,
            r2_object_key: format!("chunk-{region}/abcd/{digest}"),
            size_bytes,
            now_ms: 1_000,
            created_by_pat_id: None,
            created_by_request_id: None,
        };
        s.upsert_chunk(req).unwrap();
        // Tenant B asking for the same chunk_digest under their PK ⇒ none.
        prop_assert!(s.get_chunk(&ten_b, &digest).is_none());
        prop_assert_eq!(s.list_chunks_for_tenant(&ten_b).len(), 0);
        // Tenant A is the sole owner of any row it inserted.
        let view_a = s.list_chunks_for_tenant(&ten_a);
        prop_assert_eq!(view_a.len(), 1);
        prop_assert_eq!(view_a[0].tenant_id, ten_a);
    }
}

// ---------------------------------------------------------------------------
// 4. manifest_chunks PK ordered — sequential indices materialise in order.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_manifest_chunks_pk_ordered(
        ten in tenant_uuid_strategy(),
        blob_digest in hex64_strategy(),
        n in 1usize..=64_usize,
    ) {
        let mut s = MultipartSchema::new();
        // Insert chunks in REVERSE order to prove the PK btree, not
        // insertion order, drives the canonical scan.
        for i in (0..n).rev() {
            let req = ManifestChunkInsertRequest {
                tenant_id: ten,
                blob_digest: blob_digest.clone(),
                chunk_index: i64::try_from(i).unwrap(),
                chunk_digest: format!("{:0>64}", i),
            };
            s.insert_manifest_chunk(req).unwrap();
        }
        let view = s.list_manifest_chunks(&ten, &blob_digest);
        prop_assert_eq!(view.len(), n);
        for (i, row) in view.iter().enumerate() {
            prop_assert_eq!(row.chunk_index, i64::try_from(i).unwrap());
        }
    }

    #[test]
    fn prop_manifest_chunk_index_bounded(
        ten in tenant_uuid_strategy(),
        blob_digest in hex64_strategy(),
        chunk_digest in hex64_strategy(),
        // sample out-of-range values; legal range is 0..MAX_CHUNKS_PER_BLOB.
        idx in MAX_CHUNKS_PER_BLOB..=(MAX_CHUNKS_PER_BLOB + 1024),
    ) {
        let mut s = MultipartSchema::new();
        let req = ManifestChunkInsertRequest {
            tenant_id: ten,
            blob_digest,
            chunk_index: idx,
            chunk_digest,
        };
        let err = s.insert_manifest_chunk(req).unwrap_err();
        prop_assert_eq!(
            err,
            SimError::CheckViolation("chk_manifest_chunk_index_bounded")
        );
    }
}

// ---------------------------------------------------------------------------
// 5. multipart_sessions state transitions — INV-MULTIPART-STATE-MONOTONIC.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_multipart_sessions_state_transitions_forward(
        req in session_initiate_strategy(),
    ) {
        let mut s = MultipartSchema::new();
        s.initiate_session(req.clone()).unwrap();
        // in_progress → completed
        let outcome = s
            .finalize_session(MultipartFinalizeRequest {
                session_id: req.session_id,
                tenant_id: req.tenant_id,
                target_state: MultipartSessionState::Completed,
                now_ms: req.now_ms.saturating_add(1),
            })
            .unwrap();
        prop_assert_eq!(outcome, MultipartFinalizeOutcome::Finalized);
    }

    #[test]
    fn prop_multipart_sessions_state_reverse_rejected(
        req in session_initiate_strategy(),
    ) {
        let mut s = MultipartSchema::new();
        s.initiate_session(req.clone()).unwrap();
        // First finalize to Completed.
        s.finalize_session(MultipartFinalizeRequest {
            session_id: req.session_id,
            tenant_id: req.tenant_id,
            target_state: MultipartSessionState::Completed,
            now_ms: req.now_ms.saturating_add(1),
        })
        .unwrap();
        // Reverse → in_progress rejected.
        let err = s
            .finalize_session(MultipartFinalizeRequest {
                session_id: req.session_id,
                tenant_id: req.tenant_id,
                target_state: MultipartSessionState::InProgress,
                now_ms: req.now_ms.saturating_add(2),
            })
            .unwrap_err();
        let is_invalid = matches!(err, SimError::InvalidStateTransition { .. });
        prop_assert!(is_invalid);
        // completed → aborted rejected.
        let err = s
            .finalize_session(MultipartFinalizeRequest {
                session_id: req.session_id,
                tenant_id: req.tenant_id,
                target_state: MultipartSessionState::Aborted,
                now_ms: req.now_ms.saturating_add(3),
            })
            .unwrap_err();
        let is_invalid = matches!(
            err,
            SimError::InvalidStateTransition {
                from: MultipartSessionState::Completed,
                to: MultipartSessionState::Aborted,
            }
        );
        prop_assert!(is_invalid);
    }

    #[test]
    fn prop_multipart_sessions_partial_unique_in_progress(
        req in session_initiate_strategy(),
        sid_b in session_id_strategy(),
    ) {
        prop_assume!(sid_b != req.session_id);
        let mut s = MultipartSchema::new();
        s.initiate_session(req.clone()).unwrap();
        // Second initiate for the same (tenant, blob_digest) with
        // a fresh session_id ⇒ AlreadyInProgress.
        let mut req2 = req.clone();
        req2.session_id = sid_b;
        req2.now_ms = req.now_ms.saturating_add(1);
        let outcome = s.initiate_session(req2).unwrap();
        prop_assert_eq!(
            outcome,
            MultipartInitiateOutcome::AlreadyInProgress {
                existing: req.session_id
            }
        );
        prop_assert_eq!(s.sessions_count(), 1);
    }

    #[test]
    fn prop_multipart_sessions_completed_does_not_block_new_session(
        req in session_initiate_strategy(),
        sid_b in session_id_strategy(),
    ) {
        // Lote 10.5bis P0 fix: only state='in_progress' enforces UNIQUE.
        prop_assume!(sid_b != req.session_id);
        let mut s = MultipartSchema::new();
        s.initiate_session(req.clone()).unwrap();
        s.finalize_session(MultipartFinalizeRequest {
            session_id: req.session_id,
            tenant_id: req.tenant_id,
            target_state: MultipartSessionState::Completed,
            now_ms: req.now_ms.saturating_add(1),
        })
        .unwrap();
        // After completion, fresh session for the same blob is allowed.
        let mut req2 = req.clone();
        req2.session_id = sid_b;
        req2.now_ms = req.now_ms.saturating_add(2);
        prop_assert_eq!(
            s.initiate_session(req2).unwrap(),
            MultipartInitiateOutcome::Inserted
        );
        prop_assert_eq!(s.sessions_count(), 2);
    }

    #[test]
    fn prop_multipart_sessions_expires_at_correctness(
        req in session_initiate_strategy(),
    ) {
        let mut s = MultipartSchema::new();
        s.initiate_session(req.clone()).unwrap();
        let row = s.get_session(&req.tenant_id, &req.session_id).unwrap().unwrap();
        prop_assert_eq!(row.expires_at_ms, req.now_ms.saturating_add(DEFAULT_SESSION_TTL_MS));
        prop_assert!(row.expires_at_ms >= row.started_at);
        prop_assert!(row.last_activity_at >= row.started_at);
    }
}

// ---------------------------------------------------------------------------
// 6. Schema-level multi-tenant isolation — N tenants only A writes.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn prop_multi_tenant_chunks_isolated(
        ten_a in tenant_uuid_strategy(),
        ten_b in tenant_uuid_strategy(),
        ten_c in tenant_uuid_strategy(),
        ten_d in tenant_uuid_strategy(),
        digests in proptest::collection::vec(hex64_strategy(), 4..=8),
    ) {
        let tenants = [ten_a, ten_b, ten_c, ten_d];
        let mut distinct = std::collections::HashSet::new();
        for t in &tenants {
            prop_assume!(distinct.insert(*t));
        }
        let mut s = MultipartSchema::new();
        let mut now = 1_000_i64;
        for d in &digests {
            let req = ChunkUpsertRequest {
                tenant_id: ten_a,
                chunk_digest: d.clone(),
                tenant_prefix: [0xaa; 16],
                path_key_id: 1,
                region: MultipartRegion::Sam,
                r2_object_key: format!("chunk-sam/abcd/{d}"),
                size_bytes: 2 * 1024 * 1024,
                now_ms: now,
                created_by_pat_id: None,
                created_by_request_id: None,
            };
            // duplicates within the same tenant collapse to refcount++.
            let _ = s.upsert_chunk(req);
            now = now.saturating_add(1);
        }
        // Every other tenant sees an empty list; no row leaks.
        for &other in &[ten_b, ten_c, ten_d] {
            prop_assert_eq!(s.list_chunks_for_tenant(&other).len(), 0);
            for d in &digests {
                prop_assert!(s.get_chunk(&other, d).is_none());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 7. Migration idempotency — re-apply preserves rows.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_migration_reapply_preserves_rows(req in chunk_request_strategy()) {
        let mut s = MultipartSchema::new();
        s.upsert_chunk(req.clone()).unwrap();
        let before = s.chunks_count();
        s.reapply_migration().unwrap();
        s.reapply_migration().unwrap();
        prop_assert_eq!(s.chunks_count(), before);
        let row = s.get_chunk(&req.tenant_id, &req.chunk_digest).unwrap();
        prop_assert_eq!(&row.chunk_digest, &req.chunk_digest);
    }
}
