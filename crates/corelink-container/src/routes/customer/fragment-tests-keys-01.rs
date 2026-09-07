#[tokio::test]
async fn keys_revoke_rejects_member_viewer_and_missing_role_before_handler() {
    let (state, shared) = fixture();
    shared
        .seed_pat(
            "t6",
            PatRow::new(
                "pat_rev",
                "revoke-me",
                vec![],
                "2026-05-01T00:00:00Z",
                None,
                None,
            ),
        )
        .expect("seed");

    for role in ["member", "viewer", "", "operator"] {
        let request = Request::builder()
            .uri("/v1/customer/keys/pat_rev/revoke")
            .method("POST")
            .header("x-corelink-tenant-id", "t6")
            .header("x-corelink-token-prefix", "clerk")
            .header("x-corelink-scope", "read-write")
            .header("x-corelink-role", role)
            .body(Body::empty())
            .unwrap();
        let response = router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "role {role:?}");
    }

    let rows = CustomerKeysHandler::list(
        shared.as_ref(),
        KeysListRequest::new("t6", "clerk", now_ms()),
    )
    .expect("list seeded PAT");
    assert!(rows.pats[0].revoked_at.is_none());
}
