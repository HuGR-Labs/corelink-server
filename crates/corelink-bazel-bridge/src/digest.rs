//! REAPI v2 `Digest` type: `{ hash, size_bytes }`.
//!
//! # Wire format
//!
//! REAPI v2 represents a digest as `<sha256_hex>/<size_bytes>` in URI paths
//! (e.g., `abc...def/1234`) and as a JSON object `{"hash":"<hex>","sizeBytes":1234}`
//! in request/response bodies.
//!
//! # Validation rules (INV-BAZEL-DIGEST-VALIDATE)
//!
//! 1. `hash` MUST be exactly 64 lowercase hexadecimal characters.
//! 2. `size_bytes` MUST be parseable as a `u64`.
//! 3. `size_bytes` MUST be ≤ [`crate::MAX_BLOB_SIZE_BYTES`] (4 GiB).
//!    Larger blobs require ByteStream which this REST bridge does not
//!    implement.

use serde::{Deserialize, Serialize};

use crate::error::BazelBridgeError;

/// A validated REAPI v2 content digest: SHA-256 hash + expected byte size.
///
/// Invariants:
/// - `hash` is exactly 64 lowercase hex characters.
/// - `size_bytes` is ≤ [`crate::MAX_BLOB_SIZE_BYTES`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Digest {
    /// SHA-256 hex hash (64 lowercase hex characters).
    pub hash: String,
    /// Expected byte size of the blob.
    pub size_bytes: u64,
}

impl Digest {
    /// Parse a REAPI v2 URI path segment of the form `<hash>/<size_bytes>`.
    ///
    /// The `ac/` prefix used for AC path segments must be stripped by the
    /// caller before invoking this function (see [`crate::uri`]).
    ///
    /// # Errors
    ///
    /// Returns [`BazelBridgeError::InvalidDigest`] if:
    /// - the input does not contain exactly one `/` separator
    /// - the hash portion is not exactly 64 lowercase hex characters
    /// - the size portion is not a valid `u64`
    /// - the size exceeds 4 GiB
    pub fn parse(s: &str) -> Result<Self, BazelBridgeError> {
        let (hash_part, size_part) = s.split_once('/').ok_or_else(|| {
            BazelBridgeError::InvalidDigest {
                reason: format!("expected '<hash>/<size_bytes>', got: {s:?}"),
            }
        })?;
        validate_hash(hash_part)?;
        let size_bytes = size_part
            .parse::<u64>()
            .map_err(|_| BazelBridgeError::InvalidDigest {
                reason: format!("size_bytes is not a valid u64: {size_part:?}"),
            })?;
        validate_size(size_bytes)?;
        Ok(Self {
            hash: hash_part.to_owned(),
            size_bytes,
        })
    }

    /// Build a `Digest` from already-validated components.
    ///
    /// # Errors
    ///
    /// Returns [`BazelBridgeError::InvalidDigest`] if either component
    /// fails the validation rules (same as [`Digest::parse`]).
    pub fn new(hash: impl Into<String>, size_bytes: u64) -> Result<Self, BazelBridgeError> {
        let hash = hash.into();
        validate_hash(&hash)?;
        validate_size(size_bytes)?;
        Ok(Self { hash, size_bytes })
    }

    /// Render the digest as a URI path segment `<hash>/<size_bytes>`.
    #[must_use]
    pub fn to_uri_segment(&self) -> String {
        format!("{}/{}", self.hash, self.size_bytes)
    }
}

impl core::fmt::Display for Digest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.hash, self.size_bytes)
    }
}

/// Validate that `hash` is exactly 64 lowercase hexadecimal characters.
fn validate_hash(hash: &str) -> Result<(), BazelBridgeError> {
    if hash.len() != 64 {
        return Err(BazelBridgeError::InvalidDigest {
            reason: format!(
                "hash must be exactly 64 hex chars, got {} chars: {hash:?}",
                hash.len()
            ),
        });
    }
    if !hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(BazelBridgeError::InvalidDigest {
            reason: format!("hash must be lowercase hex, got: {hash:?}"),
        });
    }
    Ok(())
}

/// Validate that `size_bytes` does not exceed the 4 GiB blob cap.
fn validate_size(size_bytes: u64) -> Result<(), BazelBridgeError> {
    if size_bytes > crate::MAX_BLOB_SIZE_BYTES {
        return Err(BazelBridgeError::InvalidDigest {
            reason: format!(
                "size_bytes {size_bytes} exceeds 4 GiB cap ({}); \
                 use ByteStream for large blobs",
                crate::MAX_BLOB_SIZE_BYTES
            ),
        });
    }
    Ok(())
}

/// JSON-wire representation of a REAPI digest in request/response bodies.
///
/// REAPI v2 uses `sizeBytes` (camelCase) in JSON even though URI segments
/// use the slash-separated positional form. This type owns the serde shapes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestJson {
    /// SHA-256 hex hash.
    pub hash: String,
    /// Expected byte size.
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
}

impl TryFrom<DigestJson> for Digest {
    type Error = BazelBridgeError;

    fn try_from(v: DigestJson) -> Result<Self, Self::Error> {
        Digest::new(v.hash, v.size_bytes)
    }
}

impl From<Digest> for DigestJson {
    fn from(d: Digest) -> Self {
        Self {
            hash: d.hash,
            size_bytes: d.size_bytes,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// 64 lowercase hex chars (all zeros).
    const GOOD_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn parse_valid_digest() {
        let d = Digest::parse(&format!("{GOOD_HASH}/1024")).expect("parse");
        assert_eq!(d.hash, GOOD_HASH);
        assert_eq!(d.size_bytes, 1024);
    }

    #[test]
    fn parse_zero_size_bytes_is_valid() {
        // Empty blobs are legal (e.g. the empty-blob SHA-256).
        let d = Digest::parse(&format!("{GOOD_HASH}/0")).expect("parse zero size");
        assert_eq!(d.size_bytes, 0);
    }

    #[test]
    fn parse_max_size_bytes_is_valid() {
        let d = Digest::parse(&format!("{GOOD_HASH}/{}", crate::MAX_BLOB_SIZE_BYTES))
            .expect("parse max size");
        assert_eq!(d.size_bytes, crate::MAX_BLOB_SIZE_BYTES);
    }

    #[test]
    fn parse_size_exceeds_cap_is_invalid() {
        let over = crate::MAX_BLOB_SIZE_BYTES + 1;
        let err = Digest::parse(&format!("{GOOD_HASH}/{over}")).expect_err("should fail");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_missing_slash_is_invalid() {
        let err = Digest::parse(GOOD_HASH).expect_err("no slash");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_hash_too_short_is_invalid() {
        let short = "0".repeat(63);
        let err = Digest::parse(&format!("{short}/100")).expect_err("too short");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_hash_too_long_is_invalid() {
        let long = "0".repeat(65);
        let err = Digest::parse(&format!("{long}/100")).expect_err("too long");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_uppercase_hash_is_invalid() {
        // REAPI spec requires lowercase hex.
        let upper = "A".repeat(64);
        let err = Digest::parse(&format!("{upper}/100")).expect_err("uppercase");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_mixed_case_hash_is_invalid() {
        let mixed = format!("{}{}", "a".repeat(32), "A".repeat(32));
        let err = Digest::parse(&format!("{mixed}/100")).expect_err("mixed case");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_non_hex_hash_is_invalid() {
        let bad = "z".repeat(64);
        let err = Digest::parse(&format!("{bad}/100")).expect_err("non-hex");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_non_numeric_size_is_invalid() {
        let err = Digest::parse(&format!("{GOOD_HASH}/notanumber")).expect_err("bad size");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn to_uri_segment_round_trips() {
        let original = format!("{GOOD_HASH}/512");
        let d = Digest::parse(&original).expect("parse");
        assert_eq!(d.to_uri_segment(), original);
    }

    #[test]
    fn display_equals_uri_segment() {
        let d = Digest::parse(&format!("{GOOD_HASH}/256")).expect("parse");
        assert_eq!(d.to_string(), d.to_uri_segment());
    }

    #[test]
    fn new_validates_same_as_parse() {
        let d = Digest::new(GOOD_HASH, 100).expect("new");
        assert_eq!(d.hash, GOOD_HASH);
        assert_eq!(d.size_bytes, 100);
    }

    #[test]
    fn digest_json_round_trip() {
        let d = Digest::new(GOOD_HASH, 42).expect("new");
        let j = DigestJson::from(d.clone());
        assert_eq!(j.hash, GOOD_HASH);
        assert_eq!(j.size_bytes, 42);
        let back = Digest::try_from(j).expect("try_from");
        assert_eq!(back, d);
    }

    #[test]
    fn digest_json_serde_round_trip() {
        let d = Digest::new(GOOD_HASH, 99).expect("new");
        let j: DigestJson = d.into();
        let text = serde_json::to_string(&j).expect("to_string");
        assert!(text.contains("sizeBytes"));
        let back: DigestJson = serde_json::from_str(&text).expect("from_str");
        assert_eq!(back.size_bytes, 99);
    }
}
