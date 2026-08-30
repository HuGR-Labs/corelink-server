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

/// Validate an `x-artifact-tag` value before it is stored or echoed.
///
/// The tag is the Turborepo artifact signature: an HMAC the CLIENT computes
/// with `TURBO_REMOTE_CACHE_SIGNATURE_KEY`, a secret CoreLink never holds. We
/// therefore cannot check that a tag is *correct* — only that it is
/// well-formed enough to store and hand back verbatim. The accepted charset is
/// printable ASCII (`0x20..=0x7E`), which covers base64 and every plausible
/// future encoding while excluding the control bytes and non-ASCII that a
/// header value must not carry anyway.
///
/// Rejected:
///
/// - the empty string (a present-but-empty tag is a client bug, and storing it
///   would make "signed with nothing" indistinguishable from "unsigned"),
/// - any value longer than [`crate::MAX_ARTIFACT_TAG_LEN`] (the tag sidecar is
///   not byte-accounted against the tenant, so it must be bounded),
/// - any control / non-printable / non-ASCII byte.
///
/// **A tag that is ABSENT is not an error** — most clients never enable
/// signatures, and the untagged PUT/GET is the normal case. Only a tag that is
/// PRESENT and malformed fails, and it fails CLOSED (the whole PUT is refused)
/// rather than being silently dropped, which would leave a client believing an
/// entry was signed when it was not.
///
/// This function does not panic and performs no allocation.
///
/// # Errors
///
/// Returns [`TurboBridgeError::ArtifactTagInvalid`] describing the first
/// failed constraint.
pub fn validate_artifact_tag(tag: &str) -> Result<(), TurboBridgeError> {
    if tag.is_empty() {
        return Err(TurboBridgeError::ArtifactTagInvalid {
            len: 0,
            max: crate::MAX_ARTIFACT_TAG_LEN,
            reason: "artifact tag must not be empty when present",
        });
    }
    if tag.len() > crate::MAX_ARTIFACT_TAG_LEN {
        return Err(TurboBridgeError::ArtifactTagInvalid {
            len: tag.len(),
            max: crate::MAX_ARTIFACT_TAG_LEN,
            reason: "artifact tag too long",
        });
    }
    if !tag.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(TurboBridgeError::ArtifactTagInvalid {
            len: tag.len(),
            max: crate::MAX_ARTIFACT_TAG_LEN,
            reason: "artifact tag contains a non-printable-ASCII byte",
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

    /// A PRESENT `x-artifact-tag` failed validation
    /// ([`validate_artifact_tag`]): it was empty, longer than
    /// [`crate::MAX_ARTIFACT_TAG_LEN`], or carried a non-printable-ASCII byte.
    /// Rejected with HTTP 400 BEFORE any storage access or audit emit.
    ///
    /// An ABSENT tag never produces this error — the untagged path is the
    /// normal case for the majority of turbo clients.
    #[error("invalid x-artifact-tag: {reason} (len={len} max={max})")]
    ArtifactTagInvalid {
        /// Actual length of the supplied tag.
        len: usize,
        /// Maximum accepted length.
        max: usize,
        /// Human-readable reason the value was rejected.
        reason: &'static str,
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
    fn validate_artifact_tag_accepts_a_real_turbo_signature() {
        // Turborepo's tag is a base64 HMAC-SHA256 — 44 chars.
        validate_artifact_tag("dGhpcy1pcy1hLXR1cmJvLXNpZ25hdHVyZS10YWctdmFs")
            .expect("a real base64 tag is accepted");
        validate_artifact_tag("a").expect("a one-char tag is accepted");
    }

    #[test]
    fn validate_artifact_tag_accepts_exactly_max_len() {
        let max = "a".repeat(crate::MAX_ARTIFACT_TAG_LEN);
        validate_artifact_tag(&max).expect("exactly MAX_ARTIFACT_TAG_LEN accepted");
    }

    #[test]
    fn validate_artifact_tag_rejects_empty_and_too_long() {
        assert!(matches!(
            validate_artifact_tag("").expect_err("empty rejected"),
            TurboBridgeError::ArtifactTagInvalid { len: 0, .. }
        ));
        let too_long = "a".repeat(crate::MAX_ARTIFACT_TAG_LEN + 1);
        assert!(matches!(
            validate_artifact_tag(&too_long).expect_err("too long rejected"),
            TurboBridgeError::ArtifactTagInvalid { len, max, .. }
                if len == crate::MAX_ARTIFACT_TAG_LEN + 1 && max == crate::MAX_ARTIFACT_TAG_LEN
        ));
    }

    #[test]
    fn validate_artifact_tag_rejects_control_and_non_ascii() {
        // A tag is echoed back as an HTTP header value; control bytes and
        // non-ASCII must never reach that path (header injection / mangling).
        for bad in ["a\rb", "a\nb", "a\0b", "tag\u{e9}", "a\tb"] {
            assert!(
                matches!(
                    validate_artifact_tag(bad),
                    Err(TurboBridgeError::ArtifactTagInvalid { .. })
                ),
                "tag {bad:?} must be rejected"
            );
        }
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
