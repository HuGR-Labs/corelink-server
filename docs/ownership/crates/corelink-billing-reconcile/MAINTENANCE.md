---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-reconcile
manifest: crates/corelink-billing-reconcile/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: draft
evidence_set: billing-reconcile-static-source-20260920
---

# corelink-billing-reconcile — maintenance

Static-analysis procedure guide. It does not authorize running the binary,
Cargo, a schedule, D1, Stripe, a page/ticket system, or production operations.

[Baseline](#m01) · [Scope](#m02) · [Ladder](#m03) · [Effects](#m04) · [CLI](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline control

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | assigned worktree and baseline are known | inspect manifest, status, and module inventory | stop on divergent baseline; obtain the reconciled baseline without reset | SHA, branch, manifest |

<a id="m02"></a>
## M02 — Scope a requested change

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | target is package module, re-export, or same-package binary | map it to R03 and its direct REL/API records | stop if a dependency implementation or runtime adapter must change; route to that owner | `src/lib.rs`, R02–R04, B01–B02 |

<a id="m03"></a>
## M03 — Assess drift ladder or auto-fix change

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | threshold/config/gate change is explicit | trace constants, constructor checks, pairwise arithmetic, quiet branch, and property-test source | stop if production policy or compatibility at boundaries is required; record unknown and escalate | `event.rs`, `drift.rs`, `reconciler.rs`, property-test source |

<a id="m04"></a>
## M04 — Assess audit, history, or pause change

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | trait or orchestration-order change is explicit | verify source order: run-start audit → decision audit → history insert → SEV-1 pause | stop if durability, transactions, D1, Stripe, alerting, or rollback is needed; escalate instead of inferring it | `reconciler.rs`, `audit.rs`, `history.rs`, `stripe_pause.rs`, REL-008–010 |

<a id="m05"></a>
## M05 — Assess report or CLI change

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | input schema, output, marker, fail floor, or exit mapping changes | trace args/read/parse/pass/write/marker/exit branches and inspect bin-test source | stop if scheduler, file ownership, workflow, exit-code consumer, page/ticket, or real data is required | `run.rs`, binary source, `billing_reconcile_run_bin.rs`, REL-011–012 |

<a id="m06"></a>
## M06 — Static handoff

| Mode | Prerequisite / expected predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SOURCE | assessment is complete | report baseline, changed surfaces, source paths, relations, predicates, checks actually run, and unknowns | stop before claiming compilation, execution, real totals, schedule, D1, Stripe pause, credentials, production reconcile, page/ticket, or review; retain boundary for runtime owner | R08, B06, this record |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
