//! DEK cache with 5-minute TTL hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY).
//!
//! # Invariant
//!
//! The constructor rejects any TTL > 300 seconds.  There is no override,
//! advisory mode, or operator escape hatch.  This is the cornerstone of
//! the customer kill-switch SLA (≤ 6 min p99 global detection + eviction).
//!
//! # Eviction
//!
//! - TTL expiry: entry silently dropped on next access.
//! - CMK revocation: [`DekCache::evict_all_for_key`] atomically clears all
//!   entries for a given [`KmsKeyId`] and calls `ZeroizeOnDrop` on each
//!   evicted [`Dek`].
//! - Capacity: bounded LRU (max 10 000 entries per Worker).

use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::types::{BYOKError, Dek, KmsKeyId, WrappedDek};

/// Maximum DEK cache entries per Worker (memory bound).
const MAX_DEK_CACHE_ENTRIES: usize = 10_000;

/// Maximum TTL permitted — enforces INV-BYOK-CRYPTO-SOVEREIGNTY.
const MAX_TTL_SECONDS: u64 = 300;

/// Cache entry — DEK bytes + expiry timestamp.
struct Entry {
    dek: Dek,
    expires_at: Instant,
}

/// Thread-safe DEK cache with 5-minute TTL hard limit.
///
/// # Example
///
/// ```rust,no_run
/// use corelink_byok::DekCache;
///
/// # tokio_test::block_on(async {
/// let cache = DekCache::new(300).expect("TTL within bounds");
/// // TTL > 300 s is rejected:
/// assert!(DekCache::new(301).is_err());
/// # });
/// ```
#[derive(Debug)]
pub struct DekCache {
    inner: Arc<Mutex<LruCache<CacheKey, Entry>>>,
    ttl: Duration,
}

/// Cache key bound to the LOGICAL blob identity (audit fix **[H-1]**).
///
/// The pre-fix key was `(key_arn, ciphertext_len, ciphertext_prefix)` — it
/// omitted the tenant / blob binding, so two distinct blobs whose wrapped-DEK
/// ciphertexts collided on `(len, prefix)` could return EACH OTHER's cached
/// DEK with no AAD check. The key now includes
/// `(key_arn, tenant_id, blob_hash, enc_context_hash)` extracted from the
/// wrapped DEK's `encryption_context`, so a cached entry is bound to the exact
/// `(tenant, blob, context)` tuple. The ciphertext discriminators are retained
/// as an extra collision guard for entries that carry no encryption context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    key_arn: String,
    /// `encryption_context.tenant_id` (empty when no context).
    tenant_id: String,
    /// `encryption_context.blob_hash` (empty when no context).
    blob_hash: String,
    /// SHA-256 of the JCS-canonical `encryption_context` (all-zero when none).
    enc_context_hash: [u8; 32],
    /// Ciphertext length — extra discriminator.
    ciphertext_len: usize,
    /// First 16 bytes of ciphertext — extra discriminator (not secret).
    ciphertext_prefix: [u8; 16],
}

impl CacheKey {
    fn from_wrapped(wrapped: &WrappedDek) -> Self {
        let mut prefix = [0u8; 16];
        let src = &wrapped.ciphertext;
        let copy_len = src.len().min(16);
        // Safety: copy_len ≤ 16 ≤ prefix.len(); copy_len ≤ src.len().
        // Using get(..) to satisfy clippy::indexing_slicing.
        if let (Some(dst), Some(src_slice)) = (prefix.get_mut(..copy_len), src.get(..copy_len)) {
            dst.copy_from_slice(src_slice);
        }

        // Extract the (tenant, blob, context-hash) identity from the mandatory
        // encryption_context. Absent context → empty/zero (test-only path).
        let (tenant_id, blob_hash, enc_context_hash) = match &wrapped.encryption_context {
            Some(ctx) => {
                let tenant_id = ctx
                    .get("tenant_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let blob_hash = ctx
                    .get("blob_hash")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // JCS so the hash is stable regardless of key ordering.
                let jcs = serde_jcs::to_vec(ctx).unwrap_or_default();
                let digest: [u8; 32] = Sha256::digest(&jcs).into();
                (tenant_id, blob_hash, digest)
            }
            None => (String::new(), String::new(), [0u8; 32]),
        };

        Self {
            key_arn: wrapped.key_id.key_arn_or_id.clone(),
            tenant_id,
            blob_hash,
            enc_context_hash,
            ciphertext_len: src.len(),
            ciphertext_prefix: prefix,
        }
    }
}

impl DekCache {
    /// Create a new DEK cache.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::DekCacheTtlViolation`] if `ttl_seconds > 300`.
    /// This is an unrecoverable configuration error — INV-BYOK-CRYPTO-SOVEREIGNTY.
    pub fn new(ttl_seconds: u64) -> Result<Self, BYOKError> {
        if ttl_seconds > MAX_TTL_SECONDS {
            return Err(BYOKError::DekCacheTtlViolation {
                attempted_seconds: ttl_seconds,
            });
        }
        // MAX_DEK_CACHE_ENTRIES = 10_000 > 0; NonZeroUsize::new cannot return None here.
        // Use a match to satisfy clippy::expect_used.
        let cap = match std::num::NonZeroUsize::new(MAX_DEK_CACHE_ENTRIES) {
            Some(c) => c,
            None => {
                return Err(BYOKError::EnvelopeError(
                    "DEK cache capacity is zero — internal invariant broken".to_string(),
                ))
            }
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(LruCache::new(cap))),
            ttl: Duration::from_secs(ttl_seconds),
        })
    }

    /// Look up a cached DEK by wrapped DEK identity.
    ///
    /// Returns `None` on cache miss or TTL expiry.
    pub async fn get(&self, wrapped: &WrappedDek) -> Option<Dek> {
        let key = CacheKey::from_wrapped(wrapped);
        let mut guard = self.inner.lock().await;
        match guard.get(&key) {
            Some(entry) if entry.expires_at > Instant::now() => {
                debug!(
                    provider = ?wrapped.provider,
                    "DEK cache hit"
                );
                // Re-construct a fresh Dek from the cached bytes.
                Some(Dek {
                    bytes: entry.dek.bytes,
                })
            }
            Some(_expired) => {
                debug!(
                    provider = ?wrapped.provider,
                    "DEK cache expired — evicting"
                );
                guard.pop(&key);
                None
            }
            None => {
                debug!(provider = ?wrapped.provider, "DEK cache miss");
                None
            }
        }
    }

    /// Insert a freshly-unwrapped DEK into the cache.
    ///
    /// # Errors
    ///
    /// Currently infallible; error variant reserved for future bounded-memory errors.
    pub async fn put(&self, wrapped: &WrappedDek, dek: Dek) -> Result<(), BYOKError> {
        let key = CacheKey::from_wrapped(wrapped);
        let entry = Entry {
            dek,
            expires_at: Instant::now() + self.ttl,
        };
        let mut guard = self.inner.lock().await;
        guard.put(key, entry);
        Ok(())
    }

    /// Atomically evict all DEKs associated with a given KMS key (revocation path).
    ///
    /// Called by the KMS access-check background task (WI-S14-006) when
    /// `KmsAccessStatus::Revoked` is detected.  `ZeroizeOnDrop` is invoked
    /// automatically as each [`Dek`] is dropped.
    pub async fn evict_all_for_key(&self, key_id: &KmsKeyId) -> Result<usize, BYOKError> {
        let mut guard = self.inner.lock().await;
        // Collect keys to evict (borrow checker: collect first, then evict).
        let to_evict: Vec<CacheKey> = guard
            .iter()
            .filter(|(k, _)| k.key_arn == key_id.key_arn_or_id)
            .map(|(k, _)| k.clone())
            .collect();

        let count = to_evict.len();
        for k in to_evict {
            guard.pop(&k); // Entry.dek is dropped here → ZeroizeOnDrop fires.
        }
        warn!(
            key_arn = %key_id.key_arn_or_id,
            evicted = count,
            "DEK cache evicted on CMK revocation"
        );
        Ok(count)
    }

    /// Current number of live entries (for metrics / tests).
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Returns `true` if the cache contains no entries.
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
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
    use super::*;

    #[tokio::test]
    async fn test_ttl_violation_rejects_above_300() {
        assert!(DekCache::new(301).is_err());
        assert!(DekCache::new(u64::MAX).is_err());
    }

    #[tokio::test]
    async fn test_ttl_exactly_300_ok() {
        assert!(DekCache::new(300).is_ok());
        assert!(DekCache::new(0).is_ok());
    }

    fn make_wrapped() -> WrappedDek {
        use super::super::types::{KmsKeyId, KmsProviderKind};
        WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: KmsKeyId {
                provider: KmsProviderKind::AwsKms,
                key_arn_or_id: "arn:aws:kms:us-east-1:123:key/test-key".to_string(),
                region: "us-east-1".to_string(),
            },
            ciphertext: vec![0u8; 64],
            encryption_context: None,
        }
    }

    #[tokio::test]
    async fn test_cache_miss_returns_none() {
        let cache = DekCache::new(300).unwrap();
        let wrapped = make_wrapped();
        assert!(cache.get(&wrapped).await.is_none());
    }

    #[tokio::test]
    async fn test_put_then_get_returns_dek() {
        let cache = DekCache::new(300).unwrap();
        let wrapped = make_wrapped();
        let dek = Dek { bytes: [42u8; 32] };
        cache.put(&wrapped, dek).await.unwrap();
        let fetched = cache.get(&wrapped).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().bytes, [42u8; 32]);
    }

    #[tokio::test]
    async fn test_cache_key_binds_encryption_context() {
        // [H-1] Two wrapped DEKs with IDENTICAL ciphertext but DIFFERENT
        // encryption_context must NOT alias in the cache — the pre-fix key
        // (key_arn, len, prefix) would have collided and returned the wrong DEK.
        use super::super::types::{KmsKeyId, KmsProviderKind};
        let cache = DekCache::new(300).unwrap();
        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:us-east-1:123:key/test-key".to_string(),
            region: "us-east-1".to_string(),
        };
        let mut a = WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext: vec![7u8; 40],
            encryption_context: Some(serde_json::json!({
                "tenant_id": "tenant_a", "blob_hash": "sha256:abc"
            })),
        };
        cache.put(&a, Dek { bytes: [1u8; 32] }).await.unwrap();

        // Same ciphertext, different tenant context → must be a MISS.
        a.encryption_context = Some(serde_json::json!({
            "tenant_id": "tenant_b", "blob_hash": "sha256:abc"
        }));
        assert!(
            cache.get(&a).await.is_none(),
            "different encryption_context must not alias to another blob's DEK"
        );

        // Original context still hits with the original DEK.
        a.encryption_context = Some(serde_json::json!({
            "tenant_id": "tenant_a", "blob_hash": "sha256:abc"
        }));
        assert_eq!(cache.get(&a).await.unwrap().bytes, [1u8; 32]);
    }

    #[tokio::test]
    async fn test_evict_all_for_key_clears_entries() {
        use super::super::types::{KmsKeyId, KmsProviderKind};
        let cache = DekCache::new(300).unwrap();
        let wrapped = make_wrapped();
        let dek = Dek { bytes: [1u8; 32] };
        cache.put(&wrapped, dek).await.unwrap();
        assert_eq!(cache.len().await, 1);

        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:us-east-1:123:key/test-key".to_string(),
            region: "us-east-1".to_string(),
        };
        let evicted = cache.evict_all_for_key(&key_id).await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(cache.len().await, 0);
        // Confirm subsequent get returns None.
        assert!(cache.get(&wrapped).await.is_none());
    }
}
