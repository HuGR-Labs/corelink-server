//! `corelink-crypto` — canonical cryptography surface for the CoreLink
//! Rust workspace.
//!
//! Wave-33 Stage 0 sub-step 2 lands this crate as the **single import
//! target** Stage 1 streams will use for every cryptographic primitive:
//!
//! ```text
//! use corelink_crypto::blake3::Digest;           // BLAKE3 content-addressing
//! use corelink_crypto::ed25519::attestation::*;  // Ed25519 erasure proofs
//! use corelink_crypto::client_verify::*;         // Client-side SDK verifier
//! use corelink_crypto::hmac::*;                  // HMAC-SHA-256 primitives
//! use corelink_crypto::ct_eq::*;                 // Constant-time compare
//! ```
//!
//! ## Stage 0 absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` §6 sub-step
//! 2, this crate "absorbs" 3 existing crates. The Stage 0 audit doc
//! `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md` §4 documents the
//! deliberate choice to land sub-step 2 as a re-export aggregator
//! rather than a physical source move:
//!
//! - `corelink-hash` (BLAKE3 + Digest + VerifiedBody) — re-exported at
//!   [`blake3`].
//! - `corelink-erasure-attestation` (Ed25519 FIPS 186-5 EdDSA signing
//!   for BYOK DSR erasure) — re-exported at [`ed25519::attestation`].
//! - `corelink-client-verify` (client-side BLAKE3 mismatch detection +
//!   FFI cdylib for SDK wrappers) — re-exported at [`client_verify`].
//!
//! Why aggregator rather than physical move:
//!
//! - `corelink-client-verify` ships a `cdylib` + `staticlib` consumed
//!   by `corelink-go` / `corelink-py` SDK wrappers through `cbindgen`
//!   (`cbindgen.toml` with `parse_deps = false` + `include =
//!   ["corelink-client-verify"]`). Physically relocating the FFI
//!   module into `corelink-crypto` requires either (a) re-running
//!   cbindgen with extended `parse_deps`, or (b) atomically updating
//!   the Go cgo and Python pyO3 `path = "../corelink-client-verify"`
//!   references plus the C header at
//!   `crates/corelink-client-verify/include/corelink_client_verify.h`.
//!   That is precisely the kind of cross-stream coordination Stage 1's
//!   FFI / SDK stream should own atomically — not a Stage 0 task. This
//!   is hard-pause-trigger-1 (charter §7) partial activation; flagged
//!   in `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md` §7.
//! - `corelink-hash` benches (`benches/blake3.rs`, `blake3_bench.rs`),
//!   examples (`examples/blake3_vectors.rs`), tests (`tests/*.rs`), and
//!   fuzz targets (`fuzz/`) reference internal-crate types. A physical
//!   move would either require updating every internal use to
//!   `corelink_crypto::blake3::*` (a 5-file public-API churn) or
//!   keeping the shim's stale fuzz/bench harness wired through
//!   re-exports — both fragile. The aggregator pattern lets Stage 1's
//!   data-path stream do the rename atomically when consumers migrate.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 3 absorbed crates remains reachable at
//! its original path AND at the new canonical path:
//!
//! - `corelink_hash::Digest` ≡ `corelink_crypto::blake3::Digest`.
//! - `corelink_erasure_attestation::*` ≡
//!   `corelink_crypto::ed25519::attestation::*`.
//! - `corelink_client_verify::*` ≡ `corelink_crypto::client_verify::*`.
//!
//! No public-API contract is broken. Stage 1 streams MAY adopt the new
//! canonical paths incrementally without coordination cost.
//!
//! ## What is NOT re-exported
//!
//! The `hmac` and `ct_eq` submodules are intentionally thin: they
//! surface the upstream RustCrypto primitives (`hmac::Hmac`,
//! `sha2::Sha256`, `subtle::ConstantTimeEq`) under the canonical
//! `corelink_crypto::*` namespace so Stage 1 streams have a stable
//! import path for primitives that previously lived inline in each
//! consumer crate. No new abstractions are introduced.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod blake3;
pub mod client_verify;
pub mod ct_eq;
pub mod ed25519;
pub mod hmac;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time AND that the re-exported types preserve their
    //! semantics (no silent shim breakage).

    #[test]
    fn blake3_digest_path_resolves_and_round_trips() {
        // Path 1: canonical wave-33 import surface.
        let d = crate::blake3::Digest::compute(b"hello");
        // Path 2: original `corelink-hash` path still works (preserved
        // by the aggregator pattern; Stage 1 migration is incremental).
        let d2 = corelink_hash::Digest::compute(b"hello");
        assert_eq!(d, d2);
        // Hex round-trip survives the re-export.
        let hex = d.to_hex();
        assert_eq!(hex.len(), 64);
        let back = crate::blake3::Digest::from_hex(&hex).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn ed25519_attestation_path_resolves() {
        // Surface a constant from the re-exported module to prove the
        // path resolves at compile time. No new state introduced.
        let _ = std::any::TypeId::of::<crate::ed25519::attestation::Region>();
    }

    #[test]
    fn client_verify_path_resolves() {
        // Smoke: the `ClientVerifier` type is re-exported.
        let _ = std::any::TypeId::of::<crate::client_verify::ClientVerifier>();
    }

    #[test]
    fn hmac_sha256_alias_works() {
        use crate::hmac::{HmacSha256, Mac};
        let mut mac = HmacSha256::new_from_slice(b"key").unwrap();
        mac.update(b"data");
        let out = mac.finalize().into_bytes();
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn ct_eq_path_resolves() {
        use crate::ct_eq::ConstantTimeEq;
        let a = [1u8, 2, 3];
        let b = [1u8, 2, 3];
        let c = [1u8, 2, 4];
        assert!(bool::from(a.ct_eq(&b)));
        assert!(!bool::from(a.ct_eq(&c)));
    }
}
