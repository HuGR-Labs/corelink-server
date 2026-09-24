---
name: own-corelink-billing-reconcile
description: >-
  Own corelink-billing-reconcile when changing its three-layer drift logic,
  decision ladder, audit/history/pause trait surfaces, JSON pass/report logic,
  or billing-reconcile-run CLI; do not use it to assert or operate Cloudflare,
  D1, Stripe, credentials, paging, tickets, or production reconciliation.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-billing-reconcile"
  manifest: "crates/corelink-billing-reconcile/Cargo.toml"
  source-commit: "9f372cc1f5a34752a6b6eb4674df10b3921bce88"
  evidence-set: "billing-reconcile-static-source-20260920"
---

# Ownership — corelink-billing-reconcile

Static-source ownership guide. A trait, in-memory fake, binary, comment, or
test source is not evidence of an activated scheduler, D1, Stripe action,
credential, ticket/page, or production reconciliation.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Decision](#s04) · [Flow](#s05) · [Stop](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A three-layer total, threshold, decision, or config changes | Read R04–R05 and trace `event.rs`, `drift.rs`, and `reconciler.rs` | `LayerTotals`, `ReconcileConfig`, `compute_max_drift`, `BillingReconciler` | The request relies on an unverified input extractor or downstream consumer |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep work in the package library and its declared `billing-reconcile-run` binary | manifest; `src/lib.rs`; `src/bin/billing-reconcile-run.rs` | Treat direct package dependencies (`emit`, `aggregator`, `stripe`) as implementation, runtime wiring, or reverse consumers |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Need contract, impact, or procedure | Read [reference](../../../docs/ownership/crates/corelink-billing-reconcile/REFERENCE.md#r04), then [blast radius](../../../docs/ownership/crates/corelink-billing-reconcile/BLAST_RADIUS.md#b01), then maintenance | module and manifest paths cited there | Infer missing features, resolved graph, runtime path, or operations from static source |

<a id="s04"></a>
## S04 — Decision and invariant changes

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Threshold, auto-fix gate, or decision changes | Preserve strict config ordering and source classification (`<=` quiet/SEV boundaries; auto-fix requires nonzero drift and both bounds) | `event.rs`, `drift.rs`, `reconciler.rs`, `tests/prop_billing_reconcile.rs` | An asserted boundary or consumer compatibility is not traced |
| Audit, history, or pause changes | Trace `run_started` audit, decision audit, history insertion, then SEV-1 pause in that order | `reconciler.rs`; R05 | A durable outbox/D1/Stripe mutation or operator action is required |

<a id="s05"></a>
## S05 — Flow and CLI changes

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| JSON input, report, marker, or exit behavior changes | Trace `parse_input` → `run_reconcile_pass` → report write → marker/exit selection | `run.rs`; `src/bin/billing-reconcile-run.rs`; bin-test source | Claim a workflow invokes the binary, consumes a marker, or performs a response action |

<a id="s06"></a>
## S06 — Stop conditions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work needs Cloudflare schedule, D1 totals/history, real Stripe pause, credentials, page/ticket, or production data | Escalate to the relevant runtime/operator owner with the required endpoint and predicate | R08; B06; M06 | Do not replace missing operational evidence with comments, fakes, tests, or compilation |

<a id="s07"></a>
## S07 — Handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static assessment/change is complete | Report baseline, source paths, affected API/invariant/relation, checks actually run, and unknowns | [maintenance](../../../docs/ownership/crates/corelink-billing-reconcile/MAINTENANCE.md#m06) | Present static checks as runtime, security, compatibility, deployment, or review approval |

[Back to trigger](#s01)
