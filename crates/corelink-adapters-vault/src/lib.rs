//! `corelink-adapters-vault` — canonical Vault-adapters surface for
//! the CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream C sub-step C.4 lands this crate as the
//! **single import target** for the HTTPS / mTLS binding portion of
//! the HashiCorp Vault BYOK adapter:
//!
//! ```text
//! use corelink_adapters_vault::vault::*;   // Vault Transit HTTPS/mTLS client
//! ```
//!
//! ## Stage 1 Stream C absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream C, this crate "absorbs" the **binding portion** of
//! `corelink-byok-vault` by re-exporting it at the canonical
//! submodule path. The pure-logic portion of `corelink-byok-vault` is
//! simultaneously re-exported by `corelink_byok::vault` (Stream B
//! sub-step B.2b — BYOK microkernel umbrella, gated behind the
//! `vault` cargo feature in `corelink-byok`).
//!
//! The **physical decomposition** between pure-logic and HTTPS /
//! mTLS portions is **deferred to Stage 2** (consumer-migration
//! stream that owns the `apps/server` BYOK wiring atomically).
//!
//! ### Absorbed crate — binding portion (1)
//!
//! - [`vault`] ← `corelink-byok-vault` — HashiCorp Vault Transit BYOK
//!   adapter implementing `KmsProvider` (WI-S14-005). Customer-hosted
//!   Vault Enterprise FIPS 140-3 Level 1 build; NIST CMVP
//!   certification pending. Authentication: mTLS (client cert PEM +
//!   CA cert PEM); both stored as `SecretString` on the customer
//!   config row.
//!
//! ### HuGR Wallet broker sibling (Wave-31 follow-up)
//!
//! The HuGR Wallet broker adapter (a sibling provider — wallet-backed
//! BYOK with co-signed envelope) lands separately in a Wave-31
//! follow-up WI. When it lands it will be added here as a sibling
//! submodule `wallet`.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of `corelink-byok-vault` remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken.
//!
//! ## Charter compliance (preserved by reference)
//!
//! - `SecretString` on Vault client-cert PEM bytes + CA-cert PEM bytes
//!   — preserved by reference.
//! - `subtle::ConstantTimeEq` on Vault Transit AAD compare —
//!   preserved by reference.
//! - Audit-emit-BEFORE-mutation fail-CLOSED envelopes on every
//!   `KmsProvider` operation — preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — inherited via
//!   re-export.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod vault;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke test proving the canonical re-export path resolves at
    //! compile time. No new state is introduced.

    #[test]
    fn vault_path_resolves() {
        #[allow(unused_imports)]
        use crate::vault as _v;
    }
}
