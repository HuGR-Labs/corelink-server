//! CoreLink erasure attestation — Ed25519 (FIPS 186-5 EdDSA) signing per-region.
//!
//! # Purpose
//!
//! When a BYOK tenant files a DSR erasure request (S-11), CoreLink:
//!
//! 1. Destroys CMK access via the KMS provider API.
//! 2. Emits a cryptographically signed attestation proving the destroy occurred.
//! 3. Persists the attestation to an R2 audit bucket with 7-year retention.
//! 4. Indexes it in D1 so the public verify endpoint can look it up by `request_id`.
//!
//! The signed attestation is delivered to the customer and is independently
//! verifiable offline using the public key endpoint
//! `GET /v1/public/keys/erasure/{region}.pub`.
//!
//! # Standards
//!
//! - **FIPS 186-5 EdDSA Ed25519** — digital signature standard.
//! - **RFC 8785 JCS** — deterministic JSON canonicalization for signature
//!   stability (`serde_jcs`).
//! - **NIST SP 800-88 Rev.1 §2.4** — crypto-erase mode; `evidence_hash`
//!   binds audit chain segment IDs + KMS destroy timestamp + KMS key_id +
//!   tenant_id into a SHA-256 bundle.
//!
//! # Key rotation
//!
//! Per-region Ed25519 signing keys rotate every 30 days (canonical overlap
//! per `key_management.md §3.2.1` + ADR-0018; managed by the S-13 rotation
//! worker via [`corelink_rotation_adapters::ErasureAttestationRotationAdapter`]).
//! During the 30d overlap window both the Active and Overlap public keys are
//! served by `GET /v1/public/keys/erasure/{region}.pub`; a verifier that
//! downloaded the old public key pre-rotation can still verify attestations
//! signed after rotation.
//!
//! # Invariant: INV-ERASURE-ATTESTATION-SIGNED (HIGH)
//!
//! Every DSR erasure of a BYOK tenant MUST produce exactly one Ed25519-signed
//! attestation persisted in R2 with 7y retention and indexed in D1.
//! Enforced by [`ErasureAttester::attest_erasure`].
//!
//! # Security properties
//!
//! - [`ErasureSigningKey`] implements `ZeroizeOnDrop` — key material is
//!   cleared from memory when the struct is dropped.
//! - Signing uses `ed25519-dalek` which provides constant-time verify by
//!   default (side-channel resistant).
//! - `evidence_hash` is a SHA-256 over a canonical JSON bundle; detects
//!   incomplete evidence at attestation time.
//! - No signing key material appears in logs / traces / errors.
//!
//! # Example: sign an attestation
//!
//! ```rust,no_run
//! use corelink_erasure_attestation::{
//!     ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, Region,
//!     EvidenceBundle,
//! };
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let signing_key = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3600 * 1000);
//! let signer = ErasureAttestationSigner::new(signing_key);
//!
//! let bundle = EvidenceBundle {
//!     audit_chain_segment_ids: vec!["seg-001".to_string()],
//!     kms_destroy_ts: 1_700_000_000_000,
//!     kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
//!     tenant_id: "tenant-42".to_string(),
//! };
//! let payload = ErasureAttestationPayload {
//!     tenant_id: "tenant-42".to_string(),
//!     request_id: "req-uuid-001".to_string(),
//!     destroyed_ts: 1_700_000_000_000,
//!     kms_provider: "aws_kms".to_string(),
//!     kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
//!     evidence_hash: EvidenceBundle::compute_hash(&bundle),
//!     region: Region::Weur,
//!     attestation_key_id: 1,
//! };
//! let attestation = signer.sign(payload)?;
//! println!("signature: {}", attestation.signature_ed25519);
//! # Ok(())
//! # }
//! ```
//!
//! # Example: verify offline
//!
//! ```rust,no_run
//! use corelink_erasure_attestation::{ErasureAttestation, ErasurePublicKey, verify_attestation_signature};
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let attestation: ErasureAttestation = serde_json::from_str("{}")?; // fetched from endpoint
//! let public_key: ErasurePublicKey = serde_json::from_str("{}")?;    // fetched from pub endpoint
//! verify_attestation_signature(&attestation, &public_key)?;
//! println!("attestation verified");
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod attestation;
pub mod error;
pub mod evidence;
pub mod key;
pub mod region;
pub mod verify;

pub use attestation::{ErasureAttestation, ErasureAttestationPayload, ErasureAttestationSigner};
pub use error::AttestationError;
pub use evidence::EvidenceBundle;
pub use key::{ErasurePublicKey, ErasureSigningKey};
pub use region::Region;
pub use verify::verify_attestation_signature;

/// Crate schema version (bump on breaking payload changes per §23).
#[must_use]
pub const fn erasure_attestation_schema_version() -> u32 {
    1
}
