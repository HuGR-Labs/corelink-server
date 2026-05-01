//! Canonical object-key constructor — 5-Layer Defense Layer 4
//! (`INV-MULTIPART-PATH-TENANT-SCOPED`).
//!
//! Every R2 object owned by the multipart adapter lives at a path of
//! the canonical form
//!
//! ```text
//! <family>-<region>/<tenant_prefix>/<digest_hex>[<.suffix>]
//! ```
//!
//! where:
//! - `<family>` is `chunk` or `manifest` (encoded by [`Bucket`]);
//! - `<region>` is the lower-case ASCII residency suffix supplied by
//!   the caller (the worker's `Region::bucket_suffix()` output —
//!   intentionally NOT typed against `Region` here so the crate
//!   stays wasm32-clean and free of the worker dep cycle);
//! - `<tenant_prefix>` is the [`TenantPrefix`] HMAC16 from
//!   `corelink-tenant-path` (16 lower-case URL-safe-base64 characters
//!   — verified at construction in `corelink-tenant-path`);
//! - `<digest_hex>` is a 64-char lower-case hex BLAKE3 digest;
//! - `<.suffix>` is `.json` for manifest envelopes (handler
//!   convention), absent for chunk content.
//!
//! ## Why structurally enforced
//!
//! The composer takes typed inputs only — there is no way for a
//! caller to inject a `..` or other directory-traversal byte
//! through a tenant-controlled string, because (a) [`TenantPrefix`]
//! validates its alphabet at construction, (b) the digest is
//! validated to be 64-char lower-case hex, (c) the region literal
//! is validated to match `[a-z0-9_-]{1,16}`. Any violation surfaces
//! as [`crate::MultipartError::InvalidObjectKey`].

use corelink_tenant_path::TenantPrefix;

use crate::error::MultipartError;
use crate::types::Bucket;

/// Maximum permitted region literal length (paranoia bound — the
/// canonical Cloudflare suffixes are ≤ 4 chars).
const MAX_REGION_LITERAL_LEN: usize = 16;

/// Compose the canonical multipart object key.
///
/// # Errors
///
/// Returns [`MultipartError::InvalidObjectKey`] if any input fails
/// the structural validation (`region` outside the canonical
/// alphabet, `digest_hex` not 64-char lower hex, optional
/// `suffix` containing a path-separator byte).
///
/// # Notes
///
/// - The 5-Layer Defense path-key invariant
///   `INV-MULTIPART-PATH-TENANT-SCOPED` is captured structurally:
///   the `tenant_prefix` segment is sandwiched between the bucket
///   family and the content-addressable suffix; no caller-controlled
///   string can move it.
pub fn compose(
    bucket: Bucket,
    region: &str,
    tenant_prefix: &TenantPrefix,
    digest_hex: &str,
    suffix: Option<&str>,
) -> Result<String, MultipartError> {
    validate_region(region)?;
    validate_digest_hex(digest_hex)?;
    if let Some(s) = suffix {
        validate_suffix(s)?;
    }

    let family = bucket.family_prefix();
    let prefix_str = tenant_prefix.as_str();
    let suffix_str = suffix.unwrap_or("");

    let mut out = String::with_capacity(
        family.len() + region.len() + 1 + prefix_str.len() + 1 + digest_hex.len() + suffix_str.len(),
    );
    out.push_str(family);
    out.push_str(region);
    out.push('/');
    out.push_str(prefix_str);
    out.push('/');
    out.push_str(digest_hex);
    out.push_str(suffix_str);
    Ok(out)
}

fn validate_region(region: &str) -> Result<(), MultipartError> {
    if region.is_empty() || region.len() > MAX_REGION_LITERAL_LEN {
        return Err(MultipartError::InvalidObjectKey {
            reason: format!("region literal length {} out of range", region.len()),
        });
    }
    if !region
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(MultipartError::InvalidObjectKey {
            reason: format!("region literal `{region}` contains non-canonical bytes"),
        });
    }
    Ok(())
}

fn validate_digest_hex(digest_hex: &str) -> Result<(), MultipartError> {
    if digest_hex.len() != 64 {
        return Err(MultipartError::InvalidObjectKey {
            reason: format!("digest_hex length {} ≠ 64", digest_hex.len()),
        });
    }
    if !digest_hex
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MultipartError::InvalidObjectKey {
            reason: "digest_hex contains non-lowercase-hex bytes".to_string(),
        });
    }
    Ok(())
}

fn validate_suffix(suffix: &str) -> Result<(), MultipartError> {
    if suffix.is_empty() {
        return Ok(());
    }
    if !suffix.starts_with('.') {
        return Err(MultipartError::InvalidObjectKey {
            reason: "suffix must start with `.`".to_string(),
        });
    }
    if suffix.contains('/') || suffix.contains('\\') {
        return Err(MultipartError::InvalidObjectKey {
            reason: "suffix contains path separator".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    fn dummy_digest() -> String {
        "0123456789abcdef".repeat(4)
    }

    #[test]
    fn compose_canonical_chunk_path() {
        let p = fixed_prefix(Uuid::nil());
        let dh = dummy_digest();
        let key = compose(Bucket::Chunk, "sam", &p, &dh, None).unwrap();
        assert!(key.starts_with("chunk-sam/"));
        assert!(key.ends_with(&dh));
        // Tenant prefix segment is structurally between the family
        // and the digest — slice the path and verify.
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "chunk-sam");
        assert_eq!(parts[1], p.as_str());
        assert_eq!(parts[2], dh);
    }

    #[test]
    fn compose_manifest_with_json_suffix() {
        let p = fixed_prefix(Uuid::from_u128(7));
        let dh = dummy_digest();
        let key = compose(Bucket::Manifest, "iad", &p, &dh, Some(".json")).unwrap();
        assert!(key.starts_with("manifest-iad/"));
        assert!(key.ends_with(".json"));
    }

    #[test]
    fn compose_rejects_bad_region() {
        let p = fixed_prefix(Uuid::nil());
        let dh = dummy_digest();
        for bad in ["", "SAM", "sam!", &"x".repeat(20), "sam/extra"] {
            let r = compose(Bucket::Chunk, bad, &p, &dh, None);
            assert!(matches!(r, Err(MultipartError::InvalidObjectKey { .. })), "bad={bad}");
        }
    }

    #[test]
    fn compose_rejects_bad_digest_hex() {
        let p = fixed_prefix(Uuid::nil());
        for bad in ["", "0123", &"g".repeat(64), &"0".repeat(63)] {
            let r = compose(Bucket::Chunk, "sam", &p, bad, None);
            assert!(matches!(r, Err(MultipartError::InvalidObjectKey { .. })), "bad={bad}");
        }
    }

    #[test]
    fn compose_rejects_bad_suffix() {
        let p = fixed_prefix(Uuid::nil());
        let dh = dummy_digest();
        for bad in ["json", ".js/on", ".bad\\path"] {
            let r = compose(Bucket::Chunk, "sam", &p, &dh, Some(bad));
            assert!(matches!(r, Err(MultipartError::InvalidObjectKey { .. })), "bad={bad}");
        }
    }

    #[test]
    fn compose_distinct_tenants_yield_distinct_keys() {
        let p_a = fixed_prefix(Uuid::from_u128(1));
        let p_b = fixed_prefix(Uuid::from_u128(2));
        let dh = dummy_digest();
        let key_a = compose(Bucket::Chunk, "sam", &p_a, &dh, None).unwrap();
        let key_b = compose(Bucket::Chunk, "sam", &p_b, &dh, None).unwrap();
        assert_ne!(key_a, key_b);
    }
}
