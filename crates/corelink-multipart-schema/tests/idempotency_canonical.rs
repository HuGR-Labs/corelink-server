//! Idempotency + UNIQUE-constraint canonical regression tests.
//!
//! Mirrors the WI §8 Gherkin scenarios that pin the multipart schema
//! semantics at storage layer. The simulator preserves every immutable
//! column under the idempotent path; only refcount / last_referenced_at
//! / last_activity_at / finalized_at_ms / state mutate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_multipart_schema::{
    ChunkUpsertOutcome, ChunkUpsertRequest, ManifestChunkInsertOutcome,
    ManifestChunkInsertRequest, MultipartFinalizeOutcome, MultipartFinalizeRequest,
    MultipartInitiateOutcome, MultipartInitiateRequest, MultipartRegion, MultipartSchema,
    MultipartSessionState, SessionId, SimError, DEFAULT_SESSION_TTL_MS,
};
use uuid::Uuid;

fn ten_a() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

fn ten_b() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
}

fn hex64(seed: u8) -> String {
    let bytes = [seed; 32];
    hex::encode(bytes)
}

fn sid(seed: u8) -> SessionId {
    SessionId(Uuid::from_bytes([seed; 16]))
}

fn chunk_req(tid: Uuid, seed: u8) -> ChunkUpsertRequest {
    ChunkUpsertRequest {
        tenant_id: tid,
        chunk_digest: hex64(seed),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region: MultipartRegion::Sam,
        r2_object_key: format!("chunk-sam/abcd/{}", hex64(seed)),
        size_bytes: 2 * 1024 * 1024,
        now_ms: 1_000,
        created_by_pat_id: Some("pat-original".to_string()),
        created_by_request_id: Some("req-original".to_string()),
    }
}

fn manifest_req(tid: Uuid, blob_seed: u8, idx: i64, chunk_seed: u8) -> ManifestChunkInsertRequest {
    ManifestChunkInsertRequest {
        tenant_id: tid,
        blob_digest: hex64(blob_seed),
        chunk_index: idx,
        chunk_digest: hex64(chunk_seed),
    }
}

fn session_req(s: SessionId, tid: Uuid, blob_seed: u8, now_ms: i64) -> MultipartInitiateRequest {
    MultipartInitiateRequest {
        session_id: s,
        tenant_id: tid,
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        blob_digest_expected: hex64(blob_seed),
        region: MultipartRegion::Sam,
        bucket: "corelink-chunk-sam".to_string(),
        object_key: format!("chunk-sam/abcd/{}", hex64(blob_seed)),
        now_ms,
        ttl_ms: None,
        created_by_pat_id: None,
        created_by_request_id: "req-original".to_string(),
    }
}

// -- chunks -------------------------------------------------------

#[test]
fn chunks_first_insert_then_idempotent_increment_emits_correct_outcomes() {
    let mut s = MultipartSchema::new();
    let r1 = chunk_req(ten_a(), 1);
    assert_eq!(
        s.upsert_chunk(r1.clone()).unwrap(),
        ChunkUpsertOutcome::Inserted
    );
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    assert_eq!(
        s.upsert_chunk(r2).unwrap(),
        ChunkUpsertOutcome::Idempotent { new_refcount: 2 }
    );
    let row = s.get_chunk(&ten_a(), &r1.chunk_digest).unwrap();
    assert_eq!(row.created_at, 1_000);
    assert_eq!(row.last_referenced_at, 5_000);
}

#[test]
fn chunks_idempotent_preserves_immutable_columns() {
    let mut s = MultipartSchema::new();
    let r1 = chunk_req(ten_a(), 1);
    s.upsert_chunk(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    r2.created_by_pat_id = Some("pat-different".to_string()); // ignored on idempotent
    r2.r2_object_key = "different-key".to_string(); // ignored on idempotent
    s.upsert_chunk(r2).unwrap();
    let row = s.get_chunk(&ten_a(), &r1.chunk_digest).unwrap();
    assert_eq!(row.created_by_pat_id.as_deref(), Some("pat-original"));
    assert_eq!(
        row.r2_object_key,
        format!("chunk-sam/abcd/{}", hex64(1))
    );
}

#[test]
fn chunks_pk_per_tenant_chunk_pair() {
    // Same chunk_digest under different tenants ⇒ two rows.
    let mut s = MultipartSchema::new();
    s.upsert_chunk(chunk_req(ten_a(), 7)).unwrap();
    s.upsert_chunk(chunk_req(ten_b(), 7)).unwrap();
    assert_eq!(s.chunks_count(), 2);
    assert!(s.get_chunk(&ten_a(), &hex64(7)).is_some());
    assert!(s.get_chunk(&ten_b(), &hex64(7)).is_some());
    // Refcount is 1 on each (no cross-tenant aggregation).
    assert_eq!(s.get_chunk(&ten_a(), &hex64(7)).unwrap().refcount, 1);
    assert_eq!(s.get_chunk(&ten_b(), &hex64(7)).unwrap().refcount, 1);
}

#[test]
fn chunks_decrement_to_gc_candidate() {
    let mut s = MultipartSchema::new();
    s.upsert_chunk(chunk_req(ten_a(), 1)).unwrap();
    s.upsert_chunk(chunk_req(ten_a(), 1)).unwrap(); // refcount 2
    assert_eq!(s.decrement_chunk_refcount(&ten_a(), &hex64(1)), Some(1));
    assert_eq!(s.list_gc_candidates(&ten_a()).len(), 0);
    assert_eq!(s.decrement_chunk_refcount(&ten_a(), &hex64(1)), Some(0));
    assert_eq!(s.list_gc_candidates(&ten_a()).len(), 1);
    // Floor at zero — never goes negative.
    assert_eq!(s.decrement_chunk_refcount(&ten_a(), &hex64(1)), Some(0));
}

// -- manifest_chunks ----------------------------------------------

#[test]
fn manifest_idempotent_same_chunk_digest_no_op() {
    let mut s = MultipartSchema::new();
    let r = manifest_req(ten_a(), 0xaa, 0, 1);
    s.insert_manifest_chunk(r.clone()).unwrap();
    assert_eq!(
        s.insert_manifest_chunk(r).unwrap(),
        ManifestChunkInsertOutcome::Idempotent
    );
    assert_eq!(s.manifest_chunks_count(), 1);
}

#[test]
fn manifest_pk_collision_with_different_chunk_rejects() {
    let mut s = MultipartSchema::new();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 1))
        .unwrap();
    // Same (tenant, blob, chunk_index) but different chunk_digest =
    // manifest mismatch (handler bug) → UNIQUE violation.
    let err = s
        .insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 2))
        .unwrap_err();
    assert!(matches!(err, SimError::UniqueViolation(_)));
}

// -- multipart_sessions -------------------------------------------

#[test]
fn session_initiate_then_finalize_completed_emits_outcomes() {
    let mut s = MultipartSchema::new();
    let r = session_req(sid(1), ten_a(), 0xaa, 1_000);
    assert_eq!(
        s.initiate_session(r.clone()).unwrap(),
        MultipartInitiateOutcome::Inserted
    );
    let outcome = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: sid(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::Completed,
            now_ms: 10_000,
        })
        .unwrap();
    assert_eq!(outcome, MultipartFinalizeOutcome::Finalized);
    let row = s.get_session(&ten_a(), &sid(1)).unwrap().unwrap();
    assert_eq!(row.state, MultipartSessionState::Completed);
    assert_eq!(row.finalized_at_ms, Some(10_000));
}

#[test]
fn session_partial_unique_returns_existing_session() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let outcome = s
        .initiate_session(session_req(sid(2), ten_a(), 0xaa, 2_000))
        .unwrap();
    assert_eq!(
        outcome,
        MultipartInitiateOutcome::AlreadyInProgress { existing: sid(1) }
    );
    assert_eq!(s.sessions_count(), 1);
}

#[test]
fn session_completed_audit_trail_does_not_block_fresh_start() {
    // Lote 10.5bis P0 fix: only state='in_progress' enforces UNIQUE.
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: sid(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Completed,
        now_ms: 5_000,
    })
    .unwrap();
    // Fresh upload of the same blob is allowed post-completion.
    assert_eq!(
        s.initiate_session(session_req(sid(2), ten_a(), 0xaa, 6_000))
            .unwrap(),
        MultipartInitiateOutcome::Inserted
    );
    assert_eq!(s.sessions_count(), 2);
    // Both records coexist as audit trail.
    assert_eq!(
        s.get_session(&ten_a(), &sid(1)).unwrap().unwrap().state,
        MultipartSessionState::Completed
    );
    assert_eq!(
        s.get_session(&ten_a(), &sid(2)).unwrap().unwrap().state,
        MultipartSessionState::InProgress
    );
}

#[test]
fn session_aborted_audit_trail_does_not_block_fresh_start() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: sid(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Aborted,
        now_ms: 5_000,
    })
    .unwrap();
    assert_eq!(
        s.initiate_session(session_req(sid(2), ten_a(), 0xaa, 6_000))
            .unwrap(),
        MultipartInitiateOutcome::Inserted
    );
    assert_eq!(s.sessions_count(), 2);
}

#[test]
fn session_state_transitions_monotonic() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    // in_progress → aborted ✓
    s.finalize_session(MultipartFinalizeRequest {
        session_id: sid(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Aborted,
        now_ms: 2_000,
    })
    .unwrap();
    // aborted → in_progress ✗
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: sid(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::InProgress,
            now_ms: 3_000,
        })
        .unwrap_err();
    assert!(matches!(err, SimError::InvalidStateTransition { .. }));
    // aborted → completed ✗ (cannot recover an aborted session)
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: sid(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::Completed,
            now_ms: 4_000,
        })
        .unwrap_err();
    assert!(matches!(err, SimError::InvalidStateTransition { .. }));
}

#[test]
fn session_default_ttl_is_seven_days() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let row = s.get_session(&ten_a(), &sid(1)).unwrap().unwrap();
    assert_eq!(row.expires_at_ms, 1_000 + DEFAULT_SESSION_TTL_MS);
}

#[test]
fn session_cross_tenant_isolation_enforced() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(sid(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let err = s.get_session(&ten_b(), &sid(1)).unwrap_err();
    assert!(matches!(err, SimError::CrossTenantSession { .. }));
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: sid(1),
            tenant_id: ten_b(),
            target_state: MultipartSessionState::Completed,
            now_ms: 5_000,
        })
        .unwrap_err();
    assert!(matches!(err, SimError::CrossTenantSession { .. }));
}

#[test]
fn migration_apply_then_apply_again_is_no_op() {
    let mut s = MultipartSchema::new();
    s.upsert_chunk(chunk_req(ten_a(), 1)).unwrap();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 1))
        .unwrap();
    s.initiate_session(session_req(sid(1), ten_a(), 0xbb, 1_000))
        .unwrap();
    let chunks_before = s.chunks_count();
    let manifest_before = s.manifest_chunks_count();
    let sessions_before = s.sessions_count();
    s.reapply_migration().unwrap();
    s.reapply_migration().unwrap();
    s.reapply_migration().unwrap();
    assert_eq!(s.chunks_count(), chunks_before);
    assert_eq!(s.manifest_chunks_count(), manifest_before);
    assert_eq!(s.sessions_count(), sessions_before);
}

#[test]
fn manifest_assembled_in_canonical_order_via_pk_btree() {
    let mut s = MultipartSchema::new();
    // Insert chunks in non-monotonic order to prove PK btree drives
    // the canonical scan-by-chunk_index path (no extra index needed).
    for (idx, chunk_seed) in [(3, 0x11), (0, 0x22), (1, 0x33), (2, 0x44)] {
        s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, idx, chunk_seed))
            .unwrap();
    }
    let view = s.list_manifest_chunks(&ten_a(), &hex64(0xaa));
    assert_eq!(view.len(), 4);
    for (i, row) in view.iter().enumerate() {
        assert_eq!(row.chunk_index, i64::try_from(i).unwrap());
    }
}
