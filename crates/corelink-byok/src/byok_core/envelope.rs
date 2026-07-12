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

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm,
};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use super::{
    context::CryptoContext,
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

    /// Encrypt `plaintext` for `(tenant_id, blob_hash)` under the customer CMK
    /// (Mode B — random DEK + random nonce). Thin back-compat wrapper over
    /// [`EnvelopeEncryptor::encrypt_with_ctx`] using a default
    /// [`CryptoContext`] (`CryptoContext::legacy`).
    pub async fn encrypt(
        &self,
        plaintext: &[u8],
        key_id: &KmsKeyId,
        tenant_id: &str,
        blob_hash: &str,
    ) -> Result<EncryptedBlob, BYOKError> {
        let ctx = CryptoContext::legacy(tenant_id, blob_hash, key_id.as_str());
        self.encrypt_with_ctx(plaintext, key_id, &ctx).await
    }

    /// Encrypt `plaintext` (Mode B — random DEK + random nonce) binding the
    /// FULL [`CryptoContext`] into the body AEAD AAD.
    ///
    /// # Security
    ///
    /// - Ephemeral DEK is generated fresh via CSPRNG for each call.
    /// - Nonce is 96-bit random.
    /// - **[H-2]** the body AEAD binds `aad = JCS(ctx)` (NOT nonce-only), so a
    ///   ciphertext cannot be relabelled / moved to a different context — any
    ///   divergent field fails the decrypt tag check.
    /// - The KMS `encryption_context` binds `{tenant_id, blob_hash}` so the
    ///   wrapped DEK cannot be swapped cross-blob.
    pub async fn encrypt_with_ctx(
        &self,
        plaintext: &[u8],
        key_id: &KmsKeyId,
        ctx: &CryptoContext,
    ) -> Result<EncryptedBlob, BYOKError> {
        // Step 1: generate ephemeral DEK (CSPRNG; NOT deterministic).
        let dek = generate_dek()?;

        // Step 2: AES-256-GCM encrypt body, binding aad = JCS(ctx) [H-2].
        let nonce_bytes = generate_nonce()?;
        let nonce = &nonce_bytes;
        let aad = ctx.to_jcs_bytes()?;
        let cipher = Aes256Gcm::new((&dek.bytes).into());
        let ciphertext = cipher
            .encrypt(
                nonce.into(),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|e| BYOKError::AesGcm(e.to_string()))?;

        // Step 3: wrap DEK via KMS with the string-map encryption_context.
        let kms_aad = ctx.kms_encryption_context();
        let wrapped_dek = self
            .provider
            .wrap_dek(&dek, key_id, Some(&kms_aad))
            .await
            .map_err(|e| {
                error!(provider = ?key_id.provider, error = %e, "wrap_dek failed");
                e
            })?;

        // NOTE [H-4]: the plaintext digest (ctx.plaintext_digest) is content
        // material and is intentionally NOT logged here.
        debug!(
            provider = ?key_id.provider,
            tenant_id = %ctx.tenant_id,
            namespace = %ctx.namespace,
            "envelope encrypt ok"
        );

        Ok(EncryptedBlob {
            wrapped_dek,
            ciphertext,
            nonce: nonce_bytes,
        })
    }

    /// Decrypt an [`EncryptedBlob`] for `(tenant_id, blob_hash)`. Thin
    /// back-compat wrapper over [`EnvelopeEncryptor::decrypt_with_ctx`].
    pub async fn decrypt(
        &self,
        blob: &EncryptedBlob,
        tenant_id: &str,
        blob_hash: &str,
    ) -> Result<Vec<u8>, BYOKError> {
        let ctx = CryptoContext::legacy(tenant_id, blob_hash, blob.wrapped_dek.key_id.as_str());
        self.decrypt_with_ctx(blob, &ctx).await
    }

    /// Decrypt an [`EncryptedBlob`] under an explicit [`CryptoContext`].
    ///
    /// # Cache behaviour & [H-1] fix
    ///
    /// The mandatory `encryption_context` presence + match check now runs
    /// **before** the cache lookup, so it is enforced on BOTH the cache-HIT
    /// and the cache-MISS arms (the pre-fix code skipped it on a hit). The
    /// DEK is cached for up to 5 min (TTL hard); the cache key is bound to the
    /// `(tenant, blob, context)` tuple so a poisoned context can never alias to
    /// another blob's DEK.
    ///
    /// # M-1 (zeroize)
    ///
    /// The returned `Vec<u8>` is plaintext and CALLER-OWNED — the caller must
    /// zeroize it after use. The internal DEK ([`Dek`]) is `ZeroizeOnDrop` and
    /// is cleared when this function returns.
    pub async fn decrypt_with_ctx(
        &self,
        blob: &EncryptedBlob,
        ctx: &CryptoContext,
    ) -> Result<Vec<u8>, BYOKError> {
        // [H-1] validate the encryption_context AAD on EVERY path + unwrap the
        // DEK (cache-first). Failing closed on absence/mismatch.
        let dek = self.unwrap_validated(&blob.wrapped_dek, ctx).await?;

        // [H-2]: AES-256-GCM decrypt with aad = JCS(ctx).
        let nonce = &blob.nonce;
        let aad = ctx.to_jcs_bytes()?;
        let cipher = Aes256Gcm::new((&dek.bytes).into());
        let plaintext = cipher
            .decrypt(
                nonce.into(),
                Payload {
                    msg: blob.ciphertext.as_ref(),
                    aad: &aad,
                },
            )
            .map_err(|e| {
                error!(error = %e, "AES-GCM decrypt failed");
                BYOKError::AesGcm(e.to_string())
            })?;

        debug!(
            provider = ?blob.wrapped_dek.provider,
            tenant_id = %ctx.tenant_id,
            namespace = %ctx.namespace,
            "envelope decrypt ok"
        );

        Ok(plaintext)
    }

    /// Validate the mandatory `encryption_context` AAD ([H-1]) and unwrap the
    /// DEK (DEK-cache first, KMS on miss). Shared by [`Self::decrypt_with_ctx`]
    /// and [`Self::encrypt_body_with_wrapped`] so the AAD-match + unwrap policy
    /// is single-sourced (the check runs on BOTH the cache-HIT and -MISS arms).
    ///
    /// # Errors
    ///
    /// - [`BYOKError::EncryptionContextMissing`] if the wrapped DEK carries no
    ///   `encryption_context`.
    /// - [`BYOKError::AadMismatch`] if it does not match `ctx`.
    /// - Any [`BYOKError`] from [`KmsProvider::unwrap_dek`] on a cache miss.
    async fn unwrap_validated(
        &self,
        wrapped: &WrappedDek,
        ctx: &CryptoContext,
    ) -> Result<Dek, BYOKError> {
        let stored_aad = wrapped.encryption_context.as_ref().ok_or_else(|| {
            warn!("encryption_context missing on wrapped DEK — rejecting");
            BYOKError::EncryptionContextMissing
        })?;
        let expected_aad = ctx.kms_encryption_context();
        if stored_aad != &expected_aad {
            warn!(
                tenant_id = %ctx.tenant_id,
                "AAD mismatch — cross-blob swap attempt rejected"
            );
            return Err(BYOKError::AadMismatch);
        }
        match self.dek_cache.get(wrapped).await {
            Some(cached) => {
                debug!(provider = ?wrapped.provider, "DEK cache hit");
                Ok(cached)
            }
            None => {
                debug!(provider = ?wrapped.provider, "DEK cache miss — unwrapping via KMS");
                let fresh = self.provider.unwrap_dek(wrapped).await.map_err(|e| {
                    error!(provider = ?wrapped.provider, error = %e, "unwrap_dek failed");
                    e
                })?;
                self.dek_cache
                    .put(wrapped, Dek { bytes: fresh.bytes })
                    .await?;
                Ok(fresh)
            }
        }
    }

    /// Re-encrypt `plaintext` under an EXISTING wrapped DEK + a FIXED nonce
    /// (Mode B idempotency, audit [C2]).
    ///
    /// The Mode-B write path persists one random `(DEK, nonce)` per blob in the
    /// `byok_envelope` D1 row, then derives the stored ciphertext by encrypting
    /// the body under THAT authoritative `(DEK, nonce)`. AES-256-GCM is
    /// deterministic given `(key, nonce, plaintext, aad)`, so every writer —
    /// including a loser of the INSERT-OR-IGNORE envelope race and any re-PUT —
    /// produces BYTE-IDENTICAL ciphertext. This is what makes the R2 PUT
    /// idempotent and prevents a ciphertext/envelope desync (a permanently
    /// undecryptable blob).
    ///
    /// Binds `aad = JCS(ctx)` and validates the wrapped DEK's
    /// `encryption_context` against `ctx` exactly as the read path does.
    ///
    /// # Errors
    ///
    /// - [`BYOKError::EncryptionContextMissing`] / [`BYOKError::AadMismatch`] /
    ///   unwrap failure (see [`Self::unwrap_validated`]).
    /// - [`BYOKError::AesGcm`] on AEAD failure; [`BYOKError::EnvelopeError`] on
    ///   JCS failure.
    pub async fn encrypt_body_with_wrapped(
        &self,
        plaintext: &[u8],
        wrapped: &WrappedDek,
        nonce: &[u8; 12],
        ctx: &CryptoContext,
    ) -> Result<Vec<u8>, BYOKError> {
        let dek = self.unwrap_validated(wrapped, ctx).await?;
        let aad = ctx.to_jcs_bytes()?;
        let cipher = Aes256Gcm::new((&dek.bytes).into());
        cipher
            .encrypt(
                nonce.into(),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|e| BYOKError::AesGcm(e.to_string()))
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::format_in_format_args
)]
mod tests {
    use super::super::types::{KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek};
    use super::*;
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

        let recovered = enc.decrypt(&blob, "tenant_1", "sha256:abc").await.unwrap();

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
        enc.decrypt(&blob, "tenant_cache", "hash:xyz")
            .await
            .unwrap();
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

    // ── Counting provider: distinguishes a cache HIT from a MISS ──────────

    struct CountingKmsProvider {
        unwraps: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl KmsProvider for CountingKmsProvider {
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
            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext: dek.bytes.to_vec(),
                encryption_context: encryption_context.cloned(),
            })
        }
        async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
            self.unwraps
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

    fn make_ctx(tenant: &str, digest: &str) -> CryptoContext {
        use super::super::context::{CryptoAlgo, CryptoMode};
        CryptoContext::new_single_shot(
            tenant,
            digest,
            CryptoAlgo::Aes256Gcm,
            "ns",
            CryptoMode::Random,
            "cas",
            "arn:aws:kms:us-east-1:123456789012:key/test-cmk",
            32,
        )
    }

    #[tokio::test]
    async fn test_body_aad_binding_rejects_tampered_nonctx_field() {
        // [H-2] A field NOT in the KMS encryption_context (namespace) is bound
        // only by the BODY AEAD. Tampering it must fail the decrypt tag check.
        let cache = DekCache::new(300).unwrap();
        let enc = EnvelopeEncryptor::new(StubKmsProvider, cache);
        let key_id = make_key_id();
        let ctx_a = make_ctx("tenant_a", "sha256:abc");

        let blob = enc
            .encrypt_with_ctx(b"top secret", &key_id, &ctx_a)
            .await
            .unwrap();

        // Each of these differs from ctx_a in exactly one body-only field.
        let mut variants = Vec::new();
        let mut v = ctx_a.clone();
        v.namespace = "evil".into();
        variants.push(v);
        let mut v = ctx_a.clone();
        v.version = 2;
        variants.push(v);
        let mut v = ctx_a.clone();
        v.surface = "ac".into();
        variants.push(v);
        let mut v = ctx_a.clone();
        v.mode = super::super::context::CryptoMode::Convergent;
        variants.push(v);
        let mut v = ctx_a.clone();
        v.key_id = "arn:aws:kms:us-east-1:123456789012:key/other".into();
        variants.push(v);

        for bad in variants {
            let r = enc.decrypt_with_ctx(&blob, &bad).await;
            assert!(
                matches!(r, Err(BYOKError::AesGcm(_))),
                "tampered body-AAD field must fail closed, got {:?}",
                r
            );
        }

        // tenant / digest live in the KMS context → caught earlier as AadMismatch.
        let mut bad_tenant = ctx_a.clone();
        bad_tenant.tenant_id = "tenant_b".into();
        assert!(matches!(
            enc.decrypt_with_ctx(&blob, &bad_tenant).await,
            Err(BYOKError::AadMismatch)
        ));
        let mut bad_digest = ctx_a.clone();
        bad_digest.plaintext_digest = "sha256:zzz".into();
        assert!(matches!(
            enc.decrypt_with_ctx(&blob, &bad_digest).await,
            Err(BYOKError::AadMismatch)
        ));

        // The correct context still round-trips.
        assert_eq!(
            enc.decrypt_with_ctx(&blob, &ctx_a).await.unwrap(),
            b"top secret"
        );
    }

    #[tokio::test]
    async fn test_cache_hit_same_dek_and_enforces_context() {
        // [H-1] Second decrypt is a HIT (no extra unwrap) and returns the same
        // plaintext; a poisoned context fails closed regardless of cache state.
        let unwraps = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cache = DekCache::new(300).unwrap();
        let enc = EnvelopeEncryptor::new(
            CountingKmsProvider {
                unwraps: std::sync::Arc::clone(&unwraps),
            },
            cache,
        );
        let key_id = make_key_id();
        let ctx = make_ctx("tenant_cache", "sha256:hit");

        let blob = enc
            .encrypt_with_ctx(b"cache me", &key_id, &ctx)
            .await
            .unwrap();

        let first = enc.decrypt_with_ctx(&blob, &ctx).await.unwrap();
        assert_eq!(unwraps.load(std::sync::atomic::Ordering::SeqCst), 1, "miss");
        let second = enc.decrypt_with_ctx(&blob, &ctx).await.unwrap();
        assert_eq!(
            unwraps.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "second decrypt must hit cache (no extra unwrap)"
        );
        assert_eq!(first, second);
        assert_eq!(first, b"cache me");

        // Poisoned context after the cache is warm → still fails closed.
        let mut poisoned = ctx.clone();
        poisoned.namespace = "poison".into();
        assert!(enc.decrypt_with_ctx(&blob, &poisoned).await.is_err());
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
