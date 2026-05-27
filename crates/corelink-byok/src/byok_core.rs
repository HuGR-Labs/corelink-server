//! CoreLink BYOK (Bring Your Own Key) enterprise tier — trait surfaces, types, DEK cache,
//! and envelope encryption.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  EnvelopeEncryptor                                              │
//! │  write: gen DEK (CSPRNG) → AES-256-GCM body → KMS wrap → D1+R2 │
//! │  read:  D1 fetch → DEK cache (5 min TTL hard) → KMS unwrap      │
//! │         → AES-256-GCM decrypt                                   │
//! └───────────────────────┬─────────────────────────────────────────┘
//!                         │ KmsProvider trait
//!           ┌─────────────┼────────────────────────────┐
//!           ▼             ▼             ▼               ▼
//!      AwsKmsProvider  GcpKmsProvider  AzureKvProvider  VaultProvider
//!      (WI-S14-004)    (WI-S14-005)   (WI-S14-005)    (WI-S14-005)
//! ```
//!
//! # Invariant: INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL)
//!
//! Customer revokes CMK → cache inaccessible ≤ 5 min global.
//! [`DekCache`] constructor enforces TTL ≤ 300 s (5 min) hard — no exception.
//!
//! # Security properties
//!
//! - DEK generated via `getrandom` (OS-backed CSPRNG; NIST SP 800-90A approved DRBG).
//!   **NOT** BLAKE3-derived deterministic — deterministic DEK = compromise propagation.
//! - AES-256-GCM nonce: 96-bit random per-write (FIPS 197 + NIST SP 800-38D).
//! - AAD (`encryption_context`): bound `{tenant_id, blob_hash}` — attacker cannot swap
//!   wrapped DEKs cross-blob.
//! - [`Dek`]: `ZeroizeOnDrop` — memory cleared when dropped.
//! - No `Dek` material in logs / traces / errors.
//!
//! # Wave-35 Phase 2 absorption note
//!
//! This module was the standalone `corelink-byok-core` crate prior to
//! the Wave 35 Phase 2 BYOK umbrella absorption. The crate-level
//! `#![forbid(unsafe_code)]` attribute is now declared once at the
//! umbrella's `lib.rs` (inner attributes on a non-root module are
//! illegal). All other behaviour is unchanged.

pub mod dek_cache;
pub mod envelope;
pub mod types;

pub use dek_cache::DekCache;
pub use envelope::EnvelopeEncryptor;
pub use types::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek,
};

use async_trait::async_trait;

/// KMS provider trait — implemented by each cloud adapter.
///
/// # Contract
///
/// - All methods are network calls; p99 latency ≤ 30 ms (region-co-located).
/// - `wrap_dek` / `unwrap_dek` MUST use `encryption_context` as AAD binding
///   `{tenant_id, blob_hash}` to prevent cross-blob swap attacks.
/// - `check_access` is called every 60 s in background per active BYOK tenant
///   (WI-S14-006 kill switch path).
#[async_trait]
pub trait KmsProvider: Send + Sync {
    /// Returns the provider variant.
    fn provider_kind(&self) -> KmsProviderKind;

    /// Returns the AWS/GCP/Azure/Vault region this provider is co-located with.
    fn region(&self) -> &str;

    /// Returns the FIPS compliance level for this provider instance.
    fn fips_level(&self) -> FipsLevel;

    /// Wrap (encrypt) a DEK via KMS.
    ///
    /// - `encryption_context`: mandatory AAD `{"tenant_id": "...", "blob_hash": "..."}`.
    /// - Returns [`WrappedDek`] stored in D1.
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError>;

    /// Unwrap (decrypt) a wrapped DEK via KMS.
    ///
    /// - The `encryption_context` in `wrapped` must match what was used during `wrap_dek`.
    /// - Result is cached in [`DekCache`] with 5 min TTL hard.
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError>;

    /// Check whether CMK access is still granted.
    ///
    /// Called every 60 s in background; revoked → kill switch path (WI-S14-006).
    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError>;
}
