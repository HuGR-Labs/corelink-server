//! Audit emit helpers — every state-mutating OCI op fires through one
//! of these BEFORE the mutation lands (per
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER + the explicit
//! "Audit emit BEFORE state-mutation" Hard Rule in the wave-34
//! dispatch packet).
//!
//! ## Event-type taxonomy
//!
//! All event types are namespaced under `corelink.oci.<op>.<verb>.v1`:
//!
//! | Variant | Wire `event_type` |
//! |---|---|
//! | [`OciAuditEvent::BlobPush`]            | `corelink.oci.blob.push.v1` |
//! | [`OciAuditEvent::BlobPushDigestMismatch`] | `corelink.oci.blob.push.digest_mismatch.v1` |
//! | [`OciAuditEvent::BlobOversize`]        | `corelink.oci.blob.push.oversize.v1` |
//! | [`OciAuditEvent::ManifestPush`]        | `corelink.oci.manifest.push.v1` |
//! | [`OciAuditEvent::TagUpdate`]           | `corelink.oci.tag.update.v1` |
//! | [`OciAuditEvent::AuthDenied`]          | `corelink.oci.auth.denied.v1` |
//! | [`OciAuditEvent::CatalogDenied`]       | `corelink.oci.catalog.denied.v1` |
//!
//! ## Audit-fail-CLOSED contract
//!
//! Every helper here returns `Result<(), OciAdapterError>`; the call
//! site MUST `?` the result. The route layer at [`crate::server`]
//! maps the [`OciAdapterError::Audit`] arm to `503 Service Unavailable`
//! so the customer sees the failure and can retry.

use corelink_audit::ports::{AuditEmitter, AuditEvent};
use corelink_core::TenantId;
use serde_json::json;

use crate::error::OciAdapterError;

/// Strongly-typed OCI audit event variants, each maps to a fixed
/// `event_type` string + payload JSON shape.
#[derive(Debug)]
#[non_exhaustive]
pub enum OciAuditEvent<'a> {
    /// Blob successfully finalized.
    BlobPush {
        /// Repository name (`<namespace>/<name>` form).
        repo: &'a str,
        /// OCI digest of the persisted blob.
        digest: &'a str,
        /// Byte length.
        size_bytes: u64,
    },

    /// Declared `?digest=X` did not match the bytes' actual hash.
    /// FAIL-CLOSED variant — emit fires BEFORE the half-uploaded
    /// bytes are reaped from the upload session.
    BlobPushDigestMismatch {
        /// Repository name.
        repo: &'a str,
        /// What the client claimed.
        declared: &'a str,
        /// What the adapter actually computed.
        computed: &'a str,
        /// Upload UUID being canceled.
        upload_uuid: &'a str,
    },

    /// Blob exceeded the per-adapter size cap. The audit row carries
    /// the actual size so capacity planning has signal.
    BlobOversize {
        /// Repository name.
        repo: &'a str,
        /// Upload UUID being canceled.
        upload_uuid: &'a str,
        /// Bytes uploaded before the cap kicked in.
        size_bytes: u64,
        /// Configured cap.
        limit_bytes: u64,
    },

    /// Manifest successfully validated and persisted.
    ManifestPush {
        /// Repository name.
        repo: &'a str,
        /// `<reference>` slot: tag or digest.
        reference: &'a str,
        /// OCI digest of the manifest body itself.
        manifest_digest: &'a str,
        /// `mediaType` from the manifest JSON.
        media_type: &'a str,
    },

    /// A tag-list update fired (after [`Self::ManifestPush`] when the
    /// reference was a tag rather than a digest).
    TagUpdate {
        /// Repository name.
        repo: &'a str,
        /// Tag string.
        tag: &'a str,
        /// New manifest digest the tag now points at.
        manifest_digest: &'a str,
    },

    /// An authentication challenge or bearer-token verification
    /// failed. Emits regardless of mutation (we audit failed authn
    /// because forged-token volume is a security signal).
    AuthDenied {
        /// Wire path the request targeted (sanitized).
        path: &'a str,
        /// Reason category (e.g. `"missing-token"`, `"bad-hmac"`,
        /// `"expired"`, `"basic-auth-rejected"`).
        reason: &'a str,
    },

    /// `_catalog` endpoint hit while disabled.
    CatalogDenied {
        /// Anonymous flag — `true` if no bearer token was provided.
        anonymous: bool,
    },
}

impl<'a> OciAuditEvent<'a> {
    /// The canonical `event_type` string (CloudEvents 1.0 `type`).
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::BlobPush { .. } => "corelink.oci.blob.push.v1",
            Self::BlobPushDigestMismatch { .. } => "corelink.oci.blob.push.digest_mismatch.v1",
            Self::BlobOversize { .. } => "corelink.oci.blob.push.oversize.v1",
            Self::ManifestPush { .. } => "corelink.oci.manifest.push.v1",
            Self::TagUpdate { .. } => "corelink.oci.tag.update.v1",
            Self::AuthDenied { .. } => "corelink.oci.auth.denied.v1",
            Self::CatalogDenied { .. } => "corelink.oci.catalog.denied.v1",
        }
    }

    /// JSON payload body.
    #[must_use]
    pub fn payload(&self) -> serde_json::Value {
        match self {
            Self::BlobPush {
                repo,
                digest,
                size_bytes,
            } => json!({
                "repo": repo,
                "digest": digest,
                "size_bytes": size_bytes,
            }),
            Self::BlobPushDigestMismatch {
                repo,
                declared,
                computed,
                upload_uuid,
            } => json!({
                "repo": repo,
                "declared": declared,
                "computed": computed,
                "upload_uuid": upload_uuid,
            }),
            Self::BlobOversize {
                repo,
                upload_uuid,
                size_bytes,
                limit_bytes,
            } => json!({
                "repo": repo,
                "upload_uuid": upload_uuid,
                "size_bytes": size_bytes,
                "limit_bytes": limit_bytes,
            }),
            Self::ManifestPush {
                repo,
                reference,
                manifest_digest,
                media_type,
            } => json!({
                "repo": repo,
                "reference": reference,
                "manifest_digest": manifest_digest,
                "media_type": media_type,
            }),
            Self::TagUpdate {
                repo,
                tag,
                manifest_digest,
            } => json!({
                "repo": repo,
                "tag": tag,
                "manifest_digest": manifest_digest,
            }),
            Self::AuthDenied { path, reason } => json!({
                "path": path,
                "reason": reason,
            }),
            Self::CatalogDenied { anonymous } => json!({
                "anonymous": anonymous,
            }),
        }
    }
}

/// Fire one OCI audit row through the supplied emitter, BEFORE the
/// caller proceeds with state mutation. Convert any emit failure into
/// [`OciAdapterError::Audit`] so the route layer returns `503`.
///
/// `now_unix_ms` is taken as a parameter rather than read from a
/// global clock so deterministic tests can pin it.
///
/// # Errors
///
/// Returns [`OciAdapterError::Audit`] on any emit failure.
pub fn emit(
    emitter: &dyn AuditEmitter,
    tenant: &TenantId,
    event: &OciAuditEvent<'_>,
    now_unix_ms: u64,
) -> Result<(), OciAdapterError> {
    let row = AuditEvent::new(
        event.event_type(),
        tenant.to_canonical_text(),
        now_unix_ms,
        event.payload(),
    );
    emitter
        .emit(row)
        .map_err(|e| OciAdapterError::Audit(e.to_string()))
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
    use corelink_audit::ports::InMemoryAuditEmitter;

    fn t() -> TenantId {
        TenantId::from_uuid(uuid::Uuid::nil())
    }

    #[test]
    fn blob_push_event_type_is_canonical() {
        let e = OciAuditEvent::BlobPush {
            repo: "a/b",
            digest: "sha256:00",
            size_bytes: 42,
        };
        assert_eq!(e.event_type(), "corelink.oci.blob.push.v1");
    }

    #[test]
    fn digest_mismatch_carries_both_sides() {
        let e = OciAuditEvent::BlobPushDigestMismatch {
            repo: "a/b",
            declared: "sha256:aa",
            computed: "sha256:bb",
            upload_uuid: "u1",
        };
        let p = e.payload();
        assert_eq!(p["declared"], "sha256:aa");
        assert_eq!(p["computed"], "sha256:bb");
    }

    #[test]
    fn emit_writes_into_sink() {
        let em = InMemoryAuditEmitter::default();
        let ev = OciAuditEvent::ManifestPush {
            repo: "alpine",
            reference: "latest",
            manifest_digest: "sha256:dead",
            media_type: "application/vnd.oci.image.manifest.v1+json",
        };
        emit(&em, &t(), &ev, 1_700_000_000_000).expect("emit ok");
        let snap = em.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].event_type, "corelink.oci.manifest.push.v1");
    }
}
