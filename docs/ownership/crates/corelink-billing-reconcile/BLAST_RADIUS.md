---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-reconcile
manifest: crates/corelink-billing-reconcile/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: draft
evidence_set: billing-reconcile-static-source-20260920
---

# corelink-billing-reconcile — blast radius

Static relationship map. Each relation identifies endpoints, surface,
activation, failure, and evidence; static edges do not establish runtime flow.

[Dependencies](#b01) · [Exports](#b02) · [Decision flow](#b03) · [Effects](#b04) · [Binary](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Manifest dependency relations

| ID | Endpoints / surface | Activation | Failure / impact | Evidence |
|---|---|---|---|---|
| REL-001 | package → `corelink-billing-emit`; manifest dependency | Cargo resolution unknown | dependency API/feature change can affect source/build compatibility | crate `Cargo.toml` |
| REL-002 | package → `corelink-billing-aggregator`; manifest dependency | Cargo resolution unknown | dependency API/feature change can affect source/build compatibility | crate `Cargo.toml` |
| REL-003 | package → `corelink-billing-stripe`; manifest dependency | Cargo resolution unknown | dependency API/feature change can affect source/build compatibility | crate `Cargo.toml` |

These edges neither prove reading events/counters/Stripe values nor a reverse
consumer. All dependency features and resolved versions remain unknown.

<a id="b02"></a>
## B02 — Module-to-library-export relation

| ID | Endpoints / surface | Activation | Failure / impact | Evidence |
|---|---|---|---|---|
| REL-004 | `{audit,drift,error,event,history,reconciler,run,stripe_pause}` → crate-root re-exports | Rust source imports a root symbol | export/taxonomy change can break static consumers | `src/lib.rs` and named modules |

Known static source outside the package: `corelink-billing/src/reconcile.rs`
re-exports the complete public API, and its replay sources import decision/layer
types. This is a source relation, not proof that either path is built or run;
the complete reverse graph is unknown.

<a id="b03"></a>
## B03 — Snapshot-to-decision flow

| ID | Endpoints / surface | Activation | Failure / impact | Evidence |
|---|---|---|---|---|
| REL-005 | `ReconcileSnapshot::{layer1,layer2,layer3}` → `compute_max_drift` → `ReconcileDecision` | caller invokes `BillingReconciler::reconcile` | malformed/incorrect caller totals can yield an incorrect local decision; source has no extractor validation | `event.rs`, `drift.rs`, `reconciler.rs` |
| REL-006 | saturating `max(layer1/2/3.record_count) - min(...)` + maximum drift → `auto_fix_gate_fires` → `AutoFixed` or `NoDrift` within quiet | reconciliation reaches quiet branch; auto-fix additionally requires strictly nonzero maximum drift and derived-count/percent bounds | changing derivation or either bound changes decision/audit/history semantics; no external auto-fix is shown | `drift.rs:112-135`, `reconciler.rs:249-264` |
| REL-007 | `ReconcileConfig` → ladder classification → severity/report | `reconcile` or `run_reconcile_pass` receives config | invalid config returns `ReconcileError::Config`; source-only thresholds do not prove operational policy | `event.rs`, `reconciler.rs`, `run.rs` |

<a id="b04"></a>
## B04 — Ordered trait-effect relations

| ID | Endpoints / surface | Activation | Failure / impact | Evidence |
|---|---|---|---|---|
| REL-008 | reconciler → `ReconcileAuditSink::emit(RunStarted)` | each `reconcile` call | audit error returns before drift/history/pause steps | `reconciler.rs`, `audit.rs`, `error.rs` |
| REL-009 | decision → mapped audit emit → `DriftHistoryLedger::insert` | audit succeeds | history error aborts the call after audit; no atomic durable transaction is shown | `reconciler.rs`, `history.rs` |
| REL-010 | SEV-1 decision/history → `StripeSubmissionControl::pause(tenant, period)` | decision exceeds `0.01` | pause error propagates after prior local audit/history steps; real Stripe/D1 pause is unknown | `reconciler.rs`, `stripe_pause.rs`, `error.rs` |

The in-memory ledger/map and BTreeSet pause implementation are the only local
implementations inspected. They are not evidence of D1 state, Stripe calls,
pages, tickets, or recovery automation.

<a id="b05"></a>
## B05 — Input/report/binary relation

| ID | Endpoints / surface | Activation | Failure / impact | Evidence |
|---|---|---|---|---|
| REL-011 | JSON bytes → `parse_input` → `run_reconcile_pass` → `ReconcileReport` | library caller or binary `run` calls it | malformed JSON/pass error is returned; no partial report is constructed by that function | `run.rs`, binary source |
| REL-012 | binary args/stdin/file → report path/stdout + markers/exits | binary process is invoked, which is unobserved | report write/parse/pass errors map to exit 2; report meeting floor maps to 1; otherwise 0 | `src/bin/billing-reconcile-run.rs` |

No scheduled Cloudflare runner, workflow activation, report archive, marker
consumer, page/ticket receiver, credential, or production input endpoint is
in the inspected static evidence.

<a id="b06"></a>
## B06 — Coverage and unknown closure

`prop_billing_reconcile.rs` statically names properties for threshold/gate,
pause intent, tenant isolation, audit mapping, idempotent rerun, and arithmetic;
`billing_reconcile_run_bin.rs` statically names clean/drift/error/stdin/help
CLI cases. No test was run. Unknowns: complete reverse graph, features,
resolved graph, binary activation/execution, all runtime adapters, D1, Stripe,
Cloudflare scheduling, paging/tickets, credentials, production state, and
consumer compatibility.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
