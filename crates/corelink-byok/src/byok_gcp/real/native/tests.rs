use super::*;

#[test]
fn state_mapping_smoke() {
    assert_eq!(map_state_to_access("ENABLED"), KmsAccessStatus::Ok);
    assert_eq!(map_state_to_access("DISABLED"), KmsAccessStatus::Revoked);
    assert_eq!(
        map_state_to_access("DESTROY_SCHEDULED"),
        KmsAccessStatus::Revoked
    );
    assert_eq!(map_state_to_access("DESTROYED"), KmsAccessStatus::NotFound);
    assert!(matches!(
        map_state_to_access("PENDING_GENERATION"),
        KmsAccessStatus::ApiError(_)
    ));
    assert!(matches!(
        map_state_to_access("unknown"),
        KmsAccessStatus::ApiError(_)
    ));
}

#[test]
fn http_to_access_smoke() {
    assert_eq!(map_get_error_to_access(403), KmsAccessStatus::Revoked);
    assert_eq!(map_get_error_to_access(404), KmsAccessStatus::NotFound);
    assert_eq!(map_get_error_to_access(429), KmsAccessStatus::Throttled);
    assert!(matches!(
        map_get_error_to_access(500),
        KmsAccessStatus::ApiError(500)
    ));
}

#[test]
fn hostname_of_handles_scheme_and_path() {
    assert_eq!(
        hostname_of("https://cloudkms.googleapis.com"),
        "cloudkms.googleapis.com"
    );
    assert_eq!(
        hostname_of("https://cloudkms.us-east1.rep.googleapis.com/"),
        "cloudkms.us-east1.rep.googleapis.com"
    );
    assert_eq!(hostname_of("http://127.0.0.1:8080"), "127.0.0.1");
}
