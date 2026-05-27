//! CoreLink supply-chain policy validation library (WI-S12-004).
//!
//! This crate provides:
//! - [`LicenseClass`]: classification of an SPDX expression against the
//!   `deny.toml` allowlist (INV-SUPPLY-LICENSE-ALLOWLIST).
//! - [`SourceKind`]: enumeration of dependency source types (crates.io,
//!   unknown-registry, git-unpinned, git-pinned) for source policy
//!   (INV-SUPPLY-NO-YANKED).
//! - [`DenyPolicyOutcome`]: result type returned by the policy validator
//!   used in property + adversarial tests.
//!
//! All policy decisions must match the canonical `deny.toml` at the
//! workspace root.  Any discrepancy is a bug — tests in this crate
//! serve as the executable specification.
//!
//! # Invariants
//!
//! - **INV-SUPPLY-LICENSE-ALLOWLIST** (HIGH): zero deps outside the
//!   7-OSI-approved allowlist (MIT/Apache-2.0/BSD-2/BSD-3/ISC/MPL-2.0/
//!   Unicode-DFS-2016) plus accepted extensions.
//! - **INV-SUPPLY-NO-YANKED** (HIGH): zero yanked deps in `Cargo.lock`.
//!
//! # Feature gates
//!
//! No Rust code in this crate targets wasm32 at runtime — all logic is
//! test-only host tooling.  The crate carries no `cfg(target_arch = "wasm32")`
//! branches.

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// License classification
// ---------------------------------------------------------------------------

/// Classification of an SPDX license expression against the canonical
/// `deny.toml` allowlist/denylist.
///
/// The set of allowed licenses exactly mirrors `[licenses].allow` in
/// `deny.toml` at the workspace root.  This enum is `#[non_exhaustive]`
/// so that adding a new allowed license (via ADR + version bump of
/// `deny.toml`) is not a breaking change for downstream test harness code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LicenseClass {
    /// Allowed by the deny.toml allowlist (permissive / weak-copyleft OSI).
    Allowed,
    /// Explicitly banned (GPL/AGPL/SSPL/Commons-Clause/BUSL).
    Banned,
    /// Unknown / unparseable SPDX expression — treated as deny per
    /// `[licenses].default = "deny"`.
    Unknown,
    /// Copyleft (strong) that is not in the explicit deny list but caught
    /// by `copyleft = "deny"`.
    Copyleft,
    /// Unlicensed external dep (workspace private crates are exempted via
    /// `[licenses.private].ignore = true`).
    Unlicensed,
}

impl fmt::Display for LicenseClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LicenseClass::Allowed => write!(f, "allowed"),
            LicenseClass::Banned => write!(f, "banned"),
            LicenseClass::Unknown => write!(f, "unknown"),
            LicenseClass::Copyleft => write!(f, "copyleft"),
            LicenseClass::Unlicensed => write!(f, "unlicensed"),
        }
    }
}

/// Classify a raw SPDX expression against the canonical deny.toml allowlist.
///
/// Returns [`LicenseClass::Allowed`] for any license in the allowlist,
/// [`LicenseClass::Banned`] for explicitly banned licenses,
/// [`LicenseClass::Copyleft`] for other strong copyleft expressions, and
/// [`LicenseClass::Unknown`] for anything else.
///
/// This function is the single source of truth consumed by property and
/// adversarial tests.
pub fn classify_license(spdx: &str) -> LicenseClass {
    // Exact matches for the allowlist (mirrors deny.toml [licenses].allow).
    const ALLOWED: &[&str] = &[
        "MIT",
        "Apache-2.0",
        "Apache-2.0 WITH LLVM-exception",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "MPL-2.0",
        "Unicode-DFS-2016",
        "Unicode-3.0",
        "Zlib",
        "CC0-1.0",
        "0BSD",
    ];

    // Explicitly banned (mirrors deny.toml [licenses].deny + copyleft = "deny").
    const BANNED: &[&str] = &[
        "GPL-1.0",
        "GPL-2.0",
        "GPL-2.0+",
        "GPL-2.0-only",
        "GPL-2.0-or-later",
        "GPL-3.0",
        "GPL-3.0+",
        "GPL-3.0-only",
        "GPL-3.0-or-later",
        "AGPL-1.0",
        "AGPL-3.0",
        "AGPL-3.0-only",
        "AGPL-3.0-or-later",
        "SSPL-1.0",
        "Commons-Clause",
        "BUSL-1.1",
    ];

    // Copyleft patterns not in explicit list but matched by prefix.
    const COPYLEFT_PREFIXES: &[&str] = &["LGPL-", "EUPL-", "CDDL-", "EPL-", "CPL-"];

    if ALLOWED.contains(&spdx) {
        return LicenseClass::Allowed;
    }

    if BANNED.contains(&spdx) {
        return LicenseClass::Banned;
    }

    for prefix in COPYLEFT_PREFIXES {
        if spdx.starts_with(prefix) {
            return LicenseClass::Copyleft;
        }
    }

    if spdx.is_empty() || spdx == "UNLICENSED" || spdx == "NONE" {
        return LicenseClass::Unlicensed;
    }

    LicenseClass::Unknown
}

// ---------------------------------------------------------------------------
// Source classification
// ---------------------------------------------------------------------------

/// Kind of dependency source — maps to `deny.toml [sources]` rules.
///
/// `#[non_exhaustive]` so that adding a new source type (e.g., a private
/// registry via ADR waiver) does not break downstream exhaustive matches.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceKind {
    /// Official crates.io registry — always allowed.
    CratesIo,
    /// Unknown / non-allowlisted registry — denied per `unknown-registry = "deny"`.
    UnknownRegistry,
    /// Git dependency with an explicit SHA commit hash — allowed only if the
    /// URL is in `allow-git = []` (currently empty; any git dep fails CI).
    GitPinned,
    /// Git dependency without a pinned commit hash (branch, tag, or bare URL)
    /// — denied per `unknown-git = "deny"`.
    GitUnpinned,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::CratesIo => write!(f, "crates.io"),
            SourceKind::UnknownRegistry => write!(f, "unknown-registry"),
            SourceKind::GitPinned => write!(f, "git-pinned"),
            SourceKind::GitUnpinned => write!(f, "git-unpinned"),
        }
    }
}

/// Returns `true` if the source is allowed by the current deny.toml policy.
///
/// Only `CratesIo` is unconditionally allowed.  `GitPinned` would require
/// the URL to appear in `allow-git` (currently empty), so it also returns
/// `false` in the canonical config.
pub fn source_is_allowed(kind: &SourceKind) -> bool {
    matches!(kind, SourceKind::CratesIo)
}

// ---------------------------------------------------------------------------
// Policy outcome
// ---------------------------------------------------------------------------

/// Result of a simulated cargo-deny policy evaluation for a single dep.
///
/// Used by property tests and adversarial regression tests to assert that the
/// policy correctly classifies all inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenyPolicyOutcome {
    /// Name of the crate being evaluated.
    pub crate_name: String,
    /// Whether cargo-deny would allow this dep.
    pub allowed: bool,
    /// Reason for denial, or `None` if allowed.
    pub denial_reason: Option<DenialReason>,
}

/// Reason a dependency is denied by the cargo-deny policy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DenialReason {
    /// License not in allowlist (INV-SUPPLY-LICENSE-ALLOWLIST).
    License(LicenseClass),
    /// Dep is yanked from crates.io (INV-SUPPLY-NO-YANKED).
    Yanked,
    /// Source is not crates.io or not in allow-git list.
    UnknownSource(SourceKind),
    /// RUSTSEC advisory present and not waived.
    Advisory(String),
}

impl fmt::Display for DenialReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DenialReason::License(cls) => write!(f, "license:{cls}"),
            DenialReason::Yanked => write!(f, "yanked"),
            DenialReason::UnknownSource(src) => write!(f, "source:{src}"),
            DenialReason::Advisory(id) => write!(f, "advisory:{id}"),
        }
    }
}

/// Evaluate the deny.toml policy for a single synthetic dep.
///
/// This is the canonical policy evaluator used by all tests.  It mirrors the
/// logic in `deny.toml` exactly; divergence is a bug.
pub fn evaluate_policy(
    crate_name: &str,
    spdx: &str,
    source: &SourceKind,
    yanked: bool,
    advisory: Option<&str>,
) -> DenyPolicyOutcome {
    // Check yanked first (INV-SUPPLY-NO-YANKED — highest priority).
    if yanked {
        return DenyPolicyOutcome {
            crate_name: crate_name.to_owned(),
            allowed: false,
            denial_reason: Some(DenialReason::Yanked),
        };
    }

    // Check source.
    if !source_is_allowed(source) {
        return DenyPolicyOutcome {
            crate_name: crate_name.to_owned(),
            allowed: false,
            denial_reason: Some(DenialReason::UnknownSource(source.clone())),
        };
    }

    // Check license (INV-SUPPLY-LICENSE-ALLOWLIST).
    let license_class = classify_license(spdx);
    if license_class != LicenseClass::Allowed {
        return DenyPolicyOutcome {
            crate_name: crate_name.to_owned(),
            allowed: false,
            denial_reason: Some(DenialReason::License(license_class)),
        };
    }

    // Check RUSTSEC advisory.
    if let Some(adv) = advisory {
        return DenyPolicyOutcome {
            crate_name: crate_name.to_owned(),
            allowed: false,
            denial_reason: Some(DenialReason::Advisory(adv.to_owned())),
        };
    }

    DenyPolicyOutcome {
        crate_name: crate_name.to_owned(),
        allowed: true,
        denial_reason: None,
    }
}
