//! Core region types — WI-S14-001.
//!
//! [`Region`] is the canonical 4-value enum used throughout S-14.
//! [`DoJurisdiction`] captures Cloudflare DO jurisdictional restriction.

use serde::{Deserialize, Serialize};

/// Canonical CoreLink region identifier.
///
/// Maps to Cloudflare R2 `locationHint`, D1 `location`, and custom domain
/// `{region}.api.corelink.humangr.com`.
///
/// INV-DATA-RESIDENCY: tenant `primary_region` is pinned at signup;
/// cross-region writes → 403 + audit (enforced by WI-S14-002 insert checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Region {
    /// WNAM — us-west (Cloudflare R2/D1 `wnam`).
    Wnam,
    /// ENAM — us-east (Cloudflare R2/D1 `enam`).
    Enam,
    /// WEUR — eu-west (Cloudflare R2/D1 `weur`). DO jurisdiction = "eu" mandatory.
    Weur,
    /// SAM — sa-east (Cloudflare R2/D1 `sam`).
    Sam,
}

impl Region {
    /// All valid region values (for property tests + iterate-all patterns).
    pub const ALL: &'static [Region] = &[Region::Wnam, Region::Enam, Region::Weur, Region::Sam];

    /// Lowercase identifier string (used in resource names, metrics labels).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Region::Wnam => "wnam",
            Region::Enam => "enam",
            Region::Weur => "weur",
            Region::Sam => "sam",
        }
    }

    /// Cloudflare R2 `locationHint` (uppercase per CF API).
    #[must_use]
    pub fn r2_location_hint(&self) -> &'static str {
        match self {
            Region::Wnam => "WNAM",
            Region::Enam => "ENAM",
            Region::Weur => "WEUR",
            Region::Sam => "SAM",
        }
    }

    /// Human-readable geographic label for observability / runbook use.
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Region::Wnam => "us-west",
            Region::Enam => "us-east",
            Region::Weur => "eu-west",
            Region::Sam => "sa-east",
        }
    }

    /// R2 bucket name for this region.
    #[must_use]
    pub fn r2_bucket_name(&self) -> String {
        format!("corelink-cas-{}", self.as_str())
    }

    /// D1 database name for this region.
    #[must_use]
    pub fn d1_db_name(&self) -> String {
        format!("corelink-meta-{}", self.as_str())
    }

    /// KV namespace title for this region (FM-054 per-region scoping).
    #[must_use]
    pub fn kv_namespace_title(&self) -> String {
        format!("corelink-session-{}", self.as_str())
    }

    /// Custom domain for explicit per-region routing.
    #[must_use]
    pub fn custom_domain(&self) -> String {
        format!("{}.api.corelink.humangr.com", self.as_str())
    }

    /// Parse from lowercase string.
    ///
    /// # Errors
    /// Returns `Err` if `s` is not a known region identifier.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, crate::error::RegionError> {
        match s {
            "wnam" => Ok(Region::Wnam),
            "enam" => Ok(Region::Enam),
            "weur" => Ok(Region::Weur),
            "sam" => Ok(Region::Sam),
            other => Err(crate::error::RegionError::UnknownRegion(other.to_owned())),
        }
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cloudflare Durable Object jurisdictional restriction.
///
/// FF-HR-003: WEUR MUST use [`DoJurisdiction::Eu`] (Schrems II + GDPR Art. 46).
/// Any other value for WEUR is a CRITICAL compliance gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DoJurisdiction {
    /// No jurisdictional restriction (default CF behavior — global routing).
    None,
    /// EU jurisdiction — DO pins execution to EU infrastructure.
    /// MANDATORY for WEUR (Schrems II + GDPR Art. 46).
    Eu,
    /// US jurisdiction.
    Us,
}

impl DoJurisdiction {
    /// Terraform / Cloudflare API string value.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            DoJurisdiction::None => "none",
            DoJurisdiction::Eu => "eu",
            DoJurisdiction::Us => "us",
        }
    }

    /// Expected jurisdiction for a given region (per WI-S14-001 §1).
    #[must_use]
    pub fn expected_for_region(region: Region) -> Self {
        match region {
            Region::Wnam => DoJurisdiction::Us,
            Region::Enam => DoJurisdiction::Us,
            Region::Weur => DoJurisdiction::Eu, // MANDATORY: Schrems II
            Region::Sam => DoJurisdiction::None,
        }
    }

    /// Validate that this jurisdiction is correct for the given region.
    ///
    /// CRITICAL for WEUR: must be `Eu`. Any mismatch = Schrems II violation risk.
    #[must_use]
    pub fn is_valid_for_region(&self, region: Region) -> bool {
        *self == Self::expected_for_region(region)
    }
}

impl std::fmt::Display for DoJurisdiction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Region health status for Prometheus gauge metric.
///
/// `corelink_region_health_status{region}` (gauge; 0=down/1=degraded/2=healthy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RegionHealthStatus {
    /// Region is completely down (no reads/writes succeeding).
    Down = 0,
    /// Region is degraded (partial service; failover may be engaged).
    Degraded = 1,
    /// Region is healthy (normal operation).
    Healthy = 2,
}

impl RegionHealthStatus {
    /// Prometheus gauge value.
    #[must_use]
    pub fn gauge_value(&self) -> u8 {
        *self as u8
    }

    /// Prometheus label string.
    #[must_use]
    pub fn as_label(&self) -> &'static str {
        match self {
            RegionHealthStatus::Down => "down",
            RegionHealthStatus::Degraded => "degraded",
            RegionHealthStatus::Healthy => "healthy",
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_region_all_coverage() {
        assert_eq!(Region::ALL.len(), 4);
        for r in Region::ALL {
            assert!(!r.as_str().is_empty());
            assert!(!r.r2_location_hint().is_empty());
            assert!(!r.display_name().is_empty());
            assert!(r.r2_bucket_name().contains(r.as_str()));
            assert!(r.d1_db_name().contains(r.as_str()));
            assert!(r.kv_namespace_title().contains(r.as_str()));
            assert!(r.custom_domain().contains(r.as_str()));
        }
    }

    #[test]
    fn test_region_from_str_roundtrip() {
        for r in Region::ALL {
            let parsed = Region::from_str(r.as_str()).expect("roundtrip");
            assert_eq!(*r, parsed);
        }
    }

    #[test]
    fn test_region_from_str_unknown() {
        assert!(Region::from_str("apac").is_err());
        assert!(Region::from_str("").is_err());
    }

    #[test]
    fn test_weur_jurisdiction_mandatory_eu() {
        let j = DoJurisdiction::expected_for_region(Region::Weur);
        assert_eq!(
            j,
            DoJurisdiction::Eu,
            "WEUR MUST have EU jurisdiction (Schrems II)"
        );
        assert!(DoJurisdiction::Eu.is_valid_for_region(Region::Weur));
        assert!(!DoJurisdiction::Us.is_valid_for_region(Region::Weur));
        assert!(!DoJurisdiction::None.is_valid_for_region(Region::Weur));
    }

    #[test]
    fn test_jurisdiction_per_region() {
        assert_eq!(
            DoJurisdiction::expected_for_region(Region::Wnam),
            DoJurisdiction::Us
        );
        assert_eq!(
            DoJurisdiction::expected_for_region(Region::Enam),
            DoJurisdiction::Us
        );
        assert_eq!(
            DoJurisdiction::expected_for_region(Region::Sam),
            DoJurisdiction::None
        );
    }

    #[test]
    fn test_region_health_status_gauge_values() {
        assert_eq!(RegionHealthStatus::Down.gauge_value(), 0);
        assert_eq!(RegionHealthStatus::Degraded.gauge_value(), 1);
        assert_eq!(RegionHealthStatus::Healthy.gauge_value(), 2);
    }

    #[test]
    fn test_serde_region() {
        let j = serde_json::to_string(&Region::Weur).expect("serialize");
        assert_eq!(j, r#""weur""#);
        let r: Region = serde_json::from_str(r#""wnam""#).expect("deserialize");
        assert_eq!(r, Region::Wnam);
    }
}
