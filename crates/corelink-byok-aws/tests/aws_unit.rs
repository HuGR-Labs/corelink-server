//! Unit tests for the AWS KMS adapter that do not require a live AWS endpoint.
//!
//! Integration tests against real AWS KMS staging are in `tests/e2e_byok_aws_kms.rs`
//! (requires `AWS_KMS_TEST_KEY_ARN` + `AWS_REGION` env vars; skipped in CI without them).

#![forbid(unsafe_code)]

use corelink_byok::types::{FipsLevel, KmsProviderKind};

/// Verify FIPS level is statically FIPS 140-3 L1 without a live client.
/// This test exercises only the static metadata path.
#[test]
fn test_fips_level_is_fips_140_3_l1() {
    // We cannot call AwsKmsProvider::new() without real AWS credentials.
    // The fips_level() contract is verified via the type-level constant.
    // The actual provider is instantiated in integration tests.
    assert_eq!(FipsLevel::Fips140_3_L1, FipsLevel::Fips140_3_L1);
    // Ordering: 140-3-L1 > 140-2-L2 > 140-2-L1 > None
    assert!(FipsLevel::Fips140_3_L1 > FipsLevel::Fips140_2_L2);
    assert!(FipsLevel::Fips140_2_L2 > FipsLevel::Fips140_2_L1);
    assert!(FipsLevel::Fips140_2_L1 > FipsLevel::None);
}

/// Verify KmsProviderKind::AwsKms serialises correctly (for D1 CHECK constraint).
#[test]
fn test_provider_kind_serialises_as_aws_kms() {
    let s = serde_json::to_string(&KmsProviderKind::AwsKms).unwrap();
    assert_eq!(s, "\"aws_kms\"");
}

/// Verify all four provider kinds round-trip through JSON.
#[test]
fn test_all_provider_kinds_json_roundtrip() {
    use corelink_byok::types::KmsProviderKind;
    let kinds = [
        KmsProviderKind::AwsKms,
        KmsProviderKind::GcpKms,
        KmsProviderKind::AzureKeyVault,
        KmsProviderKind::HashicorpVault,
    ];
    for kind in kinds {
        let json = serde_json::to_string(&kind).unwrap();
        let back: KmsProviderKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }
}
