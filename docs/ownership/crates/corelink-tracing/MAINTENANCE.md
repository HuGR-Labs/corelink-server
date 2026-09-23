---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-tracing
manifest: crates/corelink-tracing/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: tracing-static-source-20260920
---

# corelink-tracing — maintenance

Static-analysis procedures only. Each mode and evidence state is explicit: SOURCE means inspected static text, never runtime or executed proof.

[Baseline](#m01) · [Scope](#m02) · [Context](#m03) · [Sampling](#m04) · [Lifecycle](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline control

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SOURCE | Read assigned SHA, branch, manifest, and changed paths | Stop on baseline drift; obtain a reconciled baseline without reset |

<a id="m02"></a>
## M02 — Scope selection

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SOURCE | Map the request to the seven modules and root exports | Stop when it requests an adapter, transport, deployment, or operational result |

<a id="m03"></a>
## M03 — Context change

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SOURCE | Compare API-001 / INV-001: context constants, constructor, parser, formatter, and named property-test source | Stop before inferring header receipt or propagation; record that unknown |

<a id="m04"></a>
## M04 — Sampling change

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SOURCE | Review API-002 / INV-004: rate validation, decision mapping, audit emission, and decision consumers | Stop before claiming a configured, observed, or production rate |

<a id="m05"></a>
## M05 — Lifecycle, exporter, or fake change

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SOURCE | Trace API-003/004 and INV-002/003: service ordering, span mutation, trait signatures, error branches, and fake behavior | Stop before asserting OTLP, Tempo, audit durability, or test execution; route runtime questions to an authorized owner |

<a id="m06"></a>
## M06 — Static handoff
Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004).

| Execution mode | Evidence state | Procedure | Stop / recovery |
|---|---|---|---|
| LOCAL_ISOLATED | EXECUTED_LOCAL | Run the four checked-in document checks below; report structural results with paths and unknowns | It cannot certify semantic completeness, cold review, runtime, or executed approval |

```bash
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-tracing/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-tracing/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-tracing/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-tracing/MAINTENANCE.md
```

**Invariants:** preserve the R01–R07 falsifiable source claims. **Success/Completeness/Quality:** use the closure criteria in [REFERENCE R08](REFERENCE.md#r08). Unknowns remain explicit until separately evidenced.

<a id="proc-001"></a>
### PROC-001 — Review a tracing contract change
**Objective/trigger:** context, sampler, span, service, audit, or exporter source changes.

**Preconditions/inputs:** pinned source, changed symbols, diff, R04 APIs, R05 invariants, B01–B06. **Mode/environment/permissions:** `READ_ONLY`; local source only, no build or runtime permission.

**Steps:** 1. Trace changed symbols through local calls and exports. 2. Update matching API, invariant, and relation records. 3. Keep consumers and runtime claims unknown without evidence.

**Expected predicate:** changed boundaries have source paths, falsifier, and REL ID. **Stop:** resolved graph, transport, deploy, or execution required. **Recovery:** remove unsupported claim and route the evidence gap. **Evidence:** source paths, diff, IDs.

**Review:** REVIEWED. **Execution:** REVIEWED_NOT_EXECUTED; no code executed.
[Procedure index](#m03)

<a id="proc-002"></a>
### PROC-002 — Validate ownership documents
**Objective/trigger:** assigned artifact edits are ready for handoff. **Preconditions/inputs:** root, checker, four paths, current diff. **Mode/environment/permissions:** `LOCAL_ISOLATED`; local Python, docs only.

**Steps:** 1. Run the four commands in M06. 2. Run `git diff --check`. 3. Record outputs and changed paths.

**Expected predicate:** four passes and whitespace-clean diff. **Stop:** checker failure or out-of-scope paths. **Recovery:** fix owned docs and rerun checks. **Evidence:** JSON and diff output.

**Review:** REVIEWED. **Execution:** REVIEWED_NOT_EXECUTED; record actual results at handoff.
[Procedure index](#m03)

<a id="proc-003"></a>
### PROC-003 — Trace failure and observability behavior
**Objective/trigger:** audit, sampler, service mutex, exporter, error mapping, or observability claim changes. **Preconditions/inputs:** pinned `service.rs`, `error.rs`, `audit.rs`, `exporter.rs`, exact changed diff, API-002–004, INV-002/003, REL-004/005. **Mode/environment/permissions:** `READ_ONLY`; no sink, network, or production permission.

**Steps:** 1. Follow each typed error from its source to its caller/return branch. 2. Check whether audit failure precedes ledger mutation and whether exporter failure is recorded in the local counter/audit branch. 3. Name the observable field/event and its owner; keep external exporter, metric, alert, and durable audit delivery unknown. 4. Update the matching API, invariant and blast relation together.

**Expected predicate:** failure effects, propagation, and observability claims match source ordering; local counter/event is not called a delivered metric/alert. **Stop:** any claim needs executing a failing sink, remote transport, HTTP propagation, or deployed telemetry. **Recovery:** narrow the claim to SOURCE and route transport/operations questions to the composition/provider owner through verified OKF. **Evidence:** revision, line anchors, error variants, branch, relation IDs. **State:** `REVIEWED_NOT_EXECUTED` unless separately evidenced.
[Procedure index](#m02)

<a id="proc-004"></a>
### PROC-004 — Validate and hand off changed documents
**Objective/trigger:** any ownership artifact byte changes. **Preconditions/inputs:** repo root, current artifact paths, checker, diff. **Mode/environment/permissions:** `LOCAL_ISOLATED`; local documentation checks.

**Steps:** 1. Run each profile-S checker command in M06 for the exact four artifacts. 2. Run scoped `git diff --check` and inspect changed paths. 3. Record command/output, source SHA, and artifact hashes. 4. Request an independent cold review of the final bytes; on a failed verdict, resolve the finding and repeat the changed-artifact review.

**Expected predicate:** all structural checks pass and no unowned path is attributed to this package; structural success is not semantic approval. **Stop:** checker failure, stale source pin, missing reviewer, or request for code/runtime operation. **Recovery:** correct owned docs and rerun; route source drift to integration owner, runtime questions to authorized composition/operations owner. **Evidence:** exact outputs, hashes, reviewer verdict, unresolved items. **State:** local execution only when actually run; cold review remains a separate status.
[Procedure index](#m02)

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
