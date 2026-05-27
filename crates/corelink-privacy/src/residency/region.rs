//! 6-region canonical closed enum — `privacy_model.md §7.1` + `data_model.md §2.1 L95`.
//!
//! **Cardinality discipline**: adding a new region requires an ADR +
//! Privacy Officer + Compliance review (ADR-S11-006 pattern; see
//! `ADR-S11-012` for cardinality governance). The enum is **closed**
//! (no open-string region drift) per anti-pattern §32.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 6-region canonical closed enum per `privacy_model.md §7.1` and `data_model.md §2.1 L95`.
///
/// Adding a region requires ADR + Privacy Officer + Compliance review
/// (cardinality discipline per anti-pattern §32 — open region string forbidden).
///
/// # Serialization
///
/// Serializes to/from lowercase snake_case string: `"wnam"`, `"enam"`,
/// `"weur"`, `"sam"`, `"apac"`, `"afr"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Western North America (Canada-based) — PIPEDA, CCPA.
    Wnam,
    /// Eastern North America (US-based) — CCPA, HIPAA opt-in.
    Enam,
    /// Western Europe (NL/DE based) — GDPR; Schrems II primary.
    Weur,
    /// South America (BR-based; cliente-âncora HuGR) — LGPD Art. 33 §1º.
    Sam,
    /// Asia-Pacific (SG-based) — PDPA SG, APPI JP. Phase 2.
    Apac,
    /// Africa (ZA-based) — POPIA. Phase 3.
    Afr,
}

impl Region {
    /// All 6 canonical regions in canonical order.
    pub const ALL: [Region; 6] = [
        Region::Wnam,
        Region::Enam,
        Region::Weur,
        Region::Sam,
        Region::Apac,
        Region::Afr,
    ];

    /// Parse from the canonical lowercase string used in DNS and D1.
    ///
    /// Returns `None` for any non-canonical string (open-string drift rejected
    /// per cardinality discipline anti-pattern §32).
    pub fn parse_canonical(s: &str) -> Option<Self> {
        match s {
            "wnam" => Some(Region::Wnam),
            "enam" => Some(Region::Enam),
            "weur" => Some(Region::Weur),
            "sam" => Some(Region::Sam),
            "apac" => Some(Region::Apac),
            "afr" => Some(Region::Afr),
            _ => None,
        }
    }

    /// Canonical lowercase string representation, used in DNS (`<tenant>.<region>.corelink.humangr.com`),
    /// D1 columns, and CloudEvent payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Region::Wnam => "wnam",
            Region::Enam => "enam",
            Region::Weur => "weur",
            Region::Sam => "sam",
            Region::Apac => "apac",
            Region::Afr => "afr",
        }
    }

    /// Returns the primary regulatory framework for this region.
    pub fn regulatory_framework(self) -> &'static str {
        match self {
            Region::Wnam => "PIPEDA, CCPA",
            Region::Enam => "CCPA, HIPAA opt-in",
            Region::Weur => "GDPR Art. 44, Schrems II (CJEU C-311/18)",
            Region::Sam => "LGPD Art. 33 §1º",
            Region::Apac => "PDPA SG, APPI JP",
            Region::Afr => "POPIA",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
