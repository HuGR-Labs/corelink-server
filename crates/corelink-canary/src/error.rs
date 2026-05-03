//! `corelink-canary` canonical error taxonomy.

use thiserror::Error;

use crate::assertion::CanaryAssertion;
use crate::region::CanaryRegion;
use crate::result::HealthComponent;

/// Canonical `corelink-canary` error taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on sprints (e.g. S-13
/// admin manual override / S-14 enterprise tier custom assertion arm).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanaryError {
    /// Canary assertion ladder boundary breach (latency ceiling
    /// missed). Carries the region and the canonical assertion arm
    /// that fired so the SEV-2 alert payload is structurally
    /// complete.
    #[error("canary assertion failed in region {region}: {assertion}")]
    AssertionFailed {
        /// Canary region where the assertion fired.
        region: CanaryRegion,
        /// Canonical assertion arm that fired.
        assertion: CanaryAssertion,
    },
    /// BLAKE3 digest mismatch between the written + read blob bytes
    /// (data integrity issue; SEV-1 alert source). Carries the
    /// canonical hex-encoded digests for forensic logging.
    #[error("BLAKE3 digest mismatch in region {region}: written={written} read={read}")]
    DigestMismatch {
        /// Canary region where the mismatch was observed.
        region: CanaryRegion,
        /// Hex-encoded written digest.
        written: String,
        /// Hex-encoded read digest.
        read: String,
    },
    /// Observability stack component unhealthy (SEV-3 alert source).
    /// Carries the component arm so the alert payload + dashboard
    /// drill-down route to the right operator.
    #[error("observability stack component unhealthy in region {region}: {component}")]
    ObservabilityUnhealthy {
        /// Canary region where the unhealthy probe was observed.
        region: CanaryRegion,
        /// Canonical observability component arm.
        component: HealthComponent,
    },
    /// CF Workers cron-trigger dispatch lag exceeded the canonical
    /// 90s SEV-3 threshold per WI-S09-007 §6.1.9.
    #[error("canary cron dispatch lag exceeded SEV-3 threshold in region {region}: {observed_ms} ms (threshold {threshold_ms} ms)")]
    DispatchLagExceeded {
        /// Canary region where the lag was observed.
        region: CanaryRegion,
        /// Observed dispatch lag in ms.
        observed_ms: u64,
        /// Canonical SEV-3 threshold (90_000 ms).
        threshold_ms: u64,
    },
    /// Audit-of-canary emit failed; the canary path aborts per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern +
    /// S-07 P1-1 fix). Caller MUST NOT retry the canary loop without
    /// first resolving the audit failure.
    #[error("canary audit emit failed: {0}")]
    Audit(String),
    /// Internal invariant violation surfaced via `Mutex` poisoning or
    /// state corruption. Treat as a non-recoverable fault: the caller
    /// MUST tear down the orchestrator instance + reconstruct.
    #[error("internal canary orchestrator fault: {0}")]
    Internal(String),
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
    fn assertion_failed_renders_canonical() {
        let e = CanaryError::AssertionFailed {
            region: CanaryRegion::Enam,
            assertion: CanaryAssertion::CasGetP99,
        };
        let s = format!("{e}");
        assert!(s.contains("region enam"));
        assert!(s.contains("cas_get_p99_ms"));
    }

    #[test]
    fn digest_mismatch_renders_canonical() {
        let e = CanaryError::DigestMismatch {
            region: CanaryRegion::Weur,
            written: "abc".to_string(),
            read: "def".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("weur"));
        assert!(s.contains("written=abc"));
        assert!(s.contains("read=def"));
    }

    #[test]
    fn observability_unhealthy_renders_canonical() {
        let e = CanaryError::ObservabilityUnhealthy {
            region: CanaryRegion::Apac,
            component: HealthComponent::Mimir,
        };
        let s = format!("{e}");
        assert!(s.contains("apac"));
        assert!(s.contains("mimir"));
    }

    #[test]
    fn dispatch_lag_renders_canonical_threshold() {
        let e = CanaryError::DispatchLagExceeded {
            region: CanaryRegion::Enam,
            observed_ms: 95_000,
            threshold_ms: 90_000,
        };
        let s = format!("{e}");
        assert!(s.contains("95000"));
        assert!(s.contains("90000"));
    }

    #[test]
    fn audit_renders() {
        let e = CanaryError::Audit("sink down".to_string());
        assert!(format!("{e}").contains("audit emit failed"));
    }

    #[test]
    fn internal_renders() {
        let e = CanaryError::Internal("mutex poisoned".to_string());
        assert!(format!("{e}").contains("internal canary orchestrator fault"));
    }
}
