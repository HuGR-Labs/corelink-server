//! Canonical R2 key construction.
//!
//! Single source of truth for the R2 key layout described in
//! `remote_cache_product_profile.md §7.1` (REG-NAMESPACE-001..005) and
//! `storage_semantics_matrix.md §3.9`:
//!
//! ```text
//! cas-<region>/<HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>
//! ```
//!
//! - `<region>` ∈ {wnam, weur, sam} — see [`crate::Region::bucket_suffix`].
//! - `<HMAC16>` is the 16-char URL-safe-base64 HMAC prefix from
//!   [`corelink_tenant_path::derive_prefix`].
//! - `<hex[0:2]>` / `<hex[2:4]>` are the canonical 2-level shard prefixes
//!   (REG-NAMESPACE-005); 65 536 sub-prefixes spread LIST/PUT load.
//! - `<full-hex>` is the 64-char lowercase hex encoding of the BLAKE3 digest.
//!
//! The bucket name itself (`cas-<region>`) is **embedded in the key** in
//! addition to being the bucket binding. This is deliberate: it makes the
//! key self-describing in offline forensics (R2 list dumps, audit logs) and
//! catches a class of binding-misconfiguration bugs where a wrong-region
//! writer would otherwise silently land blobs in the wrong bucket — the
//! emitted full key would carry a `cas-weur/...` prefix into a `cas-wnam`
//! bucket, an inconsistency easily caught by any subsequent integrity scan.
//!
//! `canonical_key` is `pub(crate)`: outside callers must go through a writer
//! / reader, never construct a key by hand. This is REG-NAMESPACE-002 ("no
//! code path may construct a key without going through `derive_prefix`")
//! enforced at the module-visibility level.

use corelink_hash::Digest;

use crate::Region;
use corelink_tenant_path::TenantPrefix;

/// Build the canonical R2 key for `(region, prefix, digest)`.
///
/// Format: `cas-<region>/<HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>`.
///
/// All inputs are non-secret and have been validated by their newtype
/// constructors:
/// - [`Region`] is an enum, so `bucket_suffix` is a static string.
/// - [`TenantPrefix`] is exactly 16 ASCII chars (URL-safe base64) by
///   construction.
/// - [`Digest`] is exactly 32 bytes; `to_hex` returns 64 lowercase hex chars.
///
/// Returns a fresh `String` whose length is deterministic. With a 3-char
/// suffix (`sam`) the key is exactly 102 bytes; with a 4-char suffix
/// (`wnam` / `weur`) the key is exactly 103 bytes
/// (`"cas-".len() + suffix.len() + 1 + 16 + "/blake3/".len() + 2 + 1 + 2 + 1 + 64`
/// = 4 + {3,4} + 1 + 16 + 8 + 2 + 1 + 2 + 1 + 64 = {102, 103}).
#[must_use]
pub(crate) fn canonical_key(region: Region, prefix: &TenantPrefix, digest: &Digest) -> String {
    let hex = digest.to_hex();
    // SAFETY-by-construction: `hex` is exactly 64 ASCII characters
    // (`Digest::to_hex` returns the lowercase encoding of the 32-byte digest;
    // see `corelink_hash::Digest::to_hex`). `split_at(2)` and `split_at(4)`
    // therefore land on character boundaries.
    let s0 = hex.get(0..2).unwrap_or("");
    let s1 = hex.get(2..4).unwrap_or("");
    debug_assert_eq!(s0.len(), 2, "Digest::to_hex must return ≥ 4 chars");
    debug_assert_eq!(s1.len(), 2, "Digest::to_hex must return ≥ 4 chars");
    debug_assert_eq!(hex.len(), 64, "BLAKE3 hex must be 64 chars");
    format!(
        "{bucket}/{prefix}/blake3/{s0}/{s1}/{hex}",
        bucket = region.cas_bucket_name(),
        prefix = prefix.as_str(),
    )
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use corelink_hash::Digest;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    #[test]
    fn shape_matches_canonical_grammar() {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        let tid = Uuid::nil();
        let prefix = derive_prefix(&tdk, tid);
        let digest = Digest::compute(b"x");
        let key = canonical_key(Region::Wnam, &prefix, &digest);

        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 6, "exactly 6 path segments");
        assert_eq!(parts.first().copied(), Some("cas-wnam"));
        assert_eq!(parts.get(1).map(|s| s.len()), Some(16));
        assert_eq!(parts.get(2).copied(), Some("blake3"));
        assert_eq!(parts.get(3).map(|s| s.len()), Some(2));
        assert_eq!(parts.get(4).map(|s| s.len()), Some(2));
        assert_eq!(parts.get(5).map(|s| s.len()), Some(64));
        let hex = digest.to_hex();
        assert_eq!(
            parts.get(3).copied().unwrap_or(""),
            hex.get(0..2).unwrap_or("")
        );
        assert_eq!(
            parts.get(4).copied().unwrap_or(""),
            hex.get(2..4).unwrap_or("")
        );
        assert_eq!(parts.get(5).copied().unwrap_or(""), &hex);
    }

    #[test]
    fn bucket_segment_includes_region() {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        let prefix = derive_prefix(&tdk, Uuid::nil());
        let digest = Digest::compute(b"x");
        for region in [Region::Wnam, Region::Weur, Region::Sam] {
            let key = canonical_key(region, &prefix, &digest);
            assert!(
                key.starts_with(region.cas_bucket_name()),
                "key {key:?} did not start with {}",
                region.cas_bucket_name(),
            );
            assert!(
                key.starts_with(&format!("{}/", region.cas_bucket_name())),
                "missing slash after bucket name in {key:?}",
            );
        }
    }
}
