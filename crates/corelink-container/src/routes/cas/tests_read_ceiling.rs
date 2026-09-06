#[test]
fn over_size_maps_to_413_not_404_and_not_500() {
    let resp = map_err(CasHandlerError::ObjectTooLarge {
        actual_bytes: 100 * 1024 * 1024,
        limit_bytes: CAS_READ_MAX_OBJECT_BYTES,
    });
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "404 would tell the client to re-upload bytes we already hold; 500 invites a \
             retry that cannot succeed"
    );
}

#[test]
fn absent_and_over_size_stay_distinguishable() {
    let missing = map_err(CasHandlerError::NotFound {
        tenant: "t1".to_owned(),
        hash: "a".repeat(64),
    });
    let too_big = map_err(CasHandlerError::ObjectTooLarge {
        actual_bytes: 1,
        limit_bytes: 0,
    });
    assert_ne!(
        missing.status(),
        too_big.status(),
        "`absent` and `present but refused` are different answers"
    );
}
