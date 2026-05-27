//! [`RevocationAuditEvent`] — CloudEvent payload for `corelink.byok.cmk_revoked`.

use serde::{Deserialize, Serialize};

/// CloudEvent type for CMK revocation.
pub const EVENT_TYPE_CMK_REVOKED: &str = "corelink.byok.cmk_revoked";

/// CloudEvent type for tenant restoration (CMK re-enabled).
pub const EVENT_TYPE_CMK_RESTORED: &str = "corelink.byok.cmk_restored";

/// Audit event payload for `corelink.byok.cmk_revoked`.
///
/// Emitted atomically with the D1 tenant status update
/// (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Retained 7 years (CTRL-AUDIT-005).
///
/// # Example
///
/// ```rust
/// use corelink_byok::revocation::RevocationAuditEvent;
///
/// let event = RevocationAuditEvent {
///     event_type: "corelink.byok.cmk_revoked".to_string(),
///     provider: "aws".to_string(),
///     kms_key_id_hashed: "sha256:abc123".to_string(),
///     tenant_id_hashed: "sha256:def456".to_string(),
///     detected_at_ms: 1_000_000,
///     evicted_at_ms: 1_000_001,
///     alerted_at_ms: 1_000_100,
///     kill_switch_duration_ms: 100,
///     evicted_dek_count: 5,
/// };
/// assert_eq!(event.event_type, "corelink.byok.cmk_revoked");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationAuditEvent {
    /// CloudEvent type: `corelink.byok.cmk_revoked` or
    /// `corelink.byok.cmk_restored`.
    pub event_type: String,

    /// Provider kind (e.g. `aws`, `gcp`, `azure`, `vault`).
    pub provider: String,

    /// SHA-256 hash of the KMS key ID (PII/sensitive; never raw).
    pub kms_key_id_hashed: String,

    /// SHA-256 hash of the tenant ID (PII/sensitive; never raw).
    pub tenant_id_hashed: String,

    /// Millisecond timestamp when the revocation was detected.
    pub detected_at_ms: u64,

    /// Millisecond timestamp when the DEK cache was fully evicted.
    pub evicted_at_ms: u64,

    /// Millisecond timestamp when the customer alert was dispatched.
    pub alerted_at_ms: u64,

    /// Total kill switch duration in milliseconds
    /// (detected → evicted + degraded + alerted).
    pub kill_switch_duration_ms: u64,

    /// Number of DEK cache entries evicted.
    pub evicted_dek_count: usize,
}
