//! Canonical [`Region`] enum — wave-33 Stage 0 cross-cutting type.
//!
//! Mirrors `corelink_region::Region` exactly. The canonical CoreLink
//! region set is 4-valued (WNAM / ENAM / WEUR / SAM) because Cloudflare
//! R2 `locationHint` + D1 `location` are the source of truth for data
//! residency. The wave-33 reorg spec §3 example labels (`SAM/IAD/LHR/
//! NRT/SYD`) are illustrative airport-codes; the actual canonical
//! values preserved here are the production R2/D1 region tokens.
//!
//! INV-DATA-RESIDENCY: tenant `primary_region` is pinned at signup;
//! cross-region writes → 403 + audit.

use serde::{Deserialize, Serialize};

/// Canonical CoreLink region identifier.
///
/// Maps to Cloudflare R2 `locationHint`, D1 `location`, and custom
/// domain `{region}.api.corelink.humangr.com`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Region {
    /// WNAM — us-west (Cloudflare R2/D1 `wnam`).
    Wnam,
    /// ENAM — us-east (Cloudflare R2/D1 `enam`).
    Enam,
    /// WEUR — eu-west (Cloudflare R2/D1 `weur`).
    /// DO jurisdiction = "eu" mandatory.
    Weur,
    /// SAM — sa-east (Cloudflare R2/D1 `sam`).
    Sam,
}

impl Region {
    /// All valid region values (for property tests + iterate-all
    /// patterns).
    pub const ALL: &'static [Region] =
        &[Region::Wnam, Region::Enam, Region::Weur, Region::Sam];

    /// Lowercase identifier string (used in resource names, metrics
    /// labels). Behaviour parity with `corelink_region::Region::as_str`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Region::Wnam => "wnam",
            Region::Enam => "enam",
            Region::Weur => "weur",
            Region::Sam => "sam",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn region_all_has_four_variants() {
        assert_eq!(Region::ALL.len(), 4);
    }

    #[test]
    fn region_as_str_lowercase() {
        for r in Region::ALL {
            let s = r.as_str();
            assert_eq!(s, s.to_lowercase());
        }
    }

    #[test]
    fn region_serde_round_trip_lowercase() {
        let json = serde_json::to_string(&Region::Weur).unwrap();
        assert_eq!(json, "\"weur\"");
        let back: Region = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Region::Weur);
    }
}
