//! CoreLink transparency-log submission seam — hugit-P2 seam E (ADR-0066).
//!
//! # Purpose
//!
//! CoreLink already produces tamper-evident *private* artifacts: per-tenant
//! audit exports with chain-link inclusion proofs (`corelink-audit-chain`)
//! and Ed25519-signed erasure attestations (`corelink-erasure-attestation`).
//! Those are verifiable *via* CoreLink — a relying party must trust CoreLink's
//! own export. hugit-P2 asked for a **public witness**: something verifiable
//! *against* CoreLink that anyone can check without trusting us.
//!
//! Per **ADR-0066** the ratified decision is to **integrate the public
//! sigstore/Rekor transparency log — not to build our own**. This crate is the
//! thin **submission seam** the ADR ratifies:
//!
//! 1. Take an already-signed CoreLink entry ([`SignedEntry`]) — a payload, the
//!    public key, and the detached signature over that payload.
//! 2. Build the canonical Rekor `hashedrekord` v0.0.1 proposed-entry
//!    ([`RekorHashedRekord`]) that witnesses it.
//! 3. Submit it to the public Rekor instance (the [`RekorSubmitter`] seam) —
//!    **post-hoc, off the write path**.
//! 4. Record the returned [`RekorWitnessRecord`] (`{log_index, inclusion_proof}`)
//!    alongside the entry so it can be re-verified against the public log.
//!
//! # What this crate is (and is not)
//!
//! CoreLink is a **submitter**, never a log operator. We do not run an
//! append-only log, do not gossip checkpoints, and do not vouch for Rekor's
//! consistency — that is exactly the public good ADR-0066 declines to rebuild.
//!
//! # Fail-OPEN on the witness (ADR-0066 §Consequences)
//!
//! The witness is a *best-effort enrichment*, never a gate. The entry itself is
//! already durably logged (ADR-0065) before any submission is attempted, so a
//! Rekor outage degrades witnessing but **never** blocks or fails the write
//! path. [`witness_or_degrade`] encodes this: a transport failure yields a
//! [`WitnessOutcome::Degraded`] (queue for out-of-band retry) rather than an
//! `Err` that could propagate onto a caller's hot path.
//!
//! # Deferred (ADR-0066): the real HTTPS transport
//!
//! Following the repo's `trait-abstraction-defer` charter (mirrored by
//! `corelink-erasure-attestation`'s "in-memory fake; real handler deferred"),
//! this crate ships the **pure-logic seam** — the canonical entry builder, the
//! [`RekorSubmitter`] trait, the response model, the fail-open policy, and an
//! [`InMemoryRekor`] fake that pins every algebraic invariant in CI without a
//! network dependency. The single binding portion the ADR leaves open — the
//! real HTTPS POST to `https://rekor.sigstore.dev/api/v1/log/entries` and the
//! out-of-band retry queue — is wired by the consumer (the erasure worker /
//! container) against this frozen seam, exactly as the ADR scopes it.
//!
//! # Example
//!
//! ```rust
//! use corelink_transparency_log::{
//!     InMemoryRekor, RekorSubmitter, SignedEntry, WitnessOutcome, witness_or_degrade,
//! };
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! // A CoreLink-signed entry (e.g. an Ed25519 erasure attestation).
//! let entry = SignedEntry::new(
//!     b"{\"request_id\":\"req-001\"}".to_vec(),
//!     "ed25519",
//!     b"-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----\n".to_vec(),
//!     b"\x01\x02\x03".to_vec(),
//! );
//!
//! let rekor = InMemoryRekor::new();
//! match witness_or_degrade(&rekor, &entry).await {
//!     WitnessOutcome::Witnessed(record) => {
//!         // Persist `{log_index, inclusion_proof}` alongside the entry.
//!         println!("witnessed at log index {}", record.log_index);
//!     }
//!     WitnessOutcome::Degraded(_reason) => {
//!         // Rekor unreachable: queue for out-of-band retry; entry stays durable.
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod entry;
pub mod error;
pub mod submit;
pub mod witness;

pub use entry::{RekorHashedRekord, SignedEntry};
pub use error::TransparencyLogError;
pub use submit::{InMemoryRekor, RekorSubmitter, WitnessOutcome, witness_or_degrade};
pub use witness::{InclusionProof, RekorWitnessRecord};

/// Crate schema version (bump on breaking witness-record changes).
#[must_use]
pub const fn transparency_log_schema_version() -> u32 {
    1
}
