---
name: own-corelink-config-do
description: >-
  Use for source-grounded ownership or contract changes to corelink-config-do
  payload types, validation, hash, errors, metrics interfaces, propagation
  snapshot model, ConfigSingletonStore, or in-memory audit/store behavior. Do
  not use as evidence of a Durable Object, D1, Prometheus, queue propagation,
  rollback service, or deployed runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-config-do
  manifest: crates/corelink-config-do/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-config-do-structural-normalization-20260921
---

# Ownership — corelink-config-do

This guide is limited to the checked source and static Cargo/import graph. It
does not establish Durable Object, D1, Prometheus, queue/poll propagation,
rollback service, deployment, test execution, or operational behavior.

[Entry](#s01) · [Boundary](#s02) · [Contracts](#s03) · [Mutation](#s04) · [Models](#s05) · [Portability](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Establish the static baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership or contract work is proposed | Record revision; inspect the manifest, `src/lib.rs`, and the seven implementation modules | Package declaration and [R01](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r01) | Baseline or target surface differs |
| A conclusion needs execution or provider evidence | Keep it outside the static result | [R08](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r08) | Do not convert documentation or a trait into runtime evidence |

<a id="s02"></a>
## S02 — Preserve the boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public type, error, trait, metric constant, or store method changes | Map root re-exports and static consumers before editing | `src/lib.rs`; [B02](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b02) | Consumer compatibility is not fully known from source search |
| Work asks for Durable Object, D1, metrics exporter, queue, poller, or deployment behavior | Route to the separately scoped runtime/adapter owner | [R02](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r02) | This crate supplies no runtime proof for that behavior |

<a id="s03"></a>
## S03 — Change data contracts deliberately

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing payload fields, schema version, typed maps, actor, history entry, or change kind | Reconcile `types.rs`, `validation.rs`, `hash.rs`, root exports, and property-test source | [R04](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r04); [B04](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b04) | Assume an old consumer, stored payload, or migration accepts the change |
| Changing hash behavior | Preserve the source-visible JCS-to-SHA-256 relationship and 32-byte/hex interfaces | `src/hash.rs`; [R05](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r05) | Claim cross-platform or persisted-data compatibility without execution evidence |
| Changing a `ConfigError` variant | Review static mappings and callers that import it | `src/error.rs`; [B03](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b03) | Infer HTTP status, retries, or handler behavior without the relevant source |

<a id="s04"></a>
## S04 — Treat mutation ordering as a shared contract

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `update`, `rollback_to`, audit sink, history, or retention logic | Trace validation/hash, lock/CAS or target checks, audit emission, mutation, pruning, and metric calls in the selected in-memory path | `src/store.rs`; [R06](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r06) | Claim backend atomicity, persistence, or audit delivery |
| Changing audit failure handling | Preserve source-visible `emit` before in-memory mutation and failure propagation | `ConfigAuditSink::emit`; `update`; `rollback_to`; `tests/prop_cas.rs` | Treat the in-memory regression test as proof for another implementation |
| Changing rollback semantics | Review target existence, 90-day source constant, payload re-validation, new history entry, and error/metric branches | `src/store.rs`; [B05](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b05) | Claim a live rollback endpoint or recovery objective |

<a id="s05"></a>
## S05 — Keep model interfaces source-local

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing metric names, outcomes, or observer calls | Review constants, outcome label methods, `MetricsObserver`, and in-memory/no-op fixtures | `src/metrics.rs`; [R07](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r07) | Claim Prometheus registration, scrape, export, or alerting |
| Changing snapshot advancement | Preserve the source-visible strictly-greater-version comparison and mutex error result | `src/propagation.rs`; [B06](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b06) | Claim queue delivery, polling, latency, or fleet propagation |

<a id="s06"></a>
## S06 — Preserve declared portability and coverage boundaries

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing UUID use or target-specific dependencies | Reconcile base and `wasm32` UUID features, including `js` | `Cargo.toml`; [R03](../../../docs/ownership/crates/corelink-config-do/REFERENCE.md#r03) | State that wasm compilation or runtime behavior passed without an executed check |
| Changing data/control invariants | Read the named property-test source and add source-local coverage as needed | `tests/prop_cas.rs`; [B07](../../../docs/ownership/crates/corelink-config-do/BLAST_RADIUS.md#b07) | Present test source as a test result |

<a id="s07"></a>
## S07 — Record the bounded result

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work is ready to report | State baseline, files read, changed contract, static consumer relations, checks, and unknowns | [M07](../../../docs/ownership/crates/corelink-config-do/MAINTENANCE.md#m07) | Label static documentation checks as runtime validation, cold review, or deployment approval |

[Back to entry](#s01)
