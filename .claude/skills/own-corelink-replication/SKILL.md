---
name: own-corelink-replication
description: >-
  Route source-backed ownership work for the corelink-replication facade and
  its static rollout-controller, abort, audit, and budget contracts.
metadata:
  schema: "corelink-ownership/1.3"
  package: "corelink-replication"
  manifest: "crates/corelink-replication/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "corelink-replication-static-20260920"
---

# Ownership — corelink-replication

Static source only. This candidate describes a façade and in-memory controller
contracts; it does not certify external replication, a completed rollout,
controller scheduling, audit delivery, storage, Cloudflare, deployment, or
runtime execution. The verified canonical OKF remains an external reference:
do not copy, redefine, or revalidate it here.

[Start](#s01) · [Boundary](#s02) · [Read](#s03) · [Axioms](#s04) · [Flow](#s05) · [Stop](#s06) · [Close](#s07).

<a id="s01"></a>
## S01 — Start condition

| Condition | Required decision | Static evidence | Stop when |
|---|---|---|---|
| Façade or rollout public surface changes | Own this package and read R01–R06 | `Cargo.toml`, `src/lib.rs`, `src/rollout_controller.rs` | A re-export owner must change a dependent contract. |
| Abort, budget, or state transition changes | Trace R04–R07 and B02–B05 | rollout-controller modules | External state or an operational result is required. |

<a id="s02"></a>
## S02 — Authoring boundary

| Boundary | Decision | Static evidence | Stop when |
|---|---|---|---|
| `region`, `replica`, `coordinator`, `failover` | Preserve façade paths; the dependent packages own their implementations | `src/lib.rs` and four thin modules | A re-exported API needs redesign. |
| `rollout_controller` | Own its local types, traits, in-memory fixtures, state machine, audit, and budget abstractions | `src/rollout_controller{.rs,/}` | A concrete DO, D1, audit, or deploy binding is required. |
| `region_resolver` | Preserve the `corelink-worker` type-alias boundary | `src/lib.rs` | Worker resolver behavior needs alteration. |

<a id="s03"></a>
## S03 — Reading route

| Question | Read | Evidence mode | Stop when |
|---|---|---|---|
| Public shape and dependencies | R01–R04 | Manifest and declarations | A caller census is requested. |
| State, abort, audit, or budget effect | R05–R07 and B02–B05 | Falsifiable source invariant | Runtime proof is requested. |
| Permitted closeout | M01–M06 | Static diff and external structural checker | Any step would run Cargo, tests, network, deploy, or GitHub. |

<a id="s04"></a>
## S04 — Five source axioms

| Axiom | Required preservation | Static witness | Stop when |
|---|---|---|---|
| Façade separation | Re-export modules do not become an uncoordinated replacement for dependency-owned behavior | `src/lib.rs` | A dependent owner has not frozen the interface. |
| Signed start | `start` rejects an artifact without both signature and Rekor index | `types.rs`, `controller.rs` | Signature verification outside supplied fields is needed. |
| Single active session | A second in-memory start returns `RolloutInFlight` | `controller.rs` | Cross-process uniqueness is claimed. |
| Ordered transition | Audit emit occurs before local session mutation on decision arms | `controller.rs`, `audit.rs` | External atomicity or delivery is claimed. |
| Progressive cap | `probe_and_advance` passes the driver’s sustained-trigger output to the state machine; public `auto_rollback` instead accepts its trigger from its caller; `consumed > 1.0` remains a distinct budget check | `controller.rs`, `state_machine.rs`, `auto_rollback.rs`, `budget.rs` | Measured metrics or a real rolling window is required. |

<a id="s05"></a>
## S05 — Change flow

| Phase | Decision | Evidence | Stop when |
|---|---|---|---|
| Before edit | Confirm the manifest and baseline source tree | M02 | Baseline differs. |
| During edit | Map each changed declaration to one atomic B relation and one R invariant | R04–R07, B01–B05 | Scope enters a re-export owner. |
| After edit | Run only M06’s four structural checks and baseline diff check | M06 | Evidence would imply execution. |

<a id="s06"></a>
## S06 — Stop conditions

| Request | Required response | Evidence | Stop rule |
|---|---|---|---|
| External replication, rollout completion, scheduler, Cloudflare, D1, audit delivery, or deployment | Record as unknown | R08, B06 | Do not infer it from source or OKF. |
| A concrete dependent-package, worker, or provider contract | Coordinate with that owner | S02, B01, B06 | Do not implement the other owner’s change. |
| Canonical OKF modification | Route to OKF authority | Declared scope | Do not duplicate or revalidate canonical content. |

<a id="s07"></a>
## S07 — Closeout

| Item | Criterion |
|---|---|
| Success | Exactly these four ownership artifacts identify `crates/corelink-replication/Cargo.toml` and retain static-only scope. |
| Completeness | S01–S07, R01–R08, B01–B06, and M01–M06 are navigable; the five axioms and explicit unknowns are present. |
| Quality | Claims are falsifiable source statements with atomic relations and named evidence modes. |
| Done | M06’s four checker invocations return `IMPLEMENTED_CHECKS_PASS`; the baseline diff reports no whitespace error. This is not approval, runtime proof, or cold review. |
