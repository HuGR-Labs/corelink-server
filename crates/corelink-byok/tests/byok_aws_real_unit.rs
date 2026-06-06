//! Unit tests for `AwsKmsRealProvider` — the GA-hardened AWS KMS adapter.
//!
//! Coverage (≥ 8 unit + 1 prop test):
//!
//! 1. `fips_endpoint_url_pattern_assertion_us_east_1`
//!    — Resolved URL exactly equals `kms-fips.us-east-1.amazonaws.com`.
//! 2. `fips_endpoint_url_pattern_assertion_govcloud`
//!    — Resolved URL exactly equals `kms-fips.us-gov-east-1.amazonaws.com`.
//! 3. `fips_endpoint_enforced_by_default`
//!    — `fips_endpoint_enforced()` is true on a `new_mock` provider.
//! 4. `wrap_unwrap_roundtrip_mock`
//!    — wrap then unwrap recovers original DEK bytes.
//! 5. `aad_canonicalization_is_order_independent`
//!    — Two AAD JSON values with same keys in different order produce the
//!    same wire bytes (deterministic JCS).
//! 6. `aad_tamper_rejected_constant_time`
//!    — Tampered AAD on unwrap returns `AadMismatch` (mock-mode CT compare).
//! 7. `missing_aad_rejected_on_wrap` — `EncryptionContextMissing`.
//! 8. `non_string_aad_value_rejected` — JCS canonicalization fails fast.
//! 9. `wrong_provider_rejected` — Cross-provider key_id rejected.
//! 10. `malformed_arn_rejected` — ARN validation fail-CLOSED.
//! 11. (prop) `prop_aad_jcs_roundtrip_deterministic` — Same logical AAD
//!     always produces same canonical bytes regardless of order.

#![forbid(unsafe_code)]
#![allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use corelink_byok::aws::real::canonicalize_aad_to_string_map;
use corelink_byok::aws::AwsKmsRealProvider;
use corelink_byok::{
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};
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

// ── 1. FIPS endpoint URL pattern assertions ──────────────────────────────────

#[test]
fn fips_endpoint_url_pattern_assertion_us_east_1() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    assert_eq!(
        p.resolved_fips_endpoint(),
        "kms-fips.us-east-1.amazonaws.com"
    );
    assert!(p.fips_endpoint_enforced());
}

#[test]
fn fips_endpoint_url_pattern_assertion_govcloud() {
    let p = AwsKmsRealProvider::new_mock("us-gov-east-1");
    assert_eq!(
        p.resolved_fips_endpoint(),
        "kms-fips.us-gov-east-1.amazonaws.com"
    );
}

#[test]
fn fips_endpoint_url_pattern_starts_with_kms_fips_prefix() {
    for region in ["us-east-1", "us-west-2", "eu-west-1", "us-gov-east-1"] {
        let p = AwsKmsRealProvider::new_mock(region);
        let url = p.resolved_fips_endpoint();
        assert!(
            url.starts_with("kms-fips."),
            "region {region} URL must start with 'kms-fips.': got {url}"
        );
        assert!(
            url.ends_with(".amazonaws.com"),
            "region {region} URL must end with '.amazonaws.com': got {url}"
        );
        assert!(
            url.contains(region),
            "region {region} URL must contain region: got {url}"
        );
    }
}

#[test]
fn fips_level_is_fips_140_3_l1() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    assert_eq!(p.fips_level(), FipsLevel::Fips140_3_L1);
    assert_eq!(p.provider_kind(), KmsProviderKind::AwsKms);
    assert_eq!(p.region(), "us-east-1");
}

// ── 2. Wrap / unwrap roundtrip ───────────────────────────────────────────────

#[tokio::test]
async fn wrap_unwrap_roundtrip_mock() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t-001", "blob_hash": "sha256:abc"});
    let dek = Dek::generate().unwrap();
    let orig = dek.bytes;
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.unwrap();
    assert_eq!(wrapped.provider, KmsProviderKind::AwsKms);
    let unwrapped = p.unwrap_dek(&wrapped).await.unwrap();
    assert_eq!(unwrapped.bytes, orig);
}

// ── 3. AAD JCS canonicalization ───────────────────────────────────────────────

#[test]
fn aad_canonicalization_is_order_independent() {
    let a = json!({"tenant_id": "t-1", "blob_hash": "sha256:abc"});
    let b = json!({"blob_hash": "sha256:abc", "tenant_id": "t-1"});
    let (_, ba) = canonicalize_aad_to_string_map(&a).unwrap();
    let (_, bb) = canonicalize_aad_to_string_map(&b).unwrap();
    assert_eq!(ba, bb);
}

#[tokio::test]
async fn non_string_aad_value_rejected() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let bad_aad = json!({"tenant_id": "t-1", "weight": 42});
    let dek = Dek::generate().unwrap();
    let err = p
        .wrap_dek(&dek, &key_id, Some(&bad_aad))
        .await
        .expect_err("must reject");
    assert!(matches!(err, BYOKError::EnvelopeError(_)));
}

// ── 4. AAD binding enforcement (mock-mode constant-time check) ───────────────

#[tokio::test]
async fn aad_tamper_rejected_constant_time() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad_a = json!({"tenant_id": "t-A", "blob_hash": "sha256:aaa"});
    let aad_b = json!({"tenant_id": "t-B", "blob_hash": "sha256:bbb"});
    let dek = Dek::generate().unwrap();
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad_a)).await.unwrap();
    let tampered = WrappedDek {
        encryption_context: Some(aad_b),
        ..wrapped
    };
    let err = p.unwrap_dek(&tampered).await.expect_err("must reject");
    assert!(matches!(err, BYOKError::AadMismatch));
}

#[tokio::test]
async fn missing_aad_rejected_on_wrap() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let dek = Dek::generate().unwrap();
    let err = p
        .wrap_dek(&dek, &key_id, None)
        .await
        .expect_err("must reject");
    assert!(matches!(err, BYOKError::EncryptionContextMissing));
}

#[tokio::test]
async fn missing_aad_rejected_on_unwrap() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let dek = Dek::generate().unwrap();
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.unwrap();
    let stripped = WrappedDek {
        encryption_context: None,
        ..wrapped
    };
    let err = p.unwrap_dek(&stripped).await.expect_err("must reject");
    assert!(matches!(err, BYOKError::EncryptionContextMissing));
}

// ── 5. Cross-provider / malformed ARN ────────────────────────────────────────

#[tokio::test]
async fn wrong_provider_rejected() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id: VALID_ARN.to_string(),
        region: "us-east-1".to_string(),
    };
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let dek = Dek::generate().unwrap();
    let err = p
        .wrap_dek(&dek, &bad, Some(&aad))
        .await
        .expect_err("must reject");
    assert!(matches!(err, BYOKError::EnvelopeError(_)));
}

#[tokio::test]
async fn malformed_arn_rejected() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:000000000000:alias/my-key".to_string(),
        region: "us-east-1".to_string(),
    };
    let aad = json!({"tenant_id": "t", "blob_hash": "h"});
    let dek = Dek::generate().unwrap();
    let err = p
        .wrap_dek(&dek, &bad, Some(&aad))
        .await
        .expect_err("must reject");
    assert!(matches!(err, BYOKError::Provider(_)));
}

#[tokio::test]
async fn check_access_mock_returns_ok() {
    let p = AwsKmsRealProvider::new_mock("us-east-1");
    let key_id = fixture_key_id();
    let st = p.check_access(&key_id).await.unwrap();
    assert_eq!(st, KmsAccessStatus::Ok);
}

// ── 6. Property test: deterministic JCS canonicalization ────────────────────

mod prop {
    use super::*;
    use proptest::prelude::*;

    /// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 96
    /// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
    fn proptest_cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(96)
    }

    fn arb_kv() -> impl Strategy<Value = (String, String)> {
        let key = "[a-z][a-z0-9_]{0,15}";
        let val = "[a-zA-Z0-9_:.-]{1,32}";
        (key, val).prop_map(|(k, v)| (k, v))
    }

    fn arb_ctx_pair() -> impl Strategy<Value = (serde_json::Value, serde_json::Value)> {
        proptest::collection::vec(arb_kv(), 1..=6).prop_map(|mut kvs| {
            // Build two values: one in original order, one reversed.
            let mut m1 = serde_json::Map::new();
            for (k, v) in &kvs {
                m1.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            kvs.reverse();
            let mut m2 = serde_json::Map::new();
            for (k, v) in &kvs {
                m2.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            (serde_json::Value::Object(m1), serde_json::Value::Object(m2))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

        /// Same logical AAD (same key/value set, any order) → same canonical bytes.
        /// This is the determinism invariant per INV-BYOK-CRYPTO-SOVEREIGNTY.
        #[test]
        fn prop_aad_jcs_roundtrip_deterministic((a, b) in arb_ctx_pair()) {
            let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
            let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
            prop_assert_eq!(bytes_a, bytes_b);
        }
    }
}
