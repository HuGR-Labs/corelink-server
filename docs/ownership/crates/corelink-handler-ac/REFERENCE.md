---
schema: corelink-ownership/1.1
document: reference
package: corelink-handler-ac
manifest: crates/corelink-handler-ac/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: S
state: draft
evidence_set: handler-ac-source-static-20260920
---

# corelink-handler-ac — reference

Static reference for the Action Cache handler package at `12ca4a8d1`. Evidence is limited to the checked-in manifest, Rust source, and declared property-test source; no Cargo execution, HTTP route, CF Worker, provider operation, audit persistence, telemetry delivery, deployment, or cold review is represented.

[Identity](#r01) · [Exports](#r02) · [Handler ports](#r03) · [Ordering](#r04) · [Fake boundary](#r05) · [Audit and SLO](#r06) · [Declared tests](#r07) · [Limits](#r08).

<a id="r01"></a>
## R01 — Package identity and scope

`crates/corelink-handler-ac/Cargo.toml` names this package `corelink-handler-ac`. `src/lib.rs` declares local `audit`, `error`, `handler`, and `observer` modules and forbids unsafe code while denying missing documentation and missing `Debug` implementations. The manifest directly declares `thiserror` and `corelink-slo`; `proptest` is a development dependency. Evidence: `Cargo.toml:1-25`, `src/lib.rs:1-32`.

<a id="r02"></a>
## R02 — Root public surface

The root re-exports AC audit types/sink/fake, `AcHandlerError`, lookup/update/delete/list requests, responses and traits, `InMemoryAcHandler`, and SLI observer types including `Sli`. The source documents request shapes using route-like strings, but those comments do not establish a mounted HTTP route or worker implementation. Evidence: `src/lib.rs:3-32`, `src/handler.rs:9-382`.

<a id="r03"></a>
## R03 — Handler ports and error taxonomy

`AcLookupHandler` and `AcUpdateHandler` are `Send + Sync + Debug` traits whose methods return their respective response or `AcHandlerError`; the same module also exports delete and list ports. `AcHandlerError` is non-exhaustive and distinguishes cache miss, cross-tenant denial, divergent body, audit failure, and internal failure. Source-level lookup semantics treat `Miss` as non-availability-error even though it is returned as an error. Evidence: `src/handler.rs:315-382`, `src/error.rs:5-53`.

<a id="r04"></a>
## R04 — Falsifiable ordering and data invariants

For the in-memory implementation, cross-tenant lookup emits `LookupDenied` before returning `CrossTenantDenied`; permitted lookup emits `LookupAttempted` before locking/reading the entries map. Its `LookupHit`/`LookupMiss` emission follows the read and precedes its successful/miss result. The update path emits `UpdateAttempted` before acquiring the mutable map lock.

If that first update emit fails, it returns `AuditFailed` without map mutation. Existing unequal bytes return `DivergentBody` without replacement; equal bytes produce `durable=false`; absent keys are inserted with `durable=true`. `UpdateCommitted` is emitted after mutation; a failure there returns an error without source-level rollback. Evidence: `src/handler.rs:414-588`.

<a id="r05"></a>
## R05 — In-memory fake/provider boundary

`InMemoryAcHandler` owns a mutex-protected `HashMap<(String, String), Vec<u8>>` and receives `Arc<dyn AuditSink>` plus `Arc<dyn SliObserver>`. It is an in-process deterministic fake, not evidence of an AC storage provider. The `AuditSink` and `SliObserver` traits are composition seams; neither selects a concrete external implementation. Evidence: `src/handler.rs:386-412`, `src/audit.rs:93-160`, `src/observer.rs:32-75`.

<a id="r06"></a>
## R06 — Audit envelope and SLO observer surface

`AuditEventKind` defines lookup, update, delete, and list attempt/outcome/denial labels, and `AuditEvent` carries kind, tenant, action digest, principal, and timestamp. `AuditSink::emit` returns a string error; `InMemoryAuditSink` stores rows locally and can inject later emit failures.

`observer.rs` re-exports `corelink_slo::definition::Sli`; `SliObservation` carries SLI, error flag, and latency. Its in-memory observer stores observations locally and ignores a poisoned lock during `observe`. Lookup emits observations only on branches reached after relevant audit emission; update emits availability on reached branches. This source does not establish persistence, aggregation, or delivery. Evidence: `src/audit.rs:5-160`, `src/observer.rs:5-75`, `src/handler.rs:414-588`.

<a id="r07"></a>
## R07 — Declared property-test source

The manifest declares the `prop_handler_ac` test target. Its checked-in property source sets a runtime case count from `PROPTEST_CASES` (default 256) and defines properties for availability observation on lookup entry, cross-tenant lookup denial with `LookupDenied` as the first captured row, and an injected update audit failure returning `AuditFailed`. These are source declarations only; no test execution or PASS result is claimed. Evidence: `Cargo.toml:20-25`, `tests/prop_handler_ac.rs:1-116`.

<a id="r08"></a>
## R08 — Evidence limits and unknowns

This static review does not establish actual HTTP or CF Worker routing, wasm target selection, real authentication/principal provenance, provider-backed action-cache behavior, external audit persistence, SLO calculation or telemetry delivery, selected features, complete downstream consumers, runtime reachability, deploy state, or test results. An audit/observer trait or in-memory fake is not evidence for any of those facts.

[Ownership guide](../../../../.claude/skills/own-corelink-handler-ac/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
