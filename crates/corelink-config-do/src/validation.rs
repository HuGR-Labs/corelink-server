//! Schema and domain validation for [`crate::ConfigPayload`] (WI-S13-001).
//!
//! Invoked pre-write by every update path to ensure malformed payloads
//! never reach DO storage. Enforces:
//!
//! - `schema_version == SUPPORTED_SCHEMA_VERSION` (old/future schemas rejected).
//! - `rollout_pct ∈ 0..=100` for every feature flag.
//! - `refill_rate_per_sec ≥ 1` for every rate-limit tunable.
//! - `ttl_days ≤ 365` for every retention policy (ADR override not yet wired).

use crate::{ConfigError, ConfigPayload, SUPPORTED_SCHEMA_VERSION};

/// Validate a [`ConfigPayload`] against all domain invariants.
///
/// Returns `Ok(())` if the payload is safe to persist, or
/// [`ConfigError::SchemaInvalid`] with a descriptive message otherwise.
///
/// # Errors
///
/// Returns [`ConfigError::SchemaInvalid`] when any domain constraint is
/// violated.
pub fn validate_payload(payload: &ConfigPayload) -> Result<(), ConfigError> {
    if payload.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ConfigError::SchemaInvalid(format!(
            "schema_version {} not supported (supported: {})",
            payload.schema_version, SUPPORTED_SCHEMA_VERSION
        )));
    }

    for (id, flag) in &payload.feature_flags {
        if flag.rollout_pct > 100 {
            return Err(ConfigError::SchemaInvalid(format!(
                "feature flag {id:?}: rollout_pct {} out of range (0..=100)",
                flag.rollout_pct
            )));
        }
    }

    for (key, tunable) in &payload.rate_limits {
        if tunable.refill_rate_per_sec == 0 {
            return Err(ConfigError::SchemaInvalid(format!(
                "rate limit ({:?}/{:?}): refill_rate_per_sec must be ≥ 1 (got 0; self-inflicted DoS)",
                key.layer, key.tier
            )));
        }
    }

    for (resource, policy) in &payload.retention_policies {
        if policy.ttl_days > 365 {
            return Err(ConfigError::SchemaInvalid(format!(
                "retention policy {resource:?}: ttl_days {} > 365 requires ADR override (not yet supported)",
                policy.ttl_days
            )));
        }
    }

    Ok(())
}
