//! Example: BYOK read path with DEK cache.
//!
//! Demonstrates:
//! 1. Fetch wrapped DEK from D1 (stub).
//! 2. Check DEK cache (5 min TTL hard).
//! 3. On miss: unwrap via KMS.
//! 4. Decrypt body with AES-256-GCM.

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::print_stdout, clippy::print_stderr)]
use async_trait::async_trait;
use serde_json::Value;

use corelink_byok::{
    dek_cache::DekCache,
    envelope::EnvelopeEncryptor,
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};

struct StubProvider;

#[async_trait]
impl KmsProvider for StubProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AwsKms
    }
    fn region(&self) -> &str {
        "us-east-1"
    }
    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_3_L1
    }
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext: dek.bytes.to_vec(),
            encryption_context: encryption_context.cloned(),
        })
    }
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&wrapped.ciphertext);
        Ok(Dek { bytes })
    }
    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(KmsAccessStatus::Ok)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = DekCache::new(300)?;
    let enc = EnvelopeEncryptor::new(StubProvider, cache);

    let key_id = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:123:key/example-cmk".to_string(),
        region: "us-east-1".to_string(),
    };

    // Simulate write first.
    let plaintext = b"CoreLink BYOK read example";
    let blob = enc
        .encrypt(plaintext, &key_id, "tenant_example", "sha256:read_example")
        .await?;

    // Read path.
    let recovered = enc
        .decrypt(&blob, "tenant_example", "sha256:read_example")
        .await?;

    assert_eq!(recovered, plaintext);
    println!("Read path complete: {} bytes recovered", recovered.len());
    println!("  plaintext = {:?}", std::str::from_utf8(&recovered)?);

    Ok(())
}
