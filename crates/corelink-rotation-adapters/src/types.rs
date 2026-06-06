//! [`AssetClass`], [`KeyState`], and [`KeyHandle`] — canonical types for
//! the rotation state machine (WI-S13-003 §1).

use serde::{Deserialize, Serialize};

/// 30-day hard upper bound in seconds per `key_management.md §3.2.1`.
pub const HARD_UPPER_BOUND_SECONDS: u64 = 30 * 24 * 3_600;

/// The 5 asset classes rotated by the CoreLink secret rotation worker
/// (ADR-0018; `key_management.md §3.2.1`).
///
/// Each variant carries a canonical overlap window and the same 30d
/// hard upper bound (`HARD_UPPER_BOUND_SECONDS`).
///
/// `#[non_exhaustive]` per CoreLink codex (S-06 P0-2): callers MUST
/// use a wildcard arm so future asset classes (e.g. DSR receipt JWS
/// key) are forward-compatible without a breaking change.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssetClass {
    /// Tenant derivation keys (S-01). Overlap 7d (CTRL-KEY-005 +
    /// CTRL-KEY-006 canonical). Re-wrap envelope CAS background job;
    /// 24h causes starvation at TB scale.
    Tdk,
    /// PAT signing keys (S-03 hybrid HMAC + Argon2id cycle 9 SEAL
    /// decision (a)). Overlap 24h. Multi-key via signing_key_id column
    /// (`data_model.md §4.1`).
    PatSigning,
    /// Audit chain key (S-09; per-region BLAKE3 chain). Overlap 24h.
    /// Per-region staggered rotation to avoid simultaneous transitions.
    AuditChain,
    /// Admin signing key (per-region HMAC-SHA256 for dual-approval op
    /// signing; consumed by WI-S13-002). Overlap 24h. ADR-0018 5th
    /// asset class added Lote 10.13 codex P0.
    AdminSigning,
    /// BYOK customer CMK (S-14 forward; customer-trigger). Overlap 7d.
    /// CoreLink-side DEK cache invalidated 5 min hard on customer revoke.
    Byok,
    /// Erasure attestation Ed25519 signing key (WI-S14-007; per-region).
    /// Overlap 30d canonical (key_management.md §3.2.1 + ADR-0018):
    /// long overlap preserves verifiability of attestations signed
    /// pre-rotation; verify endpoint accepts both keys during window.
    ErasureAttestationKey,
}

impl AssetClass {
    /// Canonical overlap window in seconds per `key_management.md §3.2.1`.
    #[must_use]
    pub const fn overlap_seconds(self) -> u64 {
        match self {
            Self::Tdk => 7 * 24 * 3_600,                    // 7d
            Self::PatSigning => 24 * 3_600,                 // 24h
            Self::AuditChain => 24 * 3_600,                 // 24h
            Self::AdminSigning => 24 * 3_600,               // 24h
            Self::Byok => 7 * 24 * 3_600,                   // 7d
            Self::ErasureAttestationKey => 30 * 24 * 3_600, // 30d canonical per key_management.md §3.2.1
        }
    }

    /// Hard upper bound in seconds per `key_management.md §3.2.1`.
    /// Identical across all asset classes; 30d per NIST SP 800-57 Pt 1
    /// Rev 5 §5.3 CoreLink defense-in-depth.
    #[must_use]
    pub const fn hard_upper_bound_seconds(self) -> u64 {
        HARD_UPPER_BOUND_SECONDS
    }

    /// Return the canonical asset class string for D1 `CHECK` + metric
    /// labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tdk => "tdk",
            Self::PatSigning => "pat_signing",
            Self::AuditChain => "audit_chain",
            Self::AdminSigning => "admin_signing",
            Self::Byok => "byok",
            Self::ErasureAttestationKey => "erasure_attestation_key",
        }
    }
}

impl core::fmt::Display for AssetClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Key lifecycle state.
///
/// State machine canonical (INV-KEY-NO-SKIP):
/// ```text
/// Pending --[promote]--> Active --[next rotation]--> Overlap
///                                                       |
///                                               [overlap window expired]
///                                                       |
///                                                  Retired --[grace 90d]--> Destroyed
/// Active --[downstream errors > 1%]--> RolledBack
/// ```
///
/// `#[non_exhaustive]` per CoreLink codex.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyState {
    /// Generated but not yet promoted as the write key.
    Pending,
    /// Current write key (only state where writes are accepted;
    /// INV-KEY-NO-SKIP).
    Active,
    /// Accepted for reads during overlap window; not for writes.
    Overlap,
    /// Post-overlap; not accepted for reads or writes.
    Retired,
    /// Key material zeroized (post 90d grace from Retired).
    Destroyed,
    /// Promotion reverted by PAT-ROLL-FORWARD-001 auto-rollback.
    RolledBack,
}

impl KeyState {
    /// Return the canonical state string for D1 `CHECK` constraints.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Overlap => "overlap",
            Self::Retired => "retired",
            Self::Destroyed => "destroyed",
            Self::RolledBack => "rolled_back",
        }
    }
}

impl core::fmt::Display for KeyState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A handle to a key in the rotation state machine.
///
/// All timestamps are milliseconds since Unix epoch (Cloudflare
/// Workers `Date.now()` convention; no `std::time` dependency so this
/// type is wasm32-clean).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyHandle {
    /// Unique monotonic key ID (maps to `rotation_state.key_id` BIGINT).
    pub key_id: u64,
    /// Asset class this key belongs to.
    pub asset_class: AssetClass,
    /// Current lifecycle state.
    pub state: KeyState,
    /// Millisecond timestamp when the key material was generated.
    pub created_at_ms: u64,
    /// Millisecond timestamp when the key was promoted to Active.
    pub promoted_at_ms: Option<u64>,
    /// Millisecond timestamp until which the key is valid for reads
    /// in Overlap state (= `promoted_at_ms + overlap_seconds * 1000`
    /// of the *successor* key).
    pub overlap_until_ms: Option<u64>,
    /// Millisecond timestamp when the key was retired.
    pub retired_at_ms: Option<u64>,
}
