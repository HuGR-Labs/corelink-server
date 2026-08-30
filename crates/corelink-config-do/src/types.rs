//! Domain types for the DO config-singleton (WI-S13-001).
//!
//! All types are schema-versioned via [`ConfigPayload::schema_version`].
//! Breaking schema changes MUST bump `schema_version` and file an ADR
//! (per design decision §9.5 + ADR-XXXX).
//!
//! Serialization uses `#[serde(deny_unknown_fields)]` on every inbound
//! decode path (enforced in [`crate::validation`]) so that an old Worker
//! reading a new schema_version always returns a hard error, never a
//! silent default (risk R-003).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Schema version of the config payload.
///
/// Bumped via mandatory ADR whenever a breaking field is added or removed.
/// In-process deserialization of a `schema_version` higher than
/// `SUPPORTED_SCHEMA_VERSION` MUST return
/// [`crate::ConfigError::SchemaInvalid`] (Worker rejects unknown schema).
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Canonical config payload stored in the DO config-singleton.
///
/// `schema_version` guards against silent schema drift in distributed
/// Workers (design decision §9.5; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
/// herdada).
///
/// All maps use [`BTreeMap`] for deterministic JCS canonical-JSON
/// serialization (required for `payload_hash` reproducibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigPayload {
    /// Bumped via ADR on any breaking schema change.
    pub schema_version: u32,
    /// Typed feature flags keyed by feature id string.
    pub feature_flags: BTreeMap<String, FeatureFlag>,
    /// Rate-limit tunables keyed by `(layer, tier)`.
    pub rate_limits: BTreeMap<RateLimitKey, RateLimitTunable>,
    /// Retention policies keyed by resource name.
    pub retention_policies: BTreeMap<String, RetentionPolicy>,
}

impl ConfigPayload {
    /// Return a safe genesis config (all maps empty, schema_version=1).
    ///
    /// Used for DO initialization before any admin has written a real config.
    #[must_use]
    pub fn genesis() -> Self {
        Self {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            feature_flags: BTreeMap::new(),
            rate_limits: BTreeMap::new(),
            retention_policies: BTreeMap::new(),
        }
    }
}

/// A single feature flag definition.
///
/// `rollout_pct` MUST be in `0..=100`; validation enforced in
/// [`crate::validation::validate_payload`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FeatureFlag {
    /// Whether the feature is globally enabled.
    pub enabled: bool,
    /// Rollout percentage 0–100.
    pub rollout_pct: u8,
    /// Tenant allowlist (explicit overrides rollout_pct).
    pub allowlist_tenants: Vec<Uuid>,
}

/// Composite key for a rate-limit tunable.
///
/// `layer` examples: `"cas_put"`, `"ac_get"`.
/// `tier` examples: `"Solo"`, `"Team"`, `"Business"`, `"Enterprise"`.
///
/// Implements `Ord` for [`BTreeMap`] key use (deterministic JSON order).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct RateLimitKey {
    /// Operation layer identifier.
    pub layer: String,
    /// Tenant tier identifier.
    pub tier: String,
}

/// Rate-limit tunable values for a `(layer, tier)` pair.
///
/// `refill_rate_per_sec` MUST be ≥ 1; validation enforced in
/// [`crate::validation::validate_payload`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RateLimitTunable {
    /// Token refill rate per second. MUST be ≥ 1.
    pub refill_rate_per_sec: u32,
    /// Burst capacity.
    pub burst: u32,
}

/// Retention policy for a named resource.
///
/// `ttl_days` MUST be ≤ 365 unless an ADR override flag is set;
/// validation enforced in [`crate::validation::validate_payload`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    /// TTL in days. MUST be ≤ 365 (ADR override required otherwise).
    pub ttl_days: u32,
}

/// A historical config version entry stored in D1 `config_change_log`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersionEntry {
    /// Monotonically increasing version counter.
    pub version: u64,
    /// SHA-256 of the canonical JCS JSON payload.
    pub payload_hash: [u8; 32],
    /// Actor who applied this version.
    pub actor: AdminActor,
    /// MFA timestamp (Unix ms) of the session that applied this change.
    pub mfa_ts_ms: u64,
    /// Wall-clock creation timestamp (Unix ms).
    pub created_at_ms: u64,
    /// Whether this entry was an update or a rollback restore.
    pub change_type: ChangeType,
    /// Previous version that was replaced (None for v1 genesis).
    pub previous_version: Option<u64>,
}

/// Change type discriminant for audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ChangeType {
    /// A normal CAS update.
    #[serde(rename = "update")]
    Update,
    /// A rollback that restored a previous version's payload.
    #[serde(rename = "rollback")]
    Rollback,
}

/// Pseudonymized admin actor for audit records.
///
/// `email_hash` is SHA-256(email) — pseudonymized for audit
/// (INV-AUDIT-NO-RAW-PII herdada from corelink-audit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminActor {
    /// Admin user UUID (UUIDv7).
    pub user_id: Uuid,
    /// SHA-256(email) — pseudonymized; never raw email in audit log.
    pub email_hash: [u8; 32],
}

impl AdminActor {
    /// Construct a synthetic test/operator actor (zeroed email hash).
    /// Production wiring reads the actor from the auth middleware's
    /// verified JWT and constructs a real instance — this is a test
    /// helper, NOT a real-admin entry point.
    #[must_use]
    #[allow(
        clippy::indexing_slicing,
        reason = "indices are bounded by construction"
    )]
    pub fn operator(id: &str) -> Self {
        // Stable per-label UUID via a simple stable mix (no external
        // dep) of the first 16 bytes of the label. The input is
        // bounded to 16 bytes before the copy, so the index-slicing
        // clippy lint is satisfied (no out-of-bounds possible).
        let mut h = [0u8; 32];
        let bytes = id.as_bytes();
        let n = bytes.len().min(16);
        for (i, b) in bytes.iter().take(n).enumerate() {
            // Mirror the padding shape so the bytes occupy the same
            // slot as if we'd copied them into a 16-byte buffer.
            h[i % 32] ^= b.wrapping_add((i as u8).wrapping_mul(31));
            // Fold the index into a second slot so the hash is sensitive
            // to the position of the last byte (distinguishes "ab" from "ba").
            h[(i + 16) % 32] ^= b.wrapping_add((i as u8).wrapping_mul(17));
        }
        // Construct a UUIDv4 from the first 16 bytes of the mix.
        let mut bytes16 = [0u8; 16];
        bytes16.copy_from_slice(&h[..16]);
        bytes16[6] = (bytes16[6] & 0x0f) | 0x40; // version 4
        bytes16[8] = (bytes16[8] & 0x3f) | 0x80; // variant
        Self {
            user_id: Uuid::from_bytes(bytes16),
            email_hash: [0u8; 32],
        }
    }
}
