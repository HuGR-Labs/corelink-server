#[test]
fn session_id_pk_collision_rejected() {
    let mut s = MultipartSchema::new();
    let r1 = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    s.initiate_session(r1).unwrap();
    // Same session_id, different blob → PK collision.
    let r2 = session_req(session_id(1), ten_a(), 0xbb, 2_000);
    let err = s.initiate_session(r2).unwrap_err();
    assert!(matches!(err, SimError::UniqueViolation(_)));
}

#[test]
fn session_finalize_in_progress_to_completed() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let outcome = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: session_id(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::Completed,
            now_ms: 5_000,
        })
        .unwrap();
    assert_eq!(outcome, MultipartFinalizeOutcome::Finalized);
    let row = s.get_session(&ten_a(), &session_id(1)).unwrap().unwrap();
    assert_eq!(row.state, MultipartSessionState::Completed);
    assert_eq!(row.finalized_at_ms, Some(5_000));
}

#[test]
fn session_finalize_idempotent_echo() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Completed,
        now_ms: 5_000,
    })
    .unwrap();
    let outcome = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: session_id(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::Completed,
            now_ms: 9_000,
        })
        .unwrap();
    assert_eq!(outcome, MultipartFinalizeOutcome::IdempotentEcho);
}

#[test]
fn session_finalize_completed_to_aborted_rejected() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Completed,
        now_ms: 5_000,
    })
    .unwrap();
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: session_id(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::Aborted,
            now_ms: 7_000,
        })
        .unwrap_err();
    assert!(matches!(
        err,
        SimError::InvalidStateTransition {
            from: MultipartSessionState::Completed,
            to: MultipartSessionState::Aborted,
        }
    ));
}

#[test]
fn session_finalize_in_progress_target_rejected() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: session_id(1),
            tenant_id: ten_a(),
            target_state: MultipartSessionState::InProgress,
            now_ms: 5_000,
        })
        .unwrap_err();
    assert!(matches!(err, SimError::InvalidStateTransition { .. }));
}

#[test]
fn session_cross_tenant_get_returns_err() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let err = s.get_session(&ten_b(), &session_id(1)).unwrap_err();
    assert!(matches!(err, SimError::CrossTenantSession { .. }));
}

#[test]
fn session_cross_tenant_finalize_rejected() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    let err = s
        .finalize_session(MultipartFinalizeRequest {
            session_id: session_id(1),
            tenant_id: ten_b(),
            target_state: MultipartSessionState::Completed,
            now_ms: 5_000,
        })
        .unwrap_err();
    assert!(matches!(err, SimError::CrossTenantSession { .. }));
}

#[test]
fn session_touch_monotonic() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 5_000))
        .unwrap();
    s.touch_session(&ten_a(), &session_id(1), 3_000).unwrap(); // older — clamp
    let row = s.get_session(&ten_a(), &session_id(1)).unwrap().unwrap();
    assert_eq!(row.last_activity_at, 5_000); // not rolled back
    s.touch_session(&ten_a(), &session_id(1), 7_000).unwrap();
    let row = s.get_session(&ten_a(), &session_id(1)).unwrap().unwrap();
    assert_eq!(row.last_activity_at, 7_000);
}

#[test]
fn session_touch_terminal_rejected() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Aborted,
        now_ms: 5_000,
    })
    .unwrap();
    let err = s
        .touch_session(&ten_a(), &session_id(1), 7_000)
        .unwrap_err();
    assert!(matches!(err, SimError::InvalidStateTransition { .. }));
}

#[test]
fn session_record_r2_upload_id_terminal_rejected() {
    let mut s = MultipartSchema::new();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(1),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Aborted,
        now_ms: 5_000,
    })
    .unwrap();
    let err = s
        .record_r2_upload_id(&ten_a(), &session_id(1), "r2-upload-1".to_string())
        .unwrap_err();
    assert!(matches!(err, SimError::InvalidStateTransition { .. }));
}

#[test]
fn session_orphan_sweep_partial_index_semantic() {
    let mut s = MultipartSchema::new();
    // Three sessions, two in_progress, one completed.
    s.initiate_session(session_req(session_id(1), ten_a(), 0xaa, 1_000))
        .unwrap();
    s.initiate_session(session_req(session_id(2), ten_a(), 0xbb, 5_000))
        .unwrap();
    s.initiate_session(session_req(session_id(3), ten_a(), 0xcc, 1_000))
        .unwrap();
    s.finalize_session(MultipartFinalizeRequest {
        session_id: session_id(3),
        tenant_id: ten_a(),
        target_state: MultipartSessionState::Completed,
        now_ms: 1_500,
    })
    .unwrap();
    // Cutoff = 2_500; only session 1 (last_activity 1_000 < 2_500)
    // qualifies among in_progress rows. Session 2's last_activity
    // is 5_000 (in_progress but past cutoff). Session 3 is
    // completed so the partial index excludes it regardless of
    // last_activity.
    let orphans = s.list_orphan_sessions(2_500);
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].session_id, session_id(1));
}

#[test]
fn session_default_ttl_is_seven_days() {
    let mut s = MultipartSchema::new();
    let r = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    s.initiate_session(r).unwrap();
    let row = s.get_session(&ten_a(), &session_id(1)).unwrap().unwrap();
    assert_eq!(row.expires_at_ms, 1_000 + DEFAULT_SESSION_TTL_MS);
}

#[test]
fn session_check_blob_digest_len() {
    let mut s = MultipartSchema::new();
    let mut r = session_req(session_id(1), ten_a(), 0xaa, 1_000);
    r.blob_digest_expected = "abcd".to_string();
    assert_eq!(
        s.initiate_session(r).unwrap_err(),
        SimError::CheckViolation("chk_multipart_blob_digest_len")
    );
}

#[test]
fn reapply_migration_is_idempotent() {
    let mut s = MultipartSchema::new();
    s.upsert_chunk(chunk_req(ten_a(), 1, MultipartRegion::Sam))
        .unwrap();
    s.insert_manifest_chunk(manifest_req(ten_a(), 0xaa, 0, 1))
        .unwrap();
    s.initiate_session(session_req(session_id(1), ten_a(), 0xbb, 1_000))
        .unwrap();
    s.reapply_migration().unwrap();
    s.reapply_migration().unwrap();
    assert_eq!(s.chunks_count(), 1);
    assert_eq!(s.manifest_chunks_count(), 1);
    assert_eq!(s.sessions_count(), 1);
}
