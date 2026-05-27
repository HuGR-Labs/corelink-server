//! `corelink-otel-export` — Customer-facing observability export primitive
//! (R-prep — enterprise observability forwarding lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous-execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic skeleton**
//! of the per-vendor metric + trace forwarder behind a single
//! [`MetricsExporter`] trait surface. It models:
//!
//! 1. The trait surface every enterprise observability adapter
//!    (Datadog, OTel Collector, Grafana Cloud) will satisfy: a single
//!    [`MetricsExporter`] with [`export_batch`](MetricsExporter::export_batch)
//!    and [`export_trace`](MetricsExporter::export_trace).
//! 2. A canonical [`ExporterVariant`] `#[non_exhaustive]` 4-enum
//!    (`Datadog` / `OtelCollector` / `GrafanaCloud` / `Disabled`).
//! 3. Per-vendor `#[non_exhaustive]` config structs:
//!    [`DatadogConfig`](config::DatadogConfig),
//!    [`OtelCollectorConfig`](config::OtelCollectorConfig),
//!    [`GrafanaCloudConfig`](config::GrafanaCloudConfig).
//! 4. A canonical [`MetricPoint`] envelope (linked to
//!    `corelink-analytics::RedMetricKind`) + canonical [`TraceSpan`]
//!    envelope (alias for [`corelink_tracing::SpanRecord`]).
//! 5. An [`InMemoryFake`](exporter::InMemoryFake) for tests + one
//!    deferred-real stub per vendor:
//!    [`DatadogExporter`](exporter::DatadogExporter),
//!    [`OtelCollectorExporter`](exporter::OtelCollectorExporter),
//!    [`GrafanaCloudExporter`](exporter::GrafanaCloudExporter). Each stub
//!    returns `Ok(())` with `tracing::debug!` so behavior is testable
//!    end-to-end. Real HTTP / gRPC / Prom remote-write wiring is
//!    deferred to a future WI per the charter pattern.
//! 6. A canonical [`audit::ExportFailedEvent`] envelope (audit event
//!    `corelink.observability.export_failed`) emitted on EVERY transport
//!    failure path (per WI charter audit-emit-BEFORE-mutation +
//!    `INV-OBS-EXPORT-FAIL-OPEN`).
//! 7. Constant-time HMAC equality for API-key validation
//!    ([`secret::constant_time_secret_eq`]) — Datadog API key + Grafana
//!    Cloud password are compared via `subtle::ConstantTimeEq` to remove
//!    a class of timing oracles when the customer mis-pastes a key.
//!
//! # Invariants enforced
//!
//! - **INV-OBS-EXPORT-FAIL-OPEN** (R-prep enterprise observability
//!   contract): every exporter fails-OPEN — when the customer's
//!   observability stack is down (HTTP 5xx, network unreachable, auth
//!   reject), CoreLink MUST NOT propagate the failure into the request
//!   path. The export call records an audit event
//!   `corelink.observability.export_failed` but returns `Ok(())` to the
//!   caller. The customer's dashboard going dark is THEIR incident, not
//!   ours. Pinned by `prop_fail_open_under_any_transport_error`.
//!
//! - **INV-OBS-NO-PII** (sprint contract §10.s09.6; cross-link
//!   `observability_model.md §10`): every metric / trace forwarded to a
//!   third-party MUST be PII-free. We forward only the
//!   `RedMetricKind` canonical taxonomy (15 enum variants) + canonical
//!   label sets (tenant tier pseudonymous; region; result; layer;
//!   reason; bucket; op_type; etc.). The free-form `tenant_id` is
//!   pseudonymized at the boundary BEFORE export. The audit log lists
//!   every forwarded metric kind for evidence. Pinned by
//!   `prop_exported_label_set_pii_clean` + audit doc
//!   `specs/_audits/2026-05-15-otel-export-spec.md`.
//!
//! - **INV-OBS-CONFIG-NON-EXHAUSTIVE**: every per-vendor config struct
//!   is `#[non_exhaustive]` so adding a new auth field (mTLS, OAuth2
//!   client credentials, AWS SigV4 for Datadog AWS) lands additively
//!   without a breaking change.
//!
//! - **INV-OBS-CT-SECRET-EQ**: API-key + password equality goes through
//!   [`secret::constant_time_secret_eq`] (constant-time over the
//!   underlying bytes, with a length-mismatch short-circuit that does
//!   not leak via timing because the secret length is itself part of
//!   the secret in our threat model — we always pad-compare against the
//!   stored canonical length first). Pinned by
//!   `prop_constant_time_secret_eq_total_function`.
//!
//! # Why trait + fake here, real impl in future WI
//!
//! Real HTTP/gRPC wiring touches Cloudflare Workers `worker::send_future`
//! fire-and-forget (NEVER `tokio::spawn` per Lote 10.7bis R5 P0-3) plus
//! one HTTP client per vendor (`reqwest` for Datadog, OTLP HTTP `/v1/...`
//! for OTel Collector, Prometheus remote-write v1 for Grafana Cloud).
//! Each carries its own retry budget, its own auth shape, and its own
//! wire encoding (Datadog v2 series JSON; OTLP protobuf; Prom
//! remote-write protobuf + Snappy). Per the charter, the algorithmic
//! invariants — fail-OPEN semantics, audit envelope discipline, label
//! PII hygiene, constant-time secret compare — land first behind a
//! trait surface, then the real wire client lands behind a follow-up
//! WI alongside its own Stripe/PD-style chaos test (induced 5xx,
//! induced network partition, induced auth reject, induced
//! over-quota-throttle).
//!
//! Cross-links: `specs/03_architecture/observability_model.md`,
//! `specs/_audits/2026-05-15-otel-export-spec.md`,
//! `apps/docs/docs/how-to/observability/forward-to-{datadog,
//! otel-collector,grafana-cloud}.mdx`,
//! `apps/docs/docs/trust/data-handling.mdx`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod config;
pub mod error;
pub mod exporter;
pub mod metric;
pub mod secret;

pub use audit::{
    ExportFailedAuditSink, ExportFailedEvent, InMemoryExportFailedAuditSink,
    EXPORT_FAILED_EVENT_TYPE,
};
pub use config::{
    DatadogConfig, DatadogSite, GrafanaCloudConfig, OtelCollectorConfig,
    OtlpProtocol,
};
pub use error::{ExporterError, SecretValidationError};
pub use exporter::{
    DatadogExporter, ExporterVariant, GrafanaCloudExporter, InMemoryFake,
    MetricsExporter, OtelCollectorExporter,
};
pub use metric::{MetricPoint, MetricValue, TraceSpan};
pub use secret::{constant_time_secret_eq, validate_api_key_shape};

/// Canonical schema version for the customer-export emitter (FROZEN at 1
/// for the pure-logic skeleton; real-wire ship gate bumps this).
#[must_use]
pub const fn otel_export_schema_version() -> u32 {
    1
}

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::otel"
}
