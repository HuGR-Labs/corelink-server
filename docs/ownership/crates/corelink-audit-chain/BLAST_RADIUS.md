---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-audit-chain
manifest: crates/corelink-audit-chain/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: author_validated
evidence_set: audit-chain-static-graph-20260920
---

# corelink-audit-chain — blast radius

Static dependency, flow, and impact relations only. Relations identify what must be inspected when a surface changes; they do not prove an invoked, persisted, scheduled, or deployed effect.

[Links](#b01) · [Epochs](#b02) · [Public API](#b03) · [Composition](#b04) · [Runtime boundary](#b05) · [Gaps](#b06).

<a id="b01"></a>
## B01 — Event-to-chain relation

`AuditEvent` and canonical-event bytes feed link functions and `HashChainBuilder`; changing event fields, serialization, genesis values, sequence behavior, or hash inputs can alter link verification and any stored/exported representation. Inspect `event.rs`, `chain.rs`, `verifier.rs`, `sealed_archive.rs`, `exporter.rs`, direct consumers, and prior-reader compatibility. Static source does not establish a persisted corpus.

<a id="b02"></a>
## B02 — Epoch-to-verification relation

`epoch.rs` exposes link algorithm, commitments, keyring, and epoch state used by epoch-aware archive functions. A keyed/unkeyed, commitment, or epoch transition change can affect verification and archive serialization paths. Inspect all `link_for_epoch`, `verify_chunk_for_epoch`, and keyring callers. Key custody, rotation procedure, and historical data policy are unknown.

<a id="b03"></a>
## B03 — Trait/reexport-to-consumer relation

`lib.rs` reexports sink, verifier, archive, exporter, Neon-shadow, and `Region` surfaces. Altering a public name, trait signature, error, feature, or default can compile-break static consumers including container, clerk-CF, audit, billing materializer, CLI, fuzz, and tests. The independent `corelink-audit-chain-fuzz` consumer's `HashChainBuilder`/`AuditEvent` edge is [REL-001](../corelink-audit-chain-fuzz/BLAST_RADIUS.md#rel-001), shared key `repo:1232040291:boundary:audit-fuzz-chain-api-001`. Repository text matching is incomplete; resolve Cargo and feature-specific consumers before asserting compatibility.

<a id="b04"></a>
## B04 — Crate-to-server-composition relation

`corelink-container` has static route and audit-related references, including export/archive and analytics paths. Its handlers select composition, request handling, and adapter wiring; this crate supplies contracts rather than owning those decisions. A change that needs route behavior, authorization, response mapping, or dependency assembly must cross to the server composition owner. No running server path was observed.

<a id="b05"></a>
## B05 — Contract-to-remote-runtime relation

R2-shaped sink APIs, verifier source, queue/object-lock/cron statements, and native/Postgres abstractions may imply external integration points, but source shows only contracts, fakes, or deferred intentions for the named production R2 put, cron verification, SIEM queue, and object lock. Runtime operators must validate credentials, scheduling, retention, delivery, and durable state. Do not infer those effects from static paths.

<a id="b06"></a>
## B06 — Coverage limits and stop conditions

The static map does not establish every consumer, feature combination, target architecture, remote object, or historical reader. Stop a compatibility or operational conclusion when one of those is required but lacks direct evidence. Preserve the changed symbol and inspected paths, then request the applicable consumer contract or runtime-operator evidence. A local fake, manifest comment, or source-only test declaration is insufficient recovery evidence for external state.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to links](#b01)
