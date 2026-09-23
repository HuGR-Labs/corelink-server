---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-tracing
manifest: crates/corelink-tracing/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: tracing-static-source-20260920
---

# corelink-tracing — blast radius

Source relationships only. A dependency, import, trait, or fake does not prove execution, propagation from HTTP, OTLP/Tempo delivery, deployment, or runtime observation.

[B01 Scope](#b01) · [B02 Method](#b02) · [B03 Direct relationships](#b03) · [B04 Transitive propagation](#b04) · [B05 Change validation](#b05) · [B06 Coverage](#b06).

<a id="b01"></a>
## B01 — Scope

Implementation and local public-contract owner: `corelink-tracing`. Direct type dependency owner: `corelink-analytics` for `Tier` and `RedMetricKind`. Canonical facade/composition is `corelink-telemetry`, which re-exports tracing symbols without taking implementation ownership. Runtime adapter, HTTP propagation, exporter transport, audit durability, and deployment owners are not established by this package evidence; route those questions through the composition owner and verified OKF process.

<a id="b02"></a>
## B02 — Method and populations

**Inventory:** inspected `Cargo.toml`, `src/lib.rs`, seven declared public module families, and package source references for analytics types, exporter/audit traits, sampler/service calls, plus the telemetry façade manifest/root. **Resolved graph:** no target/feature selection was resolved for a shipped artifact. **Semantic graph:** source call sites were traced in context, sampler, service, span, audit, exporter, and telemetry re-export. **Reverse consumers:** current evidence names `corelink-telemetry`; exhaustive reverse Cargo and non-Cargo callers are unknown. No tests or runtime paths were executed.

| Population | Inspection method | Found/documented | Limit |
|---|---|---|---|
| Package manifest | Read dependencies, dev-dependencies, features, targets, and explicit test declarations | One `prop_tracing` target; no explicit feature section; normal and dev dependencies summarized in R09 | Workspace resolution and CI selection unknown |
| Package source | Read root and seven declared module families; follow calls through service and fixtures | API-001–004, INV-001–004, source failure/observer branches | Source only; no compile or execution |
| Test population | Read `tests/prop_tracing.rs` and its named properties; search manifest for additional target declarations | Property-test source for context, sampler, tenant isolation, exemplar, and audit/lifecycle behavior | No test result or coverage percentage claimed |
| Reverse consumers | Search pinned workspace manifest/imports and inspect the known facade manifest/root | `corelink-telemetry`, REL-006 | No exhaustive external/generated callers |
| Peer reconciliation | Compare tracing relation with telemetry facade relation | Telemetry facade edge is identified at both sides; shared fingerprint/bilateral reconciliation remains `NOT_RECONCILED` | No peer approval or build selection claimed |

<a id="b03"></a>
## B03 — Direct relationships

<a id="rel-001"></a><a id="rel-002"></a><a id="rel-003"></a><a id="rel-004"></a><a id="rel-005"></a><a id="rel-006"></a>

**REL-001 — Analytics type dependency.** Identity `repo:1232040291:boundary:corelink-tracing-analytics-tier`. Producer: `corelink-analytics::{Tier,RedMetricKind}`; consumer: tracing service/span. **Type:** dependency/data. **Activation:** normal manifest edge and source imports. **Contract/effect:** tier keys in-flight state; metric kind appears in exemplars. **Failure propagation:** upstream type changes can break tracing source/API. **Validation/coordination:** compare both manifests and exact imports; coordinate analytics contract owner. **Evidence:** manifest, `src/service.rs`, `src/span.rs`; runtime unknown.

**REL-002 — Local modules to public root.** Identity `repo:1232040291:boundary:corelink-tracing-root-exports`. Producer: seven implementation modules; consumer: `src/lib.rs` public module/re-export paths. **Type:** re-export. **Activation:** unconditional root declarations. **Contract/effect:** symbols become importable from crate paths; behavior remains owned by each local module. **Failure propagation:** path/signature changes may break direct and façade consumers. **Validation/coordination:** compare root exports, API-001–004, and inverse imports. **Evidence:** root and module sources; complete inverse census unknown.

**REL-003 — Context values to lifecycle records.** Identity `repo:1232040291:boundary:corelink-tracing-context-span-data`. Producer: parsed/constructed `TraceId`/`SpanId`; consumer: `StartSpanInput`, `EndSpanInput`, and `SpanRecord`. **Type:** data. **Activation:** local service method receives the IDs. **Contract/effect:** fixed-width identifiers are encoded into local records/keys. **Failure propagation:** width/format changes affect parser, service, and export shape. **Validation/coordination:** compare `context.rs`, `service.rs`, and `span.rs`; coordinate package API owner. **Evidence:** named source types; no HTTP receipt/propagation is evidenced.

**REL-004 — Sampler to lifecycle.** Identity `repo:1232040291:boundary:corelink-tracing-sampler-lifecycle`. Producer: injected `Sampler::should_sample`; consumer: `TracingService::start_span` and stored decision. **Type:** runtime-call (source). **Activation:** each source call to start. **Contract/effect:** one local sampling decision controls later export selection. **Failure propagation:** decision enum or sampler contract changes affect end/export behavior. **Validation/coordination:** trace `start_span`, sampler implementations, and tests; coordinate sampler API owner. **Evidence:** source call sites only; configured rate/observed sample rate unknown.

**REL-005 — Lifecycle to injected exporter.** Identity `repo:1232040291:boundary:corelink-tracing-lifecycle-exporter`. Producer: `TracingService::end_span`; consumer: injected `OtlpExporter::export(Vec<SpanRecord>)`. **Type:** runtime-call (source). **Activation:** sampled decision after lookup/end mutation. **Contract/effect:** local batch passed to trait; exporter error is recorded and follows the source-defined best-effort path. **Failure propagation:** trait/error changes affect callers; failing exporter is not transport evidence. **Validation/coordination:** inspect service/exporter/error and matching tests; coordinate adapter owner. **Evidence:** source only; no OTLP or Tempo delivery observed.

**REL-006 — Telemetry facade reverse consumer.** Identity `repo:1232040291:boundary:corelink-telemetry-tracing-facade`; peer reference: `corelink-telemetry#REL-008`. Producer: `corelink-tracing::*`; consumer: `corelink_telemetry::tracing` re-export. **Type:** dependency/re-export. **Activation:** manifest edge and facade source. **Contract/effect:** facade offers import paths; implementation and canonical source contract stay with this package. **Failure propagation:** symbol/signature changes can break facade and its downstream users. **Validation/coordination:** compare both manifests, roots, and facade module; peer fingerprint reconciliation is `NOT_RECONCILED`. **Evidence:** pinned manifests/source; resolved target and runtime unknown.

<a id="b04"></a>
## B04 — Transitive propagation

The telemetry facade can propagate a tracing path/signature change to packages importing `corelink_telemetry::tracing` (REL-006); downstream population is not exhaustively resolved and peer reconciliation remains open. Within this crate, context format changes flow through service inputs and exported records (REL-003). Sampler decisions are stored by `start_span` and control the sampled completion/export branch (REL-004→REL-005). Analytics types are imported into tracing surfaces through a separate normal dependency (REL-001); public modules route those symbols through the root (REL-002).

These are source dependencies/control flow, not proof that HTTP headers reach the parser or that an exporter is wired. `corelink-analytics` owns its types; telemetry owns façade composition. Re-exports and manifest edges do not transfer tracing implementation ownership.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Impact | Validation/coordination |
|---|---|---|
| Public root path/type | REL-002 and telemetry REL-006 importers may break | Inspect root exports and all found inverse imports; coordinate tracing API and telemetry façade owners |
| Context grammar/ID representation | REL-003 parser, service key, record compatibility | Compare parser/formatter, `SpanRecord`, exact source property tests; no executed result unless separately recorded |
| Sampling decision/rate logic | REL-004 lifecycle and export selection | Trace implementations and `start_span`/`end_span`; coordinate sampler owner; configured runtime rate remains unknown |
| Audit/export error or call order | REL-005 path and error propagation/observability | Inspect error taxonomy, audit-before-mutation ordering, exporter-failure counter/event path; coordinate sink/exporter owners |
| Analytics contract | REL-001 compile/API boundary | Compare analytics type source and tracing imports; feature/build selection unknown |
| Declared target, feature, or configuration surface | R09 and test population in B02; any affected API/REL row | Compare manifest target/dependency declarations, root exports, and named test source; do not infer resolved selection or configured rates |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Covered/documented | Limit / unknown |
|---|---|---|
| Manifest/targets | Normal and dev dependencies, one declared `prop_tracing` target, no explicit features in R09 | Workspace resolution and CI selection unknown |
| Implementation source | Seven modules, public root paths, context, sampler, service, span, audit/exporter traits | No HTTP ingress, transport, or selected adapter established |
| Tests | `tests/prop_tracing.rs` named property-test source is in B02 | No execution, coverage percentage, or result asserted |
| Consumers/peers | `corelink-analytics` type dependency and `corelink-telemetry` facade in REL-001/006 | Reverse caller population incomplete; facade peer not reconciled |
| Failures/observability | Typed tracing errors, local in-flight/exporter-failure counts, exporter-failure audit branch | No metrics export, alert, durable audit, or observation proof |

Unknown: complete inverse consumers, selected feature/build targets, HTTP propagation, external adapters, OTLP serialization/delivery, Tempo, audit persistence, config, executed tests, deploy, and runtime. Absence from this bounded census is not absence proof; repeat discovery before compatibility or operational claims.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
