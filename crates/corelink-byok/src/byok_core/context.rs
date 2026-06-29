//! Canonical crypto context — the SINGLE structured, length-framed input to
//! BOTH the HKDF key-derivation `info` and the AEAD AAD. Audit fix **[C-2]**.
//!
//! # Why a struct + real JCS (NEVER raw `‖` concatenation)
//!
//! Raw byte-concatenation of fields is canonical-collision-prone: with
//! `info = a ‖ b`, the pair `("ab", "cd")` and `("a", "bcd")` produce the
//! IDENTICAL `info` (`"abcd"`) — so two semantically different contexts
//! derive the SAME key/AAD. Binding `algo`/`mode`/`version`/`surface`
//! structurally also closes the audit's "attacker-chosen `algo`" gap: the
//! algorithm is part of the derivation, not a free parameter.
//!
//! [`CryptoContext`] is serialized via a real RFC-8785 (JCS) serializer
//! (`serde_jcs`). JCS sorts object keys lexicographically and emits a single
//! canonical byte string, so the same logical context produces byte-identical
//! `info`/AAD across architectures (native, wasm32) and field-insertion
//! orderings. Those canonical bytes feed:
//!
//! - HKDF `info` (see [`crate::byok_core::convergent`]), and
//! - the AEAD AAD bound into the body cipher (see
//!   [`crate::byok_core::envelope`]).
//!
//! # Single-shot only (audit **[C-1]**)
//!
//! This crate operates on WHOLE objects. [`CryptoContext::chunk_index`] exists
//! to bind a future multipart unit into the derivation, but the convergent
//! layer here REFUSES any `chunk_index != 0` — whole-object-digest derivation
//! must never be reused across chunks (that is a catastrophic GCM
//! nonce-reuse break). Multipart BYOK is deferred and must be gated elsewhere.

use serde::{Deserialize, Serialize};

use super::types::BYOKError;

/// Current canonical context schema version. Bumping this changes every
/// derived key + AAD (it is bound into the JCS bytes), so it doubles as an
/// epoch / domain-separation knob.
pub const CRYPTO_CONTEXT_VERSION: u32 = 1;

/// AEAD / cipher algorithm bound into the KDF + AAD for domain separation.
///
/// Binding the algorithm structurally (rather than accepting an
/// attacker-chosen free parameter) closes audit gap **[C-2]**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CryptoAlgo {
    /// AES-256-GCM (FIPS 197 + NIST SP 800-38D).
    Aes256Gcm,
}

/// Envelope policy mode (plan §2). Bound into the derivation so a Mode-A
/// ciphertext can never be decrypted under a Mode-B context (or vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CryptoMode {
    /// Mode A — convergent (deterministic DEK+nonce, dedup-preserving).
    Convergent,
    /// Mode B — max-isolation (random DEK + random nonce, no dedup).
    Random,
}

/// The canonical, length-framed key-derivation / AAD context.
///
/// Every field is bound into both the HKDF `info` and the AEAD AAD via the
/// JCS bytes from [`CryptoContext::to_jcs_bytes`]. Two contexts that differ in
/// ANY field derive different keys and produce non-interchangeable
/// ciphertext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoContext {
    /// Schema / epoch version (see [`CRYPTO_CONTEXT_VERSION`]).
    pub version: u32,
    /// Owning tenant id (also used as the HKDF salt).
    pub tenant_id: String,
    /// Content-addressed plaintext digest of the whole object.
    pub plaintext_digest: String,
    /// AEAD algorithm in use.
    pub algo: CryptoAlgo,
    /// Logical namespace (e.g. cache namespace) — domain separation.
    pub namespace: String,
    /// Policy mode (convergent vs random).
    pub mode: CryptoMode,
    /// Surface that produced the bytes (e.g. `"cas"`, `"ac"`) — domain
    /// separation across cache surfaces.
    pub surface: String,
    /// CMK identifier the DEK is wrapped under.
    pub key_id: String,
    /// Chunk index. MUST be `0` in this crate (single-shot only, audit
    /// [C-1]); reserved for a future multipart unit.
    pub chunk_index: u64,
    /// Total whole-object length in bytes (`0` when not asserted).
    pub total_len: u64,
}

impl CryptoContext {
    /// Construct a whole-object (single-shot) context. `chunk_index` is fixed
    /// to `0` — this crate has no chunked path (audit [C-1]).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_single_shot(
        tenant_id: impl Into<String>,
        plaintext_digest: impl Into<String>,
        algo: CryptoAlgo,
        namespace: impl Into<String>,
        mode: CryptoMode,
        surface: impl Into<String>,
        key_id: impl Into<String>,
        total_len: u64,
    ) -> Self {
        Self {
            version: CRYPTO_CONTEXT_VERSION,
            tenant_id: tenant_id.into(),
            plaintext_digest: plaintext_digest.into(),
            algo,
            namespace: namespace.into(),
            mode,
            surface: surface.into(),
            key_id: key_id.into(),
            chunk_index: 0,
            total_len,
        }
    }

    /// Build the default context used by the back-compat
    /// `EnvelopeEncryptor::encrypt`/`decrypt` wrappers (Mode B, single-shot,
    /// AES-256-GCM, CAS surface). `total_len` is left `0` so both the encrypt
    /// and decrypt sides reconstruct byte-identical context without coupling
    /// to the GCM tag length.
    #[must_use]
    pub fn legacy(
        tenant_id: impl Into<String>,
        plaintext_digest: impl Into<String>,
        key_id: impl Into<String>,
    ) -> Self {
        Self::new_single_shot(
            tenant_id,
            plaintext_digest,
            CryptoAlgo::Aes256Gcm,
            "",
            CryptoMode::Random,
            "cas",
            key_id,
            0,
        )
    }

    /// `true` iff this context describes a whole-object (single-shot) unit.
    #[must_use]
    pub const fn is_single_shot(&self) -> bool {
        self.chunk_index == 0
    }

    /// RFC-8785 (JCS) canonical bytes — the single source of truth fed to
    /// both the HKDF `info` and the body AEAD AAD.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::EnvelopeError`] if JCS serialization fails
    /// (practically impossible — the value is a plain struct).
    pub fn to_jcs_bytes(&self) -> Result<Vec<u8>, BYOKError> {
        serde_jcs::to_vec(self)
            .map_err(|e| BYOKError::EnvelopeError(format!("crypto context JCS canonicalize: {e}")))
    }

    /// The AWS-KMS-compatible `encryption_context` (string→string map) bound
    /// at `wrap_dek` time. Kept minimal (`tenant_id` + `blob_hash`) so it
    /// satisfies the KMS string-map constraint; the FULL context is bound
    /// into the body AEAD AAD instead (audit [H-2]).
    #[must_use]
    pub fn kms_encryption_context(&self) -> serde_json::Value {
        serde_json::json!({
            "tenant_id": self.tenant_id,
            "blob_hash": self.plaintext_digest,
        })
    }
}
