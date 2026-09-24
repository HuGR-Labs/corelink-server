---
schema: corelink-ownership/1.1
document: reference
package: corelink-chaos-scheduler
manifest: crates/corelink-chaos-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: chaos-scheduler-static-source-20260920
---

# corelink-chaos-scheduler — ownership reference

Static-source reference only. It does not demonstrate that a chaos experiment was scheduled, run, safely stopped, emitted, archived, observed, or deployed. The [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is routing context only, not policy reproduced or revalidated here.

[Identity](#r01) · [Surface](#r02) · [Catalog](#r03) · [Runner](#r04) · [Seams](#r05) · [Queue](#r06) · [Tests](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Package identity and evidence modes

The manifest names `corelink-chaos-scheduler`, declares no normal dependencies, declares workspace `proptest` as a development dependency, and names three test targets. The static source inventory is `src/lib.rs`, `src/catalog.rs`, `src/runner.rs`, and `src/types.rs`. Evidence modes are STATIC_MANIFEST, STATIC_SOURCE, STATIC_RELATION, TEST_SOURCE, and DOCUMENTARY_CHECK. A manifest dependency or source-file inventory change falsifies this record; none of those facts selects a feature or runs a test.

<a id="r02"></a>
## R02 — Root-surface invariant

`src/lib.rs` forbids unsafe code, denies missing docs and missing debug implementations, exposes the three modules, re-exports their public surface, and declares `ChaosScheduler`, `InMemoryScheduler`, and `weekly_rotation`. Falsifier: adding, removing, or renaming a module, re-export, trait, or root declaration changes the source-visible root contract. These declarations do not establish a worker, cron trigger, or concrete queue adapter.

<a id="r03"></a>
## R03 — Catalog and value-object invariants

`canonical_catalog` constructs eight rows with eight distinct FM identifiers; the rows cover latency injection, failure injection, resource exhaustion, and network partition. `distinct_fm_count` sorts and deduplicates the declared FM strings; `lookup` returns the first matching ID or `None`.

`ChaosExperimentId` and `ChaosRunId` wrap supplied strings without a local grammar check, while `ChaosKind::label`, `ChaosTarget::label`, `ChaosOutcome::label`, and `ChaosAuditEvent::name` select fixed literals. Falsifiers: remove a catalog row, duplicate an FM id, alter a label branch, or make `lookup` return a value for an absent key. Source values are not evidence of a target dependency, environment, event sink, or archive.

<a id="r04"></a>
## R04 — Deterministic runner invariants

`derive_seed` applies the stated FNV-1a constants to `run_id`, a vertical-bar separator, and `experiment_id`; equal supplied strings follow the same source steps. `SafeModeSnapshot::evaluate` selects its first matching static predicate in this order: non-staging target, active production SEV-1 flag, then staging error rate strictly above 5,000 basis points.

`run_experiment` calls that predicate before the non-abort telemetry calls; an abort builds an `Aborted` record and calls `emit_audit(Aborted)`. Otherwise its local call order is `Started`, pre-state, post-state, impact measure, then `Completed`; impact strictly above `blast_radius_bps` selects `SteadyStateBreached`. Falsifiers: a production target, a 5,001-bps snapshot, impact equal to the bound, and impact one greater than the bound distinguish these predicates. This is branch/order evidence only, not an observed guard, rollback, or event.

<a id="r05"></a>
## R05 — Abstract telemetry and scheduling seams

`Telemetry` requires audit emission, pre-state capture, post-state capture, and SLO-impact measurement. `ChaosScheduler` requires enqueue, next, and idle queries. The signatures prove only that callers can provide implementations; they do not identify an audit, metrics, storage, cron, or provider adapter, and they do not prove any invocation. Falsifier: a trait method/signature change alters this static seam. Evidence: `src/runner.rs`; `src/lib.rs`.

<a id="r06"></a>
## R06 — Local queue and rotation invariants

`InMemoryScheduler` stores IDs in `Mutex<VecDeque<_>>`; its successful local lock path pushes to the back, removes from the front, and tests empty state. On a lock error, `pending` returns zero, `next` returns `None`, and `is_idle` returns true. `weekly_rotation` constructs the catalog, selects `week_index % len` when nonempty, and otherwise constructs `noop`.

Falsifiers: enqueue `a` then `b` and observe the source-local FIFO path; compare indexes 0 and 8 for the current eight-row catalog. No lock acquisition, persistence, week source, or scheduling behavior was observed.

<a id="r07"></a>
## R07 — Declared test-source boundary

The manifest declares `prop_chaos_scheduler`, `adversarial_steady_state_breach`, and `catalog_inventory`; unit-test modules also appear in the source files. Their text supplies candidate assertions for catalog inventory, static branch outcomes, deterministic inputs, and local queue behavior. Falsifier: removing a named target or assertion changes test-source inventory. TEST_SOURCE is not a test execution, coverage result, adapter proof, or runtime safety conclusion.

<a id="r08"></a>
## R08 — Five axioms, unknowns, and closure

Five axioms: (1) manifest/source prove static declarations only; (2) public signatures and re-exports prove source-visible contracts only; (3) traits prove neither implementation nor invocation; (4) local branches, mutex queue, and test text prove neither a scheduled run nor an external effect; (5) the [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is routing context only.

Known inverse source edges are `corelink-ops` (manifest dependency and `src/chaos.rs` public re-export) and `e2e-chaos` (manifest target and direct API imports/calls in its checked-in sources). This static inventory does not establish compilation, target selection, invocation, or reachability. Unknowns include feature resolution, any additional consumers/callers, real input values, cron/worker wiring, environment provenance, SEV-1 state, metrics, telemetry/audit delivery and retention, actual fault injection, rollback, target service effects, locks, persistence, network, deployment, production reachability, test execution, and independent review.

Success: every source claim has a falsifier. Completeness: R01–R07 and B01–B06 are reconciled. Quality: atomic relations and evidence modes remain explicit. DoD: the four ownership artifacts and structural results are handed off; no scheduled, runtime, operational, or approval result is implied.
