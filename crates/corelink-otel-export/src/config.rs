//! Per-vendor `#[non_exhaustive]` config structs.
//!
//! Every config struct in this module is `#[non_exhaustive]` so that
//! adding a new auth field (mTLS, OAuth2 client credentials, AWS SigV4
//! for Datadog AWS, GCP workload-identity for OTel-Collector) lands
//! additively without a breaking change. Public construction goes
//! through canonical `new()` / `builder()` constructors; callers MUST
//! NOT destructure the struct.

use serde::{Deserialize, Serialize};

use crate::error::{ExporterError, SecretValidationError};
use crate::secret::validate_api_key_shape;

/// Canonical conservative bounds for the Datadog API key (32-char hex
/// canonical, 40-char hex for APP key; widened band to absorb future
/// rotations).
pub const DATADOG_KEY_MIN: usize = 16;
/// Canonical conservative max for the Datadog API key.
pub const DATADOG_KEY_MAX: usize = 128;

/// Canonical conservative band for the Grafana Cloud HMAC token /
/// remote-write password.
pub const GRAFANA_KEY_MIN: usize = 16;
/// Canonical conservative max for the Grafana Cloud password.
pub const GRAFANA_KEY_MAX: usize = 256;

/// Canonical conservative band for the OTel-Collector bearer token
/// (opaque).
pub const OTEL_BEARER_MIN: usize = 8;
/// Canonical conservative max for the OTel-Collector bearer token.
pub const OTEL_BEARER_MAX: usize = 4096;

/// Canonical default per-request timeout (milliseconds). Per the
/// fail-OPEN contract, even a vendor-down case completes locally within
/// this budget so the request path is not impacted.
pub const DEFAULT_EXPORT_TIMEOUT_MS: u32 = 2_000;

/// Canonical Datadog ingest sites (`us1` / `us3` / `us5` / `eu1` /
/// `ap1` / `gov1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DatadogSite {
    /// `datadoghq.com` — North America (us1).
    Us1,
    /// `us3.datadoghq.com` — North America (us3).
    Us3,
    /// `us5.datadoghq.com` — North America (us5).
    Us5,
    /// `datadoghq.eu` — Europe (eu1).
    Eu1,
    /// `ap1.datadoghq.com` — Asia Pacific (ap1).
    Ap1,
    /// `ddog-gov.com` — US Government Cloud.
    Gov1,
}

impl DatadogSite {
    /// Canonical ingest host for this site.
    #[must_use]
    pub const fn ingest_host(self) -> &'static str {
        match self {
            Self::Us1 => "api.datadoghq.com",
            Self::Us3 => "api.us3.datadoghq.com",
            Self::Us5 => "api.us5.datadoghq.com",
            Self::Eu1 => "api.datadoghq.eu",
            Self::Ap1 => "api.ap1.datadoghq.com",
            Self::Gov1 => "api.ddog-gov.com",
        }
    }
}

/// Datadog HTTP API exporter config.
///
/// Authenticated via `DD-API-KEY` header. Optionally a second `DD-APPLICATION-KEY`
/// is required for the Tags API; metric + trace intake needs only the API key.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DatadogConfig {
    /// Datadog API key (residency-scoped; rotated quarterly).
    pub api_key: String,
    /// Datadog ingest site (governs the regional residency boundary).
    pub site: DatadogSite,
    /// Per-request timeout (milliseconds).
    pub timeout_ms: u32,
    /// Optional: opt-out of forwarding low-value high-cost metric kinds
    /// (defaults to forward-all-top-level + SLO-bound; see
    /// `specs/_audits/2026-05-15-otel-export-spec.md`).
    pub opt_out_low_value_metrics: bool,
}

impl DatadogConfig {
    /// Construct a new Datadog config with canonical defaults.
    ///
    /// # Errors
    ///
    /// - [`ExporterError::InvalidConfig`] wrapping a
    ///   [`SecretValidationError`] if `api_key` is empty / out of band /
    ///   non-ASCII.
    pub fn new(api_key: impl Into<String>, site: DatadogSite) -> Result<Self, ExporterError> {
        let api_key = api_key.into();
        validate_api_key_shape(&api_key, DATADOG_KEY_MIN, DATADOG_KEY_MAX)
            .map_err(secret_err_to_exporter)?;
        Ok(Self {
            api_key,
            site,
            timeout_ms: DEFAULT_EXPORT_TIMEOUT_MS,
            opt_out_low_value_metrics: false,
        })
    }
}

/// OTLP wire-protocol selection for the OTel-Collector exporter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OtlpProtocol {
    /// OTLP/gRPC (the canonical wire format; OpenTelemetry Spec §3).
    Grpc,
    /// OTLP/HTTP `/v1/metrics` + `/v1/traces` (protobuf body; for
    /// environments where gRPC is awkward — e.g., CF Workers).
    Http,
}

/// OpenTelemetry Collector exporter config.
///
/// The Collector is the customer's own router; CoreLink streams OTLP
/// gRPC/HTTP and the customer fan-outs from there. Auth is via bearer
/// token (canonical: `Authorization: Bearer <token>`) or by IP allow-list
/// on a private endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OtelCollectorConfig {
    /// OTLP collector endpoint (`https://otel.example.com:4318` for
    /// HTTP; `grpcs://otel.example.com:4317` for gRPC).
    pub endpoint: String,
    /// OTLP wire protocol.
    pub protocol: OtlpProtocol,
    /// Optional bearer token for `Authorization: Bearer <token>`.
    pub bearer_token: Option<String>,
    /// Per-request timeout (milliseconds).
    pub timeout_ms: u32,
    /// Optional: opt-out of forwarding low-value high-cost metric kinds.
    pub opt_out_low_value_metrics: bool,
}

impl OtelCollectorConfig {
    /// Construct a new OTel-Collector config.
    ///
    /// # Errors
    ///
    /// - [`ExporterError::InvalidConfig`] if `endpoint` is empty or does
    ///   not start with a canonical scheme.
    /// - [`ExporterError::InvalidConfig`] wrapping
    ///   [`SecretValidationError`] if `bearer_token` is present but
    ///   out-of-shape.
    pub fn new(
        endpoint: impl Into<String>,
        protocol: OtlpProtocol,
        bearer_token: Option<String>,
    ) -> Result<Self, ExporterError> {
        let endpoint = endpoint.into();
        if endpoint.is_empty() {
            return Err(ExporterError::InvalidConfig(
                "endpoint must not be empty".into(),
            ));
        }
        if !endpoint.starts_with("https://")
            && !endpoint.starts_with("http://")
            && !endpoint.starts_with("grpcs://")
            && !endpoint.starts_with("grpc://")
        {
            return Err(ExporterError::InvalidConfig(format!(
                "endpoint {endpoint:?} must start with https:// / http:// / grpcs:// / grpc://"
            )));
        }
        if let Some(ref t) = bearer_token {
            validate_api_key_shape(t, OTEL_BEARER_MIN, OTEL_BEARER_MAX)
                .map_err(secret_err_to_exporter)?;
        }
        Ok(Self {
            endpoint,
            protocol,
            bearer_token,
            timeout_ms: DEFAULT_EXPORT_TIMEOUT_MS,
            opt_out_low_value_metrics: false,
        })
    }
}

/// Grafana Cloud exporter config.
///
/// Metrics fan-out goes through Prometheus remote-write v1 (Grafana
/// Cloud Prom endpoint at `https://prometheus-prod-XX.grafana.net/api/prom/push`,
/// HMAC-Basic auth). Traces go through OTLP HTTP to the Tempo endpoint
/// (`https://tempo-prod-XX.grafana.net/tempo/api/push`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GrafanaCloudConfig {
    /// Prometheus remote-write endpoint URL.
    pub prom_remote_write_url: String,
    /// Tempo OTLP HTTP endpoint URL (traces).
    pub tempo_otlp_url: String,
    /// Grafana Cloud instance ID (Basic auth username).
    pub instance_id: String,
    /// Grafana Cloud API key / HMAC password (Basic auth password).
    pub api_key: String,
    /// Per-request timeout (milliseconds).
    pub timeout_ms: u32,
    /// Optional: opt-out of forwarding low-value high-cost metric kinds.
    pub opt_out_low_value_metrics: bool,
}

impl GrafanaCloudConfig {
    /// Construct a new Grafana Cloud config.
    ///
    /// # Errors
    ///
    /// - [`ExporterError::InvalidConfig`] if either endpoint is empty
    ///   or `instance_id` is empty.
    /// - [`ExporterError::InvalidConfig`] wrapping
    ///   [`SecretValidationError`] if `api_key` is out-of-shape.
    pub fn new(
        prom_remote_write_url: impl Into<String>,
        tempo_otlp_url: impl Into<String>,
        instance_id: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, ExporterError> {
        let prom = prom_remote_write_url.into();
        let tempo = tempo_otlp_url.into();
        let inst = instance_id.into();
        let key = api_key.into();
        if prom.is_empty() || tempo.is_empty() {
            return Err(ExporterError::InvalidConfig(
                "prom_remote_write_url and tempo_otlp_url must be non-empty".into(),
            ));
        }
        if inst.is_empty() {
            return Err(ExporterError::InvalidConfig(
                "instance_id must be non-empty".into(),
            ));
        }
        validate_api_key_shape(&key, GRAFANA_KEY_MIN, GRAFANA_KEY_MAX)
            .map_err(secret_err_to_exporter)?;
        Ok(Self {
            prom_remote_write_url: prom,
            tempo_otlp_url: tempo,
            instance_id: inst,
            api_key: key,
            timeout_ms: DEFAULT_EXPORT_TIMEOUT_MS,
            opt_out_low_value_metrics: false,
        })
    }
}

fn secret_err_to_exporter(e: SecretValidationError) -> ExporterError {
    ExporterError::InvalidConfig(format!("api-key shape: {e}"))
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
    fn datadog_site_canonical_hosts() {
        assert_eq!(DatadogSite::Us1.ingest_host(), "api.datadoghq.com");
        assert_eq!(DatadogSite::Eu1.ingest_host(), "api.datadoghq.eu");
        assert_eq!(DatadogSite::Gov1.ingest_host(), "api.ddog-gov.com");
    }

    #[test]
    fn datadog_config_accepts_canonical_key() {
        let c = DatadogConfig::new("a".repeat(32), DatadogSite::Us1).unwrap();
        assert_eq!(c.api_key.len(), 32);
        assert_eq!(c.site, DatadogSite::Us1);
        assert_eq!(c.timeout_ms, DEFAULT_EXPORT_TIMEOUT_MS);
        assert!(!c.opt_out_low_value_metrics);
    }

    #[test]
    fn datadog_config_rejects_short_key() {
        let err = DatadogConfig::new("abc", DatadogSite::Us1).unwrap_err();
        assert!(matches!(err, ExporterError::InvalidConfig(_)));
    }

    #[test]
    fn otel_collector_accepts_canonical_endpoint() {
        let c = OtelCollectorConfig::new(
            "https://otel.example.com:4318",
            OtlpProtocol::Http,
            Some("x".repeat(32)),
        )
        .unwrap();
        assert_eq!(c.protocol, OtlpProtocol::Http);
    }

    #[test]
    fn otel_collector_rejects_bare_endpoint() {
        let err =
            OtelCollectorConfig::new("otel.example.com:4318", OtlpProtocol::Grpc, None).unwrap_err();
        assert!(matches!(err, ExporterError::InvalidConfig(_)));
    }

    #[test]
    fn otel_collector_rejects_short_bearer() {
        let err = OtelCollectorConfig::new(
            "https://otel.example.com:4318",
            OtlpProtocol::Http,
            Some("a".into()),
        )
        .unwrap_err();
        assert!(matches!(err, ExporterError::InvalidConfig(_)));
    }

    #[test]
    fn grafana_cloud_config_canonical_constructs() {
        let c = GrafanaCloudConfig::new(
            "https://prometheus-prod-13.grafana.net/api/prom/push",
            "https://tempo-prod-04.grafana.net/tempo/api/push",
            "12345",
            "g".repeat(32),
        )
        .unwrap();
        assert_eq!(c.instance_id, "12345");
        assert_eq!(c.api_key.len(), 32);
    }

    #[test]
    fn grafana_cloud_rejects_empty_endpoint() {
        let err = GrafanaCloudConfig::new("", "tempo", "id", "k".repeat(32)).unwrap_err();
        assert!(matches!(err, ExporterError::InvalidConfig(_)));
    }

    #[test]
    fn grafana_cloud_rejects_empty_instance_id() {
        let err = GrafanaCloudConfig::new("https://p", "https://t", "", "k".repeat(32)).unwrap_err();
        assert!(matches!(err, ExporterError::InvalidConfig(_)));
    }
}
