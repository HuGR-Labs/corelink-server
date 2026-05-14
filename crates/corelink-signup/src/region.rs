//! Region pinning canonical map per Lote 10.16 cookie-canonical fix.
//!
//! Per spec contract S-19 §5.1 R-S19-1 + WI §9.3: signup region is
//! pinned from the rendered locale cookie `corelink_locale` set by S-16
//! middleware (NOT directly from the `Accept-Language` header). The
//! Lote 10.19 codex P1 fix rejects `Accept-Language` as a direct DPA/
//! residency fallback; first-visit middleware translates header → cookie
//! BEFORE DPA flow renders; DPA capture always reads the cookie. This
//! crate models that contract as a pure-logic map.
//!
//! Region taxonomy (`PrimaryRegion`, `#[non_exhaustive]`):
//!
//! - `Enam` — default; `en-US` and unknown locales pin here.
//! - `Sam` — `pt-BR` pins here (South America DC; `corelink-region`
//!   alignment).
//! - `Eu` — `de-DE` / `fr-FR` / `es-ES` (EU resident locales pin here;
//!   reserved for S-14 cross-region routing).
//!
//! Once pinned, `tenant.primary_region` is **immutable post-signup** per
//! INV-REGION-NO-CROSS-LEAK (S-14 alignment); the orchestrator never
//! exposes a rewrite path.

/// BCP-47 locale newtype carrying the `corelink_locale` cookie value.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Bcp47Locale(String);

impl Bcp47Locale {
    /// Construct a new locale from any string-like value. Caller is
    /// responsible for normalising case + tag separator before passing.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Bcp47Locale {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Primary region a tenant is pinned to at signup time. Once persisted
/// to the `tenant.primary_region` D1 column, this value is immutable
/// per INV-REGION-NO-CROSS-LEAK (S-14 alignment).
///
/// `#[non_exhaustive]` reserves additive growth for follow-on region
/// expansion (e.g. APAC) without breaking changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PrimaryRegion {
    /// `enam` — Eastern North America (default; `en-US`, unknown).
    Enam,
    /// `sam` — South America (`pt-BR`).
    Sam,
    /// `eu` — Europe (reserved for S-14 EU residency routing).
    Eu,
}

impl PrimaryRegion {
    /// Canonical lowercase region label as persisted to D1
    /// `tenant.primary_region`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enam => "enam",
            Self::Sam => "sam",
            Self::Eu => "eu",
        }
    }

    /// Map a [`Bcp47Locale`] to the canonical [`PrimaryRegion`] per Lote
    /// 10.16 cookie-canonical fix. Unknown locales fall back to
    /// [`PrimaryRegion::Enam`] (default).
    ///
    /// Note: this is a pure-logic map. The caller is responsible for
    /// ensuring the locale comes from the `corelink_locale` cookie (set
    /// by S-16 middleware) and NOT directly from the `Accept-Language`
    /// header per Lote 10.19 codex P1 fix; this map intentionally
    /// refuses to encode any header-fallback logic.
    #[must_use]
    pub fn from_locale(locale: &Bcp47Locale) -> Self {
        let s = locale.as_str();
        // Normalise to lowercase for matching; primary subtag prefix
        // match keeps the map small + stable across regional variants.
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("pt-br") || lower == "pt" {
            Self::Sam
        } else if lower.starts_with("de") || lower.starts_with("fr") || lower.starts_with("es-es") {
            Self::Eu
        } else {
            Self::Enam
        }
    }
}

impl core::fmt::Display for PrimaryRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element list of [`PrimaryRegion`] for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_regions() -> &'static [&'static str; 3] {
    &["enam", "sam", "eu"]
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
    fn pt_br_maps_sam() {
        let r = PrimaryRegion::from_locale(&Bcp47Locale::new("pt-BR"));
        assert_eq!(r, PrimaryRegion::Sam);
        assert_eq!(r.as_str(), "sam");
    }

    #[test]
    fn en_us_maps_enam() {
        let r = PrimaryRegion::from_locale(&Bcp47Locale::new("en-US"));
        assert_eq!(r, PrimaryRegion::Enam);
    }

    #[test]
    fn de_maps_eu() {
        let r = PrimaryRegion::from_locale(&Bcp47Locale::new("de-DE"));
        assert_eq!(r, PrimaryRegion::Eu);
    }

    #[test]
    fn unknown_locale_falls_back_enam() {
        let r = PrimaryRegion::from_locale(&Bcp47Locale::new("zz-XX"));
        assert_eq!(r, PrimaryRegion::Enam);
    }

    #[test]
    fn canonical_regions_stable() {
        assert_eq!(canonical_regions(), &["enam", "sam", "eu"]);
    }
}
