//! OTel-export request-path layer — closes the observability export SEAM.
//!
//! # What this closes
//!
//! `corelink-telemetry` ships three library-complete, tested OTel exporters
//! ([`DatadogExporter`], [`OtelCollectorExporter`], [`GrafanaCloudExporter`])
//! plus the fail-OPEN orchestration boundary. Until this layer, the container
//! never CONSTRUCTED any of them — its telemetry was `tracing_subscriber::fmt()`
//! stdout only (`main.rs`), so a customer who configured a collector saw
//! nothing. This middleware is the missing wire: it builds the configured
//! exporter from the `[observability.export.*]` env surface and streams a
//! canonical [`MetricPoint`] + [`TraceSpan`] per data-plane request through the
//! crate's fail-OPEN boundary.
//!
//! # Where it sits
//!
//! Wired as ONE `.layer(...)` line at the END of
//! [`crate::routes::build_with_factory`] — the OUTERMOST data-plane layer, so it
//! observes the FINAL response status of every data-plane route (including a
//! rate-limit `429` or a residency `409`). Like the rate-limit layer, it wraps
//! exactly the composed data-plane router: the `/_health` readiness probe and
//! the `/_internal/*` surfaces are merged in `main.rs` AFTER `build_with_factory`
//! returns and stay OUTSIDE this layer by construction (we do not want to export
//! a metric/span for every DO health probe).
//!
//! # Config surface (env-driven)
//!
//! The container reads the `[observability.export.*]` surface as environment
//! variables (the DO forwards them from `wrangler.toml`'s `[env.*.vars]`):
//!
//! | Env var | Meaning |
//! |---|---|
//! | `CORELINK_OBSERVABILITY_EXPORT_VARIANT` | `otel_collector` \| `datadog` \| `grafana_cloud` \| `disabled` (unset ⇒ disabled) |
//! | `CORELINK_OTEL_COLLECTOR_ENDPOINT` | OTLP endpoint (`https://collector:4318`) |
//! | `CORELINK_OTEL_COLLECTOR_PROTOCOL` | `http` (default) \| `grpc` |
//! | `CORELINK_OTEL_COLLECTOR_BEARER_TOKEN` | optional `Authorization: Bearer` token |
//! | `CORELINK_DATADOG_API_KEY` / `CORELINK_DATADOG_SITE` | Datadog key + ingest site (`us1`…`gov1`) |
//! | `CORELINK_GRAFANA_PROM_REMOTE_WRITE_URL` / `..._TEMPO_OTLP_URL` / `..._INSTANCE_ID` / `..._API_KEY` | Grafana Cloud |
//!
//! When the variant is unset / `disabled`, or the selected variant's config is
//! absent / malformed, [`OtelExportState::from_env`] returns `None` and the
//! layer is NOT mounted (dev/CI: zero overhead, behaviour unchanged). A present
//! variant with malformed config logs a `warn!` and stays unmounted (fail-SAFE —
//! a bad observability config must never brick the data plane).
//!
//! # OPERATOR RESIDUAL — the network egress is a follow-up
//!
//! The three concrete exporters in `corelink-telemetry` are, per that crate's
//! `trait-abstraction-defer` charter, deferred-real STUBS: they validate config,
//! accept the batch, and `tracing::debug!` under `target: "corelink_otel_export"`
//! (real OTLP/HTTP + Datadog intake + Prom remote-write land in a follow-up WI
//! behind their own chaos tests). This layer wires the FULL request-path emit
//! (construct → fail-OPEN export) so the seam is closed and observable in logs
//! today; the operator's collector endpoint + the exporter's HTTP egress are the
//! remaining step. Until egress lands, the emit is visible via the debug target
//! above and via this layer's own structured `otel_export` tracing events.
//!
//! # Fail-OPEN + PII discipline
//!
//! Export runs through [`MetricsExporter::export_batch_fail_open`] /
//! [`export_trace_fail_open`](MetricsExporter::export_trace_fail_open): a vendor
//! failure becomes a `corelink.observability.export_failed` audit event and the
//! request path is unaffected. Per `INV-OBS-NO-PII`, only pseudonymous labels
//! (`region`, `result`, `op_type`) are attached — never the raw tenant id.
//!
//! # Bounded audit sink (F-022 lesson)
//!
//! The only concrete [`ExportFailedAuditSink`] in `corelink-telemetry` is the
//! unbounded in-memory TEST-capture sink. Wiring that into the prod request path
//! would leak heap on every failed export once real egress lands (the exact
//! rate-limit F-022 trap). This layer instead wires a bounded
//! [`TracingExportFailedAuditSink`] — O(1) memory, one `warn!` per failure. The
//! full `corelink-audit-chain`-backed writer is the deferred production sink.

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{extract::State, middleware::Next, response::Response};
use corelink_analytics::RedMetricKind;
use corelink_telemetry::otel::audit::{
    AuditEmitError, ExportFailedAuditSink, ExportFailedEvent,
};
use corelink_telemetry::otel::config::{
    DatadogConfig, DatadogSite, GrafanaCloudConfig, OtelCollectorConfig, OtlpProtocol,
};
use corelink_telemetry::otel::exporter::{
    DatadogExporter, GrafanaCloudExporter, MetricsExporter, OtelCollectorExporter,
};
use corelink_telemetry::otel::metric::{MetricPoint, MetricValue, TraceSpan};
use corelink_telemetry::tracing::{SpanKind, SpanStatus};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Selector env var: which vendor the container exports to.
const VARIANT_ENV: &str = "CORELINK_OBSERVABILITY_EXPORT_VARIANT";
/// Region label source (already a prod signal; see `main.rs` prod-arming block).
const REGION_ENV: &str = "R2_CAS_REGION";

/// Bounded production [`ExportFailedAuditSink`] — O(1) memory.
///
/// Emits ONE structured `tracing::warn!` per export failure under
/// `target: "corelink_otel_export"` and returns `Ok(())`. This is the correct
/// prod posture until the `corelink-audit-chain`-backed writer is wired: the
/// crate's only other sink (`InMemoryExportFailedAuditSink`) is an unbounded
/// test-capture buffer that would leak heap on the request path (F-022).
#[derive(Clone, Copy, Debug, Default)]
pub struct TracingExportFailedAuditSink;

impl ExportFailedAuditSink for TracingExportFailedAuditSink {
    fn record(&self, event: &ExportFailedEvent) -> Result<(), AuditEmitError> {
        tracing::warn!(
            target: "corelink_otel_export",
            event_type = event.event_type,
            variant = event.variant.as_str(),
            path = ?event.path,
            batch_size = event.batch_size,
            reason = %event.reason,
            "observability export failed (fail-OPEN; request path unaffected)"
        );
        Ok(())
    }
}

/// Shared state for the OTel-export layer: the configured exporter, the bounded
/// audit sink, and the pseudonymous `region` label. Cheap to clone (all `Arc` /
/// `Copy`).
#[derive(Clone, Debug)]
pub struct OtelExportState {
    exporter: Arc<dyn MetricsExporter>,
    audit_sink: Arc<dyn ExportFailedAuditSink>,
    region: Arc<str>,
}

impl OtelExportState {
    /// Construct from the process environment, or `None` when export is disabled
    /// / unconfigured / malformed (see module docs). Never panics; a malformed
    /// but PRESENT variant logs a `warn!` and returns `None` (fail-SAFE).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Env-injection seam for tests. `lookup` returns the value for a var name.
    #[must_use]
    pub fn from_lookup<F>(lookup: F) -> Option<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let variant = lookup(VARIANT_ENV)?;
        let variant = variant.trim().to_ascii_lowercase();
        let region: Arc<str> = lookup(REGION_ENV)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
            .into();

        let exporter: Arc<dyn MetricsExporter> = match variant.as_str() {
            "otel_collector" => Arc::new(build_otel_collector(&lookup)?),
            "datadog" => Arc::new(build_datadog(&lookup)?),
            "grafana_cloud" => Arc::new(build_grafana_cloud(&lookup)?),
            "disabled" | "" => return None,
            other => {
                tracing::warn!(
                    target: "corelink_otel_export",
                    variant = other,
                    "unknown {VARIANT_ENV} value; observability export NOT mounted"
                );
                return None;
            }
        };

        tracing::info!(
            target: "corelink_otel_export",
            variant = exporter.variant().as_str(),
            region = %region,
            "observability export layer armed (real OTLP/HTTP egress is a documented \
             deferred-real follow-up; emit visible in logs)"
        );

        Some(Self {
            exporter,
            audit_sink: Arc::new(TracingExportFailedAuditSink),
            region,
        })
    }
}

/// Build the OTel-Collector exporter from env (the primary, recommended path).
fn build_otel_collector<F>(lookup: &F) -> Option<OtelCollectorExporter>
where
    F: Fn(&str) -> Option<String>,
{
    let endpoint = non_empty(lookup("CORELINK_OTEL_COLLECTOR_ENDPOINT"))?;
    let protocol = match lookup("CORELINK_OTEL_COLLECTOR_PROTOCOL")
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("grpc") => OtlpProtocol::Grpc,
        // Default HTTP — CF-Workers-friendly (see the how-to).
        _ => OtlpProtocol::Http,
    };
    let bearer = non_empty(lookup("CORELINK_OTEL_COLLECTOR_BEARER_TOKEN"));
    match OtelCollectorConfig::new(endpoint, protocol, bearer) {
        Ok(cfg) => Some(OtelCollectorExporter::new(cfg)),
        Err(e) => {
            warn_bad_config("otel_collector", &e.to_string());
            None
        }
    }
}

/// Build the Datadog exporter from env.
fn build_datadog<F>(lookup: &F) -> Option<DatadogExporter>
where
    F: Fn(&str) -> Option<String>,
{
    let api_key = non_empty(lookup("CORELINK_DATADOG_API_KEY"))?;
    let site = match lookup("CORELINK_DATADOG_SITE")
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("us3") => DatadogSite::Us3,
        Some("us5") => DatadogSite::Us5,
        Some("eu1") => DatadogSite::Eu1,
        Some("ap1") => DatadogSite::Ap1,
        Some("gov1") => DatadogSite::Gov1,
        _ => DatadogSite::Us1,
    };
    match DatadogConfig::new(api_key, site) {
        Ok(cfg) => Some(DatadogExporter::new(cfg)),
        Err(e) => {
            warn_bad_config("datadog", &e.to_string());
            None
        }
    }
}

/// Build the Grafana Cloud exporter from env.
fn build_grafana_cloud<F>(lookup: &F) -> Option<GrafanaCloudExporter>
where
    F: Fn(&str) -> Option<String>,
{
    let prom = non_empty(lookup("CORELINK_GRAFANA_PROM_REMOTE_WRITE_URL"))?;
    let tempo = non_empty(lookup("CORELINK_GRAFANA_TEMPO_OTLP_URL"))?;
    let instance = non_empty(lookup("CORELINK_GRAFANA_INSTANCE_ID"))?;
    let api_key = non_empty(lookup("CORELINK_GRAFANA_API_KEY"))?;
    match GrafanaCloudConfig::new(prom, tempo, instance, api_key) {
        Ok(cfg) => Some(GrafanaCloudExporter::new(cfg)),
        Err(e) => {
            warn_bad_config("grafana_cloud", &e.to_string());
            None
        }
    }
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn warn_bad_config(variant: &str, err: &str) {
    tracing::warn!(
        target: "corelink_otel_export",
        variant,
        error = err,
        "observability export config malformed; export NOT mounted (fail-SAFE — \
         the data plane serves without export)"
    );
}

/// Map a data-plane `(method, path)` to its canonical RED metric kind, if any.
///
/// Respects the closed 15-variant [`RedMetricKind`] taxonomy: only routes with a
/// canonical counterpart emit a metric point; every other route still emits a
/// trace span. Returns `(kind, is_duration)` so the CAS-PUT path can ALSO emit
/// its duration histogram.
fn metric_kind_for(method: &axum::http::Method, path: &str) -> Option<RedMetricKind> {
    use axum::http::Method;
    if path.starts_with("/v1/cas") && matches!(*method, Method::POST | Method::PUT) {
        Some(RedMetricKind::CasPutRequestsTotal)
    } else if path.starts_with("/v1/ac") && *method == Method::GET {
        Some(RedMetricKind::AcLookupRequestsTotal)
    } else {
        None
    }
}

/// Coarse, PII-free op_type label derived from the path prefix.
fn op_type_for(path: &str) -> &'static str {
    if path.starts_with("/v1/cas") {
        "cas"
    } else if path.starts_with("/v1/ac") {
        "ac"
    } else if path.starts_with("/v2/") || path == "/token" {
        "oci"
    } else if path.starts_with("/v8/") {
        "turbo"
    } else if path.starts_with("/bazel") {
        "bazel"
    } else {
        "other"
    }
}

/// Build the pseudonymous label set for a request outcome. PII-free per
/// `INV-OBS-NO-PII` — no tenant id, no path params.
fn labels(region: &str, op_type: &str, is_error: bool) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("region".to_string(), region.to_string()),
        ("op_type".to_string(), op_type.to_string()),
        (
            "result".to_string(),
            if is_error { "error" } else { "ok" }.to_string(),
        ),
    ])
}

/// One completed request's observation inputs — the vendor-agnostic envelope
/// [`emit`] turns into a [`MetricPoint`] + [`TraceSpan`]. Grouping these keeps
/// `emit` a two-argument function (state + observation) and lets tests drive it
/// deterministically without touching the clock / env.
#[derive(Clone, Debug)]
pub(crate) struct RequestObservation<'a> {
    /// HTTP method of the completed request.
    pub method: &'a axum::http::Method,
    /// Request path (used only for op_type / metric-kind mapping — never a label).
    pub path: &'a str,
    /// Final response status.
    pub status: axum::http::StatusCode,
    /// Unix-epoch nanoseconds at request start (metric timestamp + span start).
    pub start_ns: u128,
    /// Wall-clock request duration in seconds (CAS-PUT histogram + span end).
    pub duration_secs: f64,
    /// W3C 16-byte trace id.
    pub trace_id: [u8; 16],
    /// W3C 8-byte span id.
    pub span_id: [u8; 8],
    /// Unix-epoch milliseconds for the fail-OPEN audit envelope.
    pub now_unix_ms: u64,
}

/// Emit the metric point(s) + trace span for one completed request through the
/// fail-OPEN boundary. Pure over its inputs (no env / clock reads) so tests can
/// drive it deterministically.
pub(crate) fn emit(state: &OtelExportState, obs: &RequestObservation<'_>) {
    let is_error = obs.status.is_server_error();
    let op = op_type_for(obs.path);
    let region = state.region.as_ref();

    // --- metric(s) ---
    if let Some(kind) = metric_kind_for(obs.method, obs.path) {
        let point = MetricPoint::new(
            kind,
            labels(region, op, is_error),
            MetricValue::Counter(1),
            obs.start_ns,
        );
        // Fail-OPEN: a vendor failure becomes an audit event, never an Err here.
        let _ = state.exporter.export_batch_fail_open(
            &[point],
            state.audit_sink.as_ref(),
            obs.now_unix_ms,
        );

        // CAS PUT additionally carries a duration histogram.
        if kind == RedMetricKind::CasPutRequestsTotal {
            let dur = MetricPoint::new(
                RedMetricKind::CasPutDurationSeconds,
                labels(region, op, is_error),
                MetricValue::Histogram(obs.duration_secs),
                obs.start_ns,
            );
            let _ = state.exporter.export_batch_fail_open(
                &[dur],
                state.audit_sink.as_ref(),
                obs.now_unix_ms,
            );
        }
    }

    // --- trace span (every data-plane request) ---
    let start_u64 = u64::try_from(obs.start_ns).unwrap_or(u64::MAX);
    let end_u64 = start_u64.saturating_add((obs.duration_secs * 1e9) as u64);
    let mut span = TraceSpan::new(
        obs.trace_id,
        obs.span_id,
        None,
        format!("{} {}", obs.method.as_str(), op),
        SpanKind::Server,
        start_u64,
    );
    span.add_attribute("region", region);
    span.add_attribute("op_type", op);
    span.add_attribute("http.status_code", obs.status.as_str());
    span.end(
        if is_error {
            SpanStatus::Error
        } else {
            SpanStatus::Ok
        },
        end_u64,
    );
    let _ = state
        .exporter
        .export_trace_fail_open(&span, state.audit_sink.as_ref(), obs.now_unix_ms);
}

/// Axum middleware: export a canonical [`MetricPoint`] + [`TraceSpan`] for every
/// data-plane request through the configured exporter's fail-OPEN boundary.
///
/// Wired as `.layer(axum::middleware::from_fn_with_state(state, otel_export_layer))`
/// (OUTERMOST data-plane layer). The export is done AFTER the handler completes,
/// so it observes the final status and total latency; the fail-OPEN boundary
/// guarantees a vendor fault never touches the response.
pub async fn otel_export_layer(
    State(state): State<OtelExportState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();
    let start_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let response = next.run(req).await;

    let status = response.status();
    let duration_secs = start.elapsed().as_secs_f64();
    let now_unix_ms = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(u64::MAX);

    // W3C-random ids: 16-byte trace id + 8-byte span id from UUIDv4 entropy.
    let trace_id = *Uuid::new_v4().as_bytes();
    let mut span_id = [0u8; 8];
    span_id.copy_from_slice(&Uuid::new_v4().as_bytes()[..8]);

    emit(
        &state,
        &RequestObservation {
            method: &method,
            path: &path,
            status,
            start_ns,
            duration_secs,
            trace_id,
            span_id,
            now_unix_ms,
        },
    );

    response
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
    use std::collections::HashMap;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use corelink_telemetry::otel::exporter::{ExporterVariant, InMemoryFake};
    use tower::ServiceExt;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    fn fake_state(variant: ExporterVariant) -> (OtelExportState, InMemoryFake) {
        let fake = InMemoryFake::new(variant);
        let state = OtelExportState {
            exporter: Arc::new(fake.clone()),
            audit_sink: Arc::new(TracingExportFailedAuditSink),
            region: Arc::from("iad"),
        };
        (state, fake)
    }

    #[test]
    fn from_env_disabled_and_unset_return_none() {
        assert!(OtelExportState::from_lookup(env(&[])).is_none());
        assert!(OtelExportState::from_lookup(env(&[(VARIANT_ENV, "disabled")])).is_none());
    }

    #[test]
    fn from_env_unknown_variant_returns_none() {
        assert!(OtelExportState::from_lookup(env(&[(VARIANT_ENV, "honeycomb")])).is_none());
    }

    #[test]
    fn from_env_otel_collector_arms() {
        let s = OtelExportState::from_lookup(env(&[
            (VARIANT_ENV, "otel_collector"),
            ("CORELINK_OTEL_COLLECTOR_ENDPOINT", "https://c.example.com:4318"),
            ("CORELINK_OTEL_COLLECTOR_PROTOCOL", "http"),
            (REGION_ENV, "fra"),
        ]));
        let s = s.expect("otel_collector should arm");
        assert_eq!(s.exporter.variant(), ExporterVariant::OtelCollector);
        assert_eq!(s.region.as_ref(), "fra");
    }

    #[test]
    fn from_env_otel_collector_missing_endpoint_returns_none() {
        assert!(
            OtelExportState::from_lookup(env(&[(VARIANT_ENV, "otel_collector")])).is_none()
        );
    }

    #[test]
    fn from_env_otel_collector_bad_endpoint_scheme_returns_none() {
        // bare host (no scheme) is rejected by OtelCollectorConfig::new → fail-SAFE None
        assert!(OtelExportState::from_lookup(env(&[
            (VARIANT_ENV, "otel_collector"),
            ("CORELINK_OTEL_COLLECTOR_ENDPOINT", "collector:4318"),
        ]))
        .is_none());
    }

    #[test]
    fn from_env_datadog_arms_with_site() {
        let s = OtelExportState::from_lookup(env(&[
            (VARIANT_ENV, "datadog"),
            ("CORELINK_DATADOG_API_KEY", &"a".repeat(32)),
            ("CORELINK_DATADOG_SITE", "eu1"),
        ]))
        .expect("datadog should arm");
        assert_eq!(s.exporter.variant(), ExporterVariant::Datadog);
    }

    #[test]
    fn from_env_grafana_arms() {
        let s = OtelExportState::from_lookup(env(&[
            (VARIANT_ENV, "grafana_cloud"),
            ("CORELINK_GRAFANA_PROM_REMOTE_WRITE_URL", "https://prom/push"),
            ("CORELINK_GRAFANA_TEMPO_OTLP_URL", "https://tempo/push"),
            ("CORELINK_GRAFANA_INSTANCE_ID", "12345"),
            ("CORELINK_GRAFANA_API_KEY", &"g".repeat(32)),
        ]))
        .expect("grafana should arm");
        assert_eq!(s.exporter.variant(), ExporterVariant::GrafanaCloud);
    }

    #[test]
    fn metric_kind_mapping_is_taxonomy_faithful() {
        assert_eq!(
            metric_kind_for(&Method::POST, "/v1/cas/t/abc"),
            Some(RedMetricKind::CasPutRequestsTotal)
        );
        assert_eq!(
            metric_kind_for(&Method::GET, "/v1/ac/t/abc"),
            Some(RedMetricKind::AcLookupRequestsTotal)
        );
        // GET on CAS has no canonical request-count kind → span-only.
        assert_eq!(metric_kind_for(&Method::GET, "/v1/cas/t/abc"), None);
        assert_eq!(metric_kind_for(&Method::GET, "/v1/admin/read/x"), None);
    }

    #[test]
    fn labels_are_pii_free() {
        let l = labels("iad", "cas", false);
        assert_eq!(l.get("region").map(String::as_str), Some("iad"));
        assert_eq!(l.get("result").map(String::as_str), Some("ok"));
        assert_eq!(l.get("op_type").map(String::as_str), Some("cas"));
        // No tenant / no id keys.
        assert!(!l.keys().any(|k| k.contains("tenant") || k.contains("id")));
    }

    /// Build a `RequestObservation` with canonical placeholder ids/timestamps.
    fn obs<'a>(
        method: &'a Method,
        path: &'a str,
        status: StatusCode,
        duration_secs: f64,
    ) -> RequestObservation<'a> {
        RequestObservation {
            method,
            path,
            status,
            start_ns: 1_000,
            duration_secs,
            trace_id: [1u8; 16],
            span_id: [2u8; 8],
            now_unix_ms: 42,
        }
    }

    #[test]
    fn emit_cas_put_records_counter_duration_and_span() {
        let (state, fake) = fake_state(ExporterVariant::OtelCollector);
        emit(&state, &obs(&Method::POST, "/v1/cas/t/abc", StatusCode::OK, 0.005));
        // one counter + one duration histogram
        let metrics = fake.snapshot_metrics();
        assert_eq!(metrics.len(), 2);
        assert!(metrics
            .iter()
            .any(|m| m.metric_name() == "corelink_cas_put_requests_total"));
        assert!(metrics
            .iter()
            .any(|m| m.metric_name() == "corelink_cas_put_duration_seconds"));
        // one span, ended OK
        let spans = fake.snapshot_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].status, SpanStatus::Ok);
    }

    #[test]
    fn emit_ac_lookup_records_single_counter_and_span() {
        let (state, fake) = fake_state(ExporterVariant::Datadog);
        emit(&state, &obs(&Method::GET, "/v1/ac/t/abc", StatusCode::OK, 0.001));
        assert_eq!(fake.snapshot_metrics().len(), 1);
        assert_eq!(fake.snapshot_spans().len(), 1);
    }

    #[test]
    fn emit_non_canonical_route_is_span_only() {
        let (state, fake) = fake_state(ExporterVariant::OtelCollector);
        emit(
            &state,
            &obs(&Method::GET, "/v1/admin/read/x", StatusCode::OK, 0.001),
        );
        assert!(fake.snapshot_metrics().is_empty());
        assert_eq!(fake.snapshot_spans().len(), 1);
    }

    #[test]
    fn emit_5xx_marks_span_error_and_result_error() {
        let (state, fake) = fake_state(ExporterVariant::OtelCollector);
        emit(
            &state,
            &obs(
                &Method::POST,
                "/v1/cas/t/abc",
                StatusCode::INTERNAL_SERVER_ERROR,
                0.002,
            ),
        );
        let spans = fake.snapshot_spans();
        assert_eq!(spans[0].status, SpanStatus::Error);
        let metrics = fake.snapshot_metrics();
        assert!(metrics
            .iter()
            .all(|m| m.labels.get("result").map(String::as_str) == Some("error")));
    }

    #[tokio::test]
    async fn layer_exports_for_data_plane_request_via_router() {
        let (state, fake) = fake_state(ExporterVariant::OtelCollector);
        let app = Router::new()
            .route("/v1/cas/{tenant}/{hash}", post(|| async { StatusCode::OK }))
            .route("/v1/ac/{tenant}/{hash}", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                otel_export_layer,
            ));

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/cas/t1/deadbeef")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // CAS PUT ⇒ 2 metric points (counter + duration) + 1 span.
        assert_eq!(fake.snapshot_metrics().len(), 2);
        assert_eq!(fake.snapshot_spans().len(), 1);
    }
}
