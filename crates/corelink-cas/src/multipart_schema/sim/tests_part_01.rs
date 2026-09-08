use super::*;

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

fn session_id(seed: u8) -> SessionId {
    let bytes = [seed; 16];
    SessionId(Uuid::from_bytes(bytes))
}

fn chunk_req(tid: Uuid, digest_seed: u8, region: MultipartRegion) -> ChunkUpsertRequest {
    ChunkUpsertRequest {
        tenant_id: tid,
        chunk_digest: hex64(digest_seed),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        region,
        r2_object_key: format!("chunk-{}/abcd/{}", region, hex64(digest_seed)),
        size_bytes: 2 * 1024 * 1024,
        now_ms: 1_000,
        created_by_pat_id: None,
        created_by_request_id: None,
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

fn session_req(sid: SessionId, tid: Uuid, blob_seed: u8, now_ms: i64) -> MultipartInitiateRequest {
    MultipartInitiateRequest {
        session_id: sid,
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

// -- chunks ---------------------------------------------------

#[test]
fn chunks_first_upsert_inserts_with_refcount_one() {
    let mut s = MultipartSchema::new();
    let r = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    assert_eq!(
        s.upsert_chunk(r.clone()).unwrap(),
        ChunkUpsertOutcome::Inserted
    );
    assert_eq!(s.chunks_count(), 1);
    let row = s.get_chunk(&ten_a(), &r.chunk_digest).unwrap();
    assert_eq!(row.refcount, 1);
}

#[test]
fn chunks_idempotent_upsert_increments_refcount() {
    let mut s = MultipartSchema::new();
    let r1 = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    s.upsert_chunk(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    let outcome = s.upsert_chunk(r2).unwrap();
    assert_eq!(outcome, ChunkUpsertOutcome::Idempotent { new_refcount: 2 });
    let row = s.get_chunk(&ten_a(), &r1.chunk_digest).unwrap();
    assert_eq!(row.refcount, 2);
    assert_eq!(row.last_referenced_at, 5_000);
}

#[test]
fn chunks_cross_tenant_isolated() {
    let mut s = MultipartSchema::new();
    s.upsert_chunk(chunk_req(ten_a(), 1, MultipartRegion::Sam))
        .unwrap();
    let row = s.get_chunk(&ten_b(), &hex64(1));
    assert!(row.is_none());
    assert_eq!(s.list_chunks_for_tenant(&ten_b()).len(), 0);
    assert_eq!(s.list_chunks_for_tenant(&ten_a()).len(), 1);
}

#[test]
fn chunks_check_digest_len() {
    let mut s = MultipartSchema::new();
    let mut r = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    r.chunk_digest = "abcd".to_string();
    assert_eq!(
        s.upsert_chunk(r).unwrap_err(),
        SimError::CheckViolation("chk_chunks_digest_len")
    );
}

#[test]
fn chunks_check_size_bounds() {
    let mut s = MultipartSchema::new();
    let mut r = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    r.size_bytes = 0; // below lower bound 1
    assert_eq!(
        s.upsert_chunk(r.clone()).unwrap_err(),
        SimError::CheckViolation("chk_chunks_size_bytes")
    );
    r.size_bytes = CHUNK_SIZE_BYTES_MAX + 1;
    assert_eq!(
        s.upsert_chunk(r).unwrap_err(),
        SimError::CheckViolation("chk_chunks_size_bytes")
    );
}

#[test]
fn chunks_check_path_key_positive() {
    let mut s = MultipartSchema::new();
    let mut r = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    r.path_key_id = 0;
    assert_eq!(
        s.upsert_chunk(r).unwrap_err(),
        SimError::CheckViolation("chk_chunks_path_key_id_positive")
    );
}

#[test]
fn chunks_region_immutable_on_idempotent() {
    let mut s = MultipartSchema::new();
    let r1 = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    s.upsert_chunk(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.region = MultipartRegion::Iad;
    assert_eq!(
        s.upsert_chunk(r2).unwrap_err(),
        SimError::CheckViolation("chk_chunks_region_immutable")
    );
}

#[test]
fn chunks_size_immutable_on_idempotent() {
    let mut s = MultipartSchema::new();
    let r1 = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    s.upsert_chunk(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.size_bytes = 1_024;
    assert_eq!(
        s.upsert_chunk(r2).unwrap_err(),
        SimError::CheckViolation("chk_chunks_size_immutable")
    );
}

#[test]
fn chunks_decrement_refcount_floors_at_zero() {
    let mut s = MultipartSchema::new();
    let r = chunk_req(ten_a(), 1, MultipartRegion::Sam);
    s.upsert_chunk(r.clone()).unwrap();
    assert_eq!(
        s.decrement_chunk_refcount(&ten_a(), &r.chunk_digest),
        Some(0)
    );
    assert_eq!(
        s.decrement_chunk_refcount(&ten_a(), &r.chunk_digest),
        Some(0)
    );
    assert_eq!(s.list_gc_candidates(&ten_a()).len(), 1);
}

// -- manifest_chunks ------------------------------------------

#[test]
fn manifest_first_insert() {
    let mut s = MultipartSchema::new();
    let r = manifest_req(ten_a(), 0xaa, 0, 1);
    assert_eq!(
        s.insert_manifest_chunk(r).unwrap(),
        ManifestChunkInsertOutcome::Inserted
    );
    assert_eq!(s.manifest_chunks_count(), 1);
}

#[test]
fn manifest_idempotent_same_chunk_digest() {
    let mut s = MultipartSchema::new();
    let r = manifest_req(ten_a(), 0xaa, 0, 1);
    s.insert_manifest_chunk(r.clone()).unwrap();
    assert_eq!(
        s.insert_manifest_chunk(r).unwrap(),
        ManifestChunkInsertOutcome::Idempotent
    );
}

#[test]
fn manifest_pk_collision_with_different_chunk_rejects() {
    let mut s = MultipartSchema::new();
    let r1 = manifest_req(ten_a(), 0xaa, 0, 1);
    s.insert_manifest_chunk(r1).unwrap();
    let r2 = manifest_req(ten_a(), 0xaa, 0, 2);
    let err = s.insert_manifest_chunk(r2).unwrap_err();
    assert!(matches!(err, SimError::UniqueViolation(_)));
}

#[test]
fn manifest_chunk_index_bounded() {
    let mut s = MultipartSchema::new();
    let mut r = manifest_req(ten_a(), 0xaa, MAX_CHUNKS_PER_BLOB, 1);
    assert_eq!(
        s.insert_manifest_chunk(r.clone()).unwrap_err(),
        SimError::CheckViolation("chk_manifest_chunk_index_bounded")
    );
    r.chunk_index = -1;
    assert_eq!(
        s.insert_manifest_chunk(r).unwrap_err(),
        SimError::CheckViolation("chk_manifest_chunk_index_bounded")
    );
}

#[test]
fn manifest_list_ordered() {
    let mut s = MultipartSchema::new();
    // Insert in non-monotonic order; PK btree gives us an
    // ordered-by-key view for free.
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 2, 0x11))
        .unwrap();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 0x12))
        .unwrap();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 1, 0x13))
        .unwrap();
    let view = s.list_manifest_chunks(&ten_a(), &hex64(0xaa));
    assert_eq!(view.len(), 3);
    assert_eq!(view[0].chunk_index, 0);
    assert_eq!(view[1].chunk_index, 1);
    assert_eq!(view[2].chunk_index, 2);
}

#[test]
fn manifest_cross_tenant_isolated() {
    let mut s = MultipartSchema::new();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 1))
        .unwrap();
    assert_eq!(s.list_manifest_chunks(&ten_b(), &hex64(0xaa)).len(), 0);
}

// -- multipart_sessions ---------------------------------------

#[test]
fn session_initiate_inserts() {
    let mut s = MultipartSchema::new();
    let r = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    assert_eq!(
        s.initiate_session(r).unwrap(),
        MultipartInitiateOutcome::Inserted
    );
    assert_eq!(s.sessions_count(), 1);
}

#[test]
fn session_partial_unique_blocks_concurrent_in_progress() {
    let mut s = MultipartSchema::new();
    let r1 = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    s.initiate_session(r1).unwrap();
    let r2 = session_req(session_id(2), ten_a(), 0xaa, 2_000);
    let outcome = s.initiate_session(r2).unwrap();
    assert_eq!(
        outcome,
        MultipartInitiateOutcome::AlreadyInProgress {
            existing: session_id(1)
        }
    );
    assert_eq!(s.sessions_count(), 1); // second insert blocked
}

#[test]
fn session_completed_does_not_block_new_session_for_same_blob() {
    // Lote 10.5bis P0 fix: partial UNIQUE only on in_progress.
    let mut s = MultipartSchema::new();
    let r1 = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    s.initiate_session(r1).unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Completed,
        now_ms: 5_000,
    })
    .unwrap();
    // After completion, a new in_progress session for the same
    // blob_digest_expected MUST be allowed (audit trail vs active
    // upload).
    let r2 = session_req(session_id(2), ten_a(), 0xaa, 6_000);
    assert_eq!(
        s.initiate_session(r2).unwrap(),
        MultipartInitiateOutcome::Inserted
    );
    assert_eq!(s.sessions_count(), 2);
}

#[test]
fn session_cross_tenant_blob_dedup_independent() {
    // Tenant A's in_progress for blob X does NOT block Tenant B's
    // in_progress for blob X — partial UNIQUE is keyed on
    // (tenant_id, blob_digest).
    let mut s = MultipartSchema::new();
    let r_a = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    let r_b = session_req(session_id(2), ten_b(), 0xaa, 1_000);
    s.initiate_session(r_a).unwrap();
    s.initiate_session(r_b).unwrap();
    assert_eq!(s.sessions_count(), 2);
}
