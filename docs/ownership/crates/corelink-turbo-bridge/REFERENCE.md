---
schema: corelink-ownership/1.1
document: reference
package: corelink-turbo-bridge
manifest: crates/corelink-turbo-bridge/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-turbo-bridge-structural-normalization-20260921
---

# corelink-turbo-bridge — static reference

SOURCE-only reference at `16d9f0303`. Every axiom is falsifiable from the listed source; none proves routing, transport, a Turbo client, provider behavior, storage durability, audit delivery, network, deployment, or runtime. The canonical OKF route is [Turborepo surface](../../../knowledge/surfaces/turborepo.md); it is routing context only and is neither copied nor revalidated.

[Identity](#r01) · [Contracts](#r02) · [Validation](#r03) · [Axioms](#r04) · [Failures](#r05) · [Test source](#r06) · [Evidence](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Identity and owned surface

| Field | Static observation | Falsifier |
|---|---|---|
| Manifest | `corelink-turbo-bridge`; direct dependencies are `thiserror`, `serde`, and `serde_json`. | Manifest name or dependency list changes. |
| Modules | `lib.rs` publicly exposes `adapter`, `audit`, `error`, `events`, `handler`, and `status`. | A public module declaration changes. |
| Role | Source supplies an opaque-key artifact handler contract, two local implementations, port traits, validation, audit values, and static event/status payloads. | The cited types/modules are removed or their role changes. |
| Local fakes | `InMemoryTurboHandler`, `InMemoryKvStore`, and `InMemoryTurboAuditSink` are in-process `Mutex`-backed source implementations. | Their fields or implementations stop using local maps/vectors. |

<a id="r02"></a>
## R02 — Public contracts

| Contract | Source-backed shape | Falsifier / boundary |
|---|---|---|
| `TurboArtifactHandler` | `put`, `get`, `events`, and `status` return typed results or `TurboBridgeError`. | A signature changes; route/HTTP mapping is outside this crate. |
| Requests/responses | PUT/GET requests contain caller tenant, team label, opaque hash, principal, timestamp, and PUT bytes; GET can return an optional tag. | A field/type changes. |
| Ports | `CasReadStore::read(tenant,key)` and `CasWriteStore::write(tenant,key,bytes)` are opaque-key seams. | A port adds hash verification or changes a parameter/result. |
| Limits | `MAX_HASH_LEN=128`, `MAX_TEAM_ID_LEN=256`, and `MAX_ARTIFACT_TAG_LEN=512`. | A constant changes. |
| Static values | `VERCEL_API_VERSION="v8"`; `events` returns unit success and `status` constructs `status="enabled"`. | The constant or constructors change. |

<a id="r03"></a>
## R03 — Input and key grammar

**R03/AX-001 — opaque hash.** PUT and GET reject only a hash whose byte length exceeds 128; this source does not validate a hash algorithm or its bytes. **Falsifier:** add a hash grammar/content check, or remove the length branch. **Evidence:** `src/{handler,adapter}.rs` implementations; `src/lib.rs`. **Unknown:** acceptance by any caller or client.

**R03/AX-002 — team sub-namespace.** `validate_team_id` accepts a nonempty ASCII `[A-Za-z0-9_-]` string of at most 256 bytes. Both implementations read/write artifact keys as `(caller_tenant, format!("{team_id}/{hash}"))`; no implementation checks `team_id == caller_tenant`. **Falsifier:** alter grammar, tenant dimension, key format, or add the equality branch. **Evidence:** `src/error.rs`; `src/handler.rs`; `src/adapter.rs`. **Unknown:** authenticated-tenant provenance.

**R03/AX-003 — optional tag grammar.** An absent tag bypasses validation; a present tag must be nonempty printable ASCII and at most 512 bytes. The adapter stores it at `$tag/{team_id}/{hash}`, a namespace disjoint from a valid artifact's first key segment; its GET turns absent sidecar into `None` and propagates another read error. **Falsifier:** change validation, namespace, missing-sidecar branch, or non-`NotFound` propagation. **Evidence:** `src/error.rs`; `src/adapter.rs`. **Unknown:** header extraction/emission and provider consistency.

<a id="r04"></a>
## R04 — Implementation axioms

**R04/AX-004 — injected seam.** `CasAdapterTurboHandler` holds trait-object reader, writer, and audit sink; it delegates artifact/tag reads and writes through those ports. **Falsifier:** replace a port call with an unrelated backend path. **Evidence:** `src/adapter.rs`. **Unknown:** concrete port behavior and atomicity.

**R04/AX-005 — implementation distinction.** The adapter emits `PutAttempted`, probes the reader, returns `AlreadyExists` only for a confirmed existing artifact, and otherwise calls writer; `InMemoryTurboHandler` inserts directly and reports its prior length. **Falsifier:** remove the adapter probe/refusal or change the in-memory insertion result. **Evidence:** `src/{adapter,handler}.rs`. **Unknown:** any caller-level serialization; the adapter's source has no lock.

**R04/AX-006 — audit order is split.** Both PUT implementations emit `PutAttempted` before artifact mutation and emit `PutCommitted` after artifact/tag work; both GET implementations emit `GetAttempted` before lookup and `GetServed` after successful lookup. **Falsifier:** move a named emit across its stated read/write. **Evidence:** `src/{handler,adapter}.rs`. A `PutCommitted` failure and adapter tag-write failure can be returned after an artifact write; source shows no rollback. **Unknown:** sink delivery and object recovery.


<a id="r05"></a>
## R05 — Error boundaries

| Static condition | Result / boundary | Falsifier |
|---|---|---|
| Overlong hash, invalid team, or malformed present tag | Error returns before PUT audit/storage; hash/team return before GET audit/storage. | Reorder a guard after the named effect. |
| Adapter reader finds artifact before PUT | `AlreadyExists`; `PutAttempted` was already emitted. | Change the `Ok(_)` branch. |
| Adapter reader probe has another error | Source proceeds to writer. | Return or otherwise change the non-`NotFound` branch. |
| Port/audit/lock failure | Mapped/propagated as `Internal`, `AuditFailed`, or port error according to the local branch. | Alter the exact branch mapping. |
| Missing artifact | Read port/map emits `NotFound`; no source mapping to a transport response is present. | Add/remove the missing branch. |

<a id="r06"></a>
## R06 — Declared test-source boundary

`tests/integration.rs` and `#[cfg(test)]` modules are source evidence of named assertions around both local implementations, validation, tags, audit order, and static values. **Falsifier:** remove the declared test source or the cited test modules. No test result is claimed: tests were not run, so a declared assertion is not execution evidence.


<a id="r07"></a>
## R07 — Evidence record

**SOURCE record:** manifest/exports support R01–R02; handler, adapter, validation, audit, event, and status source support R03–R05; declared test source supports R06. None is a resolution/build, provider/transport, or test-result claim.


<a id="r08"></a>
## R08 — Evidence limits and unknowns

Read: the manifest, seven `src/*.rs` files, and declared integration test source. Not executed: Cargo build/test or any external operation. Unknown: reverse consumers; route registration; request, header, and body treatment; authentication/authorization; HTTP status mapping; client compatibility; concrete reader/writer/audit semantics; provider state; storage durability/atomicity; audit delivery; concurrency; network; deployment; and runtime traffic. Absence from this inspection is not evidence of absence.

[Impact relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
