---
schema: corelink-ownership/1.1
document: reference
package: corelink-runbook-tracker
manifest: crates/corelink-runbook-tracker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: runbook-tracker-static-source-20260920
---

# corelink-runbook-tracker — ownership reference

Static-source reference only. SOURCE does not prove a drill was executed,
persisted, scheduled, alerted, or used to operate a runbook. The verified
OKF SRE hub is a routed reference, not policy revalidated here.

[Identity](#r01) · [Surface](#r02) · [Identifier](#r03) · [Record](#r04) · [Cadence](#r05) · [Scan](#r06) · [Errors](#r07) · [Limits](#r08).

<a id="r01"></a>
## R01 — Package identity and evidence modes

The manifest names `corelink-runbook-tracker`; it declares `thiserror`,
`serde`, `serde_json`, `uuid`, and development `proptest`. `src/lib.rs` is the
only package source file. Evidence modes are STATIC_MANIFEST for declarations,
STATIC_SOURCE for code text, STATIC_RELATION for named source endpoints, and
DOCUMENTARY_CHECK for ownership-document structure. A manifest edit or source
file addition falsifies this inventory.

<a id="r02"></a>
## R02 — Root surface invariant

The root forbids unsafe code and denies missing documentation. Its public
surface contains cadence and drift constants, validated identifiers, records,
outcomes, errors, drift reports, recorder trait, overdue alerts, and helpers
for drift, cadence, elapsed days, and scans. Falsifier: adding, removing, or
renaming one of those public declarations changes the root contract. Evidence:
`src/lib.rs`.

<a id="r03"></a>
## R03 — Runbook identifier invariant

`RunbookId::new` accepts `RB-` plus a nonempty uppercase ASCII letter, digit,
or hyphen suffix; other values return `TrackerError::InvalidRunbookId`.
Falsifier: `RB-FM-202` and `RB-fm-202` distinguish the stated predicate.
`as_str`, `Display`, `TryFrom<String>`, and `From<RunbookId> for String`
preserve the locally validated wire value. Evidence: `src/lib.rs`.

<a id="r04"></a>
## R04 — Drill record and outcome invariant

`DrillRecord::new` rejects blank executor or evidence, negative duration, and
nonpositive expected duration. Failed input selects `Fail`; successful input
uses `compute_drift`, selecting `DriftFlagged` only above the strict 2.0 ratio,
otherwise `Pass`. Falsifier: ratio 2.0 is not flagged, while a greater ratio
is. Labels and post-mortem predicates are source-local `Outcome` methods;
they prove no external effect. Evidence: `src/lib.rs`.

<a id="r05"></a>
## R05 — Fixed cadence invariant

`CADENCE_WINDOW_SECS` is 30 fixed days. `is_overdue` uses saturated elapsed
seconds and the inclusive `>=` boundary; a future last timestamp does not flag.
`days_since` floors nonnegative elapsed seconds to days. Falsifier: exactly the
window is overdue, one second below is not, and reversed timestamps stay false.
Evidence: `src/lib.rs`.

<a id="r06"></a>
## R06 — Recorder and overdue scan invariant

`DrillRecorder` exposes `record` and `last_drill`; the trait comment requires
an idempotent `drill_id` record but does not supply an implementation.
`scan_overdue` visits supplied catalog IDs in order, returning an alert for
missing or overdue timestamps until `last_drill` returns an error; `?` then
returns that first error and leaves later IDs unchecked. Falsifier: a recorder
error before a later ID prevents a lookup for that later ID. A fresh timestamp
yields no alert, while missing history preserves `None` fields. Evidence:
`src/lib.rs`.

<a id="r07"></a>
## R07 — Error and serialization invariant

`TrackerError` names invalid identifier, empty executor, empty evidence,
negative duration, and zero expected-duration cases. `Outcome`, `RunbookId`,
`DrillRecord`, and `OverdueAlert` derive the source-declared serde traits.
Falsifier: changing an error branch, variant, or derive changes the visible
source contract. Serialization declarations do not establish any emitted JSON.
Evidence: `src/lib.rs`; manifest.

<a id="r08"></a>
## R08 — Five axioms, unknowns, and closure

Five axioms: (1) manifest/source prove static declarations only; (2) public
signatures prove source-visible contracts only; (3) a trait proves neither its
implementation nor invocation; (4) branches and local records prove no drill
execution; (5) [the verified OKF SRE hub](../../../knowledge/ops/sre-operations-hub.md)
is routing context only. Unknowns include adapter selection, persistence,
catalog contents, clock source, scheduling, alert delivery, evidence storage,
feature resolution, all consumers, execution, deployment, and review.

Success: every source claim above has a falsifier. Completeness: R01–R07 and
B01–B06 are reconciled. Quality: modes and unknowns remain explicit. Definition
of Done: the four ownership artifacts and structural results are handed off;
neither operation nor approval is implied. [Impact map](BLAST_RADIUS.md#b01) ·
[Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
