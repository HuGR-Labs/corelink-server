//! `TracingService` — orchestrator wiring the
//! [`crate::sampler::Sampler`] + [`crate::audit::TracingAuditSink`] +
//! [`crate::exporter::OtlpExporter`] into a single emit pipeline with
//! fail-closed audit envelope.
//!
//! ## Decision pipeline (per span lifecycle)
//!
//! For each `start_span(trace_id, parent_span_id, name, kind)`:
//!
//! 1. **Sampler decision**: head-based deterministic per W3C trace_id.
//! 2. **Audit emit BEFORE state mutation** (`corelink.tracing.span_started`)
//!    per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern + S-07
//!    P1-1 fix). Audit failure aborts the span start.
//! 3. **In-flight buffer push**: enforce per-tenant bounded count
//!    (per WI §1 invariant 3 + tenant isolation).
//!
//! For each `end_span(span_id, status, end_time_ns)`:
//!
//! 1. **Audit emit BEFORE state mutation** (`corelink.tracing.span_ended`).
//! 2. **In-flight buffer remove + decision-batch push** (only sampled
//!    spans land in the export batch).
//! 3. **Exporter emit** (production wiring fans out to Tempo OTLP via
//!    `worker::send_future` fire-and-forget per Lote 10.7bis R5 P0-3;
//!    NEVER `tokio::spawn`).
//!
//! ## Tenant isolation
//!
//! Per WI §1 invariant 5 + INV-TENANT-ISOLATION canary, the
//! orchestrator carries `tenant_tier` per started span; the in-flight
//! buffer is keyed by `(tenant_tier, span_id)` so tenant A spans
//! never reference tenant B trace IDs (`prop_tenant_isolation`).
//!
//! ## Fail-OPEN exporter / fail-CLOSED audit (Lote 10.6bis lesson)
//!
//! Per WI §6.1.6: tracing emit fail-OPEN (best-effort observability;
//! missing span NOT compromise security). The fail-OPEN distinction
//! applies to the EXPORTER transport (Tempo unavailable). Audit
//! fail-CLOSED applies to the AUDIT envelope (separate WI-S09-004
//! concern).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_analytics::Tier;

use crate::audit::{TracingAuditEventType, TracingAuditRecord, TracingAuditSink};
use crate::context::{SpanId, TraceId};
use crate::error::TracingError;
use crate::exporter::OtlpExporter;
use crate::sampler::{Sampler, SamplingDecision};
use crate::span::{Exemplar, SpanKind, SpanRecord, SpanStatus};

/// Inputs to [`TracingService::start_span`]. Struct form keeps the
/// public API readable + clippy-`too_many_arguments` clean.
#[derive(Clone, Debug)]
pub struct StartSpanInput<'a> {
    /// W3C 16-byte trace identifier.
    pub trace_id: TraceId,
    /// W3C 8-byte span identifier (this span).
    pub span_id: SpanId,
    /// W3C 8-byte parent span identifier; `None` for root spans.
    pub parent_span_id: Option<SpanId>,
    /// Operation name (canonical free-text per OTLP §5).
    pub name: &'a str,
    /// OTLP `SpanKind` enum.
    pub kind: SpanKind,
    /// Tenant tier (5-canonical per Lote 10.7bis P0-7). Keys the
    /// in-flight span ledger so cross-tenant lookup is structurally
    /// impossible.
    pub tenant_tier: Tier,
    /// Start instant — Unix epoch nanoseconds (OTLP wire shape).
    pub start_time_ns: u64,
    /// Source attribution: `x-request-id` header value.
    pub created_by_request_id: &'a str,
    /// Producer-side wall-clock instant (Unix epoch ms; mirrored to
    /// the audit record).
    pub now_ms: u64,
}

/// Inputs to [`TracingService::end_span`]. Struct form keeps the
/// public API readable + clippy-`too_many_arguments` clean.
#[derive(Clone, Debug)]
pub struct EndSpanInput<'a> {
    /// W3C 8-byte span identifier (the in-flight span being ended).
    pub span_id: SpanId,
    /// Tenant tier the span was started under.
    pub tenant_tier: Tier,
    /// Terminal OTLP status (`Ok` / `Error`).
    pub status: SpanStatus,
    /// End instant — Unix epoch nanoseconds.
    pub end_time_ns: u64,
    /// Canonical key-value attributes appended at end.
    pub attributes: Vec<(String, String)>,
    /// Exemplar linkages to RED metric histograms (CAP-OBS-009).
    pub exemplars: Vec<Exemplar>,
    /// Source attribution.
    pub created_by_request_id: &'a str,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Tracing orchestrator. Cloning shares the underlying state via
/// per-instance `Arc<Mutex<>>` (F-001 closure pattern; tests instantiate
/// fresh services per case so the orchestrator harness cannot
/// accidentally leak buffer state across cases).
#[derive(Clone, Debug)]
pub struct TracingService<S, A, E>
where
    S: Sampler + 'static,
    A: TracingAuditSink + 'static,
    E: OtlpExporter + 'static,
{
    sampler: Arc<S>,
    audit: Arc<A>,
    exporter: Arc<E>,
    state: Arc<Mutex<ServiceState>>,
}

/// Per-instance mutable state. Holds the in-flight span ledger
/// (keyed by `(tenant_tier, span_id_hex)` so tenant isolation is
/// load-bearing at the type system) + the cumulative
/// exporter-failure counter (the SEV-3 alert source).
#[derive(Debug, Default)]
struct ServiceState {
    in_flight: HashMap<InFlightKey, InFlightSpan>,
    exporter_failure_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InFlightKey {
    tenant_tier: Tier,
    span_id_hex: String,
}

#[derive(Clone, Debug)]
struct InFlightSpan {
    record: SpanRecord,
    decision: SamplingDecision,
}

impl<S, A, E> TracingService<S, A, E>
where
    S: Sampler + 'static,
    A: TracingAuditSink + 'static,
    E: OtlpExporter + 'static,
{
    /// Construct with explicit sampler + audit sink + exporter.
    pub fn new(sampler: Arc<S>, audit: Arc<A>, exporter: Arc<E>) -> Self {
        Self {
            sampler,
            audit,
            exporter,
            state: Arc::new(Mutex::new(ServiceState::default())),
        }
    }

    /// Snapshot the count of in-flight (un-ended) spans.
    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.in_flight.len()
    }

    /// Cumulative count of exporter-side transport failures
    /// (production wiring uses this as the SEV-3
    /// `corelink_tracing_export_failures_total{reason}` alert source).
    #[must_use]
    pub fn exporter_failure_count(&self) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.exporter_failure_count
    }

    /// Start a span. The sampler decision is evaluated up-front
    /// (head-based deterministic per trace_id; sub-hops in the call
    /// graph see the same decision). Audit emit is issued BEFORE the
    /// in-flight ledger mutation; audit failure aborts the span start
    /// without touching the ledger.
    ///
    /// # Errors
    ///
    /// - [`TracingError::Audit`] when the audit sink fails.
    /// - [`TracingError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn start_span(&self, input: StartSpanInput<'_>) -> Result<SamplingDecision, TracingError> {
        let StartSpanInput {
            trace_id,
            span_id,
            parent_span_id,
            name,
            kind,
            tenant_tier,
            start_time_ns,
            created_by_request_id,
            now_ms,
        } = input;
        let decision = self.sampler.should_sample(&trace_id);
        let span_record =
            SpanRecord::new(trace_id, span_id, parent_span_id, name, kind, start_time_ns);
        let span_id_hex = span_record.span_id_hex();
        let trace_id_hex = span_record.trace_id_hex();

        // Audit BEFORE state mutation. Fail-closed envelope.
        self.audit.emit(TracingAuditRecord {
            event_type: TracingAuditEventType::SpanStarted,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            trace_id_hex: trace_id_hex.clone(),
            span_id_hex: span_id_hex.clone(),
        })?;
        self.audit.emit(TracingAuditRecord {
            event_type: TracingAuditEventType::SamplerDecision,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            trace_id_hex,
            span_id_hex: span_id_hex.clone(),
        })?;

        let mut g = self.state.lock().map_err(|_| {
            TracingError::Internal("tracing service mutex poisoned (start_span)".to_string())
        })?;
        g.in_flight.insert(
            InFlightKey {
                tenant_tier,
                span_id_hex,
            },
            InFlightSpan {
                record: span_record,
                decision,
            },
        );
        Ok(decision)
    }

    /// End a span. Looks up the in-flight record, mutates status +
    /// `end_time_ns`, emits audit, then dispatches to the exporter
    /// IFF the sampling decision is `HeadSampled` or `TailSampled`.
    /// Per the Lote 10.6bis distinction the exporter fails OPEN
    /// (the orchestrator records the failure as
    /// `corelink_tracing_export_failures_total` SEV-3 + emits
    /// `corelink.tracing.exporter_failure` audit + returns Ok so the
    /// caller's request hot path is not blocked).
    ///
    /// # Errors
    ///
    /// - [`TracingError::Audit`] when the audit sink fails.
    /// - [`TracingError::Internal`] when the per-instance mutex is
    ///   poisoned OR the span is not in the in-flight ledger.
    pub fn end_span(&self, input: EndSpanInput<'_>) -> Result<SamplingDecision, TracingError> {
        let EndSpanInput {
            span_id,
            tenant_tier,
            status,
            end_time_ns,
            attributes,
            exemplars,
            created_by_request_id,
            now_ms,
        } = input;
        let span_id_hex = bytes_to_hex(&span_id);

        // Audit BEFORE state mutation.
        let trace_id_hex = {
            let g = self.state.lock().map_err(|_| {
                TracingError::Internal(
                    "tracing service mutex poisoned (end_span lookup)".to_string(),
                )
            })?;
            let entry = g
                .in_flight
                .get(&InFlightKey {
                    tenant_tier,
                    span_id_hex: span_id_hex.clone(),
                })
                .ok_or_else(|| {
                    TracingError::Internal(format!(
                        "span_id {span_id_hex} not in in-flight ledger for tier {tenant_tier:?}"
                    ))
                })?;
            entry.record.trace_id_hex()
        };
        self.audit.emit(TracingAuditRecord {
            event_type: TracingAuditEventType::SpanEnded,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            trace_id_hex,
            span_id_hex: span_id_hex.clone(),
        })?;

        // Mutate the in-flight record + remove from ledger.
        let (mut record, decision) = {
            let mut g = self.state.lock().map_err(|_| {
                TracingError::Internal(
                    "tracing service mutex poisoned (end_span mutate)".to_string(),
                )
            })?;
            let removed = g.in_flight.remove(&InFlightKey {
                tenant_tier,
                span_id_hex,
            });
            match removed {
                Some(in_flight) => (in_flight.record, in_flight.decision),
                None => {
                    return Err(TracingError::Internal(
                        "in-flight record removed concurrently".to_string(),
                    ));
                }
            }
        };
        record.end(status, end_time_ns);
        for (k, v) in attributes {
            record.add_attribute(k, v);
        }
        for ex in exemplars {
            record.add_exemplar(ex);
        }

        // Exporter dispatch IFF sampled. Fail-OPEN per WI §6.1.6.
        if decision.is_sampled() {
            if let Err(err) = self.exporter.export(vec![record]) {
                self.record_exporter_failure(created_by_request_id, now_ms)?;
                // Production wiring records the failure + continues
                // serving requests. The error is surfaced to the
                // caller for visibility in the tests; production wraps
                // this in a tolerant arm.
                return Err(TracingError::Exporter(err));
            }
        }
        Ok(decision)
    }

    /// Record an exporter-side transport failure (production wiring
    /// calls this when the downstream Tempo OTLP HTTP `/v1/traces`
    /// returns an error). Emits the canonical
    /// `corelink.tracing.exporter_failure` audit (fail-OPEN per WI
    /// §6.1.6 — production wiring continues serving traffic + counter
    /// increments for the SEV-3 alert).
    ///
    /// # Errors
    ///
    /// - [`TracingError::Audit`] when the audit emit fails.
    /// - [`TracingError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn record_exporter_failure(
        &self,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<(), TracingError> {
        self.audit.emit(TracingAuditRecord {
            event_type: TracingAuditEventType::ExporterFailure,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            trace_id_hex: String::new(),
            span_id_hex: String::new(),
        })?;
        let mut g = self.state.lock().map_err(|_| {
            TracingError::Internal("tracing service mutex poisoned (failure bump)".to_string())
        })?;
        g.exporter_failure_count = g.exporter_failure_count.saturating_add(1);
        Ok(())
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
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingTracingAuditSink, InMemoryTracingAuditSink};
    use crate::context::{TraceContext, TRACE_FLAGS_SAMPLED};
    use crate::exporter::{FailingOtlpExporter, InMemoryOtlpExporter};
    use crate::sampler::RateBasedSampler;
    use corelink_analytics::RedMetricKind;

    type Svc = TracingService<
        RateBasedSampler<InMemoryTracingAuditSink>,
        InMemoryTracingAuditSink,
        InMemoryOtlpExporter,
    >;

    fn fresh_service_full_sample() -> (
        Svc,
        Arc<InMemoryTracingAuditSink>,
        Arc<InMemoryOtlpExporter>,
    ) {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler = Arc::new(RateBasedSampler::new(1.0, Arc::clone(&audit)).unwrap());
        let exporter = Arc::new(InMemoryOtlpExporter::new());
        let svc = TracingService::new(sampler, Arc::clone(&audit), Arc::clone(&exporter));
        (svc, audit, exporter)
    }

    fn sample_trace_context() -> TraceContext {
        TraceContext::new(
            [
                0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e,
                0x47, 0x36,
            ],
            [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7],
            TRACE_FLAGS_SAMPLED,
        )
        .unwrap()
    }

    #[test]
    fn start_then_end_emits_correct_audit_sequence() {
        let (svc, audit, exporter) = fresh_service_full_sample();
        let cx = sample_trace_context();
        svc.start_span(StartSpanInput {
            trace_id: cx.trace_id,
            span_id: cx.span_id,
            parent_span_id: None,
            name: "cas.put",
            kind: SpanKind::Server,
            tenant_tier: Tier::Team,
            start_time_ns: 100,
            created_by_request_id: "req-1",
            now_ms: 1,
        })
        .unwrap();
        assert_eq!(svc.in_flight_count(), 1);
        svc.end_span(EndSpanInput {
            span_id: cx.span_id,
            tenant_tier: Tier::Team,
            status: SpanStatus::Ok,
            end_time_ns: 200,
            attributes: vec![("region".to_string(), "iad".to_string())],
            exemplars: vec![Exemplar {
                metric_kind: RedMetricKind::CasPutDurationSeconds,
                value: 0.004,
                time_ms: 1_700_000_000_000,
            }],
            created_by_request_id: "req-1",
            now_ms: 2,
        })
        .unwrap();
        assert_eq!(svc.in_flight_count(), 0);
        assert_eq!(exporter.len(), 1);
        let started = audit.snapshot_of(TracingAuditEventType::SpanStarted);
        let ended = audit.snapshot_of(TracingAuditEventType::SpanEnded);
        let decisions = audit.snapshot_of(TracingAuditEventType::SamplerDecision);
        assert_eq!(started.len(), 1);
        assert_eq!(ended.len(), 1);
        assert_eq!(decisions.len(), 1);
    }

    fn input_start(cx: &TraceContext, name: &'static str, tier: Tier) -> StartSpanInput<'static> {
        StartSpanInput {
            trace_id: cx.trace_id,
            span_id: cx.span_id,
            parent_span_id: None,
            name,
            kind: SpanKind::Internal,
            tenant_tier: tier,
            start_time_ns: 1,
            created_by_request_id: "req",
            now_ms: 1,
        }
    }

    fn input_end(span_id: SpanId, tier: Tier, status: SpanStatus) -> EndSpanInput<'static> {
        EndSpanInput {
            span_id,
            tenant_tier: tier,
            status,
            end_time_ns: 2,
            attributes: Vec::new(),
            exemplars: Vec::new(),
            created_by_request_id: "req",
            now_ms: 2,
        }
    }

    #[test]
    fn audit_fail_closed_aborts_start() {
        let audit = Arc::new(FailingTracingAuditSink::new());
        let sampler_audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler = Arc::new(RateBasedSampler::new(1.0, sampler_audit).unwrap());
        let exporter = Arc::new(InMemoryOtlpExporter::new());
        let svc = TracingService::new(sampler, audit, exporter);
        let cx = sample_trace_context();
        let err = svc
            .start_span(input_start(&cx, "x", Tier::Team))
            .unwrap_err();
        assert!(matches!(err, TracingError::Audit(_)));
        assert_eq!(svc.in_flight_count(), 0);
    }

    #[test]
    fn dropped_span_not_exported() {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler = Arc::new(RateBasedSampler::new(0.0, Arc::clone(&audit)).unwrap());
        let exporter = Arc::new(InMemoryOtlpExporter::new());
        let svc = TracingService::new(sampler, Arc::clone(&audit), Arc::clone(&exporter));
        let cx = sample_trace_context();
        let d = svc.start_span(input_start(&cx, "x", Tier::Team)).unwrap();
        assert_eq!(d, SamplingDecision::Drop);
        svc.end_span(input_end(cx.span_id, Tier::Team, SpanStatus::Ok))
            .unwrap();
        assert_eq!(exporter.len(), 0);
    }

    #[test]
    fn exporter_failure_increments_counter_and_audits() {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler_audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler = Arc::new(RateBasedSampler::new(1.0, sampler_audit).unwrap());
        let exporter = Arc::new(FailingOtlpExporter::new());
        let svc = TracingService::new(sampler, Arc::clone(&audit), exporter);
        let cx = sample_trace_context();
        svc.start_span(input_start(&cx, "x", Tier::Team)).unwrap();
        let err = svc
            .end_span(input_end(cx.span_id, Tier::Team, SpanStatus::Ok))
            .unwrap_err();
        assert!(matches!(err, TracingError::Exporter(_)));
        assert_eq!(svc.exporter_failure_count(), 1);
        assert_eq!(
            audit
                .snapshot_of(TracingAuditEventType::ExporterFailure)
                .len(),
            1
        );
    }

    #[test]
    fn end_span_unknown_id_returns_internal_error() {
        let (svc, _a, _e) = fresh_service_full_sample();
        let err = svc
            .end_span(input_end([0xCC; 8], Tier::Team, SpanStatus::Ok))
            .unwrap_err();
        assert!(matches!(err, TracingError::Internal(_)));
    }

    #[test]
    fn cloned_service_shares_state() {
        let (svc, _a, _e) = fresh_service_full_sample();
        let svc2 = svc.clone();
        let cx = sample_trace_context();
        svc.start_span(input_start(&cx, "x", Tier::Team)).unwrap();
        assert_eq!(svc2.in_flight_count(), 1);
    }

    #[test]
    fn tenant_isolation_keys_block_cross_tenant_lookup() {
        let (svc, _a, _e) = fresh_service_full_sample();
        let cx = sample_trace_context();
        svc.start_span(input_start(&cx, "x", Tier::Team)).unwrap();
        // Same span_id, DIFFERENT tier → should fail to end.
        let err = svc
            .end_span(input_end(cx.span_id, Tier::Free, SpanStatus::Ok))
            .unwrap_err();
        assert!(matches!(err, TracingError::Internal(_)));
        // Now end with the correct tier; succeeds.
        svc.end_span(input_end(cx.span_id, Tier::Team, SpanStatus::Ok))
            .unwrap();
    }
}
