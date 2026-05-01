//! Sign-count newtype + W3C-compliant regression policy.
//!
//! Per `WI-S03-006 §9.9 + §28 R-004` (Lote 10.3-tris P0-R5-002b) the
//! canonical policy distinguishes three cases:
//!
//! 1. **Stored = 0 AND incoming = 0** → passkey behaviour
//!    ([`SignCountSeverity::PasskeyExempt`]). NEVER alerts.
//! 2. **Stored < incoming** → monotonic OK. Returns the new value to
//!    persist.
//! 3. **Stored ≥ incoming AND at least one of them ≠ 0** → regression.
//!    First event → SEV-2 investigation; downstream forensic
//!    confirmation (≥ 2 independent signals: SRE ack + IP geolocation
//!    mismatch + UA fingerprint mismatch) escalates to SEV-1 + force
//!    re-registration.
//!
//! The "≥ 3 in 24 h" threshold from the original §28 R-004 was
//! REMOVED per Lote 10.3-tris P0-R5-002b — it produced a 24-72h false
//! safety window that absorbed real cloned-authenticator attacks.

use serde::{Deserialize, Serialize};

/// Sign-count newtype (`u64` storage; W3C `authData.signCount` is
/// `u32` on the wire but the database column is `BIGINT` so a future
/// counter rollover does not require a schema change).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignCount(pub u64);

impl SignCount {
    /// Sentinel for the "passkey reports counter = 0 always"
    /// canonical pattern (W3C §6.1.1; passkey provider behaviour).
    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Construct from a raw `u64`.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Inner value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Whether this is the canonical passkey sentinel.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// W3C-compliant severity of a sign-count assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignCountSeverity {
    /// Both stored and incoming are 0 — passkey provider canonical
    /// behaviour. No action.
    PasskeyExempt,
    /// Strictly increasing counter; persist the new value.
    Monotonic,
    /// First regression event observed; emit SEV-2; escalate to
    /// SEV-1 only after forensic confirmation.
    Sev2InvestigationRequired,
}

/// Outcome of an assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignCountAssessment {
    /// Canonical severity.
    pub severity: SignCountSeverity,
    /// Persisted value AFTER this ceremony (or unchanged if the
    /// authenticator reported a regression).
    pub persisted: SignCount,
}

impl SignCountAssessment {
    /// Whether this assessment indicates a regression event that the
    /// caller must surface as `WebAuthnError::SignCountRegression`.
    #[must_use]
    pub const fn is_regression(&self) -> bool {
        matches!(self.severity, SignCountSeverity::Sev2InvestigationRequired)
    }

    /// Whether the assessment authorises persistence of a new value.
    #[must_use]
    pub const fn should_persist_advance(&self) -> bool {
        matches!(self.severity, SignCountSeverity::Monotonic)
    }
}

/// Apply the canonical policy.
///
/// `stored` is the value persisted at the previous successful
/// authentication; `incoming` is the value reported by the
/// authenticator on the current authenticator data.
#[must_use]
pub fn assess(stored: SignCount, incoming: SignCount) -> SignCountAssessment {
    let s = stored.value();
    let i = incoming.value();
    if s == 0 && i == 0 {
        return SignCountAssessment {
            severity: SignCountSeverity::PasskeyExempt,
            persisted: SignCount::zero(),
        };
    }
    if i > s {
        return SignCountAssessment {
            severity: SignCountSeverity::Monotonic,
            persisted: incoming,
        };
    }
    SignCountAssessment {
        severity: SignCountSeverity::Sev2InvestigationRequired,
        persisted: stored,
    }
}
