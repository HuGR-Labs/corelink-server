//! Rotation + shift abstractions.

use crate::engineer::EngineerId;
use crate::error::OncallError;
use crate::tier::Tier;
use crate::{PROTECTION_PERIOD_SECONDS, SHIFT_CAP_SECONDS};

/// Opaque shift identifier (PagerDuty assignment id surface).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShiftId(String);

impl ShiftId {
    /// Construct a new shift id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Canonical slug.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for ShiftId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A single oncall shift.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Shift {
    /// Canonical shift id.
    pub id: ShiftId,
    /// Assigned engineer.
    pub engineer: EngineerId,
    /// Tier the engineer is rostered into.
    pub tier: Tier,
    /// Shift start timestamp (ms since epoch).
    pub start_ms: u64,
    /// Shift end timestamp (ms since epoch). MUST satisfy
    /// `end_ms - start_ms <= SHIFT_CAP_SECONDS * 1000`.
    pub end_ms: u64,
}

impl Shift {
    /// Construct + validate a shift. Returns
    /// [`OncallError::InvalidRotation`] if the shift exceeds the 7-day
    /// canonical cap or has `end_ms <= start_ms`.
    pub fn try_new(
        id: ShiftId,
        engineer: EngineerId,
        tier: Tier,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<Self, OncallError> {
        if end_ms <= start_ms {
            return Err(OncallError::InvalidRotation(
                "shift end_ms must be strictly greater than start_ms".to_string(),
            ));
        }
        let duration_ms = end_ms - start_ms;
        let cap_ms = SHIFT_CAP_SECONDS.saturating_mul(1000);
        if duration_ms > cap_ms {
            return Err(OncallError::InvalidRotation(format!(
                "shift duration {duration_ms}ms exceeds 7-day canonical cap {cap_ms}ms"
            )));
        }
        Ok(Self {
            id,
            engineer,
            tier,
            start_ms,
            end_ms,
        })
    }

    /// Returns the end of the post-shift protection window.
    #[must_use]
    pub fn protection_end_ms(&self) -> u64 {
        self.end_ms
            .saturating_add(PROTECTION_PERIOD_SECONDS.saturating_mul(1000))
    }

    /// True if `ts_ms` falls within the post-shift protection window
    /// for this shift (i.e. shift ended but recovery period has not
    /// elapsed; engineer cannot be assigned a new shift).
    #[must_use]
    pub fn in_protection_window(&self, ts_ms: u64) -> bool {
        ts_ms >= self.end_ms && ts_ms < self.protection_end_ms()
    }

    /// True if `ts_ms` falls within the active shift window.
    #[must_use]
    pub fn covers(&self, ts_ms: u64) -> bool {
        ts_ms >= self.start_ms && ts_ms < self.end_ms
    }
}

/// A rotation is an ordered set of [`Shift`]s for a given tier.
/// The rotation is append-only; the ledger keeps the audit-of-audit
/// invariant by emitting BEFORE the shift is appended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rotation {
    /// Canonical tier this rotation covers.
    pub tier: Tier,
    /// Shifts in chronological order.
    pub shifts: Vec<Shift>,
}

impl Rotation {
    /// Construct an empty rotation for a tier.
    #[must_use]
    pub fn new(tier: Tier) -> Self {
        Self {
            tier,
            shifts: Vec::new(),
        }
    }

    /// Try to append a shift. Validates the rotation invariants:
    ///   - the appended shift's tier matches the rotation tier
    ///   - the appended shift starts no earlier than the previous
    ///     shift's end (no overlap with the prior shift on the same
    ///     tier)
    pub fn try_append(&mut self, shift: Shift) -> Result<(), OncallError> {
        if shift.tier != self.tier {
            return Err(OncallError::InvalidRotation(format!(
                "shift tier {} mismatches rotation tier {}",
                shift.tier, self.tier
            )));
        }
        if let Some(prev) = self.shifts.last() {
            if shift.start_ms < prev.end_ms {
                return Err(OncallError::InvalidRotation(format!(
                    "shift start {} overlaps prior shift end {}",
                    shift.start_ms, prev.end_ms
                )));
            }
        }
        self.shifts.push(shift);
        Ok(())
    }

    /// True if `engineer` is within their post-shift protection
    /// window at `ts_ms` (across any historical shift in this
    /// rotation).
    #[must_use]
    pub fn engineer_in_protection(&self, engineer: &EngineerId, ts_ms: u64) -> bool {
        self.shifts
            .iter()
            .filter(|s| &s.engineer == engineer)
            .any(|s| s.in_protection_window(ts_ms))
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

    fn eng(id: &str) -> EngineerId {
        EngineerId::new(id)
    }

    #[test]
    fn shift_validates_7d_cap() {
        let start = 1_000_000;
        let end = start + SHIFT_CAP_SECONDS * 1000;
        let s = Shift::try_new(ShiftId::new("s1"), eng("a"), Tier::Tier1, start, end);
        assert!(s.is_ok());

        let end_over = end + 1;
        let s_over = Shift::try_new(ShiftId::new("s2"), eng("a"), Tier::Tier1, start, end_over);
        assert!(s_over.is_err());
    }

    #[test]
    fn shift_rejects_zero_duration() {
        let s = Shift::try_new(ShiftId::new("s1"), eng("a"), Tier::Tier1, 100, 100);
        assert!(s.is_err());
        let s = Shift::try_new(ShiftId::new("s1"), eng("a"), Tier::Tier1, 100, 50);
        assert!(s.is_err());
    }

    #[test]
    fn protection_window_2w() {
        let start = 0;
        let end = SHIFT_CAP_SECONDS * 1000;
        let s = Shift::try_new(ShiftId::new("s1"), eng("a"), Tier::Tier1, start, end).unwrap();
        assert_eq!(
            s.protection_end_ms(),
            end + PROTECTION_PERIOD_SECONDS * 1000
        );
        assert!(s.in_protection_window(end));
        assert!(s.in_protection_window(end + 1));
        assert!(!s.in_protection_window(end + PROTECTION_PERIOD_SECONDS * 1000));
    }

    #[test]
    fn rotation_rejects_tier_mismatch() {
        let mut rot = Rotation::new(Tier::Tier1);
        let s = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier2,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        assert!(rot.try_append(s).is_err());
    }

    #[test]
    fn rotation_rejects_overlap() {
        let mut rot = Rotation::new(Tier::Tier1);
        let s1 = Shift::try_new(ShiftId::new("s1"), eng("a"), Tier::Tier1, 0, 1000).unwrap();
        let s2 = Shift::try_new(ShiftId::new("s2"), eng("b"), Tier::Tier1, 500, 1500).unwrap();
        rot.try_append(s1).unwrap();
        assert!(rot.try_append(s2).is_err());
    }
}
