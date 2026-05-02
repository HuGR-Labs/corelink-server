//! Canonical [`RateLimitError`] taxonomy surfaced by every fallible API
//! in the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07 +
//! S-08 lesson — the variant set grows additively across S-08 follow-on
//! WIs without breaking downstream callers.

use thiserror::Error;
use uuid::Uuid;

use crate::audit::RateLimitAuditSinkError;
use crate::metrics::RateLimitMetricsObserverError;

/// Canonical errors surfaced by [`crate::limiter::RateLimiter`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RateLimitError {
    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The decision engine
    /// rolls back the per-request bucket mutation on this error per
    /// WI §6.1.8 fail-closed envelope.
    #[error("ratelimit audit sink error: {0}")]
    Audit(#[from] RateLimitAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s08.001.5 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("ratelimit metrics observer error: {0}")]
    Metrics(#[from] RateLimitMetricsObserverError),

    /// Cost-overflow: caller passed a `cost` strictly larger than the
    /// bucket's burst capacity — request can NEVER be served. Mapped
    /// to 400 by handler (programmer / config error).
    #[error("ratelimit cost {cost} exceeds bucket capacity {capacity}")]
    CostExceedsCapacity {
        /// Requested cost.
        cost: u32,
        /// Bucket capacity ceiling.
        capacity: u32,
    },

    /// Tenant id mismatch — the bucket key's tenant_id and the
    /// caller-supplied tenant_id disagree. Mapped to 500 by handler
    /// (programmer wiring error; INV-AVAIL-ISOLATION canary —
    /// orchestrator MUST also bump
    /// `corelink.ratelimit.cross_tenant_violation_total`).
    #[error(
        "ratelimit tenant mismatch: caller={caller_tenant_id}, bucket={bucket_tenant_id}"
    )]
    TenantMismatch {
        /// Tenant id the caller (TenantCtx-extracted) supplied.
        caller_tenant_id: Uuid,
        /// Tenant id the bucket key carries.
        bucket_tenant_id: Uuid,
    },

    /// Backend transport failure (DO storage / D1 / fake mutex
    /// poisoning).
    #[error("ratelimit backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn cost_exceeds_capacity_carries_fields() {
        let e = RateLimitError::CostExceedsCapacity {
            cost: 10_000,
            capacity: 1000,
        };
        let s = format!("{e}");
        assert!(s.contains("10000"));
        assert!(s.contains("1000"));
    }

    #[test]
    fn tenant_mismatch_carries_both_ids() {
        let a = Uuid::from_u128(0xa);
        let b = Uuid::from_u128(0xb);
        let e = RateLimitError::TenantMismatch {
            caller_tenant_id: a,
            bucket_tenant_id: b,
        };
        let s = format!("{e}");
        assert!(s.contains(&format!("{a}")));
        assert!(s.contains(&format!("{b}")));
    }

    #[test]
    fn backend_carries_message() {
        let e = RateLimitError::Backend("D1 unavailable".to_string());
        let s = format!("{e}");
        assert!(s.contains("D1 unavailable"));
    }
}
