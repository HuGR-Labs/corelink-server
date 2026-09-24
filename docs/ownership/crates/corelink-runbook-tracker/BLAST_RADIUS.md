---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-runbook-tracker
manifest: crates/corelink-runbook-tracker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: runbook-tracker-static-source-20260920
---

# corelink-runbook-tracker — blast radius

Atomic static relations only. Each relation names a source-local endpoint and
evidence mode; none proves an operated runbook drill, persistence, schedule,
alert, or deployment.

[Identifier](#b01) · [Outcome](#b02) · [Cadence](#b03) · [Recorder](#b04) · [Alert](#b05) · [Boundary](#b06).

<a id="b01"></a>
## B01 — Identifier-to-record relation

`RunbookId::new` → `DrillRecord::new`: a record receives only an already
validated identifier. Impact: an identifier predicate or error change requires
record-constructor review. Falsifier: invalid lowercase suffix must fail before
a record can be built. Evidence mode: STATIC_SOURCE, `src/lib.rs`. Unknown:
whether any external catalog supplies the identifier.

<a id="b02"></a>
## B02 — Inputs-to-outcome relation

`DrillRecord::new` → `compute_drift` → `Outcome`: success selects the strict
ratio branch; failure selects `Fail` before drift evaluation. Impact: duration,
expected-duration, comparison, or variant changes alter the local result.
Falsifier: exactly 2.0 remains `Pass`; above 2.0 becomes `DriftFlagged`.
Evidence mode: STATIC_SOURCE, `src/lib.rs`. Unknown: any post-mortem action.

<a id="b03"></a>
## B03 — Cadence-to-overdue relation

`CADENCE_WINDOW_SECS` → `is_overdue` → `days_since`: the fixed 30-day constant
sets the inclusive overdue boundary and floored elapsed-day result. Impact: a
constant or arithmetic change requires both helper contracts to be reviewed.
Falsifier: the exact boundary flags and a reversed timestamp does not. Evidence
mode: STATIC_SOURCE, `src/lib.rs`. Unknown: the origin or accuracy of timestamps.

<a id="b04"></a>
## B04 — Recorder-to-scan relation

`DrillRecorder::last_drill` → `scan_overdue`: catalog IDs are looked up in
order only until the first lookup error; `?` returns it and leaves later IDs
unchecked. Before that error, each ID produces zero or one local alert. Impact:
trait signature, catalog iteration, or lookup error propagation changes the
scan contract. Falsifier: a lookup error exits the scan rather than fabricating
an alert or querying a later ID. Evidence mode: STATIC_RELATION, `src/lib.rs`.
Unknown: recorder adapter implementation and invocation.

<a id="b05"></a>
## B05 — Scan-to-alert relation

`scan_overdue` → `OverdueAlert`: absent history yields `None` timestamp and
days; overdue history yields both values; fresh history yields no alert. Impact:
field or branch changes alter the local alert data shape. Falsifier: a missing
timestamp must not be presented as a numeric day count. Evidence mode:
STATIC_SOURCE, `src/lib.rs`. Unknown: downstream handling of the returned list.

<a id="b06"></a>
## B06 — Coverage and non-relation boundary

B01–B05 cover the identifier, constructor, outcome, cadence, trait, scan, and
alert data relations available in package source. They do not enumerate
consumers, resolve features, choose adapters, or establish persistence, clocks,
scheduling, execution, evidence retention, alert delivery, deployment, or
review. Evidence mode: STATIC_MANIFEST plus STATIC_SOURCE. The verified OKF
SRE hub remains a reference only; its operational content is not reproduced.

Reverse-consumer edges below are source-backed declarations. Activation means
the named consumer package/entry point is selected or called; Cargo inclusion
does not prove a compiled target or runtime use.

| Relation | Arrow and activation | Evidence | Unknown / limit |
|---|---|---|---|
| RC-001 | `corelink-ops` manifest → `runbook` facade; its crate source re-exports this crate when the facade is compiled. | `crates/corelink-ops/Cargo.toml`; `crates/corelink-ops/src/runbook.rs` | No downstream import, drill invocation, persistence, or operation is established. |
| RC-002 | `corelink-cli` manifest → direct dependency; `runbook_drill` imports this crate and `run_runbook_drill` is selected by the `RunbookDrill` dispatch arm. | `tools/cli/Cargo.toml`; `tools/cli/src/commands/runbook_drill.rs`; `tools/cli/src/main.rs` | Requires selecting the CLI package and invoking the command to reach that path; no drill execution or external runbook is established. |

Both are static reverse edges, not an exhaustive consumer graph; features,
resolved targets, and runtime behavior remain unknown.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
