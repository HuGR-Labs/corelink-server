//! Semver primitives + bump kind classification.

use serde::{Deserialize, Serialize};

/// Canonical `major.minor.patch` semver triple for DPA versions.
///
/// Equality + ordering are lexicographic on (major, minor, patch),
/// matching semver 2.0 precedence rules. We intentionally do NOT
/// support pre-release tags or build metadata — the DPA legal contract
/// is publicly versioned; pre-release tags would muddy the
/// notice-to-controller signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SemverVersion {
    /// Major component (material change axis).
    pub major: u32,
    /// Minor component (non-material additive change).
    pub minor: u32,
    /// Patch component (typo / clarification).
    pub patch: u32,
}

impl SemverVersion {
    /// Construct a [`SemverVersion`] from its three components.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Render as `v<major>.<minor>.<patch>` canonical form.
    #[must_use]
    pub fn render(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Bump kind taxonomy: `Major` = material change requires
/// re-acceptance; `Minor` / `Patch` = no re-acceptance (silent
/// clarification).
///
/// `#[non_exhaustive]` per CoreLink anti-pattern policy — future
/// taxonomy extensions (e.g. `SecurityHotfix`) must not break callers.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BumpKind {
    /// Material change — re-acceptance required + 30d grace + email
    /// broadcast.
    Major,
    /// Non-material additive change — silent (no re-acceptance).
    Minor,
    /// Typo / clarification — silent.
    Patch,
}

/// Derivation outcome for [`classify_bump`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpClassification {
    /// Forward semver bump correctly classified.
    Detected(BumpKind),
    /// `new == old` — no version movement.
    NoBump,
    /// `new < old` — illegal regression; CI gate must reject.
    Regression,
}

/// Classify a `(old, new)` semver pair into [`BumpKind`].
///
/// Rules:
/// - `new.major > old.major` ⇒ [`BumpKind::Major`]
/// - `new.major == old.major` ∧ `new.minor > old.minor` ⇒
///   [`BumpKind::Minor`]
/// - `new.major == old.major` ∧ `new.minor == old.minor` ∧
///   `new.patch > old.patch` ⇒ [`BumpKind::Patch`]
/// - `new == old` ⇒ [`BumpClassification::NoBump`]
/// - `new < old` ⇒ [`BumpClassification::Regression`]
#[must_use]
pub fn classify_bump(old: SemverVersion, new: SemverVersion) -> BumpClassification {
    match new.cmp(&old) {
        std::cmp::Ordering::Less => BumpClassification::Regression,
        std::cmp::Ordering::Equal => BumpClassification::NoBump,
        std::cmp::Ordering::Greater => {
            if new.major > old.major {
                BumpClassification::Detected(BumpKind::Major)
            } else if new.minor > old.minor {
                BumpClassification::Detected(BumpKind::Minor)
            } else {
                BumpClassification::Detected(BumpKind::Patch)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_bump_classified_major() {
        let old = SemverVersion::new(1, 4, 7);
        let new = SemverVersion::new(2, 0, 0);
        assert_eq!(
            classify_bump(old, new),
            BumpClassification::Detected(BumpKind::Major)
        );
    }

    #[test]
    fn minor_bump_classified_minor() {
        let old = SemverVersion::new(1, 4, 7);
        let new = SemverVersion::new(1, 5, 0);
        assert_eq!(
            classify_bump(old, new),
            BumpClassification::Detected(BumpKind::Minor)
        );
    }

    #[test]
    fn patch_bump_classified_patch() {
        let old = SemverVersion::new(1, 4, 7);
        let new = SemverVersion::new(1, 4, 8);
        assert_eq!(
            classify_bump(old, new),
            BumpClassification::Detected(BumpKind::Patch)
        );
    }

    #[test]
    fn equal_versions_yield_no_bump() {
        let v = SemverVersion::new(2, 0, 0);
        assert_eq!(classify_bump(v, v), BumpClassification::NoBump);
    }

    #[test]
    fn regression_detected() {
        let old = SemverVersion::new(2, 0, 0);
        let new = SemverVersion::new(1, 9, 9);
        assert_eq!(classify_bump(old, new), BumpClassification::Regression);
    }

    #[test]
    fn render_canonical() {
        assert_eq!(SemverVersion::new(2, 3, 4).render(), "v2.3.4");
    }
}
