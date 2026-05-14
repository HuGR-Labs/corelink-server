//! Core types for dual-approval enforcement (WI-S13-002).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An admin operation request requiring dual-approval gate.
///
/// Corresponds to `POST /v1/admin/ops` body after header extraction.
///
/// # Wire headers mapped into struct
///
/// | Header | Field |
/// |---|---|
/// | `X-Dual-Approver: <uuid>` | `approver_user_id` |
/// | `X-Approver-Signature: <hex32>` | `approver_signature` |
/// | `X-Approver-Nonce: <hex16>` | `nonce` |
/// | `X-Request-Ts-Ms: <u64>` | `ts_ms` |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminOpRequest {
    /// The user making the request (from JWT sub claim).
    pub caller_user_id: Uuid,
    /// Approver user ID extracted from `X-Dual-Approver` header.
    pub approver_user_id: Uuid,
    /// HMAC-SHA256 over `op_payload_canonical_bytes || nonce || ts_ms_be`.
    /// Extracted from `X-Approver-Signature` header (raw bytes; 32 bytes).
    pub approver_signature: [u8; 32],
    /// Canonical operation type (destructive ops are gated).
    pub op_type: AdminOpType,
    /// JCS-canonical (RFC 8785) JSON payload bytes.
    pub op_payload: Vec<u8>,
    /// 128-bit random nonce from approver side (replay protection).
    pub nonce: [u8; 16],
    /// Request timestamp ms since UNIX epoch (clock-skew bound ≤ 60s).
    pub ts_ms: u64,
    /// Tenant scope for collusion-rotation window query.
    pub tenant_id: Uuid,
}

/// Canonical admin operation type taxonomy.
///
/// Destructive variants require full dual-approval + collusion-rotation
/// check; non-destructive variants are still audited but approver is
/// optional (recommended).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AdminOpType {
    // ── Destructive (dual-approval gated) ──────────────────────────────
    /// Roll back a config version (WI-S13-001 rollback API).
    ConfigRollback,
    /// Reduce a retention policy TTL.
    RetentionPolicyReduce,
    /// Disable a feature flag globally or per-tenant.
    FeatureFlagDisable,
    /// Kick off a secret rotation (WI-S13-003).
    SecretRotationStart,
    /// Tombstone a tenant (S-11 erasure pipeline).
    TenantTombstone,
    // ── Non-destructive (audited; approver optional but recommended) ───
    /// Toggle a feature flag in a safe direction (enable / rollout pct ↑).
    FeatureFlagToggleSafe,
    /// Increase a rate-limit bucket (conservative direction).
    RateLimitAdjustUp,
}

impl AdminOpType {
    /// Returns `true` for ops that require the full dual-approval gate
    /// (caller ≠ approver + collusion-rotation check).
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::ConfigRollback
                | Self::RetentionPolicyReduce
                | Self::FeatureFlagDisable
                | Self::SecretRotationStart
                | Self::TenantTombstone
        )
    }
}

/// A successfully-verified dual-approval context passed to the op
/// dispatcher.
#[derive(Debug, Clone)]
pub struct VerifiedApproval {
    /// Caller who submitted the request.
    pub caller_user_id: Uuid,
    /// Approved-by user.
    pub approver_user_id: Uuid,
    /// Caller MFA assertion timestamp (ms).
    pub mfa_ts_ms: u64,
    /// Approved operation type.
    pub op_type: AdminOpType,
    /// SHA-256 of prior state (for rollback chain integrity).
    pub prev_state_hash: [u8; 32],
    /// JCS-canonical payload bytes (forwarded to dispatcher).
    pub op_payload: Vec<u8>,
}

/// Outcome of a dual-approval verification attempt — stored in
/// `admin_op_log.outcome` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ApprovalOutcome {
    /// Op was approved and executed.
    Approved,
    /// Denied: X-Dual-Approver header missing.
    DeniedMissing,
    /// Denied: HMAC signature invalid.
    DeniedSig,
    /// Denied: caller == approver.
    DeniedCallerEq,
    /// Denied: collusion-rotation rolling window violation.
    DeniedCollusion,
    /// Denied: MFA timestamp stale.
    DeniedMfaStale,
    /// Denied: approver not in active admin role.
    DeniedApproverNotAdmin,
    /// Denied: nonce replay.
    DeniedNonceReplay,
    /// Denied: clock skew > 60s.
    DeniedClockSkew,
}

impl ApprovalOutcome {
    /// Returns the D1 `outcome` column value (matches CHECK constraint).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::DeniedMissing => "denied_missing",
            Self::DeniedSig => "denied_sig",
            Self::DeniedCallerEq => "denied_caller_eq",
            Self::DeniedCollusion => "denied_collusion",
            Self::DeniedMfaStale => "denied_mfa_stale",
            Self::DeniedApproverNotAdmin => "denied_approver_not_admin",
            Self::DeniedNonceReplay => "denied_nonce_replay",
            Self::DeniedClockSkew => "denied_clock_skew",
        }
    }
}

/// Rich CloudEvent payload for `corelink.admin.op.executed` /
/// `corelink.admin.op.denied` events (CAP-ADMIN-006).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAuditEventData {
    /// Actor (caller) identity.
    pub actor: ActorIdentity,
    /// MFA assertion timestamp (ms).
    pub mfa_ts_ms: u64,
    /// Dual-approver identity.
    pub dual_approver: ActorIdentity,
    /// Operation type string.
    pub op_type: String,
    /// SHA-256 hex of `op_payload`.
    pub op_payload_hash: String,
    /// SHA-256 hex of prior state.
    pub prev_state_hash: String,
    /// Hex-encoded nonce.
    pub nonce: String,
    /// Approval outcome string.
    pub outcome: String,
    /// HMAC-SHA256 hex (chain integrity: HMAC over canonical audit bytes).
    pub signature: String,
}

/// Privacy-safe actor identity (email hashed, UUID present for forensics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorIdentity {
    /// User UUID.
    pub user_id: Uuid,
    /// SHA-256 hex of email (raw email never persisted per INV-AUDIT-NO-RAW-PII).
    pub email_hash: String,
}

impl ActorIdentity {
    /// Construct from UUID + SHA-256 hex of email.
    pub fn new(user_id: Uuid, email_hash: impl Into<String>) -> Self {
        Self {
            user_id,
            email_hash: email_hash.into(),
        }
    }
}

/// Row stored in `admin_op_log` D1 table.
#[derive(Debug, Clone)]
pub struct AdminOpLogRow {
    /// UUID primary key.
    pub op_id: Uuid,
    /// Operation type string.
    pub op_type: String,
    /// Caller UUID.
    pub caller_user_id: Uuid,
    /// Approver UUID (NULL only for non-destructive ops).
    pub approver_user_id: Option<Uuid>,
    /// SHA-256 of op_payload.
    pub op_payload_hash: [u8; 32],
    /// SHA-256 of prior state.
    pub prev_state_hash: [u8; 32],
    /// MFA timestamp ms.
    pub mfa_ts_ms: u64,
    /// Replay nonce.
    pub nonce: [u8; 16],
    /// Request timestamp ms.
    pub ts_ms: u64,
    /// Outcome string.
    pub outcome: String,
}

/// A recent approver entry from the collusion-rotation query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentApprover {
    /// Approver UUID.
    pub approver_user_id: Uuid,
}
