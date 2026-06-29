//! Convergence layer (Mode A) — the dedup-preserving derivation path.
//!
//! Plan §1/§2: within an org, identical plaintext must yield identical
//! ciphertext so intra-org dedup survives encryption-at-rest. We achieve this
//! deterministically WITHOUT touching the CMK by deriving the per-blob DEK
//! (and nonce) from the per-tenant [`Tcs`] via HKDF-SHA256:
//!
//! ```text
//! DEK   = HKDF-SHA256(ikm = TCS, salt = tenant_id, info = framed(DEK_LABEL,   JCS(ctx)))
//! nonce = HKDF-SHA256(ikm = TCS, salt = tenant_id, info = framed(NONCE_LABEL, JCS(ctx)))
//! ```
//!
//! # GCM nonce-reuse safety (single-shot ONLY — audit [C-1])
//!
//! A deterministic AES-GCM nonce is normally fatal. It is safe here BY
//! CONSTRUCTION for a WHOLE object: the nonce is derived from the
//! plaintext-digest-bearing context, so a given `(DEK, nonce)` pair only ever
//! encrypts ONE plaintext (the one whose digest produced both). There is no
//! second distinct plaintext under the same `(key, nonce)`, which is the
//! precondition for the GCM forgery / keystream-reuse attack. **This argument
//! FAILS for multipart/chunked data** (one whole-object digest reused across
//! distinct parts → catastrophic break). Therefore this crate is single-shot
//! only: every derivation entry point rejects `ctx.chunk_index != 0` (see
//! [`crate::byok_core::types::BYOKError::ChunkedConvergentUnsupported`]).
//! Multipart BYOK is deferred and MUST be gated elsewhere — do not add a
//! chunked path here.
//!
//! # Domain separation (audit [C-2])
//!
//! The DEK and nonce are DISTINCT HKDF outputs under DISTINCT, length-framed
//! `info` labels, so the DEK and the nonce are never the same digest material.
//! All structural fields (`algo`, `mode`, `version`, `surface`, …) are bound
//! via the JCS bytes — no raw concatenation.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Key, Nonce,
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::context::CryptoContext;
use super::types::{BYOKError, Dek, Tcs};

/// HKDF `info` label for the convergent DEK output (domain separation).
const DEK_LABEL: &[u8] = b"corelink/byok/convergent/dek/v1";

/// HKDF `info` label for the convergent nonce output (domain separation) —
/// DISTINCT from [`DEK_LABEL`] so DEK and nonce are independent material.
const NONCE_LABEL: &[u8] = b"corelink/byok/convergent/nonce/v1";

/// Output of the convergent write path. Deterministic: identical
/// `(tcs, ctx, plaintext)` produces a byte-identical [`ConvergentBlob`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergentBlob {
    /// AES-256-GCM ciphertext (AAD-bound to `JCS(ctx)`).
    pub ciphertext: Vec<u8>,
    /// 96-bit deterministic nonce (derived from `(tcs, ctx)`).
    pub nonce: [u8; 12],
}

/// Length-framed concatenation: `len(label) ‖ label ‖ len(jcs) ‖ jcs`.
///
/// Length framing makes the encoding injective — there is no `(label, jcs)`
/// pair distinct from another that maps to the same bytes (defeats the
/// canonical-collision class the audit flagged for raw `‖`).
fn framed_info(label: &[u8], jcs: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + label.len() + jcs.len());
    v.extend_from_slice(&(label.len() as u64).to_be_bytes());
    v.extend_from_slice(label);
    v.extend_from_slice(&(jcs.len() as u64).to_be_bytes());
    v.extend_from_slice(jcs);
    v
}

/// Reject any chunked/multipart context — single-shot only (audit [C-1]).
fn guard_single_shot(ctx: &CryptoContext) -> Result<(), BYOKError> {
    if !ctx.is_single_shot() {
        return Err(BYOKError::ChunkedConvergentUnsupported {
            chunk_index: ctx.chunk_index,
        });
    }
    Ok(())
}

/// Deterministically derive the convergent DEK from `(tcs, ctx)`.
///
/// `HKDF-SHA256(ikm = tcs, salt = tenant_id, info = framed(DEK_LABEL, JCS(ctx)))`.
///
/// # Errors
///
/// - [`BYOKError::ChunkedConvergentUnsupported`] if `ctx.chunk_index != 0`.
/// - [`BYOKError::EnvelopeError`] on JCS or HKDF failure.
pub fn derive_dek_convergent(tcs: &Tcs, ctx: &CryptoContext) -> Result<Dek, BYOKError> {
    guard_single_shot(ctx)?;
    let jcs = ctx.to_jcs_bytes()?;
    let hk = Hkdf::<Sha256>::new(Some(ctx.tenant_id.as_bytes()), &tcs.bytes);
    let mut okm = [0u8; 32];
    hk.expand(&framed_info(DEK_LABEL, &jcs), &mut okm)
        .map_err(|e| BYOKError::EnvelopeError(format!("HKDF expand dek: {e}")))?;
    Ok(Dek { bytes: okm })
}

/// Deterministically derive the convergent 96-bit nonce from `(tcs, ctx)`.
///
/// Uses a SEPARATE HKDF `info` label ([`NONCE_LABEL`]) so the nonce is NOT the
/// same digest material as the DEK (audit [C-2] domain separation).
///
/// # Errors
///
/// - [`BYOKError::ChunkedConvergentUnsupported`] if `ctx.chunk_index != 0`.
/// - [`BYOKError::EnvelopeError`] on JCS or HKDF failure.
pub fn derive_nonce_convergent(tcs: &Tcs, ctx: &CryptoContext) -> Result<[u8; 12], BYOKError> {
    guard_single_shot(ctx)?;
    let jcs = ctx.to_jcs_bytes()?;
    let hk = Hkdf::<Sha256>::new(Some(ctx.tenant_id.as_bytes()), &tcs.bytes);
    let mut nonce = [0u8; 12];
    hk.expand(&framed_info(NONCE_LABEL, &jcs), &mut nonce)
        .map_err(|e| BYOKError::EnvelopeError(format!("HKDF expand nonce: {e}")))?;
    Ok(nonce)
}

/// Convergent (Mode A) encrypt of a WHOLE object.
///
/// Deterministic: `(tcs, ctx, plaintext)` → identical `(ciphertext, nonce)`.
/// The body AEAD binds `aad = JCS(ctx)` (audit [H-2]).
///
/// # Errors
///
/// - [`BYOKError::ChunkedConvergentUnsupported`] if `ctx.chunk_index != 0`.
/// - [`BYOKError::AesGcm`] on AEAD failure; [`BYOKError::EnvelopeError`] on
///   JCS / HKDF failure.
pub fn encrypt_convergent(
    plaintext: &[u8],
    tcs: &Tcs,
    ctx: &CryptoContext,
) -> Result<ConvergentBlob, BYOKError> {
    guard_single_shot(ctx)?;
    let dek = derive_dek_convergent(tcs, ctx)?;
    let nonce_bytes = derive_nonce_convergent(tcs, ctx)?;
    let aad = ctx.to_jcs_bytes()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek.bytes));
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|e| BYOKError::AesGcm(e.to_string()))?;
    Ok(ConvergentBlob {
        ciphertext,
        nonce: nonce_bytes,
    })
}

/// Convergent (Mode A) decrypt. Re-derives the DEK from `(tcs, ctx)` and binds
/// `aad = JCS(ctx)`; any context divergence fails closed (wrong DEK and/or AAD
/// tag mismatch).
///
/// # M-1 (zeroize)
///
/// The returned `Vec<u8>` is plaintext and is CALLER-OWNED — the caller is
/// responsible for zeroizing it after use. The internal DEK ([`Dek`]) is
/// `ZeroizeOnDrop` and is cleared when this function returns.
///
/// # Errors
///
/// - [`BYOKError::ChunkedConvergentUnsupported`] if `ctx.chunk_index != 0`.
/// - [`BYOKError::AesGcm`] on AEAD / tag failure; [`BYOKError::EnvelopeError`]
///   on JCS / HKDF failure.
pub fn decrypt_convergent(
    blob: &ConvergentBlob,
    tcs: &Tcs,
    ctx: &CryptoContext,
) -> Result<Vec<u8>, BYOKError> {
    guard_single_shot(ctx)?;
    let dek = derive_dek_convergent(tcs, ctx)?;
    let aad = ctx.to_jcs_bytes()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek.bytes));
    cipher
        .decrypt(
            Nonce::from_slice(&blob.nonce),
            Payload {
                msg: blob.ciphertext.as_ref(),
                aad: &aad,
            },
        )
        .map_err(|e| BYOKError::AesGcm(e.to_string()))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;
    use crate::byok_core::context::{CryptoAlgo, CryptoMode};
    use proptest::prelude::*;

    fn ctx(tenant: &str, digest: &str) -> CryptoContext {
        CryptoContext::new_single_shot(
            tenant,
            digest,
            CryptoAlgo::Aes256Gcm,
            "ns",
            CryptoMode::Convergent,
            "cas",
            "arn:aws:kms:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000",
            16,
        )
    }

    fn tcs_of(seed: u8) -> Tcs {
        Tcs::from_bytes([seed; 32])
    }

    #[test]
    fn convergence_is_deterministic_dek_nonce_and_ciphertext() {
        let tcs = tcs_of(7);
        let c = ctx("tenant_a", "sha256:abc");
        let pt = b"identical plaintext";

        let dek1 = derive_dek_convergent(&tcs, &c).unwrap();
        let dek2 = derive_dek_convergent(&tcs, &c).unwrap();
        assert_eq!(dek1.bytes, dek2.bytes, "DEK must be deterministic");

        let n1 = derive_nonce_convergent(&tcs, &c).unwrap();
        let n2 = derive_nonce_convergent(&tcs, &c).unwrap();
        assert_eq!(n1, n2, "nonce must be deterministic");

        let b1 = encrypt_convergent(pt, &tcs, &c).unwrap();
        let b2 = encrypt_convergent(pt, &tcs, &c).unwrap();
        assert_eq!(b1.nonce, b2.nonce, "convergent nonce must match");
        assert_eq!(
            b1.ciphertext, b2.ciphertext,
            "convergent ciphertext must be identical (dedup)"
        );
    }

    #[test]
    fn convergent_roundtrip() {
        let tcs = tcs_of(3);
        let c = ctx("tenant_a", "sha256:xyz");
        let pt = b"the quick brown fox";
        let blob = encrypt_convergent(pt, &tcs, &c).unwrap();
        let out = decrypt_convergent(&blob, &tcs, &c).unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn different_context_fields_change_the_dek() {
        let tcs = tcs_of(9);
        let base = ctx("tenant_a", "sha256:abc");
        let base_dek = derive_dek_convergent(&tcs, &base).unwrap().bytes;

        // tenant
        let mut v = base.clone();
        v.tenant_id = "tenant_b".into();
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
        // namespace
        let mut v = base.clone();
        v.namespace = "other".into();
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
        // mode
        let mut v = base.clone();
        v.mode = CryptoMode::Random;
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
        // version
        let mut v = base.clone();
        v.version = 99;
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
        // digest
        let mut v = base.clone();
        v.plaintext_digest = "sha256:zzz".into();
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
        // surface
        let mut v = base.clone();
        v.surface = "ac".into();
        assert_ne!(derive_dek_convergent(&tcs, &v).unwrap().bytes, base_dek);
    }

    #[test]
    fn nonce_varies_with_context_and_is_not_constant() {
        // Kills constant-nonce mutants (`Ok([0;12])` / `Ok([1;12])`) AND any
        // context-independent constant: different contexts → different nonces.
        let tcs = tcs_of(11);
        let a = ctx("tenant_a", "sha256:aaa");
        let mut b = a.clone();
        b.plaintext_digest = "sha256:bbb".into();
        let na = derive_nonce_convergent(&tcs, &a).unwrap();
        let nb = derive_nonce_convergent(&tcs, &b).unwrap();
        assert_ne!(na, nb, "different contexts must yield different nonces");
        assert_ne!(na, [0u8; 12], "nonce must not be all-zeros");
        assert_ne!(na, [1u8; 12], "nonce must not be all-ones");
        assert_ne!(nb, [0u8; 12], "nonce must not be all-zeros");
        assert_ne!(nb, [1u8; 12], "nonce must not be all-ones");
    }

    #[test]
    fn different_tcs_changes_the_dek() {
        let c = ctx("tenant_a", "sha256:abc");
        let a = derive_dek_convergent(&tcs_of(1), &c).unwrap().bytes;
        let b = derive_dek_convergent(&tcs_of(2), &c).unwrap().bytes;
        assert_ne!(a, b);
    }

    #[test]
    fn hkdf_domain_separation_dek_ne_nonce_material() {
        // Derive a full 32-byte block under the nonce label and compare to the
        // DEK — they must be independent material, not a prefix relationship.
        let tcs = tcs_of(5);
        let c = ctx("tenant_a", "sha256:abc");
        let dek = derive_dek_convergent(&tcs, &c).unwrap().bytes;
        let nonce = derive_nonce_convergent(&tcs, &c).unwrap();
        assert_ne!(
            &dek[..12],
            &nonce[..],
            "nonce must not be a prefix of the DEK (distinct HKDF labels)"
        );

        let jcs = c.to_jcs_bytes().unwrap();
        let hk = Hkdf::<Sha256>::new(Some(c.tenant_id.as_bytes()), &tcs.bytes);
        let mut nonce_block = [0u8; 32];
        hk.expand(&framed_info(NONCE_LABEL, &jcs), &mut nonce_block)
            .unwrap();
        assert_ne!(dek, nonce_block, "DEK and nonce-domain block must differ");
    }

    #[test]
    fn convergent_tamper_fails_closed() {
        let tcs = tcs_of(8);
        let c = ctx("tenant_a", "sha256:abc");
        let blob = encrypt_convergent(b"secret", &tcs, &c).unwrap();
        let mut tampered = c.clone();
        tampered.namespace = "evil".into();
        assert!(decrypt_convergent(&blob, &tcs, &tampered).is_err());
    }

    #[test]
    fn chunked_context_is_rejected() {
        let tcs = tcs_of(4);
        let mut c = ctx("tenant_a", "sha256:abc");
        c.chunk_index = 1;
        assert!(matches!(
            derive_dek_convergent(&tcs, &c),
            Err(BYOKError::ChunkedConvergentUnsupported { chunk_index: 1 })
        ));
        assert!(matches!(
            encrypt_convergent(b"x", &tcs, &c),
            Err(BYOKError::ChunkedConvergentUnsupported { chunk_index: 1 })
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn prop_convergent_roundtrip(
            plaintext in proptest::collection::vec(0u8..=255, 0..256),
            tenant in "[a-z]{3,8}",
            digest in "[0-9a-f]{16}",
            seed in any::<u8>(),
        ) {
            let tcs = Tcs::from_bytes([seed; 32]);
            let c = ctx(&tenant, &format!("sha256:{digest}"));
            let blob = encrypt_convergent(&plaintext, &tcs, &c).unwrap();
            // Determinism: a second encrypt is byte-identical.
            let blob2 = encrypt_convergent(&plaintext, &tcs, &c).unwrap();
            prop_assert_eq!(&blob.ciphertext, &blob2.ciphertext);
            prop_assert_eq!(blob.nonce, blob2.nonce);
            let out = decrypt_convergent(&blob, &tcs, &c).unwrap();
            prop_assert_eq!(out, plaintext);
        }

        #[test]
        fn prop_distinct_tenant_distinct_dek(
            digest in "[0-9a-f]{16}",
            seed in any::<u8>(),
        ) {
            let tcs = Tcs::from_bytes([seed; 32]);
            let ca = ctx("tenant_a", &format!("sha256:{digest}"));
            let cb = ctx("tenant_b", &format!("sha256:{digest}"));
            let da = derive_dek_convergent(&tcs, &ca).unwrap().bytes;
            let db = derive_dek_convergent(&tcs, &cb).unwrap().bytes;
            prop_assert_ne!(da, db);
        }
    }
}
