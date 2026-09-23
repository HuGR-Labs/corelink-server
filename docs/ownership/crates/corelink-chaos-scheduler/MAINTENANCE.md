---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-chaos-scheduler
manifest: crates/corelink-chaos-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: chaos-scheduler-static-source-20260920
---

# corelink-chaos-scheduler — maintenance

Static ownership procedures only. They authorize source/document inspection and external structural checking, never Cargo, test execution, scheduling, runtime operation, network, deployment, GitHub, or production access. The [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is routed without copying or revalidating it.

[Baseline](#m01) · [Surface](#m02) · [Catalog](#m03) · [Runner](#m04) · [Relations](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the static baseline

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_MANIFEST | Intended checkout is available | Confirm `3feae2baed63061354533ffdfe2d94acfd24aa9a`, manifest path, and four source paths | Stop on another baseline; obtain the intended static snapshot | SHA; `Cargo.toml`; `src/{lib,catalog,runner,types}.rs` |

<a id="m02"></a>
## M02 — Map the public surface

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | A root export, type, trait, dependency, or known inverse edge changes | Map the changed declaration to R02/R05/R06 and B01–B06; reconcile the `corelink-ops` re-export and `e2e-chaos` direct imports as source edges; state its falsifier | Stop if selected features, other consumers, adapters, or runtime results are needed; retain those as unknowns | `src/lib.rs`; `crates/corelink-ops/{Cargo.toml,src/chaos.rs}`; `tests/e2e-chaos` |

<a id="m03"></a>
## M03 — Maintain catalog and value predicates

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | Catalog, ID, label, FM, or bound changes | Trace exact rows, lookup/count behavior, labels, and the `is_ga_mandatory` match; name an absent-ID, duplicate-FM, or label-branch falsifier | Stop before claiming a real fault target, prerequisite, SLO, or rollback result | `src/catalog.rs`; `src/types.rs`; R03; B01 |

<a id="m04"></a>
## M04 — Maintain runner predicates and seams

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_RELATION | Snapshot, seed, outcome, event, or trait changes | Reconcile ordered safe-mode branches, FNV inputs, strict impact comparison, and trait call order; use 5,000/5,001 and bound/bound+1 falsifiers | Stop before asserting an executed guard, emitted event, measured SLO, archive, or rollback; route adapter facts externally | `src/runner.rs`; R04/R05; B02–B04 |

<a id="m05"></a>
## M05 — Preserve atomic relations and unknowns

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_RELATION | A relation is reported or queue/rotation changes | Name one endpoint chain, change impact, falsifier, evidence mode, and explicit unknown; preserve the five R08 axioms | Stop if a relation is relabeled as schedule, runtime, or external effect; recover by separating it as unknown | B01–B06; R08 |

<a id="m06"></a>
## M06 — Documentary handoff

| Mode | Prerequisite | Predicate and action | Stop / recovery | Evidence |
|---|---|---|---|---|
| DOCUMENTARY_CHECK | Only the four assigned ownership artifacts changed | Run the checked-in profile-S checker once per artifact and `git diff --check` against the fixed baseline; report all exit states and changed paths | If checker/scope/diff fails, report failure; never call a structural pass semantic approval or operational proof | `docs/ownership/tools/check_docs.py`; four checker outcomes; diff status; SHA; paths |

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-chaos-scheduler/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/MAINTENANCE.md
git diff --check 3feae2baed63061354533ffdfe2d94acfd24aa9a..HEAD
```

The checked-in checker's four results and the whitespace check are structural only. Handoff includes baseline, source paths, affected R/B/M IDs, falsifiers, changed paths, exact checker/diff results, and R08 unknowns. Success is a source-bounded packet; completeness requires S01–S07, R01–R08, B01–B06, M01–M06, and all four artifacts; quality keeps static evidence separate from scheduled/runtime claims.

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
