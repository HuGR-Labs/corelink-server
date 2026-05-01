//! HKDF-SHA256 digest signing infrastructure (WI-S04-004 + CTRL-AC-002).
//!
//! This module is the canonical real implementation of the AC envelope
//! signer + verifier sketched as a trait surface in `corelink-worker`'s
//! `reapi::ac::sig` (WI-S04-001 trait-abstraction-defer pattern). The
//! production worker handler consumes [`HkdfSigner`] / [`HkdfVerifier`]
//! through the [`SignatureSigner`] / [`SignatureVerifier`] trait
//! surface — the worker's existing `Signer` trait is satisfied by an
//! adapter that delegates into this crate.
//!
//! ## Cripto algorithm summary (ADR-0021)
//!
//! 1. **Per-tenant key derivation**: HKDF-SHA256 (RFC 5869) with
//!    - `IKM` = `tdk` (Tenant Derivation Key, 32 bytes; fetched from
//!      KMS / Cloudflare Secrets via [`TdkHandle`]).
//!    - `salt` = `sig_key_id.to_le_bytes()` (4 bytes; binds the key
//!      version into the Extract step per Lote 10.4bis P0 fix — TDK
//!      rotation produces correlated keying material; binding the
//!      version into the salt removes that correlation).
//!    - `info` = `b"ac-sig"` (constant; CI test
//!      [`tests::canonical_info_string`] asserts byte-equal).
//!    - Output `OKM` = 32-byte sig key.
//! 2. **MAC primitive**: BLAKE3 keyed-hash (`blake3::keyed_hash(&sig_key,
//!    canonical_bytes)`); 32-byte tag; 256-bit MAC; 2^128 PRF-secure
//!    forge resistance.
//! 3. **Constant-time compare**: [`subtle::ConstantTimeEq`] in the
//!    verify path; never plain `==`.
//! 4. **Key rotation**: `sig_key_id: u32` envelope column; verifier
//!    accepts a configured `accepted_key_ids` whitelist (canonical
//!    production deployment: current + 1 previous); post-grace
//!    expiry returns [`SigError::KeyIdUnknown`].
//! 5. **Reserved sentinel**: `sig_key_id == 0` is the "never-issued"
//!    sentinel (Lote 10.4-tris P0-R5-001); rotation starts at 1; both
//!    sign and verify reject `0` explicitly via
//!    [`SigError::KeyIdReserved`].
//!
//! ## Canonical preimage layout (ADR-0021 v1.0.0 + Lote 10.4-tris P0-R5-003)
//!
//! The 121-byte canonical preimage layout is fixed BIG-ENDIAN per
//! ADR-0021 §3 and matches the worker handler's
//! `corelink-worker::reapi::ac::sig::AcEnvelope::canonicalize` shape
//! 1:1 — see [`canonical::AC_ENVELOPE_PREIMAGE_LEN`] and
//! [`canonical::compose`] for the field layout.
//!
//! ## TDK hygiene (memory zero-on-drop)
//!
//! Every [`TdkHandle::fetch`] surface returns a [`Tdk`] newtype that
//! wraps a `Zeroizing<Vec<u8>>` so the underlying TDK bytes are
//! zero-on-drop (defense against post-mortem memory dump). The newtype
//! has redacted [`fmt::Debug`] (`Tdk(REDACTED)`) and no `Display` /
//! `PartialEq` impls — leak-by-print is a compile error.

#![allow(
    clippy::module_name_repetitions,
    reason = "module is named `sig`; the structs are named with a `Hkdf` prefix to disambiguate the production impl from any future Ed25519-based variant; the canonical names are stable per ADR-0021 + WI-S04-004 §1"
)]

pub mod canonical;
pub mod error;
pub mod hkdf_signer;
pub mod tdk;

pub use canonical::{compose as compose_canonical_bytes, AC_ENVELOPE_PREIMAGE_LEN};
pub use error::SigError;
pub use hkdf_signer::{
    compute_signature, HkdfSigner, HkdfVerifier, AC_ENVELOPE_SIG_LEN, HKDF_INFO_AC_SIG, TDK_LEN,
};
pub use tdk::{derive_default_mock_tdk, MockTdkHandle, Tdk, TdkHandle};

/// Reserved `sig_key_id` sentinel (`0`); production rotations start at
/// `1` per ADR-0021 §Sentinel + Lote 10.4-tris P0-R5-001.
///
/// The wire schema in `migrations/d1/0002_ac_meta.sql` enforces the
/// CHECK predicate `sig_key_id > 0` so a bug that signed with the
/// sentinel cannot corrupt the persisted ledger.
pub const RESERVED_SIG_KEY_ID: u32 = 0;

/// Canonical [`SignatureSigner`] surface — the production handler
/// pre-persist hook calls [`SignatureSigner::sign`] over the canonical
/// preimage bytes derived from the AC envelope.
pub trait SignatureSigner: Send + Sync {
    /// Sign `canonical_bytes` for the per-tenant key derived from
    /// `tenant_id` and `sig_key_id`. Returns the 32-byte tag.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] if `sig_key_id == 0`.
    /// - [`SigError::KeyIdUnknown`] if `sig_key_id` is not in the
    ///   signer's accepted key set.
    /// - [`SigError::BackendError`] when the backing
    ///   [`TdkHandle::fetch`] fails.
    /// - [`SigError::TdkDerivationFailed`] when HKDF-Expand reports a
    ///   length error (unreachable in practice; surfaced as a
    ///   structural failure).
    fn sign(
        &self,
        tenant_id: uuid::Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError>;
}

/// Canonical [`SignatureVerifier`] surface — the production
/// `GetActionResult` handler post-fetch hook + the dual-side client
/// SDK both call [`SignatureVerifier::verify`].
pub trait SignatureVerifier: Send + Sync {
    /// Verify `signature` over `canonical_bytes` for the per-tenant key
    /// derived from `tenant_id` and `sig_key_id`.
    ///
    /// Constant-time on the success / cripto-mismatch path
    /// (`subtle::ConstantTimeEq` + sig-key fetch happens before the
    /// compare so the timing of the Extract+Expand round-trip is
    /// uniform across both arms).
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] if `sig_key_id == 0`.
    /// - [`SigError::KeyIdUnknown`] if `sig_key_id` is not in the
    ///   verifier's `accepted_key_ids` whitelist.
    /// - [`SigError::LengthMismatch`] if `signature.len() !=
    ///   AC_ENVELOPE_SIG_LEN`.
    /// - [`SigError::Invalid`] on cripto mismatch (constant-time).
    /// - [`SigError::BackendError`] when the backing
    ///   [`TdkHandle::fetch`] fails.
    /// - [`SigError::TdkDerivationFailed`] when HKDF-Expand reports a
    ///   length error.
    fn verify(
        &self,
        tenant_id: uuid::Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
        signature: &[u8],
    ) -> Result<(), SigError>;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_info_string_is_ac_sig() {
        // CI gate: HKDF info bytes drift = global signature mismatch.
        // The constant must be exactly `b"ac-sig"` per ADR-0021.
        assert_eq!(HKDF_INFO_AC_SIG, b"ac-sig");
        assert_eq!(HKDF_INFO_AC_SIG.len(), 6);
    }

    #[test]
    fn reserved_sig_key_id_is_zero() {
        // Pin the canonical sentinel — schema CHECK constraints in
        // migrations/d1/0002_ac_meta.sql + downstream consumers all
        // assume `0` is the reserved sentinel.
        assert_eq!(RESERVED_SIG_KEY_ID, 0);
    }
}
