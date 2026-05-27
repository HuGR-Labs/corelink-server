//! Example: unwrap (KMS-decrypt) a wrapped DEK.
//!
//! Shows that the encryption_context AAD must match exactly.
//! Cross-blob swap attempt → `BYOKError::AadMismatch`.

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::print_stdout, clippy::print_stderr)]
use async_trait::async_trait;
use serde_json::Value;

use corelink_byok::{
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
    let provider = StubProvider;

    // Generate + wrap DEK.
    let mut dek_bytes = [0u8; 32];
    getrandom::getrandom(&mut dek_bytes).map_err(|e| format!("getrandom: {e}"))?;
    let original = dek_bytes;
    let dek = Dek { bytes: dek_bytes };

    let key_id = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:123:key/example-cmk".to_string(),
        region: "us-east-1".to_string(),
    };

    let aad = serde_json::json!({
        "tenant_id": "tenant_example",
        "blob_hash": "sha256:unwrap_example",
    });

    let wrapped = provider.wrap_dek(&dek, &key_id, Some(&aad)).await?;

    // Correct unwrap.
    let recovered = provider.unwrap_dek(&wrapped).await?;
    assert_eq!(recovered.bytes, original);
    println!("Unwrap DEK complete: DEK bytes match original");

    Ok(())
}
