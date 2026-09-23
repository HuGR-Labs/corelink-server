---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-reconcile
manifest: crates/corelink-billing-reconcile/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: draft
evidence_set: billing-reconcile-static-source-20260920
---

# corelink-billing-reconcile — ownership reference

Static-source reference only. It does not prove a scheduled Cloudflare runner,
D1 totals/history, Stripe pause, ticket/page dispatch, credentials, or a
production reconciliation.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-billing-reconcile` / `crates/corelink-billing-reconcile/Cargo.toml` |
| Targets | library, declared `billing-reconcile-run` bin, property-test target; `billing_reconcile_run_bin.rs` is source-visible integration-test code |
| Role | typed three-layer drift classification, in-memory orchestration seams, JSON pass/report helpers, and CLI glue |
| Direct package dependencies | `corelink-billing-emit`, `corelink-billing-aggregator`, `corelink-billing-stripe` in the manifest |
| Activation | package/binary declarations are static; scheduler, invocation, features, resolved graph, and runtime are unknown |

<a id="r02"></a>
## R02 — Boundary

The package owns `audit`, `drift`, `error`, `event`, `history`, `reconciler`,
`run`, and `stripe_pause`, re-exported through `lib.rs`, plus its same-package
binary. The three billing dependencies are manifest edges; their source is not
implemented here. `InMemory*` types are local fakes, not durable services.
Comments naming a Cloudflare, D1, Stripe, page, or ticket are not activation
evidence.

<a id="r03"></a>
## R03 — Implementation map

| Surface | Source-visible responsibility | Limit |
|---|---|---|
| `event` | layer totals/snapshot, five decisions, three layers, threshold config | no source extractor is wired |
| `drift` | pairwise relative drift, maximum/primary layer, count delta, gate | inputs are caller-provided totals |
| `audit` / `history` / `stripe_pause` | traits, records/outcomes, in-memory and failing fixtures | no production backend is shown |
| `reconciler` | audit-before-history-before-SEV-1-pause orchestration | operates through supplied trait implementations |
| `run` | JSON input/report, severity, marker constants, in-memory pass | pause is in-memory for this pass |
| `billing-reconcile-run` | CLI argument/input/report/marker/exit glue | no scheduler or remote client exists in this binary source |
| `error` / `lib` | typed fail paths and public re-exports | runtime error mapping is unknown |

<a id="r04"></a>
## R04 — Public contracts

[Three-layer values](#api-001) · [Decision ladder](#api-002) · [Trait seams](#api-003) · [Pass and CLI](#api-004).

<a id="api-001"></a>
### API-001 — Three-layer reconciliation values

`ReconcileLayerKind` has `Layer1Emit`, `Layer2Aggregate`, and `Layer3Stripe`;
`LayerTotals` carries `u128 total_qty` and `u64 record_count`; `ReconcileSnapshot`
groups them with a tenant UUID. `compute_pairwise_drift_pct` uses the larger
total as denominator, returns zero for two zero totals, and
`compute_max_drift` compares all three pairs. Source: `event.rs`, `drift.rs`.
Unknown: the actual provenance or completeness of each supplied total.

[Return](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Classification ladder and auto-fix decision

`ReconcileDecision` exposes `NoDrift`, `AutoFixed`, `TicketSev3`, `PageSev2`,
and `PageSev1AutoPaused`. Defaults are quiet `0.0001`, SEV-3-to-SEV-2
`0.001`, SEV-2-to-SEV-1 `0.01`; classification uses `<=` at each boundary.
`drift_record_count` is saturating `max(layer1/2/3.record_count) -
min(layer1/2/3.record_count)`. Within quiet, `AutoFixed` needs strictly
nonzero maximum drift plus that derived count `<= 5` and drift `<= 0.0001`;
source records that decision and history row, but exposes no corrective external
write. Source: `event.rs`, `drift.rs:112-135`, `reconciler.rs:249-264`.

[Return](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Audit, history, and pause trait seams

`ReconcileAuditSink::emit`, `DriftHistoryLedger::{insert,latest}`, and
`StripeSubmissionControl::{pause,is_paused}` are the public side-effect
surfaces. The in-memory history key is `(tenant_id, billing_period,
run_started_at)`; identical rows return `AlreadyExistsIdempotent`, divergent
rows at that key return an error. The pause fake stores the same tenant/period
tuple and returns `Acked` or `AlreadyPaused`. Source: `audit.rs`, `history.rs`,
`stripe_pause.rs`. No durable persistence or Stripe action is evidenced.

[Return](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Reconcile pass, report, and executable boundary

`run_reconcile_pass` constructs in-memory audit/history/pause surfaces, then
returns `ReconcileReport`; `parse_input` decodes JSON into `ReconcileRunInput`.
The binary accepts `--input`, `--report`, and `--fail-on sev3|sev2|sev1`;
input is file except `-`/omission uses stdin, and report goes to a path or
stdout. It writes the report before marker/exit selection. Source: `run.rs`,
`src/bin/billing-reconcile-run.rs`. No scheduled invocation is proven.

[Return](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State and source-visible invariants

| ID | Rule and source enforcement | Boundary |
|---|---|---|
| INV-001 | maximum drift evaluates L1↔L2, L2↔L3, and L1↔L3; ties prioritize downstream Layer3 except all-zero drift selects Layer1 | `drift.rs`; source assertion, not observed totals |
| INV-002 | config rejects non-finite/negative values, non-strict ladder, and auto-fix percent above quiet | `event.rs` | configuration deployment is unknown |
| INV-003 | `reconcile` emits `RunStarted`, computes/classifies, emits decision audit, inserts history, then invokes pause only on SEV-1 | `reconciler.rs` | in-memory ordering is not a distributed transaction |
| INV-004 | audit failure returns before history insertion or pause; history/pause failures propagate typed errors | `reconciler.rs`, `error.rs` | alert, retry, and operator response are unknown |
| INV-005 | report severity maps clean/SEV-3/SEV-2/SEV-1; `fails_at` is `max_severity >= floor` | `run.rs` | consumer treatment of report/marker is unknown |

The property-test file statically names coverage for thresholds, gate, Layer3
pause, tenant isolation, audit mapping, idempotent rerun, and drift arithmetic.
It was not executed for this artifact.

<a id="r06"></a>
## R06 — Configuration and targets

`ReconcileConfig::default()` fixes the values in API-002; constructor validation
enforces their ordering. The manifest declares no package feature table and
declares `billing-reconcile-run` as a bin. The binary's default fail floor is
`sev3`; it maps parse/pass/write errors to exit 2, `fails_at` to exit 1, and
other successful reports to exit 0. These are source contracts only; feature
resolution, compilation, and actual process execution are unknown.

<a id="r07"></a>
## R07 — Errors and observability

`ReconcileError` wraps audit, drift-history, and pause backend errors plus
config/internal variants. Audit taxonomy supplies six strings:
`run_started`, `no_drift`, `auto_fixed`, `ticket_filed`, `page_dispatched`,
and `stripe_paused` under `corelink.billing_reconcile.`. The binary source
uses `BILLING_RECONCILE_CLEAN`, `BILLING_RECONCILE_DRIFT_DETECTED`, and
`BILLING_RECONCILE_ERROR`; a marker does not prove a page, ticket, or alert.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: manifest; all eight library modules; same-package binary; and
the two declared test sources, inspected statically. Unknown: reverse graph
completeness, resolved dependency/features, binary compilation/execution,
input extraction, schedule, Cloudflare wiring, D1 totals/history, real Stripe
pause, credentials, report archival/consumption, paging/tickets, production
data, runtime behavior, compatibility consumers, and review status.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
