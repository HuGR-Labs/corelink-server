//! Burn-rate window canonical taxonomy + Google SRE Workbook Ch 5
//! Table 4 multi-window-multi-burn-rate matrix.
//!
//! ## Why 4 windows
//!
//! Google SRE Workbook Ch 5 §"Multi-Window, Multi-Burn-Rate Alerts"
//! canonical: 4 windows balance recall (catch real incidents fast)
//! against precision (avoid false-positive flapping). The threshold
//! multipliers (14.4× / 6× / 3× / 1×) are calibrated against monthly
//! error budget burn rate — at 14.4×, 2 % of monthly budget is
//! consumed in 1h sustained = page-worthy; at 1× over 3d, the
//! sustained error rate equals the SLO target = ticket-only review.
//!
//! ## Why these specific multipliers
//!
//! - `Fast1h` × 14.4× → 2% budget burned in 1h (page SEV-1).
//! - `Medium6h` × 6× → 5% budget burned in 6h (page SEV-1).
//! - `Slow24h` × 3× → 10% budget burned in 24h (ticket SEV-2).
//! - `Long3d` × 1× → 10% budget burned in 3d (ticket SEV-3).

/// Canonical 4-burn-rate window taxonomy per Google SRE Workbook Ch 5
/// Table 4. The `#[non_exhaustive]` marker reserves additive growth
/// for follow-on WIs (e.g. S-13 admin custom-window forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BurnRateWindow {
    /// 1h window × 14.4× threshold → 2 % of monthly budget burned per
    /// hour sustained = page imediato (Google SRE Workbook Table 4
    /// row 1).
    Fast1h,
    /// 6h window × 6× threshold → 5 % of monthly budget burned per 6h
    /// sustained = page (Google SRE Workbook Table 4 row 2).
    Medium6h,
    /// 24h window × 3× threshold → 10 % of monthly budget burned per
    /// 24h sustained = ticket SEV-2 (Google SRE Workbook Table 4
    /// row 3).
    Slow24h,
    /// 72h (3d) window × 1× threshold → 10 % of monthly budget burned
    /// per 3d sustained = ticket SEV-3 review (Google SRE Workbook
    /// Table 4 row 4).
    Long3d,
}

impl BurnRateWindow {
    /// Canonical Google SRE Workbook Ch 5 Table 4 burn-rate threshold
    /// multiplier. The decision rule is: `error_rate > multiplier ×
    /// error_budget_pct` → fire at this window's severity arm.
    #[must_use]
    pub const fn threshold_multiplier(self) -> f64 {
        match self {
            Self::Fast1h => 14.4,
            Self::Medium6h => 6.0,
            Self::Slow24h => 3.0,
            Self::Long3d => 1.0,
        }
    }

    /// Canonical window duration in seconds (used as Prometheus
    /// `for:` clause hold-down so a single transient spike does not
    /// trip the alert; also used as PromQL range vector duration in
    /// `rate(...[Xs])`).
    #[must_use]
    pub const fn window_duration_seconds(self) -> u64 {
        match self {
            Self::Fast1h => 3_600,
            Self::Medium6h => 21_600,
            Self::Slow24h => 86_400,
            Self::Long3d => 259_200,
        }
    }

    /// Canonical Prometheus `for:` clause hold-down duration in
    /// seconds (debounce a single transient spike). Calibrated per
    /// Google SRE Workbook canonical: longer windows can afford
    /// longer hold-downs because the sustained burn is by definition
    /// slower.
    #[must_use]
    pub const fn alertmanager_for_seconds(self) -> u64 {
        match self {
            Self::Fast1h => 120,
            Self::Medium6h => 900,
            Self::Slow24h => 3_600,
            Self::Long3d => 14_400,
        }
    }

    /// Canonical PromQL range vector label for the window
    /// (e.g. `[1h]` / `[6h]` / `[24h]` / `[3d]`). Used in alert YAML
    /// `expr` clauses.
    #[must_use]
    pub const fn prometheus_range_label(self) -> &'static str {
        match self {
            Self::Fast1h => "[1h]",
            Self::Medium6h => "[6h]",
            Self::Slow24h => "[24h]",
            Self::Long3d => "[3d]",
        }
    }

    /// Canonical slug used in audit records, alert annotations, and
    /// PagerDuty dedup-key construction.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Fast1h => "fast_1h",
            Self::Medium6h => "medium_6h",
            Self::Slow24h => "slow_24h",
            Self::Long3d => "long_3d",
        }
    }
}

impl core::fmt::Display for BurnRateWindow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 4-element burn-rate window list — pinned at the type
/// system layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_burn_rate_windows() -> &'static [BurnRateWindow; 4] {
    &[
        BurnRateWindow::Fast1h,
        BurnRateWindow::Medium6h,
        BurnRateWindow::Slow24h,
        BurnRateWindow::Long3d,
    ]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn google_sre_workbook_table4_multipliers_pinned() {
        assert_eq!(BurnRateWindow::Fast1h.threshold_multiplier(), 14.4);
        assert_eq!(BurnRateWindow::Medium6h.threshold_multiplier(), 6.0);
        assert_eq!(BurnRateWindow::Slow24h.threshold_multiplier(), 3.0);
        assert_eq!(BurnRateWindow::Long3d.threshold_multiplier(), 1.0);
    }

    #[test]
    fn window_duration_seconds_pinned() {
        assert_eq!(BurnRateWindow::Fast1h.window_duration_seconds(), 3_600);
        assert_eq!(BurnRateWindow::Medium6h.window_duration_seconds(), 21_600);
        assert_eq!(BurnRateWindow::Slow24h.window_duration_seconds(), 86_400);
        assert_eq!(BurnRateWindow::Long3d.window_duration_seconds(), 259_200);
    }

    #[test]
    fn alertmanager_for_seconds_pinned() {
        assert_eq!(BurnRateWindow::Fast1h.alertmanager_for_seconds(), 120);
        assert_eq!(BurnRateWindow::Medium6h.alertmanager_for_seconds(), 900);
        assert_eq!(BurnRateWindow::Slow24h.alertmanager_for_seconds(), 3_600);
        assert_eq!(BurnRateWindow::Long3d.alertmanager_for_seconds(), 14_400);
    }

    #[test]
    fn prometheus_range_label_pinned() {
        assert_eq!(BurnRateWindow::Fast1h.prometheus_range_label(), "[1h]");
        assert_eq!(BurnRateWindow::Medium6h.prometheus_range_label(), "[6h]");
        assert_eq!(BurnRateWindow::Slow24h.prometheus_range_label(), "[24h]");
        assert_eq!(BurnRateWindow::Long3d.prometheus_range_label(), "[3d]");
    }

    #[test]
    fn slug_pinned() {
        assert_eq!(BurnRateWindow::Fast1h.slug(), "fast_1h");
        assert_eq!(BurnRateWindow::Medium6h.slug(), "medium_6h");
        assert_eq!(BurnRateWindow::Slow24h.slug(), "slow_24h");
        assert_eq!(BurnRateWindow::Long3d.slug(), "long_3d");
    }

    #[test]
    fn canonical_list_matches_enum_count() {
        let v = canonical_burn_rate_windows();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for w in v {
            assert!(set.insert(w.slug()), "duplicate slug: {w}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", BurnRateWindow::Fast1h), "fast_1h");
        assert_eq!(format!("{}", BurnRateWindow::Long3d), "long_3d");
    }

    #[test]
    fn ordering_canonical_fast_to_long() {
        let v = canonical_burn_rate_windows();
        for i in 1..v.len() {
            let prev = v.get(i - 1).copied();
            let cur = v.get(i).copied();
            if let (Some(p), Some(c)) = (prev, cur) {
                assert!(
                    p.window_duration_seconds() < c.window_duration_seconds(),
                    "windows must be ordered fast → long"
                );
            }
        }
    }
}
