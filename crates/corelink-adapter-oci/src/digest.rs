//! OCI digest (`sha256:<hex>` / `sha512:<hex>`) ↔ CoreLink CAS bridge.
//!
//! The OCI Distribution Spec v1.1 carries content addresses as
//! `<algorithm>:<hex>` strings. The dominant case is `sha256:<64-hex>`
//! (32-byte SHA-256 output). CoreLink internally addresses by BLAKE3
//! ([`corelink_core::Digest`], also 32 bytes), so the OCI digest is
//! NOT directly translatable to a CAS key. The adapter holds an
//! external mapping in KV (`oci_blob_index:<tenant>:<oci-digest>` →
//! `CAS key (BLAKE3 hex)`) maintained at blob-finalize time.
//!
//! This module only handles the **parse / verify / format** half of
//! the bridge — the KV mapping write is in [`crate::push::upload`].
//!
//! ## Why we don't reuse `corelink_core::Digest::from_hex`
//!
//! `corelink_core::Digest` is BLAKE3-typed; passing in 32 bytes of
//! SHA-256 would silently mis-type. We keep the OCI digest as its own
//! [`OciDigest`] newtype so the type system enforces "this came from
//! the wire, treat as opaque address until verified".

use sha2::{Digest as _, Sha256};

use crate::error::OciAdapterError;

/// Supported OCI hash algorithms.
///
/// Per OCI Distribution Spec v1.1 §digest, `sha256` is REQUIRED and
/// `sha512` is OPTIONAL. We accept both on parse but compute only the
/// algorithm declared by the client (so a `sha256:` declared digest is
/// recomputed with SHA-256, never with SHA-512).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OciDigestAlgo {
    /// SHA-256 (32 bytes / 64 lowercase hex chars).
    Sha256,
    /// SHA-512 (64 bytes / 128 lowercase hex chars).
    Sha512,
}

impl OciDigestAlgo {
    /// Hex-encoded output length.
    #[must_use]
    pub const fn hex_len(&self) -> usize {
        match self {
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }

    /// String form as it appears on the wire (`"sha256"` / `"sha512"`).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

/// Parsed OCI digest, opaque until verified against actual bytes.
///
/// Construct via [`Self::parse`]; never fabricate from arbitrary
/// strings (the type's lone field is private specifically to gate
/// construction through validation).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct OciDigest {
    algo: OciDigestAlgo,
    /// Lowercase hex; always exactly `algo.hex_len()` bytes long after
    /// [`Self::parse`] validation.
    hex: String,
}

impl OciDigest {
    /// Parse a wire-form OCI digest (`"sha256:<hex>"`).
    ///
    /// # Errors
    ///
    /// - [`OciAdapterError::DigestMismatch`] if format is malformed
    ///   (we reuse the variant rather than introducing a parse-time
    ///   error since adversarial tests need a single deterministic
    ///   wire shape for invalid digests).
    pub fn parse(wire: &str) -> Result<Self, OciAdapterError> {
        let Some((algo_str, hex)) = wire.split_once(':') else {
            return Err(OciAdapterError::DigestMismatch {
                declared: wire.to_string(),
                computed: String::from("<unparseable>"),
            });
        };
        let algo = match algo_str {
            "sha256" => OciDigestAlgo::Sha256,
            "sha512" => OciDigestAlgo::Sha512,
            _ => {
                return Err(OciAdapterError::DigestMismatch {
                    declared: wire.to_string(),
                    computed: String::from("<unsupported-algo>"),
                });
            }
        };
        if hex.len() != algo.hex_len() {
            return Err(OciAdapterError::DigestMismatch {
                declared: wire.to_string(),
                computed: String::from("<bad-length>"),
            });
        }
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(OciAdapterError::DigestMismatch {
                declared: wire.to_string(),
                computed: String::from("<non-hex>"),
            });
        }
        // Normalize to lowercase: OCI Distribution Spec §digest says
        // "hex digit are lowercase" but real clients sometimes upper.
        Ok(Self {
            algo,
            hex: hex.to_ascii_lowercase(),
        })
    }

    /// Render this digest in canonical wire form (`"sha256:<hex>"`).
    #[must_use]
    pub fn to_wire(&self) -> String {
        format!("{}:{}", self.algo.name(), self.hex)
    }

    /// Hex portion only (no `algo:` prefix).
    #[must_use]
    pub fn hex(&self) -> &str {
        &self.hex
    }

    /// Algorithm.
    #[must_use]
    pub const fn algo(&self) -> OciDigestAlgo {
        self.algo
    }

    /// Compute the digest of `bytes` under `algo` and return a fresh
    /// [`OciDigest`].
    ///
    /// SHA-512 is currently UNSUPPORTED in this build (would require
    /// pulling `sha2::Sha512` separately; deferred until a customer
    /// requests it — every container client in the wild defaults to
    /// sha256). Passing `OciDigestAlgo::Sha512` returns an error.
    ///
    /// # Errors
    ///
    /// Returns [`OciAdapterError::Cas`] on unsupported algorithm.
    pub fn compute(algo: OciDigestAlgo, bytes: &[u8]) -> Result<Self, OciAdapterError> {
        match algo {
            OciDigestAlgo::Sha256 => {
                let mut h = Sha256::new();
                h.update(bytes);
                let out = h.finalize();
                Ok(Self {
                    algo,
                    hex: hex::encode(out),
                })
            }
            OciDigestAlgo::Sha512 => Err(OciAdapterError::Cas(String::from(
                "sha512 OCI digest compute is unsupported in this build",
            ))),
        }
    }

    /// Verify that the bytes' actual digest equals `self`. Constant-
    /// time string compare on the hex form to avoid timing leak on
    /// the prefix-mismatch path (an adversary could otherwise binary-
    /// search the digest one nibble at a time).
    ///
    /// # Errors
    ///
    /// [`OciAdapterError::DigestMismatch`] if the recomputed hash
    /// differs. The error carries both `declared` (this) and
    /// `computed` (recomputed) for audit-log forensics.
    pub fn verify_against_bytes(&self, bytes: &[u8]) -> Result<(), OciAdapterError> {
        let recomputed = Self::compute(self.algo, bytes)?;
        // Constant-time compare on the hex form.
        let a = self.hex.as_bytes();
        let b = recomputed.hex.as_bytes();
        let lengths_equal = a.len() == b.len();
        // The lengths are always equal by construction (same algo →
        // same hex_len), but keep the assertion explicit so a future
        // refactor doesn't silently slip a short-circuit in.
        if !lengths_equal {
            return Err(OciAdapterError::DigestMismatch {
                declared: self.to_wire(),
                computed: recomputed.to_wire(),
            });
        }
        // `subtle::ConstantTimeEq` on byte-equal-length slices.
        use subtle::ConstantTimeEq;
        if a.ct_eq(b).into() {
            Ok(())
        } else {
            Err(OciAdapterError::DigestMismatch {
                declared: self.to_wire(),
                computed: recomputed.to_wire(),
            })
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

    #[test]
    fn parses_sha256() {
        let d = OciDigest::parse(
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        .expect("valid sha256");
        assert_eq!(d.algo(), OciDigestAlgo::Sha256);
        assert_eq!(d.hex().len(), 64);
    }

    #[test]
    fn parses_sha512() {
        let h = "a".repeat(128);
        let d = OciDigest::parse(&format!("sha512:{h}")).expect("valid sha512");
        assert_eq!(d.algo(), OciDigestAlgo::Sha512);
    }

    #[test]
    fn rejects_unknown_algo() {
        assert!(OciDigest::parse("md5:abcd").is_err());
    }

    #[test]
    fn rejects_bad_length() {
        assert!(OciDigest::parse("sha256:deadbeef").is_err());
    }

    #[test]
    fn rejects_non_hex() {
        let bad = format!("sha256:{}", "z".repeat(64));
        assert!(OciDigest::parse(&bad).is_err());
    }

    #[test]
    fn compute_then_verify_roundtrip() {
        let bytes = b"hello world";
        let d = OciDigest::compute(OciDigestAlgo::Sha256, bytes).expect("compute");
        d.verify_against_bytes(bytes).expect("verify");
    }

    #[test]
    fn verify_mismatch_returns_digest_mismatch() {
        let d = OciDigest::compute(OciDigestAlgo::Sha256, b"hello").expect("compute");
        let err = d
            .verify_against_bytes(b"goodbye")
            .expect_err("must mismatch");
        match err {
            OciAdapterError::DigestMismatch { .. } => {}
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
    }

    #[test]
    fn case_insensitive_hex_parse() {
        // Some clients (older buildah versions) emit uppercase hex.
        let upper = "sha256:E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        let d = OciDigest::parse(upper).expect("uppercase ok");
        // After parse, hex must be canonical lowercase.
        assert!(d.hex().chars().all(|c| !c.is_ascii_uppercase()));
    }
}
