//! Envelope encryption — write and read paths for BYOK blobs.
//!
//! ## Write path
//!
//! 1. Generate ephemeral DEK: 32 random bytes via `getrandom::getrandom`
//!    (OS-backed CSPRNG; NIST SP 800-90A DRBG).  **NOT** KDF-derived deterministic.
//! 2. Encrypt body with AES-256-GCM (FIPS 197); nonce = 96-bit random per-write.
//! 3. Wrap DEK via [`KmsProvider::wrap_dek`] with mandatory AAD
//!    `{"tenant_id": "...", "blob_hash": "..."}`.
//! 4. Return [`EncryptedBlob`] — caller stores `wrapped_dek` in D1 and `ciphertext` in R2.
//!
//! ## Read path
//!
//! 1. Accept [`EncryptedBlob`] (ciphertext from R2 + wrapped DEK from D1).
//! 2. Check [`DekCache`] (5 min TTL hard); on hit, skip KMS call.
//! 3. On miss: call [`KmsProvider::unwrap_dek`]; cache result.
//! 4. Decrypt body with AES-256-GCM.

#![forbid(unsafe_code)]

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use crate::{
    dek_cache::DekCache,
    types::{BYOKError, Dek, KmsKeyId, WrappedDek},
    KmsProvider,
};

/// Output of the write path — stored split across D1 (wrapped DEK) and R2 (ciphertext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    /// KMS-wrapped DEK — stored in D1 `byok_envelope`.
    pub wrapped_dek: WrappedDek,
    /// AES-256-GCM ciphertext — stored in R2.
    pub ciphertext: Vec<u8>,
    /// 96-bit nonce — stored alongside ciphertext (not secret; needed for decrypt).
    pub nonce: [u8; 12],
}

/// Orchestrates envelope encryption for a single BYOK tenant.
#[derive(Debug)]
pub struct EnvelopeEncryptor<P: KmsProvider> {
    provider: P,
    dek_cache: DekCache,
}

impl<P: KmsProvider> EnvelopeEncryptor<P> {
    /// Construct with a KMS provider and a DEK cache (TTL ≤ 300 s enforced by [`DekCache`]).
    pub fn new(provider: P, dek_cache: DekCache) -> Self {
        Self {
            provider,
            dek_cache,
        }
    }

    /// Encrypt `plaintext` for `(tenant_id, blob_hash)` under the customer CMK.
    ///
    /// # Security
    ///
    /// - Ephemeral DEK is generated fresh via CSPRNG for each call.
    /// - Nonce is 96-bit random.
    /// - AAD binding prevents cross-blob wrapped-DEK swap attacks.
    pub async fn encrypt(
        &self,
        plaintext: &[u8],
        key_id: &KmsKeyId,
        tenant_id: &str,
        blob_hash: &str,
    ) -> Result<EncryptedBlob, BYOKError> {
        // Step 1: generate ephemeral DEK (CSPRNG; NOT deterministic).
        let dek = generate_dek()?;

        // Step 2: AES-256-GCM encrypt body with random 96-bit nonce.
        let nonce_bytes = generate_nonce()?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek.bytes));
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| BYOKError::AesGcm(e.to_string()))?;

        // Step 3: build mandatory AAD.
        let aad = build_aad(tenant_id, blob_hash);

        // Step 4: wrap DEK via KMS.
        let wrapped_dek = self
            .provider
            .wrap_dek(&dek, key_id, Some(&aad))
            .await
            .map_err(|e| {
                error!(provider = ?key_id.provider, error = %e, "wrap_dek failed");
                e
            })?;

        debug!(
            provider = ?key_id.provider,
            tenant_id = tenant_id,
            blob_hash = blob_hash,
            "envelope encrypt ok"
        );

        Ok(EncryptedBlob {
            wrapped_dek,
            ciphertext,
            nonce: nonce_bytes,
        })
    }

    /// Decrypt an [`EncryptedBlob`] using the cached or freshly-unwrapped DEK.
    ///
    /// # Cache behaviour
    ///
    /// DEK is cached for up to 5 min (TTL hard).  On cache miss a KMS network
    /// call is made (p99 ≤ 30 ms region-co-located).
    pub async fn decrypt(
        &self,
        blob: &EncryptedBlob,
        tenant_id: &str,
        blob_hash: &str,
    ) -> Result<Vec<u8>, BYOKError> {
        // Step 1: check DEK cache.
        let dek = match self.dek_cache.get(&blob.wrapped_dek).await {
            Some(cached) => {
                debug!(provider = ?blob.wrapped_dek.provider, "DEK cache hit");
                cached
            }
            None => {
                debug!(provider = ?blob.wrapped_dek.provider, "DEK cache miss — unwrapping via KMS");

                // Validate AAD field present.
                if blob.wrapped_dek.encryption_context.is_none() {
                    warn!("encryption_context missing on wrapped DEK — rejecting");
                    return Err(BYOKError::EncryptionContextMissing);
                }

                // Verify AAD matches expected tenant/blob binding.
                let expected_aad = build_aad(tenant_id, blob_hash);
                let stored_aad = match blob.wrapped_dek.encryption_context.as_ref() {
                    Some(v) => v,
                    None => {
                        warn!("encryption_context None after presence check — logic error");
                        return Err(BYOKError::EncryptionContextMissing);
                    }
                };
                if stored_aad != &expected_aad {
                    warn!(
                        tenant_id = tenant_id,
                        blob_hash = blob_hash,
                        "AAD mismatch — cross-blob swap attempt rejected"
                    );
                    return Err(BYOKError::AadMismatch);
                }

                // Step 2: KMS unwrap.
                let fresh_dek = self
                    .provider
                    .unwrap_dek(&blob.wrapped_dek)
                    .await
                    .map_err(|e| {
                        error!(provider = ?blob.wrapped_dek.provider, error = %e, "unwrap_dek failed");
                        e
                    })?;

                // Step 3: cache for TTL window.
                let dek_for_cache = Dek {
                    bytes: fresh_dek.bytes,
                };
                self.dek_cache
                    .put(&blob.wrapped_dek, dek_for_cache)
                    .await?;

                fresh_dek
            }
        };

        // Step 4: AES-256-GCM decrypt.
        let nonce = Nonce::from_slice(&blob.nonce);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek.bytes));
        let plaintext = cipher
            .decrypt(nonce, blob.ciphertext.as_ref())
            .map_err(|e| {
                error!(error = %e, "AES-GCM decrypt failed");
                BYOKError::AesGcm(e.to_string())
            })?;

        debug!(
            provider = ?blob.wrapped_dek.provider,
            tenant_id = tenant_id,
            blob_hash = blob_hash,
            "envelope decrypt ok"
        );

        Ok(plaintext)
    }
}

/// Generate a fresh 32-byte DEK via OS CSPRNG.
///
/// NIST SP 800-90A approved DRBG (OS-backed; not deterministic).
fn generate_dek() -> Result<Dek, BYOKError> {
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes).map_err(|e| BYOKError::EnvelopeError(format!("getrandom: {e}")))?;
    Ok(Dek { bytes })
}

/// Generate a fresh 96-bit (12-byte) random nonce for AES-256-GCM.
fn generate_nonce() -> Result<[u8; 12], BYOKError> {
    let mut nonce = [0u8; 12];
    getrandom(&mut nonce).map_err(|e| BYOKError::EnvelopeError(format!("getrandom nonce: {e}")))?;
    Ok(nonce)
}

/// Build the mandatory AAD `{"tenant_id": "...", "blob_hash": "..."}`.
fn build_aad(tenant_id: &str, blob_hash: &str) -> serde_json::Value {
    serde_json::json!({
        "tenant_id": tenant_id,
        "blob_hash": blob_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek};
    use async_trait::async_trait;

    // ── Stub KMS provider for unit tests ──────────────────────────────────────

    struct StubKmsProvider;

    #[async_trait]
    impl KmsProvider for StubKmsProvider {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::AwsKms
        }
        fn region(&self) -> &str {
            "us-east-1"
        }
        fn fips_level(&self) -> crate::types::FipsLevel {
            crate::types::FipsLevel::Fips140_3_L1
        }

        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            encryption_context: Option<&serde_json::Value>,
        ) -> Result<WrappedDek, BYOKError> {
            // Stub: store DEK bytes as "ciphertext" (unsafe in prod; fine for tests).
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

        async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            Ok(KmsAccessStatus::Ok)
        }
    }

    fn make_key_id() -> KmsKeyId {
        KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:us-east-1:123456789012:key/test-cmk".to_string(),
            region: "us-east-1".to_string(),
        }
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let cache = DekCache::new(300).unwrap();
        let enc = EnvelopeEncryptor::new(StubKmsProvider, cache);
        let key_id = make_key_id();
        let plaintext = b"hello byok world";

        let blob = enc
            .encrypt(plaintext, &key_id, "tenant_1", "sha256:abc")
            .await
            .unwrap();

        let recovered = enc
            .decrypt(&blob, "tenant_1", "sha256:abc")
            .await
            .unwrap();

        assert_eq!(recovered, plaintext);
    }

    #[tokio::test]
    async fn test_second_decrypt_uses_cache() {
        let cache = DekCache::new(300).unwrap();
        let enc = EnvelopeEncryptor::new(StubKmsProvider, cache);
        let key_id = make_key_id();
        let plaintext = b"cache test";

        let blob = enc
            .encrypt(plaintext, &key_id, "tenant_cache", "hash:xyz")
            .await
            .unwrap();

        // First decrypt — populates cache.
        enc.decrypt(&blob, "tenant_cache", "hash:xyz").await.unwrap();
        // Second decrypt — should hit cache (no KMS call).
        let recovered = enc
            .decrypt(&blob, "tenant_cache", "hash:xyz")
            .await
            .unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[tokio::test]
    async fn test_aad_mismatch_rejects() {
        let cache = DekCache::new(300).unwrap();
        let enc = EnvelopeEncryptor::new(StubKmsProvider, cache);
        let key_id = make_key_id();
        let plaintext = b"aad test";

        let blob = enc
            .encrypt(plaintext, &key_id, "tenant_a", "hash:correct")
            .await
            .unwrap();

        // Attempt decrypt with wrong tenant — should fail.
        let result = enc.decrypt(&blob, "tenant_b", "hash:correct").await;
        assert!(
            matches!(result, Err(BYOKError::AadMismatch)),
            "expected AadMismatch, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_nonce_uniqueness_sample() {
        // Ensure 1000 consecutive nonces are all distinct.
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..1000 {
            let n = generate_nonce().unwrap();
            assert!(nonces.insert(n), "nonce collision detected");
        }
    }

    #[tokio::test]
    async fn test_dek_uniqueness_sample() {
        // Ensure 1000 consecutive DEKs are all distinct.
        let mut deks: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
        for _ in 0..1000 {
            let d = generate_dek().unwrap();
            assert!(deks.insert(d.bytes), "DEK collision detected");
        }
    }
}
