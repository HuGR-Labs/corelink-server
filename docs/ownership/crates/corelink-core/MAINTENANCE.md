---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-core
manifest: crates/corelink-core/Cargo.toml
source_commit: 5d617634662ee9475dc66cb83b5a57296eb75dc2
profile: S
state: author_validated
evidence_set: core-static-source-20260920
---

# corelink-core — maintenance

Source-static procedures for a scoped checkout. They define predicates and recovery records; they do not authorize runtime operations, deployment, release, or the use of secret values.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Predicates](#m04) · [Recovery](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Safe preparation

Use an isolated, clean checkout at the recorded source commit before changing ownership artifacts or source. Preserve the baseline SHA and changed-path list. Do not reset an active branch, load credential files, or use a secret value as a test fixture.

**Stop:** baseline mismatch or unrelated modifications. **Recovery:** stop without modifying unrelated work; reconcile the intended baseline and scope first. **Evidence:** SHA, branch, porcelain status, and inspected source paths.

<a id="m02"></a>
## M02 — Procedure selection

| Condition | Procedure mode | Predicate | Stop |
|---|---|---|---|
| Any of six public surfaces changes | [PROC-001](#proc-001) STATIC_SCOPE | Source files and affected contract identified | Scope reaches migration or runtime |
| Identity, region, or digest format changes | [PROC-002](#proc-002) STATIC_COMPAT | Text/width/case/set impact enumerated | Consumer or data compatibility unknown |
| Secret formatting or exposure changes | [PROC-003](#proc-003) STATIC_SECRET | No value is recorded; redaction boundary traced | Credential operation is requested |
| Error or clock semantics change | [PROC-004](#proc-004) STATIC_SEMANTICS | Variant/default conversion impact enumerated | Concrete implementation or runtime path required |
| Consumer impact is needed | [PROC-005](#proc-005) STATIC_GRAPH | Direct manifests are separated from unknowns | Static entries are presented as complete |
| Documentation change is ready | [PROC-006](#proc-006) STATIC_HANDOFF | Records, links, and status are internally consistent | Validation is presented as approval |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Establish source scope

**Mode:** STATIC_SCOPE. **Predicate:** baseline matches and the requested symbol maps to `lib.rs`, a listed type file, `errors.rs`, or `time.rs`.
1. Record baseline SHA and clean/dirty state.
2. Read the matching source plus `Cargo.toml`.
3. Identify only the matching reference and blast records.
**Stop:** symbol or source boundary is unclear. **Recovery:** leave code untouched and request a bounded ownership decision. **Evidence:** SHA, paths, records, and unknowns.
[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Assess representation compatibility

**Mode:** STATIC_COMPAT. **Predicate:** the change identifies tenant text, digest width/case/alphabet, or region vocabulary effect.
1. Compare the proposed behavior with API-001, API-002, or API-004.
2. List the matching relations from B04 and direct manifest consumers.
3. State whether compatibility or migration evidence is absent.
**Stop:** persisted or consumer compatibility cannot be established. **Recovery:** do not claim a safe revert; obtain a consumer/data plan. **Evidence:** before/after contract and affected static relations.
[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Preserve the secret boundary

**Mode:** STATIC_SECRET. **Predicate:** the change preserves explicit exposure and redacted debug behavior without recording plaintext.
1. Read API-005 and REL-009.
2. Use a non-sensitive sentinel only if source-level validation is later selected.
3. Keep credentials, logs, and runtime operations outside the task.
**Stop:** a request needs a real credential, log inspection, or external operation. **Recovery:** remove any sensitive material from the task record and escalate to the authorized operator. **Evidence:** source paths and redaction/exposure contract; no secret value.
[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Preserve error and time semantics

**Mode:** STATIC_SEMANTICS. **Predicate:** the change accounts for non-exhaustive error handling or default clock conversion. 1. Compare the proposed change to API-006 and INV-004. 2. Record effects on pre-epoch and overflow behavior when clock conversion changes. 3. Record which error boundary changes, without inventing HTTP or runtime mapping. **Stop:** an implementation, scheduler, or request path is required to decide semantics. **Recovery:** defer to its owning package with the static contract attached.

**Evidence:** source lines, affected invariant, and explicit runtime unknown. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Reconcile static consumers

**Mode:** STATIC_GRAPH. **Predicate:** each known direct manifest consumer is named separately from unobserved consumers.
1. Inspect the four manifests in B03.
2. Record ordinary or development declaration context when observable.
3. Keep selected features, targets, imports, legacy migration, and runtime paths as unknown.
**Stop:** a compatibility decision depends on a full reverse graph. **Recovery:** obtain a fresh graph and consumer-owner evidence before proceeding. **Evidence:** manifest paths, search scope, and limitations.
[Procedure index](#m02)

<a id="proc-006"></a>

### PROC-006 — Hand off static documentation

**Mode:** STATIC_HANDOFF. **Predicate:** all changed documents identify source commit, scope, contracts, relations, maintenance recovery, and unknowns.
1. Run the approved document checker for each document kind.
2. Run whitespace validation and retain exact verdicts.
3. Report only static evidence and non-certification limits.
**Stop:** a checker failure, broken navigation, or unsupported claim appears. **Recovery:** correct the scoped document; do not alter source or shared registries to obtain a pass. **Evidence:** checker output, diff check, SHA, and changed paths.
[Procedure index](#m02)


<a id="m04"></a>
## M04 — Predicates and evidence matrix

| Change class | Required predicate | Evidence | Stop / recovery |
|---|---|---|---|
| Identity/digest/region | Exact format or vocabulary effect is written | R04, R05, B04 | Unknown compatibility: seek consumer/data plan |
| Secret wrapper | Explicit exposure and redaction remain bounded | R04, B05 | Sensitive operation: authorized operator |
| Error/clock | Variant or saturation effect is explicit | R04, R05, B05 | Runtime decision: owning implementation |
| Consumer impact | Four direct manifests separated from unknowns | R06, B03, B06 | Full graph needed: acquire fresh evidence |
| Documentation | Structural checker and diff check pass | PROC-006 record | Repair scoped docs only |

No procedure in this document is evidence that a Cargo test, deployment, integration, cold review, or recovery operation occurred.

<a id="m05"></a>
## M05 — Recovery limits

| Boundary | Recovery action | Evidence needed | Do not assume |
|---|---|---|---|
| Representation change | Pause and coordinate compatibility | Prior/new contract and consumer evidence | A source revert repairs persisted text |
| Secret boundary | Stop exposure work and remove sensitive record | Sanitized task record | Debug redaction authorizes credential use |
| Clock semantics | Return decision to concrete-clock owner | Fixture/implementation evidence | Trait source proves runtime timing |
| Graph uncertainty | Rebuild evidence outside this static set | Fresh graph and selected targets | Manifest presence proves reachability |

<a id="m06"></a>
## M06 — Handoff and explicit unknowns

Handoff must include procedure mode, predicate result, stop/recovery decision, source paths, baseline, changed paths, checker verdicts, and unknowns. The required unknowns are legacy migration state, selected targets/features, complete consumer graph, concrete clock wiring, external effects, runtime reachability, deployment, and cold review.

Do not include secret values. A structural documentation pass is author validation only.

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Start](#m01)
