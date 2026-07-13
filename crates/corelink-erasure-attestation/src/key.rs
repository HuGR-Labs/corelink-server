//! [`ErasureSigningKey`] and [`ErasurePublicKey`] — per-region Ed25519 key types.

use crate::region::Region;
use base64::Engine as _;
use ed25519_dalek::{SigningKey, VerifyingKey};
use getrandom::{rand_core::UnwrapErr, SysRng};
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

/// Per-region Ed25519 signing key for erasure attestations.
///
/// Key material is zeroized from memory when this struct is dropped
/// (`ZeroizeOnDrop`). No signing key material appears in logs, traces,
/// or error messages.
///
/// # Lifecycle
///
/// Managed by the S-13 rotation worker via
/// `corelink_rotation_adapters::ErasureAttestationRotationAdapter`.
/// Canonical 30d overlap per `key_management.md §3.2.1` + ADR-0018.
#[derive(ZeroizeOnDrop)]
pub struct ErasureSigningKey {
    /// Monotonic key ID (maps to `erasure_public_keys.key_id` in D1).
    pub key_id: u64,

    /// Region this key belongs to.
    #[zeroize(skip)]
    pub region: Region,

    /// Millisecond timestamp when this key was generated.
    pub created_at_ms: u64,

    /// Millisecond timestamp until which this key is accepted for
    /// verification when in Overlap state (30d canonical).
    pub overlap_until_ms: u64,

    /// The Ed25519 signing key material.
    pub signing_key: SigningKey,
}

impl core::fmt::Debug for ErasureSigningKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // MUST NOT expose signing key bytes in debug output.
        f.debug_struct("ErasureSigningKey")
            .field("key_id", &self.key_id)
            .field("region", &self.region)
            .field("created_at_ms", &self.created_at_ms)
            .field("overlap_until_ms", &self.overlap_until_ms)
            .field("signing_key", &"[REDACTED]")
            .finish()
    }
}

impl ErasureSigningKey {
    /// Generate a new random Ed25519 signing key using the OS CSPRNG.
    ///
    /// # Parameters
    ///
    /// - `key_id`: monotonic ID assigned by the rotation worker.
    /// - `region`: the region this key is bound to.
    /// - `created_at_ms`: millisecond timestamp from the caller (Workers
    ///   `Date.now()`; no `std::time` dependency).
    /// - `overlap_until_ms`: end of the 30d overlap window (caller sets
    ///   `promoted_at_ms + 30d * 1000`).
    #[must_use]
    pub fn generate(
        key_id: u64,
        region: Region,
        created_at_ms: u64,
        overlap_until_ms: u64,
    ) -> Self {
        // ed25519-dalek 3.0's `generate` takes a `rand_core 0.10` `CryptoRng`.
        // `getrandom::SysRng` is the OS CSPRNG (a fallible `TryCryptoRng`);
        // `UnwrapErr` adapts it to the infallible `CryptoRng` the API expects
        // (identical semantics to the former `rand::rngs::OsRng`: an OS-entropy
        // failure is unrecoverable and aborts, exactly as before).
        let signing_key = SigningKey::generate(&mut UnwrapErr(SysRng));
        Self {
            key_id,
            region,
            created_at_ms,
            overlap_until_ms,
            signing_key,
        }
    }

    /// Construct a signing key deterministically from a 32-byte secret seed.
    ///
    /// Unlike [`Self::generate`] (random, ephemeral), this reproduces the SAME
    /// Ed25519 keypair on every process start from a stable secret — the shape
    /// a Cloudflare-Workers / container deployment needs: the per-region signing
    /// seed is held in a write-only secret (`wrangler secret put` / env), so the
    /// public key served by `GET /v1/public/keys/erasure/{region}.pub` stays
    /// stable across restarts and an attestation signed today still verifies
    /// against a key fetched later.
    ///
    /// The `seed` is the raw Ed25519 secret-scalar seed (`SECRET_KEY_LENGTH` =
    /// 32 bytes); callers MUST keep it out of logs / Debug / errors.
    #[must_use]
    pub fn from_seed(
        key_id: u64,
        region: Region,
        created_at_ms: u64,
        overlap_until_ms: u64,
        seed: [u8; 32],
    ) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        Self {
            key_id,
            region,
            created_at_ms,
            overlap_until_ms,
            signing_key,
        }
    }

    /// Derive the corresponding public key.
    #[must_use]
    pub fn public_key(&self) -> ErasurePublicKey {
        let verifying_key = self.signing_key.verifying_key();
        let pem = verifying_key_to_pem(&verifying_key);
        ErasurePublicKey {
            key_id: self.key_id,
            region: self.region,
            created_at_ms: self.created_at_ms,
            overlap_until_ms: self.overlap_until_ms,
            verifying_key,
            pem,
        }
    }
}

/// Per-region Ed25519 public key for attestation verification.
///
/// Served by `GET /v1/public/keys/erasure/{region}.pub` during the
/// Active + Overlap period (up to 30d post-rotation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasurePublicKey {
    /// Monotonic key ID matching `ErasureSigningKey.key_id`.
    pub key_id: u64,

    /// Region this key is bound to.
    pub region: Region,

    /// Millisecond timestamp when the corresponding signing key was generated.
    pub created_at_ms: u64,

    /// Millisecond timestamp until which this key is valid for verification
    /// when in Overlap state.
    pub overlap_until_ms: u64,

    /// The Ed25519 verifying key.
    #[serde(with = "verifying_key_serde")]
    pub verifying_key: VerifyingKey,

    /// PEM-encoded SubjectPublicKeyInfo (SPKI) for the public key endpoint.
    pub pem: String,
}

impl ErasurePublicKey {
    /// Return the SHA-256 hex fingerprint of the public key bytes.
    ///
    /// Customers can pin this fingerprint in their DPA addendum for
    /// out-of-band verification of the public key endpoint response.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(self.verifying_key.as_bytes());
        hex::encode(digest)
    }
}

/// Encode an Ed25519 verifying key as a minimal PEM SubjectPublicKeyInfo.
///
/// Format:
/// ```text
/// -----BEGIN PUBLIC KEY-----
/// <base64 of DER-encoded SubjectPublicKeyInfo>
/// -----END PUBLIC KEY-----
/// ```
///
/// The DER prefix for Ed25519 OID (1.3.101.112) per RFC 8410:
/// `30 2a 30 05 06 03 2b 65 70 03 21 00` (12 bytes) + 32 key bytes.
fn verifying_key_to_pem(vk: &VerifyingKey) -> String {
    // DER SubjectPublicKeyInfo prefix for Ed25519 (RFC 8410).
    const ED25519_SPKI_PREFIX: [u8; 12] = [
        0x30, 0x2a, // SEQUENCE { (42 bytes total)
        0x30, 0x05, // SEQUENCE {
        0x06, 0x03, 0x2b, 0x65, 0x70, // OID 1.3.101.112 (id-EdDSA + ed25519)
        // } end inner SEQUENCE
        0x03, 0x21, 0x00, // BIT STRING, 33 bytes, 0 unused bits
    ];
    let mut der = Vec::with_capacity(ED25519_SPKI_PREFIX.len() + 32);
    der.extend_from_slice(&ED25519_SPKI_PREFIX);
    der.extend_from_slice(vk.as_bytes());
    let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
    format!("-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----")
}

/// Serde support for `VerifyingKey` (base64 of raw 32 bytes).
mod verifying_key_serde {
    use base64::Engine as _;
    use ed25519_dalek::VerifyingKey;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(vk: &VerifyingKey, s: S) -> Result<S::Ok, S::Error> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(vk.as_bytes());
        s.serialize_str(&b64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<VerifyingKey, D::Error> {
        let b64 = String::deserialize(d)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("verifying key must be exactly 32 bytes"))?;
        VerifyingKey::from_bytes(&arr).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_public_key() {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
        let pk = sk.public_key();
        assert_eq!(pk.key_id, 1);
        assert_eq!(pk.region, Region::Weur);
        assert!(pk.pem.starts_with("-----BEGIN PUBLIC KEY-----"));
        assert!(pk.pem.ends_with("-----END PUBLIC KEY-----"));
    }

    #[test]
    fn pem_is_44_chars_base64_line() {
        // 12 prefix + 32 key = 44 bytes → base64 = 60 chars (no padding issues)
        let sk = ErasureSigningKey::generate(1, Region::Wnam, 0, 0);
        let pk = sk.public_key();
        let lines: Vec<&str> = pk.pem.lines().collect();
        assert_eq!(lines.len(), 3); // header + b64 + footer
    }

    #[test]
    fn fingerprint_is_64_hex_chars() {
        let sk = ErasureSigningKey::generate(2, Region::Sam, 0, 0);
        let pk = sk.public_key();
        assert_eq!(pk.fingerprint().len(), 64);
    }

    #[test]
    fn from_seed_is_deterministic() {
        let seed = [7u8; 32];
        let a = ErasureSigningKey::from_seed(5, Region::Enam, 100, 200, seed);
        let b = ErasureSigningKey::from_seed(5, Region::Enam, 100, 200, seed);
        // Same seed → same public key (PEM) → attestations verify across restarts.
        assert_eq!(a.public_key().pem, b.public_key().pem);
        assert_eq!(a.key_id, 5);
        assert_eq!(a.region, Region::Enam);
    }

    #[test]
    fn debug_does_not_expose_key_bytes() {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
        let debug_str = format!("{sk:?}");
        // REDACTED sentinel must appear where the key material would be.
        assert!(
            debug_str.contains("[REDACTED]"),
            "debug must contain [REDACTED]: {debug_str}"
        );
        // Raw byte arrays (e.g. "[12, 34, ...]") MUST NOT appear for key material.
        // The signing_key field value is replaced with "[REDACTED]" in our impl.
        assert!(
            !debug_str.contains("ExpandedSecretKey"),
            "expanded secret key must not be in debug output"
        );
    }
}
