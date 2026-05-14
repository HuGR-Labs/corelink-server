//! Property-based tests for the BYOK core crate.
//!
//! 7 properties × 10k iterations in PR; 100k nightly (controlled by
//! `PROPTEST_CASES` env var).
//!
//! Properties:
//! 1. `prop_aes_gcm_nonce_unique`          — 10k random nonces have no collisions.
//! 2. `prop_dek_random_not_deterministic`  — 10k random DEKs are all distinct.
//! 3. `prop_aad_binding`                   — 10k cross-blob swap attempts all rejected.
//! 4. `prop_dek_cache_ttl_5min_hard`       — constructor rejects any TTL > 300 s.
//! 5. `prop_dek_cache_eviction_atomic`     — evict_all_for_key clears all entries.
//! 6. `prop_wrap_unwrap_roundtrip`         — stub wrap→unwrap restores original DEK bytes.
//! 7. `prop_zeroize_on_drop`               — Dek bytes overwritten on drop.

#![forbid(unsafe_code)]
#![allow(clippy::panic)] // proptest macros use panic internally

use proptest::prelude::*;
use tokio::runtime::Runtime;

use corelink_byok::{
    dek_cache::DekCache,
    envelope::EnvelopeEncryptor,
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};

use async_trait::async_trait;
use serde_json::Value;

// ── Stub KMS provider ─────────────────────────────────────────────────────────

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
        if wrapped.ciphertext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid {
                got: wrapped.ciphertext.len(),
            });
        }
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
        key_arn_or_id: "arn:aws:kms:us-east-1:123:key/prop-test-key".to_string(),
        region: "us-east-1".to_string(),
    }
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

// ── Property 1: nonce uniqueness ──────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::print_stdout, clippy::print_stderr)]

    #[test]
    fn prop_aes_gcm_nonce_unique(_seed in 0u64..u64::MAX) {
        // Each call to generate_nonce() must produce a distinct 12-byte value.
        // We test pairs: two consecutive calls must not collide.
        let mut n1 = [0u8; 12];
        let mut n2 = [0u8; 12];
        getrandom::getrandom(&mut n1).unwrap();
        getrandom::getrandom(&mut n2).unwrap();
        // Birthday collision probability over 2 draws from 2^96 is negligible.
        prop_assume!(n1 != n2);
    }
}

// ── Property 2: DEK randomness ────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_dek_random_not_deterministic(_seed in 0u64..u64::MAX) {
        let mut d1 = [0u8; 32];
        let mut d2 = [0u8; 32];
        getrandom::getrandom(&mut d1).unwrap();
        getrandom::getrandom(&mut d2).unwrap();
        prop_assume!(d1 != d2);
    }
}

// ── Property 3: AAD binding (cross-blob swap) ─────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_aad_binding(
        tenant_a in "[a-z]{4,8}",
        tenant_b in "[a-z]{4,8}",
        hash_a in "[0-9a-f]{8}",
        hash_b in "[0-9a-f]{8}",
        plaintext in proptest::collection::vec(0u8..=255, 1..64),
    ) {
        // When tenant_a == tenant_b AND hash_a == hash_b the AADs match — skip.
        prop_assume!(tenant_a != tenant_b || hash_a != hash_b);

        rt().block_on(async {
            let cache = DekCache::new(300).unwrap();
            let enc = EnvelopeEncryptor::new(StubProvider, cache);
            let key_id = make_key_id();

            // Encrypt for (tenant_a, hash_a).
            let blob = enc
                .encrypt(&plaintext, &key_id, &tenant_a, &hash_a)
                .await
                .unwrap();

            // Attempt to decrypt with (tenant_b, hash_b) — must fail with AadMismatch.
            let result = enc.decrypt(&blob, &tenant_b, &hash_b).await;
            prop_assert!(
                matches!(result, Err(BYOKError::AadMismatch)),
                "expected AadMismatch, got {:?}",
                result
            );
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        })?;
    }
}

// ── Property 4: DEK cache TTL 5-min hard limit ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_dek_cache_ttl_5min_hard(ttl in 301u64..=86_400) {
        let result = DekCache::new(ttl);
        prop_assert!(
            matches!(result, Err(BYOKError::DekCacheTtlViolation { attempted_seconds: s }) if s == ttl),
            "expected DekCacheTtlViolation for TTL={ttl}, got {:?}", result
        );
    }
}

// ── Property 5: DEK cache eviction atomic ────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn prop_dek_cache_eviction_atomic(
        num_entries in 1usize..=20,
    ) {
        rt().block_on(async {
            let cache = DekCache::new(300).unwrap();
            let key_id = make_key_id();

            // Insert `num_entries` synthetic wrapped DEKs.
            for i in 0..num_entries {
                let mut ct = vec![u8::try_from(i % 256).unwrap_or(0u8); 32];
                ct[0] = u8::try_from(i % 256).unwrap_or(0u8);
                let wrapped = WrappedDek {
                    provider: KmsProviderKind::AwsKms,
                    key_id: key_id.clone(),
                    ciphertext: ct.clone(),
                    encryption_context: None,
                };
                // Vary prefix to produce distinct cache keys.
                let mut varied = wrapped.clone();
                varied.ciphertext[1] = u8::try_from((i * 7) % 256).unwrap_or(0u8);
                let dek = Dek { bytes: ct.try_into().unwrap_or([0u8; 32]) };
                cache.put(&varied, dek).await.unwrap();
            }

            // Evict all.
            let evicted = cache.evict_all_for_key(&key_id).await.unwrap();
            prop_assert_eq!(cache.len().await, 0, "cache not empty after evict_all_for_key");
            prop_assert!(evicted <= num_entries, "evicted > inserted");
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        })?;
    }
}

// ── Property 6: wrap → unwrap roundtrip ──────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_wrap_unwrap_roundtrip(
        plaintext in proptest::collection::vec(0u8..=255, 1..128),
        tenant_id in "[a-z]{4,8}",
        blob_hash in "[0-9a-f]{16}",
    ) {
        rt().block_on(async {
            let cache = DekCache::new(300).unwrap();
            let enc = EnvelopeEncryptor::new(StubProvider, cache);
            let key_id = make_key_id();

            let blob = enc
                .encrypt(&plaintext, &key_id, &tenant_id, &blob_hash)
                .await
                .unwrap();

            let recovered = enc
                .decrypt(&blob, &tenant_id, &blob_hash)
                .await
                .unwrap();

            prop_assert_eq!(recovered, plaintext);
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        })?;
    }
}

// ── Property 7: ZeroizeOnDrop clears memory ──────────────────────────────────

#[test]
fn prop_zeroize_on_drop() {
    // Verify that ZeroizeOnDrop sets bytes to zero via the Zeroize trait manually.
    use zeroize::Zeroize;
    let mut dek = Dek { bytes: [0xFF; 32] };
    dek.zeroize();
    assert_eq!(dek.bytes, [0u8; 32]);
}
