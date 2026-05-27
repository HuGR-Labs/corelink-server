//! Canary assertion canonical taxonomy + SLO ceilings ladder.
//!
//! ## Why this ladder
//!
//! Per WI-S09-007 §1 invariant 2: each canary loop runs the canonical
//! CAS PUT + GET + AC LOOKUP synthetic flow and asserts:
//!
//! - `cas_put_p99_ms ≤ 100` — CAS write latency p99 ceiling.
//! - `cas_get_p99_ms ≤ 50` — CAS read latency p99 ceiling.
//! - `ac_lookup_p99_ms ≤ 30` — AC lookup latency p99 ceiling.
//! - BLAKE3 digest match — written digest ≡ read digest byte-for-byte.
//!
//! All 4 assertions pass = `CanaryDecision::Pass`; otherwise the
//! decision is `Degraded` (latency miss only) or `FailedRegion`
//! (digest mismatch — data integrity issue, SEV-1 alert).

/// Canonical canary assertion taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for follow-on sprints (e.g. S-14 enterprise
/// tier custom SLO ceilings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CanaryAssertion {
    /// CAS PUT latency p99 ceiling (canonical 100 ms).
    CasPutP99,
    /// CAS GET latency p99 ceiling (canonical 50 ms).
    CasGetP99,
    /// AC lookup latency p99 ceiling (canonical 30 ms).
    AcLookupP99,
    /// BLAKE3 digest match — written digest ≡ read digest byte-for-
    /// byte (data integrity invariant; SEV-1 on mismatch).
    DigestMatch,
}

impl CanaryAssertion {
    /// Canonical slug used in audit records + alert annotations.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::CasPutP99 => "cas_put_p99_ms",
            Self::CasGetP99 => "cas_get_p99_ms",
            Self::AcLookupP99 => "ac_lookup_p99_ms",
            Self::DigestMatch => "digest_match",
        }
    }
}

impl core::fmt::Display for CanaryAssertion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 4-element canary assertion list — pinned at the type
/// system layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_canary_assertions() -> &'static [CanaryAssertion; 4] {
    &[
        CanaryAssertion::CasPutP99,
        CanaryAssertion::CasGetP99,
        CanaryAssertion::AcLookupP99,
        CanaryAssertion::DigestMatch,
    ]
}

/// Canonical SLO ceiling ladder for the canary loop assertions.
/// All values are p99 milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssertionCeilings {
    /// CAS PUT latency p99 ceiling in ms (canonical 100).
    pub cas_put_p99_ms: u32,
    /// CAS GET latency p99 ceiling in ms (canonical 50).
    pub cas_get_p99_ms: u32,
    /// AC lookup latency p99 ceiling in ms (canonical 30).
    pub ac_lookup_p99_ms: u32,
}

impl AssertionCeilings {
    /// Canonical ceilings per WI-S09-007 §1 invariant 2.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            cas_put_p99_ms: 100,
            cas_get_p99_ms: 50,
            ac_lookup_p99_ms: 30,
        }
    }
}

impl Default for AssertionCeilings {
    fn default() -> Self {
        Self::canonical()
    }
}

/// Observed canary loop latencies in ms (per region, per loop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanaryLatenciesMs {
    /// Observed CAS PUT latency in ms.
    pub cas_put_ms: u32,
    /// Observed CAS GET latency in ms.
    pub cas_get_ms: u32,
    /// Observed AC lookup latency in ms.
    pub ac_lookup_ms: u32,
}

impl CanaryLatenciesMs {
    /// Construct latencies snapshot from the 3 canary loop steps.
    #[must_use]
    pub const fn new(cas_put_ms: u32, cas_get_ms: u32, ac_lookup_ms: u32) -> Self {
        Self {
            cas_put_ms,
            cas_get_ms,
            ac_lookup_ms,
        }
    }

    /// Whether every observed latency satisfies the canonical
    /// ceilings (i.e. all 3 latency assertions pass).
    #[must_use]
    pub fn within_ceilings(&self, ceilings: AssertionCeilings) -> bool {
        self.cas_put_ms <= ceilings.cas_put_p99_ms
            && self.cas_get_ms <= ceilings.cas_get_p99_ms
            && self.ac_lookup_ms <= ceilings.ac_lookup_p99_ms
    }

    /// Identify the first canonical assertion that fails (used for
    /// `CanaryAuditRecord::failed_assertion` audit metadata). Returns
    /// `None` if every latency is within ceilings.
    #[must_use]
    pub fn first_breach(&self, ceilings: AssertionCeilings) -> Option<CanaryAssertion> {
        if self.cas_put_ms > ceilings.cas_put_p99_ms {
            return Some(CanaryAssertion::CasPutP99);
        }
        if self.cas_get_ms > ceilings.cas_get_p99_ms {
            return Some(CanaryAssertion::CasGetP99);
        }
        if self.ac_lookup_ms > ceilings.ac_lookup_p99_ms {
            return Some(CanaryAssertion::AcLookupP99);
        }
        None
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
    fn canonical_ceilings_pinned() {
        let c = AssertionCeilings::canonical();
        assert_eq!(c.cas_put_p99_ms, 100);
        assert_eq!(c.cas_get_p99_ms, 50);
        assert_eq!(c.ac_lookup_p99_ms, 30);
    }

    #[test]
    fn assertion_slugs_pinned() {
        assert_eq!(CanaryAssertion::CasPutP99.slug(), "cas_put_p99_ms");
        assert_eq!(CanaryAssertion::CasGetP99.slug(), "cas_get_p99_ms");
        assert_eq!(CanaryAssertion::AcLookupP99.slug(), "ac_lookup_p99_ms");
        assert_eq!(CanaryAssertion::DigestMatch.slug(), "digest_match");
    }

    #[test]
    fn canonical_assertion_list_has_four_elements() {
        let v = canonical_canary_assertions();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for a in v {
            assert!(set.insert(a.slug()));
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn within_ceilings_strict_le_boundary() {
        let c = AssertionCeilings::canonical();
        assert!(CanaryLatenciesMs::new(100, 50, 30).within_ceilings(c));
        assert!(CanaryLatenciesMs::new(0, 0, 0).within_ceilings(c));
        assert!(!CanaryLatenciesMs::new(101, 50, 30).within_ceilings(c));
        assert!(!CanaryLatenciesMs::new(100, 51, 30).within_ceilings(c));
        assert!(!CanaryLatenciesMs::new(100, 50, 31).within_ceilings(c));
    }

    #[test]
    fn first_breach_canonical_order() {
        let c = AssertionCeilings::canonical();
        assert_eq!(CanaryLatenciesMs::new(100, 50, 30).first_breach(c), None);
        assert_eq!(
            CanaryLatenciesMs::new(101, 51, 31).first_breach(c),
            Some(CanaryAssertion::CasPutP99),
        );
        assert_eq!(
            CanaryLatenciesMs::new(100, 51, 31).first_breach(c),
            Some(CanaryAssertion::CasGetP99),
        );
        assert_eq!(
            CanaryLatenciesMs::new(100, 50, 31).first_breach(c),
            Some(CanaryAssertion::AcLookupP99),
        );
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", CanaryAssertion::DigestMatch), "digest_match");
    }
}
