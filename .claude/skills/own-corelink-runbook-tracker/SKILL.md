---
name: own-corelink-runbook-tracker
description: >-
  Route source-only ownership changes to corelink-runbook-tracker validation,
  outcome selection, cadence arithmetic, and recorder contracts; exclude drill execution.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "corelink-runbook-tracker"
  manifest: "crates/corelink-runbook-tracker/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "runbook-tracker-static-source-20260920"
---

# Ownership — corelink-runbook-tracker

Static-source routing guide. SOURCE is not proof that a runbook drill was
performed, recorded, alerted, or operated.

[Trigger](#s01) · [Boundary](#s02) · [Identifier](#s03) · [Outcome](#s04) · [Cadence](#s05) · [Recorder](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| A tracker contract changes | Route it to the static records for this package | `Cargo.toml`; `src/lib.rs` | The request needs an operated drill or external adapter result |

<a id="s02"></a>
## S02 — Boundary

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Scope is uncertain | Retain only identifier, record, outcome, cadence, scan, and trait contracts | `src/lib.rs` | Infer persistence, scheduling, alerting, or execution |

Static five-axiom coverage: (1) manifest and source establish declarations only;
(2) signatures establish source-visible contracts only; (3) traits establish no
adapter invocation; (4) local branches establish no executed drill; (5) the
verified OKF SRE hub is a routing reference only, never revalidated here.

<a id="s03"></a>
## S03 — Identifier contract

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| `RunbookId` changes | Trace prefix, suffix character, and error predicates | [R03](../../../docs/ownership/crates/corelink-runbook-tracker/REFERENCE.md#r03) | Claim a catalog accepts the identifier |

<a id="s04"></a>
## S04 — Record and outcome contract

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Record fields or drift logic change | Reconcile validation, success precedence, ratio, labels, and trigger predicate | [R04](../../../docs/ownership/crates/corelink-runbook-tracker/REFERENCE.md#r04) | Claim a post-mortem or evidence delivery occurred |

<a id="s05"></a>
## S05 — Cadence contract

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Window or timestamp logic changes | Trace the fixed 30-day constant, saturation, and exact boundary | [R05](../../../docs/ownership/crates/corelink-runbook-tracker/REFERENCE.md#r05) | Treat a supplied timestamp as observed time |

<a id="s06"></a>
## S06 — Recorder and scan contract

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Trait or scan logic changes | Map record/lookup calls and overdue-alert fields as source relations | [B04](../../../docs/ownership/crates/corelink-runbook-tracker/BLAST_RADIUS.md#b04) | Claim a store, cron, or alert path exists or ran |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Static assessment is complete | Report baseline, changed source contract, relations, checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-runbook-tracker/MAINTENANCE.md#m06) | Call documentary checks operational proof or cold review |

[Reference](../../../docs/ownership/crates/corelink-runbook-tracker/REFERENCE.md#r01) · [Impact map](../../../docs/ownership/crates/corelink-runbook-tracker/BLAST_RADIUS.md#b01) · [Start](#s01)
