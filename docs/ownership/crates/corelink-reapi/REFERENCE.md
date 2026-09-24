---
schema: corelink-ownership/1.1
document: reference
package: corelink-reapi
manifest: crates/corelink-reapi/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: reapi-static-6ed297f5b
---

# corelink-reapi — static reference

SOURCE-only reference at `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. “Invariant” means a source-falsifiable condition below, not an executed test result or runtime guarantee. Verified canonical OKF is reference context only and is neither copied nor revalidated here.

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) · [Contracts](#r04) · [State and invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and role

`crates/corelink-reapi/Cargo.toml` names package `corelink-reapi`. The package implements REAPI CAS read/write, find-missing, capability data, PAT scope checks, and audit envelope construction. It declares a library with default `host-server` feature and thirteen explicit test targets. SOURCE: `Cargo.toml`, `src/lib.rs`.

<a id="r02"></a>
## R02 — Boundaries and ownership

This package owns orchestration, handlers, static REAPI capability values, and its auth/audit abstractions. Injected `PatValidator`, `R2Backend`, `MetaStore`, `OrphanReconciler`, and `Clock` implementations, concrete R2/D1 state, route composition, and audit delivery remain outside its source proof. Its handler, proto, and HTTP read modules require `host-server`; this feature declaration alone does not establish a deployed route. SOURCE: `src/{lib,handler/cas,orchestrator}.rs`, `Cargo.toml`.

<a id="r03"></a>
## R03 — Implementation map

`orchestrator.rs` orders CAS writes and reconciliation; `read.rs` and `find_missing.rs` handle read/discovery outcomes. `pat.rs` defines scope and validation seams, `audit.rs` constructs envelopes, and `capabilities.rs` constructs static values. The `handler.rs` façade and its `handler/{cas,capabilities,bytestream}.rs` implementations adapt these contracts to tonic; `proto` is generated code, `http_read.rs` builds the Axum GET router, and `timing_padding_wiring.rs` constructs middleware layers.

`worker_adapter` is a feature-gated reexport of `corelink_worker::reapi::*`, not a local adapter implementation. `lib.rs` gates all five of `proto`, `handler`, `http_read`, `timing_padding_wiring`, and `worker_adapter` on `host-server`, and reexports the service, router and padding constructors only under that feature. SOURCE: `src/lib.rs:73-138`, `src/handler.rs:81-104` and named modules.

<a id="r04"></a>
## R04 — Public contracts

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) · [API-016](#api-016).

<a id="api-001"></a>
### API-001 — Write orchestration
**Signature:** `CasWriteOrchestrator::commit_put(&self, CommitPutPlan<'_>) -> Result<CommitPutOutput, OrchestratorError>` (async). **Input:** tenant context, body, claimed digest, size bytes, audit/request fields. **Output/effects:** `Fresh` or `Idempotent` plus envelope; verifies BLAKE3, serializes audit, writes R2, commits D1. **Errors:** `HashMismatch`, `AuditEnvelopeSerialize`, `R2`, `Meta`; reconciliation is best-effort after fresh R2 plus failed D1. **Compatibility:** order and duplicate semantics. **Invariant:** [INV-001](#inv-001); **relations:** [REL-001](BLAST_RADIUS.md#rel-001), [REL-002](BLAST_RADIUS.md#rel-002), [REL-003](BLAST_RADIUS.md#rel-003). **Evidence:** `orchestrator.rs:271-349,375-430`.
[Back to API index](#r04)

<a id="api-002"></a>
### API-002 — Read orchestration
**Signature:** `CasReadOrchestrator::read_blob<'r>(&'r self, &'r TenantCtx, &'r Digest) -> impl Future<Output=Result<ReadOutcome, ReadOrchestratorError>> + Send + 'r`. **Output:** `Hit { body: Bytes, size_bytes: u64 }` or `NotFound(MissReason)`. **Effects:** tenant-scoped D1 authorization lookup precedes R2 read. **Errors:** `Meta` or `R2`; not-found is an outcome. **Compatibility:** cross-tenant and tombstoned misses use the same wire 404 despite distinct forensic reasons. **Invariant:** [INV-007](#inv-007); **relation:** [REL-004](BLAST_RADIUS.md#rel-004). **Evidence:** `read.rs:84-240`.
[Back to API index](#r04)

<a id="api-003"></a>
### API-003 — Find-missing discovery
**Signature:** `FindMissingOrchestrator::find_missing_with_sizes<'r>(&'r self, &'r TenantCtx, &'r [(Digest, Option<u64>)]) -> impl Future<Output=Result<FindMissingOutcome, FindMissingError>> + Send + 'r`. **Output:** ordered `missing`, per-input `slot_is_missing`, D1 lookup count. **Effects:** tenant-scoped metadata reads; `Some(size)` checks hash and size, `None` skips size. **Error:** first `Meta` fault aborts batch. **Compatibility:** handler must use per-slot flags for duplicate hashes with different sizes. **Invariant:** [INV-003](#inv-003); **relation:** [REL-006](BLAST_RADIUS.md#rel-006). **Evidence:** `find_missing.rs:99-130,227-330`.
[Back to API index](#r04)

<a id="api-004"></a>
### API-004 — Authorization boundary
**Signatures:** `TenantContext::new(Uuid,Uuid,Region,BTreeSet<AuthScope>,impl Into<String>)->Self`; `require_scope(&self,AuthScope)->Result<(),AuthStubError>`; `PatValidator::authenticate(&self,&str,&str)->Result<TenantContext,AuthStubError>`. **Inputs:** tenant/principal/region/scopes/request ID; token and request ID for authentication. **Effects:** `require_scope` checks exact membership without I/O; concrete validator may access a provider. **Error:** missing scope is `ScopeInsufficient`. **Compatibility:** `CacheFindMissing` grants neither read nor write. **Invariant:** [INV-002](#inv-002); **relation:** [REL-005](BLAST_RADIUS.md#rel-005). **Evidence:** `pat.rs:55-62,105-185,225-237`.
[Back to API index](#r04)

<a id="api-005"></a>
### API-005 — Capabilities
**Signatures:** `cache_capabilities()->CacheCapabilities`; `server_capabilities()->ServerCapabilities`. **Inputs:** none. **Outputs:** BLAKE3, 4 MiB total batch limit, 5 MiB blob limit, REAPI 2.12.0. **Errors/effects:** value construction only; no network response. **Compatibility:** limits, digest function and version are advertised contract data. **Invariant:** [INV-004](#inv-004); **relation:** [REL-007](BLAST_RADIUS.md#rel-007). **Evidence:** `capabilities.rs:27-110`.
[Back to API index](#r04)

<a id="api-006"></a>
### API-006 — Completed put audit envelope
**Signatures:** `AuditEnvelopeBuilder::put_completed(AuditPrincipal,impl Into<String>,impl Into<String>,u64,AuditTime)->Self`; `build(self)->AuditEnvelope`; `AuditEnvelope::to_json(&self)->Result<String,serde_json::Error>`. **Inputs:** principal, client request ID, canonical digest, size in bytes, envelope UUID. **Output/effects:** `corelink.cas.put_completed` CloudEvents 1.0 envelope and compact JSON, without I/O or CE time. The write handlers derive the envelope UUID with BLAKE3 as described in R05; `commit_put` uses it for `AuditEvent.id`, the outbox primary-key field.

**Error:** JSON serialization can fail; outbox delivery is external. **Compatibility:** event type, CE ID, subject, size, request ID and UUID derivation are material. **Invariant:** [INV-006](#inv-006); **relations:** [REL-003](BLAST_RADIUS.md#rel-003), [REL-028](BLAST_RADIUS.md#rel-028). **Evidence:** `audit.rs:158-178,247-319`, `handler/{audit_emit,per_blob,bytestream}.rs`, `orchestrator.rs:284-320`.
[Back to API index](#r04)

<a id="api-007"></a>
### API-007 — Orphan callback
**Signature:** `OrphanReconciler::on_d1_failure_after_r2_put<'a>(&'a self,&'a TenantCtx,&'a Digest) -> impl Future<Output=()> + Send + 'a`; `R2DeleteReconciler::new(Arc<B>,Region)->Self`. **Trigger:** fresh R2 PUT followed by D1 error; duplicate skips callback. **Output/effects:** callback has no error result; current concrete reconciler emits a warning and does not delete R2. **Compatibility:** caller still receives original `Meta` error. **Invariant:** [INV-001](#inv-001); **relation:** [REL-002](BLAST_RADIUS.md#rel-002). **Evidence:** `orchestrator.rs:117-200,301-335`.
[Back to API index](#r04)

<a id="api-008"></a>
### API-008 — Legacy digest-only discovery
**Signature:** `FindMissingOrchestrator::find_missing<'r>(&'r self,&'r TenantCtx,&'r [Digest]) -> impl Future<Output=Result<FindMissingOutcome,FindMissingError>> + Send + 'r`. **Input:** tenant context and digest slice without declared sizes. **Output/effects:** delegates each digest with `None` size to metadata discovery, retaining input slots. **Error:** `Meta` lookup failure. **Compatibility:** size-aware handlers use API-003; digest-only callers cannot assert size equality. **Invariant:** [INV-003](#inv-003); **relation:** [REL-006](BLAST_RADIUS.md#rel-006). **Evidence:** `find_missing.rs:183-241`.
[Back to API index](#r04)

<a id="api-009"></a>
### API-009 — Shared handler core constructor
**Signature:** `HandlerCore::new(pat:V,tdk:Arc<TenantDerivationKey>,writer:Arc<R2Writer<B>>,reader:Arc<R2Reader<B>>,meta:Arc<M>,reconciler:Arc<R>,clock:C)->Self`. **Feature/precondition:** `host-server`; `V:PatValidator`, `B:R2Backend`, `M:MetaStore`, `R:OrphanReconciler`, `C:Clock`, and caller supplies a writer/reader pinned to the same region. **Output/effects:** stores dependencies for the three gRPC services and HTTP read state; `debug_assert_eq!` checks region agreement in debug builds. **Failure/compatibility:** no `Result`; mismatched regions panic only with debug assertions, so release wiring must check this separately. **Invariant:** [INV-005](#inv-005). **Evidence:** `handler.rs:169-241`.
[Back to API index](#r04)

<a id="api-010"></a>
### API-010 — gRPC service constructors
**Signatures:** `CasWriteService::new(core:Arc<HandlerCore<V,B,M,R,C>>)->Self`, `CapabilitiesService::new(core:Arc<HandlerCore<V,B,M,R,C>>)->Self`, `ByteStreamService::new(core:Arc<HandlerCore<V,B,M,R,C>>)->Self`; each has `into_server(self)->ContentAddressableStorageServer<Self>`, `CapabilitiesServer<Self>`, or `ByteStreamServer<Self>` respectively. **Feature/precondition:** `host-server`, with the `HandlerCore` bounds of API-009. **Output/effects:** wraps the shared core, then returns a tonic server wrapper; constructors do no I/O and return no error. RPC auth, read/write and response failures occur when invoked, not at construction. **Compatibility:** each service implements a distinct generated gRPC service; `ByteStreamService` covers read/write and leaves `QueryWriteStatus` unimplemented. **Invariant:** [INV-005](#inv-005); **relations:** [REL-001](BLAST_RADIUS.md#rel-001), [REL-007](BLAST_RADIUS.md#rel-007). **Evidence:** `handler/{cas,capabilities,bytestream}.rs:47-105`.
[Back to API index](#r04)

<a id="api-011"></a>
### API-011 — HTTP read routers
**Signatures:** `cas_get_router<V,B,M,R,C>(HttpReadState<V,B,M,R,C>)->Router`; `cas_get_router_with_padding<V,B,M,R,C>(HttpReadState<V,B,M,R,C>,TimingPaddingLayer)->Router`; `HttpReadState` publicly holds `core:Arc<HandlerCore<V,B,M,R,C>>`. **Feature/precondition:** `host-server`, `V:PatValidator+Clone+'static`, and API-009 backend/clock bounds; caller supplies injected dependencies. **Output/effects:** mounts `GET /v1/cas/{digest}`; first form supplies canonical timing padding, second uses caller layer. Construction returns no `Result` or network response. **Compatibility:** router path, GET method, and padding selection affect transport behavior only when a caller serves the router. **Invariant:** [INV-005](#inv-005); **relation:** [REL-004](BLAST_RADIUS.md#rel-004). **Evidence:** `http_read.rs:76-163`.
[Back to API index](#r04)

<a id="api-012"></a>
### API-012 — Padding constructors
**Signatures:** `canonical_http_padding_layer()->TimingPaddingLayer`; `canonical_grpc_padding_layer()->TimingPaddingLayer`; `MissPaddingLayer` is a public type alias for `TimingPaddingLayer`. **Feature/precondition:** `host-server`; no inputs. **Output/effects:** each constructs canonical config with seeded jitter and `PredicateKind::Any`; neither mounts itself or returns an error. The HTTP router in API-011 mounts its default layer; gRPC callers must mount the returned layer. **Compatibility:** 404, miss-marker and gRPC-not-found padding predicates are transport sensitive. **Invariant:** [INV-005](#inv-005). **Evidence:** `timing_padding_wiring.rs:38-91`, `lib.rs:130-138`.
[Back to API index](#r04)

<a id="api-013"></a>
### API-013 — Per-blob audit dedup key
**Signature:** `audit_request_id_for_blob(&str,Uuid,&str)->String`. **Inputs:** client request ID, tenant UUID and canonical digest text. **Output/effects:** pure deterministic `<client_request_id>:<tenant_id>:<digest_canonical_text>` string; no storage or error. The write handler passes it as `AuditEvent.request_id`; the meta contract dedups on `(request_id,event_type)`, separately from the BLAKE3-derived `AuditEvent.id`/CloudEvents `id`. **Compatibility:** changing any component can alter retry dedup and per-blob batch uniqueness at the metadata outbox seam. **Invariant:** [INV-001](#inv-001); **relations:** [REL-003](BLAST_RADIUS.md#rel-003), [REL-028](BLAST_RADIUS.md#rel-028). **Evidence:** `orchestrator.rs:208-222,308-320,357-406`.
[Back to API index](#r04)

<a id="api-014"></a>
### API-014 — Poisoning attempt audit envelope
**Signature:** `AuditEnvelopeBuilder::poisoning_attempt(AuditPrincipal,impl Into<String>,impl Into<String>,AuditTime)->Self`. **Inputs:** principal, client request ID, canonical digest and envelope UUID; no caller-supplied size. **Output/effects:** builder for `corelink.cas.poisoning_attempt` with `size_bytes=0`; no handler emission is established. `build`/`to_json` are shared operations. **Error:** construction none; serialization can fail. **Compatibility:** event type and forced zero size distinguish a rejected write. **Invariant:** [INV-006](#inv-006); **relation:** [REL-030](BLAST_RADIUS.md#rel-030). **Evidence:** `audit.rs:179-201,247-319`.
[Back to API index](#r04)

<a id="api-015"></a>
### API-015 — Cross-tenant attempt audit envelope
**Signature:** `AuditEnvelopeBuilder::cross_tenant_attempt(AuditPrincipal,impl Into<String>,impl Into<String>,AuditTime)->Self`. **Inputs:** requesting principal, client request ID, canonical digest and envelope UUID; foreign size is unavailable. **Output/effects:** builder for `corelink.cas.cross_tenant_attempt` with `size_bytes=0`; no handler emission is established. **Error:** construction none; serialization can fail. **Compatibility:** event type and zero-size privacy boundary are material. **Invariant:** [INV-006](#inv-006); **relation:** [REL-030](BLAST_RADIUS.md#rel-030). **Evidence:** `audit.rs:202-224,247-319`.
[Back to API index](#r04)

<a id="api-016"></a>
### API-016 — Completed read audit envelope
**Signature:** `AuditEnvelopeBuilder::read_completed(AuditPrincipal,impl Into<String>,impl Into<String>,u64,AuditTime)->Self`. **Inputs:** principal, client request ID, canonical digest, metadata-recorded size in bytes and envelope UUID. **Output/effects:** builder for `corelink.cas.read_completed`; read emitters log its JSON after stream completion, without outbox staging. **Error:** construction none; serialization can fail. **Compatibility:** event type and recorded size are material. **Invariant:** [INV-006](#inv-006); **relation:** [REL-031](BLAST_RADIUS.md#rel-031). **Evidence:** `audit.rs:225-246,247-319`, `handler/{audit_emit,audit_emit_batch}.rs`.
[Back to API index](#r04)

<a id="r05"></a>
## R05 — State, flows, and invariants

The write flow constructs `VerifiedBody`, serializes the audit envelope, calls `writer.put`, then calls `meta.commit_put`. A path reaching storage before verification or reversing these calls falsifies that order. On D1 failure after fresh R2 success, the orchestrator invokes reconciliation; a duplicate skips the callback. `R2DeleteReconciler` logs an orphan and defers deletion to GC, so callback invocation does not establish rollback. SOURCE: `orchestrator.rs:115-200,271-349`.

| State / owner | Key, lifetime and durability boundary | Transition, concurrency and failure boundary |
|---|---|---|
| Body / REAPI call | `Bytes` and `VerifiedBody` live for one `commit_put` call; BLAKE3 must match the claimed `Digest`. | Verification and audit JSON serialization finish before any backend call; their errors leave no R2 write from this call. |
| CAS object / injected `R2Writer<B>` and backend | Writer derives a region plus tenant-prefix plus digest object key. `put` returns `Fresh` or `Duplicate`; R2 persistence and provider consistency belong to the backend. | `put` occurs before metadata commit. Conditional put supplies the duplicate boundary under concurrent writers; this module holds no cross-request lock or transaction spanning R2 and D1. A writer error stops before D1. |
| Metadata / injected `MetaStore` | `BlobMetaKey(ctx.tenant_id(), claimed_digest)` addresses the `blob_meta` row; `size_bytes` and handler-supplied Unix `now_ms` enter `CommitPutRequest`. Read and discovery use tenant-scoped keys. | `commit_put` returns `Inserted` or `AlreadyExists`; the provider owns row transaction, atomicity and persistence. `Fresh` is emitted only for `(R2 Fresh, Meta Inserted)`; every other successful pair emits `Idempotent`. Neither label alone proves physical durability. |
| Audit outbox / injected `MetaStore` | The write handler derives `AuditEvent.id`/CE `id` from `(client_request_id, tenant UUID, digest)` with BLAKE3; this is the outbox primary-key field. Separately, `request_id` is `<client_request_id>:<tenant_id>:<digest_canonical_text>`, with `event_type=CasPutCompleted`. The provider contract dedups on `(request_id,event_type)`; CE data retains the original client request ID. | One `CommitPutRequest` carries row and JSON envelope. Provider-side atomic staging, primary-key and UNIQUE enforcement are trait/schema contracts, not an observed D1 transaction here; a conflicting payload or ID can fail the commit. Delivery is outside this crate. |
| Failure candidate / `OrphanReconciler` | Fresh R2 PUT followed by `MetaStore::commit_put` error leaves an R2 object that this call may have created and no confirmed metadata/outbox commit. | Callback is awaited only for `PutOutcome::Fresh`; current `R2DeleteReconciler` warns and performs no delete. Thus that R2 object survives this source path after D1 failure until an external owner verifies and reconciles it; concurrent writers or uncertain D1/provider outcomes must be checked before compensation. On `Duplicate`, no callback runs and any preexisting R2 object remains. |

The write handlers' non-slot `deterministic_audit_id` hashes the domain `corelink-reapi-audit-id-v2\0`, client request ID bytes, a NUL separator, tenant UUID bytes, another NUL separator and digest hex bytes. They take the first 16 BLAKE3 output bytes and stamp UUID version 7 and variant bits. This UUID is deterministic for a retry of the same triple and tenant-scoped, but its timestamp-shaped bits are content-derived, not wall-clock time.

`BatchReadBlobs` read emitters instead hash the distinct `corelink-reapi-audit-id-v2-slot\0` domain plus the same fields, a NUL separator and `slot_idx` as little-endian `u64`; duplicate digest slots therefore have distinct hash inputs and CE IDs. Current read emitters write structured tracing envelopes, not `MetaStore::commit_put` outbox rows.

The truncated/stamped hash is not a mathematical uniqueness guarantee: a write outbox PK collision or conflicting payload needs meta-provider handling. Read emitters would need the same if ever staged. Neither ID derivation nor the separate `(request_id,event_type)` key proves delivery. SOURCE: `handler/{audit_emit,audit_emit_batch,per_blob,bytestream}.rs`, `orchestrator.rs:208-222,284-320`, `corelink-meta/src/store.rs:91-115`.

The handler supplies clock and UUID values; this crate does not own a long-lived clock, cache, lock, outbox drainer or GC. `now_ms` is a Unix-millisecond input to metadata, while CloudEvents has no `time` field. Source ordering is clear; actual R2/D1 persistence, race outcome, and GC completion remain UNKNOWN. SOURCE: `orchestrator.rs:271-430`, `read.rs:172-240`, `corelink-meta/src/{store,types}.rs` (provider contract), `corelink-worker/src/storage/r2.rs` (writer contract).

`TenantContext::require_scope` checks exact membership: `CacheFindMissing` implies neither `CacheRead` nor `CacheWrite`. The find-missing handler authenticates, requires the find-missing scope, and passes parsed digest/size pairs to `find_missing_with_sizes`; removing that gate or dropping sizes falsifies the flow. SOURCE: `pat.rs:49-62,179-185,393-412`; `handler/cas.rs:274-295,321-373`.

Static capability data contains a 4 MiB batch limit, 5 MiB blob limit, and BLAKE3; changing the values or digest vector falsifies this source contract. `AuditEnvelopeBuilder::build` fixes `specversion` to `1.0`, and `to_json` serializes the envelope; changing either falsifies it. `CasWriteService` requires injected PAT, R2, metadata, reconciler, and clock types. SOURCE: `capabilities.rs:27-110`; `audit.rs:244-319`; `handler/cas.rs:46-103`.

Invariant index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007).

<a id="inv-001"></a>
### INV-001 — CAS write order
Predicate: digest verification and audit serialization precede fresh R2 PUT, which precedes D1 metadata commit; fresh R2 plus D1 failure calls reconciliation. Enforcement: `CasWriteOrchestrator::commit_put` (`orchestrator.rs:271-349`). Violation: a write before verification, D1 before R2, or skipped callback after fresh R2/D1 failure. Verification: source branch inspection and declared `prop_cas_integrity_rejects_mismatch` / `prop_cas_idempotency_collapses_retries` in `tests/prop_cas.rs`. Current state: SOURCE_CONFIRMED; tests not run for this record and rollback is not claimed.
[Back to invariant index](#r05)

<a id="inv-002"></a>
### INV-002 — Exact PAT scopes
Predicate: `CacheFindMissing` grants neither `CacheRead` nor `CacheWrite`; find-missing handler requires its own scope after authentication. Enforcement: `TenantContext::require_scope` and `handler/cas.rs:274-295,321-373`. Violation: accepting a different scope or skipping the gate. Verification: source inspection plus declared `prop_no_cross_tenant_leak` in `tests/prop_find_missing_batch.rs` for the broader isolation path. Current state: SOURCE_CONFIRMED; no test executed here.
[Back to invariant index](#r05)

<a id="inv-003"></a>
### INV-003 — Size-aware discovery
Predicate: one result flag corresponds to each input slot, including duplicate digests with different optional sizes. Enforcement: `find_missing_with_sizes` retains slot order and compares each supplied size (`find_missing.rs:227-330`); `handler/cas.rs` passes pairs rather than digest-only input. Violation: hash-only deduplication of per-slot results or discarded size. Verification: inspect both paths and declared `prop_determinism_same_input_same_output` in `tests/prop_find_missing_batch.rs`. Current state: SOURCE_CONFIRMED; test not run here.
[Back to invariant index](#r05)

<a id="inv-004"></a>
### INV-004 — Static capabilities
Predicate: capability constructors return BLAKE3, 4 MiB batch, 5 MiB blob and REAPI 2.12.0. Enforcement: constants and constructors in `capabilities.rs:27-110`. Violation: changed limit, digest vector or version. Verification: source inspection and declared `tests/capabilities.rs`. Current state: SOURCE_CONFIRMED; test target not run here.
[Back to invariant index](#r05)

<a id="inv-005"></a>
### INV-005 — Host-server composition
Predicate: the five transport modules and root reexports require `host-server`; shared `HandlerCore` dependencies feed gRPC services/HTTP state, and padding constructors use `PredicateKind::Any`. Enforcement: `lib.rs:73-138`, `handler.rs:169-241`, `http_read.rs:131-163`, `timing_padding_wiring.rs:38-91`. Violation: ungated export, mismatched core region, or a layer without the expected predicate. Verification: inspect feature gates and constructors; declared `timing_padding_wiring` unit tests and `tests/timing_padding_grpc_e2e.rs`. Current state: SOURCE_CONFIRMED; debug assertion is build-dependent and tests/deployment not verified.
[Back to invariant index](#r05)

<a id="inv-006"></a>
### INV-006 — Audit envelope fields
Predicate: `AuditEnvelopeBuilder::build` fixes CloudEvents `specversion` to `1.0`, `to_json` serializes that envelope, and handler audit IDs retain the non-slot and slot-aware BLAKE3 input domains and UUID markings described above. The write outbox UUID primary key remains distinct from the `(request_id,event_type)` dedup key. Enforcement: `audit.rs:244-319`, `handler/{audit_emit,audit_emit_batch,per_blob,bytestream}.rs`, `orchestrator.rs:284-320`. Violation: changed fixed field, serializer path, ID material, slot input or key mapping.

Verification: source inspection, declared handler ID tests in `handler/tests.rs:88-140` and `tests/canonical_vectors.rs` for envelope fields; compare slot outputs and meta conflict behavior for a selected change. Current state: SOURCE_CONFIRMED; tests not run here and delivery is unknown.
[Back to invariant index](#r05)

<a id="inv-007"></a>
### INV-007 — Read authorization order
Predicate: a tenant-scoped metadata lookup precedes R2 GET; absent or tombstoned rows return `NotFound` without R2 access. Enforcement: `CasReadOrchestrator::read_blob` constructs `BlobMetaKey` from `ctx.tenant_id()` and branches before `reader.get` (`read.rs:172-240`). Violation: a global digest probe or R2 read before authorization. Verification: source inspection and declared `cross_tenant_read_returns_uniform_not_found` / `prop_tombstone_returns_uniform_not_found` in `tests/prop_cross_tenant_read.rs` / `tests/prop_cas_read.rs`. Current state: SOURCE_CONFIRMED; tests not run here.
[Back to invariant index](#r05)

<a id="r06"></a>
## R06 — Configuration, targets, and features

The manifest declares default feature `host-server`, activating optional tonic/prost/tokio/http/Axum and auth middleware dependencies, plus `uuid/v7` and `uuid/rng-getrandom`. Root modules `handler`, `proto`, `http_read`, `timing_padding_wiring`, and `worker_adapter`, and their transport reexports are all gated on it. Without that feature, the seven pure modules remain declared: `audit`, `capabilities`, `error_map`, `find_missing`, `orchestrator`, `pat`, and `read`. Build dependencies are `tonic-build` and `tonic-prost-build`. SOURCE: `Cargo.toml:13-50`; `lib.rs:73-138`. Which feature set is compiled or served is UNKNOWN.

This reference uses the default S profile: the 23 Rust source files include a generated-proto boundary, a reexport façade and several handler helpers, and the current record does not establish more than 20 distinct semantic implementation modules. The S caps fit this document; no H eligibility is asserted.

Thirteen explicit test targets are declared: `canonical_vectors`, `prop_idempotency`, `handler_e2e`, `capabilities`, `prop_cas`, `prop_cross_tenant_read`, `prop_find_missing_batch`, `read_handler_e2e`, `find_missing_handler_e2e`, `batch_read_blobs_e2e`, `timing_padding_grpc_e2e`, `prop_cas_read`, and `integration_bit_rot`.

<a id="r07"></a>
## R07 — Failures and observability

Failure surfaces include digest mismatch, PAT/scope denial, writer/meta errors, read/miss mapping, audit serialization, and feature-gated module absence. The reconciler emits a warning for an orphan after a fresh R2 write and failed metadata commit; source-level warning does not prove later GC deletion. Concrete provider availability, network responses, outbox persistence, and audit delivery remain UNKNOWN. Evidence: `error_map.rs`, `orchestrator.rs`, `pat.rs`, handlers, `audit.rs`.

<a id="r08"></a>
## R08 — Verification and evidence

The contracts and invariants above are SOURCE observations at pinned commit `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. Cargo declarations do not prove resolved targets, test execution, route wiring, runtime behavior, or rollback effectiveness. To verify a claimed behavior, recheck the cited branch and feature selections, execute an applicable isolated test, and obtain environment-specific evidence for provider or wire effects. No such execution or deployment proof is asserted here.
