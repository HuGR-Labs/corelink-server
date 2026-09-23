---
schema: corelink-ownership/1.1
document: reference
package: corelink-replication
manifest: crates/corelink-replication/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: corelink-replication-static-20260920
---

# corelink-replication — ownership reference

This is a static-source reference for the replication-context façade and its
local progressive-rollout controller module. It does not establish completed
external replication or rollout, controller execution, Cloudflare/D1 binding,
audit delivery, measurements, scheduling, deployment, or production state.
The verified canonical OKF is an external reference only; it is not reproduced,
redefined, or revalidated here.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Axioms](#r05) · [Configuration](#r06) · [Failure](#r07) · [Unknowns](#r08).

[Blast-radius relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-replication/SKILL.md#s01).

<a id="r01"></a>
## R01 — Identity and static scope

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-replication` / `crates/corelink-replication/Cargo.toml` |
| Root surface | `coordinator`, `failover`, `region`, `replica`, `rollout_controller`, and `region_resolver` are public in `src/lib.rs`. |
| Declared dependencies | Four CoreLink re-export targets, `corelink-worker`, `thiserror`, `serde`, `serde_json`, and `uuid`; `proptest` is a development dependency. |
| Local implementation | The rollout-controller module is physically local; the other four named modules are façade paths. |

<a id="r02"></a>
## R02 — Ownership boundary

| Boundary | Observed responsibility | Falsifiable limit |
|---|---|---|
| Four façade modules | Re-export each dependency’s public surface | Their implementations are not defined in this package. |
| `region_resolver` | Re-exports `corelink_worker::{Region, TenantCtx}` | It does not implement resolver behavior locally. |
| `rollout_controller` | Defines types, traits, state-machine logic, audit and budget abstractions, and in-memory fixtures | Source does not bind a concrete controller, store, audit transport, or deployment API. |

<a id="r03"></a>
## R03 — Implementation map

| File or module | Observed role |
|---|---|
| `src/lib.rs` | Exposes façade modules, the local rollout controller, and worker aliases. |
| `rollout_controller/types.rs` | Defines stages, statuses, artifact, actor, handle, metrics, decision, and action types. |
| `controller.rs` / `state_machine.rs` | Declares `RolloutController`, its in-memory implementation, and immediate-successor transition validation. |
| `auto_rollback.rs` / `budget.rs` | Tracks three sustained triggers and a rolling-window budget abstraction. |
| `audit.rs` / `error.rs` / `metrics.rs` | Declares audit taxonomy and sink, error taxonomy, and metric-name constants. |

<a id="r04"></a>
## R04 — Public contracts

| Contract | Static input/result | Falsifiable limit |
|---|---|---|
| `RolloutController::start` | Artifact and actor produce a handle or declared error | No concrete deploy is invoked. |
| `probe_and_advance` | Handle plus `GateMetrics` produces a decision or error | No probe loop or real metric collection is implemented here. |
| `auto_rollback` / `abort` | The public methods accept a caller-provided trigger or actor and produce an updated handle or error | No external rollback or abort is observed. |
| `BudgetTracker` / `RolloutAuditSink` | Traits accept budget record or audit record and return `Result` | Trait declarations do not prove D1 or CloudEvent delivery. |
| `RolloutStateMachine` | Evaluates hold, advance, rollback, or complete; validates the immediate successor | It does not persist a distributed session. |

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| Axiom | Static predicate | Evidence | Does not prove |
|---|---|---|---|
| A1 façade separation | `coordinator`, `failover`, `region`, and `replica` are exposed through local modules while their dependency ownership remains external | `lib.rs` and façade modules | Consumer migration or behavior execution. |
| A2 signed start | `DeployArtifact::is_cosign_signed` requires non-`None` signature URL and Rekor index; `start` otherwise returns `UnsignedDeploy` | `types.rs`, `controller.rs` | Signature authenticity or Rekor lookup. |
| A3 one active local session | `start` rejects when `active_session` is already `Some` | `controller.rs` | Cross-isolate or D1 uniqueness. |
| A4 audit before mutation | `start`, decision arms, explicit rollback, and abort emit audit before their local session mutation | `controller.rs`, `audit.rs` | External atomic commit or delivered event. |
| A5 probe-driven progression | `probe_and_advance` passes `AutoRollbackDriver::observe` output to `RolloutStateMachine::evaluate`; `validate_advance` permits only `current.next`; public `auto_rollback` accepts its trigger directly from its caller; budget freeze uses `consumed > 1.0` | `controller.rs`, `state_machine.rs`, `auto_rollback.rs`, `budget.rs` | Real gate inputs, elapsed cadence, or budget consumption. |

<a id="r06"></a>
## R06 — Static configuration and compatibility

| Item | Source-visible value or relation | Change boundary |
|---|---|---|
| Stage progression | `Stage1Pct → Stage10Pct → Stage50Pct → Stage100Pct`; dwell minima are 15, 30, 60, and 0 minutes | Preserve immediate-successor validation. |
| Auto-rollback | Error rate, SLO burn, and p99 latency counters use `SUSTAINED_THRESHOLD_SECS = 300` | A constant does not establish an executed five-minute window. |
| Budget | In-window basis points sum is divided by 3000; freeze predicate is strictly greater than `1.0` | Concrete budget storage belongs to a tracker implementation. |
| Schema marker | `rollout_controller_schema_version()` returns `24` | The function does not apply a schema. |
| Audit and metrics | Eight audit-type strings and seven metric-name constants are declared | Names do not prove export, collection, or delivery. |

<a id="r07"></a>
## R07 — Failure and recovery semantics

| Condition | Static outcome | Ownership recovery |
|---|---|---|
| Artifact misses signature or Rekor value | `UnsignedDeploy` from `start` | Do not claim external verification; inspect artifact contract. |
| Budget ratio exceeds `1.0` | `BudgetExceeded`; probe path records `BudgetFrozen` locally after audit succeeds | Coordinate with a concrete tracker owner for stored records. |
| Local session exists | `RolloutInFlight` | Do not infer distributed mutual exclusion. |
| Audit sink returns error | The operation propagates the `RolloutError` returned by `RolloutAuditSink::emit` unchanged before its documented local mutation | Resolve sink implementation separately; delivery remains unknown. |
| Invalid stage successor | `StageBypassed` | Preserve the state-machine boundary rather than add a bypass. |

<a id="r08"></a>
## R08 — Evidence modes and explicit unknowns

Evidence modes used here are: manifest declaration, source declaration,
falsifiable source invariant, and static baseline/diff check. The external
structural checker validates document shape only. Neither source nor the
checker certifies operation.

Explicit unknowns are: (1) external replication; (2) rollout completion or
controller scheduling; (3) Cloudflare gradual-deploy or Durable Object binding;
(4) D1 budget/state persistence and audit transport; and (5) measured metrics,
network activity, deployment, and production behavior. Absence of evidence in
this artifact does not prove absence in the system.
