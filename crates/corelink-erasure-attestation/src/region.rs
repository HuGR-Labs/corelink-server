//! [`Region`] — 4 CoreLink production regions (S-14).

use serde::{Deserialize, Serialize};

/// CoreLink production region (4-region baseline, S-14).
///
/// `#[non_exhaustive]` per CoreLink codex: future regions (APAC, AFR)
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
        let regions = [Region::Wnam, Region::Enam, Region::Weur, Region::Sam];
        let expected = ["wnam", "enam", "weur", "sam"];
        for (r, e) in regions.iter().zip(expected.iter()) {
            assert_eq!(r.as_str(), *e);
        }
    }

    #[test]
    fn audit_bucket_name() {
        assert_eq!(Region::Weur.audit_bucket(), "corelink-audit-weur");
    }
}
