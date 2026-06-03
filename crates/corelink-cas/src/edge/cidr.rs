//! Canonical CIDR (Classless Inter-Domain Routing) parser + matcher.
//!
//! Hand-rolled bit-level matcher that covers IPv4 (`<dotted>/<prefix>`,
//! prefix `0..=32`) + IPv6 (`<lowercased-zero-suppressed>/<prefix>`,
//! prefix `0..=128`). The hand-rolled implementation keeps the crate
//! wasm32-clean (no `ipnet` dep), keeps the surface to ~150 LOC, and
//! makes the load-bearing invariants (longest-prefix-match;
//! address-family isolation) trivial to property-test.
//!
//! ## Canonical normalisation
//!
//! [`Cidr::parse`] accepts liberal input forms and emits a canonical
//! [`Cidr`] value:
//!
//! - IPv4: `192.0.2.5/24` → `Cidr { family=V4, bits=0xC000_02_05_…, prefix=24 }`.
//! - IPv6: `2001:db8::1/32` → `Cidr { family=V6, bits=…, prefix=32 }`.
//! - The host bits BEYOND `prefix_len` are zeroed in the canonical
//!   form; the canonical text re-rendering uses the masked form.
//!
//! ## Longest-prefix-match
//!
//! [`Cidr::contains`] returns true iff the candidate address falls under
//! THIS network. [`longest_match`] scans a slice of [`Cidr`] entries and
//! returns the entry with the largest prefix length whose `contains`
//! predicate succeeds — this is the canonical pinned semantic per WI
//! §6.1.7 ("admin overlap rejected; runtime match resolves to most
//! specific").

use core::fmt;

/// Address family — closed canonical 2-set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CidrFamily {
    /// IPv4 (32-bit address space).
    V4,
    /// IPv6 (128-bit address space).
    V6,
}

impl CidrFamily {
    /// Maximum prefix length for THIS family.
    #[must_use]
    pub const fn max_prefix(self) -> u8 {
        match self {
            Self::V4 => 32,
            Self::V6 => 128,
        }
    }

    /// Canonical SQL-literal mnemonic ("4" / "6") matching the
    /// `cidr_family` CHECK constraint in the D1 migration.
    #[must_use]
    pub const fn as_int(self) -> u8 {
        match self {
            Self::V4 => 4,
            Self::V6 => 6,
        }
    }
}

/// Canonical CIDR (network + prefix length). `bits` carries the full
/// 128-bit address; for IPv4 the address occupies the LOW 32 bits with
/// the upper 96 bits zeroed (so the same matching machinery serves
/// both families).
///
/// The host bits beyond `prefix_len` are ALWAYS zeroed in the canonical
/// form; [`Cidr::parse`] enforces this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cidr {
    family: CidrFamily,
    /// Network address packed into 128 bits (big-endian; for IPv4 the
    /// address is right-aligned in the low 32 bits).
    bits: u128,
    /// Prefix length (0..=32 for IPv4; 0..=128 for IPv6).
    prefix_len: u8,
}

/// IP address — canonical 4-octet IPv4 + 16-octet IPv6 wrapper. The
/// matching machinery interprets the value via the family-tagged
/// `bits` interpretation: IPv4 occupies the low 32 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IpAddr {
    family: CidrFamily,
    bits: u128,
}

impl IpAddr {
    /// Construct an IPv4 address from 4 octets.
    #[must_use]
    pub const fn v4(octets: [u8; 4]) -> Self {
        let bits = ((octets[0] as u128) << 24)
            | ((octets[1] as u128) << 16)
            | ((octets[2] as u128) << 8)
            | (octets[3] as u128);
        Self {
            family: CidrFamily::V4,
            bits,
        }
    }

    /// Construct an IPv6 address from 16 octets.
    #[must_use]
    #[allow(
        clippy::indexing_slicing,
        reason = "const fn requires direct indexing; the loop guard `i < 16` keeps the access in-bounds for the [u8; 16] input"
    )]
    pub const fn v6(octets: [u8; 16]) -> Self {
        let mut bits: u128 = 0;
        let mut i = 0;
        while i < 16 {
            bits = (bits << 8) | (octets[i] as u128);
            i += 1;
        }
        Self {
            family: CidrFamily::V6,
            bits,
        }
    }

    /// Address family for THIS address.
    #[must_use]
    pub const fn family(self) -> CidrFamily {
        self.family
    }

    /// Packed bits (IPv4 right-aligned in low 32 bits; IPv6 full 128).
    #[must_use]
    pub const fn bits(self) -> u128 {
        self.bits
    }

    /// Parse a textual literal — accepts `192.0.2.5` (dotted IPv4) or
    /// `2001:db8::1` (lowercased IPv6). Returns `None` on any malformed
    /// input.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.contains(':') {
            parse_ipv6(s).map(|bits| Self {
                family: CidrFamily::V6,
                bits,
            })
        } else {
            parse_ipv4(s).map(|bits| Self {
                family: CidrFamily::V4,
                bits: u128::from(bits),
            })
        }
    }
}

/// Errors surfaced by [`Cidr::parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CidrParseError {
    /// Missing `/` separator between address and prefix length.
    MissingPrefix,
    /// Prefix length not parseable as u8.
    MalformedPrefix,
    /// Prefix length out of range for THIS address family
    /// (0..=32 for IPv4; 0..=128 for IPv6).
    PrefixOutOfRange {
        /// The offending prefix length.
        prefix: u32,
        /// The maximum allowed for THIS family.
        max: u8,
    },
    /// Address part malformed (wrong digit count / non-numeric / etc.).
    MalformedAddress,
}

impl fmt::Display for CidrParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("CIDR missing '/' prefix separator"),
            Self::MalformedPrefix => f.write_str("CIDR prefix length not parseable"),
            Self::PrefixOutOfRange { prefix, max } => {
                write!(f, "CIDR prefix {prefix} out of range (max {max})")
            }
            Self::MalformedAddress => f.write_str("CIDR address malformed"),
        }
    }
}

impl std::error::Error for CidrParseError {}

impl Cidr {
    /// Construct from a pre-validated network address + prefix.
    ///
    /// Returns `None` when:
    /// - `prefix_len > family.max_prefix()`.
    ///
    /// The host bits beyond `prefix_len` are masked away so the
    /// canonical form holds.
    #[must_use]
    pub fn new(addr: IpAddr, prefix_len: u8) -> Option<Self> {
        if prefix_len > addr.family.max_prefix() {
            return None;
        }
        let mask = prefix_mask(addr.family, prefix_len);
        Some(Self {
            family: addr.family,
            bits: addr.bits & mask,
            prefix_len,
        })
    }

    /// Parse a textual CIDR literal. Accepts `192.0.2.0/24` or
    /// `2001:db8::/32`. The host bits beyond the prefix are masked away
    /// in the returned canonical form.
    ///
    /// # Errors
    ///
    /// Surface [`CidrParseError`] on any malformed input.
    pub fn parse(s: &str) -> Result<Self, CidrParseError> {
        let (addr_str, prefix_str) = s.split_once('/').ok_or(CidrParseError::MissingPrefix)?;
        let prefix: u32 = prefix_str
            .parse()
            .map_err(|_| CidrParseError::MalformedPrefix)?;
        let ip = IpAddr::parse(addr_str).ok_or(CidrParseError::MalformedAddress)?;
        let max = ip.family.max_prefix();
        let prefix_u8 =
            u8::try_from(prefix).map_err(|_| CidrParseError::PrefixOutOfRange { prefix, max })?;
        if prefix_u8 > max {
            return Err(CidrParseError::PrefixOutOfRange { prefix, max });
        }
        let mask = prefix_mask(ip.family, prefix_u8);
        Ok(Self {
            family: ip.family,
            bits: ip.bits & mask,
            prefix_len: prefix_u8,
        })
    }

    /// Family for THIS network.
    #[must_use]
    pub const fn family(&self) -> CidrFamily {
        self.family
    }

    /// Prefix length (0..=32 for IPv4; 0..=128 for IPv6).
    #[must_use]
    pub const fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    /// Network address bits (host bits zeroed).
    #[must_use]
    pub const fn bits(&self) -> u128 {
        self.bits
    }

    /// Whether `addr` falls inside THIS network. Cross-family always
    /// returns false (IPv4 vs IPv6 is an architectural mismatch, NOT a
    /// "no match" result).
    #[must_use]
    pub fn contains(&self, addr: IpAddr) -> bool {
        if self.family != addr.family {
            return false;
        }
        let mask = prefix_mask(self.family, self.prefix_len);
        (addr.bits & mask) == self.bits
    }

    /// Canonical text form: `<dotted-or-colonised>/<prefix>` with host
    /// bits masked away. The IPv6 path emits the unabbreviated form
    /// (zero-suppression is for human display only; we keep the
    /// canonical D1 PK comparison stable).
    #[must_use]
    pub fn to_canonical_text(&self) -> String {
        match self.family {
            CidrFamily::V4 => {
                let v = self.bits as u32;
                let o0 = (v >> 24) & 0xff;
                let o1 = (v >> 16) & 0xff;
                let o2 = (v >> 8) & 0xff;
                let o3 = v & 0xff;
                format!("{o0}.{o1}.{o2}.{o3}/{}", self.prefix_len)
            }
            CidrFamily::V6 => {
                let mut groups = [0u16; 8];
                for (i, slot) in groups.iter_mut().enumerate() {
                    let shift = (7 - i) * 16;
                    *slot = ((self.bits >> shift) & 0xffff) as u16;
                }
                let parts: Vec<String> = groups.iter().map(|g| format!("{g:x}")).collect();
                format!("{}/{}", parts.join(":"), self.prefix_len)
            }
        }
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical_text())
    }
}

/// Find the most-specific [`Cidr`] in `entries` that contains `addr`.
/// Returns `None` when no entry matches.
///
/// Pinned by the canonical longest-prefix-match semantic per WI §6.1.7
/// (admin add of `/24` over an existing `/16` is REJECTED, but a
/// runtime match against a candidate covered by both networks must
/// resolve to the most specific). For this WI's storage layer we
/// reject overlap up-front (admin intent ambiguous); the matcher still
/// implements the canonical semantic so the trait surface is correct
/// for follow-on extensions.
#[must_use]
pub fn longest_match(entries: &[Cidr], addr: IpAddr) -> Option<&Cidr> {
    let mut best: Option<&Cidr> = None;
    for entry in entries {
        if !entry.contains(addr) {
            continue;
        }
        match best {
            None => best = Some(entry),
            Some(prev) if entry.prefix_len > prev.prefix_len => best = Some(entry),
            _ => {}
        }
    }
    best
}

/// Whether two networks overlap (one contains the other). Used by the
/// admin add-block path to reject ambiguous overlap up-front per WI
/// §6.1.7.
#[must_use]
pub fn overlaps(a: &Cidr, b: &Cidr) -> bool {
    if a.family != b.family {
        return false;
    }
    let shorter = a.prefix_len.min(b.prefix_len);
    let mask = prefix_mask(a.family, shorter);
    (a.bits & mask) == (b.bits & mask)
}

fn prefix_mask(family: CidrFamily, prefix_len: u8) -> u128 {
    let max = family.max_prefix();
    if prefix_len == 0 {
        return 0;
    }
    if prefix_len >= max {
        return match family {
            CidrFamily::V4 => 0xFFFF_FFFF_u128,
            CidrFamily::V6 => u128::MAX,
        };
    }
    let unused = max - prefix_len;
    let raw = u128::MAX << unused;
    match family {
        CidrFamily::V4 => raw & 0xFFFF_FFFF_u128,
        CidrFamily::V6 => raw,
    }
}

fn parse_ipv4(s: &str) -> Option<u32> {
    let mut octets = [0u32; 4];
    let mut idx = 0;
    for part in s.split('.') {
        if idx >= 4 {
            return None;
        }
        if part.is_empty() {
            return None;
        }
        let v: u32 = part.parse().ok()?;
        if v > 255 {
            return None;
        }
        if let Some(slot) = octets.get_mut(idx) {
            *slot = v;
        } else {
            return None;
        }
        idx += 1;
    }
    if idx != 4 {
        return None;
    }
    let o0 = octets.first().copied()?;
    let o1 = octets.get(1).copied()?;
    let o2 = octets.get(2).copied()?;
    let o3 = octets.get(3).copied()?;
    Some((o0 << 24) | (o1 << 16) | (o2 << 8) | o3)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the explicit bounds check above guarantees <= 0xffff fits in u16"
)]
fn parse_ipv6(s: &str) -> Option<u128> {
    let parts: Vec<&str> = s.split("::").collect();
    let groups: Vec<u16> = match parts.len() {
        1 => {
            let split: Vec<&str> = s.split(':').collect();
            if split.len() != 8 {
                return None;
            }
            let mut out = Vec::with_capacity(8);
            for part in split {
                if part.is_empty() {
                    return None;
                }
                let v = u32::from_str_radix(part, 16).ok()?;
                if v > 0xffff {
                    return None;
                }
                out.push(v as u16);
            }
            out
        }
        2 => {
            let head = parts.first().copied().unwrap_or("");
            let tail = parts.get(1).copied().unwrap_or("");
            let head_groups: Vec<u16> = if head.is_empty() {
                Vec::new()
            } else {
                let split: Vec<&str> = head.split(':').collect();
                let mut out = Vec::with_capacity(split.len());
                for part in split {
                    if part.is_empty() {
                        return None;
                    }
                    let v = u32::from_str_radix(part, 16).ok()?;
                    if v > 0xffff {
                        return None;
                    }
                    out.push(v as u16);
                }
                out
            };
            let tail_groups: Vec<u16> = if tail.is_empty() {
                Vec::new()
            } else {
                let split: Vec<&str> = tail.split(':').collect();
                let mut out = Vec::with_capacity(split.len());
                for part in split {
                    if part.is_empty() {
                        return None;
                    }
                    let v = u32::from_str_radix(part, 16).ok()?;
                    if v > 0xffff {
                        return None;
                    }
                    out.push(v as u16);
                }
                out
            };
            let total = head_groups.len() + tail_groups.len();
            if total > 8 {
                return None;
            }
            let zero_groups = 8 - total;
            let mut out = Vec::with_capacity(8);
            out.extend_from_slice(&head_groups);
            out.resize(out.len() + zero_groups, 0);
            out.extend_from_slice(&tail_groups);
            out
        }
        _ => return None,
    };
    if groups.len() != 8 {
        return None;
    }
    let mut bits: u128 = 0;
    for g in groups {
        bits = (bits << 16) | u128::from(g);
    }
    Some(bits)
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
    fn ipv4_parse_dotted_quad() {
        let ip = IpAddr::parse("192.0.2.5").unwrap();
        assert_eq!(ip.family(), CidrFamily::V4);
        assert_eq!(ip.bits(), 0xC000_0205);
    }

    #[test]
    fn ipv4_parse_rejects_octet_overflow() {
        assert!(IpAddr::parse("256.0.0.0").is_none());
        assert!(IpAddr::parse("1.2.3").is_none());
        assert!(IpAddr::parse("1.2.3.4.5").is_none());
        assert!(IpAddr::parse("1..2.3.4").is_none());
        assert!(IpAddr::parse("a.b.c.d").is_none());
    }

    #[test]
    fn ipv6_parse_full_form() {
        let ip = IpAddr::parse("2001:db8:0:0:0:0:0:1").unwrap();
        assert_eq!(ip.family(), CidrFamily::V6);
        // 2001:0db8:0000:0000:0000:0000:0000:0001
        assert_eq!(ip.bits(), 0x2001_0db8_0000_0000_0000_0000_0000_0001_u128);
    }

    #[test]
    fn ipv6_parse_compressed_form() {
        let ip = IpAddr::parse("2001:db8::1").unwrap();
        assert_eq!(ip.bits(), 0x2001_0db8_0000_0000_0000_0000_0000_0001_u128);
    }

    #[test]
    fn ipv6_parse_double_colon_only() {
        let ip = IpAddr::parse("::1").unwrap();
        assert_eq!(ip.bits(), 0x1u128);
        let ip = IpAddr::parse("::").unwrap();
        assert_eq!(ip.bits(), 0u128);
    }

    #[test]
    fn ipv6_parse_rejects_malformed() {
        assert!(IpAddr::parse(":::1").is_none());
        assert!(IpAddr::parse("zz::1").is_none());
        assert!(IpAddr::parse("12345::1").is_none());
        assert!(IpAddr::parse("1:2:3:4:5:6:7").is_none());
    }

    #[test]
    fn cidr_parse_ipv4_24() {
        let c = Cidr::parse("192.0.2.5/24").unwrap();
        assert_eq!(c.family(), CidrFamily::V4);
        assert_eq!(c.prefix_len(), 24);
        assert_eq!(c.bits(), 0xC000_0200);
        assert_eq!(c.to_canonical_text(), "192.0.2.0/24");
    }

    #[test]
    fn cidr_parse_ipv6_64() {
        let c = Cidr::parse("2001:db8:abcd:1234::1/64").unwrap();
        assert_eq!(c.family(), CidrFamily::V6);
        assert_eq!(c.prefix_len(), 64);
        let canonical = c.to_canonical_text();
        assert!(canonical.starts_with("2001:db8:abcd:1234:"));
        assert!(canonical.ends_with("/64"));
    }

    #[test]
    fn cidr_parse_rejects_missing_slash() {
        assert!(matches!(
            Cidr::parse("192.0.2.0").unwrap_err(),
            CidrParseError::MissingPrefix
        ));
    }

    #[test]
    fn cidr_parse_rejects_overlong_prefix() {
        let err = Cidr::parse("192.0.2.0/40").unwrap_err();
        assert!(matches!(
            err,
            CidrParseError::PrefixOutOfRange {
                prefix: 40,
                max: 32
            }
        ));
        let err = Cidr::parse("2001:db8::/200").unwrap_err();
        assert!(matches!(err, CidrParseError::PrefixOutOfRange { .. }));
    }

    #[test]
    fn cidr_parse_rejects_malformed_address() {
        let err = Cidr::parse("999.0.0.0/24").unwrap_err();
        assert!(matches!(err, CidrParseError::MalformedAddress));
    }

    #[test]
    fn cidr_contains_ipv4_24() {
        let c = Cidr::parse("192.0.2.0/24").unwrap();
        assert!(c.contains(IpAddr::parse("192.0.2.0").unwrap()));
        assert!(c.contains(IpAddr::parse("192.0.2.255").unwrap()));
        assert!(!c.contains(IpAddr::parse("192.0.3.0").unwrap()));
    }

    #[test]
    fn cidr_contains_ipv6_64() {
        let c = Cidr::parse("2001:db8::/32").unwrap();
        assert!(c.contains(IpAddr::parse("2001:db8::1").unwrap()));
        assert!(c.contains(IpAddr::parse("2001:db8:ffff::1").unwrap()));
        assert!(!c.contains(IpAddr::parse("2001:db9::1").unwrap()));
    }

    #[test]
    fn cidr_contains_rejects_cross_family() {
        let v4 = Cidr::parse("0.0.0.0/0").unwrap();
        let v6_addr = IpAddr::parse("::1").unwrap();
        assert!(!v4.contains(v6_addr));
        let v6 = Cidr::parse("::/0").unwrap();
        let v4_addr = IpAddr::parse("0.0.0.0").unwrap();
        assert!(!v6.contains(v4_addr));
    }

    #[test]
    fn cidr_zero_prefix_matches_all_within_family() {
        let v4 = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(v4.contains(IpAddr::parse("8.8.8.8").unwrap()));
        let v6 = Cidr::parse("::/0").unwrap();
        assert!(v6.contains(IpAddr::parse("2001:db8::1").unwrap()));
    }

    #[test]
    fn cidr_max_prefix_matches_only_self() {
        let v4 = Cidr::parse("203.0.113.5/32").unwrap();
        assert!(v4.contains(IpAddr::parse("203.0.113.5").unwrap()));
        assert!(!v4.contains(IpAddr::parse("203.0.113.6").unwrap()));
        let v6 = Cidr::parse("2001:db8::1/128").unwrap();
        assert!(v6.contains(IpAddr::parse("2001:db8::1").unwrap()));
        assert!(!v6.contains(IpAddr::parse("2001:db8::2").unwrap()));
    }

    #[test]
    fn host_bits_zeroed_in_canonical_form() {
        let c = Cidr::parse("192.0.2.123/24").unwrap();
        assert_eq!(c.bits(), 0xC000_0200);
        assert_eq!(c.to_canonical_text(), "192.0.2.0/24");
    }

    #[test]
    fn longest_match_picks_most_specific() {
        let nets = vec![
            Cidr::parse("192.0.0.0/8").unwrap(),
            Cidr::parse("192.0.2.0/24").unwrap(),
            Cidr::parse("192.0.2.0/30").unwrap(),
        ];
        let m = longest_match(&nets, IpAddr::parse("192.0.2.1").unwrap()).unwrap();
        assert_eq!(m.prefix_len(), 30);
    }

    #[test]
    fn longest_match_returns_none_when_no_match() {
        let nets = vec![Cidr::parse("10.0.0.0/8").unwrap()];
        assert!(longest_match(&nets, IpAddr::parse("192.0.2.1").unwrap()).is_none());
    }

    #[test]
    fn longest_match_skips_other_family() {
        let nets = vec![Cidr::parse("::/0").unwrap()];
        assert!(longest_match(&nets, IpAddr::parse("203.0.113.1").unwrap()).is_none());
    }

    #[test]
    fn overlaps_detects_containment() {
        let big = Cidr::parse("192.0.0.0/16").unwrap();
        let small = Cidr::parse("192.0.2.0/24").unwrap();
        assert!(overlaps(&big, &small));
        assert!(overlaps(&small, &big));
    }

    #[test]
    fn overlaps_rejects_cross_family() {
        let v4 = Cidr::parse("0.0.0.0/0").unwrap();
        let v6 = Cidr::parse("::/0").unwrap();
        assert!(!overlaps(&v4, &v6));
    }

    #[test]
    fn overlaps_rejects_disjoint() {
        let a = Cidr::parse("10.0.0.0/8").unwrap();
        let b = Cidr::parse("11.0.0.0/8").unwrap();
        assert!(!overlaps(&a, &b));
    }

    #[test]
    fn cidr_new_rejects_overlong_prefix() {
        let v4 = IpAddr::parse("192.0.2.0").unwrap();
        assert!(Cidr::new(v4, 33).is_none());
        let v6 = IpAddr::parse("::1").unwrap();
        assert!(Cidr::new(v6, 129).is_none());
    }

    #[test]
    fn cidr_family_max_prefix_canonical() {
        assert_eq!(CidrFamily::V4.max_prefix(), 32);
        assert_eq!(CidrFamily::V6.max_prefix(), 128);
    }

    #[test]
    fn cidr_family_as_int_matches_sql_check() {
        assert_eq!(CidrFamily::V4.as_int(), 4);
        assert_eq!(CidrFamily::V6.as_int(), 6);
    }

    #[test]
    fn display_matches_canonical_text() {
        let c = Cidr::parse("192.0.2.0/24").unwrap();
        assert_eq!(format!("{c}"), "192.0.2.0/24");
    }
}
