//! [`Region`] enum + canonical bucket-name mapping.
//!
//! S-01 ships three regions (WI-S01-003 §6.1.4): WNAM (West North America),
//! WEUR (West Europe), SAM (South America). Bucket names follow the canonical
//! `cas-<region>` pattern from `remote_cache_product_profile.md §7.1`. Region
//! is stable, lower-cased ASCII; never user-supplied.

/// Cloudflare R2 region in which a CAS bucket is provisioned.
///
/// Each variant maps to a distinct R2 bucket binding in `wrangler.toml`
/// (`R2_CAS_WNAM`, `R2_CAS_WEUR`, `R2_CAS_SAM`). A `Region` is part of every
/// [`crate::TenantCtx`] so that residency (INV-DATA-RESIDENCY) is enforced at
/// adapter construction: a tenant pinned to WEUR cannot accidentally write to
/// the WNAM bucket because the writer carries its region as state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(
    feature = "tower-middleware",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "tower-middleware", serde(rename_all = "lowercase"))]
pub enum Region {
    /// West North America (Cloudflare hint: `wnam`).
    Wnam,
    /// West Europe (Cloudflare hint: `weur`).
    Weur,
    /// South America (Cloudflare hint: `sam`).
    Sam,
}

impl Region {
    /// Lower-case ASCII bucket suffix used in canonical R2 keys
    /// (`cas-<suffix>/...`).
    #[must_use]
    pub const fn bucket_suffix(self) -> &'static str {
        match self {
            Self::Wnam => "wnam",
            Self::Weur => "weur",
            Self::Sam => "sam",
        }
    }

    /// Canonical CAS bucket name for this region (`cas-<suffix>`).
    #[must_use]
    pub const fn cas_bucket_name(self) -> &'static str {
        match self {
            Self::Wnam => "cas-wnam",
            Self::Weur => "cas-weur",
            Self::Sam => "cas-sam",
        }
    }
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.bucket_suffix())
    }
}
