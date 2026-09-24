---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-tenant-path
manifest: crates/tenant-path/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: w007-tenant-path-static-source-20260920
---

# corelink-tenant-path — blast radius

SOURCE-only atomic relationship map. Declarations and source references identify review boundaries; they do not prove build selection, storage effects, deployment, or runtime reachability.

[Scope](#b01) · [Method/populations](#b02) · [Relations](#b03) · [Propagation](#b04) · [Change impact](#b05) · [Coverage](#b06)

<a id="b01"></a>
## B01 — Scope and ownership boundary

This map covers package `corelink-tenant-path`, manifest `crates/tenant-path/Cargo.toml`, its library source, three benches, two integration-test files, and its independent fuzz peer. The package owns the typed derivation and cache contracts; it does not own storage, worker, service, authentication, deployment, or key-provenance behavior.

The source pin is `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. All facts below are SOURCE evidence. A direct manifest edge, import, re-export, test fixture, or typed value does not prove target selection, execution, persistence, or production reachability.

`src/lib.rs` re-exports public names from private `prefix` and `cache` modules. The re-export facade is a public-contract relation, not an implementation-ownership transfer.

Evidence: `crates/tenant-path/{Cargo.toml,src/lib.rs,src/prefix.rs,src/cache.rs,src/error.rs}`.

<a id="b02"></a>
## B02 — Method and populations

| Population | Count | Static result | Boundary |
|---|---:|---|---|
| Package manifest | 1 | one library package; 6 normal and 3 dev dependencies | no resolution |
| Local implementation | 4 files | `lib`, `prefix`, `cache`, `error` | no deployment |
| Own verification surfaces | 3 benches + 2 integration tests | derivation, cache, and parity sources | not executed here |
| Reverse Cargo consumers | 7 packages | server, multipart, REAPI, worker, auth, e2e, worker-fuzz | declaration only |
| Consumer source files with symbols | 93 | 21 server, 8 multipart, 17 REAPI, 39 worker, 2 auth, 4 e2e, 2 worker-fuzz | not call reachability |
| Outside-Cargo parity | 4 named surfaces | two Worker ports, vector JSON, parity test | no runtime proof |
| Resolved/runtime evidence | 0 | no metadata, build, run, provider, or deployment query | UNKNOWN |

The seven reverse declarations are `corelink-server` (`crates/corelink-container/Cargo.toml`), `corelink-r2-multipart`, `corelink-reapi`, `corelink-worker`, `corelink-auth`, `e2e-tenant-isolation`, and `corelink-worker-fuzz`. Directory names are not package identities.

The 93-file count is a literal symbol census at the source pin: 21 server, 8 multipart, 17 REAPI, 39 worker, 2 auth, 4 e2e, and 2 worker-fuzz files. It includes imports, types, comments, and tests; it is not a count of runtime calls. The two previously omitted server files are `storage/r2_s3_parts/ac_handler.rs` and `storage/r2_s3_parts/cas_builder.rs`.

The method was manifest inspection plus `git grep` for `corelink_tenant_path`, `derive_prefix`, `TenantPrefix`, and `TenantDerivationKey`, followed by source reading at the listed paths. Cargo resolution, compilation, fuzzing, CI queries, provider access, and runtime observation were not performed.

Evidence: source-pinned manifests, source files, tests, benches, fuzz targets, `worker/src/lib/{edge_find_missing,edge_public_read}.ts`, `worker/tests/vectors/tenant_prefix_vectors.json`, and `crates/tenant-path/tests/edge_parity_vectors.rs`.

The parent artifact is pinned to `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`; the independent fuzz artifact is pinned to `1177dad2ca2a9f21c29b5a118aa7944b77147798`. The scoped diff for `crates/tenant-path` is one metadata-only change: the Cargo `repository` URL changes from `HuGR-Labs/corelink-server` to `HuGR-dev/corelink-server`. No parent algorithm or fuzz-target source changed in that diff. Re-check this drift before approval; shared semantic facts remain SOURCE-only.

The independent fuzz peer [REL-013](#rel-013) and its `corelink-tenant-path-fuzz` BLAST `REL-001` share identity `repo:1232040291:boundary:tenant-path-derive-prefix-001`. Its fingerprint is `corelink-tenant-path::derive_prefix(&TenantDerivationKey, Uuid) -> TenantPrefix`, using HMAC-SHA256 and the first 16 URL-safe base64 characters. Dependency is fuzz → parent; data is bytes → typed key/UUID → prefix; impact is parent contract change → oracle change.

The exact peer surfaces are `crates/tenant-path/src/prefix.rs::derive_prefix`, `crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs`, and `crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs`. The parent owns the API/algorithm; the fuzz package owns slicing/assertions. No fuzz run or runtime observation is claimed.

<a id="b03"></a>
## B03 — Atomic direct relationships

| ID | Boundary | Dependency direction | Contract owner / peer |
|---|---|---|---|
| [REL-001](#rel-001) | server R2/KV prefix | server → tenant-path | tenant-path / server REL-041 |
| [REL-002](#rel-002) | server multipart/CAS | server → tenant-path | tenant-path / server REL-051 |
| [REL-003](#rel-003) | server DSR prefix | server → tenant-path | tenant-path / server REL-052 |
| [REL-004](#rel-004) | server BYOK prefix | server → tenant-path | tenant-path / server REL-053 |
| [REL-005](#rel-005) | server GC prefix | server → tenant-path | tenant-path / server REL-054 |
| [REL-006](#rel-006) | multipart object key | multipart → tenant-path | tenant-path / package tests |
| [REL-007](#rel-007) | REAPI TDK type seam | REAPI → tenant-path | tenant-path / no direct derive call |
| [REL-008](#rel-008) | worker prefix/context | worker → tenant-path | tenant-path / worker |
| [REL-009](#rel-009) | auth facade | auth → tenant-path | tenant-path / re-export only |
| [REL-010](#rel-010) | isolation harness | e2e → tenant-path | tenant-path / test harness |
| [REL-011](#rel-011) | worker fuzz harnesses | fuzz → tenant-path | tenant-path / harness |
| [REL-012](#rel-012) | local cache | cache → local derive | tenant-path |
| [REL-013](#rel-013) | independent tenant-path fuzz | fuzz → tenant-path | shared fuzz peer |

Each record separates dependency, data, impact, activation, failure, and boundary. The source does not establish runtime behavior.

<a id="rel-001"></a>
### REL-001 — server R2/KV prefix boundary

**Identity:** `repo:1232040291:boundary:tenant-path-server-r2-kv-prefix-001`; peer is `corelink-server` BLAST `REL-041`.

**Dependency / data / impact:** server → tenant-path; TDK + UUID → `derive_prefix` → R2/KV prefix; algorithm or width changes can strand existing keys.

**Surface/activation:** `storage/r2_kv.rs:{129,462,467,527,908}`; selected storage path only. No bucket, object, secret, mount, or runtime call was observed. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — server multipart/CAS prefix boundary

**Identity:** `repo:1232040291:boundary:tenant-path-server-multipart-prefix-001`; peer is server BLAST `REL-051`.

**Dependency / data / impact:** server → tenant-path; TDK/UUID → prefix → multipart/CAS key; `_public` is a special namespace and must not fall back to a tenant prefix. Width or fallback changes can cause collision, isolation, or compatibility failure.

**Surfaces:** `storage/r2_s3_parts/{ac_handler.rs:20,cas_builder.rs:205,cas_helpers.rs:{59,72,130}}`; CAS erase/scrub `{routes/cas_erase/b126_m2_impl_02_part2.rs:128,routes/cas_scrub.rs:{319,744}}`; tests `storage/r2_s3_parts/tests_2.rs:{169,174,228}`. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — server DSR prefix boundary

**Identity:** `repo:1232040291:boundary:tenant-path-server-dsr-prefix-001`; peer is server BLAST `REL-052`.

**Dependency / data / impact:** server DSR → tenant-path; TDK + tenant UUID → prefix → AC/CAS/legal-hold erase key; derivation drift can make erase miss or address another namespace.

**Surfaces:** `routes/dsr/adapter_r2_ac.rs:{216,237}`; `adapter_r2_cas.rs:{186,211}`; `adapter_r2_cas_legalhold.rs:{196,229}`. No erase execution or storage observation was performed. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — server BYOK prefix boundary

**Identity:** `repo:1232040291:boundary:tenant-path-server-byok-prefix-001`; peer is server BLAST `REL-053`.

**Dependency / data / impact:** server BYOK workers → tenant-path; TDK + tenant UUID → prefix → BYOK activation/backfill/purge key paths; drift can break convergent address compatibility.

**Surfaces:** `storage/byok_activation_worker.rs:{297,1834,1910,2330}`; `byok_backfill_d1.rs:151`; `byok_purge_io.rs:301`. KMS, D1, provider, and runtime activation are not evidenced. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — server GC prefix boundary

**Identity:** `repo:1232040291:boundary:tenant-path-server-gc-prefix-001`; peer is server BLAST `REL-054`.

**Dependency / data / impact:** server GC → tenant-path; TDK + tenant UUID → prefix → sweep key; derivation drift can leave objects undiscovered or target the wrong namespace.

**Surface:** `src/gc_sweep/part-04.rs:7`. Binary selection, sweep execution, and remote deletion were not observed. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — multipart typed prefix and object-key boundary

**Dependency / data / impact:** multipart → tenant-path; `TenantPrefix` → adapter/object-key path; typed prefix changes can alter multipart key grammar and compatibility.

**Surfaces:** `src/adapter.rs:{26,45,61}`; `src/object_key.rs:{34,62}`; in-memory/tests `src/in_memory.rs:588`, `src/object_key.rs:161`, `tests/{chaos_r2_multipart,prop_r2_multipart}.rs:{37,42}`; examples `examples/{cross_tenant_replay,happy_path,orphan_sweeper}.rs:{27,29}`. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — REAPI typed TDK seam

**Dependency / data / impact:** REAPI → tenant-path; TDK is stored or constructed in handlers/tests and can break type or fixture compatibility when changed.

**Surfaces:** `src/handler.rs:{178,215,260}`; fixtures `src/{find_missing,orchestrator,read}.rs`; integration tests `tests/{batch_read_blobs_e2e,capabilities,find_missing_handler_e2e,handler_e2e,integration_bit_rot,prop_cas,prop_cas_read,prop_cross_tenant_read,prop_find_missing_batch,prop_idempotency,read_handler_e2e,timing_padding_grpc_e2e}.rs`. No direct derive call was found. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — worker prefix/context boundary

**Dependency / data / impact:** worker → tenant-path; TDK + UUID → `TenantPrefix`/context → storage and REAPI key paths; type, derivation, or width changes can alter worker key construction.

**Surfaces:** `src/tenant.rs:57`; `src/middleware/auth_ctx.rs:259`; `src/storage/key.rs:{83,110}`; `src/cache/negative.rs:{413,426,441}`; AC/CAS derivation tests `src/reapi/{ac/meta.rs:577,ac/ttl/{evict,worker}.rs:{349,464},cas/{assembler,chunk_store,session,sweeper}.rs:{274,492,504,697}}`; worker tests `{prop_ac_ttl,prop_auth_full,prop_auth_middleware}.rs:{98,540,645}`. [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — auth re-export facade

**Dependency / data / impact:** auth → tenant-path; `pub use corelink_tenant_path::*` exposes the parent surface through `src/tenant_path.rs`; public-name or visibility changes can break the facade.

**Surfaces:** `crates/corelink-auth/Cargo.toml`; `crates/corelink-auth/src/tenant_path.rs:15`. This does not transfer algorithm ownership or prove an auth runtime call. [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — tenant-isolation harness

**Identity:** `repo:1232040291:boundary:e2e-tenant-isolation-tenant-path-001`; peer is `e2e-tenant-isolation` BLAST `REL-001`.

**Dependency / data / impact:** `tests/e2e-tenant-isolation` → `corelink-tenant-path::{derive_prefix,TenantDerivationKey,TenantPrefix}`; fixed test TDK + UUID → typed `TenantPrefix` → fake-store ownership and cross-tenant assertions; derivation/API changes alter the local namespace oracle. Tenant-path owns the API/algorithm; the harness owns fixtures/assertions.

**Surfaces:** `tests/e2e-tenant-isolation/src/tenants.rs:{18,55,82,83,95}` and its manifest declaration. Static source boundary only: no test execution, production composition, or runtime reachability is established. [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — worker fuzz harnesses

**Dependency / data / impact:** worker-fuzz → tenant-path; fuzz bytes → TDK → `TenantCtx` → worker R2 writer/reader; parent or worker key changes can alter the oracle.

**Surfaces:** `crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs:{20,51}` and `r2_put_get_roundtrip.rs:{12,45}`. The harness does not call `derive_prefix` directly. No fuzz run is claimed. [Relation index](#b03)


<a id="rel-012"></a>
### REL-012 — local cache-to-derivation relation

**Dependency / data / impact:** `TenantPrefixCache` → local `derive_prefix`; `(TdkVersion,Uuid)` miss → typed prefix; key, eviction, or fallback changes alter local selection.

**Surface:** `crates/tenant-path/src/cache.rs:{82,100,117,120,132}`. No cache instance, rotation, contention, memory profile, or deployment is evidenced. [Relation index](#b03)


<a id="rel-013"></a>
### REL-013 — independent tenant-path fuzz peer

**Identity:** `repo:1232040291:boundary:tenant-path-derive-prefix-001`; the same key is required in `corelink-tenant-path-fuzz` BLAST `REL-001`.

**Dependency / data / impact:** fuzz → tenant-path; arbitrary bytes → typed TDK/UUID → `derive_prefix` → `TenantPrefix`; parent API, algorithm, encoding, or width changes alter the oracle.

**Surfaces:** parent `crates/tenant-path/src/prefix.rs::derive_prefix`; peer `fuzz_targets/{derive_prefix,derive_prefix_extended}.rs`. Peer owns slicing/assertions only. No fuzz run or runtime behavior is claimed. [Relation index](#b03)


<a id="b04"></a>
## B04 — Transitive propagation

| Population | Count | Propagation boundary | Evidence limit |
|---|---:|---|---|
| Direct Cargo consumers | 7 | package declarations above | no resolved graph |
| Direct semantic relation records | 13 | B03 records | no runtime reachability |
| Server downstream families | 7 | KV, multipart/CAS, erase/scrub, DSR, BYOK, GC, tests | grouped source only |
| Worker typed-prefix files | 39 | context, cache, AC/CAS, tests | imports/types may be dormant |
| Outside-Cargo parity surfaces | 4 | Worker ports and shared vectors | no execution |

The server relation propagates through storage key construction, multipart helpers, CAS erase/scrub, DSR adapters, BYOK workers, and GC sweep. These are separate downstream effects even when they share `derive_prefix`; each owner retains its storage, deletion, cryptographic, or operational contract.

The Worker TypeScript implementation in `worker/src/lib/edge_find_missing.ts` and `edge_public_read.ts` is a compatibility surface. `worker/tests/vectors/tenant_prefix_vectors.json` and `crates/tenant-path/tests/edge_parity_vectors.rs` are vector links, not calls from Rust consumers.

The auth package is a facade, not the implementation owner. REAPI and worker-fuzz carry typed inputs or contexts without proving direct algorithm invocation. The e2e package owns test orchestration and fakes, not production tenant isolation.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Material impact | Required validation |
|---|---|---|
| HMAC/input/encoding/width | prefix values and persisted key addresses | vectors, parent tests, server/multipart/worker key tests |
| `TenantDerivationKey` visibility/constructor | secret-bearing construction and consumers | API review, auth/REAPI/worker compile selections |
| `TenantPrefix` formatting/visibility | object-key and facade contracts | object-key, e2e, re-export compatibility tests |
| cache key/eviction/fallback | local value selection and counters | cache properties plus rotation review |
| manifest/features/target | selected consumers and fuzz paths | resolved inverse graph for each shipped target |
| worker parity/vector change | cross-language namespace compatibility | vector comparison and authorized runtime evidence |

For the server peers, validate `corelink-server` REL-041 and REL-051–REL-054 together with the matching parent identities. A revert of the algorithm does not restore already-written keys or wire compatibility; migration and rollback ownership remains with downstream storage operators.

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented | Runtime observed | Unknown |
|---|---:|---:|---:|---|
| Local package surfaces | 9 named groups | 9 | 0 | selected targets/features |
| Reverse package declarations | 7 | 7 | 0 | resolved inverse graph |
| Semantic relation groups | 13 | 13 | 0 | call reachability |
| Consumer symbol files | 93 | count and routes | 0 | generated/dynamic edges |
| Outside-Cargo parity | 4 | 4 | 0 | executed parity/deployment |
| Storage/provider/runtime | 0 | 0 | 0 | buckets, keys, TDK source, rollout |

The census is complete for the seven declared reverse manifests and the named static search populations at this source pin. It is not proof of all aliases, feature selections, generated code, dynamic dispatch, deployment, or external consumers.

Unknowns include resolved dependency versions, selected targets, TDK provenance and rotation, actual callers and values, storage/bucket state, key migration, cache residency, tenant authorization, CI/fuzz outcomes, Worker execution, rollback, and production reachability. Obtain resolved-graph, execution, provider, deployment, or runtime evidence from the responsible owner before making those claims.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-tenant-path/SKILL.md#s01)
