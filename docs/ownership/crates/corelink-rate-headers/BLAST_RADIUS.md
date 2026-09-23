---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-rate-headers
manifest: crates/corelink-rate-headers/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: S
state: author_validated
evidence_set: rate-headers-source-static-20260920
---

# corelink-rate-headers — blast radius

This is a bounded SOURCE relation map. Each arrow is static text, not proof of
compilation, invocation, HTTP emission, provider/D1 activity, runtime, or
deployment.

[Header route](#b01) · [Circuit route](#b02) · [Audit route](#b03) ·
[Metric route](#b04) · [Consumer references](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Header-to-root relation

`src/headers.rs` → `src/lib.rs` public re-export is the local header-surface
route. A changed kind, policy, builder, error body, or exported constant can
alter source-visible import paths; no rendered HTTP header is evidenced.

<a id="b02"></a>
## B02 — Circuit-to-root relation

`src/circuit.rs` → `src/lib.rs` public re-export routes the state, thresholds,
decision, trait, in-memory breaker, and helper functions. A source-path change
can affect callers' type references; it does not show a circuit ran.

<a id="b03"></a>
## B03 — Audit-to-circuit relation

`src/audit.rs` → `src/circuit.rs` is a local trait/type import relation for
`CircuitAuditRecord`, `CircuitAuditSink`, and `CircuitEventType`. Changing that
source contract requires local circuit assessment; it does not prove an audit
was persisted or delivered.

<a id="b04"></a>
## B04 — Metric-to-circuit relation

`src/metrics.rs` → `src/circuit.rs` is a local observer-trait import relation.
Changes to metric kinds or observer methods affect the source seam only; no
metric backend, scrape, alert, or observed value is established.

<a id="b05"></a>
## B05 — Bounded consumer-reference relations

[REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007). [Relation index](#b05)

<a id="rel-005"></a>
### REL-005 — billing re-export consumer

**Shared fingerprint:** `repo:1232040291:boundary:billing-rate-headers-export-001`. [corelink-billing REL-008](../corelink-billing/BLAST_RADIUS.md#rel-008) `crates/corelink-billing/src/rate_headers.rs` → `corelink_rate_headers::*` is a
single source re-export relation. It can change a source-visible billing import
path; it does not show billing compiled, invoked a header builder, or emitted
HTTP. [Back to B05](#b05)

<a id="rel-006"></a>
### REL-006 — e2e-resilience manifest dependency

**Shared fingerprint:** `repo:1232040291:boundary:e2e-resilience-manifest->corelink-rate-headers-normal`. [e2e-resilience REL-011](../e2e-resilience/BLAST_RADIUS.md#rel-011) `tests/e2e-resilience/Cargo.toml` → `corelink-rate-headers` is one declared
manifest dependency relation. It can change the harness's declared package
input; it does not establish dependency resolution, compilation, or execution.
[Back to B05](#b05)

<a id="rel-007"></a>
### REL-007 — e2e-resilience circuit scenario consumer

**Shared fingerprint:** `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-rate-headers-api-v1`. **Consumer arrow:** [e2e-resilience](../e2e-resilience/BLAST_RADIUS.md#rel-012) scenario source → this crate's circuit types, constants, constructors, observation/check/probe/snapshot methods; source `scenarios.rs:28-35,81-139,222-342,388-471`. **Data:** thresholds/observations/time → state, decision, audit. **Activation:** source mention only; compilation, execution and runtime unknown. **Failure:** API/state changes may break harness assertions; local 503 versus upstream 429 remains unresolved. **Contract owner:** `corelink-rate-headers`. **Validation:** compare imports/calls/assertions; coordinate both owners. Evidence: pinned manifest/source at `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`. [Back to B05](#b05)

`tests/e2e-resilience/tests/scenarios.rs` → named `corelink_rate_headers`
imports/constants is one scenario-source mention relation. It can change a
checked-in source reference; it does not show a scenario ran, a circuit
transition occurred, or a provider/D1/runtime result. The source mention is
covered by REL-007; no duplicate relation is asserted.

<a id="b06"></a>
## B06 — Closure and unknowns

Coverage is the manifest, crate root, five local modules, declared test files,
and the three B05 relations (REL-005–REL-007). The [storage quota header](../../../knowledge/tenancy/storage-quota-header.md)
is the verified canonical OKF route, not a runtime edge and not revalidated.
Unknown: full reverse graph, resolved features, API adoption, execution, HTTP,
provider/D1 activity, durable effects, runtime, deployment, and review.

Atomic relation index in B06: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) [Index](#b06)

<a id="rel-001"></a>
### REL-001 — Header module re-export
**Producer → consumer:** `headers` public types/functions → crate root exports. **Activation:** crate import. **Contract/effect:** typed values and local string renderers become importable. **Failure:** export/type rename breaks callers. **Validation/coordination:** compare `lib.rs` exports and callers. **Evidence:** `src/lib.rs`, `src/headers.rs`; [API-001](REFERENCE.md#api-001). [Index](#b06)

<a id="rel-002"></a>
### REL-002 — Circuit module re-export
**Producer → consumer:** circuit public types/functions → crate root. **Activation:** caller imports circuit surface. **Contract/effect:** state, decisions, trait, fake and helpers exported. **Failure:** symbol changes break caller API. **Validation/coordination:** compare exports and trait; no instantiation proof. **Evidence:** `lib.rs`, `circuit.rs`; [API-002](REFERENCE.md#api-002). [Index](#b06)

<a id="rel-003"></a>
### REL-003 — Audit trait consumed by circuit
**Producer → consumer:** `audit::{CircuitAuditSink,CircuitEventType}` → circuit transition code. **Activation:** calls that emit records. **Contract/effect:** event record/error informs local result. **Failure:** sink error may fail closed; trait does not establish durable write. **Validation/coordination:** trace call and error branch. **Evidence:** `audit.rs`, `circuit.rs`. [Index](#b06)

<a id="rel-004"></a>
### REL-004 — Metrics observer consumed by circuit
**Producer → consumer:** `CircuitMetricsObserver` → circuit logic. **Activation:** signal, transition, rejection or recovery path. **Contract/effect:** typed metrics through injected trait. **Failure:** observer error matters only where propagated in source; backend unknown. **Validation/coordination:** follow each call. **Evidence:** `metrics.rs`, `circuit.rs`. [Index](#b06)

<a id="rel-008"></a>
### REL-008 — Embedded migration source
**Producer → consumer:** SQL migration → `MIGRATION_0014_GLOBAL_CIRCUIT_STATE` by `include_str!`. **Activation:** library constant read. **Contract/effect:** compile-time text inclusion/version constant; no DDL apply. **Failure:** path/content/schema drift changes embedded artifact. **Validation/coordination:** compare SQL, include and version function. **Evidence:** `src/lib.rs:220-230`; [API-003](REFERENCE.md#api-003). [Index](#b06)

<a id="rel-009"></a>
### REL-009 — Migration regression target
**Producer → consumer:** embedded migration/version → declared `migration_canonical_0014` test target. **Activation:** target selected/run, status unknown. **Contract/effect:** text DDL predicates and version assertion. **Failure:** SQL drift can fail assertions; a pass would not prove D1 apply. **Validation/coordination:** compare manifest, test and SQL. **Evidence:** `Cargo.toml`, `tests/migration_canonical_0014.rs`. [Index](#b06)

<a id="rel-010"></a>
### REL-010 — Header property target
**Producer → consumer:** public header API → declared `prop_rate_headers` test target. **Activation:** target selected/run, status unknown. **Contract/effect:** property assertions over local builder/taxonomy. **Failure:** API drift can fail property checks. **Validation/coordination:** compare test inputs/assertions with API-001. **Evidence:** manifest and test source. [Index](#b06)

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to header route](#b01)
