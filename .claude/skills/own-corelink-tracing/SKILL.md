---
name: own-corelink-tracing
description: Use for static ownership of corelink-tracing W3C context parsing, sampling, span, exporter, service, audit, and fake contracts; exclude runtime telemetry claims.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "corelink-tracing"
  manifest: "crates/corelink-tracing/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "tracing-static-source-20260920"
---

# Ownership — corelink-tracing

Static-source routing guide. SOURCE is not proof of runtime, execution, delivery, deployment, or production telemetry.

[Trigger](#s01) · [Boundary](#s02) · [Parser](#s03) · [Sampler](#s04) · [Service](#s05) · [Fakes](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| A tracing contract changes | Route to this crate’s static ownership records | `src/lib.rs`; manifest | The request asserts live telemetry behavior |

<a id="s02"></a>
## S02 — Boundary

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Scope is uncertain | Keep it to parser, sampler, span, service, audit, exporter, errors, and test-source contracts | `src/*.rs`; `tests/prop_tracing.rs` | Infer OTLP, Tempo, backend, or runtime execution |

**Static five-axiom / unknowns coverage:** (1) manifest and source prove only static declarations; (2) re-exports and signatures prove only source-visible contracts; (3) imports and traits prove no invocation; (4) fakes and test source prove no execution; (5) `WAVE_008_PLAN` and canonical OKF material are routing references, never revalidated or redefined here. Unknowns include resolved consumers, configuration, delivery, durability, deployment, and review.

<a id="s03"></a>
## S03 — W3C parser

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| `traceparent` shape changes | Compare `TraceContext`, `parse_traceparent`, and `format_traceparent` | `src/context.rs` | Treat static parsing as HTTP propagation proof |

<a id="s04"></a>
## S04 — Sampler

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Rate or decision changes | Review `Sampler`, `RateBasedSampler`, and audit ordering | `src/sampler.rs` | Claim a configured production sampling rate |

<a id="s05"></a>
## S05 — Service and spans

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Lifecycle, span, or exemplar changes | Trace start/end state, span fields, and analytics types | `src/service.rs`; `src/span.rs` | Claim a handler, SDK, or remote exporter path |

<a id="s06"></a>
## S06 — Exporter and audit fakes

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Sink behavior changes | Distinguish traits from in-memory and failing fixtures | `src/exporter.rs`; `src/audit.rs` | Present a fake capture as transport or durability evidence |

<a id="s07"></a>
## S07 — Handoff

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Static assessment is complete | Report baseline, inspected source, static relations, and unknowns | [reference](../../../docs/ownership/crates/corelink-tracing/REFERENCE.md#r08) | Call static checks runtime, executed, or production proof |

[Back to trigger](#s01)
