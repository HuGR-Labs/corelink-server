---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-reapi
manifest: crates/corelink-reapi/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: reapi-static-6ed297f5b
---

# corelink-reapi — static blast radius

This is a source and manifest census at the pinned commit. A declared edge is not evidence of a selected binary, live call, D1 transaction, R2 object, or delivered event. Peer keys identify the same boundary in both packages; relation IDs are local anchors.

[Scope](#b01) · [Method and population](#b02) · [Direct relations](#b03) · [Propagation](#b04) · [Change matrix](#b05) · [Coverage](#b06)

<a id="b01"></a>
## B01 — Scope

The entrypoints are pure orchestration (`commit_put`, `read_blob`, `find_missing_with_sizes`), PAT/audit/capability helpers, and `host-server` gated tonic/Axum handlers. The outgoing boundary is hash verification, CAS façade/worker R2 writer and reader, tenant context, metadata/outbox, auth middleware and generated gRPC dependencies. The inverse boundary is adapter-host's REAPI PAT trait use, the separate fuzz manifest, root workspace membership and an OpenAPI source reference. Storage providers, transport serving and audit drain are owned elsewhere. Evidence: `crates/corelink-reapi/{Cargo.toml,src/lib.rs,src/orchestrator.rs,src/read.rs,src/find_missing.rs}`, root `Cargo.toml`.

<a id="b02"></a>
## B02 — Method and population

Method at source pin: inspect REAPI's manifest declarations, workspace consumers, `lib.rs` exports, handler imports and bridge calls. Classify source-only references separately. This is a static inventory; no Cargo resolution ran.

For a selected host artifact, run `cargo metadata --locked --offline --no-deps --format-version=1` and `cargo tree --locked --offline --workspace --invert corelink-reapi --target <validated-target> --edges normal,build`, then inspect that artifact's manifest/features. These commands were not executed here; selected target and feature closure remain UNKNOWN.

| Population at pin | Discovered | Documented | Excluded with reason | Structural unknown |
|---|---:|---:|---:|---:|
| REAPI normal path declarations | 7 | 7: REL-001/002/003/011/012/013/014 | 0 | 0 |
| REAPI normal registry declarations | 8 | 3: `blake3`/`uuid` REL-028 and `serde_json` REL-029 | 5: `thiserror`, `serde`, `tracing`, `bytes`, `futures` have no separately owned data/transport boundary beyond the documented API effects | 0 |
| Optional host dependency declarations | 12 | 12: REL-007 groups one `host-server` activation | 0 | 0 |
| Build dependency declarations | 2 | 2: REL-007 generated-proto build boundary | 0 | 0 |
| Dev dependency declarations | 10 | 0 | 10: test-only, not production edges; thirteen explicit targets are in REFERENCE R06 | 0 |
| Inverse Cargo declarations | 3 | 3: root REL-015, fuzz REL-016, adapter-host REL-017 | 0 | 0 |
| Inverse Rust source files outside REAPI naming the package | 22 | 6: five adapter PAT call files REL-020–024 and OpenAPI REL-019 | 16: worker/CAS comments and test descriptions name REAPI but contain no `corelink_reapi` import/call; these are contextual references, not direct contracts | 0 |
| REAPI traced contract seams | 14 | 14: REL-001–007/025–031 | 0 | 0 |

For each row, **discovered = documented + excluded** (7=7+0; 8=3+5; 12=12+0; 2=2+0; 10=0+10; 3=3+0; 22=6+16; 14=14+0). Structural unknowns in this static population are zero. Cargo resolution, shipped selection, external consumers beyond this workspace, runtime effects and provider state remain unresolved. Evidence: REAPI and root manifests, `crates/corelink-reapi/fuzz/Cargo.toml`, inverse manifest search and Rust source census.

Adapter-host peer fingerprints use the same source-set recipe as [adapter-host B02](../corelink-adapter-host/BLAST_RADIUS.md#b02). For each path below, expand `@A` to `59c76cf260bcdeb5246772f70821ac8b7e8a9780` and `@R` to `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`; resolve `git rev-parse <pin>:<path>`. Sort UTF-8 `path@full-pin=git-blob-SHA` lines bytewise with `LC_ALL=C`, retain one LF per line, then SHA-256 those bytes. The shared `repo:...` key is an identity, not this hash.

| Input | Source path @ pin | Git blob SHA |
|---|---|---|
| AM / RM | `crates/corelink-adapter-host/Cargo.toml@A` / `crates/corelink-reapi/Cargo.toml@R` | `805e65fb79a1c00019e8051d7b8694769ae35351` / `eb9ab5b3d0f168ed83021815d7094600c76264bd` |
| RP | `crates/corelink-reapi/src/pat.rs@R` | `70850bf882a6387cc39dd36fa398511502711a37` |
| CaB / CaP | `crates/corelink-adapter-host/src/cargo/{bridge,ports}.rs@A` | `c3b3643c6ddf49f65474bc1ac31cfdab07c045a1` / `53eb7835fc4e1a519fbbc4b04019c0ea45a71c86` |
| BrB / BrP | `crates/corelink-adapter-host/src/brew/{bridge,ports}.rs@A` | `91fe22681a675c92f8e6ac0e80bdf6b99115fdc3` / `9ccab3e1827045ea5e9dfb04e60327e87d13ed18` |
| NpB / NpP | `crates/corelink-adapter-host/src/npm/{bridge,ports}.rs@A` | `129596922c5af43741aa280029275a62fddef7e5` / `435286e115b827999fc76274a7d58a4afeaada88` |
| PiB / PiP | `crates/corelink-adapter-host/src/pip/{bridge,ports}.rs@A` | `7c41afccc824fb7f76b8d991909ece31d00e57f9` / `e059a1c8f9f6521cb69c8295189c5b0dfab01a6b` |
| OcB / OcP | `crates/corelink-adapter-host/src/oci/{bridge,ports}.rs@A` | `52df2a9304491faba9d5037cce490886167ab080` / `2318729b3065cd724f0c5033968ced365232add5` |

| REAPI REL ↔ adapter REL | Source-set inputs | SHA-256 |
|---|---|---|
| 017 ↔ 035 | AM, RM | `e1b1eb9622e520054631992034f1bb98d0918fbb7a0fa1fd6dc6f7eac9cec010` |
| 020 ↔ 036 | CaB, CaP, RP | `d653e4ecccf0cc8a5b87188cbdd75826daf4bcb56f6e6d1963f5f3e47241deb4` |
| 021 ↔ 037 | BrB, BrP, RP | `b690335a2c6dc2b53b918798b2d019a578bbc8fc9264a290b46b96b2c7c50738` |
| 022 ↔ 038 | NpB, NpP, RP | `beec04845d4fc00ba79b5e46dba33f195553287fa94fa2aefad823f153aa9570` |
| 023 ↔ 039 | PiB, PiP, RP | `08f04e18eb83cc33ea587d281f5d3dcd78582dc886de5ec22f704b46693aa3ae` |
| 024 ↔ 040 | OcB, OcP, RP | `11243c55312c095d8da319a63f926660b2d25b84afc7b35cc499294b5146bf46` |

Matching pinned bytes and keys establish source identity only. Bilateral semantic approval, selected build and runtime remain UNKNOWN.

<a id="b03"></a>
## B03 — Direct relations

Index: [001](#rel-001) · [002](#rel-002) · [003](#rel-003) · [004](#rel-004) · [005](#rel-005) · [006](#rel-006) · [007](#rel-007) · [011](#rel-011) · [012](#rel-012) · [013](#rel-013) · [014](#rel-014) · [015](#rel-015) · [016](#rel-016) · [017](#rel-017) · [019](#rel-019) · [020](#rel-020) · [021](#rel-021) · [022](#rel-022) · [023](#rel-023) · [024](#rel-024) · [025](#rel-025) · [026](#rel-026) · [027](#rel-027) · [028](#rel-028) · [029](#rel-029) · [030](#rel-030) · [031](#rel-031). REL-008/009/010 from the earlier draft duplicated hash, writer and MetaStore seams; their call sites are represented by the atomic records below.

<a id="rel-001"></a>
### REL-001 — Hash verified write
**Type/direction/key:** runtime-call, `corelink-hash::VerifiedBody` → REAPI `commit_put`; `repo:1232040291:boundary:hash-reapi-verified-write-001`. **Peer:** [hash REL-023](../corelink-hash/BLAST_RADIUS.md#rel-023), reciprocal key confirmed. **Activation/contract:** CAS batch per-blob and ByteStream write both call `commit_put`, which calls `VerifiedBody::new(body,digest)` before R2. **State/failure/limit:** mismatch returns `HashMismatch` before backend touch; later R2/D1 effects are separate. **Validation/owner:** digest vectors and source order with hash owner. **Evidence:** `src/handler/{cas,per_blob,bytestream}.rs`, `src/orchestrator.rs:271-302`, hash peer.

[Index](#b03)

<a id="rel-002"></a>
### REL-002 — R2 writer contract
**Type/direction/key:** runtime-call, REAPI → `corelink-cas::r2_storage` façade → worker `R2Writer`; `repo:1232040291:boundary:reapi-worker-r2-write-001`. **Peer:** worker-side backlink pending. **Activation/contract:** `commit_put` calls `put` with verified body and receives `Fresh/Duplicate`; batch and ByteStream writes share this call site. **State/failure/limit:** R2 error stops before metadata; conditional put, object key and persistence are provider contracts, not observed objects. **Validation/owner:** compare region/key/outcome and isolated write tests with worker/CAS owners. **Evidence:** `src/orchestrator.rs:301-335`, `src/handler/{cas,per_blob,bytestream}.rs`, `corelink-cas/src/lib.rs:109-110`.

[Index](#b03)

<a id="rel-003"></a>
### REL-003 — Metadata commit and outbox contract
**Type/direction/key:** runtime-call/data, REAPI → `corelink-meta::MetaStore::commit_put`; `repo:1232040291:boundary:reapi-meta-commit-put-001`. **Peer:** meta-side backlink pending. **Activation/contract:** successful writer outcome leads to `CommitPutRequest` with tenant/digest key and audit. **State/failure/limit:** `Meta` error propagates; only fresh R2 plus failed commit invokes orphan callback. Provider owns D1 atomicity, outbox uniqueness and persistence. **Validation/owner:** compare request/result/error and provider transaction with meta owner; test idempotency and failure branches. **Evidence:** `src/orchestrator.rs:304-349`, `corelink-meta/src/store.rs:58-115`.

[Index](#b03)

<a id="rel-004"></a>
### REL-004 — Handler to read orchestration
**Type/direction:** internal runtime-call, ByteStream and HTTP read handlers → `CasReadOrchestrator::read_blob`. **Activation/contract:** parsed digest/tenant → metadata gate REL-026 → R2 body REL-025 or uniform miss. **State/failure/limit:** `Meta`/R2 faults map through handler error functions; no assertion that a router is served. **Validation/owner:** read handler and cross-tenant tests; REAPI owner. **Evidence:** `src/handler/bytestream.rs:14-38,162-170`, `src/http_read.rs`, `src/read.rs:172-240`.

[Index](#b03)

<a id="rel-005"></a>
### REL-005 — Handler PAT scope
**Type/direction:** internal runtime-call, CAS handler → injected `PatValidator` and `TenantContext::require_scope`. **Activation/contract:** find-missing request authenticates then requires exactly `CacheFindMissing`; other read/write handlers require their scopes. **State/failure/limit:** invalid PAT/scope maps to tonic status; provider acceptance is external. **Validation/owner:** exact-scope test and handler call review; REAPI/auth owner. **Evidence:** `src/handler/cas.rs:274-295`, `src/pat.rs:179-185,393-412`.

[Index](#b03)

<a id="rel-006"></a>
### REL-006 — Size-aware discovery
**Type/direction:** internal runtime-call, CAS handler → `FindMissingOrchestrator`. **Activation/contract:** parsed `(Digest,Option<u64>)` slots → `find_missing_with_sizes`; duplicates retain individual size checks through metadata REL-027. **State/failure/limit:** first metadata fault aborts; returned wire behavior is unobserved. **Validation/owner:** duplicate-size and handler tests; REAPI owner. **Evidence:** `src/handler/cas.rs:321-373`, `src/find_missing.rs:227-330`.

[Index](#b03)

<a id="rel-007"></a>
### REL-007 — Host feature and generated wire
**Type/direction:** build-deploy, REAPI manifest/build script → gated tonic/Axum/proto modules. **Activation/contract:** default `host-server` selects twelve optional declarations and auth/worker middleware features; `tonic-build` and `tonic-prost-build` generate proto surface. **State/failure/limit:** missing feature removes handlers/reexports; compile or service registration is unverified. **Validation/owner:** selected target/feature build plus generated wire comparison; REAPI host owner. **Evidence:** `Cargo.toml:13-50`, `src/lib.rs:73-138`, `build.rs`.

[Index](#b03)

<a id="rel-011"></a>
### REL-011 — CAS façade import
**Type/direction:** reexport/dependency, `corelink-cas` → REAPI `r2_storage` imports. **Activation/contract:** package builds and imports façade `R2Error`, `R2Writer`, `R2Reader`, `R2Backend`. **Failure/limit:** façade path drift breaks import; underlying behavior remains worker-owned. **Validation/owner:** compile selected feature and compare reexport target; CAS/worker owners. **Evidence:** `corelink-cas/src/lib.rs:104-110`, REAPI `src/{orchestrator,read}.rs`.

[Index](#b03)

<a id="rel-012"></a>
### REL-012 — Tenant derivation key
**Type/direction:** dependency/data, `corelink-tenant-path::TenantDerivationKey` → REAPI `HandlerCore`. **Activation/contract:** host constructor receives the key used when deriving tenant-scoped storage context. **Failure/limit:** derivation or key-wiring drift can alter tenant prefix selection; ByteStream resource grammar belongs to REAPI's own `handler/helpers.rs`. **Validation/owner:** compare constructor and tenant-context tests with tenant-path owner. **Evidence:** REAPI manifest, `src/handler.rs:75,169-241`, `src/handler/helpers.rs:244-335`.

[Index](#b03)

<a id="rel-013"></a>
### REL-013 — Region and tenant context
**Type/direction:** reexport/dependency, `corelink-replication::region_resolver` → REAPI `TenantCtx`/`Region`. **Activation/contract:** injected context determines tenant metadata key and R2 region/prefix. **Failure/limit:** type/key drift affects isolation; no active replication is inferred. **Validation/owner:** source key construction and context tests; replication/worker owners. **Evidence:** REAPI manifest, `src/{orchestrator,read,pat}.rs`, `corelink-replication/src/lib.rs:98`.

[Index](#b03)

<a id="rel-014"></a>
### REL-014 — Auth feature propagation
**Type/direction:** config/dependency, REAPI `host-server` → `corelink-auth/tower-middleware` and worker middleware feature. **Activation/contract:** compile-time selection only. **Failure/limit:** feature graph drift can remove padding imports or alter closure; no deployed middleware is inferred. **Validation/owner:** selected-feature Cargo resolution and layer tests; auth/host owners. **Evidence:** REAPI `Cargo.toml:13-50`, `src/timing_padding_wiring.rs`.

[Index](#b03)

<a id="rel-015"></a>
### REL-015 — Root workspace declaration
**Type/direction/key:** dependency, root workspace → REAPI member/alias; `repo:1232040291:relation:corelink-server-to-corelink-reapi-workspace-member`. **Activation/contract:** Cargo membership, not shipped inclusion. **Failure/limit:** path/package rename changes resolution. **Validation/owner:** root manifest and inverse census; workspace owner. **Evidence:** root `Cargo.toml:90,558`.

[Index](#b03)

<a id="rel-016"></a>
### REL-016 — Independent fuzz harness
**Type/direction:** test, REAPI package → `corelink-reapi-fuzz` path dependency. **Activation/contract:** separately selected fuzz manifest has proto, audit-ID and resource-name targets. **Failure/limit:** API drift breaks harness; no production call. **Validation/owner:** compile/run selected harness separately; REAPI/fuzz owner. **Evidence:** `crates/corelink-reapi/fuzz/Cargo.toml:27-43`.

[Index](#b03)

<a id="rel-017"></a>
### REL-017 — Adapter-host manifest edge
**Contract fingerprint:** B02 source-set SHA-256 `e1b1eb9622e520054631992034f1bb98d0918fbb7a0fa1fd6dc6f7eac9cec010` (AM, RM), matching adapter-host REL-035; approval remains separate.
**Type/direction/key:** dependency, REAPI package → adapter-host Cargo consumer; `repo:1232040291:relation:corelink-adapter-host-to-corelink-reapi-manifest`. **Peer:** [adapter-host REL-035](../corelink-adapter-host/BLAST_RADIUS.md#rel-035), reciprocal key confirmed in this documentation wave. **Activation/contract:** Cargo path declaration; source PAT calls are separate REL-020–024. **Failure/limit:** package identity/path drift can break resolution; selected build unknown. **Validation/owner:** inspect both manifests, then target-specific inverse query; adapter-host owner. **Evidence:** `crates/corelink-adapter-host/Cargo.toml:17`.

[Index](#b03)

<a id="rel-019"></a>
### REL-019 — OpenAPI source reference
**Type/direction:** external-contract source reference, REAPI named CAS read → OpenAPI source. **Activation/contract:** `tools/openapi/src/lib.rs` names REAPI in a route comment; generator invocation/publication unverified. **Failure/limit:** name/route drift can stale generated documentation. **Validation/owner:** compare source route and generated artifact under OpenAPI owner. **Evidence:** `tools/openapi/src/lib.rs:176`.

[Index](#b03)

<a id="rel-020"></a>
### REL-020 — Cargo adapter PAT call
**Contract fingerprint:** B02 source-set SHA-256 `d653e4ecccf0cc8a5b87188cbdd75826daf4bcb56f6e6d1963f5f3e47241deb4` (CaB, CaP, RP), matching adapter-host REL-036.
**Type/direction/key:** runtime-call, REAPI `PatValidator` → adapter-host Cargo `TenantResolver`; `repo:1232040291:boundary:reapi-adapter-host-pat-cargo`. **Peer:** [adapter-host REL-036](../corelink-adapter-host/BLAST_RADIUS.md#rel-036). **Activation/contract:** `resolve` calls `authenticate(token,"cargo-adapter-host")` in `spawn_blocking`, returns tenant UUID string. **Failure/limit:** invalid PAT → `InvalidPat`; join/other auth error → `Backend`; no live call proved. **Validation/owner:** bridge resolver tests; adapter-host and REAPI PAT owners. **Evidence:** `crates/corelink-adapter-host/src/cargo/bridge.rs:154-177`.

[Index](#b03)

<a id="rel-021"></a>
### REL-021 — Brew adapter PAT call
**Contract fingerprint:** B02 source-set SHA-256 `b690335a2c6dc2b53b918798b2d019a578bbc8fc9264a290b46b96b2c7c50738` (BrB, BrP, RP), matching adapter-host REL-037.
**Type/direction/key:** runtime-call, REAPI `PatValidator` → adapter-host Brew `TenantResolver`; `repo:1232040291:boundary:reapi-adapter-host-pat-brew`. **Peer:** [adapter-host REL-037](../corelink-adapter-host/BLAST_RADIUS.md#rel-037). **Activation/contract:** `resolve` calls `authenticate(token,"brew-adapter-host")` in `spawn_blocking`, returns tenant UUID string. **Failure/limit:** invalid PAT → `InvalidPat`; join/other auth error → `Backend`; runtime unverified. **Validation/owner:** bridge resolver tests; adapter-host and REAPI PAT owners. **Evidence:** `crates/corelink-adapter-host/src/brew/bridge.rs:135-158`.

[Index](#b03)

<a id="rel-022"></a>
### REL-022 — npm adapter PAT call
**Contract fingerprint:** B02 source-set SHA-256 `beec04845d4fc00ba79b5e46dba33f195553287fa94fa2aefad823f153aa9570` (NpB, NpP, RP), matching adapter-host REL-038.
**Type/direction/key:** runtime-call, REAPI `PatValidator` → adapter-host npm `TenantResolver`; `repo:1232040291:boundary:reapi-adapter-host-pat-npm`. **Peer:** [adapter-host REL-038](../corelink-adapter-host/BLAST_RADIUS.md#rel-038). **Activation/contract:** `resolve` calls `authenticate(token,"npm-adapter-host")` in `spawn_blocking`, returns typed `TenantId`. **Failure/limit:** invalid PAT, other auth and join faults map to `NpmAdapterError::Auth`; runtime unverified. **Validation/owner:** npm bridge tests; adapter-host and REAPI PAT owners. **Evidence:** `crates/corelink-adapter-host/src/npm/bridge.rs:258-279`.

[Index](#b03)

<a id="rel-023"></a>
### REL-023 — pip adapter PAT call
**Contract fingerprint:** B02 source-set SHA-256 `08f04e18eb83cc33ea587d281f5d3dcd78582dc886de5ec22f704b46693aa3ae` (PiB, PiP, RP), matching adapter-host REL-039.
**Type/direction/key:** runtime-call, REAPI `PatValidator` → adapter-host pip `TenantResolver`; `repo:1232040291:boundary:reapi-adapter-host-pat-pip`. **Peer:** [adapter-host REL-039](../corelink-adapter-host/BLAST_RADIUS.md#rel-039). **Activation/contract:** `resolve` calls `authenticate(token,"pip-adapter-host")` in `spawn_blocking`, returns typed `TenantId`. **Failure/limit:** invalid PAT, other auth and join faults map to `PipAdapterError::Auth`; runtime unverified. **Validation/owner:** pip bridge tests; adapter-host and REAPI PAT owners. **Evidence:** `crates/corelink-adapter-host/src/pip/bridge.rs:247-268`.

[Index](#b03)

<a id="rel-024"></a>
### REL-024 — OCI adapter PAT call
**Contract fingerprint:** B02 source-set SHA-256 `11243c55312c095d8da319a63f926660b2d25b84afc7b35cc499294b5146bf46` (OcB, OcP, RP), matching adapter-host REL-040.
**Type/direction/key:** runtime-call, REAPI `PatValidator` → adapter-host OCI `resolve_pat`; `repo:1232040291:boundary:reapi-adapter-host-pat-oci`. **Peer:** [adapter-host REL-040](../corelink-adapter-host/BLAST_RADIUS.md#rel-040). **Activation/contract:** unwraps `SecretWrap`, calls `authenticate(token,"oci-adapter-host")` in `spawn_blocking`, returns typed `TenantId`. **Failure/limit:** invalid PAT and other auth/join faults return distinct `PortResult` strings; runtime unverified. **Validation/owner:** OCI bridge tests; adapter-host and REAPI PAT owners. **Evidence:** `crates/corelink-adapter-host/src/oci/bridge.rs:292-312`.

[Index](#b03)

<a id="rel-025"></a>
### REL-025 — R2 reader contract
**Type/direction/key:** runtime-call, REAPI → `corelink-cas::r2_storage` façade → worker `R2Reader`; `repo:1232040291:boundary:reapi-worker-r2-read-001`. **Peer:** worker-side backlink pending. **Activation/contract:** `read_blob` calls `get(ctx,digest)` only after alive tenant metadata. **State/failure/limit:** R2 NotFound becomes `R2OrphanRow` miss; other R2 errors propagate. Provider owns object existence and region/prefix enforcement. **Validation/owner:** cross-tenant and missing-object tests with worker/CAS owners. **Evidence:** `src/read.rs:172-240`, `corelink-cas/src/lib.rs:109-110`. [Index](#b03)

<a id="rel-026"></a>
### REL-026 — Read authorization metadata lookup
**Type/direction/key:** runtime-call, REAPI → `corelink-meta::MetaStore::get`; `repo:1232040291:boundary:reapi-meta-read-get-001`. **Peer:** meta-side backlink pending. **Activation/contract:** `read_blob` constructs `BlobMetaKey(ctx.tenant_id(),digest)` before R2. **State/failure/limit:** absent/tombstoned row returns miss without R2; metadata transport error propagates. Provider owns actual row state. **Validation/owner:** cross-tenant/tombstone tests with meta owner. **Evidence:** `src/read.rs:172-218`, `corelink-meta/src/store.rs:144-155`. [Index](#b03)

<a id="rel-027"></a>
### REL-027 — Discovery metadata lookup
**Type/direction/key:** runtime-call, REAPI → `corelink-meta::MetaStore::get`; `repo:1232040291:boundary:reapi-meta-find-missing-get-001`. **Peer:** meta-side backlink pending. **Activation/contract:** `find_missing_with_sizes` issues tenant/digest lookups for distinct keys and retains per-input size flags. **State/failure/limit:** first metadata error aborts batch; lookup parallelism is bounded by finder, while provider row state is unknown. **Validation/owner:** duplicate-size and fault tests with meta owner. **Evidence:** `src/find_missing.rs:227-330`. [Index](#b03)

<a id="rel-028"></a>
### REL-028 — BLAKE3 audit identity material
**Type/direction:** dependency/data, registry `blake3` and `uuid` → handler audit IDs → `AuditEvent.id`/CloudEvents `id`. **Activation/contract:** write handlers hash client request ID, tenant UUID and digest under the non-slot domain, stamp UUIDv7 bits and construct `Uuid::from_bytes` for the outbox PK. Batch read emitters add `slot_idx` under a distinct domain; they log envelopes without staging outbox rows.

**Limit:** repeated inputs reproduce IDs; distinct inputs do not guarantee collision freedom. The UUID PK and REL-003's `(request_id,event_type)` dedup key are separate; provider behavior remains meta-owned. **Validation/owner:** compare ID vectors, slot behavior and `CommitPutRequest` with REAPI/meta owners. **Evidence:** `Cargo.toml`, `src/handler/{audit_emit,audit_emit_batch,per_blob,bytestream}.rs`, `src/orchestrator.rs:284-320`, `corelink-meta/src/store.rs:91-115`. [Index](#b03)

<a id="rel-029"></a>
### REL-029 — Audit JSON serialization
**Type/direction:** dependency/data, registry `serde_json` → `AuditEnvelope::to_json`. **Activation/contract:** write orchestration serializes before R2 and passes compact JSON to `CommitPutRequest`; read emitters serialize before tracing. **Failure/limit:** write serialization error aborts before R2; read single-shot emitters skip logging on error and batch emitters use an empty string fallback. No outbox delivery follows from JSON construction. **Validation/owner:** canonical vectors and per-call error branches; REAPI/meta owners. **Evidence:** `src/audit.rs:315-320`, `src/orchestrator.rs:284-320`, `src/handler/{audit_emit,audit_emit_batch}.rs`. [Index](#b03)

<a id="rel-030"></a>
### REL-030 — Source-only attempt builders
**Type/direction:** internal data contract, `AuditEnvelopeBuilder::{poisoning_attempt,cross_tenant_attempt}` → `AuditEnvelope`. **Activation/contract:** callers can construct distinct attempt types with forced zero size; these constructors have no production handler call in the inspected source. **Failure/limit:** construction has no I/O; `to_json` follows REL-029 if called. Neither declaration proves an outbox row or tracing emission. **Validation/owner:** builder vectors and call-site census; REAPI audit owner. **Evidence:** `src/audit.rs:179-224,247-319`, `rg` call-site census. [Index](#b03)

<a id="rel-031"></a>
### REL-031 — Completed-read tracing audit
**Type/direction:** internal telemetry, `AuditEnvelopeBuilder::read_completed` → read emitters → `tracing` target `corelink.audit`. **Activation/contract:** ByteStream and HTTP emit after full stream completion; batch reads use slot-aware IDs after delivery. **Failure/limit:** JSON error suppresses single-shot trace; no `MetaStore::commit_put` or outbox staging occurs in these read paths. Log sink/delivery remains UNKNOWN. **Validation/owner:** read handler and batch-slot tests; REAPI/observability owners. **Evidence:** `src/handler/{audit_emit,audit_emit_batch,bytestream_stream,batch_read_compose}.rs`, `src/http_read.rs:321-439`. [Index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

| Destination | Witness REL path | Condition and potential effect | Containment / validation |
|---|---|---|---|
| Worker R2 backend | REL-001 → REL-002/011; REL-026 → REL-025 | Verified bytes can reach writer; authorized reads can reach reader | Stop at worker/CAS owner; conditional-put, region and key tests; no observed object |
| D1 metadata and audit outbox | REL-028 → REL-003; REL-001 → REL-002 → REL-003 | BLAKE3-derived UUID becomes the write outbox PK, while the per-blob request/event pair is a separate dedup key; successful writer allows metadata/audit commit and failed commit after fresh writer yields orphan candidate | Stop at meta owner; ID vectors, transaction/idempotency/failure tests; delivery excluded |
| Read audit trace | REL-028/029 → REL-031 | Completed read builds JSON and emits a tracing field after stream completion; attempt builders REL-030 are source-only | Stop at tracing sink; test output shape and slot IDs, delivery unknown |
| Read transport | REL-026 → REL-004/025 → REL-007 | Metadata lookup controls R2 read and tonic/Axum mapping when `host-server` is selected | Host registration and read handler tests; serving unknown |
| Discovery transport | REL-005 → REL-006 → REL-027 → REL-007 | PAT scope, per-slot size and metadata determine reported misses | Scope/duplicate-size tests; provider data unknown |
| Adapter callers | REL-017 → REL-020–024 | Package resolution enables five distinct PAT source calls | Adapter-host owner, each bridge test; no selected caller/runtime proof |
| Fuzz/OpenAPI | REL-016 or REL-019 | Independent harness or named documentation source can drift | Run selected harness/generator under their owners; output unknown |

These are causal source paths, not a resolved transitive dependency graph. An external provider boundary terminates the claim at its trait/request; no backend or customer effect is inferred past that point.

<a id="b05"></a>
## B05 — Change, impact and validation matrix

| Changed surface | Direct REL / possible effect | Required bounded validation and owner |
|---|---|---|
| Digest/body or write order | 001/002/003; accepted data, orphan and idempotency branch | Verify source order; `prop_cas`, `prop_idempotency`, failure injection; hash, worker and meta owners |
| R2 key, region or result API | 002/025/011/013; tenant isolation or `Fresh/Duplicate` mapping | Compare provider contract; `prop_cas_read`, `prop_cross_tenant_read`; worker/CAS owners |
| Metadata or audit request | 003/026/027; row/outbox key, authorization lookup, conflict | Compare `CommitPutRequest`, schema and fake/D1 implementations; `prop_idempotency`, `integration_bit_rot`; meta owner |
| BLAKE3 audit ID or batch slot input | 028/003; CE ID, write outbox PK, retry stability or duplicate-slot identity | Compare non-slot and slot ID vectors, UUID markings and outbox conflict behavior; REAPI/meta owners |
| Audit JSON, attempt builders or read trace | 029/030/031; serialized shape or post-stream trace behavior | Compare builder vectors and read emitters; REAPI/observability owners, no outbox inference |
| PAT trait/scope | 005/020–024; adapter denial and tenant resolution | Exact-scope test plus five bridge tests; REAPI PAT and adapter-host owners |
| Discovery and resource grammar | 006/027 plus REAPI `handler/helpers.rs`; per-slot misses or accepted names | `prop_find_missing_batch`, `find_missing_handler_e2e`, resource parser/fuzz; REAPI owner |
| Feature/proto/HTTP path | 004/007/014; compile/wire surface | Selected-feature build and handler/capability/timing tests; host/wire owner, N/N-1 review |
| Workspace/fuzz/OpenAPI | 015/016/017/019; resolution or generated artifact | Inverse Cargo query, selected harness/generator; workspace/adapter/OpenAPI owners |

<a id="b06"></a>
## B06 — Coverage and unknowns

The B02 census reconciles every discovered static element, with structural unknown count **0** for its stated scope. REL-001 and REL-017/020–024 have reciprocal keys. Worker peers for REL-002/025 and meta peers for REL-003/026/027/028 are **UNKNOWN / UNAPPROVED**: no reciprocal backlink or source-set contract fingerprint is established in those provider documents. Former duplicate REL-008/009/010 call sites belong to the atomic hash, R2 and metadata records.

Selected Cargo graph, deployed features, live routes, PAT acceptance, R2/D1 persistence, outbox delivery, GC and consumers outside the searched workspace are **UNKNOWN**. A cold reviewer must rerun inverse queries and inspect peer docs before promotion.
