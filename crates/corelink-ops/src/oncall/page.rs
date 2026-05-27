//! Page event + fatigue scoring (rolling windows).

use super::engineer::EngineerId;
use super::severity::Severity;

/// A single page event recorded against an engineer.
/// `correlation_id` ties the page back to the incident/audit chain
/// (PAT-CORRELATION-ID-001 + WI §17). The struct is tenant-agnostic
/// per CTRL-PRIV-001 (no tenant labels surfaced).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PageEvent {
    /// Recipient engineer.
    pub engineer: EngineerId,
    /// Incident severity.
    pub severity: Severity,
    /// Page timestamp (ms since epoch).
    pub ts_ms: u64,
    /// Correlation id from the incident response chain.
    pub correlation_id: String,
}

impl PageEvent {
    /// Construct a new page event.
    #[must_use]
    pub fn new(
        engineer: EngineerId,
        severity: Severity,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            engineer,
            severity,
            ts_ms,
            correlation_id: correlation_id.into(),
        }
    }

    /// True if the page falls within the sleep-hour window (00:00 -
    /// 06:00 local UTC) per WI §6.1.4 Grafana dashboard label
    /// `sleep_hour_pages`. Sleep-hour pages are weighted equally for
    /// fatigue counting but are surfaced separately on the dashboard.
    #[must_use]
    pub const fn is_sleep_hour(&self) -> bool {
        let ms_in_day: u64 = 24 * 60 * 60 * 1000;
        let ms_offset = self.ts_ms % ms_in_day;
        let six_hours_ms: u64 = 6 * 60 * 60 * 1000;
        ms_offset < six_hours_ms
    }
}

/// Rolling fatigue window taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum FatigueWindow {
    /// Rolling 7-day window (WI §1 / §6.1.2 weekly cadence).
    Rolling7d,
    /// Rolling 30-day window (WI §1 / §6.1.2 monthly cadence).
    Rolling30d,
}

impl FatigueWindow {
    /// Window duration (milliseconds).
    #[must_use]
    pub const fn duration_ms(self) -> u64 {
        match self {
            Self::Rolling7d => 7 * 24 * 60 * 60 * 1000,
            Self::Rolling30d => 30 * 24 * 60 * 60 * 1000,
        }
    }

    /// Canonical slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rolling7d => "rolling_7d",
            Self::Rolling30d => "rolling_30d",
        }
    }
}

impl core::fmt::Display for FatigueWindow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fatigue score = page count within `window` for a given engineer
/// at a snapshot `as_of_ms` timestamp. Constructed via
/// [`super::ledger::RotationLedger::fatigue_score`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FatigueScore {
    /// Number of pages within the window.
    pub count: u64,
    /// Window the score was computed over.
    pub window: FatigueWindow,
    /// Snapshot timestamp (right-edge of the window).
    pub as_of_ms: u64,
}

impl FatigueScore {
    /// Construct a new fatigue score snapshot.
    #[must_use]
    pub const fn new(count: u64, window: FatigueWindow, as_of_ms: u64) -> Self {
        Self {
            count,
            window,
            as_of_ms,
        }
    }
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
    fn window_durations_canonical() {
        assert_eq!(
            FatigueWindow::Rolling7d.duration_ms(),
            7 * 24 * 60 * 60 * 1000
        );
        assert_eq!(
            FatigueWindow::Rolling30d.duration_ms(),
            30 * 24 * 60 * 60 * 1000
        );
    }

    #[test]
    fn sleep_hour_detection() {
        let p1 = PageEvent::new(
            EngineerId::new("a"),
            Severity::Sev1,
            3 * 60 * 60 * 1000, // 03:00 UTC day 0
            "cid",
        );
        assert!(p1.is_sleep_hour());
        let p2 = PageEvent::new(
            EngineerId::new("a"),
            Severity::Sev1,
            10 * 60 * 60 * 1000, // 10:00 UTC day 0
            "cid",
        );
        assert!(!p2.is_sleep_hour());
    }
}
