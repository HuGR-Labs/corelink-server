//! Unit tests for the AWS KMS adapter — mock-mode coverage that does not
//! require a live AWS endpoint.
//!
//! Real-network integration tests live in `tests/e2e_byok_aws_kms.rs`
//! (requires `AWS_KMS_TEST_KEY_ARN` + `AWS_REGION`; skipped without them).
//!
//! Scope (R2-6):
//! - wrap → unwrap roundtrip (mock mode).
//! - encryption_context AAD binding (mismatch = AadMismatch).
//! - Malformed key ARN rejection (alias, wrong partition, wrong service,
//!   short account, non-UUID resource).
//! - FIPS endpoint hostname resolution.
//! - Mandatory encryption_context.
//! - Wrong-provider cross-tenant rejection.
//! - DescribeKey check_access in mock mode = Ok.
//! - Property test: random AAD context roundtrips and tamper detection.

#![forbid(unsafe_code)]
#![allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use corelink_byok::{
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};
use corelink_byok::aws::{validate_aws_kms_key_arn, AwsKmsProvider};
use serde_json::json;

const VALID_ARN: &str =
    "arn:aws:kms:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000";

fn fixture_key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: VALID_ARN.to_string(),
        region: "us-east-1".to_string(),
    }
}

// ── Smoke / static metadata ───────────────────────────────────────────────────

#[test]
fn fips_level_is_fips_140_3_l1_static() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    assert_eq!(p.fips_level(), FipsLevel::Fips140_3_L1);
    assert_eq!(p.provider_kind(), KmsProviderKind::AwsKms);
    assert_eq!(p.region(), "us-east-1");
}

#[test]
fn provider_kind_serialises_as_aws_kms() {
    let s = serde_json::to_string(&KmsProviderKind::AwsKms).unwrap();
    assert_eq!(s, "\"aws_kms\"");
}

#[test]
fn all_provider_kinds_json_roundtrip() {
    for kind in [
        KmsProviderKind::AwsKms,
        KmsProviderKind::GcpKms,
        KmsProviderKind::AzureKeyVault,
        KmsProviderKind::HashicorpVault,
    ] {
        let json = serde_json::to_string(&kind).unwrap();
        let back: KmsProviderKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }
}

// ── Wrap / unwrap roundtrip (mock) ────────────────────────────────────────────

#[tokio::test]
async fn wrap_unwrap_roundtrip_mock() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t-001", "blob_hash": "sha256:abc"});

    let dek = Dek::generate().expect("dek");
    let orig = dek.bytes;
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.expect("wrap");
    assert_eq!(wrapped.provider, KmsProviderKind::AwsKms);
    let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
    assert_eq!(unwrapped.bytes, orig);
}

#[tokio::test]
async fn encryption_context_mismatch_is_rejected() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad_a = json!({"tenant_id": "t-A", "blob_hash": "sha256:aaa"});
    let aad_b = json!({"tenant_id": "t-B", "blob_hash": "sha256:bbb"});

    let dek = Dek::generate().expect("dek");
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad_a)).await.expect("wrap");

    // Tamper: substitute a different encryption_context on the wrapped struct.
    let tampered = WrappedDek {
        encryption_context: Some(aad_b),
        ..wrapped
    };

    let err = p.unwrap_dek(&tampered).await.expect_err("must reject");
    assert!(
        matches!(err, BYOKError::AadMismatch),
        "expected AadMismatch, got {err:?}"
    );
}

#[tokio::test]
async fn encryption_context_missing_on_wrap_is_rejected() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let dek = Dek::generate().expect("dek");

    let err = p.wrap_dek(&dek, &key_id, None).await.expect_err("must reject");
    assert!(
        matches!(err, BYOKError::EncryptionContextMissing),
        "expected EncryptionContextMissing, got {err:?}"
    );
}

#[tokio::test]
async fn encryption_context_missing_on_unwrap_is_rejected() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});

    let dek = Dek::generate().expect("dek");
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.expect("wrap");

    let stripped = WrappedDek {
        encryption_context: None,
        ..wrapped
    };
    let err = p.unwrap_dek(&stripped).await.expect_err("must reject");
    assert!(
        matches!(err, BYOKError::EncryptionContextMissing),
        "expected EncryptionContextMissing, got {err:?}"
    );
}

// ── DescribeKey access check ──────────────────────────────────────────────────

#[tokio::test]
async fn check_access_mock_returns_ok() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let status = p.check_access(&key_id).await.expect("check_access");
    assert_eq!(status, KmsAccessStatus::Ok);
}

#[tokio::test]
async fn check_access_rejects_wrong_provider_key_id() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = KmsKeyId {
        provider: KmsProviderKind::GcpKms, // wrong provider
        key_arn_or_id: VALID_ARN.to_string(),
        region: "us-east-1".to_string(),
    };
    assert!(p.check_access(&key_id).await.is_err());
}

#[tokio::test]
async fn check_access_rejects_malformed_arn() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "not-an-arn".to_string(),
        region: "us-east-1".to_string(),
    };
    let err = p.check_access(&key_id).await.expect_err("must reject");
    assert!(
        matches!(err, BYOKError::Provider(_)),
        "expected Provider err, got {err:?}"
    );
}

// ── ARN validation ────────────────────────────────────────────────────────────

#[test]
fn arn_validation_accepts_canonical_and_partitions() {
    assert!(validate_aws_kms_key_arn(VALID_ARN).is_ok());
    let gov = "arn:aws-us-gov:kms:us-gov-west-1:000000000000:key/00000000-0000-0000-0000-000000000000";
    assert!(validate_aws_kms_key_arn(gov).is_ok());
    let cn = "arn:aws-cn:kms:cn-north-1:000000000000:key/00000000-0000-0000-0000-000000000000";
    assert!(validate_aws_kms_key_arn(cn).is_ok());
}

#[test]
fn arn_validation_rejects_aliases() {
    assert!(validate_aws_kms_key_arn(
        "arn:aws:kms:us-east-1:000000000000:alias/my-key"
    )
    .is_err());
}

#[test]
fn arn_validation_rejects_short_account() {
    assert!(validate_aws_kms_key_arn(
        "arn:aws:kms:us-east-1:12345:key/00000000-0000-0000-0000-000000000000"
    )
    .is_err());
}

#[test]
fn arn_validation_rejects_non_uuid_resource() {
    assert!(validate_aws_kms_key_arn(
        "arn:aws:kms:us-east-1:000000000000:key/not-a-uuid"
    )
    .is_err());
}

#[test]
fn arn_validation_rejects_wrong_service() {
    assert!(validate_aws_kms_key_arn(
        "arn:aws:s3:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000"
    )
    .is_err());
}

#[test]
fn arn_validation_rejects_empty() {
    assert!(validate_aws_kms_key_arn("").is_err());
}

#[tokio::test]
async fn wrap_rejects_malformed_arn() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:000000000000:alias/my-key".to_string(),
        region: "us-east-1".to_string(),
    };
    let dek = Dek::generate().expect("dek");
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let err = p.wrap_dek(&dek, &key_id, Some(&aad)).await.expect_err("must reject");
    assert!(matches!(err, BYOKError::Provider(_)));
}

// ── FIPS endpoint resolution ─────────────────────────────────────────────────

#[test]
fn fips_endpoint_off_by_default_in_mock() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    assert!(!p.fips_endpoint_enabled());
    assert_eq!(p.resolved_endpoint_hostname(), "kms.us-east-1.amazonaws.com");
}

// ── Cross-provider tampering ─────────────────────────────────────────────────

#[tokio::test]
async fn rejects_wrong_provider_on_unwrap() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let dek = Dek::generate().expect("dek");
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.expect("wrap");

    let tampered = WrappedDek {
        provider: KmsProviderKind::GcpKms,
        ..wrapped
    };
    let err = p.unwrap_dek(&tampered).await.expect_err("must reject");
    assert!(
        matches!(err, BYOKError::EnvelopeError(_)),
        "expected EnvelopeError, got {err:?}"
    );
}

#[tokio::test]
async fn rejects_wrong_provider_on_wrap_key_id() {
    let p = AwsKmsProvider::new_mock("us-east-1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::AzureKeyVault, // wrong
        key_arn_or_id: VALID_ARN.to_string(),
        region: "us-east-1".to_string(),
    };
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let dek = Dek::generate().expect("dek");
    let err = p.wrap_dek(&dek, &bad, Some(&aad)).await.expect_err("must reject");
    assert!(matches!(err, BYOKError::EnvelopeError(_)));
}

// ── Property test: random AAD context ─────────────────────────────────────────

mod prop {
    use super::*;
    use proptest::prelude::*;

    /// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 64
    /// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
    fn proptest_cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(64)
    }

    fn arb_kv() -> impl Strategy<Value = (String, String)> {
        let key = "[a-z][a-z0-9_]{0,15}";
        let val = "[a-zA-Z0-9_:.-]{1,32}";
        (key, val).prop_map(|(k, v)| (k, v))
    }

    fn arb_ctx() -> impl Strategy<Value = serde_json::Value> {
        proptest::collection::vec(arb_kv(), 1..=6).prop_map(|kvs| {
            let mut m = serde_json::Map::new();
            for (k, v) in kvs {
                m.insert(k, serde_json::Value::String(v));
            }
            serde_json::Value::Object(m)
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

        /// Same EncryptionContext on wrap and unwrap → success.
        /// Modified EncryptionContext on unwrap → AadMismatch.
        #[test]
        fn prop_aad_roundtrip_and_tamper(ctx in arb_ctx()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let p = AwsKmsProvider::new_mock("us-east-1");
                let key_id = fixture_key_id();
                let dek = Dek::generate().unwrap();
                let orig = dek.bytes;
                let wrapped = p.wrap_dek(&dek, &key_id, Some(&ctx)).await.unwrap();
                let recovered = p.unwrap_dek(&wrapped).await.unwrap();
                prop_assert_eq!(recovered.bytes, orig);

                // Tamper: add or change a key.
                let mut tampered_ctx = ctx.as_object().unwrap().clone();
                tampered_ctx.insert("__tamper__".to_string(), serde_json::Value::String("x".to_string()));
                let tampered = WrappedDek {
                    encryption_context: Some(serde_json::Value::Object(tampered_ctx)),
                    ..wrapped
                };
                let err = p.unwrap_dek(&tampered).await;
                prop_assert!(err.is_err(), "tamper must be rejected");
                Ok(())
            }).unwrap();
        }
    }
}
