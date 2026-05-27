//! Synthetic-severity taxonomy.

/// Canonical synthetic-severity taxonomy. The synthetic drill emits a
/// dedicated `Sev2Synthetic` variant routed through PagerDuty's
/// `synthetic-drill` service so production SEV-2 escalation paths are
/// NEVER inadvertently triggered by a drill. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on synthetic tests (e.g.
/// `Sev1Synthetic` quarterly major-incident drill).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DrillSeverity {
    /// Synthetic SEV-2 drill — pages oncall via the dedicated
    /// `synthetic-drill` service routing key. Production escalation
    /// rules MUST NOT match this severity.
    Sev2Synthetic,
}

impl DrillSeverity {
    /// Canonical wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sev2Synthetic => "sev2_synthetic",
        }
    }
}

impl core::fmt::Display for DrillSeverity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 1-element list of [`DrillSeverity`] for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_drill_severities() -> &'static [DrillSeverity; 1] {
    &[DrillSeverity::Sev2Synthetic]
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
    fn severity_label() {
        assert_eq!(DrillSeverity::Sev2Synthetic.as_str(), "sev2_synthetic");
    }

    #[test]
    fn canonical_surface_stable() {
        assert_eq!(canonical_drill_severities().len(), 1);
    }
}
