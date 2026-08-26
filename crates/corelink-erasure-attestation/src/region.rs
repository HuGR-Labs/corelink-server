//! [`Region`] — 5 CoreLink production regions (S-14; APAC added).

use serde::{Deserialize, Serialize};

/// CoreLink production region (5-region baseline, S-14; APAC added).
///
/// `#[non_exhaustive]` per CoreLink codex: future regions (AFR)
/// can be added without a breaking change.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Region {
    /// US West (primary: Cloudflare WNAM).
    Wnam,
    /// US East (Cloudflare ENAM).
    Enam,
    /// EU West (Cloudflare WEUR; EU data residency tenant primary region).
    Weur,
    /// South America (Cloudflare SAM).
    Sam,
    /// Asia-Pacific (Cloudflare APAC).
    Apac,
    /// Africa (Cloudflare AFR).
    Afr,
}

impl Region {
    /// Return the canonical lowercase string used in D1 CHECK constraints
    /// and R2 bucket suffixes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wnam => "wnam",
            Self::Enam => "enam",
            Self::Weur => "weur",
            Self::Sam => "sam",
            Self::Apac => "apac",
            Self::Afr => "afr",
        }
    }

    /// Parse the canonical lowercase region string (the value stored in the
    /// D1 `region` CHECK columns + `tenant.primary_region`). Returns `None` for
    /// any value outside the current region baseline.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "wnam" => Some(Self::Wnam),
            "enam" => Some(Self::Enam),
            "weur" => Some(Self::Weur),
            "sam" => Some(Self::Sam),
            "apac" => Some(Self::Apac),
            "afr" => Some(Self::Afr),
            _ => None,
        }
    }

    /// Return the R2 audit bucket name for this region.
    ///
    /// Convention: `corelink-audit-{region}`.
    #[must_use]
    pub fn audit_bucket(self) -> String {
        format!("corelink-audit-{}", self.as_str())
    }
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_str_roundtrip() {
        let regions = [
            Region::Wnam,
            Region::Enam,
            Region::Weur,
            Region::Sam,
            Region::Apac,
            Region::Afr,
        ];
        let expected = ["wnam", "enam", "weur", "sam", "apac", "afr"];
        for (r, e) in regions.iter().zip(expected.iter()) {
            assert_eq!(r.as_str(), *e);
        }
    }

    #[test]
    fn audit_bucket_name() {
        assert_eq!(Region::Weur.audit_bucket(), "corelink-audit-weur");
    }

    #[test]
    fn parse_roundtrips_canonical_and_rejects_unknown() {
        for r in [
            Region::Wnam,
            Region::Enam,
            Region::Weur,
            Region::Sam,
            Region::Apac,
            Region::Afr,
        ] {
            assert_eq!(Region::parse(r.as_str()), Some(r));
        }
        // APAC has an attestation region → Some.
        assert_eq!(Region::parse("apac"), Some(Region::Apac));
        // AFR has an attestation region → Some.
        assert_eq!(Region::parse("afr"), Some(Region::Afr));
        assert_eq!(Region::parse(""), None);
        assert_eq!(Region::parse("WEUR"), None); // case-sensitive (canonical lowercase)
    }
}
