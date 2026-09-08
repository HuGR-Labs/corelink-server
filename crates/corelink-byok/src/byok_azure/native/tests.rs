use super::*;

#[test]
fn fips_static_l2_for_test_constructor() {
    let http = reqwest::Client::new();
    let creds = EntraCredentials::for_test_static("t");
    let p = AzureKeyVaultRealProvider::for_test(http, creds, "eastus", "https://x.invalid");
    assert_eq!(p.fips_level(), FipsLevel::Fips140_2_L2);
    assert_eq!(p.provider_kind(), KmsProviderKind::AzureKeyVault);
    assert_eq!(p.region(), "eastus");
}

#[test]
fn access_mapping() {
    let mut kb = KeyBundle::default();
    kb.attributes.enabled = Some(true);
    assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Ok);

    kb.attributes.enabled = Some(false);
    assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Revoked);

    kb.attributes.enabled = Some(true);
    kb.key = Some(JsonWebKey {
        kty: "RSA-HSM".to_string(),
        key_ops: vec!["sign".to_string()],
    });
    assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Revoked);
}

#[test]
fn http_to_access() {
    let api = ApiErrorInner::default();
    assert_eq!(
        map_get_error_to_access(401, &api, "k"),
        KmsAccessStatus::Revoked
    );
    assert_eq!(
        map_get_error_to_access(403, &api, "k"),
        KmsAccessStatus::Revoked
    );
    assert_eq!(
        map_get_error_to_access(404, &api, "k"),
        KmsAccessStatus::NotFound
    );
    assert_eq!(
        map_get_error_to_access(429, &api, "k"),
        KmsAccessStatus::Throttled
    );
    assert!(matches!(
        map_get_error_to_access(500, &api, "k"),
        KmsAccessStatus::ApiError(500)
    ));
}
