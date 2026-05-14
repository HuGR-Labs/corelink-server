//! [`SbomError`] taxonomy for all SBOM pipeline failure modes.
//!
//! Exit-code mapping (per CLI contract WI-S12-002 §23):
//! - [`SbomError::GenerationFailed`] | [`SbomError::SchemaInvalid`] → exit 1
//! - [`SbomError::NtiaValidationFailed`] → exit 2
//! - [`SbomError::TsaRequestFailed`] → exit 3
//! - [`SbomError::DtIngestionFailed`] → exit 4 (fallback queue triggered)

use crate::ntia::NtiaValidation;

/// All errors that can occur in the SBOM pipeline.
///
/// # Error taxonomy
///
/// Each variant maps to a dedicated exit code in the CLI binary so that
/// calling scripts can distinguish generation / validation / TSA / DT
/// failures unambiguously.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SbomError {
    /// `cargo cyclonedx` subprocess returned non-zero or produced empty output.
    #[error("cargo-cyclonedx execution failed: {0}")]
    GenerationFailed(String),

    /// CycloneDX JSON failed schema validation (malformed output).
    #[error("CycloneDX schema validation failed: {0}")]
    SchemaInvalid(String),

    /// NTIA minimum elements check failed in strict mode.
    #[error("NTIA validation failed: {0:?}")]
    NtiaValidationFailed(NtiaValidation),

    /// RFC 3161 TSA timestamp request to `tsa.sigstore.dev` failed.
    #[error("RFC 3161 TSA timestamp request failed: {0}")]
    TsaRequestFailed(String),

    /// Dependency-Track ingestion API returned a non-2xx status after retries.
    #[error("Dependency-Track ingestion failed: HTTP {status}: {body}")]
    DtIngestionFailed {
        /// HTTP status code returned by Dependency-Track.
        status: u16,
        /// Response body (first 2 KiB) for diagnostics.
        body: String,
    },

    /// All DT ingestion retries were exhausted; SBOM queued in fallback store.
    #[error("Dependency-Track ingestion exhausted all retries; queued to fallback: {0}")]
    DtRetryExhausted(String),

    /// API key environment variable was not set.
    #[error("API key env var '{0}' not set")]
    ApiKeyMissing(String),

    /// I/O error reading or writing SBOM / TSR files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialisation error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP client error.
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
}
