//! The surface-tagged content-addressing digest algorithm.
//!
//! CoreLink's CAS keyspace is **surface-partitioned, single-function**:
//! each blob's key fixes exactly one digest function and the durable gate
//! never mixes functions within one key. Native CAS + sccache content-
//! address with **BLAKE3**; the Bazel REAPI v2 surface content-addresses
//! with **SHA-256** (REAPI v2's default) under its own `bazel/sha256/`
//! key sub-prefix.
//!
//! The algorithm is threaded as an **explicit** [`DigestAlgo`] value (never
//! inferred from hash-string length — that would be a silent gate) so the
//! durable `verify_content_hash` re-verification on the read path (bitrot)
//! re-applies the SAME function the blob was admitted under. See
//! `docs/followups/2026-06-15-bazel-sha256-concern-D.md` (Option A) and
//! ADR-0044.

/// The content-addressing digest function for a CAS keyspace.
///
/// Default (native CAS + sccache) is [`DigestAlgo::Blake3`]; only the Bazel
/// REAPI v2 adapter selects [`DigestAlgo::Sha256`]. The two never mix within
/// one key — each keyspace is single-function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestAlgo {
    /// BLAKE3 — the native CAS + sccache canonical digest (ADR-0044 §1).
    Blake3,
    /// SHA-256 — the Bazel REAPI v2 canonical digest, stored under the
    /// surface-tagged `bazel/sha256/` key sub-prefix.
    Sha256,
}

impl Default for DigestAlgo {
    /// The native/sccache default: BLAKE3.
    fn default() -> Self {
        Self::Blake3
    }
}
