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
    /// Human-readable message; safe to surface to clients.
    pub message: String,
}

impl OciAdapterError {
    /// OCI Distribution Spec v1.1 error code string.
    #[must_use]
    pub const fn oci_code(&self) -> &'static str {
        match self {
            Self::Bind(_) => "UNKNOWN",
            Self::Auth(_) | Self::InvalidToken => "UNAUTHORIZED",
            Self::Cas(_) | Self::Kv(_) | Self::Audit(_) => "UNKNOWN",
            Self::DigestMismatch { .. } => "DIGEST_INVALID",
            Self::BlobOversized(_) => "SIZE_INVALID",
            Self::ManifestInvalid(_) => "MANIFEST_INVALID",
            Self::UploadSessionMissing(_) => "BLOB_UPLOAD_UNKNOWN",
            Self::NotFound => "NAME_UNKNOWN",
            Self::CatalogDisabled => "DENIED",
            Self::InvalidRepoName(_) => "NAME_INVALID",
        }
    }

    /// Build a single-entry envelope ready to JSON-serialize.
    #[must_use]
    pub fn to_envelope(&self) -> OciErrorEnvelope {
        OciErrorEnvelope {
            errors: vec![OciErrorEntry {
                code: self.oci_code(),
                message: self.to_string(),
            }],
        }
    }
}
