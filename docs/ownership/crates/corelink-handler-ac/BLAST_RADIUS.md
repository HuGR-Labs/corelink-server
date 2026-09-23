---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-handler-ac
manifest: crates/corelink-handler-ac/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: S
state: draft
evidence_set: handler-ac-source-static-20260920
---

# corelink-handler-ac — blast radius

Static relations only. “Dependency” identifies a manifest or source edge, “flow” identifies a local source-level sequence, and “impact” identifies the consequence suggested by that evidence. None establishes HTTP routing, CF Worker/wasm execution, an external provider, audit persistence, telemetry delivery, deployment, or runtime reachability.

[Public boundary](#b01) · [Lookup](#b02) · [Update](#b03) · [Collaborators](#b04) · [Other operations](#b05) · [Tests and consumers](#b06).

<a id="b01"></a>
## B01 — Root-to-local public boundary

Dependency: `src/lib.rs` publicly re-exports symbols defined by the local audit, error, handler, and observer modules. Flow: defining module → root `pub use` → crate consumer path. Impact: changing a re-export can change compile-time public paths, but it neither proves nor transfers ownership of a route, worker, caller, or external implementation. Evidence: `src/lib.rs:20-32`.

<a id="b02"></a>
## B02 — Lookup request-to-local-outcome relation

Dependency: the in-memory lookup implementation depends on `AuditSink`, `SliObserver`, its entries map, and `AcHandlerError`. Flow: tenant mismatch → `LookupDenied` emit → availability/latency observations → denial; matching tenant → `LookupAttempted` emit → map read → hit/miss emit → observations → response or `Miss`.

Impact: changing audit order, tenant comparison, or result handling can alter the local fake’s observable audit/SLI sequence and error/result contract. An emit failure short-circuits the reached branch, so source does not prove observations on literally every possible error return. Evidence: `src/handler.rs:414-506`.

<a id="b03"></a>
## B03 — Update audit-to-mutation relation

Dependency: update depends on injected audit/observer ports and the mutex-protected entries map. Flow: tenant mismatch → `UpdateDenied` emit → availability observation → denial; matching tenant → `UpdateAttempted` emit → mutable map decision. The normal non-divergent path continues through `UpdateCommitted` emit → availability observation → response; the divergent-body path returns `DivergentBody` after `UpdateAttempted`, without `UpdateCommitted`.

Impact: a failure of `UpdateAttempted` prevents the mutation, divergent bytes are not overwritten, and equal bytes remain an idempotent no-op. `UpdateCommitted` occurs after mutation, so modifying its failure handling can change post-mutation error behavior; no rollback relation is present in source. Evidence: `src/handler.rs:508-588`.

<a id="b04"></a>
## B04 — Audit and SLI port relations

Dependency: `InMemoryAcHandler` stores trait-object `AuditSink` and `SliObserver` collaborators; `observer.rs` directly depends on `corelink-slo` for the re-exported `Sli`. Flow: handler creates `AuditEvent`/`SliObservation` values → trait method call → selected implementation; the checked-in implementations append to mutex-protected vectors. Impact: an audit or observer contract change affects the fake and any unobserved implementors, while the direct SLO dependency does not prove aggregation, exporter selection, or delivery. Evidence: `Cargo.toml:18-19`, `src/handler.rs:386-412`, `src/audit.rs:93-160`, `src/observer.rs:5-75`.

<a id="b05"></a>
## B05 — Delete/list local-state relations

Dependency: delete and list use the same entries map, audit sink, observer, and `AcHandlerError` as lookup/update. Flow: delete attempts/denies then removes a local entry, returning existence and reclaimed byte count; list attempts/denies then filters one tenant’s local keys, sorts them, and applies cursor/limit pagination. Impact: changes to shared key representation or collaborator semantics can affect all four in-memory operations. Route-like comments and fixed fake timestamps do not prove HTTP status mapping, storage metadata, or provider pagination. Evidence: `src/handler.rs:349-382`, `src/handler.rs:590-717`.

<a id="b06"></a>
## B06 — Declared test and unknown-consumer relations

Dependency: the manifest declares `proptest` only as a development dependency and names `prop_handler_ac`; its source imports the public handler/audit/observer surface. Flow: generated tenant/digest/payload inputs → local fake fixture → asserted local result, captured audit row, or captured SLI count. Impact: source changes to those public symbols or local ordering expectations need test-source review, but the declaration does not prove execution or identify all consumers. Feature-resolved dependency closure, reverse consumers, external providers, and runtime composition remain unknown. Evidence: `Cargo.toml:20-25`, `tests/prop_handler_ac.rs:10-116`.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
