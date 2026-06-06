//! `corelink-tracing` — OTLP distributed tracing primitive (WI-S09-003).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the OTLP tracing primitive: trait surfaces every
//! production OpenTelemetry SDK + Cloudflare Workers binding will
//! satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on. Property
//! tests pinned at 10k iter against the orchestrator cover W3C Trace
//! Context Recommendation 2020 round-trip, sampler proportionality,
//! tenant isolation, exemplar OpenMetrics 1.0 §exemplars binding to
//! the `RedMetricKind` canonical taxonomy, and audit fail-closed
//! envelope discipline.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`context`] module ships [`TraceContext`] (W3C Trace Context
//!    Recommendation 2020 canonical: 16-byte `trace_id` + 8-byte
//!    `span_id` + 1-byte trace flags) + [`parse_traceparent`] /
//!    [`format_traceparent`] strict ABNF compliance + canonical
//!    constants (`W3C_TRACE_CONTEXT_VERSION`, `TRACE_FLAGS_SAMPLED`,
//!    `ALL_ZERO_TRACE_ID`, `ALL_ZERO_SPAN_ID`, `TRACEPARENT_HEADER`,
//!    `TRACESTATE_HEADER`).
//! 2. The [`span`] module ships [`SpanRecord`] (OTLP §5
//!    ResourceSpans-aligned: `trace_id` / `span_id` /
//!    `parent_span_id` / `name` / `kind` / `status` / `start_time_ns` /
//!    `end_time_ns` / `attributes` / `exemplars`) + [`SpanKind`]
//!    `#[non_exhaustive]` 6-canonical (`Unspecified` / `Internal` /
//!    `Server` / `Client` / `Producer` / `Consumer` per OTLP
//!    `trace/v1/trace.proto`) + [`SpanStatus`] (`Unset` / `Ok` /
//!    `Error`) + [`Exemplar`] (linkage to `corelink-analytics`
//!    `RedMetricKind` per OpenMetrics 1.0 §exemplars + CAP-OBS-009).
//! 3. The [`sampler`] module ships [`Sampler`] trait +
//!    [`RateBasedSampler`] (head-based deterministic per W3C
//!    trace_id; canonical 1% production / 100% staging) +
//!    [`SamplingDecision`] `#[non_exhaustive]` 3-canonical
//!    (`HeadSampled` / `TailSampled` / `Drop`; tail-based deferred
//!    to WI-S09-007 PRR ship gate).
//! 4. The [`exporter`] module ships [`OtlpExporter`] trait +
//!    [`InMemoryOtlpExporter`] capture sink + [`FailingOtlpExporter`]
//!    adversarial fixture.
//! 5. The [`service`] module ships [`TracingService`] orchestrator
//!    (sampler → audit → in-flight ledger → exporter dispatch with
//!    fail-closed audit envelope on every decision arm).
//! 6. The [`audit`] module ships [`TracingAuditEventType`]
//!    (`#[non_exhaustive]` 4-event taxonomy:
//!    `corelink.tracing.{span_started, span_ended, sampler_decision,
//!    exporter_failure}`) + [`TracingAuditRecord`] +
//!    [`TracingAuditSink`] + [`InMemoryTracingAuditSink`] capture sink
//!    (fail-closed envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 7. The [`error`] module ships the canonical [`TracingError`]
//!    `#[non_exhaustive]` taxonomy.
//!
//! # Why tracing is `trait + fake` here, real OTLP HTTP exporter in
//! # WI-S09-007
//!
//! S-09 lands without Grafana Tempo OTLP HTTP `/v1/traces` bindings
//! wired into CI (no remote + Cloudflare Workers + Tempo staging are
//! HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! W3C Trace Context Recommendation 2020 round-trip strict (parse +
//! format identity for valid inputs; reject all canonical violations
//! per W3C §3.2.2); sampler proportionality (over N spans at rate p,
//! sampled count ≈ p × N within statistical tolerance); tenant
//! isolation (tenant A spans never reference tenant B trace IDs);
//! exemplar OpenMetrics 1.0 §exemplars binding to `RedMetricKind`;
//! audit fail-closed envelope on every decision arm. The live
//! Tempo OTLP HTTP exporter + tail-based sampling + W3C tracestate
//! vendor extension propagation runs alongside WI-S09-007 (PRR ship
//! gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **W3C Trace Context Recommendation 2020 strict compliance**
//!   (sprint contract §5.3 R-S09-7): canonical 55-char ASCII
//!   `traceparent` header; parser rejects malformed inputs (invalid
//!   version / wrong length / malformed hex / all-zero trace_id /
//!   all-zero span_id per W3C §3.2.2). Pinned by
//!   `prop_traceparent_roundtrip` + `prop_traceparent_rejects_invalid` +
//!   `prop_zero_trace_id_rejected`.
//! - **OTLP SpanKind 6-canonical** (sprint contract §5.3 R-S09-7;
//!   OTLP `trace/v1/trace.proto`): the [`SpanKind`] enum mirrors the
//!   OTLP closed taxonomy 1:1. Pinned by `prop_span_kind_matches_otlp`.
//! - **Sampler proportionality** (sprint contract §5.3 R-S09-8):
//!   over N spans at rate p, sampled count ≈ p × N (within statistical
//!   tolerance). Pinned by `prop_sampler_rate_proportional`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07
//!   P1-1 fix plus Lote 10.6bis pattern): audit emit BEFORE state
//!   mutation on every decision arm (`span_started`, `span_ended`,
//!   `sampler_decision`, `exporter_failure`); audit failure aborts
//!   the emit plus returns a typed error. Pinned by
//!   `prop_audit_emit_per_event_type`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): tenant A spans never reference tenant B trace
//!   IDs. The orchestrator keys the in-flight span ledger by
//!   `(tenant_tier, span_id)` so cross-tenant lookup is structurally
//!   impossible. Pinned by `prop_tenant_isolation`.
//! - **Exemplar canonical link** (CAP-OBS-009; sprint contract §5.3
//!   R-S09-9 + §10.s09.6): every exemplar carries a canonical
//!   `RedMetricKind` so the dashboard widget deep-links from the
//!   histogram cell to the Tempo trace view. Pinned by
//!   `prop_exemplar_link_to_metric`.
//!
//! # Production wiring (deferred to WI-S09-007)
//!
//! - OTLP HTTP exporter (Tempo backend) via `worker::send_future`
//!   fire-and-forget per Lote 10.7bis R5 P0-3 (NEVER `tokio::spawn`).
//! - Tail-based sampling (100% errors + p99 SLO breach per Google SRE
//!   Workbook Ch 6). Initial ship is head-based at 1% production /
//!   100% staging.
//! - W3C `tracestate` vendor extension propagation (per W3C §3.3).
//! - Per-region Tempo tenant API key in CF Worker secret.
//! - 100k nightly property test (`PROPTEST_CASES=100000`) + chaos 10.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod context;
pub mod error;
pub mod exporter;
pub mod sampler;
pub mod service;
pub mod span;

pub use audit::{
    canonical_audit_event_strings, FailingTracingAuditSink, InMemoryTracingAuditSink,
    TracingAuditEmitError, TracingAuditEventType, TracingAuditRecord, TracingAuditSink,
};
pub use context::{
    format_traceparent, parse_traceparent, SpanId, TraceContext, TraceId, ALL_ZERO_SPAN_ID,
    ALL_ZERO_TRACE_ID, TRACEPARENT_HEADER, TRACESTATE_HEADER, TRACE_FLAGS_SAMPLED,
    W3C_TRACE_CONTEXT_VERSION,
};
pub use error::{OtlpExporterError, TracingAuditSinkError, TracingError};
pub use exporter::{FailingOtlpExporter, InMemoryOtlpExporter, OtlpExporter};
pub use sampler::{RateBasedSampler, Sampler, SamplingDecision};
pub use service::{EndSpanInput, StartSpanInput, TracingService};
pub use span::{canonical_span_kinds, Exemplar, SpanKind, SpanRecord, SpanStatus};

/// Canonical schema version for the tracing emitter (mirrors the
/// production OTLP HTTP wire shape; today FROZEN at 1 since the
/// crate ships only the pure-logic skeleton without a durable D1
/// migration mirror — production wiring at WI-S09-007 will lift this
/// alongside the OTLP HTTP exporter ship gate).
#[must_use]
pub const fn tracing_schema_version() -> u32 {
    1
}
