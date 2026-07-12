//! Errors returned by the OCI registry adapter.
//!
//! Every variant maps deterministically to an HTTP status in
//! [`crate::oci::server::status_for`]. Wire-level OCI error bodies follow the
//! Distribution Spec v1.1 §error format:
//!
//! ```json
//! { "errors": [ { "code": "<UPPER_SNAKE>", "message": "...", "detail": {} } ] }
//! ```

use serde::Serialize;

/// Canonical adapter error.
///
/// Variants are intentionally narrower than the OCI Distribution Spec
/// error code list so the production wiring layer can choose how to
/// surface "unknown" upstream errors (currently mapped to
/// [`Self::Cas`] / [`Self::Kv`]).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OciAdapterError {
    /// Failed to bind the listen socket (`run_oci_adapter` startup).
    #[error("bind: {0}")]
    Bind(std::io::Error),

    /// Authentication / authorization failure. The wire shape returns
    /// `401 Unauthorized` with the `Www-Authenticate: Bearer …`
    /// challenge so a compliant OCI client kicks off the realm-token
    /// exchange.
    #[error("auth: {0}")]
    Auth(String),

    /// Underlying CAS layer (`BlobStore` port) returned an error.
    /// Surfaces as `500` to the client; audit row already emitted.
    #[error("cas: {0}")]
    Cas(String),

    /// KV (`ManifestKvStore` port) returned an error during manifest
    /// or tag-list mutation / read. Surfaces as `500` after audit.
    #[error("kv: {0}")]
    Kv(String),

    /// The bytes streamed for an upload session hash to `computed`,
    /// but the client declared `digest=<declared>` on the finalize
    /// `PUT`. Fail-CLOSED per oci.md §9: reject + audit
    /// `oci.push.digest_mismatch` BEFORE any state mutation.
    #[error("digest mismatch: declared {declared}, computed {computed}")]
    DigestMismatch {
        /// The digest the client claimed at finalize time
        /// (`?digest=sha256:<hex>`).
        declared: String,
        /// The digest the adapter computed over the actually-uploaded
        /// bytes. MUST NOT match `declared` for this variant.
        computed: String,
    },

    /// The uploaded blob exceeds [`crate::oci::config::OciAdapterConfig::blob_size_limit_bytes`].
    /// Wire shape: `413 Payload Too Large`.
    #[error("blob exceeds limit: {0} bytes")]
    BlobOversized(u64),

    /// Manifest body failed JSON schema validation (mediaType check,
    /// required fields, etc.). Wire shape: `400 Bad Request`.
    #[error("manifest schema invalid: {0}")]
    ManifestInvalid(String),

    /// `PATCH` or `PUT` referenced an upload UUID that the adapter
    /// has no record of (either it was never opened or it was reaped
    /// after the abandoned-upload TTL). Wire shape: `404 Not Found`.
    #[error("upload session not found: {0}")]
    UploadSessionMissing(String),

    /// Audit emit returned an error. Surfaces as `503` to the client
    /// per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER; the in-flight mutation
    /// MUST be rolled back at the call site.
    #[error("audit: {0}")]
    Audit(String),

    /// The client referenced a blob or manifest digest that does not
    /// exist for the request's tenant. Wire shape: `404 Not Found`.
    /// Important: NEVER return `403` here — that would leak existence
    /// across tenant boundaries (oci.md §8 row 4).
    #[error("not found")]
    NotFound,

    /// OCI catalog endpoint is disabled and the client requested it.
    /// Wire shape: `401`. The catalog is OFF by default per oci.md §6.
    #[error("catalog endpoint disabled")]
    CatalogDisabled,

    /// The bearer token (Authorization header) is malformed, expired,
    /// or its HMAC fails the constant-time check.
    /// Wire shape: `401` with `Www-Authenticate: Bearer …`.
    #[error("invalid bearer token")]
    InvalidToken,

    /// The repository name failed the OCI Distribution Spec v1.1
    /// grammar check (`[a-z0-9]+(?:[._-][a-z0-9]+)*` per component,
    /// `/`-separated). Wire shape: `400`.
    #[error("invalid repository name: {0}")]
    InvalidRepoName(String),

    /// The per-tenant open upload-session cap has been reached.
    ///
    /// Each open session buffers blob chunks in process memory until the
    /// matching `PUT` finalises or the session is cancelled; without a cap
    /// an authenticated tenant can exhaust heap by opening N sessions and
    /// feeding large `PATCH` bodies.
    ///
    /// Wire shape: `429 Too Many Requests` + `Retry-After: <secs>`.
    /// The client SHOULD cancel or finalise an existing session before
    /// retrying the `POST /v2/<repo>/blobs/uploads/`.
    ///
    /// `retry_after_secs` is advisory (we do not track when the oldest
    /// session will expire). 60 s is a conservative default that covers
    /// most realistic push timeouts.
    #[error("too many open upload sessions (limit {limit}); retry after {retry_after_secs}s")]
    TooManyOpenSessions {
        /// The configured per-tenant cap that was reached.
        limit: usize,
        /// Advisory retry delay in seconds.
        retry_after_secs: u32,
    },

    /// The manifest `PUT` body exceeds the hard server-side limit
    /// ([`crate::oci::server::handlers::MAX_MANIFEST_BYTES`]).
    ///
    /// OCI manifests are JSON documents; realistic fat indexes are well
    /// under 1 MiB. A body larger than the cap is rejected with `413
    /// Payload Too Large` *before* it is buffered into heap, so no
    /// large allocation ever occurs (audit #5 / WP-OCI-DOS).
    #[error("manifest body exceeds server limit")]
    ManifestOversized,
}

/// Wire-shape error envelope per OCI Distribution Spec v1.1.
///
/// Serialized as:
/// ```json
/// { "errors": [ { "code": "...", "message": "...", "detail": {} } ] }
/// ```
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct OciErrorEnvelope {
    /// Always exactly one inner row (we never batch errors).
    pub errors: Vec<OciErrorEntry>,
}

/// One entry inside [`OciErrorEnvelope::errors`].
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct OciErrorEntry {
    /// OCI Distribution Spec v1.1 error code (UPPER_SNAKE).
    pub code: &'static str,
    /// Human-readable message. SCRUBBED of internal backend detail for
    /// backend-fault variants (Cluster E) — see
    /// [`OciAdapterError::client_message`].
    pub message: String,
}

impl OciAdapterError {
    /// True for variants whose inner string carries INTERNAL backend detail
    /// (D1 / Cloudflare API errors, possibly SQL; R2 storage topology; the
    /// derived per-tenant R2 prefix) that MUST NOT reach the client (A24 /
    /// A28 / A29). Their wire `message` is replaced with an opaque,
    /// reference-only string; the real detail is logged server-side.
    ///
    /// `Auth(_)` is the highest-priority case: the OCI `/token` leg is
    /// UNAUTHENTICATED-reachable and its PAT-verify backend fault otherwise
    /// leaked the raw CF D1 API error (status + body, possibly SQL) into the
    /// public 401 response body.
    #[must_use]
    pub const fn leaks_internal_detail(&self) -> bool {
        matches!(
            self,
            Self::Bind(_) | Self::Auth(_) | Self::Cas(_) | Self::Kv(_) | Self::Audit(_)
        )
    }

    /// The CLIENT-FACING message for this error (Cluster E).
    ///
    /// For backend-fault variants ([`Self::leaks_internal_detail`]) this is a
    /// fixed, opaque string keyed by the failure class plus a `ref` request
    /// id the operator can correlate to the server-side `tracing::error!`
    /// line that DOES carry the real detail. For every other variant (digest
    /// mismatch, oversized, not-found, invalid repo name, throttled — none of
    /// which carry internal topology) the original, already-safe message is
    /// kept so clients retain the actionable signal they depend on.
    #[must_use]
    pub fn client_message(&self, request_id: &str) -> String {
        if self.leaks_internal_detail() {
            let class = match self {
                Self::Auth(_) => "authentication failed",
                // Bind only appears at boot, never on a served request, but
                // scrub it defensively too.
                _ => "internal error",
            };
            format!("{class} (ref: {request_id})")
        } else {
            self.to_string()
        }
    }

    /// OCI Distribution Spec v1.1 error code string.
    #[must_use]
    pub const fn oci_code(&self) -> &'static str {
        match self {
            Self::Bind(_) => "UNKNOWN",
            Self::Auth(_) | Self::InvalidToken => "UNAUTHORIZED",
            Self::Cas(_) | Self::Kv(_) | Self::Audit(_) => "UNKNOWN",
            Self::DigestMismatch { .. } => "DIGEST_INVALID",
            Self::BlobOversized(_) => "SIZE_INVALID",
            Self::ManifestInvalid(_) | Self::ManifestOversized => "MANIFEST_INVALID",
            Self::UploadSessionMissing(_) => "BLOB_UPLOAD_UNKNOWN",
            Self::NotFound => "NAME_UNKNOWN",
            Self::CatalogDisabled => "DENIED",
            Self::InvalidRepoName(_) => "NAME_INVALID",
            // BLOB_UPLOAD_THROTTLED is not a Distribution Spec v1.1 standard
            // code; "DENIED" is the closest standard value. The HTTP 429
            // status and the `Retry-After` header are the client's primary
            // signal — the OCI code is informational only.
            Self::TooManyOpenSessions { .. } => "DENIED",
        }
    }

    /// Build a single-entry envelope ready to JSON-serialize, with the
    /// `message` SCRUBBED of internal backend detail (Cluster E).
    ///
    /// `request_id` is the correlation handle echoed in the scrubbed message
    /// (`ref: …`) and logged alongside the real detail server-side, so an
    /// operator can join a client-visible opaque error to its full cause
    /// without that cause ever crossing the wire.
    #[must_use]
    pub fn to_envelope(&self, request_id: &str) -> OciErrorEnvelope {
        OciErrorEnvelope {
            errors: vec![OciErrorEntry {
                code: self.oci_code(),
                message: self.client_message(request_id),
            }],
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    const RID: &str = "abc123ref";

    #[test]
    fn backend_fault_message_is_opaque_and_ref_tagged() {
        // Cluster E: every internal-detail-bearing variant scrubs its message
        // and carries the correlation ref. The raw inner string never appears.
        let raw = "D1 HTTP 500: no such table: pat in SELECT ... FROM pat";
        for err in [
            OciAdapterError::Auth(format!("backend: {raw}")),
            OciAdapterError::Cas(raw.to_owned()),
            OciAdapterError::Kv(raw.to_owned()),
            OciAdapterError::Audit(raw.to_owned()),
        ] {
            assert!(err.leaks_internal_detail(), "{err:?} must be flagged leaky");
            let msg = err.client_message(RID);
            assert!(!msg.contains("D1"), "leaked D1: {msg}");
            assert!(!msg.contains("SELECT"), "leaked SQL: {msg}");
            assert!(!msg.contains("FROM pat"), "leaked SQL: {msg}");
            assert!(!msg.contains("backend"), "leaked backend tag: {msg}");
            assert!(msg.contains(RID), "missing correlation ref: {msg}");
        }
        // Auth uses the auth-specific class; the rest use the generic class.
        assert!(OciAdapterError::Auth("x".into())
            .client_message(RID)
            .starts_with("authentication failed"));
    }

    #[test]
    fn safe_variants_keep_their_actionable_message() {
        // Variants that carry NO internal topology keep their useful message.
        let dm = OciAdapterError::DigestMismatch {
            declared: "sha256:aaa".into(),
            computed: "sha256:bbb".into(),
        };
        assert!(!dm.leaks_internal_detail());
        let msg = dm.client_message(RID);
        assert!(msg.contains("sha256:aaa") && msg.contains("sha256:bbb"));
        // NotFound stays the bare, non-leaking string.
        assert_eq!(OciAdapterError::NotFound.client_message(RID), "not found");
    }

    #[test]
    fn envelope_shape_preserved_with_scrubbed_message() {
        let env = OciAdapterError::Auth("backend: D1 HTTP 500: secret".into()).to_envelope(RID);
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"errors\""), "envelope shape must survive");
        assert!(json.contains("UNAUTHORIZED"), "code preserved");
        assert!(
            !json.contains("D1"),
            "scrubbed body must not leak D1: {json}"
        );
        assert!(json.contains(RID), "ref present: {json}");
    }
}
