//! Notice version major-bump detection (CTRL-PRIV-CONSENT-005; AC-006).
//!
//! # Rule
//!
//! A major bump in `notice_version` (e.g. `1.x.y → 2.0.0`) forces
//! re-consent. The subject must explicitly re-confirm; old consent is
//! flagged `stale_consent = true` (NOT deleted — audit trail preserved).
//!
//! # Grace period (DD-002)
//!
//! 30d notice grace + 60d cap = 90d total. After 90d, purpose enters
//! `consent_lapsed` state — processing STOPS. NO fallback to
//! `legitimate_interest` (corrige GPT P0-1 round-1: legal_basis is
//! **fixed** per purpose, no dynamic swap).
//!
//! # Invariant
//!
//! `is_notice_version_stale(current, submitted)` returns `true` when
//! the submitted proof's major version is strictly less than the current
//! canonical major version.

/// Error returned when a submitted proof carries a stale major version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeVersionCheckError {
    /// Current canonical major version.
    pub current_major: u64,
    /// Major version in the submitted proof.
    pub submitted_major: u64,
}

impl std::fmt::Display for NoticeVersionCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "notice version stale: current major={}, submitted major={}; \
             re-consent required (CTRL-PRIV-CONSENT-005)",
            self.current_major, self.submitted_major
        )
    }
}

/// Parse the major component from a semver string `"MAJOR.MINOR.PATCH"`.
fn parse_major(semver: &str) -> Option<u64> {
    semver.split('.').next()?.parse().ok()
}

/// Returns `true` when the `submitted_version` major is strictly less than
/// the `current_version` major (force re-consent required).
///
/// Returns `false` if either version cannot be parsed (fail-open on parse
/// errors to avoid blocking a valid consent submit; the store layer will
/// enforce stricter validation).
#[must_use]
pub fn is_notice_version_stale(current_version: &str, submitted_version: &str) -> bool {
    match (parse_major(current_version), parse_major(submitted_version)) {
        (Some(current_major), Some(submitted_major)) => submitted_major < current_major,
        _ => false,
    }
}

/// Strict variant: returns an error if the submitted version is stale.
pub fn check_notice_version(
    current_version: &str,
    submitted_version: &str,
) -> Result<(), NoticeVersionCheckError> {
    match (parse_major(current_version), parse_major(submitted_version)) {
        (Some(current_major), Some(submitted_major)) if submitted_major < current_major => {
            Err(NoticeVersionCheckError {
                current_major,
                submitted_major,
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn same_major_not_stale() {
        assert!(!is_notice_version_stale("1.5.0", "1.2.0"));
        assert!(!is_notice_version_stale("2.0.0", "2.9.9"));
    }

    #[test]
    fn older_major_is_stale() {
        assert!(is_notice_version_stale("2.0.0", "1.5.0"));
        assert!(is_notice_version_stale("3.0.0", "2.9.9"));
    }

    #[test]
    fn newer_submitted_major_not_stale() {
        // Submitted is ahead — unusual but not stale
        assert!(!is_notice_version_stale("1.0.0", "2.0.0"));
    }

    #[test]
    fn check_returns_error_on_stale() {
        let err = check_notice_version("2.0.0", "1.5.0").unwrap_err();
        assert_eq!(err.current_major, 2);
        assert_eq!(err.submitted_major, 1);
    }

    #[test]
    fn check_returns_ok_on_fresh() {
        assert!(check_notice_version("1.5.0", "1.2.0").is_ok());
    }
}
