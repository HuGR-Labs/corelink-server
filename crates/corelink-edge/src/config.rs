//! Edge config knobs surfaced by the production binding.
//!
//! Defaults pin canonical thresholds from WI-S08-002 §6.1 + spec
//! contract §5 R-S08-2 (CF List 10000 max items per WI §6.1.6).

/// Canonical CF List item ceiling per CF docs (WI §6.1.6). The
/// in-memory mirror enforces the same hard ceiling so a runaway admin
/// add cannot push past CF's own limit.
pub const DEFAULT_MAX_BLOCKLIST_SIZE: usize = 10_000;

/// Canonical alert threshold — emit SEV-3 when blocklist size crosses
/// 80% of the CF List ceiling (WI §6.1.6 proactive shard signal).
pub const DEFAULT_ALERT_THRESHOLD_SIZE: usize = 8_000;

/// Default action when no rule matches (no blocklist hit; no abuse
/// signal). Canonical = `Allow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeDefaultAction {
    /// Allow the request through to the per-IP token bucket layer.
    Allow,
    /// Deny by default — only used in lockdown / emergency mode.
    Deny,
}

/// Knobs driving the edge policy decision engine.
///
/// All fields are private; access via inherent methods so the
/// production wiring cannot accidentally drift from canonical defaults
/// without going through [`EdgeConfig::with_overrides`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeConfig {
    /// Default action when no blocklist entry matches.
    default_action: EdgeDefaultAction,
    /// Maximum blocklist size (matches the production CF List ceiling).
    max_blocklist_size: usize,
    /// Threshold above which the size gauge fires SEV-3 (proactive
    /// shard signal).
    alert_threshold_size: usize,
}

impl EdgeConfig {
    /// Construct with the canonical defaults pinned to the spec contract.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            default_action: EdgeDefaultAction::Allow,
            max_blocklist_size: DEFAULT_MAX_BLOCKLIST_SIZE,
            alert_threshold_size: DEFAULT_ALERT_THRESHOLD_SIZE,
        }
    }

    /// Construct with explicit knob overrides.
    ///
    /// Returns `None` when:
    /// - `max_blocklist_size == 0` (a 0-cap blocklist can never store).
    /// - `alert_threshold_size > max_blocklist_size` (alert above max
    ///   never fires).
    #[must_use]
    pub const fn with_overrides(
        default_action: EdgeDefaultAction,
        max_blocklist_size: usize,
        alert_threshold_size: usize,
    ) -> Option<Self> {
        if max_blocklist_size == 0 {
            return None;
        }
        if alert_threshold_size > max_blocklist_size {
            return None;
        }
        Some(Self {
            default_action,
            max_blocklist_size,
            alert_threshold_size,
        })
    }

    /// Default action when no rule matches.
    #[must_use]
    pub const fn default_action(&self) -> EdgeDefaultAction {
        self.default_action
    }

    /// Maximum blocklist size.
    #[must_use]
    pub const fn max_blocklist_size(&self) -> usize {
        self.max_blocklist_size
    }

    /// Alert threshold size.
    #[must_use]
    pub const fn alert_threshold_size(&self) -> usize {
        self.alert_threshold_size
    }
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self::canonical()
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
    fn canonical_constants_pinned() {
        assert_eq!(DEFAULT_MAX_BLOCKLIST_SIZE, 10_000);
        assert_eq!(DEFAULT_ALERT_THRESHOLD_SIZE, 8_000);
    }

    #[test]
    fn canonical_config_uses_canonical_defaults() {
        let c = EdgeConfig::canonical();
        assert_eq!(c.default_action(), EdgeDefaultAction::Allow);
        assert_eq!(c.max_blocklist_size(), 10_000);
        assert_eq!(c.alert_threshold_size(), 8_000);
    }

    #[test]
    fn default_matches_canonical() {
        assert_eq!(EdgeConfig::default(), EdgeConfig::canonical());
    }

    #[test]
    fn with_overrides_accepts_valid() {
        let c =
            EdgeConfig::with_overrides(EdgeDefaultAction::Deny, 100, 80).unwrap();
        assert_eq!(c.default_action(), EdgeDefaultAction::Deny);
        assert_eq!(c.max_blocklist_size(), 100);
        assert_eq!(c.alert_threshold_size(), 80);
    }

    #[test]
    fn with_overrides_rejects_zero_max() {
        assert!(EdgeConfig::with_overrides(EdgeDefaultAction::Allow, 0, 0).is_none());
    }

    #[test]
    fn with_overrides_rejects_alert_above_max() {
        assert!(
            EdgeConfig::with_overrides(EdgeDefaultAction::Allow, 100, 200).is_none()
        );
    }

    #[test]
    fn alert_at_max_is_allowed() {
        // Boundary: alert == max is permitted (no separation).
        assert!(
            EdgeConfig::with_overrides(EdgeDefaultAction::Allow, 100, 100)
                .is_some()
        );
    }
}
