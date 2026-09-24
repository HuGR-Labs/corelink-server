---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-stripe-traits
manifest: crates/corelink-billing-stripe-traits/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-traits-structural-normalization-20260921
---

# corelink-billing-stripe-traits — maintenance

Source-static procedures for the leaf trait package. They neither run nor authorize Cargo, tests, webhook traffic, D1 operations, secret use, deployment, publication, or recovery against an external service.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Predicates](#m04) · [Recovery limits](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Safe preparation

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Work from the recorded source revision in a scoped checkout. Before change, record branch, porcelain status, the four intended artifact paths, `Cargo.toml`, and `src/lib.rs`. Do not reset unrelated work or inspect credentials.

**Stop:** the baseline, package identity, or writable-path scope differs. **Recovery:** stop without modifying unrelated paths and obtain the correct scope. **Evidence:** revision, status, and inspected paths.

<a id="m02"></a>
## M02 — Procedure selection

| Condition | Procedure mode | Predicate | Stop |
|---|---|---|---|
| A trait method or bound changes | [PROC-001](#proc-001) STATIC_PORT | Exact signature and affected relation identified | Implementation or selected-build evidence is needed |
| A type, label, field, or helper changes | [PROC-002](#proc-002) STATIC_REPRESENTATION | Source-visible shape impact enumerated | External compatibility is required |
| A manifest dependency changes | [PROC-003](#proc-003) STATIC_LEAF | No `corelink-*` dependency is introduced | Leaf invariant is violated or graph decision is needed |
| Alias or consumer impact is requested | [PROC-004](#proc-004) STATIC_GRAPH | Observed imports/re-exports separated from unknowns | Full consumer graph is required |
| Documentation is ready | [PROC-005](#proc-005) STATIC_HANDOFF | Four records and their navigation are internally consistent | Checker or scope predicate fails |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Preserve a port contract

**Mode:** STATIC_PORT. **Prerequisites:** source revision and targeted port are known. **Predicate:** the current and proposed `Debug + Send + Sync` bounds, method names, parameter order/types, and return types are explicitly compared.

1. Read R03 and the matching B03 relation.
2. Record each changed type and whether a default method is affected.
3. Keep implementor and caller compatibility outside the conclusion unless independently evidenced.

**Stop:** an implementation, generated binding, selected target, or behavior decision is necessary. **Recovery:** defer that decision to the implementation or consumer owner. **Evidence:** source paths, before/after signatures, relation identifiers, and unknowns. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Assess a representation change

**Mode:** STATIC_REPRESENTATION. **Prerequisites:** the altered enum, struct, helper, label, or response mapping is identified. **Predicate:** affected variants, fields, constructor parameters, or numeric mapping are enumerated from source.

1. Read R04 and INV-002 or INV-004 as applicable.
2. Identify the corresponding B05 relation and static aliases in B04.
3. State that Rust API shape is the evidence boundary.

**Stop:** a claim depends on Stripe events, JSON wire compatibility, persisted records, HTTP routes, or an external service. **Recovery:** obtain bounded evidence from the owning integration/data path; do not infer it from these types. **Evidence:** source symbols and documented unverified boundary. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Preserve the leaf invariant

**Mode:** STATIC_LEAF. **Prerequisites:** the manifest dependency section is inspected. **Predicate:** no package dependency name beginning `corelink-` appears in this package manifest.

1. Inspect every dependency section in `Cargo.toml`.
2. Compare the result with INV-001 and REL-002.
3. Record any proposed first-party edge as an architecture decision rather than a routine edit.

**Stop:** a first-party dependency is present or proposed. **Recovery:** leave the manifest unchanged and seek a graph/cycle decision. **Evidence:** manifest path, section inventory, and invariant verdict. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Reconcile static aliases and consumers

**Mode:** STATIC_GRAPH. **Prerequisites:** a changed symbol is identified. **Predicate:** direct manifest edges, materializer imports, and named re-exports are listed separately from unsearched or unselected consumers.

1. Inspect the paths listed by REL-007 through REL-009.
2. Record whether the change affects direct trait imports or a compatibility alias.
3. Retain selected features, full reverse graph, and runtime reachability as unknown.

**Stop:** compatibility approval requires every consumer or an executing build. **Recovery:** request fresh graph/build evidence from the responsible owner. **Evidence:** paths searched, each observed relation, and coverage limit. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Hand off scoped documentation

**Mode:** STATIC_HANDOFF. **Prerequisites:** only the four owned artifacts changed. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; manifests/source support material claims; cross-links resolve.

1. Run the declared documentary checker once for each of skill, reference, blast radius, and maintenance artifacts.
2. Run `git diff --check` and inspect the changed-path list.
3. Report static validation only, with no build/runtime/cold-review assertion.

**Stop:** any structural check, link, or scope predicate fails. **Recovery:** repair only the owned artifact; do not change source, manifests, shared indices, or registries to obtain a pass. **Evidence:** exact commands/verdicts, diff check, revision, and changed paths. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Predicates and evidence matrix

| Change class | Required predicate | Evidence | Stop / recovery |
|---|---|---|---|
| Trait ports | Exact bounds and signatures are compared | R03, B03 | Implementation decision: responsible owner |
| Types and labels | Shape and helper mapping are enumerated | R04, R05, B05 | External compatibility: integration/data evidence |
| Dependency manifest | No first-party dependency appears | INV-001, REL-002 | Graph decision: preserve leaf |
| Aliases/consumers | Observed locations separate from unknowns | R06, B04, B06 | Full graph: acquire fresh evidence |
| Documentation | Four structural checks and diff check pass | PROC-005 record | Repair owned docs only |

<a id="m05"></a>
## M05 — Recovery limits

| Boundary | Recovery action | Evidence needed | Do not assume |
|---|---|---|---|
| Port API change | Pause shared-contract decision | Implementor/caller review | A source trait edit preserves implementations |
| Representation change | Obtain integration or data evidence | Actual consumer/format plan | Rust fields prove wire compatibility |
| Leaf violation | Escalate architecture/cycle decision | Fresh graph analysis | A manifest entry is harmless |
| Alias change | Review named provider and direct imports | Selected consumer evidence | Re-export proves universal migration |
| Operational request | Transfer to system owner | Authorized operation evidence | Static docs authorize runtime recovery |

<a id="m06"></a>
## M06 — Handoff and explicit unknowns

Handoff includes mode, predicate result, stop/recovery decision, source and manifest paths, baseline, changed paths, four checker verdicts, `git diff --check`, and explicit unknowns. Required unknowns are selected targets/features, complete consumer graph, API compatibility of callers, concrete implementations, data or wire formats, Stripe, D1, FFI, metric/audit delivery, routes, secrets, runtime reachability, deployment, and cold review.

An author documentary pass is not a build, semantic approval, runtime observation, or cold review.

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Start](#m01)
