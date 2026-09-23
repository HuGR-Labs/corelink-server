---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-runbook-tracker
manifest: crates/corelink-runbook-tracker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: runbook-tracker-static-source-20260920
---

# corelink-runbook-tracker — maintenance

Static ownership procedures only. An evidence mode records inspected text or
document structure, never an operational runbook drill or execution result.

[Baseline](#m01) · [Surface](#m02) · [Record](#m03) · [Cadence](#m04) · [Boundary](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Confirm baseline

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_MANIFEST | SHA and package path match this record | Read branch, manifest, and `src/lib.rs` | Stop on a different baseline; obtain the intended snapshot |

<a id="m02"></a>
## M02 — Map the root surface

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_SOURCE | A public declaration or dependency changes | Map it to R02, R03–R07, and B01–B05 | Stop when a consumer, adapter, or operation is required; record it unknown |

<a id="m03"></a>
## M03 — Maintain record and outcome predicates

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_SOURCE | Identifier, record, error, ratio, or outcome changes | Compare validation sequence, failure precedence, strict ratio, label, and trigger predicate | Stop before asserting an emitted record, post-mortem, or evidence delivery |

<a id="m04"></a>
## M04 — Maintain cadence and scan predicates

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_SOURCE | Window, timestamp, recorder, or alert fields change | Trace fixed seconds, saturation, exact boundary, lookup error, and missing-history branch | Stop before asserting a clock, catalog, schedule, store, or alert path |

<a id="m05"></a>
## M05 — Preserve evidence boundary

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_RELATION | A source relation is reported | Name endpoints, falsifier, evidence mode, and explicit unknown | Stop if a relation is relabeled as execution; route operational facts externally |

The five axioms and unknowns in [R08](REFERENCE.md#r08) are mandatory: source
and manifest do not establish invocation, adapter behavior, or drill operation;
the verified OKF link remains reference-only.

<a id="m06"></a>
## M06 — Documentary handoff

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| DOCUMENTARY_CHECK | Four artifacts are the only changed paths | If separately provisioned, run the external candidate checker and the committed-artifact diff check; otherwise report BLOCKED with no checker pass | A pass is structural only; stop before semantic approval or operational claims |

```bash
test -f "$CHECKER" || { echo "BLOCKED: candidate checker is not provisioned"; exit 2; }
python3 "$CHECKER" --kind skill --profile S --root . .claude/skills/own-corelink-runbook-tracker/SKILL.md
python3 "$CHECKER" --kind reference --profile S --root . docs/ownership/crates/corelink-runbook-tracker/REFERENCE.md
python3 "$CHECKER" --kind blast_radius --profile S --root . docs/ownership/crates/corelink-runbook-tracker/BLAST_RADIUS.md
python3 "$CHECKER" --kind maintenance --profile S --root . docs/ownership/crates/corelink-runbook-tracker/MAINTENANCE.md
git diff --check 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6..HEAD
```

`$CHECKER` is an external, unintegrated candidate path: it is not stable or
provisioned by this repository. If it is absent, report BLOCKED and do not claim
a pass. The four checker results and `git diff --check` are structural-only;
they are not semantic completeness, cold review, execution, or proof of
runbook operation. [Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01)
· [Start](#m01)
