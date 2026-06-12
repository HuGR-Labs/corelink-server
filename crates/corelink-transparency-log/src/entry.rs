//! [`SignedEntry`] — a CoreLink-signed entry to witness — and
//! [`RekorHashedRekord`] — the canonical Rekor `hashedrekord` v0.0.1
//! proposed-entry that witnesses it.
//!
//! ## Why `hashedrekord`
//!
//! sigstore/Rekor's `hashedrekord` v0.0.1 type witnesses *a signature over a
//! hashed payload*, binding `{content_digest, signature, public_key}` without
//! ever uploading the payload bytes themselves. That is exactly CoreLink's
//! shape: the audit/attestation payload is already JCS-canonical bytes signed
//! with a published per-region Ed25519 key. We submit the SHA-256 of the
//! canonical payload, the detached signature, and the PEM public key — the
//! payload (which may carry pseudonymous tenant data) never leaves CoreLink.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// A CoreLink-signed entry that is a candidate for public witnessing.
///
/// This is the *input* to the seam — produced upstream by
/// `corelink-erasure-attestation` (an Ed25519 attestation: `payload` =
/// `canonical_payload_jcs` bytes, `signature` = the raw Ed25519 signature,
/// `public_key_pem` = the published per-region key) or by
/// `corelink-audit-chain` (a signed chain head). The seam is agnostic to which
/// — it only needs the three cryptographic facts Rekor requires.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEntry {
    /// The exact canonical bytes that were signed (e.g. the RFC 8785 JCS
    /// canonical payload). Only its SHA-256 digest is submitted; the bytes
    /// themselves never leave CoreLink.
    pub payload: Vec<u8>,
    /// The signature algorithm identifier (e.g. `"ed25519"`). Carried for the
    /// caller's bookkeeping; Rekor infers the scheme from the public key.
    pub signature_algorithm: String,
    /// The signer's public key in PEM form (the key served by
    /// `GET /v1/public/keys/erasure/{region}.pub`).
    pub public_key_pem: Vec<u8>,
    /// The detached signature over `payload`.
    pub signature: Vec<u8>,
}

impl SignedEntry {
    /// Construct a signed entry from its parts.
    #[must_use]
    pub fn new(
        payload: Vec<u8>,
        signature_algorithm: impl Into<String>,
        public_key_pem: Vec<u8>,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            payload,
            signature_algorithm: signature_algorithm.into(),
            public_key_pem,
            signature,
        }
    }

    /// Lowercase-hex SHA-256 digest of the payload — the value Rekor records
    /// under `spec.data.hash.value`.
    #[must_use]
    pub fn content_sha256_hex(&self) -> String {
        let digest = Sha256::digest(&self.payload);
        hex::encode(digest)
    }

    /// Build the canonical Rekor `hashedrekord` v0.0.1 proposed entry that
    /// witnesses this signed entry.
    #[must_use]
    pub fn to_rekor_hashedrekord(&self) -> RekorHashedRekord {
        let b64 = base64::engine::general_purpose::STANDARD;
        RekorHashedRekord {
            api_version: "0.0.1".to_owned(),
            kind: "hashedrekord".to_owned(),
            spec: RekorHashedRekordSpec {
                data: RekorData {
                    hash: RekorHash {
                        algorithm: "sha256".to_owned(),
                        value: self.content_sha256_hex(),
                    },
                },
                signature: RekorSignature {
                    content: b64.encode(&self.signature),
                    public_key: RekorPublicKey {
                        content: b64.encode(&self.public_key_pem),
                    },
                },
            },
        }
    }
}

/// A Rekor `hashedrekord` v0.0.1 proposed entry — the exact JSON shape the
/// public Rekor `POST /api/v1/log/entries` endpoint accepts.
///
/// Field naming matches the Rekor wire schema (`apiVersion`, `kind`, `spec`)
/// via `#[serde(rename)]` so the serialized form is submission-ready without a
/// translation layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorHashedRekord {
    /// Rekor type schema version. Always `"0.0.1"` for this type.
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    /// Rekor entry kind. Always `"hashedrekord"`.
    pub kind: String,
    /// The entry specification.
    pub spec: RekorHashedRekordSpec,
}

impl RekorHashedRekord {
    /// Serialize to the Rekor wire JSON.
    ///
    /// # Errors
    ///
    /// Returns [`crate::TransparencyLogError::Serialization`] if `serde_json`
    /// fails (never on the happy path — the struct is plain owned data).
    pub fn to_wire_json(&self) -> Result<String, crate::TransparencyLogError> {
        serde_json::to_string(self)
            .map_err(|e| crate::TransparencyLogError::Serialization(e.to_string()))
    }
}

/// `spec` of a `hashedrekord` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorHashedRekordSpec {
    /// The hashed content being witnessed.
    pub data: RekorData,
    /// The detached signature + public key.
    pub signature: RekorSignature,
}

/// `spec.data` — the content hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorData {
    /// The content digest.
    pub hash: RekorHash,
}

/// `spec.data.hash` — algorithm + lowercase-hex value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorHash {
    /// Hash algorithm. Always `"sha256"` for CoreLink entries.
    pub algorithm: String,
    /// Lowercase-hex digest value.
    pub value: String,
}

/// `spec.signature` — base64 signature + public key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorSignature {
    /// Base64-encoded detached signature.
    pub content: String,
    /// The verifying public key.
    #[serde(rename = "publicKey")]
    pub public_key: RekorPublicKey,
}

/// `spec.signature.publicKey` — base64 PEM public key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorPublicKey {
    /// Base64-encoded PEM public key.
    pub content: String,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests may use unwrap/expect"
)]
mod tests {
    use super::*;

    fn sample() -> SignedEntry {
        SignedEntry::new(
            b"{\"request_id\":\"req-001\"}".to_vec(),
            "ed25519",
            b"-----BEGIN PUBLIC KEY-----\nABC\n-----END PUBLIC KEY-----\n".to_vec(),
            vec![0x01, 0x02, 0x03, 0x04],
        )
    }

    #[test]
    fn content_digest_is_lowercase_hex_sha256() {
        let e = sample();
        let h = e.content_sha256_hex();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Cross-check against an independent SHA-256.
        let expect = hex::encode(Sha256::digest(&e.payload));
        assert_eq!(h, expect);
    }

    #[test]
    fn rekor_body_has_canonical_kind_and_version() {
        let body = sample().to_rekor_hashedrekord();
        assert_eq!(body.kind, "hashedrekord");
        assert_eq!(body.api_version, "0.0.1");
        assert_eq!(body.spec.data.hash.algorithm, "sha256");
    }

    #[test]
    fn signature_and_key_are_base64() {
        let body = sample().to_rekor_hashedrekord();
        let b64 = base64::engine::general_purpose::STANDARD;
        let sig = b64.decode(&body.spec.signature.content).unwrap();
        assert_eq!(sig, vec![0x01, 0x02, 0x03, 0x04]);
        let key = b64.decode(&body.spec.signature.public_key.content).unwrap();
        assert!(key.starts_with(b"-----BEGIN PUBLIC KEY-----"));
    }

    #[test]
    fn wire_json_uses_rekor_field_names() {
        let json = sample().to_rekor_hashedrekord().to_wire_json().unwrap();
        assert!(json.contains("\"apiVersion\":\"0.0.1\""));
        assert!(json.contains("\"kind\":\"hashedrekord\""));
        assert!(json.contains("\"publicKey\""));
    }

    #[test]
    fn payload_bytes_never_appear_in_wire_json() {
        // The raw payload must NOT be uploaded — only its digest.
        let e = sample();
        let json = e.to_rekor_hashedrekord().to_wire_json().unwrap();
        assert!(!json.contains("request_id"));
        assert!(json.contains(&e.content_sha256_hex()));
    }
}
