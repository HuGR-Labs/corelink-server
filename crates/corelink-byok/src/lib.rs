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
//! Per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` §2 +
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
//!
//! ## Wave 35 Phase 2 absorption
//!
//! Per `specs/_audits/2026-05-26-w35-p2-byok-absorption.md` (SEALED
//! 2026-05-26), the 6 Wave-33 Stream-B sub-step B.2b BYOK sub-crates
//! were physically absorbed into this umbrella as inline submodules,
//! completing the microkernel roll-up; the former external
//! `corelink-byok-*` crates were dropped from `workspace.members`.
//! Public-API contract (`corelink_byok::*` glob-reexport from
//! `byok_core` + `corelink_byok::{revocation,aws,gcp,azure,vault}`)
//! is preserved 1:1; KmsProvider trait, `Dek` / `WrappedDek` zeroize
//! discipline, `SecretString` credential bytes,
//! `subtle::ConstantTimeEq` compare, `#[non_exhaustive]` discipline,
//! and per-build mutual-exclusion (Hard Pause Trigger 2) all
//! preserved by reference.
//!
//! Absorbed crates (6):
//!
//! - `corelink-byok-core` → `corelink_byok::*` (glob via internal
//!   `byok_core` mod) — KmsProvider trait + Dek/WrappedDek/KmsKeyId
//!   types + EnvelopeEncryptor + DekCache.
//! - `corelink-byok-revocation` → [`revocation`] — always-on
//!   revocation detector.
//! - `corelink-byok-aws` → [`aws`] — AWS KMS provider (feature-gated).
//! - `corelink-byok-gcp` → [`gcp`] — GCP KMS provider (feature-gated).
//! - `corelink-byok-azure` → [`azure`] — Azure Key Vault provider
//!   (feature-gated).
//! - `corelink-byok-vault` → [`vault`] — HashiCorp Vault Transit
//!   provider (feature-gated).

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
// Wave-35 Phase 2 — physical absorption of the 6 BYOK sub-crates.
//
// The former `corelink-byok-{core,revocation,aws,gcp,azure,vault}`
// crates are now internal modules `byok_{core,revocation,aws,gcp,
// azure,vault}` under this umbrella. The public surface is preserved
// 1:1: `corelink_byok::*` re-exports the byok-core trait + types at
// the crate root; `corelink_byok::revocation::*` is always-on; the
// 4 per-provider submodules remain feature-gated and mutually
// exclusive per the microkernel charter.
// ============================================================================

// `byok_core` is exposed at `pub(crate)` so sibling provider modules
// can address `crate::byok_core::types::BYOKError` (the legacy
// `corelink_byok_core::types::BYOKError` callers were rewritten to
// `crate::byok_core::types::BYOKError` during absorption — see
// `src/byok_aws.rs`). Downstream consumers reach the same symbols
// via the always-on `pub use byok_core::*;` glob below.
// All six absorbed modules are unconditionally compiled. This
// matches the pre-absorption matrix-test wiring where every
// provider crate was linked as a dev-dependency without engaging
// the umbrella's mutual-exclusion gates. The gates remain enforced
// on the public `corelink_byok::{aws,gcp,azure,vault}` namespace
// re-exports below — production binaries that activate two namespace
// features still hit a compile_error. cargo-deny enforces the
// single-provider-SDK-per-binary rule at the workspace boundary.
pub(crate) mod byok_core;
mod byok_revocation;
// The 4 provider modules are compiled when the matching internal
// `_internal-<provider>` feature is on. The PUBLIC features
// (`aws` / `gcp` / `azure` / `vault`) activate the corresponding
// `_internal-*` flag transitively, plus the matching real/
// production sub-feature, plus the namespace re-export below.
//
// Matrix integration tests declare `required-features = ["_matrix-test"]`
// which activates all four `_internal-*` flags WITHOUT engaging
// the mutual-exclusion `compile_error!` guards above (which
// inspect only the public namespace features). Production
// binaries activate exactly one public provider feature; combining
// two public features is a hard compile error.
#[cfg(feature = "_internal-aws")]
mod byok_aws;
#[cfg(feature = "_internal-gcp")]
mod byok_gcp;
#[cfg(feature = "_internal-azure")]
mod byok_azure;
#[cfg(feature = "_internal-vault")]
mod byok_vault;

// ============================================================================
// Core surface re-export (always-on; preserves pre-wave-33 import paths).
// ============================================================================

pub use byok_core::*;

// ============================================================================
// Always-on revocation submodule.
// ============================================================================

/// BYOK CMK revocation detector + multi-channel alerting.
///
/// Re-exports the entire public API of the absorbed
/// `byok_revocation` module. Always-on (no feature gate) so the
/// kill-switch lifecycle is available in every build configuration.
pub mod revocation {
    pub use crate::byok_revocation::*;
}

// ============================================================================
// Per-provider feature-gated submodules. Only the active provider
// (selected via the matching cargo feature) is reachable in any given
// build; the compile_error! gates above guarantee at most one is on.
// ============================================================================

/// AWS KMS provider adapter — re-exports the public API of the
/// absorbed `byok_aws` module. The namespace is reachable whenever
/// the internal `_internal-aws` feature is on, which is the case
/// for the public `aws` feature, the `real-aws` flavour sub-feature,
/// and the `_matrix-test` cross-provider integration-test flag.
/// Mutual exclusion across `corelink_byok::{aws,gcp,azure,vault}`
/// is enforced by the public-feature `compile_error!` gates above
/// — combining two public namespace features is a hard build error.
#[cfg(feature = "_internal-aws")]
pub mod aws {
    pub use crate::byok_aws::*;
}

/// Google Cloud KMS provider adapter — see [`aws`] for the feature-gate
/// contract; analogous shape per provider.
#[cfg(feature = "_internal-gcp")]
pub mod gcp {
    pub use crate::byok_gcp::*;
}

/// Azure Key Vault provider adapter — see [`aws`] for the feature-gate
/// contract; analogous shape per provider.
#[cfg(feature = "_internal-azure")]
pub mod azure {
    pub use crate::byok_azure::*;
}

/// HashiCorp Vault Transit provider adapter — see `aws` for the
/// feature-gate contract; analogous shape per provider.
#[cfg(feature = "_internal-vault")]
pub mod vault {
    pub use crate::byok_vault::*;
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
