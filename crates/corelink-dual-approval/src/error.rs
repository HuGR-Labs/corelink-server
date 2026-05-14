//! `DualApprovalError` — canonical error taxonomy for dual-approval
//! enforcement (WI-S13-002).
//!
//! Every variant maps to a hard-fail 403 (or 401 for MFA stale) response.
//! No advisory mode: any variant = op blocked.

use uuid::Uuid;

/// Canonical error taxonomy for [`crate::DualApprovalGate::verify`].
///
/// All variants are hard-fail: no advisory path, no grace mode.
///
/// `#[non_exhaustive]` so future refinements (e.g. multi-tier approval
/// S-16) can add variants without breaking callers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DualApprovalError {
    /// `X-Dual-Approver` header absent.
    #[error("missing X-Dual-Approver header")]
    MissingApprover,

    /// `X-Approver-Signature` HMAC-SHA256 did not match recomputed value
    /// (constant-time compare; no timing oracle).
    #[error("approver signature HMAC invalid")]
    SignatureInvalid,

    /// `caller_user_id == approver_user_id` violates D1 separation-of-duties.
    #[error("caller equals approver (separation of duties violated)")]
    CallerEqualsApprover,

    /// Proposed approver appeared in the last 2 distinct destructive-op
    /// approvers in the 24h window (Lote 10.13 canonical oracle; NIST
    /// AC-2(7) collusion-rotation defense).
    #[error(
        "collusion-rotation violation: proposed approver appeared in last 2 \
         destructive ops in 24h (rolling 3-op window must have 3 distinct)"
    )]
    CollusionRotation {
        /// Recent approver UUIDs (last ≤ 2 distinct prior approvers).
        recent_approver_uuids: Vec<Uuid>,
    },

    /// Caller MFA timestamp is older than 30 minutes.
    #[error("MFA stale (last MFA {age_min}min ago; max 30min)")]
    MfaStale {
        /// How many minutes ago the MFA assertion was last made.
        age_min: u32,
    },

    /// Approver does not hold an active admin role (runtime check).
    #[error("approver lacks admin role")]
    ApproverNotAdmin,

    /// Nonce was already seen for this caller (D1 UNIQUE replay guard).
    #[error("nonce replay detected (last seen {seen_at_ms}ms)")]
    NonceReplay {
        /// Server-side timestamp (ms) when nonce was first seen.
        seen_at_ms: u64,
    },

    /// `ts_ms` deviates from server time by > 60 seconds.
    #[error("clock skew > 60s (request_ts={req_ms}, server_ts={srv_ms})")]
    ClockSkew {
        /// Request-side timestamp (ms).
        req_ms: u64,
        /// Server-side timestamp (ms) at verify time.
        srv_ms: u64,
    },

    /// Signing key ID unknown (key expired beyond 24h rotation overlap).
    #[error("signing key_id unknown or expired beyond rotation overlap")]
    UnknownKeyId,

    /// Internal error (serialisation, store access, etc.).
    #[error("internal dual-approval error: {0}")]
    Internal(String),
}
