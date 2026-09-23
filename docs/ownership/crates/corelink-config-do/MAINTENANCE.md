---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-config-do
manifest: crates/corelink-config-do/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-config-do-structural-normalization-20260921
---

# corelink-config-do — maintenance

Static-analysis maintenance guide only. It neither instructs runtime operation
nor records Cargo, wasm, test, Durable Object, D1, Prometheus, propagation,
rollback, network, deployment, or review results.

[Baseline](#m01) · [Scope](#m02) · [Payload](#m03) · [Store](#m04) · [Models](#m05) · [Portability](#m06) · [Handoff](#m07).

<a id="m01"></a>
## M01 — Baseline control

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Expected source commit and assigned worktree are known | Read manifest, status, root exports, and source modules before a static assessment | Stop on a divergent baseline; recover by obtaining the reconciled baseline without reset | SHA, worktree, `Cargo.toml`, `src/lib.rs` |

<a id="m02"></a>
## M02 — Scope selection

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Requested change maps to errors, hashing, types, validation, store, metrics, or snapshot source | Map it to the owning module and root export before changing it | Stop if the requested behavior needs an adapter, provider, route, or deployment; recover by routing to that scope | `src/lib.rs`; REFERENCE R02 |

<a id="m03"></a>
## M03 — Payload, validation, or hash change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Payload field, schema, map key/value, validation rule, or hash interface changes | Reconcile domain types, `SUPPORTED_SCHEMA_VERSION`, validation branches, JCS/SHA-256 interface, version entry, and test source | Stop if compatibility with stored data or remote consumers is needed; recover by recording the unknown and obtaining the applicable owner decision | `types.rs`, `validation.rs`, `hash.rs`, `tests/prop_cas.rs` |

<a id="m04"></a>
## M04 — Store or audit change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | `ConfigSingletonStore`, audit sink, CAS, history, retention, or rollback logic changes | Trace the selected in-memory path from validation/hash through check, audit emission, mutation, prune, and metrics | Stop if backend transaction, persistent retention, audit delivery, or live rollback evidence is required; recover by isolating that provider scope | `store.rs`; REFERENCE R06; BLAST B05 |

<a id="m05"></a>
## M05 — Metrics or snapshot-model change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Metric vocabulary, observer method, event field, or snapshot comparison changes | Compare constants/outcome labels and every local observer/snapshot call site | Stop if registration, export, queue delivery, polling, timing, or alert behavior is needed; recover by naming the missing runtime owner/evidence | `metrics.rs`, `propagation.rs`, BLAST B06 |

<a id="m06"></a>
## M06 — Manifest or coverage change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | UUID features, wasm target declaration, test target, or source-covered invariant changes | Reconcile base and wasm32 UUID entries, test registration, and named property assertions | Stop before claiming a wasm build or property result; recover by reporting only the static declaration until execution evidence is supplied | `Cargo.toml`; `tests/prop_cas.rs`; BLAST B07 |

<a id="m07"></a>
## M07 — Static handoff

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Assessment is complete | Report baseline, modules/relations read, source-local contract, documentary checks, and explicit unknowns | Stop before claiming runtime validation, provider integration, deployment, or review; recover by retaining the boundary in the handoff | REFERENCE R08; BLAST B07 |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
