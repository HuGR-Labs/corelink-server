---
name: own-corelink-handler-ac
description: Own static public-contract and change-boundary review for the corelink-handler-ac crate.
metadata:
  evidence-set: handler-ac-source-static-20260920
  manifest: crates/corelink-handler-ac/Cargo.toml
  profile: S
  package: corelink-handler-ac
  source-commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
---

# corelink-handler-ac ownership guide

Use this guide for source-backed review of the AC handler package. It does not prove an HTTP route, a CF Worker implementation, provider selection, telemetry delivery, audit persistence, runtime behavior, deployment safety, or independent review.

[Boundary](#s01) · [Public surface](#s02) · [Lookup](#s03) · [Update](#s04) · [Fake and collaborators](#s05) · [Static relations](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Establish the static boundary

Condition → changing `crates/corelink-handler-ac` or documenting its ownership. Action → compare the requested commit with its manifest, `src/lib.rs`, and local `audit`, `error`, `handler`, and `observer` modules. Evidence → package identity, declared dependencies, public module/re-export declarations, and source paths. Stop → the requested commit or package tree differs; do not substitute a neighbouring AC, container, worker, or provider crate.

<a id="s02"></a>
## S02 — Preserve the public contract

Condition → a request/response, trait, error, audit, or observer symbol changes. Action → trace the root re-exports and their defining module, including lookup/update/delete/list traits and `AcHandlerError`. Evidence → exact Rust declarations, bounds, constructors, and `#[non_exhaustive]` annotations. Stop → a caller, wire representation, or HTTP status mapping is required but not present in this package; record it as unknown rather than infer compatibility.

<a id="s03"></a>
## S03 — Guard lookup ordering and outcome semantics

Condition → changing `AcLookupHandler`, lookup audit labels, tenant comparison, cache reads, or observations. Action → trace `InMemoryAcHandler::lookup` in execution order. Evidence → cross-tenant denial attempts `LookupDenied` before returning; permitted lookup attempts `LookupAttempted` before the in-memory map read; a hit/miss audit follows that read and observations precede the corresponding result. Stop → a claim needs durable audit, actual latency, route mapping, or a provider-backed cache; those are not established by the local fake.

<a id="s04"></a>
## S04 — Guard update ordering and immutability

Condition → changing `AcUpdateHandler`, update audit labels, insertion, or divergent-body behavior. Action → inspect the attempted-audit branch, map mutation block, committed-audit branch, and errors together. Evidence → a failed `UpdateAttempted` emit returns `AuditFailed` before the map lock/mutation; an existing divergent payload returns `DivergentBody` without overwrite; byte-identical re-PUT reports `durable=false`; a fresh insert reports `durable=true`. Stop → a change assumes rollback after a failed `UpdateCommitted` emit: source emits that event after the map mutation and contains no rollback.

<a id="s05"></a>
## S05 — Keep fakes, ports, and providers separate

Condition → changing `InMemoryAcHandler`, `AuditSink`, `SliObserver`, or their in-memory implementations. Action → identify the trait boundary, injected `Arc<dyn ...>` collaborator, and mutex-backed fake separately. Evidence → `InMemoryAcHandler` stores entries in a `HashMap`; `InMemoryAuditSink` and `InMemorySliObserver` retain process-local vectors; `Sli` is re-exported from `corelink_slo`. Stop → a provider, persistence, delivery, worker target, or runtime composition is asserted from those types; obtain separately scoped evidence.

<a id="s06"></a>
## S06 — Assess static dependencies and declared checks

Condition → changing the manifest, public surface, ordering invariant, or test target. Action → inspect direct dependencies and `tests/prop_handler_ac.rs`; distinguish manifest edge, source re-export, fake behavior, and declared property source. Evidence → direct `corelink-slo` and `thiserror` dependencies, plus the `proptest` development dependency and named property-test target. Stop → selected features, full consumer graph, execution results, SLO aggregation/delivery, or runtime reachability are needed; they remain unknown without separate evidence.

<a id="s07"></a>
## S07 — Hand off bounded evidence

Condition → static review is complete. Action → report baseline, source paths inspected, changed contracts, ordering findings, static relations, author checks, and explicit unknowns. Evidence → [reference](../../../docs/ownership/crates/corelink-handler-ac/REFERENCE.md#r01), [blast radius](../../../docs/ownership/crates/corelink-handler-ac/BLAST_RADIUS.md#b01), and [maintenance](../../../docs/ownership/crates/corelink-handler-ac/MAINTENANCE.md#m01). Stop → do not label documentary checks as Cargo execution, runtime evidence, compatibility approval, audit persistence, telemetry delivery, or cold review.
