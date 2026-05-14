//! Example: BYOK write path with a stub AWS KMS provider.
//!
//! Demonstrates the full envelope encryption flow:
//! 1. Generate ephemeral DEK via CSPRNG.
//! 2. Encrypt body with AES-256-GCM.
//! 3. Wrap DEK via KMS.
//! 4. Store wrapped DEK + ciphertext (caller persists to D1 + R2).

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

    let plaintext = b"CoreLink BYOK write example";
    let blob = enc
        .encrypt(plaintext, &key_id, "tenant_example", "sha256:example")
        .await?;

    println!("Write path complete:");
    println!("  ciphertext len = {} bytes", blob.ciphertext.len());
    println!("  nonce = {:?}", blob.nonce);
    println!(
        "  wrapped_dek.kms_provider = {:?}",
        blob.wrapped_dek.provider
    );

    Ok(())
}
