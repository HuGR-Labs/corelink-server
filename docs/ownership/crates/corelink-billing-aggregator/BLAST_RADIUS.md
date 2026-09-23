---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-aggregator
manifest: crates/corelink-billing-aggregator/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: author_validated
evidence_set: w003-aggregator-source-static-20260920
---

# corelink-billing-aggregator — blast radius

This is a SOURCE-only map of atomic relations around the aggregator package. “Dependency” states a manifest or source type edge; “flow” states what the inspected package receives or produces; “impact” states what must be reconsidered if a named contract changes. None establishes deployed reachability, reverse completeness, or provider operation.

[Scope](#b01) · [Dependencies](#b02) · [Data and state](#b03) · [Integrity](#b04) · [Compatibility](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and reading rules

The owned change surface is the manifest plus `src/{lib,aggregator,audit,chain,error,event,store}.rs`. It composes supplied usage-event types into a source-defined aggregate, an in-memory storage seam, and audit seam. The package has one direct CoreLink package dependency, `corelink-billing-emit`; it has a declared dev-dependency on `corelink-analytics`.

Read every relation in three directions. A dependency is not proof that a dependency calls back. A flow through a trait is not proof that any implementation is live. An impact is a review obligation, not evidence that the downstream system exists or is deployed. The explicit unknowns in B06 override convenient inferences from comments, names, specifications, or repository text outside the scoped packet.

<a id="b02"></a>
## B02 — Declared and source dependency relations

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004).

<a id="rel-001"></a>
### REL-001 — Usage-event dependency

**Dependency:** manifest directly declares `corelink-billing-emit`; source imports `UsageEvent`, `UsageEventKind`, and `IdemKey`. **Flow:** caller-supplied usage events enter the request and determine filtering, aggregate payload, and idem-key ordering. **Impact:** changing those imported type contracts can change public signatures, filters, canonical bytes, or grouping. **Evidence:** SOURCE manifest plus `aggregator.rs`, `event.rs`, `audit.rs`. **Limit:** this does not prove an emitter invocation, archive, queue, or runtime producer. [Index](#b02) [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Serialization and hash libraries

**Dependency:** manifest declares `serde`, `serde_json`, `serde_jcs`, `blake3`, `hex`, and `uuid`. **Flow:** aggregate serialization uses serde/JCS; chain links hash previous-hash bytes followed by canonical bytes; hashes serialize hex; UUID values are fields. **Impact:** changing aggregate serialization or formula can invalidate source-level replay equivalence and stored-digest comparison. **Evidence:** SOURCE `chain.rs`, `event.rs`, manifest. **Limit:** cryptographic implementation, persisted bytes, and external verifier compatibility are outside this package proof. [Index](#b02) [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Error and synchronization libraries

**Dependency:** `thiserror` is declared and source uses `Arc<Mutex<...>>` from the standard library. **Flow:** typed errors leave public APIs; in-memory fakes share local state through mutex-protected maps/buffers. **Impact:** error variant or lock-behavior changes affect callers and test fixtures that inspect decisions, snapshots, or failures. **Evidence:** SOURCE `error.rs`, `audit.rs`, `store.rs`, `aggregator.rs`. **Limit:** this is neither a transaction coordinator nor proof of cross-process safety. [Index](#b02) [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Analytics dev dependency

**Dependency:** `corelink-analytics` appears under `[dev-dependencies]`. **Flow:** no production source flow from this dependency was established by the scoped manifest/module inspection. **Impact:** do not add an analytics-export obligation to an aggregate change merely from the declaration; separately resolve the test and feature context if it matters. **Evidence:** SOURCE manifest. **Limit:** no export, metric, queue, or runtime analytics consumer is proven. [Index](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Data and state relations

[REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Request-to-aggregate flow

**Dependency:** `CounterAggregator` accepts `AggregationRequest`. **Flow:** matching tenant, event kind, and `[start,end)` events are ordered by `(time_ms, idem_key)`; their quantities, count, bounds, and keys construct aggregate data. **Impact:** changes to filtering, sort tie-breaker, saturation, or payload fields can alter decisions and canonical bytes. **Evidence:** SOURCE `aggregator.rs`. **Limit:** callers, input completeness, and billing-period validation upstream are unknown. [Index](#b03)

<a id="rel-006"></a>
### REL-006 — Group key to store flow

**Dependency:** the orchestrator calls `AggregatedCounterStore::{get,chain_head,upsert}`. **Flow:** the group key is tenant, billing period, and event kind; a matching prior data payload returns duplicate before a head read; otherwise aggregate and new head are sent to `upsert`. **Impact:** changing key shape or duplicate comparison changes idempotency and stored-row compatibility. **Evidence:** SOURCE `aggregator.rs`, `event.rs`, `store.rs`. **Limit:** trait implementations besides the in-memory and failing fakes are not proven. [Index](#b03)

<a id="rel-007"></a>
### REL-007 — In-memory state partition

**Dependency:** `InMemoryAggregatedCounterStore` owns maps keyed by group and `(tenant,billing_period)` head. **Flow:** inserted rows update the corresponding head record; snapshots sort stored aggregates by tenant, period, then sequence. **Impact:** changing these coordinates risks tenant/period chain mixing or changes to test-visible snapshots. **Evidence:** SOURCE `store.rs`. **Limit:** map/mutex behavior does not prove durable partitioning or database constraints. [Index](#b03)

<a id="rel-008"></a>
### REL-008 — Decision-state flow

**Dependency:** `AggregationDecision` is returned publicly. **Flow:** no matching events produces `SkippedNoEvents`; matching prior data produces `SkippedDuplicateRun`; fresh successful upsert produces `Aggregated`. **Impact:** altering variant shape or path conditions is a public compatibility and observability risk. **Evidence:** SOURCE `aggregator.rs`, `event.rs`. **Limit:** non-exhaustive variants permit future additions; no client behavior is assumed. [Index](#b03)

<a id="b04"></a>
## B04 — Integrity and audit relations

[REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012).

<a id="rel-009"></a>
### REL-009 — Canonical bytes to chain link

**Dependency:** `HashChainBuilder` uses the chain helper functions. **Flow:** canonical bytes are JCS serialization of the full aggregate; link hash is BLAKE3 of current previous-hash bytes followed by those bytes. **Impact:** any formula, field, encoding, or order change can break link verification or replay matching. **Evidence:** SOURCE `chain.rs`. **Limit:** the package does not prove a persisted canonical-byte ledger or a deployed verifier. [Index](#b04) [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Head and sequence continuity

**Dependency:** aggregate fields feed `HashChainBuilder::append`. **Flow:** append checks sequence equals expected and aggregate previous hash equals head, then computes a new head and increments next sequence saturating. **Impact:** changing genesis, equality, or advancement semantics changes chain continuity expectations. **Evidence:** SOURCE `chain.rs`, `event.rs`. **Limit:** no guarantee is made for concurrent builders, process restart, or a production chain head. [Index](#b04) [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Store digest mismatch boundary

**Dependency:** `upsert` accepts a recomputed canonical digest string. **Flow:** the in-memory store compares it with an existing group-key entry; mismatch returns `DigestMismatch`; a matching digest overwrites the stored aggregate and returns `AlreadyExistsIdempotent` without head advance; only `Inserted` advances the head. **Impact:** digest encoding or equality changes may alter duplicate/replay behavior. **Evidence:** SOURCE `aggregator.rs`, `store.rs`. **Limit:** its documented production counterpart is not implemented in the inspected package. [Index](#b04) [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Audit-before-state observation/mutation

**Dependency:** the orchestrator calls an injected `AggregatorAuditSink`. **Flow:** `RunStarted` is emitted before filtering and store access; completion is emitted on successful/no-event/duplicate paths; store failure attempts `SinkFailure` before returning. **Impact:** changing event names or order can impair source-level forensic sequencing and failure behavior. **Evidence:** SOURCE `aggregator.rs`, `audit.rs`. **Limit:** emission success is only a trait result, not durable audit delivery or alert dispatch. [Index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Compatibility and operational impact

[REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016). [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Public reexport boundary

**Dependency:** `lib.rs` publicly exposes module APIs. **Flow:** consumers may import types and helpers from the crate root. **Impact:** removing, renaming, or changing public type/trait signatures can be source-breaking even when internal modules remain. **Evidence:** SOURCE `lib.rs`. **Limit:** the full consumer set and enabled features are unknown; no reverse dependency resolution was performed. [Index](#b05) [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Non-exhaustive taxonomy boundary

**Dependency:** decisions and error enums are marked non-exhaustive; audit taxonomy is likewise non-exhaustive. **Flow:** callers receive values through result and sink surfaces. **Impact:** additions may be source-compatible only for callers with wildcard handling; removal or semantic reassignment still needs compatibility review. **Evidence:** SOURCE `event.rs`, `audit.rs`, `error.rs`. **Limit:** no consumer match patterns were certified. [Index](#b05) [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Period-window boundary

**Dependency:** `PeriodWindow` governs event inclusion. **Flow:** start is inclusive and end exclusive; an invalid non-increasing window errors at construction. **Impact:** a boundary change may add, remove, or double-count source-selected events and alter the aggregate/chain. **Evidence:** SOURCE `aggregator.rs`. **Limit:** calendar bucketing, late-event handling, and real scheduler cadence are unknown. [Index](#b05) [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Failure propagation boundary

**Dependency:** audit and store trait results convert into `AggregatorError`. **Flow:** audit failure before the run exits early; store failure reaches the sink-failure branch, whose audit failure can supersede the store error through `?`. **Impact:** changing conversion/order alters caller-visible error and mutation expectations. **Evidence:** SOURCE `aggregator.rs`, `error.rs`. **Limit:** retries, rollback, alerts, and operator response are not implemented evidence. [Index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage, unknowns, and done gate

Coverage includes every source module named by the static packet, the manifest’s direct package dependency, and its analytics dev dependency. The map deliberately does not infer a reverse graph from broad repository references. Reverse consumers, feature resolution, test execution, binary/entrypoint reachability, provider configuration, Cloudflare Cron, D1, R2, Queue, durable transactions, audit persistence, and production chain existence remain unknown.

Success is an atomic direction-labeled map that distinguishes declared dependency, source flow, and anticipated impact. Completeness means all module boundaries and source-defined state transitions above are represented, with unknowns retained. Quality means no external relation is claimed without the matching evidence class. Definition of done is the companion [reference](REFERENCE.md#r08), [maintenance](MAINTENANCE.md#m06), static document validation, scope-only diff, and an independent reviewer; these artifacts themselves do not certify any runtime property.

[Back to start](#b01)
