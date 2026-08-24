//! Error taxonomy for `corelink-turbo-bridge`.

use thiserror::Error;

/// Maximum byte length of a Turbo `teamId` accepted by the bridge.
///
/// `teamId` is interpolated verbatim into the storage key
/// (`"<teamId>/<hash>"`). An unbounded `teamId` is a denial-of-service vector
/// (heap amplification of the key) and a tenant-confusion vector (an empty or
/// `/`-bearing value lets one team alias another team's key prefix within the
/// same authenticated tenant). Values longer than this are rejected with
/// [`TurboBridgeError::TeamIdInvalid`] BEFORE any storage access or audit emit,
/// mirroring the [`crate::MAX_HASH_LEN`] guard.
pub const MAX_TEAM_ID_LEN: usize = 256;

/// Validate a Turborepo `teamId` before it is used to build a storage key.
///
/// Returns `Ok(())` when `team_id` is a safe sub-namespace label and
/// [`TurboBridgeError::TeamIdInvalid`] otherwise. The accepted charset is
/// deliberately conservative — Turborepo team identifiers are slug-like
/// (`team_xxxxxxxx`), so we admit `[A-Za-z0-9_-]` only and reject:
///
/// - the empty string (would alias the bare-`/<hash>` key prefix),
/// - any value longer than [`MAX_TEAM_ID_LEN`] (key heap amplification),
/// - `/` (would let `teamId=../other` escape the team sub-namespace and alias
///   another team's slot within the same tenant),
/// - any control / non-printable / non-ASCII byte (`\0`, etc.).
///
/// This function does not panic and performs no allocation.
///
/// # Errors
///
/// Returns [`TurboBridgeError::TeamIdInvalid`] describing the first failed
/// constraint.
pub fn validate_team_id(team_id: &str) -> Result<(), TurboBridgeError> {
    if team_id.is_empty() {
        return Err(TurboBridgeError::TeamIdInvalid {
            len: 0,
            max: MAX_TEAM_ID_LEN,
            reason: "team_id must not be empty",
        });
    }
    if team_id.len() > MAX_TEAM_ID_LEN {
        return Err(TurboBridgeError::TeamIdInvalid {
            len: team_id.len(),
            max: MAX_TEAM_ID_LEN,
            reason: "team_id too long",
        });
    }
    if !team_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(TurboBridgeError::TeamIdInvalid {
            len: team_id.len(),
            max: MAX_TEAM_ID_LEN,
            reason: "team_id contains a disallowed character (allowed: [A-Za-z0-9_-])",
        });
    }
    Ok(())
}

/// Errors returned by [`crate::handler::TurboArtifactHandler`] implementations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TurboBridgeError {
    /// The requested artifact was not found in the store.
    #[error("artifact not found: hash={hash:?}")]
    NotFound {
        /// The hash that was requested but not present.
        hash: String,
    },

    /// The `team_id` in the request does not match the authenticated
    /// `caller_tenant`.  Emitting a cross-tenant audit event and returning
    /// this error is mandatory BEFORE any storage access.
    #[error("cross-tenant denied: caller={caller:?} requested team_id={requested_team_id:?}")]
    CrossTenantDenied {
        /// The authenticated caller's tenant.
        caller: String,
        /// The `team_id` the caller supplied in the request.
        requested_team_id: String,
    },

    /// The hash string exceeds [`crate::MAX_HASH_LEN`] characters.
    #[error("artifact hash too long: len={len} max={max}")]
    HashTooLong {
        /// Actual length of the supplied hash string.
        len: usize,
        /// Maximum accepted length.
        max: usize,
    },

    /// The `team_id` failed validation ([`validate_team_id`]): it was empty,
    /// longer than [`MAX_TEAM_ID_LEN`], or contained a disallowed character
    /// (`/`, control, or non-`[A-Za-z0-9_-]`). Rejected with HTTP 400 BEFORE
    /// any storage access or audit emit (DoS / key-aliasing guard).
    #[error("invalid team_id: {reason} (len={len} max={max})")]
    TeamIdInvalid {
        /// Actual length of the supplied `team_id`.
        len: usize,
        /// Maximum accepted length.
        max: usize,
        /// Human-readable reason the value was rejected.
        reason: &'static str,
    },

    /// A PUT targeted a key that already holds an artifact, and the store is
    /// create-only (`put_if_absent`): the existing bytes are NOT overwritten.
    /// Maps to HTTP 409 Conflict. Turborepo keys are opaque/client-chosen, not
    /// content-addressed, so an overwrite could silently REPLACE the bytes
    /// behind a tenant's own existing key (within-tenant cache poisoning the
    /// content envelope cannot detect). Create-only closes that (BACKLOG
    /// B-024); the real `turbo` client never re-PUTs an existing key in normal
    /// operation and tolerates this 409 as a non-fatal warning.
    #[error("artifact already exists (create-only): hash={hash:?}")]
    AlreadyExists {
        /// The hash whose key already holds an artifact.
        hash: String,
    },

    /// An audit emit failed; the operation was aborted without mutating state
    /// (fail-CLOSED ordering).
    #[error("audit sink failed: {0}")]
    AuditFailed(String),

    /// Internal storage error (lock poisoned, I/O failure, etc.).
    #[error("internal error: {0}")]
    Internal(String),
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
    fn validate_team_id_accepts_typical_slug() {
        validate_team_id("team_x").expect("typical slug accepted");
        validate_team_id("team_AbC-123_def").expect("mixed slug accepted");
        validate_team_id("t1").expect("short slug accepted");
    }

    #[test]
    fn validate_team_id_accepts_exactly_max_len() {
        let max = "a".repeat(MAX_TEAM_ID_LEN);
        validate_team_id(&max).expect("exactly MAX_TEAM_ID_LEN accepted");
    }

    #[test]
    fn validate_team_id_rejects_empty() {
        let err = validate_team_id("").expect_err("empty rejected");
        assert!(matches!(
            err,
            TurboBridgeError::TeamIdInvalid { len: 0, .. }
        ));
    }

    #[test]
    fn validate_team_id_rejects_too_long() {
        let too_long = "a".repeat(MAX_TEAM_ID_LEN + 1);
        let err = validate_team_id(&too_long).expect_err("too long rejected");
        assert!(matches!(
            err,
            TurboBridgeError::TeamIdInvalid { len, max, .. }
                if len == MAX_TEAM_ID_LEN + 1 && max == MAX_TEAM_ID_LEN
        ));
    }

    #[test]
    fn validate_team_id_rejects_slash_and_traversal() {
        // A bare slash and a `../other` traversal both escape the team
        // sub-namespace key prefix; both MUST be rejected.
        assert!(matches!(
            validate_team_id("a/b").expect_err("slash rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
        assert!(matches!(
            validate_team_id("../other").expect_err("traversal rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
    }

    #[test]
    fn validate_team_id_rejects_control_and_non_ascii() {
        assert!(matches!(
            validate_team_id("a\0b").expect_err("nul rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
        assert!(matches!(
            validate_team_id("a\nb").expect_err("newline rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
        assert!(matches!(
            validate_team_id("team_é").expect_err("non-ascii rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
        assert!(matches!(
            validate_team_id("a b").expect_err("space rejected"),
            TurboBridgeError::TeamIdInvalid { .. }
        ));
    }
}
