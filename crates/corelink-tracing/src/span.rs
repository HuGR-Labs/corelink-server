//! `SpanRecord` shape — OTLP §5 ResourceSpans-aligned canonical
//! span envelope + `SpanKind` 5-canonical enum + `SpanStatus` enum +
//! `Exemplar` linkage to `corelink-analytics::RedMetricKind`.
//!
//! ## OTLP semantic alignment
//!
//! Per OpenTelemetry Protocol Specification §5 ResourceSpans the
//! canonical span shape carries:
//!
//! - `trace_id` (16 bytes; W3C §3.2.2.2 conformant).
//! - `span_id` (8 bytes; W3C §3.2.2.3 conformant).
//! - `parent_span_id` (8 bytes; `None` for root spans).
//! - `name` (operation name; canonical free-text).
//! - `kind` (canonical 5-enum; INTERNAL / SERVER / CLIENT / PRODUCER /
//!   CONSUMER per OTLP `SpanKind`).
//! - `status` (OK / Error per OTLP `Status`; `UNSET` initial).
//! - `start_time_ns` / `end_time_ns` (Unix epoch nanoseconds; OTLP
//!   canonical wire shape).
//! - `attributes` (canonical key-value pairs; OTLP `KeyValue`).
//! - `exemplars` (linkage to RED metric histogram observations per
//!   OpenMetrics 1.0 §exemplars + CAP-OBS-009).
//!
//! The CloudEvents-aligned envelope (used by the WI-S09-004 audit chain
//! emitter) wraps this body via the `corelink-audit::AuditEnvelope`.

use serde::{Deserialize, Serialize};

use corelink_analytics::RedMetricKind;

use crate::context::{SpanId, TraceId};

/// Canonical OTLP `SpanKind` enum. Per OTLP `trace/v1/trace.proto`
/// the 5 canonical variants are:
///
/// - `Unspecified` (default; producer SHOULD NOT use this).
/// - `Internal` (operation INTERNAL to the service).
/// - `Server` (handler at the SERVER side of a remote call).
/// - `Client` (caller at the CLIENT side of a remote call).
/// - `Producer` (asynchronous PRODUCER / message creation).
/// - `Consumer` (asynchronous CONSUMER / message processing).
///
/// `#[non_exhaustive]` so follow-on OTLP version bumps land additively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SpanKind {
    /// `SPAN_KIND_UNSPECIFIED` — default; SHOULD NOT be used by
    /// producers.
    Unspecified,
    /// `SPAN_KIND_INTERNAL` — INTERNAL operation.
    Internal,
    /// `SPAN_KIND_SERVER` — server-side handler of a remote call.
    Server,
    /// `SPAN_KIND_CLIENT` — client-side caller of a remote call.
    Client,
    /// `SPAN_KIND_PRODUCER` — asynchronous producer / message creator.
    Producer,
    /// `SPAN_KIND_CONSUMER` — asynchronous consumer / message
    /// processor.
    Consumer,
}

impl SpanKind {
    /// Canonical OTLP slug per `trace/v1/trace.proto`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "SPAN_KIND_UNSPECIFIED",
            Self::Internal => "SPAN_KIND_INTERNAL",
            Self::Server => "SPAN_KIND_SERVER",
            Self::Client => "SPAN_KIND_CLIENT",
            Self::Producer => "SPAN_KIND_PRODUCER",
            Self::Consumer => "SPAN_KIND_CONSUMER",
        }
    }
}

impl core::fmt::Display for SpanKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SpanKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SpanKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "SPAN_KIND_UNSPECIFIED" => Ok(Self::Unspecified),
            "SPAN_KIND_INTERNAL" => Ok(Self::Internal),
            "SPAN_KIND_SERVER" => Ok(Self::Server),
            "SPAN_KIND_CLIENT" => Ok(Self::Client),
            "SPAN_KIND_PRODUCER" => Ok(Self::Producer),
            "SPAN_KIND_CONSUMER" => Ok(Self::Consumer),
            other => Err(serde::de::Error::custom(format!(
                "unknown SpanKind: {other}"
            ))),
        }
    }
}

/// Canonical 6-element [`SpanKind`] list — pinned for cardinality
/// estimate (the OTLP spec freezes 6 variants today). Used by the
/// surface-pinning regression tests + the `prop_span_kind_matches_otlp`
/// property test.
#[must_use]
pub const fn canonical_span_kinds() -> &'static [SpanKind; 6] {
    &[
        SpanKind::Unspecified,
        SpanKind::Internal,
        SpanKind::Server,
        SpanKind::Client,
        SpanKind::Producer,
        SpanKind::Consumer,
    ]
}

/// Canonical OTLP `Status` enum. Per OTLP `trace/v1/trace.proto`
/// the 3 canonical variants are `Unset` / `Ok` / `Error`. The
/// `Unset` default applies to spans that have not yet ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SpanStatus {
    /// `STATUS_CODE_UNSET` — initial state.
    Unset,
    /// `STATUS_CODE_OK` — handler completed successfully.
    Ok,
    /// `STATUS_CODE_ERROR` — handler completed with error (any 5xx
    /// response / RPC failure).
    Error,
}

impl SpanStatus {
    /// Canonical OTLP slug per `trace/v1/trace.proto`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unset => "STATUS_CODE_UNSET",
            Self::Ok => "STATUS_CODE_OK",
            Self::Error => "STATUS_CODE_ERROR",
        }
    }

    /// Whether the handler completed with an error. Per WI §6.1.4
    /// tail-sampling 100% applies whenever this returns `true`
    /// (deferred to WI-S09-007 PRR ship gate).
    #[must_use]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl core::fmt::Display for SpanStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SpanStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SpanStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "STATUS_CODE_UNSET" => Ok(Self::Unset),
            "STATUS_CODE_OK" => Ok(Self::Ok),
            "STATUS_CODE_ERROR" => Ok(Self::Error),
            other => Err(serde::de::Error::custom(format!(
                "unknown SpanStatus: {other}"
            ))),
        }
    }
}

/// Exemplar linkage span ↔ RED metric histogram observation
/// (OpenMetrics 1.0 §exemplars + CAP-OBS-009).
///
/// The exemplar binds a span to a single bucket of a histogram; the
/// production wiring renders this as `<metric>{...} N # {trace_id="abc"} value timestamp`
/// per OpenMetrics 1.0 line format. The `metric_kind` slot pins the
/// exemplar to the canonical 15-RED + USE taxonomy so the dashboard
/// widget can deep-link from the histogram cell to the Tempo trace
/// view (per 3 fluxos: cas.put, cas.get, ac.lookup; sprint contract
/// §10.s09.6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exemplar {
    /// Canonical RED metric kind this exemplar links to.
    #[serde(serialize_with = "serialize_metric_kind")]
    #[serde(deserialize_with = "deserialize_metric_kind")]
    pub metric_kind: RedMetricKind,
    /// Histogram bucket observation value (seconds; canonical OpenMetrics
    /// `*_seconds` suffix).
    pub value: f64,
    /// Producer-side wall-clock instant (Unix epoch ms; matches the
    /// CloudEvents `time_ms` discipline per Lote 10.7bis P0-3).
    pub time_ms: u64,
}

fn serialize_metric_kind<S: serde::Serializer>(k: &RedMetricKind, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(k.as_str())
}

fn deserialize_metric_kind<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<RedMetricKind, D::Error> {
    let s = String::deserialize(d)?;
    metric_kind_from_str(&s)
        .ok_or_else(|| serde::de::Error::custom(format!("unknown RedMetricKind: {s}")))
}

fn metric_kind_from_str(s: &str) -> Option<RedMetricKind> {
    Some(match s {
        "corelink_cas_put_requests_total" => RedMetricKind::CasPutRequestsTotal,
        "corelink_cas_put_duration_seconds" => RedMetricKind::CasPutDurationSeconds,
        "corelink_cas_get_bytes_total" => RedMetricKind::CasGetBytesTotal,
        "corelink_ac_lookup_requests_total" => RedMetricKind::AcLookupRequestsTotal,
        "corelink_gc_runs_total" => RedMetricKind::GcRunsTotal,
        "corelink_dedup_ratio" => RedMetricKind::DedupRatio,
        "corelink_rate_limit_rejects_total" => RedMetricKind::RateLimitRejectsTotal,
        "corelink_privacy_dsr_active_total" => RedMetricKind::PrivacyDsrActiveTotal,
        "corelink_billing_events_emitted_total" => RedMetricKind::BillingEventsEmittedTotal,
        "corelink_cf_cpu_time_us" => RedMetricKind::CfCpuTimeUs,
        "corelink_r2_ops_total" => RedMetricKind::R2OpsTotal,
        "corelink_d1_row_scans_total" => RedMetricKind::D1RowScansTotal,
        "corelink_kv_read_quota_used" => RedMetricKind::KvReadQuotaUsed,
        "corelink_kv_write_quota_used" => RedMetricKind::KvWriteQuotaUsed,
        "corelink_do_storage_size_bytes" => RedMetricKind::DoStorageSizeBytes,
        _ => return None,
    })
}

/// OTLP-aligned span record. Carries the canonical W3C Trace Context
/// shape + the OTLP §5 ResourceSpans body fields. Production wiring
/// fans this out to the OTLP exporter (Tempo backend) via the
/// [`crate::exporter::OtlpExporter`] trait.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpanRecord {
    /// W3C 16-byte trace identifier.
    pub trace_id: TraceId,
    /// W3C 8-byte span identifier (this span).
    pub span_id: SpanId,
    /// W3C 8-byte parent span identifier; `None` for root spans.
    pub parent_span_id: Option<SpanId>,
    /// Operation name (canonical free-text per OTLP §5).
    pub name: String,
    /// OTLP `SpanKind` enum.
    pub kind: SpanKind,
    /// OTLP `Status` enum (`Unset` initial; `Ok`/`Error` on end).
    pub status: SpanStatus,
    /// Start instant — Unix epoch nanoseconds (OTLP wire shape).
    pub start_time_ns: u64,
    /// End instant — Unix epoch nanoseconds; `0` for spans that have
    /// not yet ended.
    pub end_time_ns: u64,
    /// Canonical key-value attributes. Per WI §6.1.9 raw PII is
    /// FORBIDDEN here (use the `corelink-logpush` redactor before
    /// admitting an attribute value).
    pub attributes: Vec<(String, String)>,
    /// Exemplar linkages to RED metric histograms (CAP-OBS-009).
    pub exemplars: Vec<Exemplar>,
}

impl SpanRecord {
    /// Construct an in-flight span record (status = `Unset`,
    /// `end_time_ns = 0`). Returns the typed shape; the orchestrator
    /// pipeline (sampler → audit → exporter) handles emit.
    #[must_use]
    pub fn new(
        trace_id: TraceId,
        span_id: SpanId,
        parent_span_id: Option<SpanId>,
        name: impl Into<String>,
        kind: SpanKind,
        start_time_ns: u64,
    ) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id,
            name: name.into(),
            kind,
            status: SpanStatus::Unset,
            start_time_ns,
            end_time_ns: 0,
            attributes: Vec::new(),
            exemplars: Vec::new(),
        }
    }

    /// Mark this span as ended with the canonical OTLP status.
    pub fn end(&mut self, status: SpanStatus, end_time_ns: u64) {
        self.status = status;
        self.end_time_ns = end_time_ns;
    }

    /// Append a canonical key-value attribute. Caller is responsible
    /// for redaction discipline (raw PII FORBIDDEN per WI §6.1.9).
    pub fn add_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.push((key.into(), value.into()));
    }

    /// Append an exemplar linkage to a RED metric histogram
    /// observation. CAP-OBS-009 deep-link from the dashboard widget
    /// to the Tempo trace view.
    pub fn add_exemplar(&mut self, exemplar: Exemplar) {
        self.exemplars.push(exemplar);
    }

    /// Hex-encode the trace_id (32 lowercase hex chars; W3C canonical).
    #[must_use]
    pub fn trace_id_hex(&self) -> String {
        bytes_to_hex(&self.trace_id)
    }

    /// Hex-encode the span_id (16 lowercase hex chars; W3C canonical).
    #[must_use]
    pub fn span_id_hex(&self) -> String {
        bytes_to_hex(&self.span_id)
    }

    /// Whether this span has ended (`end_time_ns != 0`). Spans with
    /// status `Unset` are considered in-flight.
    #[must_use]
    pub const fn is_ended(&self) -> bool {
        self.end_time_ns != 0
    }
}

fn bytes_to_hex(bs: &[u8]) -> String {
    let mut out = String::with_capacity(bs.len() * 2);
    for b in bs {
        out.push(hex_digit(*b >> 4));
        out.push(hex_digit(*b & 0x0F));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        // Unreachable by construction.
        _ => '0',
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn sample_trace_id() -> TraceId {
        [
            0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e,
            0x47, 0x36,
        ]
    }

    fn sample_span_id() -> SpanId {
        [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7]
    }

    #[test]
    fn span_kind_canonical_strings_pinned() {
        assert_eq!(SpanKind::Unspecified.as_str(), "SPAN_KIND_UNSPECIFIED");
        assert_eq!(SpanKind::Internal.as_str(), "SPAN_KIND_INTERNAL");
        assert_eq!(SpanKind::Server.as_str(), "SPAN_KIND_SERVER");
        assert_eq!(SpanKind::Client.as_str(), "SPAN_KIND_CLIENT");
        assert_eq!(SpanKind::Producer.as_str(), "SPAN_KIND_PRODUCER");
        assert_eq!(SpanKind::Consumer.as_str(), "SPAN_KIND_CONSUMER");
    }

    #[test]
    fn canonical_span_kinds_unique_strings() {
        let v = canonical_span_kinds();
        assert_eq!(v.len(), 6);
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("SPAN_KIND_"));
            assert!(set.insert(k.as_str()));
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn span_status_canonical_strings_pinned() {
        assert_eq!(SpanStatus::Unset.as_str(), "STATUS_CODE_UNSET");
        assert_eq!(SpanStatus::Ok.as_str(), "STATUS_CODE_OK");
        assert_eq!(SpanStatus::Error.as_str(), "STATUS_CODE_ERROR");
        assert!(SpanStatus::Error.is_error());
        assert!(!SpanStatus::Ok.is_error());
        assert!(!SpanStatus::Unset.is_error());
    }

    #[test]
    fn span_record_round_trip_serializes() {
        let mut s = SpanRecord::new(
            sample_trace_id(),
            sample_span_id(),
            None,
            "cas.put",
            SpanKind::Server,
            100_000,
        );
        s.end(SpanStatus::Ok, 200_000);
        s.add_attribute("region", "iad");
        s.add_exemplar(Exemplar {
            metric_kind: RedMetricKind::CasPutDurationSeconds,
            value: 0.004,
            time_ms: 1_700_000_000_000,
        });
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SpanRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(s, parsed);
        assert!(json.contains("\"kind\":\"SPAN_KIND_SERVER\""));
        assert!(json.contains("\"status\":\"STATUS_CODE_OK\""));
    }

    #[test]
    fn span_record_in_flight_has_unset_status() {
        let s = SpanRecord::new(
            sample_trace_id(),
            sample_span_id(),
            None,
            "cas.put",
            SpanKind::Internal,
            100,
        );
        assert_eq!(s.status, SpanStatus::Unset);
        assert_eq!(s.end_time_ns, 0);
        assert!(!s.is_ended());
    }

    #[test]
    fn span_record_ended_has_status() {
        let mut s = SpanRecord::new(
            sample_trace_id(),
            sample_span_id(),
            None,
            "cas.put",
            SpanKind::Internal,
            100,
        );
        s.end(SpanStatus::Error, 200);
        assert_eq!(s.status, SpanStatus::Error);
        assert_eq!(s.end_time_ns, 200);
        assert!(s.is_ended());
    }

    #[test]
    fn trace_id_hex_canonical_lowercase() {
        let s = SpanRecord::new(
            sample_trace_id(),
            sample_span_id(),
            None,
            "x",
            SpanKind::Internal,
            1,
        );
        assert_eq!(s.trace_id_hex(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(s.span_id_hex(), "00f067aa0ba902b7");
    }

    #[test]
    fn unknown_span_kind_deserialize_errors() {
        let line = r#""SPAN_KIND_BAD""#;
        let err: serde_json::Result<SpanKind> = serde_json::from_str(line);
        assert!(err.is_err());
    }

    #[test]
    fn unknown_span_status_deserialize_errors() {
        let line = r#""STATUS_CODE_BAD""#;
        let err: serde_json::Result<SpanStatus> = serde_json::from_str(line);
        assert!(err.is_err());
    }

    #[test]
    fn exemplar_serializes_metric_kind_canonical_string() {
        let e = Exemplar {
            metric_kind: RedMetricKind::CasPutDurationSeconds,
            value: 0.004,
            time_ms: 1,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"metric_kind\":\"corelink_cas_put_duration_seconds\""));
        let parsed: Exemplar = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.metric_kind, e.metric_kind);
        assert_eq!(parsed.value, e.value);
        assert_eq!(parsed.time_ms, e.time_ms);
    }

    #[test]
    fn exemplar_unknown_metric_kind_deserialize_errors() {
        let json = r#"{"metric_kind":"corelink_unknown","value":0.0,"time_ms":1}"#;
        let err: serde_json::Result<Exemplar> = serde_json::from_str(json);
        assert!(err.is_err());
    }

    #[test]
    fn add_attribute_appends() {
        let mut s = SpanRecord::new(
            sample_trace_id(),
            sample_span_id(),
            None,
            "x",
            SpanKind::Internal,
            1,
        );
        s.add_attribute("a", "1");
        s.add_attribute("b", "2");
        assert_eq!(s.attributes.len(), 2);
    }
}
