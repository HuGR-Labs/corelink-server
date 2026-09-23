---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-turbo-bridge
manifest: crates/corelink-turbo-bridge/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: candidate
evidence_set: corelink-turbo-bridge-structural-normalization-20260921
---

# corelink-turbo-bridge — static blast radius

Atomic source relations only. Each names a source graph and a change predicate; no relation proves a request, Turbo client, storage provider, network path, deployment, or runtime effect.

[Handler](#b01) · [Key](#b02) · [Ports](#b03) · [Tag](#b04) · [Audit](#b05) · [Limits](#b06)

<a id="relation-index"></a>
**Relation index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006)

<a id="b01"></a>
## B01 — Handler public surface

<a id="rel-001"></a>
### REL-001 — `lib.rs` → handler contracts

**Type:** API/export. **Source → destination:** public re-exports → `TurboArtifactHandler`, request/response, audit/error/event/status types. **Predicate:** an export, method signature, or public field changes. **Impact:** source callers can no longer construct or invoke the same contract. **Failure boundary:** compile-time/source compatibility only. **Evidence:** `src/lib.rs`; `src/handler.rs`. **Stop:** do not infer all consumers or a route. **Falsifier:** remove/rename the cited export or signature.
[Relation index](#relation-index) · [B01](#b01) [Relation index](#b03)

<a id="b02"></a>
## B02 — Validation to artifact key

<a id="rel-002"></a>
### REL-002 — request fields → `(caller_tenant, team_id/hash)`

**Type:** validation/key flow. **Source → destination:** PUT/GET request fields → length/team validators → in-memory map or injected port key. **Predicate:** hash/team grammar or tenant/key formatting changes. **Impact:** accepted input and artifact partitioning can change. **Failure boundary:** invalid values return before the named operation's audit/storage edge. **Evidence:** `src/error.rs`; `src/{handler,adapter}.rs`. **Stop:** authentication and caller-tenant provenance are external. **Falsifier:** a valid source path uses another tenant or key format.
[Relation index](#relation-index) · [B02](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Adapter to opaque ports

<a id="rel-003"></a>
### REL-003 — adapter → `CasReadStore` / `CasWriteStore`

**Type:** dependency/delegation. **Source → destination:** `CasAdapterTurboHandler` → reader/writer trait objects → typed result/error. **Predicate:** port signature, probe, write call, or error branch changes. **Impact:** adapter callers and concrete port owners may need coordinated contract changes. **Failure boundary:** a confirmed read returns `AlreadyExists`; `NotFound` permits write; another probe error also permits write in this source. **Evidence:** `src/adapter.rs`. **Stop:** provider semantics and cross-process atomicity are unobserved. **Falsifier:** remove a cited port call or alter its branch.
[Relation index](#relation-index) · [B03](#b03)

<a id="b04"></a>
## B04 — Artifact to optional tag sidecar

<a id="rel-004"></a>
### REL-004 — artifact key → `$tag/team_id/hash`

**Type:** sidecar flow. **Source → destination:** present validated tag → adapter writer sidecar; adapter reader sidecar → optional GET tag. **Predicate:** validator, namespace, write order, or read-error branch changes. **Impact:** tag representation, collision separation, or GET failure changes. **Boundary:** `NotFound` becomes `None`; another read error returns; sidecar write failure returns after artifact write, with no rollback shown. **Evidence:** `src/{error,adapter}.rs`. **Stop:** header/provider behavior is external. **Falsifier:** an artifact key enters `$tag`, or order/branches change.
[Relation index](#relation-index) · [B04](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Handler to audit sink

<a id="rel-005"></a>
### REL-005 — operation → `TurboAuditSink::emit`

**Type:** ordering/failure flow. **Source → destination:** PUT/GET implementation → attempted/committed/served event → audit sink result. **Predicate:** emit placement, event kind, or error handling changes. **Impact:** source-observable ordering and caller-visible error path change. **Failure boundary:** attempted failures precede the matching read/write; `PutCommitted` failure follows artifact/tag work. **Evidence:** `src/{handler,adapter,audit}.rs`. **Stop:** an emit call is not proof of durable audit delivery. **Falsifier:** move an emit across the cited operation.
[Relation index](#relation-index) · [B05](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Constants and static endpoint values

<a id="rel-006"></a>
### REL-006 — constants/types → accepted values

**Type:** static data. **Source → destination:** limits/version/status/event constructors → handler validation or result. **Predicate:** `MAX_*`, `VERCEL_API_VERSION`, `TurboStatusPayload::enabled`, or events implementation changes. **Impact:** local accepted bounds or returned typed values change. **Failure boundary:** none establishes wire serialization or transport status. **Evidence:** `src/{lib,error,events,status,handler,adapter}.rs`. **Stop:** do not call this client/protocol compatibility. **Falsifier:** change the cited literal, constructor, or caller.
[Relation index](#relation-index) · [B06](#b06) [Relation index](#b03)
### Coverage and unknowns

Six selected relations cover exports, key construction, ports, tag sidecar, audit ordering, and constants. They do not enumerate reverse consumers or every local test. Unknown: composed routes, caller authentication, provider behavior, storage/audit atomicity and durability, transport semantics, concurrency, client behavior, network, deploy, and runtime.

Known reverse consumer: **RC-001** — `corelink-container` declares this crate
as a direct dependency; `routes/turbo_v8.rs` imports its handler contracts,
and `build_handlers` constructs `CasAdapterTurboHandler` with an in-memory
store unless the native R2 branch is selected. Activation requires selecting
the container target and calling the router/builder path. Evidence:
`crates/corelink-container/Cargo.toml`; `src/routes.rs`;
`src/routes/turbo_v8.rs`; `src/routes/turbo_v8/b126_m2_impl_02.rs`.
Unknown: route mounting/invocation, client requests, provider state, network,
deployment, and runtime results. This is not an exhaustive consumer graph;
target and feature resolution remain unverified.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) [Relation index](#b03)
