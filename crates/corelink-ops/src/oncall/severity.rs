//! Incident severity taxonomy.

/// Canonical 4-severity taxonomy aligned with
/// `observability_model.md §3.1` SEV mapping (Sev0 = total outage;
/// Sev1 = customer-impacting; Sev2 = degraded; Sev3 = informational
/// ticket). The `#[non_exhaustive]` marker reserves additive growth
/// for follow-on WIs (e.g. Sev4 customer-cosmetic forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Severity {
    /// Sev0 — total outage / data-loss class incident.
    Sev0,
    /// Sev1 — customer-impacting; pages oncall immediately.
    Sev1,
    /// Sev2 — degraded performance; pages but does not wake.
    Sev2,
    /// Sev3 — informational ticket; no page dispatched.
    Sev3,
}

impl Severity {
    /// Canonical wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sev0 => "sev0",
            Self::Sev1 => "sev1",
            Self::Sev2 => "sev2",
            Self::Sev3 => "sev3",
        }
    }

    /// True if the severity is page-eligible (Sev0 / Sev1 / Sev2).
    /// Sev3 is ticket-only — does not contribute to per-shift Sev1 or
    /// Sev2 counters.
    #[must_use]
    pub const fn is_page(self) -> bool {
        matches!(self, Self::Sev0 | Self::Sev1 | Self::Sev2)
    }
}

impl core::fmt::Display for Severity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 4-element list of [`Severity`] for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_severities() -> &'static [Severity; 4] {
    &[
        Severity::Sev0,
        Severity::Sev1,
        Severity::Sev2,
        Severity::Sev3,
    ]
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
    fn severity_canonical_labels() {
        assert_eq!(Severity::Sev0.as_str(), "sev0");
        assert_eq!(Severity::Sev1.as_str(), "sev1");
        assert_eq!(Severity::Sev2.as_str(), "sev2");
        assert_eq!(Severity::Sev3.as_str(), "sev3");
    }

    #[test]
    fn is_page_arm_canonical() {
        assert!(Severity::Sev0.is_page());
        assert!(Severity::Sev1.is_page());
        assert!(Severity::Sev2.is_page());
        assert!(!Severity::Sev3.is_page());
    }
}
