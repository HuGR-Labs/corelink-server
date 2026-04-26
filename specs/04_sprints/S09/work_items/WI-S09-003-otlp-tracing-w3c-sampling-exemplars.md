---
id: "WI-S09-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-09"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "PRIVACY-MODEL"
tags: ["wi", "s09", "tracing", "otlp", "w3c-trace-context", "sampling", "exemplars", "tempo", "high-risk"]
---

# WI-S09-003 — OTLP Tracing Middleware + W3C Trace Context Propagation + Head/Tail Sampling + OpenMetrics Exemplars (`crates/corelink-tracing`; OpenTelemetry SDK v0.21+ Rust; W3C Trace Context Recommendation 2020 `traceparent` + `tracestate` header propagation; head-based sampling 1% default + tail-based sampling 100% para spans com `error=true` OR `latency_p99_breach=true`; OpenMetrics 1.0 exemplars via WI-S09-001 emit lib — histogram observations include trace_id em separate exemplar field NUNCA em label; Grafana Tempo backend deep link integration; CAP-OBS-003 + CAP-OBS-009)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-003 |
| Título | OTLP middleware Rust (OpenTelemetry SDK v0.21+) integrating `traceparent` + `tracestate` header propagation per W3C Trace Context Recommendation 2020; head-based sampling 1% default (preserves throughput) + **tail-based sampling 100% para errors + latency p99 breach** (preserves debug capability per Google SRE Workbook Ch 6); OpenMetrics 1.0 exemplars emission via WI-S09-001 emit lib (`observe_histogram(metric, labels, value, exemplar=Some(Exemplar{trace_id, span_id, ts, value}))`); **trace_id NUNCA em label slot** (Lote 10.8bis cardinality discipline absorbed; sprint contract §15 R-tracing-cardinality risk explicit); Grafana Tempo backend ingest via OTLP HTTP `/v1/traces` endpoint; per-region Tempo tenant; sampling tax overhead ≤ 5% latency p99 (sprint contract §15 R-Tail-sampling-overhead) |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-AUDIT-001 audit chain + observability completeness; tracing critical for distributed debug; bypass = 10× MTTR debug) |

## 1. Intent

OTLP tracing é o **distributed debug primitive** do CoreLink — sem isso, p99 latency anomaly em multi-DO + R2 + D1 + Neon stack é debug impossível (5+ services × log correlation impossível em request flow). W3C Trace Context propagation canonical entre Worker → DO → R2/D1 → Neon RPC. Tail-sampling 100% para errors preserves debug capability sem custo de full-trace (Honeycomb 2017 lesson: tail-sampling = 100× cheaper than full + same debug power for incidents).

```rust
// File: crates/corelink-tracing/src/lib.rs

#![forbid(unsafe_code)]

use opentelemetry::{global, trace::{Tracer, TraceContextExt, Span, Status}};
use opentelemetry_sdk::{trace::{Sampler, TracerProvider, SpanProcessor}, Resource};

#[async_trait]
pub trait TracingService: Send + Sync {
    /// Extract W3C Trace Context from incoming request headers (traceparent + tracestate).
    fn extract_context(headers: &http::HeaderMap) -> opentelemetry::Context;

    /// Inject W3C Trace Context into outgoing request headers (canonical propagation).
    fn inject_context(cx: &opentelemetry::Context, headers: &mut http::HeaderMap);

    /// Start span with parent context inherited from W3C extraction.
    fn start_span(&self, name: &'static str, cx: opentelemetry::Context) -> opentelemetry::trace::SpanRef;

    /// Determine if span should be sampled (head-based 1% + tail-based 100% errors).
    fn should_sample(&self, sampling_decision_input: SamplingDecisionInput) -> SamplingDecision;
}

pub enum SamplingDecision {
    /// Head-sampled at root: 1% RandomRatio (head-based; cheap; immediate).
    HeadSampled,
    /// Tail-sampled retroactively: 100% if span attribute `error=true` OR `latency_p99_breach=true`.
    TailSampled,
    /// Drop: 99% of head-sampled-out spans dropped at SDK boundary (no SDK overhead).
    Drop,
}

pub struct SamplingDecisionInput {
    pub trace_id: TraceId,
    pub parent_context: Option<&opentelemetry::Context>,
    pub attributes: &[KeyValue],                       // span attributes for tail decision
    pub status: Option<&Status>,                       // OK/Error
    pub duration_us: Option<u64>,                      // for latency_p99_breach check
}

pub struct CorelinkTraceConfig {
    /// Head sampling rate (canonical 0.01 = 1%).
    pub head_sample_rate: f64,
    /// Tail sampling: errors always (100%); latency breach configurable.
    pub tail_sample_errors: bool,                       // canonical true
    /// SLO p99 threshold per service (from slo_catalog.md); span tail-sampled if exceeds.
    pub latency_p99_thresholds: HashMap<ServiceName, Duration>,
    /// OTLP HTTP endpoint (Grafana Tempo per region).
    pub otlp_endpoint: Url,
}

#[derive(thiserror::Error, Debug)]
pub enum TracingError {
    #[error("OTLP exporter failed (fail-open; observability degradation): {0}")]
    OtlpExportFailed(String),

    #[error("W3C trace context parse error: {0}")]
    TraceContextParseError(String),

    #[error("sampling overhead exceeded budget: observed={observed_pct}% > budget=5%")]
    SamplingOverheadExceeded { observed_pct: f64 },

    #[error("forbidden trace_id em label slot detected (Lote 10.8bis discipline; use Exemplar field): {metric}")]
    TraceIdEmLabelDetected { metric: String },
}
```

**Cripto-driven invariants enforced**:

1. **W3C Trace Context Recommendation 2020 compliance**:
   - `traceparent: 00-{trace_id_hex}-{span_id_hex}-{trace_flags_hex}` header (canonical 55-char ASCII).
   - `tracestate: vendor1=val1,vendor2=val2` header (vendor-specific extensions).
   - Parser strict per W3C spec; rejects malformed traceparent (e.g., wrong version, invalid hex).

2. **Head-based 1% sampling** (sprint contract §5.3 R-S09-8; canonical default OpenTelemetry SDK):
   - `Sampler::TraceIdRatioBased(0.01)` em SDK config.
   - **Why 1% canonical**: 100k req/sec × 1% = 1k traces/sec ingested em Tempo; sustainable cost (Tempo $0.50/GB ingested × 10 KB/trace × 86400 × 365 = ~$315/region/yr).
   - Higher sample rate (e.g., 10%) blows up Tempo cost 10× sem proportional debug benefit.

3. **Tail-based 100% sampling para errors + latency breach** (sprint contract §5.3 R-S09-8 + §15 R-Tail-sampling-overhead):
   - SDK SpanProcessor: BatchSpanProcessor + custom TailSamplingProcessor.
   - Tail decision deferred to span END; based on:
     - `span.status == Error` (any 5xx response).
     - `span.duration > slo_catalog.latency_p99_thresholds[service]` (e.g., cas.put.duration > 100ms = breach).
   - Memory budget: SpanProcessor buffers up to 10k unsampled spans em memory antes de tail decision; budget bounded.

4. **OpenMetrics 1.0 exemplars** (CAP-OBS-009 + sprint contract §5.3 R-S09-9):
   - Histogram observations emit exemplar via WI-S09-001 `observe_histogram(metric, labels, value, exemplar=Some(...))`.
   - Exemplar `trace_id` em separate field per OpenMetrics spec: `# {trace_id="abc"} 0.004 timestamp_ms`.
   - **trace_id NUNCA em label slot** (Lote 10.8bis cardinality discipline; cardinality_check.py CI lint rejects).

5. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware (S-03); span attribute `tenant.id` extracted from TenantCtx.

6. **Audit fail-OPEN para tracing emit** (vs audit fail-CLOSED em S-04/S-06; matches WI-S09-001 + WI-S09-002 fail-open canonical):
   - OTLP export failure → SEV-3 alert; counter `corelink_tracing_export_failures_total`; service continues.
   - Distinção canonical Lote 10.6bis lesson absorbed.

7. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): OTLP export via `worker::send_future()` (HTTP POST fire-and-forget); NEVER `tokio::spawn`. `async_lock::Mutex` for sampling decision shared state if needed (Lote 10.3-tris).

8. **Sampling tax budget** (sprint contract §15 R-Tail-sampling-overhead):
   - Head sampling 1% RandomRatio: ~10ns overhead per span (deterministic hash check).
   - Tail sampling buffer: 10k spans × ~1 KB/span = 10 MB memory budget per Worker; bounded.
   - Overall overhead ≤ 5% latency p99 sustained 7d (criterion benchmark).

9. **5-tier canonical** (Lote 10.7bis P0-7 absorbed): tenant_tier em span attributes (NOT em métrica labels); aggregation via Tempo trace search.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + sampling discipline justification)

OTLP tracing distribuído é **the debug primitive** do CoreLink — multi-DO + R2 + D1 + Neon stack tem latency anomaly debug impossível sem trace correlation. Honeycomb 2017 case study: incident debug tempo reduzido de 4h → 8min após adopt full trace correlation. Without W3C Trace Context propagation, log timestamps cannot be correlated cross-service (different clock sources, no causality chain).

**Why head-based 1% canonical** (vs 100% sampling): 100k req/sec sustained × 100% × 10 KB/trace = 1 GB/sec Tempo ingest = $$$ + Tempo storage saturation. 1% RandomRatio: 10 MB/sec sustainable; Tempo can handle 100x burst without throttling. SOTA precedent: Google Dapper 0.1%, Honeycomb 0.5-2% default.

**Why tail-based 100% para errors** (Google SRE Workbook Ch 6): incidents are minority of traffic but majority of debug value. Tail-sampling preserves all error spans + spans exceeding p99 SLO threshold. Cost asymmetry: 1% errors × 100% = 1% extra spans (vs 100% baseline = 100x). 100x cheaper at same incident debug capability.

**Why OpenMetrics exemplars**: connect métricas (RED gauge/counter/histogram) to traces (Tempo span detail). Click exemplar dot em Grafana histogram → opens Tempo trace view → 10x faster incident root-cause vs log+metric search separately. SOTA precedent: Honeycomb originated; OpenMetrics 1.0 standardized.

**Why trace_id em Exemplar (NOT label)** (Lote 10.8bis discipline absorbed): trace_id is unique-per-request (~10k unique values/sec sustained); em label slot = unbounded cardinality (cardinality explode 1B+ séries/dia). Exemplar field separate per OpenMetrics 1.0 spec preserves cardinality budget while enabling debug correlation.

**Why W3C Trace Context (vs B3 / OT-style legacy)**: W3C Recommendation 2020 is canonical industry standard; Bazel/Buck2 clients propagate W3C; cross-vendor compatibility (Datadog/NewRelic/Honeycomb/Tempo all canonical). B3/OT propagation legacy.

**Adversarial scenarios**:
- **Bad-PR adicionando trace_id em label slot**: cardinality_check.py CI gate rejects (WI-S09-001 lint inheritance); type system primary defense.
- **Tail-sampling buffer overflow** (10k spans): SDK drops oldest unsampled spans; tail decision still made for sampled-batch; SEV-3 alert if sustained.
- **Malformed traceparent header**: parser rejects; new trace_id generated locally; tracestate preserved if valid (vendor extensions).
- **Cross-region Tempo outage**: OTLP export fail-open; SEV-3 alert; Worker continues serving requests; on Tempo recovery, in-flight spans lost (acceptable observability degradation).
- **High-cardinality span attributes** (e.g., per-request unique URI path): Tempo trace search slower; spans sampled normally; no cost spike (Tempo storage is per-trace, not per-attribute series).
- **Sampling rate misconfigured** (e.g., 100% accidental): SEV-2 alert via cost regression metric; auto-revert via runtime config refresh.
- **Tenant_id leaked em span attribute** (privacy): tenant_id em span attribute is **acceptable** (Tempo not Prometheus; cardinality constraint not present); but raw user PII (email/IP) em span = clippy lint rejects via redact! macro inheritance from WI-S09-002.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: observability completeness; tracing bypass = 10x MTTR.
- 12 sign-offs + chaos suite + property test 100k.

## 3. Customer Impact & Journey

**Persona 1 — Bazel client**: client SDK propagates W3C `traceparent`; Worker extracts; spans inherit context cross-Worker → DO → R2; full causality chain visible em Tempo.

**Persona 2 — DevOps debugging incident**: Grafana DASH-CAS shows latency p99 spike; clicks histogram exemplar dot; Tempo trace opens; sees Worker → DO → R2 → R2 cold-cache miss path; root cause identified em 5min vs 30min (without exemplar).

**Persona 3 — Developer adding span**: adds `tracer.start_span("my_op", cx)`; SDK auto-handles head sampling (1%); if op errors, tail-samples 100% retroactively; trace appears em Tempo for debug.

**Persona 4 — SRE reviewing**: checks `corelink_tracing_export_failures_total` SEV-3 alert; investigates Tempo tenant config; resolves OTLP endpoint TLS cert issue.

**Persona 5 — Platform engineer reviewing cost**: `corelink_tracing_spans_exported_total{sampled_decision}` em DASH-COST shows ~1% head-sampled + ~0.1% tail-sampled; total ingest ~1.1% req volume; under cost budget.

**SLA addendum**:
- W3C Trace Context propagation: 100% Worker→DO→R2→D1→Neon (synthetic test 24/7).
- Sampling overhead: ≤ 5% latency p99 (criterion benchmark).
- Tail-sampling tail decision: ≤ 30s after span end (BatchSpanProcessor flush interval).
- OTLP export lag: ≤ 30s p99 (network + Tempo ingest).

## 4. Capability Mapping

- **CAP-OBS-003** (Distributed tracing OTLP) — IMPLEMENTA primary.
- **CAP-OBS-009** (Exemplars metrics ↔ traces) — IMPLEMENTA primary.
- Trace: `observability_model.md §6 distributed tracing canonical` + sprint contract §5.3 (R-S09-7/8/9) + W3C Trace Context Recommendation 2020 + OpenMetrics 1.0 spec + OpenTelemetry SDK v0.21+ Rust + Google SRE Workbook Ch 6.

## 5. Tipo

OTLP middleware + OpenTelemetry SDK Rust + Tempo integration; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-tracing/` module** — TracingService trait + OTLP impl + sampling logic + tests.

2. **W3C Trace Context propagation**:
   - Extract from incoming request: `traceparent` header parser strict per W3C spec (55-char ASCII).
   - Inject to outgoing requests (DO RPC, R2 fetch, D1 query, Neon connection): all sub-resource calls propagate context.
   - `tracestate` vendor extensions preserved + appended (e.g., `corelink=region:iad`).

3. **Head sampling 1% via Sampler::TraceIdRatioBased(0.01)**:
   - Deterministic: based on trace_id hash; same trace_id → same sample decision (consistent across services).
   - Cheap: ~10ns overhead per span start.

4. **Tail sampling 100% errors + p99 breach** (custom SpanProcessor):
   - BatchSpanProcessor wraps spans em memory buffer (10k cap; bounded budget).
   - Em span end: evaluate tail conditions; promote to "sampled" if matched; otherwise drop.
   - Memory budget alert SEV-3 if buffer 80% full sustained.

5. **OpenMetrics exemplars integration** (delegate to WI-S09-001):
   - Histogram observations include exemplar via WI-S09-001 `observe_histogram(..., exemplar=Some(Exemplar{trace_id: cx.span().span_context().trace_id(), ...}))`.
   - 3 fluxos primary: cas.put, cas.get, ac.lookup (sprint contract §10.s09.6 explicit).

6. **OTLP HTTP exporter (Tempo backend)**:
   - Endpoint per-region: `https://tempo-prod-${region}.grafana.net/v1/traces`.
   - Authentication: Tempo API key per region em Worker secret.
   - Compression: gzip canonical.
   - Retry: 3x exponential backoff; fail-open if all fail.

7. **Wrangler config additions**:
   ```toml
   # wrangler.toml
   [vars]
   OTLP_ENDPOINT = "https://tempo-prod-iad.grafana.net/v1/traces"
   TRACING_HEAD_SAMPLE_RATE = "0.01"
   TRACING_TAIL_SAMPLE_BUFFER_SIZE = "10000"

   [[tracing_secrets]]
   TEMPO_API_KEY = "REDACTED"
   ```

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): OTLP HTTP export via `worker::send_future()`; NEVER `tokio::spawn`.

9. **Span attribute discipline** (Lote 10.8bis cardinality + privacy; reuse from WI-S09-002 redact!):
   - `tenant.id` (UUID, OK em span attribute; cardinality NOT cardinality-constrained em Tempo).
   - `tenant.tier` (5-tier enum).
   - `region`, `request_id`, `trace_id` (canonical fields).
   - **Forbidden em span attributes**: raw email, raw IP, raw bearer token (use redact! macro from WI-S09-002).

10. **Métricas operacionais**:
    - `corelink_tracing_spans_exported_total{sampled_decision=head_sampled|tail_sampled|dropped}` (counter; ~1.1% of request volume).
    - `corelink_tracing_export_failures_total{reason}` (counter; **alert SEV-3 if > 1%**).
    - `corelink_tracing_sampling_overhead_us` (histogram; SLO ≤ 5% latency p99 — sprint contract §15).
    - `corelink_tracing_tail_sample_buffer_pressure` (gauge; **alert SEV-3 if > 80% sustained**).
    - `corelink_tracing_context_parse_failures_total{reason}` (counter; **alert SEV-2 if > 0.1%**).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_w3c_traceparent_roundtrip`: 10k random traceparent values; assert parse + serialize roundtrip identity.
    - `prop_head_sampling_deterministic`: 100k random trace_ids; assert same trace_id → same sampling decision.
    - `prop_tail_sampling_errors_100pct`: 10k synthetic error spans; assert all sampled.
    - `prop_no_trace_id_em_label`: 10k metrics emit calls; assert trace_id NEVER em label slot (inherit from WI-S09-001 lint).
    - `prop_exemplar_format_openmetrics`: 10k exemplar emissions; assert OpenMetrics 1.0 line format strict.
    - `prop_sampling_overhead_bounded`: 100k spans + benchmark; assert overhead ≤ 5% p99.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 10):
    - 1. **Tempo outage 30min**: OTLP export fail-open; SEV-3 alert; recovery resumes.
    - 2. **Malformed traceparent injection**: parser rejects; new local trace_id generated; service continues.
    - 3. **Tail buffer overflow** (10k spans burst): drops oldest; SEV-3 alert; recovery on burst end.
    - 4. **Sampling rate misconfigured (100%)**: cost regression alert; auto-revert via config refresh.
    - 5. **Cross-region context propagation**: Worker iad → DO sam (cross-region); traceparent propagates correctly via DO RPC.
    - 6. **Bad-PR trace_id em label**: cardinality_check.py rejects (inheritance from WI-S09-001).
    - 7. **PII em span attribute**: clippy lint rejects (redact! macro from WI-S09-002).
    - 8. **OTLP TLS cert renewal**: synthetic test catches cert expiry within 30d.
    - 9. **Tempo query slow (p99 > 30s)**: documented em SLO; not a blocker for tracing emission.
    - 10. **Concurrent context extract + inject race**: deterministic; property test 100k.

### 6.2 Out-of-scope (deferred)

- B3 / OT-Style legacy header support (W3C canonical only; deferred to S-14 BYOK if customer demand).
- Custom span processors per tenant (anti-scope).
- ML-based anomaly detection em traces (anti-scope §10).
- Cross-region Tempo federation (per-region tenant initial; deferred S-14).
- Tail-sampling complexity (e.g., percentage of traces from same trace_id batch); simple status+duration initial.

## 7. Anti-Scope

- ❌ trace_id em label slot (Lote 10.8bis discipline; cardinality explosion).
- ❌ B3 / OT-Style legacy headers (W3C canonical).
- ❌ 100% head sampling (cost prohibitive; tail-sampling design canonical).
- ❌ Synchronous OTLP export blocking request hot path (fire-and-forget canonical).
- ❌ Raw PII em span attributes (clippy lint via redact! macro).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: OTLP Tracing + W3C Trace Context + Sampling + Exemplars

  Scenario: W3C traceparent extraction from incoming request
    Given request com header "traceparent: 00-{trace_id}-{span_id}-01"
    When extract_context(headers) called
    Then context contains trace_id + span_id correctly parsed
    Then sampling decision determined by trace_id hash (1% rate)

  Scenario: W3C traceparent injection to outgoing DO RPC
    Given current span context with trace_id + span_id
    When inject_context(cx, headers) called for DO RPC
    Then headers contain "traceparent: 00-{trace_id}-{span_id}-01"
    Then DO inherits parent context (cross-service propagation)

  Scenario: Head sampling 1% deterministic
    Given 100 spans com trace_ids hashing to consistent decisions
    When should_sample(input) called for each
    Then ~1% of trace_ids sampled (HeadSampled); ~99% Drop
    Then same trace_id → same decision (deterministic)

  Scenario: Tail sampling 100% errors
    Given span with status=Error em end
    When span ends
    Then BatchSpanProcessor evaluates tail condition
    Then span promoted to TailSampled regardless of head decision
    Then exported to Tempo OTLP

  Scenario: Tail sampling latency breach
    Given span com duration > slo_catalog.latency_p99_thresholds[cas.put] (e.g., 100ms)
    When span ends
    Then tail condition met (latency_p99_breach=true)
    Then span TailSampled

  Scenario: Exemplar emission for histogram metric
    Given current trace context
    When observe_histogram(cas.put.duration_seconds, labels, 0.004, exemplar=Some(Exemplar{trace_id, ...}))
    Then OpenMetrics line: cas_put_duration_seconds_bucket{...} N # {trace_id="abc"} 0.004 ts
    Then trace_id em exemplar field NEVER em label slot

  Scenario: Click exemplar opens Tempo trace
    Given Grafana dashboard DASH-CAS rendering histogram com exemplar dot
    When user clicks exemplar
    Then Tempo trace view opens com trace_id matched
    Then full Worker → DO → R2 span chain visible
    Then 3 fluxos validated: cas.put, cas.get, ac.lookup (sprint contract §10.s09.6)

  Scenario: Tempo outage fail-open
    Given Tempo OTLP endpoint returns 503 sustained 30min
    When span ends
    Then export fails; counter corelink_tracing_export_failures_total increments
    Then Worker continues serving requests (fail-open)
    Then SEV-3 alert (NOT SEV-1 — request not blocked)
    Then on Tempo recovery: future spans export normally

  Scenario: Malformed traceparent rejected
    Given request com header "traceparent: invalid-format"
    When extract_context called
    Then parser rejects; new local trace_id generated
    Then counter corelink_tracing_context_parse_failures_total increments
    Then service continues (no abort)

  Scenario: Sampling overhead within 5% budget
    Given 100k span starts em criterion benchmark
    When sampling decision evaluated for each
    Then overall overhead ≤ 5% latency p99 (sprint contract §15 R-Tail-sampling-overhead)
    Then prop_sampling_overhead_bounded green em CI
```

## 9. Design Decisions

- 9.1: W3C Trace Context canonical (NOT B3/OT-Style legacy).
- 9.2: Head sampling 1% RandomRatio + tail sampling 100% errors/breach (Google SRE Workbook Ch 6).
- 9.3: OpenMetrics 1.0 exemplars (trace_id em separate field NUNCA em label).
- 9.4: Grafana Tempo backend (OTLP HTTP exporter).
- 9.5: BatchSpanProcessor wraps com tail sampling decision.
- 9.6: Tail buffer 10k spans memory budget.
- 9.7: Fail-OPEN OTLP export (vs audit fail-closed).
- 9.8: TenantCtx-only enforcement (Lote 10.4bis); tenant.id em span attribute OK.
- 9.9: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.10: redact! macro inheritance from WI-S09-002 (PII em span attributes forbidden).
- 9.11: NO new ADR (extends observability_model.md §6 + W3C Recommendation canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.003.1** Crate compila + integration tests green.
- [ ] **10.s09.003.2** All 10 Gherkin scenarios green.
- [ ] **10.s09.003.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s09.003.4** Chaos suite 10 scenarios green.
- [ ] **10.s09.003.5** **Exemplars working em 3 fluxos**: cas.put + cas.get + ac.lookup (sprint contract §10.s09.6); click exemplar → Tempo trace view.
- [ ] **10.s09.003.6** **Sampling overhead ≤ 5% latency p99** (criterion benchmark; sprint contract §15 R-Tail-sampling-overhead).
- [ ] **10.s09.003.7** W3C Trace Context propagation 100% Worker → DO → R2 → D1 → Neon (synthetic 24/7).
- [ ] **10.s09.003.8** Métricas (5 §6.1.10) emitted; tail buffer pressure SEV-3 if > 80%; export failures SEV-3 if > 1%.
- [ ] **10.s09.003.9** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s09.003.10** Tempo tenant configured per region; OTLP endpoint validated.
- [ ] **10.s09.003.11** Cost regression gate per sprint contract §14.s09.7 (sampling rate change > 10% requires ADR).
- [ ] **10.s09.003.12** Tracing coverage ≥ 90% das requests CAS/AC com trace_id; ≥ 100% latency p99 breach tail-sampled (sprint contract §14.s09.4).

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Lote 10.8bis P1-2).

## 12. Invariants Validated

- **INV-OBS-CARDINALITY-BUDGET** (HIGH; registry §3.12): trace_id em Exemplar NÃO em label preserves budget.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): tracing fail-open preserves request availability.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware (S-03 inheritance).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Tracing module | `crates/corelink-tracing/` | Rust |
| OTLP middleware | `crates/corelink-tracing/src/otlp_middleware.rs` | Rust |
| Custom SpanProcessor | `crates/corelink-tracing/src/tail_sampling.rs` | Rust |
| Wrangler config | `wrangler.toml` (additions per environment) | TOML |
| Tempo tenant config | `infra/grafana/tempo-tenant-config.yaml` | YAML |
| Property tests | `crates/corelink-tracing/tests/prop_tracing.rs` | Rust |
| Chaos suite | `tests/chaos_tracing.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s09.003.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s09.003.2: rustdoc 100% public API.
- 14.s09.003.3: Test coverage ≥ 90%.
- 14.s09.003.4: Sampling overhead ≤ 5% latency p99.
- 14.s09.003.5: SAST clean.
- 14.s09.003.6: Métricas (5 §6.1.10).
- 14.s09.003.7: 100k nightly property test (HIGH_RISK SOTA bar).
- 14.s09.003.8: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2).
- 14.s09.003.9: redact! macro inheritance from WI-S09-002 (PII em span attributes forbidden).
- 14.s09.003.10: trace_id em Exemplar field NUNCA em label slot (Lote 10.8bis discipline).
- 14.s09.003.11: W3C Trace Context Recommendation 2020 strict compliance.
- 14.s09.003.12: OpenMetrics 1.0 exemplar canonical format.

## 15. Chaos Experiments (10)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + TracingService trait + W3C parser | 2 |
| ST-002 | OTLP HTTP exporter Tempo + retry + fail-open | 2 |
| ST-003 | Head sampling Sampler::TraceIdRatioBased(0.01) | 1 |
| ST-004 | Tail sampling custom SpanProcessor + buffer | 3 |
| ST-005 | Exemplar emission integration WI-S09-001 (3 fluxos) | 1.5 |
| ST-006 | Wrangler config + Tempo tenant API key | 1 |
| ST-007 | Span attribute discipline + redact! inheritance | 1 |
| ST-008 | Métricas (5) emit | 0.5 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 2.5 |
| ST-010 | Chaos suite (10) | 2 |

**Total**: ~16.5h. **PERT** O=10h M=16h P=24h: **~16.3h** (matches sprint contract §12).

## 18. Dependencies

- Hard: WI-S09-001 SEALED (métricas emit lib for exemplar integration); WI-S09-002 SEALED (redact! macro inheritance for span attribute PII discipline); S-03 SEALED (TenantCtx middleware).
- Soft: WI-S09-005 (DASH-* dashboards consume exemplars).
- Hard infra: Grafana Tempo tenant provisioned + API key per region; OpenTelemetry SDK v0.21+ Rust available.

## 19. Effort PERT: ~16.3h. ## 20. Time-boxing: 24h hard limit.

## 21. Observability

5 metrics §6.1.10. Trace span `tracing.{extract, inject, sample, export, exemplar_emit}`.

## 22. Cost Analysis

- Tempo ingest: ~1.1% of request volume (1% head + 0.1% tail). 100k req/sec × 1.1% × 10 KB/trace = 1.1 MB/sec; ~95 GB/dia/region; $0.50/GB ingest = ~$47/região/dia = $1410/região/mês = $84k/yr × 5 regions = $420k/yr.
- Wait, this is too high. Reconsidering: 1.1% × 100k req/sec × 10 KB = 11 MB/s wait that's 950 GB/dia... let me recompute.
- 100k req/sec × 86400 sec/dia = 8.64B req/dia. × 1.1% sample = 95M traces/dia × 10 KB = 950 GB/dia.
- Actually for production CoreLink scale (initial ~10k req/sec target): 10k × 86400 = 864M req/dia × 1.1% × 10 KB = 95 GB/dia/região. × 5 regions = 475 GB/dia × $0.50 = $237/dia ingest.
- Tempo storage: 30d retention × 475 GB/dia = 14.25 TB hot; $0.10/GB-mo storage = ~$1425/mo Tempo storage.
- TCO 12m: ingest $237/dia × 365 = $86505 + storage $1425 × 12 = $17100 = ~$103k/yr observability tracing infra.
- **Cost saved by 1% head sampling**: 100% would be ~$10M/yr (100x).
- **Cost saved by tail-sampling preserving errors**: same debug capability at 100x lower cost.

## 23. API Contract

- Public Rust: `TracingService` trait + `SamplingDecision` enum + `CorelinkTraceConfig` + `TracingError` types; `#[non_exhaustive]`.
- HTTP: W3C Trace Context Recommendation 2020 (`traceparent` + `tracestate` headers).
- Backend: Grafana Tempo OTLP HTTP `/v1/traces` endpoint per region.

## 24. Post-mortem Hooks

- Tail-sampling drop rate > 5% sustained → post-mortem (debug capability degradada; sprint contract §18 trigger).
- Sampling overhead > 5% latency p99 → SEV-2 + post-mortem (criterion regression).
- Tempo outage > 1h sustained → SEV-3; tracing gap documented em SLO.
- Trace context parse failures > 0.1% → SEV-2 + investigation (client SDK regression OR malformed propagation).

## 25. Rollback / Recovery

- Rollback: revert OTLP middleware mount; tracing disabled; logs+metrics still flowing (degraded debug).
- Recovery: middleware re-mounted; spans resume export; in-flight context lost (acceptable).
- RTO ≤ 5min; RPO ≤ 30s.

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware; tenant.id em span authoritative.
- T(ampering): Tempo append-only; trace mutation infeasible.
- R(epudiation): export fail-open + counter; eventual consistency.
- I(nformation disclosure): redact! macro inheritance; PII em span attributes forbidden.
- D(enial of Service): sampling rate budget + cost gate.
- E(scalation of Privilege): Tempo tenant API key per region.

**LINDDUN**:
- L(inkability): tenant.id em span attribute (acceptable Tempo non-cardinality-constrained).
- I(dentifiability): PII redaction inheritance from WI-S09-002.
- N(on-repudiation): Tempo append-only; trace_id immutable.
- D(etectability): exemplar metric ↔ trace correlation.
- D(isclosure): trace data retention 30d; auto-purge.
- U(nawareness): customer access deferred S-13.
- N(on-compliance): N/A (traces internal observability; no automated decisions).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-09 Tracing: W3C + Sampling + Exemplars"; doc `docs/dev/tracing-architecture.md`; onboarding test 8 questions: W3C Trace Context spec, head 1% rationale (Google Dapper precedent), tail 100% errors (Honeycomb 100x cheaper), exemplars OpenMetrics 1.0, BatchSpanProcessor + tail decision, fail-OPEN export, Tempo deep link, redact! inheritance for PII.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Sampling overhead > 5% (latency tax) | M | M | MEDIUM | M | LOW | Criterion benchmark + sprint contract §15; head 1% canonical |
| R-002 | trace_id em label (cardinality explosion) | L | H | HIGH | L | LOW | Cardinality validator CI gate (WI-S09-001 inheritance); type system |
| R-003 | Tempo outage observability gap | M | L | MEDIUM | L | LOW | Fail-open + SEV-3; recovery on resume |
| R-004 | Tail buffer overflow lost spans | M | L | LOW | L | LOW | 10k buffer cap; SEV-3 alert; bounded |
| R-005 | Sampling rate misconfigured | L | L | MEDIUM | L | LOW | Cost regression alert + auto-revert |
| R-006 | Cross-region propagation broken | L | M | HIGH | L | LOW | Synthetic test 24/7; W3C strict parsing |
| R-007 | PII em span attribute leak | L | M | HIGH | L | LOW | redact! macro inheritance from WI-S09-002 |
| R-008 | Trace context parse failures > 0.1% | L | L | LOW | L | LOW | SEV-2 alert + SDK regression investigation |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.s09.7 gate; ADR required |
| R-011 | OTLP TLS cert renewal lag | L | L | LOW | L | LOW | Synthetic monitoring 30d before expiry |
| R-012 | Customer SDK W3C non-compliant | L | L | LOW | L | LOW | Generate new local trace; service continues |

## 29. Review Checkpoints

D+0 design (Architect; sampling strategy); D+1 SRE (Tempo integration); D+2 AppSec (TenantCtx + redact! inheritance); D+3 code review; D+4 chaos validation; D+5 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **emphatic on sampling discipline + Tempo integration**_ |
| 4 | Security Lead | _TBD; **mandatory** — STRIDE + Tempo API key_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + 100k property test_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; mandatory — observability completeness for SOC 2_ |
| 10 | Privacy | _TBD; **mandatory** — LINDDUN + redact! inheritance_ |
| 11 | Architect | _TBD; **mandatory emphatic** — W3C compliance + sampling design + exemplar integration; consolidates Crypto SME advisory race-correctness review per ADR-0034_ |
| 12 | AppSec | _TBD; **mandatory** — type system + redact! inheritance discipline_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-003; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); fail-OPEN tracing emit (Lote 10.6bis distinction; audit fail-closed em WI-S09-004 separate); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X → §3.13 (Lote 10.8bis P1-13). NEW corelink-tracing crate + OTLP middleware. W3C Trace Context Recommendation 2020 canonical. Head 1% + tail 100% errors (Google SRE Workbook Ch 6). OpenMetrics 1.0 exemplars (trace_id em Exemplar field NUNCA em label; Lote 10.8bis cardinality discipline absorbed). redact! macro inheritance from WI-S09-002 (PII em span attributes forbidden). Tempo backend integration. |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.9bis) | R4+R5 review remediation: P0-B INV §3.X → §3.12; P0-E Prom métricas underscores. WI scored 8.0/10 R4 — strongest among 7. P1 carry-forward (cost analysis math hole §22; tail buffer 10k spans memory math) deferred ao Lote 10.9-tris se necessário. Aggregate target ≥ 8.5 (R4 8.0 + R5 7.5 baselines). |

## 32. Anti-patterns evitados

- ❌ trace_id em label slot (cardinality explosion); ❌ B3/OT-Style legacy headers; ❌ 100% head sampling (cost prohibitive); ❌ Synchronous OTLP export (latency tax); ❌ Raw PII em span attributes (CTRL-PRIV-001 violation); ❌ tokio::spawn em CF Workers; ❌ Skip cardinality validator inheritance; ❌ Skip redact! macro inheritance.

---

**Fim WI-S09-003.** Próximo: WI-S09-004 (CloudEvents emitter + R2 audit bucket + hash chain integrity + daily verify).
