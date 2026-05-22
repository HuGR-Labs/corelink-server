//! `corelink-byok` — canonical BYOK microkernel umbrella for the
//! CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream B sub-step B.2b lands this crate as the
//! **single import target** for every BYOK (Bring Your Own Key)
//! primitive. Two responsibilities:
//!
//! 1. **Crate-root re-export of `corelink-byok-core`** — every public
//!    symbol (`KmsProvider`, `Dek`, `WrappedDek`, `KmsKeyId`,
//!    `EnvelopeEncryptor`, `BYOKError`, etc.) is reachable at
//!    `corelink_byok::*` so consumer call sites that used the
//!    pre-wave-33 `use corelink_byok::KmsProvider;` continue to
//!    compile unchanged. The behaviour-preserving refactor charter
//!    rule is honoured by this surface preservation.
//! 2. **Microkernel provider gating** — the four cloud providers are
//!    exposed as cargo features (`aws` / `gcp` / `azure` / `vault`).
//!    AT MOST ONE may be active in any given build; multi-provider
//!    builds are rejected at compile time by the
//!    [`compile_error!`] guard below (preserves the wave-33 reorg
//!    spec §3 microkernel contract + charter Hard Pause Trigger 2).
//!
//! ## Why microkernel
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §2 +
//! decision matrix, BYOK is a "microkernel pattern" surface: a thin
//! core (`corelink-byok-core` ships the `KmsProvider` trait + types +
//! envelope encryption + `DekCache`), and exactly ONE provider plugin
//! per deployment realises the trait against a real KMS (AWS / GCP /
//! Azure / HashiCorp Vault). Compile-time selection eliminates
//! runtime branching on the hot crypto path and lets cargo-deny
//! enforce the "only one provider SDK linked per build" rule (Stage 3
//! lockdown).
//!
//! ## Cargo feature reference
//!
//! - `aws` → adds `corelink-byok-aws` (AWS KMS via aws-sdk-kms).
//! - `gcp` → adds `corelink-byok-gcp` (Google Cloud KMS via
//!   google-cloud-kms).
//! - `azure` → adds `corelink-byok-azure` (Azure Key Vault).
//! - `vault` → adds `corelink-byok-vault` (HashiCorp Vault Transit).
//!
//! At most ONE may be set. Default = no provider (trait + InMemoryFake
//! only; matches pre-wave-33 default build for test/CI scenarios).
//!
//! ## `corelink-byok-revocation`
//!
//! Always-on. The revocation detector composes with whichever provider
//! is enabled (or with InMemoryFake providers in test wiring) and is
//! re-exported at the [`revocation`] submodule for callers that need
//! the explicit namespace.
//!
//! ## Charter compliance
//!
//! - `#![forbid(unsafe_code)]` — preserved.
//! - No `unwrap()`/`expect()`/`panic!()` in production code paths
//!   (the umbrella has no production code paths beyond `pub use`).
//! - `subtle::ConstantTimeEq` — preserved by reference (lives in
//!   `corelink-byok-core::types`).
//! - `SecretString` for credential bytes — preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — re-exports
//!   inherit the attribute from the source crate.
//! - BYOK mutual-exclusion at compile-time — enforced below.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

// ============================================================================
// Microkernel mutual-exclusion enforcement (wave-33 §3 + Hard Pause Trigger 2).
//
// Compile-time rejection of multi-provider builds. The combinations
// enumerated below cover every pair of `(provider_a, provider_b)`
// drawn from `{aws, gcp, azure, vault}`; any triple/quadruple
// implies at least one pair is hit, so pairs are sufficient.
// ============================================================================

#[cfg(all(feature = "aws", feature = "gcp"))]
compile_error!(
    "BYOK microkernel: features `aws` and `gcp` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

#[cfg(all(feature = "aws", feature = "azure"))]
compile_error!(
    "BYOK microkernel: features `aws` and `azure` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

#[cfg(all(feature = "aws", feature = "vault"))]
compile_error!(
    "BYOK microkernel: features `aws` and `vault` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

#[cfg(all(feature = "gcp", feature = "azure"))]
compile_error!(
    "BYOK microkernel: features `gcp` and `azure` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

#[cfg(all(feature = "gcp", feature = "vault"))]
compile_error!(
    "BYOK microkernel: features `gcp` and `vault` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

#[cfg(all(feature = "azure", feature = "vault"))]
compile_error!(
    "BYOK microkernel: features `azure` and `vault` are mutually exclusive. \
     Select exactly one provider per build (wave-33 reorg spec §3)."
);

// ============================================================================
// Core surface re-export (always-on; preserves pre-wave-33 import paths).
// ============================================================================

pub use corelink_byok_core::*;

// ============================================================================
// Always-on revocation submodule.
// ============================================================================

/// BYOK CMK revocation detector + multi-channel alerting.
///
/// Re-exports the entire public API of `corelink-byok-revocation`.
/// Always-on (no feature gate) so the kill-switch lifecycle is
/// available in every build configuration.
pub mod revocation {
    pub use corelink_byok_revocation::*;
}

// ============================================================================
// Per-provider feature-gated submodules. Only the active provider
// (selected via the matching cargo feature) is reachable in any given
// build; the compile_error! gates above guarantee at most one is on.
// ============================================================================

/// AWS KMS provider adapter — gated by `aws` cargo feature.
///
/// Re-exports the entire public API of `corelink-byok-aws`.
#[cfg(feature = "aws")]
pub mod aws {
    pub use corelink_byok_aws::*;
}

/// Google Cloud KMS provider adapter — gated by `gcp` cargo feature.
///
/// Re-exports the entire public API of `corelink-byok-gcp`.
#[cfg(feature = "gcp")]
pub mod gcp {
    pub use corelink_byok_gcp::*;
}

/// Azure Key Vault provider adapter — gated by `azure` cargo feature.
///
/// Re-exports the entire public API of `corelink-byok-azure`.
#[cfg(feature = "azure")]
pub mod azure {
    pub use corelink_byok_azure::*;
}

/// HashiCorp Vault Transit provider adapter — gated by `vault`
/// cargo feature.
///
/// Re-exports the entire public API of `corelink-byok-vault`.
#[cfg(feature = "vault")]
pub mod vault {
    pub use corelink_byok_vault::*;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving the umbrella surface is wired correctly.
    //! Provider-gated tests live in `tests/matrix.rs` (folded from
    //! the wave-33 sub-step B.2c absorbed `corelink-byok-matrix-test`).

    #[test]
    fn core_reexport_kms_provider_trait_resolves() {
        // `use corelink_byok::KmsProvider;` (the pre-wave-33 path)
        // must continue to compile. We assert the trait object id
        // resolves through the umbrella re-export.
        fn _accept_kms_provider<P: crate::KmsProvider + ?Sized>(_p: &P) {}
        // No instance needed — the bound is checked at type-id level.
        let _ = std::any::TypeId::of::<dyn crate::KmsProvider>();
    }

    #[test]
    fn core_reexport_envelope_types_resolve() {
        // Dek + WrappedDek + KmsKeyId are core envelope-encryption
        // types every consumer touches; smoke-test re-export.
        let _ = std::any::TypeId::of::<crate::Dek>();
        let _ = std::any::TypeId::of::<crate::WrappedDek>();
        let _ = std::any::TypeId::of::<crate::KmsKeyId>();
    }

    #[test]
    fn revocation_submodule_resolves() {
        // The revocation submodule is always-on regardless of which
        // provider feature is selected. Smoke: the namespace is
        // reachable through the umbrella.
        #[allow(unused_imports)]
        use crate::revocation as _r;
    }

    /// Compile-time sanity: with no provider feature set (default
    /// build), none of the provider submodules are reachable. The
    /// test exists in two flavours: under the default build (no
    /// feature) and under a single-feature build. The presence of
    /// this test in `cargo test -p corelink-byok` (default features)
    /// proves the `cfg(not(any(...)))` path was taken.
    #[cfg(not(any(
        feature = "aws",
        feature = "gcp",
        feature = "azure",
        feature = "vault"
    )))]
    #[test]
    fn no_provider_feature_default_build() {
        // Use the kms-provider trait object's type-id as a structural
        // assertion — its presence here proves the core re-export is
        // wired AND the default-feature gate is hit (this test is
        // compiled out under any single-feature build).
        let core_marker = std::any::TypeId::of::<dyn crate::KmsProvider>();
        let any_marker = std::any::TypeId::of::<&dyn std::any::Any>();
        assert_ne!(
            core_marker, any_marker,
            "KmsProvider trait re-export must be distinct from Any"
        );
    }
}
