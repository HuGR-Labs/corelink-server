---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-slo
manifest: crates/corelink-slo/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: slo-source-static-20260920
---

# corelink-slo — blast radius

This is a static source/manifest census at the recorded source revision.
Direction labels distinguish dependency, data flow, and impact propagation.
No relation proves invocation, runtime alerting, metric ingestion, PagerDuty
delivery, deployment, or a complete external consumer set.

[Scope and method](#b01) · [Relations](#b02) · [Propagation](#b03) · [Change map](#b04) · [Coverage](#b05) · [Unknowns](#b06).

Relation index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021) · [REL-022](#rel-022).

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-corelink-slo/SKILL.md#s01).

<a id="b01"></a>
## B01 — Scope and method

Inventory is from the manifest, root exports, source call sites, and located
consumer manifests/tests. It is not a resolved feature graph. `corelink-slo`
contains pure calculation logic plus injected audit/dispatch ports; no network
client is declared. The telemetry facade and DSR metric binding are source
relations only.

<a id="b02"></a>
## B02 — Atomic direct relations

<a id="rel-001"></a>
### REL-001 — Definition values into calculator
Type/data; `SloDefinition` → `BurnRateCalculator::decide`. Activation: caller
passes definition/sample/window. Contract/effect: target-derived budget feeds a
local comparison; invalid definitions are rejected at construction. Failure:
changed validation or budget values alter outcomes. Boundary: no measurement
source. Validate API-001/INV-001 and calculator branches; evidence
`definition.rs`, `calculator.rs`.
[Relation index](#b02)

<a id="rel-002"></a>
### REL-002 — Window multiplier into decision arm
Type/data; `BurnRateWindow` → calculator threshold and `AlertDecision`. Activation:
selected window is supplied. Contract/effect: four multipliers map to fixed
decision variants. Failure: taxonomy/multiplier changes alter local decision.
Boundary: no alert-rule execution. Validate API-002/INV-003 and window match;
evidence `window.rs`, `calculator.rs`, `decision.rs`.
[Relation index](#b02)

<a id="rel-003"></a>
### REL-003 — Zero-total sample into rate calculation
Type/runtime-call; `BurnRateSample` → `error_rate_in_window`. Activation: sample
with `total_in_window == 0`. Contract/effect: returns `0.0`, avoiding division.
Failure: changed branch alters quiet/decision path. Boundary: no observed
request counts. Validate API-002/INV-002 and zero-total source/test;
evidence `calculator.rs`, `tests/prop_slo.rs`.
[Relation index](#b02)

<a id="rel-004"></a>
### REL-004 — Evaluation decision into audit sink
Type/runtime-call; `MultiBurnRateAlert::evaluate` → injected `SloAuditSink::emit`.
Activation: evaluation reaches an audit branch. Contract/effect: source emits
the decision record before corresponding local mutation. Failure: sink error
returns through the typed path before that mutation. Boundary: no durability or
external transaction. Validate API-003/INV-004; evidence `alert.rs`, `audit.rs`.
[Relation index](#b02)

<a id="rel-005"></a>
### REL-005 — Page decision into dispatcher
Type/runtime-call; `AlertDecision::is_page` → `dispatch_page` → injected
`PagerDutyDispatcher::dispatch`. Activation: decision is PageSev0/PageSev1.
Contract/effect: constructs an event and calls the trait; ticket/quiet branches
do not. Failure: dispatch error follows the typed error branch. Boundary: no
provider call/incident proven. Validate API-003/INV-005; evidence `alert.rs`,
`pagerduty.rs`.
[Relation index](#b02)

<a id="rel-006"></a>
### REL-006 — Telemetry facade re-export
Shared key `repo:1232040291:boundary:corelink-telemetry-slo-facade`.
Type/re-export; `corelink-slo::*` → `corelink-telemetry::slo`. Activation:
`corelink-telemetry/src/slo.rs` imports the SLO facade. Contract/effect: public
names are forwarded; defining implementation remains this package. Failure:
changed names can break facade imports. Boundary: no runtime reachability.
Validate manifest and telemetry `src/slo.rs`; coordinate telemetry
public-contract owner; evidence `corelink-telemetry` source. Peer record:
`corelink-telemetry` REL-009; shared facts reconciled from source.
[Relation index](#b02)

<a id="rel-007"></a>
### REL-007 — DSR metric binding test
Type/test; `corelink-privacy-erasure-worker/tests/sli_binding.rs` →
`Sli::FreshDsrErasure.prometheus_metric_base()`. Activation: worker test target
is selected. Contract/effect: asserts the static metric string matches the
worker constant. Failure: string change fails that source assertion when run.
Boundary: no cron emission or scrape. Validate test source and its dev-dependency;
coordinate worker owner; evidence worker manifest/test.
[Relation index](#b02)

<a id="rel-008"></a>
### REL-008 — Region SLI binding test
Shared key `repo:1232040291:boundary:corelink-region-slo-sli-test-binding`.
Type/test; `corelink-region/tests/sli_binding.rs` → SLI metric-base values.
Activation: region integration test target is selected. Contract/effect: binds
region metric constants to SLO taxonomy. Failure: changed names disagree in the
asserted source pair. Boundary: test not executed here. Validate region test and
dev-dependency; coordinate region owner; evidence region manifest/test. Peer
record: `corelink-region` REL-010; shared key and source facts reconciled.
[Relation index](#b02)

<a id="rel-009"></a>
### REL-009 — Container direct dependency
Type/dependency; `corelink-container/Cargo.toml` → `corelink-slo`. Activation: manifest selection for a chosen container target. Contract/effect: declared package edge can affect resolution and compile compatibility. Failure: selected build may reject changed API. Boundary: target, feature resolution, and use path remain separate unknowns. Validate manifest and imports; evidence: container manifest.
[Relation index](#b02)

<a id="rel-010"></a>
### REL-010 — Ops direct dependency
Type/dependency; `corelink-ops/Cargo.toml` → `corelink-slo`. Activation: manifest selection for an ops target. Contract/effect: declared direct edge. Failure: selected build may reject changed API. Boundary: dependency declaration does not prove runtime use. Validate manifest and imports; evidence: ops manifest.
[Relation index](#b02)

<a id="rel-011"></a>
### REL-011 — Replica-worker direct dependency
Type/dependency; `corelink-replica-worker/Cargo.toml` → `corelink-slo`. Activation: manifest selects the package. Contract/effect: path dependency creates compile/resolution coupling. Failure: selected build may reject changed API. Boundary: target and runtime invocation unknown. Validate manifest and source imports; evidence: replica-worker manifest.
[Relation index](#b02)

<a id="rel-012"></a>
### REL-012 — Container SLO composition call
Type/source call; `corelink-container` source → SLO API. Activation: source compiles or its caller invokes the named path. Contract/effect: this relation is distinct from the package edge because it is a source composition surface. Failure: changed symbol or semantics affects that caller. Boundary: runtime construction/execution unknown. Validate located import/call; evidence: container source search.
[Relation index](#b02)

<a id="rel-013"></a>
### REL-013 — Handler AC direct dependency
Type/dependency; `corelink-handler-ac/Cargo.toml` → `corelink-slo`. Activation: selected handler target. Contract/effect: manifest-level compile/resolution edge. Failure: selected build may reject API changes. Boundary: actual route invocation unknown. Validate manifest and imports; evidence: handler-ac manifest.
[Relation index](#b02)

<a id="rel-014"></a>
### REL-014 — Handler admin direct dependency
Type/dependency; `corelink-handler-admin/Cargo.toml` → `corelink-slo`. Activation: selected handler target. Contract/effect: manifest-level compile/resolution edge. Failure: selected build may reject API changes. Boundary: actual route invocation unknown. Validate manifest and imports; evidence: handler-admin manifest.
[Relation index](#b02)

<a id="rel-015"></a>
### REL-015 — Handler CAS direct dependency
Type/dependency; `corelink-handler-cas/Cargo.toml` → `corelink-slo`. Activation: selected handler target. Contract/effect: manifest-level compile/resolution edge. Failure: selected build may reject API changes. Boundary: actual route invocation unknown. Validate manifest and imports; evidence: handler-cas manifest.
[Relation index](#b02)

<a id="rel-016"></a>
### REL-016 — Handler customer direct dependency
Type/dependency; `corelink-handler-customer/Cargo.toml` → `corelink-slo`. Activation: selected handler target; the manifest also repeats the key under dev-dependencies. Contract/effect: normal and test resolution can be affected separately. Failure: selected build may reject changed API. Boundary: route invocation unknown. Validate both declarations and imports; evidence: handler-customer manifest.
[Relation index](#b02)

<a id="rel-017"></a>
### REL-017 — Telemetry SLO direct dependency
Type/dependency; `corelink-telemetry/Cargo.toml` → `corelink-slo`. Activation: selected telemetry target. Contract/effect: package dependency supports the public facade route. Failure: resolution or symbol change may break the route. Boundary: re-export is not implementation or runtime ownership. Validate dependency and facade source; evidence: telemetry manifest and `src/slo.rs`.
[Relation index](#b02)

<a id="rel-018"></a>
### REL-018 — Evaluation audit sink port
Type/source call; `MultiBurnRateAlert` → injected `SloAuditSink::emit(SloAuditRecord)`. Activation: evaluate reaches the corresponding audit branch. Contract/effect: typed record carries event type, SLI/window/decision slugs, tenant, dedup key, and timestamp; sink failure follows typed error handling. Boundary: no durable sink is established by the trait. Validate API-004/INV-004 and source order; evidence: `alert.rs`, `audit.rs`.
[Relation index](#b02)

<a id="rel-019"></a>
### REL-019 — Page event into dispatcher port
Type/source call; page decision → constructed `PagerDutyEvent` → injected `PagerDutyDispatcher::dispatch`. Activation: `AlertDecision::is_page()` branch. Contract/effect: event action, service, key, severity, summary and runbook fields cross the trait. Failure: typed transport error enters dispatcher branch. Boundary: no HTTPS or PagerDuty acceptance is established. Validate API-005/INV-005; evidence: `alert.rs`, `pagerduty.rs`, `error.rs`.
[Relation index](#b02)

<a id="rel-020"></a>
### REL-020 — Schema version export
Type/public contract; `lib.rs::slo_schema_version()` → importing caller. Activation: source import/query. Contract/effect: returns constant `1`. Failure: changed integer can affect callers' version gates. Boundary: no persisted schema or migration follows from the constant alone. Validate API-006 and root source; evidence: `lib.rs`.
[Relation index](#b02)

<a id="rel-021"></a>
### REL-021 — Audit taxonomy export
Type/data; `canonical_slo_audit_event_strings()` and `SloAuditEventType::as_str()` → caller-side event classification. Activation: caller imports or evaluates the taxonomy. Contract/effect: five stable strings and record fields are exposed. Failure: altered literal/field affects classifiers. Boundary: source strings do not prove emitted or persisted events. Validate API-004; evidence: `audit.rs`.
[Relation index](#b02)

<a id="rel-022"></a>
### REL-022 — Error taxonomy and conversion
Type/data; `SloPagerDutyDispatchError::Transport` → `SloError::Dispatcher`. Activation: typed conversion or error match. Contract/effect: preserves typed dispatch failure category. Failure: changed variant/conversion affects error routing. Boundary: no retry or recovery behavior is implied. Validate API-006; evidence: `error.rs`.
[Relation index](#b02)

<a id="b03"></a>
## B03 — Transitive propagation

The located source path is calculation/alert contracts → direct declared
consumer imports or the telemetry facade → each downstream caller. No all-path
resolution or runtime chain is claimed. For API-001, inspect REL-006/009; for
metric-base strings inspect REL-007/008; for decisions and dispatch inspect
REL-001–005. Any additional source consumer found during change review adds a
separate relation or a justified exclusion.
[Relation index](#b02)

<a id="b04"></a>
## B04 — Change, impact, validation

Definition/SLI edits can affect metric labels and located consumers
(REL-001/007–017); window/calculator edits affect decision classification
(REL-002/003); alert ordering affects the injected audit boundary
(REL-004/018/021); page routing/event edits affect the injected dispatcher
(REL-005/019); schema/error changes affect callers (REL-020/022); facade and
manifest changes affect imports/resolution (REL-006, REL-009–017). Use the
corresponding REFERENCE invariant and maintenance procedure before validation.
No runtime claim follows from a green source/document gate.
[Relation index](#b02)

<a id="b05"></a>
## B05 — Coverage statement

Declared relationship classes reviewed: local source calls, telemetry
re-export, two located metric-binding tests, and located direct consumer
manifests. Counts are not certified exhaustive because the historical consumer
search and resolved target/feature graphs have not been independently repeated.
No exclusions are presented as proof that no other relation exists.
[Relation index](#b02)

<a id="b06"></a>
## B06 — Unknowns and stop conditions

Unknown: complete inverse graph, resolved features, selected consumers,
Prometheus ingestion/rules, runtime input rates, PagerDuty configuration or
delivery, audit persistence, secrets, deployment, and incidents. Stop any claim
that depends on these facts and request scoped evidence from the relevant owner.
[Relation index](#b02)
