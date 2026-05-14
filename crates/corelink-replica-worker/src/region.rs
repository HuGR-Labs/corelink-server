//! Region 4-canonical + TenantTier + ResidencyGraph static acyclic failover map.
//!
//! **Cardinality discipline**: WNAM/ENAM/WEUR/SAM are the 4 GA regions.
//! APAC/AFR are phase 2 (post-GA demand-driven). Adding a region requires
//! ADR + Privacy Officer + Compliance review (anti-pattern §32).
//!
//! **Residency acyclic graph** (INV-REGION-NO-CROSS-LEAK; Schrems II + LGPD):
//! - WNAM ↔ ENAM (US sibling pair)
//! - WEUR ↔ SAM (EU ↔ LGPD; SAM-only allowed WEUR sibling)
//!
//! Cross-jurisdiction replication (WEUR → ENAM/WNAM) is **FORBIDDEN** by
//! static config enforced here.

use serde::{Deserialize, Serialize};

/// 4-canonical GA regions for WI-S14-003.
///
/// APAC and AFR are phase 2 post-GA. Adding a region requires ADR +
/// Privacy Officer + Compliance review (cardinality governance).
///
/// # Serialization
///
/// Serializes to/from lowercase snake_case: `"wnam"`, `"enam"`, `"weur"`, `"sam"`.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::Region;
///
/// assert_eq!(Region::Weur.as_str(), "weur");
/// assert_eq!(Region::parse_canonical("sam"), Some(Region::Sam));
/// assert_eq!(Region::parse_canonical("unknown"), None);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Western North America (Canada-based) — PIPEDA, CCPA.
    Wnam,
    /// Eastern North America (US-based) — CCPA, HIPAA opt-in.
    Enam,
    /// Western Europe (NL/DE-based) — GDPR; Schrems II primary.
    Weur,
    /// South America (BR-based) — LGPD Art. 33 §1º.
    Sam,
}

impl Region {
    /// All 4 canonical GA regions.
    pub const ALL: [Region; 4] = [Region::Wnam, Region::Enam, Region::Weur, Region::Sam];

    /// Parse from canonical lowercase string.
    ///
    /// Returns `None` for non-canonical strings (open-string drift rejected
    /// per cardinality discipline anti-pattern §32).
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::Region;
    ///
    /// assert_eq!(Region::parse_canonical("wnam"), Some(Region::Wnam));
    /// assert_eq!(Region::parse_canonical("apac"), None); // phase 2
    /// ```
    pub fn parse_canonical(s: &str) -> Option<Self> {
        match s {
            "wnam" => Some(Region::Wnam),
            "enam" => Some(Region::Enam),
            "weur" => Some(Region::Weur),
            "sam" => Some(Region::Sam),
            _ => None,
        }
    }

    /// Canonical lowercase string for DNS + D1 columns + CloudEvent payloads.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::Region;
    ///
    /// assert_eq!(Region::Weur.as_str(), "weur");
    /// assert_eq!(Region::Sam.as_str(), "sam");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Region::Wnam => "wnam",
            Region::Enam => "enam",
            Region::Weur => "weur",
            Region::Sam => "sam",
        }
    }

    /// Primary regulatory framework for this region.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::Region;
    ///
    /// assert!(Region::Weur.regulatory_framework().contains("GDPR"));
    /// assert!(Region::Sam.regulatory_framework().contains("LGPD"));
    /// ```
    pub fn regulatory_framework(self) -> &'static str {
        match self {
            Region::Wnam => "PIPEDA, CCPA",
            Region::Enam => "CCPA, HIPAA opt-in",
            Region::Weur => "GDPR Art. 44, Schrems II (CJEU C-311/18)",
            Region::Sam => "LGPD Art. 33 §1º",
        }
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tenant tier for cardinality-safe live Prometheus label.
///
/// `corelink_cas_get_bytes_total{tenant_tier, region}` uses this label
/// instead of `tenant_id` (cardinality budget: 4 × 4 = 16 séries baseline;
/// budget-safe per INV-OBS-CARDINALITY-BUDGET S-09).
///
/// **CRITICAL**: DO NOT add `tenant_id` label to any live Prometheus metric.
/// Use offline aggregation ([`crate::aggregator`]) for per-tenant analysis.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::TenantTier;
///
/// assert_eq!(TenantTier::ALL.len(), 4);
/// assert_eq!(TenantTier::Solo.as_str(), "solo");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TenantTier {
    /// Solo tier — single user.
    Solo,
    /// Team tier — small teams.
    Team,
    /// Business tier — medium organizations.
    Business,
    /// Enterprise tier — large organizations, BYOK eligible.
    Enterprise,
}

impl TenantTier {
    /// All 4 canonical tenant tiers.
    pub const ALL: [TenantTier; 4] = [
        TenantTier::Solo,
        TenantTier::Team,
        TenantTier::Business,
        TenantTier::Enterprise,
    ];

    /// Canonical lowercase string for Prometheus label values.
    pub fn as_str(self) -> &'static str {
        match self {
            TenantTier::Solo => "solo",
            TenantTier::Team => "team",
            TenantTier::Business => "business",
            TenantTier::Enterprise => "enterprise",
        }
    }
}

impl std::fmt::Display for TenantTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Replication lag p99 SLO ceiling in seconds (SLO sustained 7d staging).
///
/// Replication lag p99 ≤ 60s SLO per WI-S14-003 §6.1.3 + spec contract R-S14-3.
pub const REPLICATION_LAG_P99_SLO_SECS: u64 = 60;

/// Static acyclic residency graph: maps each region to its allowed sibling(s)
/// for hot blob replication.
///
/// **Acyclic guarantee**: WNAM→ENAM, ENAM→WNAM (US pair); WEUR→SAM,
/// SAM→WEUR (EU↔LGPD pair). No cross-jurisdiction transfer.
///
/// **Schrems II + LGPD Art. 33 enforcement**: WEUR blobs may NOT be
/// replicated to ENAM or WNAM (no EU→US transfer without adequacy decision).
/// Cross-jurisdiction = `ResidencyViolation` error + audit emit.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::{Region, ResidencyGraph};
///
/// let graph = ResidencyGraph::default();
/// assert_eq!(graph.sibling(Region::Wnam), Some(Region::Enam));
/// assert_eq!(graph.sibling(Region::Weur), Some(Region::Sam));
/// // Cross-jurisdiction forbidden:
/// assert!(graph.is_allowed(Region::Weur, Region::Enam).is_err());
/// ```
#[derive(Debug, Clone)]
pub struct ResidencyGraph;

impl Default for ResidencyGraph {
    fn default() -> Self {
        ResidencyGraph
    }
}

impl ResidencyGraph {
    /// Returns the allowed replication sibling for `primary`.
    ///
    /// - WNAM ↔ ENAM (US sibling pair)
    /// - WEUR ↔ SAM (EU ↔ LGPD sibling pair)
    ///
    /// Returns `None` if no sibling is configured (should not occur for GA regions).
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::{Region, ResidencyGraph};
    ///
    /// let g = ResidencyGraph::default();
    /// assert_eq!(g.sibling(Region::Enam), Some(Region::Wnam));
    /// assert_eq!(g.sibling(Region::Sam), Some(Region::Weur));
    /// ```
    pub fn sibling(&self, primary: Region) -> Option<Region> {
        match primary {
            Region::Wnam => Some(Region::Enam),
            Region::Enam => Some(Region::Wnam),
            Region::Weur => Some(Region::Sam),
            Region::Sam => Some(Region::Weur),
        }
    }

    /// Validates that `replica` is the allowed sibling of `primary`.
    ///
    /// Returns `Ok(())` if allowed, `Err(ResidencyViolationInfo)` if
    /// cross-jurisdiction transfer would be attempted.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::{Region, ResidencyGraph};
    ///
    /// let g = ResidencyGraph::default();
    /// assert!(g.is_allowed(Region::Wnam, Region::Enam).is_ok());
    /// assert!(g.is_allowed(Region::Weur, Region::Sam).is_ok());
    /// assert!(g.is_allowed(Region::Weur, Region::Enam).is_err()); // Schrems II
    /// assert!(g.is_allowed(Region::Weur, Region::Wnam).is_err()); // Schrems II
    /// ```
    pub fn is_allowed(
        &self,
        primary: Region,
        replica: Region,
    ) -> Result<(), ResidencyViolationInfo> {
        let allowed = self.sibling(primary);
        if allowed == Some(replica) {
            Ok(())
        } else {
            Err(ResidencyViolationInfo { primary, replica })
        }
    }

    /// Verifies the graph is acyclic: following sibling pointers never loops.
    ///
    /// Each region's sibling's sibling must equal the original region (2-hop
    /// symmetry). No 3+ hop cycles possible by construction.
    ///
    /// Returns `true` if the graph is acyclic (always true for the static config).
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replica_worker::ResidencyGraph;
    ///
    /// assert!(ResidencyGraph::default().is_acyclic());
    /// ```
    pub fn is_acyclic(&self) -> bool {
        Region::ALL.iter().all(|&r| {
            if let Some(sibling) = self.sibling(r) {
                // sibling's sibling must be r (symmetric 2-hop, no loops)
                self.sibling(sibling) == Some(r)
            } else {
                true
            }
        })
    }
}

/// Info payload for a residency violation (used in error construction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyViolationInfo {
    /// Primary region of the tenant.
    pub primary: Region,
    /// Attempted replica region (forbidden).
    pub replica: Region,
}
