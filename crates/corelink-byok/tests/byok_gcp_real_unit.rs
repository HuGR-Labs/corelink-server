//! Unit tests for `GcpKmsRealProvider` — the GA-hardened GCP Cloud KMS adapter.
//!
//! Mirrors the AWS reference suite (`crates/corelink-byok-aws/tests/real_unit.rs`)
//! with provider-specific bindings substituted in.
//!
//! Coverage (≥ 14 unit + 1 prop test):
//!
//! 1. `fips_endpoint_url_pattern_assertion_us_east1`
//!    — Resolved URL exactly equals `cloudkms.us-east1.rep.googleapis.com` on
//!    the mock provider (which forces regional FIPS for test asserts).
//! 2. `fips_endpoint_url_pattern_assertion_europe_west1`
//!    — Resolved URL exactly equals `cloudkms.europe-west1.rep.googleapis.com`.
//! 3. `fips_endpoint_url_pattern_global_default`
//!    — `resolve_endpoint_hostname(_, false)` returns `cloudkms.googleapis.com`.
//! 4. `fips_endpoint_url_starts_with_cloudkms_prefix`
//!    — URL shape gate across 4 regions.
//! 5. `fips_level_is_fips_140_2_l1`
//!    — `KmsProvider::fips_level` reports the Cloud KMS default tier.
//! 6. `wrap_unwrap_roundtrip_mock`
//!    — wrap then unwrap recovers original DEK bytes.
//! 7. `aad_canonicalization_is_order_independent`
//!    — Two AAD JSON values with same keys in different order produce the
//!    same wire bytes (deterministic JCS).
//! 8. `aad_tamper_rejected_constant_time`
//!    — Tampered AAD on unwrap returns `AadMismatch` (mock-mode CT compare).
//! 9. `missing_aad_rejected_on_wrap`
//!    — `EncryptionContextMissing`.
//! 10. `missing_aad_rejected_on_unwrap`
//!     — `EncryptionContextMissing` on unwrap with stripped context.
//! 11. `non_string_aad_value_rejected`
//!     — JCS canonicalization fails fast for non-string AAD values.
//! 12. `wrong_provider_rejected`
//!     — Cross-provider key_id rejected.
//! 13. `malformed_key_resource_rejected`
//!     — Cloud KMS path validation fail-CLOSED.
//! 14. `check_access_mock_returns_ok`
//!     — Mock-mode `check_access` smoke.
//! 15. (prop) `prop_aad_jcs_roundtrip_deterministic`
//!     — Same logical AAD always produces same canonical bytes regardless of
//!     order (the determinism invariant per INV-BYOK-CRYPTO-SOVEREIGNTY).

#![cfg(feature = "production-gcp")]
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
use corelink_byok::gcp::real::{canonicalize_aad_to_string_map, resolve_endpoint_hostname};
use corelink_byok::gcp::GcpKmsRealProvider;
use serde_json::json;

const VALID_KEY: &str =
    "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk";

fn fixture_key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id: VALID_KEY.to_string(),
        region: "us-east1".to_string(),
    }
}

// ── 1. FIPS endpoint URL pattern assertions ──────────────────────────────────

#[test]
fn fips_endpoint_url_pattern_assertion_us_east1() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    assert_eq!(
        p.resolved_fips_endpoint(),
        "cloudkms.us-east1.rep.googleapis.com"
    );
    assert!(p.fips_endpoint_enforced());
}

#[test]
fn fips_endpoint_url_pattern_assertion_europe_west1() {
    let p = GcpKmsRealProvider::new_mock("europe-west1");
    assert_eq!(
        p.resolved_fips_endpoint(),
        "cloudkms.europe-west1.rep.googleapis.com"
    );
}

#[test]
fn fips_endpoint_url_pattern_global_default() {
    assert_eq!(
        resolve_endpoint_hostname("us-east1", false),
        "cloudkms.googleapis.com"
    );
}

#[test]
fn fips_endpoint_url_starts_with_cloudkms_prefix() {
    for region in ["us-east1", "us-west1", "europe-west1", "asia-south1"] {
        let p = GcpKmsRealProvider::new_mock(region);
        let url = p.resolved_fips_endpoint();
        assert!(
            url.starts_with("cloudkms."),
            "region {region} URL must start with 'cloudkms.': got {url}"
        );
        assert!(
            url.ends_with(".rep.googleapis.com"),
            "region {region} URL must end with '.rep.googleapis.com': got {url}"
        );
        assert!(
            url.contains(region),
            "region {region} URL must contain region: got {url}"
        );
    }
}

#[test]
fn fips_level_is_fips_140_2_l1() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    assert_eq!(p.fips_level(), FipsLevel::Fips140_2_L1);
    assert_eq!(p.provider_kind(), KmsProviderKind::GcpKms);
    assert_eq!(p.region(), "us-east1");
}

// ── 2. Wrap / unwrap roundtrip ───────────────────────────────────────────────

#[tokio::test]
async fn wrap_unwrap_roundtrip_mock() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    let key_id = fixture_key_id();
    let aad = json!({"tenant_id": "t-001", "blob_hash": "sha256:abc"});
    let dek = Dek::generate().unwrap();
    let orig = dek.bytes;
    let wrapped = p.wrap_dek(&dek, &key_id, Some(&aad)).await.unwrap();
    assert_eq!(wrapped.provider, KmsProviderKind::GcpKms);
    let unwrapped = p.unwrap_dek(&wrapped).await.unwrap();
    assert_eq!(unwrapped.bytes, orig);
}

// ── 3. AAD JCS canonicalization ──────────────────────────────────────────────

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
    let p = GcpKmsRealProvider::new_mock("us-east1");
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
    let p = GcpKmsRealProvider::new_mock("us-east1");
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
    let p = GcpKmsRealProvider::new_mock("us-east1");
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
    let p = GcpKmsRealProvider::new_mock("us-east1");
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

// ── 5. Cross-provider / malformed key resource ──────────────────────────────

#[tokio::test]
async fn wrong_provider_rejected() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: VALID_KEY.to_string(),
        region: "us-east1".to_string(),
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
async fn malformed_key_resource_rejected() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:000000000000:key/abc".to_string(),
        region: "us-east1".to_string(),
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
    let p = GcpKmsRealProvider::new_mock("us-east1");
    let key_id = fixture_key_id();
    let st = p.check_access(&key_id).await.unwrap();
    assert_eq!(st, KmsAccessStatus::Ok);
}

#[tokio::test]
async fn check_access_wrong_provider_rejected() {
    let p = GcpKmsRealProvider::new_mock("us-east1");
    let bad = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: VALID_KEY.to_string(),
        region: "us-east1".to_string(),
    };
    let err = p.check_access(&bad).await.expect_err("must reject");
    assert!(matches!(err, BYOKError::EnvelopeError(_)));
}

// ── 6. Property test: deterministic JCS canonicalization ────────────────────

mod prop {
    use super::*;
    use proptest::prelude::*;

    fn arb_kv() -> impl Strategy<Value = (String, String)> {
        let key = "[a-z][a-z0-9_]{0,15}";
        let val = "[a-zA-Z0-9_:.-]{1,32}";
        (key, val).prop_map(|(k, v)| (k, v))
    }

    fn arb_ctx_pair() -> impl Strategy<Value = (serde_json::Value, serde_json::Value)> {
        proptest::collection::vec(arb_kv(), 1..=6).prop_map(|mut kvs| {
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

    fn proptest_cases_from_env() -> u32 {
        // Runtime override pattern (see AWS reference) — CI can dial cases up.
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(96)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(proptest_cases_from_env()))]

        /// Same logical AAD (same key/value set, any order) → same canonical
        /// bytes. The determinism invariant per INV-BYOK-CRYPTO-SOVEREIGNTY.
        #[test]
        fn prop_aad_jcs_roundtrip_deterministic((a, b) in arb_ctx_pair()) {
            let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
            let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
            prop_assert_eq!(bytes_a, bytes_b);
        }
    }
}
