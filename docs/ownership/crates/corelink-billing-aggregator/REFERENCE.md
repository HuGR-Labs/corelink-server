---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-aggregator
manifest: crates/corelink-billing-aggregator/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: author_validated
evidence_set: w003-aggregator-source-static-20260920
---

# corelink-billing-aggregator — ownership reference

This reference records SOURCE evidence from the manifest and seven listed source modules at the fixed commit. It describes a pure/in-memory aggregation surface; it does not establish deployment, durable storage, real provider invocation, or a complete reverse-consumer graph.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Public API](#r04) · [Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and function

| Field | Source-defined value |
|---|---|
| Package / manifest | `corelink-billing-aggregator` / `crates/corelink-billing-aggregator/Cargo.toml` |
| Role | Counter aggregation with a JCS-canonical BLAKE3 hash-chain primitive |
| Source inspected | `lib`, `aggregator`, `audit`, `chain`, `error`, `event`, `store` |
| Direct package dependency | `corelink-billing-emit` |
| Dev package dependency | `corelink-analytics`; no export follows from this declaration |
| Evidence class | SOURCE only |

The public root reexports aggregation, audit, chain, error, event, and store symbols. Its `aggregator_schema_version()` returns `18`; source presence does not prove a deployed schema.

<a id="r02"></a>
## R02 — Boundary and ownership

This package owns the orchestration contract over supplied `UsageEvent` values, aggregate envelope, deterministic ordering, chain calculation, audit sink abstraction, and counter-store abstraction. `corelink-billing-emit` owns the dependency-provided `UsageEvent`, `IdemKey`, and `UsageEventKind` types; this package consumes rather than implements them.

The in-memory implementations are source-defined fakes. No concrete Cron, D1, R2, Queue, network, or provider adapter exists in the inspected module list. Comments referring to future production wiring are not runtime evidence. See [B02](BLAST_RADIUS.md#b02) for directionality and [M06](MAINTENANCE.md#m06) for boundaries.

<a id="r03"></a>
## R03 — Implementation map

| Module | Source-defined responsibility |
|---|---|
| `aggregator` | Request/window, deterministic filtering, run decision path, in-memory orchestrator |
| `event` | Aggregate CloudEvents-aligned data types, hash newtype, keys, decisions, constants |
| `chain` | JCS bytes, BLAKE3 link computation, link verification, resumable builder |
| `store` | Counter-store trait, chain-head record, in-memory and failing stores |
| `audit` | Audit taxonomy, audit record/sink, in-memory and failing sinks |
| `error` | Non-exhaustive aggregation, store, and audit error surfaces |
| `lib` | Module exports and schema-version function |

The manifest declares `serde_jcs`, `blake3`, `hex`, `uuid`, `serde`, `serde_json`, and `thiserror`, alongside the direct package dependency. A declared dependency identifies compilation intent, not runtime flow.

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005).

<a id="api-001"></a>
### API-001 — Aggregation input and outcome

`CounterAggregator::run(AggregationRequest) -> Result<AggregationDecision, AggregatorError>` accepts tenant, billing period, event kind, `[start_ms, end_ms)` window, borrowed events, source, time, and aggregate ID. `AggregationDecision` is non-exhaustive: aggregate, no matching events, or duplicate run. `PeriodWindow::new` rejects an end not greater than start. Source evidence: `aggregator.rs`, `event.rs`. [Index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Aggregate and grouping shape

`AggregatedCounter` carries pinned CloudEvents-aligned constants, sequence, previous hash, and typed data. Its group key is `(tenant_id, billing_period, event_kind)`. `ChainHash` serializes as hex and has zero-byte genesis. Changing fields, serialization, or constants can change canonical bytes. Source evidence: `event.rs`. [Index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Chain and storage seam

`compute_canonical_bytes`, link functions, `verify_chain_link`, and `HashChainBuilder` provide the source-level chain contract. `AggregatedCounterStore` exposes `upsert`, `chain_head`, and `get`; its in-memory implementation stores rows and heads under a per-instance mutex. The trait alone does not prove durable atomic storage. Source evidence: `chain.rs`, `store.rs`. [Index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Audit seam

`AggregatorAuditSink::emit` accepts an `AggregatorAuditRecord`. The public taxonomy is non-exhaustive and currently maps four strings: run started, run completed, chain break detected, and sink failure. The orchestrator emits the start record before it observes state; the source controls later path ordering. Delivery, retention, and alerting are not observed. Source evidence: `audit.rs`, `aggregator.rs`. [Index](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Error compatibility

`AggregatorError`, `AggregatedCounterStoreError`, and `AggregatorAuditSinkError` are non-exhaustive. The top-level error distinguishes canonicalization, audit, store, chain-break, and internal paths. Consumers must not rely on exhaustive matching. Error strings are diagnostics, not a declared transport protocol. Source evidence: `error.rs`. [Index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable source invariants

| ID | Predicate tied to source |
|---|---|
| INV-001 | `deterministic_event_order` retains only matching tenant, event kind, and period window, then orders by `(time_ms, idem_key bytes)` |
| INV-002 | A new aggregate must present the builder’s current sequence and previous hash; append otherwise returns `ChainBreak` |
| INV-003 | Genesis head is zero hash with next sequence zero; a successful append computes `BLAKE3(prev_hash bytes || JCS(aggregate))` and advances sequence saturating by one |
| INV-004 | Empty and prior-data duplicate guards return before `upsert`; the latter compares typed aggregate data. A defensive post-`upsert` `AlreadyExistsIdempotent` arm handles a matching-digest race path; the in-memory store overwrites its stored aggregate while leaving the head unchanged |
| INV-005 | The start audit emission precedes filtering and store reads; a failed emission returns an audit error before those operations |
| INV-006 | The in-memory store rejects an existing group key when its stored canonical digest differs; matching digest yields idempotent outcome without head advance |

These are predicates of the inspected code, not guarantees about concurrency across processes, transaction durability, or every possible implementer of the public traits.

<a id="r06"></a>
## R06 — Configuration and compatibility

No package-local feature table, binary target, environment-variable parser, provider binding, or configuration file was identified in the inspected manifest/source list. Workspace version, edition, rust version, license, publish, and lint fields are inherited declarations. The test target is declared as `prop_billing_aggregator`; its existence is not an execution result.

Compatibility-sensitive surfaces are the aggregate serialized shape, canonical byte formula, group key, decision/error variants, audit strings, and trait signatures. Treat changes to them as cross-package risk until reverse consumers are independently resolved.

<a id="r07"></a>
## R07 — Failure model

Canonicalization can yield `AggregatorError::Canonicalization`; audit emission maps to `Audit`; store failures including digest divergence map to `Store`; sequence or previous-hash disagreement maps to `ChainBreak`; invalid window construction maps to `Internal`. On the `upsert` error branch, the inspected orchestrator attempts a `SinkFailure` audit and then returns the store error if that audit succeeds.

The source does not establish retries, alerts, an error transport mapping, a durable rollback, or operator recovery. If the failure audit itself fails, the `?` propagation returns its audit error; do not claim both errors are preserved.

<a id="r08"></a>
## R08 — Evidence, quality, and done gate

Evidence class is SOURCE: manifest and module text were inspected at `9f372cc1f5a34752a6b6eb4674df10b3921bce88`. Success for this reference is accurate package identity, public-contract coverage, falsifiable source predicates, and explicit limits. Completeness means R01–R07 map all listed modules and direct declared dependency boundaries. Quality means no source comment is promoted into operational fact. Definition of done requires the companion blast-radius and maintenance documents, structural validation, scope-only diff, and independent review; no cold review or runtime observation is claimed here.

Unknown: reverse consumer inventory, enabled feature combinations, test execution, production runtime reachability, provider configuration, and chain persistence beyond the in-memory implementation.

[Back to start](#r01)
