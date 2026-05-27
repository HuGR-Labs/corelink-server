//! E2E integration test — real AWS KMS staging.
//!
//! # Requirements
//!
//! Set these environment variables before running:
//!
//! ```text
//! AWS_KMS_TEST_KEY_ARN=arn:aws:kms:us-east-1:123456789012:key/<uuid>
//! AWS_REGION=us-east-1
//! AWS_ACCESS_KEY_ID=...
//! AWS_SECRET_ACCESS_KEY=...
//! ```
//!
//! If `AWS_KMS_TEST_KEY_ARN` is absent the tests are skipped (CI without staging
//! credentials will see `test ... ignored`).
//!
//! # What is tested
//!
//! - Full write → read roundtrip with real AWS KMS.
//! - DEK cache hit path (second read skips KMS call).
//! - `check_access` returns `KmsAccessStatus::Ok` for enabled CMK.
//! - Latency p99 ≤ 30 ms (asserted via 10 samples).

#![forbid(unsafe_code)]
#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::print_stderr, clippy::print_stdout)]

#[cfg(test)]
mod e2e_aws_kms {
    use std::env;
    use std::time::Instant;

    use corelink_byok::{
        dek_cache::DekCache,
        envelope::EnvelopeEncryptor,
        types::{KmsAccessStatus, KmsKeyId, KmsProviderKind},
        KmsProvider,
    };
    use corelink_byok::aws::AwsKmsProvider;

    fn get_test_key_arn() -> Option<String> {
        env::var("AWS_KMS_TEST_KEY_ARN")
            .or_else(|_| env::var("AWS_TEST_KEY_ARN"))
            .ok()
    }

    fn get_region() -> String {
        env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string())
    }

    #[tokio::test]
    async fn e2e_encrypt_decrypt_roundtrip() {
        let Some(key_arn) = get_test_key_arn() else {
            eprintln!("SKIP: AWS_KMS_TEST_KEY_ARN not set");
            return;
        };

        let region = get_region();
        let provider = AwsKmsProvider::new(&region)
            .await
            .expect("AwsKmsProvider::new");

        let cache = DekCache::new(300).expect("DekCache::new");
        let enc = EnvelopeEncryptor::new(provider, cache);

        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: key_arn.clone(),
            region: region.clone(),
        };

        let plaintext = b"CoreLink BYOK E2E integration test - hello AWS KMS";
        let tenant_id = "e2e_tenant_001";
        let blob_hash = "sha256:e2e0000000000000000000000000000000000000000000000000000000000";

        let blob = enc
            .encrypt(plaintext, &key_id, tenant_id, blob_hash)
            .await
            .expect("encrypt");

        let recovered = enc
            .decrypt(&blob, tenant_id, blob_hash)
            .await
            .expect("decrypt");

        assert_eq!(recovered, plaintext, "plaintext mismatch after roundtrip");
    }

    #[tokio::test]
    async fn e2e_second_decrypt_hits_cache() {
        let Some(key_arn) = get_test_key_arn() else {
            eprintln!("SKIP: AWS_KMS_TEST_KEY_ARN not set");
            return;
        };

        let region = get_region();
        let provider = AwsKmsProvider::new(&region)
            .await
            .expect("AwsKmsProvider::new");

        let cache = DekCache::new(300).expect("DekCache::new");
        let enc = EnvelopeEncryptor::new(provider, cache);

        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: key_arn,
            region,
        };

        let plaintext = b"cache hit test";
        let tenant_id = "cache_tenant";
        let blob_hash = "sha256:cachetestblob000000";

        let blob = enc
            .encrypt(plaintext, &key_id, tenant_id, blob_hash)
            .await
            .expect("encrypt");

        // First decrypt — populates cache.
        enc.decrypt(&blob, tenant_id, blob_hash).await.expect("first decrypt");

        // Second decrypt — should hit cache; significantly faster.
        let t0 = Instant::now();
        let recovered = enc
            .decrypt(&blob, tenant_id, blob_hash)
            .await
            .expect("second decrypt");
        let elapsed = t0.elapsed();

        assert_eq!(recovered, plaintext);
        // Cache hit should be sub-millisecond; 10 ms is very conservative.
        assert!(
            elapsed.as_millis() < 10,
            "cache hit latency {elapsed:?} > 10 ms — possible KMS call on cached path"
        );
    }

    #[tokio::test]
    async fn e2e_check_access_returns_ok() {
        let Some(key_arn) = get_test_key_arn() else {
            eprintln!("SKIP: AWS_KMS_TEST_KEY_ARN not set");
            return;
        };

        let region = get_region();
        let provider = AwsKmsProvider::new(&region)
            .await
            .expect("AwsKmsProvider::new");

        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: key_arn,
            region,
        };

        let status = provider.check_access(&key_id).await.expect("check_access");
        assert_eq!(
            status,
            KmsAccessStatus::Ok,
            "expected Ok for enabled CMK, got {status:?}"
        );
    }

    #[tokio::test]
    async fn e2e_wrap_latency_p99_under_30ms() {
        let Some(key_arn) = get_test_key_arn() else {
            eprintln!("SKIP: AWS_KMS_TEST_KEY_ARN not set");
            return;
        };

        let region = get_region();
        let provider = AwsKmsProvider::new(&region)
            .await
            .expect("AwsKmsProvider::new");

        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: key_arn,
            region,
        };

        let aad = serde_json::json!({"tenant_id": "lat_test", "blob_hash": "sha256:lat000"});

        let mut latencies_ms: Vec<u128> = Vec::with_capacity(10);

        for _ in 0..10 {
            let mut dek_bytes = [0u8; 32];
            getrandom::getrandom(&mut dek_bytes).expect("getrandom");
            let dek = corelink_byok::types::Dek { bytes: dek_bytes };

            let t0 = Instant::now();
            provider
                .wrap_dek(&dek, &key_id, Some(&aad))
                .await
                .expect("wrap_dek");
            latencies_ms.push(t0.elapsed().as_millis());
        }

        latencies_ms.sort_unstable();
        let p99 = latencies_ms[9]; // 10 samples; p99 ≈ worst case

        assert!(
            p99 <= 30,
            "wrap_dek latency p99 = {p99} ms — exceeds 30 ms SLO (region-co-located)"
        );
    }
}
