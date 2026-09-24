---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-chaos-scheduler
manifest: crates/corelink-chaos-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: chaos-scheduler-static-source-20260920
---

# corelink-chaos-scheduler — blast radius

Atomic static relations only. Each entry is a source endpoint, change impact, and falsifier; it does not prove a scheduled or runtime chaos operation. [Verified OKF context](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is routed only.

[Catalog helpers](#b01) · [Safe mode](#b02) · [Runner](#b03) · [Outcome](#b04) · [Queue](#b05) · [Boundary](#b06)

<a id="b01"></a>
## B01 — Catalog helper relations

The three records below have distinct inputs. `lookup` and `distinct_fm_count` accept a caller-supplied catalog slice; neither invokes `canonical_catalog`. Only `weekly_rotation` directly constructs the canonical catalog. These are static input/output relations, not a consumer census or schedule.

<a id="b01a"></a>
### B01a — Catalog slice-to-lookup relation

`lookup(catalog, id)` → `Option<ChaosExperiment>`: the input slice and requested string are scanned in slice order; the first matching `ChaosExperimentId` is cloned, and an absent input id returns `None`. Impact: changing the ID comparison, scan order, or result shape changes lookup callers. Falsifier: a slice without `nonexistent` yields `None`. Evidence: STATIC_SOURCE, `src/catalog.rs`. Unknown: which catalog or ID any caller supplies.

<a id="b01b"></a>
### B01b — Catalog slice-to-distinct-FM-count relation

`distinct_fm_count(catalog)` → `usize`: the input slice's `fm_id` strings are collected, sorted, deduplicated, and counted. Impact: changing the FM field, sort/dedup steps, or result type changes the source-local counting rule. Falsifier: two input rows with the same FM id count once. Evidence: STATIC_SOURCE, `src/catalog.rs`. Unknown: whether a caller passes the canonical catalog or uses the count.

<a id="b01c"></a>
### B01c — Week-index-to-canonical-rotation relation

`weekly_rotation(week_index)` → `canonical_catalog()` → selected `ChaosExperimentId`: this helper's input is the supplied integer; for a nonempty freshly constructed catalog it selects `week_index % len`, otherwise it constructs `noop`. Impact: catalog order, empty fallback, or modulo changes alter its local returned ID. Falsifier: 0 and 8 select the same ID for the current eight-row catalog. Evidence: STATIC_RELATION, `src/lib.rs`; `src/catalog.rs`. Unknown: clock origin, invocation, cron, and dispatch.

<a id="b02"></a>
## B02 — Target-to-safe-mode relation

`ChaosTarget::is_chaos_safe` → `SafeModeSnapshot::evaluate`: only the Staging variant avoids the first abort branch. Impact: target variants, safety match, or abort literal changes alter the local guard predicate. Falsifier: Production selects `prod_target_violation` before later snapshot checks. Evidence: STATIC_RELATION, `src/types.rs`; `src/runner.rs`. Unknown: target provenance or whether a supplied snapshot corresponds to any environment.

<a id="b03"></a>
## B03 — Snapshot-to-runner relation

`SafeModeSnapshot::evaluate` → `run_experiment`: the runner branches on the first `Some(trigger)` before its non-abort telemetry methods. Impact: predicate order, the strict `> 5_000` comparison, or early-return branch changes the source-local control flow. Falsifier: 5,000 does not select the error-rate trigger, while 5,001 does. Evidence: STATIC_RELATION, `src/runner.rs`. Unknown: SEV-1/error-rate source, invocation, and any external safety effect.

<a id="b04"></a>
## B04 — Telemetry-to-outcome relation

`run_experiment` → `Telemetry` → `ChaosOutcome` / `ChaosAuditEvent`: an accepted snapshot produces a Started call, two capture calls, one impact call, and a Completed call; an impact strictly over the experiment bound selects the breach variant, otherwise Passed. An aborted snapshot produces one Aborted event call and skips the later local calls.

Impact: call order, comparison, or variant/event changes alter the contract for any implementation. Falsifier: impact equal to the bound is Passed; one greater is SteadyStateBreached. Evidence: STATIC_RELATION, `src/runner.rs`; `src/types.rs`. Unknown: event delivery, state data, SLO measurement, rollback, and every adapter behavior.

<a id="b05"></a>
## B05 — Scheduler-trait-to-local-queue relation

`ChaosScheduler` → `InMemoryScheduler`: the local implementation provides a FIFO `VecDeque` behind a mutex. Its inputs are `ChaosExperimentId` values and the current source-local queue state; enqueue pushes back, next removes front, and idle observes emptiness. `weekly_rotation` is not a trait implementation and is recorded separately in B01c.

Impact: trait signature, queue end, or lock fallback changes local callers and test-source expectations. Falsifier: `a`, then `b`, dequeues `a` then `b`. Evidence: STATIC_RELATION, `src/lib.rs`. Unknown: implementation selection, lock contention, persistence, and dispatch.

<a id="b06"></a>
## B06 — Coverage and non-relation boundary

B01–B05 cover caller-supplied catalog lookup/count, canonical modulo rotation, target and snapshot branches, runner-to-trait call order, outcome selection, trait seams, and local queue behavior.

Known inverse source edges: `crates/corelink-ops/Cargo.toml` declares the package dependency and `crates/corelink-ops/src/chaos.rs` re-exports its public surface; `tests/e2e-chaos/Cargo.toml` declares the harness package and its source imports and calls `corelink_chaos_scheduler` APIs. These source relations do not establish compilation, harness execution, scheduling, or runtime reachability. Evidence: the named manifests and source files.

Dependency health, resolved features, real fault injection, scheduled execution, metrics, audit/archive retention, rollback, external storage, target impact, any additional consumers, network, deployment, and review remain unknown. Evidence: STATIC_MANIFEST plus STATIC_SOURCE. These omissions are unknowns, not claims of absence; the verified OKF route remains reference-only.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
