---
schema: corelink-ownership/1.1
document: reference
package: corelink-telemetry
manifest: crates/corelink-telemetry/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: author_validated
evidence_set: telemetry-static-source-20260920
---

# corelink-telemetry — ownership reference

H-profile static reference for a hybrid telemetry umbrella. SOURCE evidence
describes checked-in declarations and local code only; it is separate from
executed, runtime, delivery, deployment, and review evidence. The canonical
OKF route is [the observability plane](../../../knowledge/ops/observability-plane.md);
this packet neither copies, redefines, nor revalidates it.

[Identity](#r01) · [Authority](#r02) · [Local map](#r03) · [Facades](#r04) · [Axioms](#r05) · [Tests/config](#r06) · [Relations](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Identity and source boundary

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-telemetry`; `crates/corelink-telemetry/Cargo.toml` |
| Root public paths | `canary`, `lighthouse`, `logpush`, `otel`, `synthetic_pager`, `tracing`, and `slo` in `src/lib.rs` |
| Shape | Hybrid umbrella: five local implementations and two dependency re-export routes |
| Direct package edges | `corelink-analytics`, `corelink-tracing`, and `corelink-slo` are manifest dependencies; external `tracing` is separately declared |

The five absorbed modules are local SOURCE surfaces. The two remaining routes
are not absorbed: `tracing` and `slo` use `pub use` of their defining packages.

<a id="r02"></a>
## R02 — Ownership and authority split

| Authority | Static conclusion | Boundary |
|---|---|---|
| Implementation owner | This package owns local source for `canary`, `lighthouse`, `logpush`, `otel`, and `synthetic_pager` | A trait, fake, or local constructor is not delivery or runtime ownership |
| Defining implementation owner | `corelink-tracing` owns `tracing`; `corelink-slo` owns `slo` | Re-export does not transfer implementation ownership |
| Public-contract owner | This root owns the seven named `corelink_telemetry::<module>` paths | Source path ownership is not consumer compatibility approval |
| Composition root | `corelink-container` statically imports `otel` and `tracing` in `src/routes/otel_layer.rs` | It is a source-visible composer/importer, not this package's implementation owner |
| Runtime operator | Unknown from this package's static surface | No operator, target, credential, or telemetry delivery is established |
| Review authority | Independent review is outside this author packet | Structural checking is not review approval |

<a id="r03"></a>
## R03 — Absorbed local implementation map

| Local root | Source-visible contract family |
|---|---|
| `canary` | assertion, audit, probe, region, result, config, and error modules |
| `lighthouse` | customer/state models, GA-gate computation, and `LighthouseStore` port |
| `logpush` | records, PII redactor, sink/audit ports, in-memory/failing fixtures, and migration string |
| `otel` | exporter/config/metric/audit/secret/error modules and in-memory exporter fixture |
| `synthetic_pager` | drill outcome decision, recorder port, typed request/region/vector/severity/error modules |

These local implementation owners own the checked-in source boundary and its
public re-exports. They do not own a consumer composition, remote endpoint,
scheduled trigger, persistence service, or any runtime operation.

<a id="r04"></a>
## R04 — Public facade contracts

| Public path | Static construction | Defining owner |
|---|---|---|
| `corelink_telemetry::tracing` | `pub use corelink_tracing::*` in `src/tracing.rs` | `corelink-tracing` |
| `corelink_telemetry::slo` | `pub use corelink_slo::*` in `src/slo.rs` | `corelink-slo` |

Removing, renaming, or changing either module can affect source imports at the
umbrella path. It does not make this package the implementation, runtime, or
delivery owner of either re-exported surface.

**Contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006).

<a id="api-001"></a>
### API-001 — Canary source API
**Symbols:** root exports `CanaryRegion`, `CanaryAssertion`, `CanaryProbe`, `CanaryAuditSink`, `CanaryLoopResult`, and `canary_schema_version`. **Input/precondition:** caller-supplied region, assertion, probe, or audit record. **Output/effects:** typed local decisions and injected trait calls. **Errors:** `CanaryError` and audit/probe errors. **Compatibility:** root names, three-region enum, schema version. **Links/evidence:** INV-001; REL-002/017; `canary.rs`, `canary/*`.
[Contract index](#r04)

<a id="api-002"></a>
### API-002 — Lighthouse store API
**Symbols:** `OBSERVATION_WINDOW_SECS`, `GA_LIGHTHOUSE_SLOTS_TOTAL`, `GA_LIGHTHOUSE_TEAM_SLOTS`, `GA_LIGHTHOUSE_ENTERPRISE_SLOTS`, `TrackerError`, `CustomerId`, `Tier`, `LifecycleState`, `ObservationOutcome`, `SlaSample`, `LighthouseCustomer`, `GaGateReport`, `compute_ga_gate`, and `LighthouseStore`. **Input/precondition:** typed roster/customer values. **Output/effects:** gate report or `TrackerError`; persistence depends on a store implementor. **Compatibility:** constants, enums, and trait methods are public. **Links/evidence:** INV-002; REL-003; `lighthouse.rs`.
[Contract index](#r04)

<a id="api-003"></a>
### API-003 — Logpush record and redaction API
**Symbols:** `LogRecord`, `TenantId`, `LogEventType`, `LogAuditSink`, `LogSink`, `PiiRedactor`, `PersistedLogLine`, `MIGRATION_0016_LOG_SCHEMA`, `logpush_schema_version`, and `SCHEMA_VERSION`. **Input/precondition:** typed log fields and injected ports. **Output/effects:** local transformation/recording; migration is an embedded DDL string. **Errors:** `LogpushError`, `LogSinkError`, `LogAuditSinkError`. **Compatibility:** fields, schema/event constants, redaction/sink methods, DDL. **Links/evidence:** INV-003; REL-004/005/012/014/015/018/019; `logpush.rs`, `logpush/*`.
[Contract index](#r04)

<a id="api-004"></a>
### API-004 — OTel exporter API
**Symbols:** `MetricsExporter`, `ExporterVariant`, `DatadogExporter`, `OtelCollectorExporter`, `GrafanaCloudExporter`, `InMemoryFake`, `DatadogConfig`, `DatadogSite`, `GrafanaCloudConfig`, `OtelCollectorConfig`, `OtlpProtocol`, `MetricPoint`, `MetricValue`, `TraceSpan`, `ExportFailedEvent`, `ExportFailedAuditSink`, `constant_time_secret_eq`, `validate_api_key_shape`, and `otel_export_schema_version`. **Input/precondition:** typed config/points and injected interfaces. **Output/effects:** exporter interface result; selected stub/fake behavior is source-specific. **Errors:** `ExporterError`, `SecretValidationError`. **Compatibility:** config, trait, schema, and symbol surface. **Links/evidence:** INV-004; REL-006/010/013/020/022; `otel.rs`, `otel/*`.
[Contract index](#r04)

<a id="api-005"></a>
### API-005 — Synthetic pager API
**Symbols:** `decide_drill_outcome`, `SyntheticDrillError`, `AckOutcome`, `MttaMs`, `MTTA_BUDGET_MS`, `UNACK_HARD_WINDOW_MS`, `DrillRecord`, `DrillRecorder`, `FailingDrillRecorder`, `InMemoryDrillRecorder`, `canonical_regions`, `Region`, `SyntheticDrillId`, `SyntheticPageRequest`, `canonical_drill_severities`, `DrillSeverity`, `canonical_ack_vectors`, `AckVector`, and `synthetic_pager_schema_version`. **Input/precondition:** typed drill facts. **Output/effects:** local outcome or injected recorder call. **Compatibility:** decision, constants, and exported types. **Links/evidence:** INV-005; REL-007/016; `synthetic_pager.rs`, `synthetic_pager/*`.
[Contract index](#r04)

<a id="api-006"></a>
### API-006 — Tracing and SLO facade routes
**Symbols:** `corelink_telemetry::tracing::*`, `corelink_telemetry::slo::*`. **Input/precondition:** consumer imports umbrella path. **Output/effects:** compile-time re-export of externally defined APIs; no local implementation. **Errors/effects:** downstream source break if route or target changes. **Compatibility:** public module paths; defining owners remain `corelink-tracing` and `corelink-slo`. **Links/evidence:** INV-006; REL-008/009/021; `tracing.rs`, `slo.rs`.
[Contract index](#r04)

<a id="r05"></a>
## R05 — Five falsifiable source axioms

[AXIOM-001](#axiom-001) · [AXIOM-002](#axiom-002) · [AXIOM-003](#axiom-003) · [AXIOM-004](#axiom-004) · [AXIOM-005](#axiom-005)

<a id="axiom-001"></a>
### AXIOM-001 — Seven root paths remain declared

**Predicate:** `src/lib.rs` publicly declares exactly the seven named root
modules recorded in R01. **Falsifier:** removal, rename, or an added root module
changes that declaration set. **Evidence:** `src/lib.rs`. [Return](#r05)

<a id="axiom-002"></a>
### AXIOM-002 — Facade implementation remains external

**Predicate:** `src/{tracing,slo}.rs` each contain the named `pub use` and no
local implementation definition for the re-exported package API. **Falsifier:**
the re-export target changes or a local replacement is introduced. **Evidence:**
`src/{tracing,slo}.rs`; `Cargo.toml`. [Return](#r05)

<a id="axiom-003"></a>
### AXIOM-003 — Logpush schema surface stays coupled in source

**Predicate:** `logpush_schema_version()` returns 16 while `LogRecord` exposes
`SCHEMA_VERSION` and the root exposes `MIGRATION_0016_LOG_SCHEMA`. **Falsifier:**
any named version or embedded migration export no longer agrees. **Evidence:**
`src/logpush/record.rs` and `src/logpush.rs`. [Return](#r05)

<a id="axiom-004"></a>
### AXIOM-004 — Canary vocabulary stays root-reachable

**Predicate:** `canary.rs` re-exports its assertion, audit, probe, region, and
result families and `canary_schema_version()` returns 1. **Falsifier:** a named
re-export or the version function disappears or changes. **Evidence:**
`src/canary.rs`. [Return](#r05)

<a id="axiom-005"></a>
### AXIOM-005 — Synthetic pager decision surface stays explicit

**Predicate:** `synthetic_pager.rs` re-exports `decide_drill_outcome`,
`DrillRecorder`, `MTTA_BUDGET_MS`, and `UNACK_HARD_WINDOW_MS`. **Falsifier:**
one named root re-export is removed or redirected. **Evidence:**
`src/synthetic_pager.rs`. [Return](#r05)

<a id="r06"></a>
## R06 — Static tests, configuration, and local dependency

The manifest declares six integration targets: canary, logpush, logpush PII,
migration 0016, OTel, and synthetic-pager property/source tests. Their filenames
are source-adjacent evidence, not a test result. `corelink-analytics` supplies
the canary/logpush region type, logpush cardinality validation, and the OTel
metric-kind taxonomy (REL-017–020). These source relations neither make
analytics a runtime operator nor prove metric delivery.

<a id="r07"></a>
## R07 — Source-visible composition relation

`corelink-container/src/routes/otel_layer.rs` imports named `otel` symbols and
separately imports `SpanKind`/`SpanStatus` through the tracing facade. That
facade forwards `corelink-tracing`; this package also declares external
`tracing` for instrumentation macros. These are checked-in source/dependency
relations, not an exhaustive consumer graph, selected target, or executed
composition. See [B05](BLAST_RADIUS.md#b05).

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Evidence class is SOURCE: manifest, root, five local module families, two facade
modules, checked-in test target declarations, and static text references. Unknown:
resolved features, complete consumers, downstream compatibility, actual adapter
selection, credentials, remote endpoints, scheduled execution, persistence,
telemetry delivery, runtime operator, deployment, and independent review.

[Guide](../../../../.claude/skills/own-corelink-telemetry/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)

<a id="inv-001"></a>
### INV-001 — Canary root export and version
**Predicate:** root canary path re-exports the recorded contract families and `canary_schema_version()` returns 1. **Enforcement:** `canary.rs`, `lib.rs`. **Violation:** a named export or version differs. **Verification:** compare module re-exports and function body; execution unknown. **State:** source assertion only; REL-002.
[Index](#r04)

<a id="inv-002"></a>
### INV-002 — Lighthouse persistence remains injected
**Predicate:** local stateful lighthouse operations use the `LighthouseStore` interface and this package family declares no concrete remote store. **Enforcement:** `lighthouse.rs` and submodule traits. **Violation:** concrete persistence is introduced or a transition bypasses the interface. **Verification:** inspect trait/impl definitions; runtime unknown. **State:** source boundary only; REL-003.
[Index](#r04)

<a id="inv-003"></a>
### INV-003 — Logpush schema exports stay aligned
**Predicate:** `logpush_schema_version()` returns 16 and the root continues exporting `LogRecord::SCHEMA_VERSION` and `MIGRATION_0016_LOG_SCHEMA`. **Enforcement:** `logpush/record.rs`, `logpush.rs`. **Violation:** version, record constant, or migration export changes inconsistently. **Verification:** inspect all three source declarations; execution unknown. **State:** source contract only; REL-004/005.
[Index](#r04)

<a id="inv-004"></a>
### INV-004 — OTel configuration stays typed at this boundary
**Predicate:** OTel exported configuration and secret handling continue through the declared typed config/secret modules. **Enforcement:** `otel/config.rs`, `otel/secret.rs`, root `otel.rs`. **Violation:** the route bypasses those types or exposes an unreviewed value. **Verification:** inspect exports and constructors; no secret value inspected. **State:** source-only; REL-006.
[Index](#r04)

<a id="inv-005"></a>
### INV-005 — Pager decision surface remains root reachable
**Predicate:** `synthetic_pager` root route exports the decision function, recorder trait, and named budget constants. **Enforcement:** `synthetic_pager.rs`. **Violation:** any named route disappears or changes target. **Verification:** compare `pub use` statements and definitions; runtime unknown. **State:** source-only; REL-007.
[Index](#r04)

<a id="inv-006"></a>
### INV-006 — Facades preserve defining-package ownership
**Predicate:** tracing and SLO modules remain direct re-exports from their defining packages. **Enforcement:** `tracing.rs`, `slo.rs`, manifest. **Violation:** route target changes or is described as local implementation without evidence. **Verification:** inspect both `pub use` declarations and dependencies. **State:** static facade only; REL-008/009.
[Index](#r04)
