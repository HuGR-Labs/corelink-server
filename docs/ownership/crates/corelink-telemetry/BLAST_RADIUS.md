---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-telemetry
manifest: crates/corelink-telemetry/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: candidate
evidence_set: telemetry-static-source-20260920
---

# corelink-telemetry — blast radius

Atomic SOURCE relations for this hybrid umbrella. Directions distinguish
declaration/dependency, code input/output, and impact propagation. They do not
prove execution, telemetry delivery, deployment, or a complete consumer graph.

[Scope](#b01) · [Method](#b02) · [Direct relations](#b03) · [Propagation](#b04) · [Change map](#b05) · [Coverage and unknowns](#b06).

Relation index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021) · [REL-022](#rel-022).

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-corelink-telemetry/SKILL.md#s01).

<a id="b01"></a>
## B01 — Scope

The package has five absorbed local module families and two direct facade
re-exports. The relations below are bounded to tracked manifest/source/test
declarations at the stated SHA. Implementation, public route, composition root,
runtime operator, and review authority remain distinct as in REFERENCE R02.
[Relation index](#b03)

<a id="b02"></a>
## B02 — Method and populations

Inventory: direct normal dependencies, root routes, six declared test targets,
local source trees, and located container imports. Semantic pass: source
re-exports, injected ports, schema strings, and matching test imports. No
resolved target/feature graph or runtime observation is represented. New source
matches found during change review must be added or justified as excluded.
[Relation index](#b03)

<a id="b03"></a>
## B03 — Direct atomic relations

<a id="rel-001"></a>
### REL-001 — Root declaration to canary module
Type: public declaration. Producer `src/lib.rs` → consumer `canary.rs`; surface
`corelink_telemetry::canary`. Activation: source import compiles against route.
Effect: root route exposes local family; route removal breaks imports. Boundary:
no selected consumer inferred. Validate INV-001 and `lib.rs`/`canary.rs`;
coordinate route owner. Evidence: those files.
[Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Canary region and assertion input
Type: local data/call. Producer `CanaryRegion` and `CanaryAssertion` → assertion
decision helpers. Activation: supplied region/latencies. Effect: typed local
decision under declared thresholds. Failure: variant/threshold change alters
classification. Boundary: no probe run. Validate API-001/INV-001 and assertion
source; evidence `canary/{region,assertion,result}.rs`.
[Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Lighthouse interface route
Type: public declaration/data port. Producer `src/lib.rs` → `lighthouse.rs` and
`LighthouseStore`. Activation: import/call through package API. Effect: exposes
typed lifecycle and injected persistence contract. Failure: trait/state change
affects implementors. Boundary: concrete adapter/operator unknown. Validate
API-002/INV-002; evidence lighthouse module tree.
[Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Logpush record to schema version
Type: data. Producer `LogRecord` → `logpush_schema_version`/schema constant.
Activation: record construction or schema query. Effect: fields and version form
a source-visible schema contract. Failure: inconsistent version breaks
consumers. Boundary: no persisted row inspected. Validate API-003/INV-003;
evidence `logpush/record.rs`, `logpush.rs`.
[Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Migration SQL embedded into public constant
Type: build-time data inclusion; `migrations/d1/0016_log_schema.sql` →
`include_str!` in `logpush.rs` → `MIGRATION_0016_LOG_SCHEMA`. Activation: crate
compilation and caller import. Effect: migration bytes become a compile-time
string constant. Failure: path/content change alters compilation or embedded
DDL. Boundary: no migration execution or database effect is proven. Validate
the include path and target file; evidence `logpush.rs`, migration SQL.
[Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — OTel root to typed modules
Type: public declaration. Producer `otel.rs` → `otel::{config,secret,metric,audit,error,exporter}`.
Activation: import/call through root route. Effect: exposes typed config/exporter
contracts. Failure: route/type change breaks local consumers. Boundary: endpoint
and exporter implementation selection unknown. Validate API-004/INV-004;
evidence OTel module tree.
[Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Synthetic pager decision to recorder
Type: local call/data. Producer request/region/vector/severity inputs →
`decide_drill_outcome` and `DrillRecorder`. Activation: caller supplies drill
facts. Effect: local classification then optional injected record call. Failure:
decision/trait change affects callers. Boundary: no live alert. Validate
API-005/INV-005; evidence `synthetic_pager/*`.
[Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Tracing facade re-export
Shared key `repo:1232040291:boundary:corelink-telemetry-tracing-facade`.
Type: re-export/dependency. Producer `corelink-tracing::*` → consumer module
`corelink_telemetry::tracing`. Activation: compile-time import. Effect: public
path forwards defining crate symbols. Failure: import/API break; implementation
owner remains corelink-tracing. Validate API-006/INV-006; evidence manifest and
`src/tracing.rs`. Peer reconciliation: unknown.
[Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — SLO facade re-export
Shared key `repo:1232040291:boundary:corelink-telemetry-slo-facade`.
Type: re-export/dependency. Producer `corelink-slo::*` → consumer module
`corelink_telemetry::slo`. Activation: compile-time import. Effect: public path
forwards defining crate symbols. Failure: import/API break; implementation owner
remains corelink-slo. Validate API-006/INV-006; evidence manifest and
`src/slo.rs`. Peer reconciliation: unknown.
[Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Container OTel composition imports
Type: source import. Producer `corelink-container/src/routes/otel_layer.rs` →
consumer `corelink_telemetry::otel::{audit,config,exporter,metric}`. Activation:
Rust compilation of that source. Effect: named OTel API imports depend on these
root paths. Failure: removed/renamed OTel exports break source compilation.
Boundary: import does not prove route construction or execution. Validate OTel
imports only; coordinate container composer. Evidence: `otel_layer.rs`.
[Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Canary region type from analytics
Type/data; `corelink-analytics::Region` → `corelink-telemetry::canary::CanaryRegion` mapping. Activation: canary region conversion is called. Contract/effect: analytics region enum supplies the source region vocabulary used for labels. Failure: variant or string changes can break conversion or labels. Boundary: dependency declaration and source use do not prove runtime canary operation. Validate mapping and analytics enum; evidence `canary/region.rs`, manifest, analytics source.
[Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Log record region type from analytics
Type/data; `corelink-analytics::Region` → `LogRecord` region field. Activation: constructing a typed log record. Contract/effect: shared enum defines region values in the log record API. Failure: analytics variant/type changes affect record construction and serialization. Boundary: no persisted log row is established. Validate record type and source conversion; evidence `logpush/record.rs`, manifest.
[Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Log sink cardinality validator
Type/call; `corelink-analytics::CardinalityValidator` → `logpush::LogSink` validation path. Activation: sink processes a record under configured budgets. Contract/effect: validator can reject a metric label tuple exceeding its budget. Failure: validator contract or label set changes alter sink result. Boundary: no configured production budget or persisted output is established. Validate call and error mapping; evidence `logpush/sink.rs`, `logpush/error.rs`, manifest.
[Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — OTel metric kind from analytics
Type/data; `corelink-analytics::RedMetricKind` → `otel::MetricPoint.kind` and `metric_name()`. Activation: constructing or exporting a metric point. Contract/effect: closed analytics enum provides canonical kind and slug. Failure: variant/literal changes affect local name mapping and adapter input. Boundary: this crate's use does not prove metrics export or remote ingestion. Validate kind mapping and exporter call sites; evidence `otel/metric.rs`, manifest.
[Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Container structured-tracing facade imports
Type/re-export; `corelink-container/src/routes/otel_layer.rs` → `corelink_telemetry::tracing::{SpanKind,SpanStatus}`. Activation: compilation of that container source. Contract/effect: this route imports span vocabulary through the facade. Failure: facade export/signature changes can break this import. Boundary: does not prove tracing subscriber setup or event emission. Validate this import and REL-008 separately from the OTel imports in REL-010; evidence: container `otel_layer.rs`, telemetry `src/tracing.rs`.
[Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — OTel instrumentation to external tracing crate
Type/dependency and source call; `corelink-telemetry`'s workspace `tracing` dependency → tracing macros in `otel/exporter.rs`. Activation: exporter source executes the instrumentation branch. Contract/effect: emits local structured debug events under `corelink_otel_export`. Failure: tracing crate/dependency changes can affect macro compilation or event fields. Boundary: separate from the `corelink-tracing` facade package and no collector delivery is implied. Validate Cargo declaration and macro call sites; evidence manifest and exporter source.
[Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Canary integration-test source
Type: test relation. Producer manifest target `prop_canary` → `tests/prop_canary.rs`.
Activation: that target is selected. Effect: test source references canary
contracts. Failure: changed contracts may invalidate source assertions.
Boundary: not executed here. Validate target path and test imports; evidence
manifest and test source.
[Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Logpush property-test target
Type: test. Producer manifest target `prop_logpush` → `tests/prop_logpush.rs`.
Activation: this target is selected. Effect: source assertions bind logpush
record/sink behavior. Failure: changed contract can break test source or
predicate. Boundary: not executed here. Validate target/path and imports;
evidence manifest and test file.
[Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Logpush PII test target
Type: test. Producer manifest target `pii_redaction_100k_synthetic` →
`tests/pii_redaction_100k_synthetic.rs`. Activation: this target is selected.
Effect: source assertions constrain local redaction behavior. Failure: changed
redaction contract can alter assertions. Boundary: source is not a test result.
Validate target/path and redaction calls; evidence manifest/test source.
[Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Canonical migration test target
Type: test. Producer manifest target `migration_canonical_0016` →
`tests/migration_canonical_0016.rs`. Activation: this target is selected.
Effect: source test binds migration text/schema contracts. Failure: migration
text/version changes can invalidate assertions. Boundary: migration not run.
Validate target/path and symbols; evidence manifest/test source.
[Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — OTel integration-test target
Type: test. Producer manifest target `prop_otel_export` →
`tests/prop_otel_export.rs`. Activation: target selected. Effect: source checks
the typed export contract. Failure: changed API/payload may affect assertions.
Boundary: no test result or remote exporter operation claimed. Validate target,
imports, and assertion predicates; evidence manifest/test file.
[Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Synthetic pager property-test target
Type: test. Producer manifest target `prop_synthetic_pager` →
`tests/prop_synthetic_pager.rs`. Activation: target selected. Effect: source
assertions bind local pager decision/recording. Failure: changed predicate may
change test outcome. Boundary: not executed here. Validate target and contracts;
evidence manifest/test source.
[Relation index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

Local root changes propagate through REL-001–007; facade re-exports through
REL-008/009. Container imports are split between OTel (REL-010) and the tracing
facade (REL-021), while test constraints are REL-011–016. Analytics coupling
has separate canary, logpush record, logpush validator, and OTel metric-kind
relations (REL-017–020). The external `tracing` crate used by OTel
instrumentation is REL-022, separate from `corelink-tracing`. Changes require a
fresh consumer search and target-specific Cargo resolution before claiming
exhaustive/build reachability; resolution is not runtime evidence.
[Relation index](#b03)

<a id="b05"></a>
## B05 — Change, impact, and validation

Local family API/schema edits: linked API/INV plus REL-001–007/017–020 and
corresponding tests. Facade edits: REL-008/009/021 plus defining crate and
located import review. Container OTel imports are REL-010; OTel instrumentation
dependency changes are REL-022. Test target changes: REL-011–016. Validate
using the matching maintenance PROC; coordinate defining crate for re-export
implementation changes and the container owner for composition imports. Never
promote static source evidence into execution proof.
[Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and unknowns

Enumerated here: seven root paths, five local families, three direct package
dependencies, six declared integration test targets, two facade edges, one
located container composition source, and local source trees. This is a
declaration census, not a certified exhaustive inverse graph; search terms,
resolved features, generated/dynamic imports, external SDKs, endpoint selection,
runtime operator, and delivery remain unknown. Cold reviewer must repeat the
search and challenge exclusions. Stop any claim that requires those unknowns.
[Relation index](#b03)
