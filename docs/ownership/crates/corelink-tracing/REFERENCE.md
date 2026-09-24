---
schema: corelink-ownership/1.1
document: reference
package: corelink-tracing
manifest: crates/corelink-tracing/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: tracing-static-source-20260920
---

# corelink-tracing — ownership reference

Static-source reference for the tracing crate. SOURCE is not proof that a path ran, that telemetry was delivered, or that any OTLP/Tempo endpoint exists. `WAVE_008_PLAN` is routing context only; this record neither revalidates nor redefines canonical OKF material.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Context](#r04) · [Sampling](#r05) · [Lifecycle](#r06) · [Fixtures](#r07) · [Closure](#r08) · [Targets/config](#r09).

<a id="r01"></a>
## R01 — Package identity invariant

Falsifiable invariant: the manifest identifies this package as `corelink-tracing` and declares `corelink-analytics` as its direct internal dependency. Evidence: `crates/corelink-tracing/Cargo.toml`. A manifest edit that changes either fact invalidates this record.

<a id="r02"></a>
## R02 — Owned module boundary invariant

Falsifiable invariant: `src/lib.rs` publicly exposes `audit`, `context`, `error`, `exporter`, `sampler`, `service`, and `span`. Evidence: those module declarations and re-exports in `src/lib.rs`. It does not establish any external binding or execution path.

<a id="r03"></a>
## R03 — Static implementation-map invariant

Falsifiable invariant: context owns W3C-shaped values and formatting; sampler owns decisions; span owns records and exemplars; service composes local traits; audit/exporter own trait plus fixtures. Evidence: named source modules. A moved or removed public surface requires remapping.

<a id="r04"></a>
## R04 — W3C context invariant

Falsifiable invariant: `TraceContext::new` rejects all-zero trace or span IDs, while `parse_traceparent` and `format_traceparent` form the local parser/formatter contract. Evidence: `src/context.rs` and `prop_traceparent_roundtrip`/`prop_zero_trace_id_rejected` test source. Test source is not an executed result.

**Public contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Trace context parsing and formatting
**Symbols:** `TraceContext::new`, `parse_traceparent`, `format_traceparent`; `W3C_TRACE_CONTEXT_VERSION`. **Input:** fixed-size IDs or canonical traceparent text. **Output/errors:** context/string or `TraceContextParseError`; invalid length, version, hex or zero IDs fail. **Effects:** local conversion. **Compatibility:** wire grammar changes affect callers; see [REL-003](BLAST_RADIUS.md#b03). **Evidence:** `src/context.rs`.
[Index](#r04)

<a id="api-002"></a>
### API-002 — Sampler
**Symbols:** `Sampler::should_sample`, `RateBasedSampler::{new,evaluate_with_audit}`. **Input:** trace ID; constructor rate must be finite and in `[0,1]`. **Output/errors:** `SamplingDecision` or `SamplerRateOutOfBounds` / `Audit`. **Effects:** deterministic local decision and optional injected audit call. **Compatibility:** decision labels are source-visible; see [REL-004](BLAST_RADIUS.md#b03). **Evidence:** `src/sampler.rs`.
[Index](#r04)

<a id="api-003"></a>
### API-003 — Span lifecycle service
**Symbols:** `TracingService::{new,start_span,end_span,in_flight_count,exporter_failure_count}`, `StartSpanInput`, `EndSpanInput`. **Input:** tier-keyed IDs, timestamps, status, attributes and exemplars. **Output/errors:** decision or typed audit/internal error. **Effects:** local ledger mutation; sampled completion calls injected exporter. **Compatibility:** public fields and trait bounds affect implementations; see [REL-005](BLAST_RADIUS.md#b03). **Evidence:** `src/service.rs`.
[Index](#r04)

<a id="api-004"></a>
### API-004 — Export and audit traits
**Symbols:** `OtlpExporter::export`, `TracingAuditSink::emit`, `SpanRecord`. **Input/output:** owned record batches and package-defined `Result`. **Effects:** only injected implementations act; in-memory/failing types are fixtures. **Compatibility:** no OTLP transport or delivery guarantee follows. **Evidence:** `src/exporter.rs`, `src/audit.rs`, `src/span.rs`.
[Index](#r04)

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

<a id="inv-001"></a>
### INV-001 — Canonical context rejects invalid values
**Predicate:** parser accepts only canonical version/length/hex and rejects all-zero IDs. **Enforcement:** `TraceContext::new`, `parse_traceparent`. **Violation:** invalid input returns parse error. **Verification:** named property-test source; execution unknown. **State:** SOURCE.
[Index](#r04)

<a id="inv-002"></a>
### INV-002 — Tenant tier scopes span lookup
**Predicate:** in-flight keys include `(tenant_tier, span_id_hex)`. **Enforcement:** `InFlightKey` at insert and lookup. **Violation:** removing tier from either operation allows cross-tier collision. **Verification:** source and `prop_tenant_isolation` source; execution unknown. **State:** SOURCE.
[Index](#r06)

<a id="inv-003"></a>
### INV-003 — Audit errors precede lifecycle mutation
**Predicate:** start/end audit errors propagate before ledger mutation; exporter failure follows a separate local path. **Enforcement:** `?` placement in service. **Violation:** mutation before audit or swallowed audit failure. **Verification:** source-order inspection and test source; execution unknown. **State:** SOURCE.
[Index](#r06)

<a id="inv-004"></a>
### INV-004 — Sampling rate and decision stay source-local
**Predicate:** `RateBasedSampler::new` rejects non-finite rates and rates outside `[0,1]`; `should_sample` derives a deterministic decision from the trace ID and configured sampler value. **Enforcement:** `sampler.rs`. **Violation:** accepting out-of-range input or changing the deterministic decision mapping. **Verification:** source and named property-test source; execution/configuration unknown. **State:** SOURCE. Related API/edge: [API-002](#api-002), [REL-004](BLAST_RADIUS.md#rel-004). [Index](#r04)

**API/invariant/relation crosswalk:** API-001/INV-001 → [REL-003](BLAST_RADIUS.md#rel-003); API-002/INV-004 → [REL-004](BLAST_RADIUS.md#rel-004); API-003/INV-002/INV-003 → [REL-004](BLAST_RADIUS.md#rel-004) and [REL-005](BLAST_RADIUS.md#rel-005); API-004/INV-003 → [REL-005](BLAST_RADIUS.md#rel-005), with the facade consumer in [REL-006](BLAST_RADIUS.md#rel-006). Analytics type use is [REL-001](BLAST_RADIUS.md#rel-001), and public module paths are [REL-002](BLAST_RADIUS.md#rel-002).

<a id="r05"></a>
## R05 — Sampling invariant

Falsifiable invariant: `RateBasedSampler` accepts rates only in `[0.0, 1.0]` and derives a local decision from a trace ID; `SamplingDecision` classifies sampled variants. Evidence: `src/sampler.rs`. No configured environment rate or production sampler is evidenced.

<a id="r06"></a>
## R06 — Span lifecycle invariant

Falsifiable invariant: `TracingService::start_span` records local start and sampler-decision audit events before ledger insertion; `end_span` looks up by tier and span ID, ends the record, then conditionally calls its exporter trait. Evidence: `src/service.rs`; `SpanRecord` fields in `src/span.rs`. This is source control flow, not runtime proof.

<a id="r07"></a>
## R07 — Failures, observability, and fixture invariant

Falsifiable invariant: `InMemoryOtlpExporter` and `InMemoryTracingAuditSink` capture local data, while `FailingOtlpExporter` and `FailingTracingAuditSink` return typed failures. Evidence: `src/exporter.rs`, `src/audit.rs`, and `src/error.rs`. Fixture behavior does not demonstrate a remote transport or durable audit sink.

`TracingError` separates audit, exporter, parse, sampler-rate, and internal
failures. `TracingService` exposes local `in_flight_count` and
`exporter_failure_count`; source also emits the typed exporter-failure audit
record on the exporter error branch. These fields/calls are not an exported
metric, alert, durable audit event, or observed runtime. The exact propagation
and coordination boundary is [REL-005](BLAST_RADIUS.md#b03); the telemetry
facade is [REL-006](BLAST_RADIUS.md#b03).

<a id="r08"></a>
## R08 — Success, completeness, quality, and definition of done

**Success:** source claims can be traced to the manifest or named module. **Completeness:** all exported module families, declared test source, and the static consumer relation are covered. **Quality:** every claim distinguishes SOURCE from runtime or executed proof. **Definition of Done:** baseline, four records, candidate structural checks, and explicit unknowns are handed off; no semantic or operational approval is implied.

**Static five-axiom / unknowns coverage:** (1) manifests and source establish static declarations only; (2) re-exports and signatures establish source-visible contracts only; (3) imports and traits do not establish invocation; (4) fixtures and test source do not establish execution; (5) `WAVE_008_PLAN` and canonical OKF material remain routing references, not material revalidated or redefined here.

Unknowns: Cargo feature resolution; all transitive consumers; consumer call paths; HTTP propagation; OTLP serialization and delivery; Tempo availability; audit durability; configuration; test execution; production operation; deployment; and review. [Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)

<a id="r09"></a>
## R09 — Declared targets, features, and configuration

The pinned manifest declares normal dependencies `thiserror`, `serde`, `serde_json`, and `corelink-analytics`; development dependencies `proptest`, `rand`, and `rand_chacha`; and one integration target, `prop_tracing`, at `tests/prop_tracing.rs`. No explicit `[features]` section, binary, example, benchmark, or build script is declared in this manifest. These are declarations only: workspace versions, resolved target graph, and CI selection were not resolved.

No package configuration file or environment-variable contract is established by this source set. The rate-based sampler accepts a constructor value; source comments describing intended production/staging rates are not configuration evidence. The manifest target and test source do not prove that the target was selected or executed. See [B02](BLAST_RADIUS.md#b02) for the inspected populations and [B06](BLAST_RADIUS.md#b06) for their limits.

[Index](#r01)
