---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-replication
manifest: crates/corelink-replication/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: corelink-replication-static-20260920
---

# corelink-replication — blast radius

Each relation below is atomic and derives only from the manifest and static
source. It describes a source dependency or local code flow, never completed
external replication, rollout, controller execution, runtime scheduling,
Cloudflare, storage, deployment, or production behavior. The canonical OKF is
an external reference and is not redefined or revalidated.

[Surface](#b01) · [Start](#b02) · [Decision](#b03) · [Audit](#b04) · [Budget](#b05) · [Unknowns](#b06).

<a id="b01"></a>
## B01 — Façade surface to dependency contracts

`lib.rs` exposes `coordinator`, `failover`, `region`, and `replica`; their
modules re-export the named dependencies declared by `Cargo.toml`. Impact:
changing one façade path or its re-export can alter package consumers at that
path. Evidence: manifest, `lib.rs`, and the four façade modules. Stop: this
does not enumerate consumers or prove that any dependency behavior executes.

<a id="b02"></a>
## B02 — Start gate to local session state

`InMemoryRolloutController::start` checks signed artifact, budget ratio, then
the local `active_session` before emitting `Start` audit and assigning the
session. Impact: changing one gate or this order changes local admission
semantics. Evidence: `controller.rs`, `types.rs`, `budget.rs`. Stop: the mutex
is not evidence of cross-process, DO, or database uniqueness.

<a id="b03"></a>
## B03 — Metrics decision to rollout status

`probe_and_advance` feeds `GateMetrics` to `AutoRollbackDriver` and
`RolloutStateMachine`, which selects hold, immediate advance, rollback, or
completion. Impact: changing a trigger, dwell predicate, or successor affects
the returned local decision. Evidence: `controller.rs`, `auto_rollback.rs`,
`state_machine.rs`, `types.rs`. Stop: source does not supply a real probe,
cadence, or measured metric stream.

<a id="b04"></a>
## B04 — Transition to audit sink

The documented transition arms construct a `RolloutAuditRecord` and call
`RolloutAuditSink::emit` before the corresponding local state mutation; sink
failure propagates a `RolloutError`. Impact: changing record, sink, or ordering
changes the local fail-closed boundary. Evidence: `controller.rs`, `audit.rs`,
`error.rs`. Stop: no audit backend or atomic external commit is observed.

<a id="b05"></a>
## B05 — Controller budget-read and tracker-record boundaries

<a id="b05-controller-read"></a>
### Controller read relation

`InMemoryRolloutController::start` and `probe_and_advance` call only
`BudgetTracker::consumed_ratio`; the former returns `BudgetExceeded` above
`1.0`, while the latter emits `BudgetFreeze`, updates its local handle, and
returns `BudgetExceeded`. `InMemoryRolloutController::auto_rollback` does not
call `record_rollback`. Impact: changing the ratio, comparison, or controller
read path changes local admission/freeze semantics. Evidence: `controller.rs`,
`budget.rs`. Stop: no D1 query, alert, or production controller execution is
established.

<a id="b05-tracker-record"></a>
### Tracker record relation

`BudgetTracker::record_rollback` is a separate trait contract; the in-memory
tracker appends supplied `BudgetRecord` values and its `consumed_ratio` sums
in-window basis points. Impact: changing record filtering or denominator alters
that fixture/trait calculation, not an asserted auto-rollback call path.
Evidence: `budget.rs`. Stop: no concrete persistence, automatic record write,
or rolling-month operation is established.

<a id="b06"></a>
## B06 — Coverage boundary and unknowns

Known source relations are the façade re-exports, worker alias, and local
controller-to-state-machine, audit-sink, and budget-tracker flows. Unknown are
the reverse dependency graph, external replication, actual rollout execution or
completion, controller cadence, provider bindings, persistence, audit delivery,
metric collection, network activity, deployment, and production state. This
static absence is not evidence of system-wide absence.
