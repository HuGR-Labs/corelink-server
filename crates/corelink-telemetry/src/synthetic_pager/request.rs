//! Synthetic drill request types.

use crate::synthetic_pager::error::SyntheticDrillError;
use crate::synthetic_pager::region::Region;
use crate::synthetic_pager::severity::DrillSeverity;

/// Validated synthetic drill identifier — `SP-<uppercase alnum/hyphen>+`.
///
/// The canonical shape is `SP-<UUIDv4 with dashes stripped>` (e.g.
/// `SP-7E1F4A2C9B8D4E7AAD3F6C1B0E9D8472`), but the validator only
/// enforces the structural prefix + character class so adapters may
/// stamp shorter test ids (`SP-W19R01`) and they still round-trip.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SyntheticDrillId(String);

impl SyntheticDrillId {
    /// Validate + wrap.
    ///
    /// Accepts `SP-<id>` where the suffix is non-empty and contains
    /// only uppercase ASCII letters, ASCII digits, or hyphens.
    ///
    /// # Errors
    ///
    /// Returns [`SyntheticDrillError::InvalidDrillId`] if the shape is
    /// rejected.
    pub fn new(raw: impl Into<String>) -> Result<Self, SyntheticDrillError> {
        let s: String = raw.into();
        if !s.starts_with("SP-") || s.len() < 4 {
            return Err(SyntheticDrillError::InvalidDrillId(s));
        }
        let suffix = s.get(3..).unwrap_or("");
        if suffix.is_empty()
            || !suffix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(SyntheticDrillError::InvalidDrillId(s));
        }
        Ok(Self(s))
    }

    /// Borrow the wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SyntheticDrillId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One synthetic page emit request — the cron worker constructs this
/// then dispatches it through the [`crate::synthetic_pager::record::DrillRecorder`]
/// trait + PagerDuty Events API v2 fake.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyntheticPageRequest {
    /// Canonical drill id (`SP-<...>`).
    pub drill_id: SyntheticDrillId,
    /// Target region — the cron computes this from the current UTC
    /// hour via [`Region::for_utc_hour`].
    pub region: Region,
    /// Dedicated synthetic severity (production escalation paths MUST
    /// NOT match this).
    pub severity: DrillSeverity,
    /// Emit timestamp (ms since epoch).
    pub emit_ts_ms: i64,
    /// Audit-chain correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
}

impl SyntheticPageRequest {
    /// Construct a validated synthetic page request.
    ///
    /// # Errors
    ///
    /// * [`SyntheticDrillError::EmitTimestampInFuture`] if `emit_ts_ms`
    ///   is greater than `now_ms` (clock-skew guard; per Lote 10.5bis
    ///   audit-chain TS canonical).
    /// * [`SyntheticDrillError::EmptyEngineer`] is *not* returned here
    ///   — the engineer slug is bound on ack (the ack webhook joins
    ///   the active rotation from the PagerDuty schedule).
    pub fn new(
        drill_id: SyntheticDrillId,
        region: Region,
        emit_ts_ms: i64,
        now_ms: i64,
        correlation_id: impl Into<String>,
    ) -> Result<Self, SyntheticDrillError> {
        if emit_ts_ms > now_ms {
            return Err(SyntheticDrillError::EmitTimestampInFuture { emit_ts_ms, now_ms });
        }
        Ok(Self {
            drill_id,
            region,
            severity: DrillSeverity::Sev2Synthetic,
            emit_ts_ms,
            correlation_id: correlation_id.into(),
        })
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
    fn drill_id_canonical_shape() {
        let id = SyntheticDrillId::new("SP-W19R01").unwrap();
        assert_eq!(id.as_str(), "SP-W19R01");
    }

    #[test]
    fn drill_id_rejects_lowercase() {
        let r = SyntheticDrillId::new("SP-abc");
        assert!(matches!(r, Err(SyntheticDrillError::InvalidDrillId(_))));
    }

    #[test]
    fn drill_id_rejects_missing_prefix() {
        let r = SyntheticDrillId::new("W19R01");
        assert!(matches!(r, Err(SyntheticDrillError::InvalidDrillId(_))));
    }

    #[test]
    fn emit_ts_in_future_rejected() {
        let id = SyntheticDrillId::new("SP-X").unwrap();
        let r = SyntheticPageRequest::new(id, Region::Americas, 200, 100, "corr-1");
        assert!(matches!(
            r,
            Err(SyntheticDrillError::EmitTimestampInFuture { .. })
        ));
    }
}
