//! `ExporterError` + `SecretValidationError` canonical taxonomies.

use thiserror::Error;

/// Canonical error taxonomy for the customer-facing exporter primitive.
///
/// All variants `#[non_exhaustive]` so adding new failure modes
/// (Vendor-specific quota, mTLS handshake, OAuth2 token refresh) lands
/// additively.
///
/// **Semantic contract.** Per `INV-OBS-EXPORT-FAIL-OPEN`, the orchestrator
/// MUST NOT propagate these into the request path; it MUST instead record
/// the failure in the audit envelope
/// (`corelink.observability.export_failed`) and return `Ok(())` to the
/// caller. This error type exists so the internal transport layer can be
/// unit-tested with adversarial fixtures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExporterError {
    /// HTTP / gRPC transport failure (network unreachable, TLS handshake
    /// failure, connection reset, 5xx from vendor backend).
    #[error("transport failure: {0}")]
    Transport(String),

    /// Auth reject (401 / 403 from vendor; invalid API key, expired
    /// token, IP not in allow-list).
    #[error("auth reject from vendor: {0}")]
    AuthReject(String),

    /// Vendor-side rate limit (429); the customer's observability stack
    /// is signaling "slow down" — fail-OPEN treats this as a soft
    /// signal, surfaces it in the audit envelope, and the orchestrator
    /// returns Ok().
    #[error("vendor rate-limit (HTTP 429): {0}")]
    RateLimited(String),

    /// Config validation failure (e.g., empty API key, malformed
    /// endpoint URL, unsupported region).
    #[error("invalid exporter configuration: {0}")]
    InvalidConfig(String),

    /// Vendor returned a 4xx other than 401/403/429 (e.g., 400
    /// malformed payload from a schema drift).
    #[error("vendor rejected payload: {0}")]
    PayloadRejected(String),

    /// Internal serialization / mutex / canonical-name lookup failure.
    /// Never the customer's fault; surfaces in the audit envelope and
    /// pages SRE.
    #[error("internal exporter failure: {0}")]
    Internal(String),
}

/// Errors raised when validating an API-key / password / shared-secret
/// at config-load time.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SecretValidationError {
    /// Secret is empty.
    #[error("secret is empty")]
    Empty,
    /// Secret length is outside the [min, max] canonical band for this
    /// vendor.
    #[error("secret length {actual} outside canonical band [{min}, {max}]")]
    LengthOutOfRange {
        /// Observed length.
        actual: usize,
        /// Canonical minimum (inclusive).
        min: usize,
        /// Canonical maximum (inclusive).
        max: usize,
    },
    /// Secret contains non-ASCII bytes (Datadog/Grafana keys are
    /// canonical hex / base64 / opaque ASCII).
    #[error("secret contains non-ASCII bytes")]
    NonAscii,
}
