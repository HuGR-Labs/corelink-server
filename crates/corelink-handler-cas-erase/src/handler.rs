//! Pure decision logic for the per-hash CAS erase + the 410-Gone tombstone.
//!
//! This module owns **no I/O**. It validates the inputs the container hands it,
//! produces a [`TombstoneMarker`] the wiring layer persists to D1, and decides
//! — for the read path — whether a given hash has been erased and must answer
//! HTTP 410 Gone (vs falling through to a normal CAS lookup which may 200 or
//! 404). The R2 delete + D1 transports are composed in
//! `corelink-container::routes::cas_erase` over the DSR Wave 1 R2 CAS
//! primitives (`R2S3Client::{delete, list_objects_v2}`, DSR increment 3).

use crate::error::CasEraseError;

/// Maximum accepted CAS digest length (hex/base64 SHA-256/BLAKE3 fit well
/// under this). A bound keys-off pathological inputs before they reach storage.
const MAX_DIGEST_LEN: usize = 128;

/// A request to erase a single content-addressed blob.
///
/// The container constructs this AFTER the constant-time internal-auth gate has
/// passed and the authenticated tenant has been resolved; the pure handler then
/// re-checks the tenant echo + digest shape (defence in depth — the route does
/// not trust the caller-supplied path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasEraseRequest {
    /// Authenticated tenant (the sole isolation key).
    pub auth_tenant: String,
    /// Tenant echoed in the URL path; MUST equal `auth_tenant`.
    pub path_tenant: String,
    /// The content digest to erase.
    pub digest: String,
}

impl CasEraseRequest {
    /// Construct an erase request.
    #[must_use]
    pub fn new(
        auth_tenant: impl Into<String>,
        path_tenant: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            auth_tenant: auth_tenant.into(),
            path_tenant: path_tenant.into(),
            digest: digest.into(),
        }
    }
}

/// The durable 410-Gone marker the wiring layer writes to the `cas_tombstone`
/// D1 table (migration `0067`). Its presence under `(tenant, digest)` is the
/// sole signal that turns a subsequent GET into a 410 Gone.
///
/// The marker is intentionally minimal — it carries no blob bytes (those are
/// already deleted from R2) and no PII. `reason` is an operator-supplied,
/// free-form audit string (e.g. `"dsr-erasure"`, `"abuse-takedown"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TombstoneMarker {
    /// Tenant the erased hash belonged to.
    pub tenant: String,
    /// The erased content digest.
    pub digest: String,
    /// Operator-supplied audit reason (free-form, bounded by the route).
    pub reason: String,
}

impl TombstoneMarker {
    /// Build the tombstone marker for a validated erase request.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        digest: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            digest: digest.into(),
            reason: reason.into(),
        }
    }
}

/// Outcome of a per-hash erase.
///
/// Both variants are HTTP 200 OK at the route layer — the distinction is purely
/// observational (so an operator can see whether the call actually removed
/// bytes or merely re-asserted an existing tombstone). Re-erasing an
/// already-tombstoned hash is a deliberate, safe no-op (`AlreadyErased`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseOutcome {
    /// The blob's R2 objects were deleted and a fresh tombstone was written.
    Erased,
    /// The hash was already tombstoned; the call is an idempotent no-op.
    AlreadyErased,
}

/// What the READ path must do for a given hash.
///
/// `corelink-container::routes::cas` consults this BEFORE the normal CAS
/// lookup: an erased hash short-circuits to 410 Gone (NEVER 404, NEVER 200) so
/// an erased artifact cannot silently resurrect or masquerade as "never
/// existed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadGate {
    /// The hash is tombstoned → the route MUST return HTTP 410 Gone.
    Gone,
    /// No tombstone → proceed with the normal CAS lookup (200 / 404).
    Proceed,
}

/// Validate a CAS digest: non-empty, within [`MAX_DIGEST_LEN`], and composed
/// only of hash-safe characters (hex / base64url alphabet). This guarantees the
/// digest can never carry a `/` that would widen an R2 LIST/DELETE prefix and
/// is always safe to interpolate into a log line.
///
/// # Errors
///
/// Returns [`CasEraseError::InvalidDigest`] when the digest is empty, too long,
/// or contains a disallowed character.
pub fn validate_digest(digest: &str) -> Result<(), CasEraseError> {
    let ok = !digest.is_empty()
        && digest.len() <= MAX_DIGEST_LEN
        && digest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(CasEraseError::InvalidDigest {
            digest: digest.chars().take(MAX_DIGEST_LEN).collect(),
        })
    }
}

/// Validate an erase request fully and produce the [`TombstoneMarker`] to
/// persist.
///
/// Order matches the route's fail-CLOSED ladder: cross-tenant FIRST (deny
/// before quoting any storage shape), then digest shape. The `reason` is passed
/// through unchanged (the route bounds its length).
///
/// # Errors
///
/// - [`CasEraseError::CrossTenantDenied`] when `path_tenant != auth_tenant`.
/// - [`CasEraseError::InvalidDigest`] when the digest is malformed.
pub fn prepare_erase(
    req: &CasEraseRequest,
    reason: &str,
) -> Result<TombstoneMarker, CasEraseError> {
    if req.path_tenant != req.auth_tenant {
        return Err(CasEraseError::CrossTenantDenied {
            caller: req.auth_tenant.clone(),
            requested_tenant: req.path_tenant.clone(),
        });
    }
    validate_digest(&req.digest)?;
    Ok(TombstoneMarker::new(
        req.auth_tenant.clone(),
        req.digest.clone(),
        reason,
    ))
}

/// Decide the read-path gate from whether a tombstone row exists.
///
/// The wiring layer performs the `(tenant, digest)` D1 lookup and passes the
/// boolean here, keeping the route logic-free and the decision unit-testable.
#[must_use]
pub fn read_gate(tombstone_present: bool) -> ReadGate {
    if tombstone_present {
        ReadGate::Gone
    } else {
        ReadGate::Proceed
    }
}

/// Map the R2 delete result + the pre-erase tombstone presence into the
/// [`EraseOutcome`].
///
/// `was_already_tombstoned` is sampled BEFORE the write so a re-erase reports
/// `AlreadyErased` even though the (idempotent) R2 delete + tombstone upsert
/// still run.
#[must_use]
pub fn erase_outcome(was_already_tombstoned: bool) -> EraseOutcome {
    if was_already_tombstoned {
        EraseOutcome::AlreadyErased
    } else {
        EraseOutcome::Erased
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
    fn validate_digest_accepts_hex_and_base64url() {
        assert!(validate_digest("deadbeef0123456789abcdef").is_ok());
        assert!(validate_digest("AbC_-123").is_ok());
    }

    #[test]
    fn validate_digest_rejects_empty_slash_and_overlong() {
        assert!(matches!(
            validate_digest(""),
            Err(CasEraseError::InvalidDigest { .. })
        ));
        assert!(matches!(
            validate_digest("a/b"),
            Err(CasEraseError::InvalidDigest { .. })
        ));
        let overlong = "a".repeat(MAX_DIGEST_LEN + 1);
        assert!(matches!(
            validate_digest(&overlong),
            Err(CasEraseError::InvalidDigest { .. })
        ));
    }

    #[test]
    fn prepare_erase_denies_cross_tenant_before_digest_check() {
        // Even with a malformed digest, the cross-tenant denial wins (order).
        let req = CasEraseRequest::new("t1", "t2", "bad/digest");
        let err = prepare_erase(&req, "dsr").unwrap_err();
        assert!(matches!(err, CasEraseError::CrossTenantDenied { .. }));
    }

    #[test]
    fn prepare_erase_builds_marker_for_valid_request() {
        let req = CasEraseRequest::new("t1", "t1", "deadbeef");
        let marker = prepare_erase(&req, "abuse-takedown").unwrap();
        assert_eq!(marker.tenant, "t1");
        assert_eq!(marker.digest, "deadbeef");
        assert_eq!(marker.reason, "abuse-takedown");
    }

    #[test]
    fn read_gate_gone_iff_tombstoned() {
        assert_eq!(read_gate(true), ReadGate::Gone);
        assert_eq!(read_gate(false), ReadGate::Proceed);
    }

    #[test]
    fn erase_outcome_reports_idempotent_re_erase() {
        assert_eq!(erase_outcome(false), EraseOutcome::Erased);
        assert_eq!(erase_outcome(true), EraseOutcome::AlreadyErased);
    }
}
