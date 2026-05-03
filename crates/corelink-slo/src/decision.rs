//! Alert decision canonical taxonomy.
//!
//! ## Severity mapping
//!
//! Per `observability_model.md §3.1` SEV taxonomy + Google SRE
//! Workbook Ch 5 routing canonical:
//!
//! - `Quiet` — no alert; SLO healthy.
//! - `TicketSev3` — visibility channel, 24h response (long-burn
//!   trend; budget burning slowly but sustained).
//! - `TicketSev2` — admin notification, 1h response (slow-burn; 10 %
//!   budget consumed in 24h).
//! - `PageSev1` — oncall pager, 5min response (medium-burn; 5 %
//!   budget in 6h).
//! - `PageSev0` — oncall pager + paging escalation, 5min response
//!   (fast-burn; 2 % budget in 1h sustained).

/// Canonical alert decision taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for follow-on WIs (e.g. S-13 admin custom
/// SEV-tier forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AlertDecision {
    /// SLO healthy; no alert dispatched.
    Quiet,
    /// Visibility-channel ticket (24h response). Long-burn trend per
    /// Google SRE Workbook Ch 5 Table 4 row 4 (3d × 1×).
    TicketSev3,
    /// Admin-notification ticket (1h response). Slow-burn per Google
    /// SRE Workbook Ch 5 Table 4 row 3 (24h × 3×).
    TicketSev2,
    /// Oncall pager (5min response). Medium-burn per Google SRE
    /// Workbook Ch 5 Table 4 row 2 (6h × 6×).
    PageSev1,
    /// Oncall pager + paging escalation (5min response). Fast-burn
    /// per Google SRE Workbook Ch 5 Table 4 row 1 (1h × 14.4×).
    PageSev0,
}

impl AlertDecision {
    /// Canonical SEV-string label per `observability_model.md §3.1`.
    /// Used in alert YAML `labels.severity` + audit records.
    #[must_use]
    pub const fn severity_label(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::TicketSev3 => "sev3",
            Self::TicketSev2 => "sev2",
            Self::PageSev1 => "sev1",
            Self::PageSev0 => "sev0",
        }
    }

    /// Whether this decision triggers a PagerDuty page (pager
    /// escalation; 5min MTTA target per sprint contract §10.s09.4).
    #[must_use]
    pub const fn is_page(self) -> bool {
        matches!(self, Self::PageSev0 | Self::PageSev1)
    }

    /// Whether this decision triggers a ticket (no pager escalation;
    /// business-hours review).
    #[must_use]
    pub const fn is_ticket(self) -> bool {
        matches!(self, Self::TicketSev2 | Self::TicketSev3)
    }

    /// Whether this decision is the no-alert arm.
    #[must_use]
    pub const fn is_quiet(self) -> bool {
        matches!(self, Self::Quiet)
    }

    /// Canonical slug for audit records + alert annotations.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::TicketSev3 => "ticket_sev3",
            Self::TicketSev2 => "ticket_sev2",
            Self::PageSev1 => "page_sev1",
            Self::PageSev0 => "page_sev0",
        }
    }
}

impl core::fmt::Display for AlertDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 5-element alert decision list — pinned at the type
/// system layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_alert_decisions() -> &'static [AlertDecision; 5] {
    &[
        AlertDecision::Quiet,
        AlertDecision::TicketSev3,
        AlertDecision::TicketSev2,
        AlertDecision::PageSev1,
        AlertDecision::PageSev0,
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
    fn severity_label_pinned() {
        assert_eq!(AlertDecision::Quiet.severity_label(), "quiet");
        assert_eq!(AlertDecision::TicketSev3.severity_label(), "sev3");
        assert_eq!(AlertDecision::TicketSev2.severity_label(), "sev2");
        assert_eq!(AlertDecision::PageSev1.severity_label(), "sev1");
        assert_eq!(AlertDecision::PageSev0.severity_label(), "sev0");
    }

    #[test]
    fn is_page_predicate_pinned() {
        assert!(AlertDecision::PageSev0.is_page());
        assert!(AlertDecision::PageSev1.is_page());
        assert!(!AlertDecision::TicketSev2.is_page());
        assert!(!AlertDecision::TicketSev3.is_page());
        assert!(!AlertDecision::Quiet.is_page());
    }

    #[test]
    fn is_ticket_predicate_pinned() {
        assert!(AlertDecision::TicketSev2.is_ticket());
        assert!(AlertDecision::TicketSev3.is_ticket());
        assert!(!AlertDecision::PageSev0.is_ticket());
        assert!(!AlertDecision::PageSev1.is_ticket());
        assert!(!AlertDecision::Quiet.is_ticket());
    }

    #[test]
    fn is_quiet_predicate_pinned() {
        assert!(AlertDecision::Quiet.is_quiet());
        assert!(!AlertDecision::PageSev0.is_quiet());
        assert!(!AlertDecision::TicketSev2.is_quiet());
    }

    #[test]
    fn canonical_decisions_unique_slugs() {
        let v = canonical_alert_decisions();
        assert_eq!(v.len(), 5);
        let mut set = std::collections::HashSet::new();
        for d in v {
            assert!(set.insert(d.slug()), "duplicate slug: {d}");
        }
    }

    #[test]
    fn ordering_canonical_quiet_to_page_sev0() {
        assert!(AlertDecision::Quiet < AlertDecision::TicketSev3);
        assert!(AlertDecision::TicketSev3 < AlertDecision::TicketSev2);
        assert!(AlertDecision::TicketSev2 < AlertDecision::PageSev1);
        assert!(AlertDecision::PageSev1 < AlertDecision::PageSev0);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", AlertDecision::PageSev0), "page_sev0");
        assert_eq!(format!("{}", AlertDecision::Quiet), "quiet");
    }

    #[test]
    fn page_and_ticket_and_quiet_partition_decisions() {
        let v = canonical_alert_decisions();
        for d in v {
            let cnt = u32::from(d.is_page())
                + u32::from(d.is_ticket())
                + u32::from(d.is_quiet());
            assert_eq!(
                cnt, 1,
                "every decision belongs to exactly one of page/ticket/quiet: {d}"
            );
        }
    }
}
