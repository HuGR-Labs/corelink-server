//! libFuzzer harness — encrypt-then-decrypt invariant under random
//! `(plaintext, tenant_id, blob_hash)` triples.
//!
//! Property: `decrypt(encrypt(p, t, b), t, b) == Ok(p)` for every input
//! AND `decrypt(encrypt(p, t1, b1), t2, b2) != Ok(p)` when AAD diverges.
//! Never `Ok(garbage)` — only `Ok(p)` or `Err(_)`. This is the
//! cornerstone INV-BYOK-CRYPTO-SOVEREIGNTY invariant: forge resistance
//! at the envelope boundary.

#![no_main]

use async_trait::async_trait;
use corelink_byok::envelope::{EncryptedBlob, EnvelopeEncryptor};
use corelink_byok::{
    BYOKError, Dek, DekCache, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider,
    KmsProviderKind, WrappedDek,
};
use libfuzzer_sys::fuzz_target;

// In-process stub provider so the fuzzer doesn't touch the network. The
// stub stores DEK bytes as the wrapped-DEK ciphertext; AAD binding +
// AES-GCM stay in the real corelink-byok code paths.
struct StubKms;

#[async_trait]
impl KmsProvider for StubKms {
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
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext: dek.bytes.to_vec(),
            encryption_context: encryption_context.cloned(),
        })
    }
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.ciphertext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid {
                got: wrapped.ciphertext.len(),
            });
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&wrapped.ciphertext);
        Ok(Dek { bytes })
    }
    async fn check_access(&self, _: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(KmsAccessStatus::Ok)
    }
}

fn split_input(data: &[u8]) -> Option<(&[u8], &str, &str)> {
    // Layout: [t_len:1][b_len:1][tenant_bytes][blob_bytes][plaintext...]
    if data.len() < 2 {
        return None;
    }
    let t_len = usize::from(data[0]).min(64);
    let b_len = usize::from(data[1]).min(64);
    let need = 2 + t_len + b_len;
    if data.len() < need {
        return None;
    }
    let tenant = std::str::from_utf8(&data[2..2 + t_len]).ok()?;
    let blob = std::str::from_utf8(&data[2 + t_len..2 + t_len + b_len]).ok()?;
    let plaintext = &data[need..];
    Some((plaintext, tenant, blob))
}

fn key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:000000000000:key/fuzz".to_string(),
        region: "us-east-1".to_string(),
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((plaintext, tenant, blob_hash)) = split_input(data) else {
        return;
    };

    // Spin up a single-thread tokio runtime per call. The encryption
    // path is async but pure-CPU once the stub provider is in place.
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };

    rt.block_on(async move {
        let Ok(cache) = DekCache::new(300) else { return };
        let enc = EnvelopeEncryptor::new(StubKms, cache);
        let kid = key_id();

        // (1) Roundtrip — encrypt then decrypt with matching AAD.
        let blob: EncryptedBlob = match enc.encrypt(plaintext, &kid, tenant, blob_hash).await {
            Ok(b) => b,
            Err(_) => return,
        };
        let recovered = enc
            .decrypt(&blob, tenant, blob_hash)
            .await
            .expect("decrypt with correct AAD must succeed on a freshly-encrypted blob");
        assert_eq!(
            recovered.as_slice(),
            plaintext,
            "encrypt-then-decrypt roundtrip MUST yield identical plaintext"
        );

        // (2) AAD-mismatch path — flip the tenant_id; MUST return Err.
        // Use a fresh encryptor so the previous decrypt didn't poison
        // the cache for this wrapped DEK.
        let Ok(cache2) = DekCache::new(300) else { return };
        let enc2 = EnvelopeEncryptor::new(StubKms, cache2);
        let blob2 = match enc2.encrypt(plaintext, &kid, tenant, blob_hash).await {
            Ok(b) => b,
            Err(_) => return,
        };
        let mismatched_tenant = format!("X{tenant}");
        let attacker = enc2.decrypt(&blob2, &mismatched_tenant, blob_hash).await;
        assert!(
            attacker.is_err(),
            "cross-tenant AAD swap MUST be rejected — INV-BYOK-CRYPTO-SOVEREIGNTY \
             cross-blob-swap defense"
        );
    });
});
