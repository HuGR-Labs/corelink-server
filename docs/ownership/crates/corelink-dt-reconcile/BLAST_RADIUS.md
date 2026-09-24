---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dt-reconcile
manifest: tools/dt-reconcile/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: author_validated
evidence_set: w013-dt-reconcile-static-source-20260921
---

# corelink-dt-reconcile — blast radius

This is a SOURCE-only map of atomic directional relations in `tools/dt-reconcile/Cargo.toml` and `tools/dt-reconcile/src/main.rs`. A dependency is a manifest/import edge, a flow is local source control/data text, and an impact is a review obligation. None proves reconciliation, data effect, provider state, runtime reachability, or delivery. The verified canonical OKF is route-only through the [Wave 013 plan](../../WAVE_013_PLAN.md).

[Scope](#b01) · [Imports](#b02) · [Local flow](#b03) · [Replay branches](#b04) · [Target](#b05) · [Unknowns](#b06).

<a id="b01"></a>
## B01 — Scope and reading rule

Read every relation directionally. A manifest dependency does not establish a resolved package graph; an imported method call does not establish its implementation or an invocation; a local result/output value does not establish data; a comment/URL/environment-variable read does not establish a provider or reachable runtime. B06 unknowns override naming-based inference.

<a id="b02"></a>
## B02 — Declared dependency relations

[REL-001](#rel-001) · [REL-002](#rel-002).

<a id="rel-001"></a>
### REL-001 — Manifest to webhook import

**Dependency:** `tools/dt-reconcile/Cargo.toml` declares `corelink-dt-webhook`; `tools/dt-reconcile/src/main.rs` imports queue, handler, signing, and trait names. **Flow:** local source constructs imported values and calls their interfaces. **Impact:** an imported contract change can affect local source compatibility. **Evidence:** target manifest and source. **Limit:** this does not establish the imported implementation, a provider, queue durability, or delivery. [Index](#b02) [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Supporting dependency declarations

**Dependency:** the manifest declares `thiserror`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, and `tokio`. **Flow:** source uses JSON serialization, tracing macros/subscriber setup, and Tokio’s main attribute. **Impact:** declaration or source-shape changes can affect static target compatibility. **Evidence:** target manifest and source. **Limit:** no dependency resolution, compilation, emitted telemetry, or async execution is proven. [Index](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Local result and gap relations

[REL-003](#rel-003) · [REL-004](#rel-004). [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Local count literals to result fields

**Dependency:** local variables `dt_findings_count` and `confirmed_deliveries` feed the saturating subtraction expression. **Flow:** the calculated local `gap_count`, together with local `replayed`, populates `ReconcileResult`. **Impact:** changing literals, arithmetic, or field mapping changes the source-local result shape/branch input. **Evidence:** `tools/dt-reconcile/src/main.rs`. **Limit:** this does not establish observations, a reconciliation result, or data correctness. [Index](#b03)

<a id="rel-004"></a>
### REL-004 — Environment reads to local constructor input

**Dependency:** source reads three named environment variables with fallback branches. **Flow:** the secret byte vector is cloned for handler construction and passed to imported signing; URL/key bindings retain underscore-prefixed local names. **Impact:** changing names/default branches/call arguments changes static configuration handling. **Evidence:** `tools/dt-reconcile/src/main.rs`. **Limit:** no value, secret, authentication, endpoint, or provider reachability is established. [Index](#b03)

<a id="b04"></a>
## B04 — Queue and handler source branches

[REL-005](#rel-005) · [REL-006](#rel-006).

<a id="rel-005"></a>
### REL-005 — Imported queue entries to handler call

**Dependency:** local iteration consumes values returned by imported `drain_all`; each entry contains an imported event. **Flow:** source serializes that event, signs bytes, sets a signature, and awaits `handle_webhook`. **Impact:** changing event mapping/order/signature setup changes imported-call compatibility. **Evidence:** `tools/dt-reconcile/src/main.rs`. **Limit:** no entry exists, is drained, is accepted, or is delivered by this evidence. [Index](#b04) [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Handler error branch to conditional push call

**Dependency:** imported handler `Err` and entry `attempt_count` select a local branch. **Flow:** source constructs an imported entry and calls `push` only below the literal threshold; otherwise it selects a log call. **Impact:** threshold/field/branch changes alter static retry-source relations. **Evidence:** `tools/dt-reconcile/src/main.rs`. **Limit:** no queue mutation, retry, alert, or manual review is demonstrated. [Index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Target and observable-source compatibility

[REL-007](#rel-007) · [REL-008](#rel-008). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Manifest target to main source

**Dependency:** the manifest declares bin name `corelink-dt-reconcile` and `src/main.rs`; that source contains `main`. **Flow:** `main` initializes local tracing setup, awaits the private function, logs branches, and contains three explicit exit selections. **Impact:** name/path/signature/branch changes can affect static build or caller expectations. **Evidence:** target manifest and source. **Limit:** no compilation, process launch, exit observation, scheduler, or deployment is proven. [Index](#b05) [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Result branch to source log/exit selection

**Dependency:** outer `Result` and local `gap_count` govern nested match/if source branches. **Flow:** each branch contains a tracing macro call and `std::process::exit` literal. **Impact:** predicate or exit/log changes alter source-visible operational intent. **Evidence:** `tools/dt-reconcile/src/main.rs`. **Limit:** log emission, alerting, process result, and external response are unknown. [Index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage, unknowns, and done gate

Coverage includes the complete target manifest/source boundary, its single declared CoreLink dependency edge, local environment/configuration branches, count/result mapping, imported queue/handler call relations, and binary/exit source relation. Unknowns include resolved features, actual findings/deliveries/gaps, reconciliation, data effects, queue content/durability, handler/signature result, alert/metric/log receipt, provider state/reachability, credentials, schedule, invocation, compilation, tests, reverse consumers, deployment, and cold review.

Success is atomic source-backed relations with a falsifier-sized limit. Completeness means B01–B05 cover each scoped relation without inventing a runtime edge. Quality means directional evidence and unknowns remain explicit. Definition of done is companion [R08](REFERENCE.md#r08), [M06](MAINTENANCE.md#m06), four structural checks, baseline diff hygiene, scope review, and independent review.

[Back to start](#b01)
