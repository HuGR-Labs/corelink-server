---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-handler-ac
manifest: crates/corelink-handler-ac/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: S
state: draft
evidence_set: handler-ac-source-static-20260920
---

# corelink-handler-ac — maintenance

These are source/static-review procedures. They do not authorize Cargo compilation, test execution, HTTP/CF Worker operation, provider use, audit or telemetry delivery, secrets/data access, deployment, publication, or independent review.

[Baseline](#m01) · [Public surface](#m02) · [Lookup/update](#m03) · [Ports and fake](#m04) · [Relations](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Confirm the static baseline

Mode: STATIC_SOURCE. Prerequisites: requested commit and package identity are available. Predicate: the work names `corelink-handler-ac` and the manifest plus six source/test files match the source snapshot. Procedure: compare `Cargo.toml`, `src/{lib,audit,error,handler,observer}.rs`, and `tests/prop_handler_ac.rs` with the stated baseline. Stop: SHA, package identity, or source tree differs. Recovery: obtain a reconciled snapshot; do not infer from an AC umbrella, container, worker, or provider crate. Evidence: baseline SHA and inspected path list.

<a id="m02"></a>
## M02 — Review a public contract change

Mode: STATIC_SOURCE. Prerequisites: changed export, trait, request/response, error, audit, or observer symbol is identified. Predicate: root re-exports, defining declaration, bounds, constructor, and non-exhaustive status are traced. Procedure: compare `src/lib.rs` with the corresponding module and record added, removed, or changed public paths. Stop: wire compatibility, HTTP mapping, or caller compatibility requires an untraced consumer. Recovery: mark it unknown and request evidence from the responsible consumer/composition owner. Evidence: source declarations and static references.

<a id="m03"></a>
## M03 — Review lookup or update ordering

Mode: STATIC_SOURCE. Prerequisites: a change affects tenant checks, audit labels, map read/mutation, observations, or errors. Predicate: local execution order is documented separately for normal and audit-failure branches. Procedure: trace `lookup` and `update` from tenant comparison through each return; retain the distinction between `UpdateAttempted` before mutation and `UpdateCommitted` after mutation.

Stop: a requested invariant assumes emitted audit rows are persistent, observations are delivered, or a later audit failure rolls back state. Recovery: preserve the observed qualification and request provider/runtime evidence where needed. Evidence: `src/handler.rs` branch order and `AcHandlerError` paths.

<a id="m04"></a>
## M04 — Review fake and collaborator changes

Mode: STATIC_SOURCE. Prerequisites: a change affects `InMemoryAcHandler`, `AuditSink`, `SliObserver`, or their in-memory implementations. Predicate: fake state, trait-object injection, and direct `corelink-slo` dependency are classified separately. Procedure: inspect map/vector ownership, mutex behavior, trait method signatures, and the manifest edge. Stop: provider selection, durable storage, SLO aggregation/export, or runtime composition is required. Recovery: leave those claims unknown and obtain separately scoped provider/runtime evidence. Evidence: manifest and module declarations.

<a id="m05"></a>
## M05 — Review declared tests and static impact

Mode: STATIC_GRAPH. Prerequisites: a public symbol, local invariant, or manifest relation changed. Predicate: direct dependencies, re-exports, and the named property-test source are listed separately from unknown consumers. Procedure: inspect `Cargo.toml` and `tests/prop_handler_ac.rs`; identify whether each relation is a local fake, trait port, or declared test assertion.

Stop: a complete reverse graph, feature selection, test result, or runtime consumer list is requested. Recovery: request separately recorded graph/execution evidence; do not call the static list exhaustive. Evidence: manifest/source paths and explicit unknown-consumer statement.

<a id="m06"></a>
## M06 — Validate and hand off scoped artifacts

Mode: STATIC_HANDOFF. Prerequisites: only the four assigned ownership artifacts changed, and the candidate documentary checker is available in the worktree/integration environment. Predicate: S01–S07, R01–R08, B01–B06, and M01–M06 exist; all material claims cite static source; each artifact passes its S-profile structural check; and the diff has no whitespace or scope violation.

Procedure: run the candidate checker once for each kind (`skill`, `reference`, `blast_radius`, `maintenance`) with profile `S` and repository root; run `git diff --check`; inspect the changed-path list. Stop: checker/diff/scope failure, or any required fix outside the four owned paths. Recovery: correct only an owned artifact and rerun the failed documentary check; otherwise hand off the exact failure. Evidence: exact commands and exit statuses, changed paths, baseline SHA, and unknowns.

Author validation is structural/source-static only. It does not prove test execution, HTTP/CF Worker routing, provider behavior, audit persistence, SLO/telemetry delivery, runtime reachability, deployment, compatibility, or cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01).
