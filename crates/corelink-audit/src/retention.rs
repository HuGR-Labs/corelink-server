//! Per-tenant retention hint enums (INV-AUDIT-RETENTION-HINT-ACCURATE, HIGH).
//!
//! The retention worker (S-11 forward) consumes the [`RetentionHint`] tagged
//! on every emitted [`crate::AuthEvent`] to decide when an audit row may
//! be reaped. The mapping from [`TenantTier`] to [`RetentionHint`] is
//! the **single canonical source of truth** for that decision; a future
//! tier (e.g. `Compliance` for SOC 2 Type II) is added strictly via
//! additive enum growth behind `#[non_exhaustive]`.

use serde::Serialize;

/// CoreLink tenant tier — drives both billing surface (S-19) and audit
/// retention budget (S-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TenantTier {
    /// Solo tier — single-developer plan; 30-day audit retention promised.
    Solo,
    /// Team tier — small org plan; 90-day audit retention.
    Team,
    /// Business tier — mid-market plan; 1-year audit retention.
    Business,
    /// Enterprise tier — high-touch plan; 7-year audit retention default
    /// (SOC 2 Type II + LGPD Art. 16 + GDPR Art. 30 baseline).
    Enterprise,
}

/// Retention hint tagged on every emitted audit event. The hint is set
/// at emit-time by the producer (looking up `tenant.tier` in Neon) and
/// carried through the outbox + chain so the retention worker can
/// honor per-tenant promises without re-querying Neon for every row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RetentionHint {
    /// Solo tier → 30 days.
    Solo30d,
    /// Team tier → 90 days.
    Team90d,
    /// Business tier → 1 year.
    Business1y,
    /// Enterprise tier → 7 years (default).
    Enterprise7y,
}

impl RetentionHint {
    /// Canonical mapping `TenantTier → RetentionHint`. Used at emit-time
    /// by every producer; centralized here so a future tier addition
    /// requires touching exactly one match arm.
    #[must_use]
    pub const fn for_tier(tier: TenantTier) -> Self {
        match tier {
            TenantTier::Solo => Self::Solo30d,
            TenantTier::Team => Self::Team90d,
            TenantTier::Business => Self::Business1y,
            TenantTier::Enterprise => Self::Enterprise7y,
        }
    }

    /// Approximate retention duration in days. Used by the retention
    /// worker as a bound for the row-eligible-for-reap predicate.
    #[must_use]
    pub const fn approx_days(self) -> u32 {
        match self {
            Self::Solo30d => 30,
            Self::Team90d => 90,
            Self::Business1y => 365,
            Self::Enterprise7y => 365 * 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_tier_canonical_mapping() {
        assert_eq!(
            RetentionHint::for_tier(TenantTier::Solo),
            RetentionHint::Solo30d
        );
        assert_eq!(
            RetentionHint::for_tier(TenantTier::Team),
            RetentionHint::Team90d
        );
        assert_eq!(
            RetentionHint::for_tier(TenantTier::Business),
            RetentionHint::Business1y
        );
        assert_eq!(
            RetentionHint::for_tier(TenantTier::Enterprise),
            RetentionHint::Enterprise7y
        );
    }

    #[test]
    fn approx_days_monotonic() {
        let solo = RetentionHint::Solo30d.approx_days();
        let team = RetentionHint::Team90d.approx_days();
        let biz = RetentionHint::Business1y.approx_days();
        let ent = RetentionHint::Enterprise7y.approx_days();
        assert!(solo < team);
        assert!(team < biz);
        assert!(biz < ent);
    }
}
