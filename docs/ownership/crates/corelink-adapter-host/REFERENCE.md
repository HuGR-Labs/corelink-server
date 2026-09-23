---
schema: corelink-ownership/1.1
document: reference
package: corelink-adapter-host
manifest: crates/corelink-adapter-host/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: draft
evidence_set: adapter-host-static-20260920
---

# corelink-adapter-host — ownership reference

This H-profile reference records observable source contracts for the hybrid adapter composition package. It is static-graph evidence only: package-manager wire interoperability, upstream availability, DNS resolution, deployed composition, and runtime audit delivery remain unobserved.

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) · [Contracts](#r04) · [State and invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and role

| Field | Source-backed observation |
|---|---|
| Manifest | `crates/corelink-adapter-host/Cargo.toml` names the package and declares handler-CAS, worker, REAPI, audit, and core dependencies. |
| Manifest census | Root `Cargo.toml` declares `corelink-adapter-host` as a workspace member and workspace dependency; `crates/corelink-container/Cargo.toml` directly declares it. This is a manifest census, not evidence of semantic use. |
| Public modules | `lib.rs` exposes `brew`, `cargo`, `npm`, `oci`, `overload`, `pip`, and `upstream_ssrf`. |
| Role | The library source describes bridges from adapter-local ports to workspace Stage-1 SPI traits; a binary composition is outside this package's source proof. |
| Families | Cargo, Brew, npm, OCI, and pip each have auth/config/ports/bridge/server/error/audit source; protocol helpers vary by family. |

Falsifier: removal of a public module or a manifest dependency disproves this map. Source: `Cargo.toml`; `src/lib.rs`.

<a id="r02"></a>
## R02 — Boundaries and ownership

The package owns translation and HTTP-facing adapter code, not the implementations of handler-CAS, Worker, REAPI, audit, or core. `lib.rs` states that production binaries compose bridges at boot; this statement is not proof of a particular production binary. Local port traits isolate each adapter family, while bridge implementations name the workspace SPI types.

The material boundaries are credentials, tenant resolution, CAS/KV calls, audit emission, digest material, and outbound upstream requests. A source change crossing one needs the related family plus its bridge/error/server path reviewed. Falsifier: a direct SPI import in a local adapter-only file, or a mutation path lacking the documented local audit call, changes the boundary analysis. Source: `src/lib.rs`; `src/{cargo,brew,npm,pip,oci}/ports.rs`; `bridge.rs`; `audit.rs`.

<a id="r03"></a>
## R03 — Implementation map

| Family | Observable source components | Special flow |
|---|---|---|
| Cargo | auth, bridge, config, ports, server, translate, audit | sccache key translation and CAS bridge |
| Brew | auth, bridge, bottle, upstream, config, ports, server, audit | bottle URL/digest and read-through |
| npm | auth, bridge, metadata, search, tarball, upstream, config, ports, server, audit | registry metadata/tarball and KV/CAS |
| pip | auth, bridge, index, pep503_html, wheel, upstream, config, ports, server, audit | PyPI index/wheel and KV/CAS |
| OCI | auth, bridge, digest, pull, push, tags, dispatch, config, ports, server, audit | distribution routes, blob sessions, manifests |

This is a file-tree map, not a claim that all routes are reachable. Source: `src/lib.rs` and files under the listed directories.

H-profile semantic-module inventory: 42 implementation modules with distinct route, auth, audit, bridge, protocol, or upstream behavior. Cargo has 5 (`audit`, `auth`, `bridge`, `server`, `translate`); Brew 6 (`audit`, `auth`, `bottle`, `bridge`, `server`, `upstream`); npm 8 (`audit`, `auth`, `bridge`, `metadata`, `search`, `server`, `tarball`, `upstream`); pip 8 (`audit`, `auth`, `bridge`, `index`, `pep503_html`, `server`, `upstream`, `wheel`).

OCI has 15 (`audit`, `auth`, `bridge`, `digest`, `pull`, `push`, `server`, `tags`, `pull/{blob,manifest}`, `push/{manifest,upload}`, `server/{core,dispatch,handlers}`). The count excludes family declaration files, `lib.rs`, `config`, `error`, `ports`, `overload`, and `upstream_ssrf`; it exceeds the H threshold of 20 without relying on the raw 65-file count. Source: `src/{cargo,brew,npm,pip,oci}.rs` and their declared child modules.

<a id="r04"></a>
## R04 — Public contracts

Cargo and Brew bridges construct `CasReadRequest`/`CasWriteRequest`, invoke `CasReadHandler`/`CasWriteHandler` via `tokio::task::spawn_blocking`, map handler `NotFound` to `Ok(None)`, and map other handler/join errors into local errors. npm, pip, and OCI bridges expose equivalent family-local CAS/blob translation; npm/pip/OCI include generic KV bridges because `KvBackend` is not object-safe according to `lib.rs`.

OCI has a baseline defect in `OciBlobBridge`: `open_upload` formats the UUID with the opening tenant but stores `HashMap<String, Vec<u8>>`; `append_chunk`, `finalize_upload`, and `cancel_upload` receive `_tenant` and do not compare it to session ownership. The tenant supplied to `finalize_upload` is used for the eventual `CasWriteRequest`; that does not prove session ownership. Required contract: bind `upload_uuid` to the opening tenant and reject append/finalize/cancel when the caller tenant differs.

The OCI port specifies a separate tenant quota contract: `finalize_upload` receives the verified token's resolved `storage_cap_bytes` and should reserve bytes against that cap (`Some(0)` means unlimited; `None` is indeterminate). The concrete `OciBlobBridge::finalize_upload` names the argument `_storage_cap_bytes`, ignores it, and builds `CasWriteRequest::new` without `with_storage_quota_bytes`. The current bridge therefore does not propagate this cap to the CAS write; this is a source-observed policy gap, not proof of any deployed quota result. Source: `src/oci/ports.rs:90-105`; `src/oci/bridge.rs:121-155`.

Invariant: a bridge edit that directly blocks the async runtime or changes the `NotFound` distinction is a contract change. OCI invariant: session authorization must be enforced at every session operation, not only at final CAS write. Falsifier: the cited bridge source no longer constructs the documented request or maps the result as stated; until the OCI defect is fixed, its current code itself falsifies the required session-tenant invariant. Source: `src/lib.rs`; `src/cargo/bridge.rs`; `src/brew/bridge.rs`; `src/npm/bridge.rs`; `src/pip/bridge.rs`; `src/oci/bridge.rs:95-164`.

### Public adapter-port and HTTP composition contracts

API index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) · [API-016](#api-016) · [API-017](#api-017) · [API-018](#api-018) · [API-019](#api-019) · [API-020](#api-020).

| ID | Exact symbols and source contract | Bounds / errors / effects | Evidence and links |
|---|---|---|---|
| <a id="api-001"></a>API-001 Cargo CAS | `cargo::CasStore`: `async fn get(&self, &str, &str) -> Result<Option<Vec<u8>>, CasError>`; `async fn put(&self, &str, &str, Vec<u8>) -> Result<(), CasError>`; `async fn exists(&self, &str, &str) -> Result<bool, CasError>`. Inputs are tenant ID then digest hex. | `None` is a miss; `Err` is backend failure. Default `exists` reads the body; bridge overrides via SPI `exists`. Write changes CAS; synchronous SPI bridge calls use `spawn_blocking`. | `src/cargo/{ports,bridge}.rs`; INV-001; [REL-008](BLAST_RADIUS.md#rel-008). [API index](#r04). |
| <a id="api-002"></a>API-002 Cargo auth | `cargo::TenantResolver`: `async fn resolve(&self, &str) -> Result<String, TenantResolveError>`; `async fn resolve_with_capability(&self, &str) -> Result<ResolvedTenant, TenantResolveError>`. Input is PAT plaintext. | `ResolvedTenant` carries tenant ID, `can_write`, and `runner_job`; default sets `false` and `true` respectively, denying uncertain write/evict authority. Invalid PAT and backend errors remain distinct. | `src/cargo/ports.rs:84-159`; [REL-008](BLAST_RADIUS.md#rel-008). [API index](#r04). |
| <a id="api-003"></a>API-003 Brew CAS/read-through | `brew::CasStore`: `async fn get(&self, &str, &str) -> Result<Option<Vec<u8>>, CasError>`; `async fn put(&self, &str, &str, Vec<u8>) -> Result<(), CasError>`. `BottleService`: `async fn fetch(&self, &str, &str) -> Result<CacheFetch, BrewAdapterError>`. Inputs are tenant and CAS key or raw path. | `CacheFetch { bytes, is_hit }` distinguishes CAS hit from upstream fill; invalid repo path errors before cache/network. `put` persists; `fetch` may fetch upstream. | `src/brew/{ports,bottle,bridge}.rs`; INV-002; [REL-009](BLAST_RADIUS.md#rel-009). [API index](#r04). |
| <a id="api-004"></a>API-004 Brew auth | `brew::TenantResolver`: `async fn resolve(&self, &str) -> Result<String, TenantResolveError>`; `async fn resolve_with_capability(&self, &str) -> Result<ResolvedTenant, TenantResolveError>`. Input is PAT plaintext. | Default `can_write=false`; invalid PAT and resolver backend failure are separate variants. No caller may infer write authority from `resolve` alone. | `src/brew/ports.rs:82-139`; [REL-009](BLAST_RADIUS.md#rel-009). [API index](#r04). |
| <a id="api-005"></a>API-005 npm ports | `npm::CasStore`: `async fn get(&self,&TenantId,&Digest)->Result<Option<Vec<u8>>,NpmAdapterError>`; `async fn put(&self,&TenantId,&Digest,Vec<u8>)->Result<(),NpmAdapterError>`. `npm::KvStore`: `async fn get(&self,&TenantId,&str)->Result<Option<(Vec<u8>,u64)>,NpmAdapterError>`; `async fn put(&self,&TenantId,&str,Vec<u8>,u64)->Result<(),NpmAdapterError>`. | `None` is miss, not error; KV timestamp is Unix milliseconds. `put` mutates scoped storage; CAS/KV faults use distinct error variants. | `src/npm/ports.rs:36-100`; INV-003; [REL-010](BLAST_RADIUS.md#rel-010). [API index](#r04). |
| <a id="api-006"></a>API-006 npm auth | `npm::TenantResolver`: `async fn resolve(&self,&str)->Result<TenantId,NpmAdapterError>`; `async fn resolve_with_capability(&self,&str)->Result<ResolvedTenant,NpmAdapterError>`. Input is PAT plaintext. | `ResolvedTenant { tenant_id, can_write }`; default capability is false. PAT failure maps to `Auth`; no storage operation follows failed resolution. | `src/npm/ports.rs:104-150`; [REL-010](BLAST_RADIUS.md#rel-010). [API index](#r04). |
| <a id="api-007"></a>API-007 pip ports | `pip::CasStore`: `async fn get(&self,&TenantId,&Digest)->Result<Option<Vec<u8>>,PipAdapterError>`; `async fn put(&self,&TenantId,&Digest,Vec<u8>)->Result<(),PipAdapterError>`. `pip::KvStore`: `async fn get(&self,&TenantId,&str)->Result<Option<(Vec<u8>,u64)>,PipAdapterError>`; `async fn put(&self,&TenantId,&str,Vec<u8>,u64)->Result<(),PipAdapterError>`. | CAS/KV miss is `None`; KV timestamp is Unix milliseconds for TTL logic. `put` mutates scoped storage; CAS/KV errors differ. | `src/pip/ports.rs:36-102`; INV-004; [REL-011](BLAST_RADIUS.md#rel-011). [API index](#r04). |
| <a id="api-008"></a>API-008 pip auth | `pip::TenantResolver`: `async fn resolve(&self,&str)->Result<TenantId,PipAdapterError>`; `async fn resolve_with_capability(&self,&str)->Result<ResolvedTenant,PipAdapterError>`. Input is PAT plaintext. | `ResolvedTenant { tenant_id, can_write }`; default denies writes. PAT failure uses `Auth`. | `src/pip/ports.rs:106-153`; [REL-011](BLAST_RADIUS.md#rel-011). [API index](#r04). |
| <a id="api-009"></a>API-009 OCI open | `oci::BlobStore::open_upload(&self,&TenantId)->PortResult<String>` allocates an upload session ID. | Concrete bridge inserts an empty `Vec<u8>` under `{tenant}:{unix_ms_now()}` in a process-local map; same-tenant same-millisecond open overwrites. Lock failure returns `Err(String)`. | `src/oci/ports.rs:67-70`; `src/oci/bridge.rs:93-102`; [INV-005](#inv-005); [REL-007](BLAST_RADIUS.md#rel-007). [API index](#r04). |
| <a id="api-018"></a>API-018 OCI append | `oci::BlobStore::append_chunk(&self,&TenantId,&str,Bytes)->PortResult<u64>` takes tenant, upload ID, and chunk. | Returns cumulative bytes; missing session or lock failure errors. Concrete bridge ignores `_tenant`, so a known ID can be appended across tenants (INV-005). | `src/oci/ports.rs:72-84`; `src/oci/bridge.rs:104-119`; [INV-005](#inv-005); [REL-007](BLAST_RADIUS.md#rel-007). [API index](#r04). |
| <a id="api-019"></a>API-019 OCI finalize | `oci::BlobStore::finalize_upload(&self,&TenantId,&str,&str,Option<i64>)->PortResult<Bytes>` takes tenant, upload ID, blob key, and resolved storage cap. | Removes session before CAS write and returns bytes only after write success; failed write loses session. Concrete bridge uses caller tenant, ignores opener tenant and cap, violating INV-005/007. | `src/oci/ports.rs:86-105`; `src/oci/bridge.rs:121-155`; [INV-005](#inv-005), [INV-007](#inv-007); [REL-007](BLAST_RADIUS.md#rel-007). [API index](#r04). |
| <a id="api-020"></a>API-020 OCI cancel | `oci::BlobStore::cancel_upload(&self,&TenantId,&str)->PortResult<()>` takes tenant and upload ID. | Concrete bridge removes the local session and succeeds for an absent ID; lock failure errors. It ignores `_tenant`, violating INV-005. | `src/oci/ports.rs:107`; `src/oci/bridge.rs:157-164`; [INV-005](#inv-005); [REL-007](BLAST_RADIUS.md#rel-007). [API index](#r04). |
| <a id="api-010"></a>API-010 OCI reads/KV | `oci::BlobStore`: `async fn get_blob(&self,&TenantId,&str)->PortResult<Option<Bytes>>`; `async fn blob_exists(&self,&TenantId,&str)->PortResult<bool>`; `async fn blob_size(&self,&TenantId,&str)->PortResult<Option<u64>>`. `oci::ManifestKvStore`: `async fn get(&self,&TenantId,&str)->PortResult<Option<Bytes>>`; `async fn put(&self,&TenantId,&str,Bytes)->PortResult<()>`; `async fn list_prefix(&self,&TenantId,&str)->PortResult<Vec<String>>`. | `None` is miss; default `blob_size` fetches body. KV `put` overwrites; list returns full keys. | `src/oci/ports.rs:109-169`; [REL-012](BLAST_RADIUS.md#rel-012). [API index](#r04). |
| <a id="api-011"></a>API-011 OCI resolution | `oci::ManifestResolver`: `async fn resolve_on_miss(&self,&TenantId,&str,&str)->PortResult<Option<ResolvedManifest>>`. `oci::TenantResolver`: `async fn resolve_pat(&self,&SecretWrap)->PortResult<TenantId>`; `async fn resolve_pat_capability(&self,&SecretWrap)->PortResult<ResolvedPat>`. | Manifest resolution may fetch/persist upstream. `None` means no manifest. Default PAT capability denies write and sets storage cap to `None`; callers must treat unresolved cap as indeterminate. | `src/oci/ports.rs:177-277`; [REL-012](BLAST_RADIUS.md#rel-012). [API index](#r04). |
| <a id="api-012"></a>API-012 Shared policy | `upstream_ssrf::host_is_internal_ip(&str) -> bool`; `ssrf_safe_redirect_policy(usize) -> reqwest::redirect::Policy`; `overload::SHED_RETRY_AFTER_SECS: u64 = 1`. | Classifies literal IPs on redirects, not resolved DNS addresses. Policy is consumed by Brew/npm/pip upstream constructors; constant is only local retry guidance. | `src/{upstream_ssrf,overload}.rs`; INV-006; [REL-005](BLAST_RADIUS.md#rel-005), [REL-013](BLAST_RADIUS.md#rel-013). [API index](#r04). |
| <a id="api-013"></a>API-013 Cargo HTTP | `cargo::server::build_router(config: CargoAdapterConfig) -> Router`. Config supplies CAS, tenant resolver, auditor, and body limit. | Infallible builder installs `GET`, `PUT`, `HEAD /{*path}` and state; it does not bind. Requests can read/write/probe CAS and emit audit; auth, CAS, audit, and body failures map to HTTP responses. Routes activate only when a caller mounts/serves the returned router; `async fn run_cargo_adapter(config: CargoAdapterConfig) -> Result<(), CargoAdapterError>` binds and reports `Bind`. | `src/cargo/server.rs:108-153`; [REL-008](BLAST_RADIUS.md#rel-008). [API index](#r04). |
| <a id="api-014"></a>API-014 Brew HTTP | `brew::server::build_router(config: BrewAdapterConfig) -> Result<Router, BrewAdapterError>`. Config supplies upstream URL, CAS, resolver, auditor, and bottle limit. | Initializes upstream client; construction can return `Upstream` before a router exists. On success installs `GET /` and `GET /{*path}` for read-through, audit, and possible CAS fill; request failures become HTTP responses. Routes activate when mounted/served; `async fn run_brew_adapter(config: BrewAdapterConfig) -> Result<(), BrewAdapterError>` also reports `Bind`. | `src/brew/server.rs:67-108`; [REL-009](BLAST_RADIUS.md#rel-009). [API index](#r04). |
| <a id="api-015"></a>API-015 npm HTTP | `npm::server::build_router(state: AdapterState) -> Router`; `AdapterState { config: Arc<NpmAdapterConfig>, upstream: Arc<UpstreamClient> }`. | Infallible builder installs `GET /-/ping`, `/-/v1/search`, `/{pkg}`, `/{pkg}/-/{tarball}`; request paths may read/fill KV/CAS, fetch upstream, and audit, with failures mapped to HTTP responses. Caller supplies constructed state and mounts/serves the router; `async fn run_npm_adapter(config: NpmAdapterConfig) -> Result<(), NpmAdapterError>` constructs upstream (`Upstream` failure) and binds (`Bind` failure). | `src/npm/server.rs:29-78,325-350`; [REL-010](BLAST_RADIUS.md#rel-010). [API index](#r04). |
| <a id="api-016"></a>API-016 pip HTTP | `pip::server::build_router(state: AdapterState) -> Router`; `AdapterState { config: Arc<PipAdapterConfig>, upstream: Arc<UpstreamClient> }`. | Infallible builder installs `GET /simple/{project}/`, `/pkg/{sha256}/{filename}`, and `/healthz`; request paths may read/fill KV/CAS, fetch upstream, and audit, with failures mapped to HTTP responses. Caller supplies constructed state and mounts/serves the router; `async fn run_pip_adapter(config: PipAdapterConfig) -> Result<(), PipAdapterError>` constructs upstream (`Upstream` failure) and binds (`Bind` failure). | `src/pip/server.rs:28-54,213-237`; [REL-011](BLAST_RADIUS.md#rel-011). [API index](#r04). |
| <a id="api-017"></a>API-017 OCI HTTP | `oci::router(state: AppState) -> Router`; `AppState::new(config: Arc<OciAdapterConfig>, clock_unix_ms: fn() -> u64) -> AppState`. | Infallible builder installs `/v2`, `/v2/`, `/v2/_catalog`, `/token`, and `/v2/{*rest}` dispatch for pulls, uploads, manifests, and tags. Request failures use `OciAdapterError` HTTP mapping; push can mutate blob/KV state (INV-005/007). Caller mounts/serves the router; `async fn run_oci_adapter(config: OciAdapterConfig) -> Result<(), OciAdapterError>` performs config sanity check and bind before serving. | `src/oci.rs`; `src/oci/server.rs`; `src/oci/server/core.rs`; [REL-007](BLAST_RADIUS.md#rel-007), [REL-012](BLAST_RADIUS.md#rel-012). [API index](#r04). |

The test targets are auto-discovered integration tests under `tests/` (the manifest declares no explicit `[[test]]` blocks). Family groups include Cargo smoke/adversarial/translation-property, Brew smoke/adversarial/URL-normalization-property, npm smoke/adversarial/metadata-property, OCI pull/push/token/adversarial/digest/manifest/DoS, and pip smoke/adversarial/index-property targets. Exact package-level command: `cargo test -p corelink-adapter-host --locked --offline`; it is a proposed LOCAL_ISOLATED validation and was not executed for this record.

<a id="r05"></a>
## R05 — State, flows, and invariants

Cargo, Brew, npm, and pip auth paths use `subtle::ConstantTimeEq` at shown bearer/token comparisons; OCI token verification compares HMAC material with `ct_eq`. These source calls do not prove an end-to-end constant-time request. Cargo and Brew map tenant-resolver backend error to `VerifierOverloaded`; `overload.rs` fixes `SHED_RETRY_AFTER_SECS` to one, and their server/error sources map that variant separately from ordinary unauthorized results.

Invariant: verifier overload must not silently become a credential verdict in a family that has the dedicated variant. Falsifier: an auth mapping returns unauthorized for `VerifierOverloaded`, or removes its retry value. Source: `src/{cargo,brew,npm,pip}/auth.rs`; `src/oci/auth.rs`; `src/overload.rs`; `src/{cargo,brew}/error.rs`; `src/{cargo,brew}/server.rs`.

### OCI upload session flow

| Step | State change and failure boundary |
|---|---|
| Open | `OciBlobBridge::new` owns a per-instance `Arc<Mutex<HashMap<String, Vec<u8>>>>`. `open_upload` builds the key as `{tenant}:{unix_ms_now()}` and inserts an empty buffer; another open in the same tenant and millisecond replaces that entry. There is no random UUID, expiry, or per-tenant session cap in this bridge. |
| Append / cancel | `append_chunk` locks the map, extends the named buffer, and returns cumulative length; a missing key errors. `cancel_upload` removes the key and succeeds even when absent. Both ignore their `_tenant` argument. The mutex serializes these local operations, but separate bridge instances and process restarts do not share sessions. |
| Finalize | `finalize_upload` removes the buffer while locked, then makes `CasWriteRequest::new` using the **caller** tenant and supplied blob key and calls the write handler in `spawn_blocking`. A missing key, poisoned lock, join failure, or handler error returns `Err`; after removal there is no reinsert/rollback, so a failed write loses the session. It returns assembled bytes only after the write succeeds. `_storage_cap_bytes` is ignored. |
| HTTP PUT aftermath | `push/upload.rs::put` calls the port finalize before verifying the declared digest, final size, or emitting success audit. With this concrete bridge, CAS persistence has already occurred; digest mismatch, oversize, or audit failure does not undo that write. The handler's comments discuss a production split, but this bridge contains no compensation for the persisted blob. |

These are local source effects, not a durability or atomicity guarantee. The bridge buffer lasts only for the process/bridge lifetime; its local wall-clock call forms the session key. Source: `src/oci/bridge.rs:48-164`; `src/oci/push/upload.rs:257-384`.

### Falsifiable invariants

Invariant index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007).

| ID | Predicate → enforcement → violation → verification → current state |
|---|---|
| <a id="inv-001"></a>INV-001 | Cargo CAS identity is `(tenant_id,digest_hex)` → port contract and bridge request construction → another key/tenant reaches handler → inspect `cargo/ports.rs` and `cargo/bridge.rs`, plus `cargo_common.rs` → source-declared; tests not run. [Invariant index](#r05). |
| <a id="inv-002"></a>INV-002 | Brew CAS identity is `(tenant_id,cas_key)` with key derived from canonicalized URL → `brew/ports.rs` and bottle path → normalization/key drift changes lookup identity → inspect `brew_prop_url_normalize.rs` and bridge calls → source-declared; tests not run. [Invariant index](#r05). |
| <a id="inv-003"></a>INV-003 | npm metadata KV is scoped by tenant/key and carries insertion timestamp → npm `KvStore` and metadata bridge → cross-tenant or timestamp shape changes → inspect `npm_common.rs` and `npm_prop_metadata.rs` → source-declared; tests not run. [Invariant index](#r05). |
| <a id="inv-004"></a>INV-004 | pip index KV is scoped by tenant/key and timestamped in milliseconds → pip `KvStore` and index cache path → key/timestamp drift changes TTL/cross-tenant behavior → inspect `pip_prop_index_parse.rs` and bridge → source-declared; tests not run. [Invariant index](#r05). |
| <a id="inv-005"></a>INV-005 | upload session's opener tenant must match every append/finalize/cancel caller → intended bridge check at each operation → mismatched tenant mutates or deletes session → inspect `oci/bridge.rs:95-164` and adversarial tests → **violated in pinned source**: only `Vec<u8>` is stored and `_tenant` is ignored. [Invariant index](#r05). |
| <a id="inv-006"></a>INV-006 | read-through redirect clients use the shared literal-IP policy → family upstream constructors → bypass permits a redirect to a forbidden literal IP → inspect `upstream_ssrf.rs` and Brew/npm/pip constructors → source-declared for those constructors; DNS/proxy behavior unknown. [Invariant index](#r05). |
| <a id="inv-007"></a>INV-007 | OCI finalization must pass the verified tenant's resolved storage cap into the CAS byte reservation → `BlobStore::finalize_upload` port and bridge request construction → write request omits that cap → inspect `oci/ports.rs:90-105` and `oci/bridge.rs:121-155`, then test cap rejection in isolation → **violated in pinned source**: `_storage_cap_bytes` is ignored and `CasWriteRequest::new` is used without quota propagation. [Invariant index](#r05). |

<a id="r06"></a>
## R06 — Configuration, targets, and features

The manifest declares the library and its integration test targets. Configuration structs take explicit constructor inputs and collaborators; documented defaults are constants or caller-wiring guidance, not automatic constructor defaults. No deployed values or feature/target selection are established. The OCI verified bearer carries `storage_cap_bytes` into the push handler and port, but the concrete bridge drops it at finalization (INV-007).

| Family | Scalar configuration and documented defaults | Validation time / observable failure |
|---|---|---|
| Cargo | `bind_addr` (documented `0.0.0.0:8085`), `body_size_limit_bytes` (`DEFAULT_BODY_SIZE_LIMIT_BYTES` = 16 MiB); CAS, tenant resolver, auditor supplied. | `new` and `build_router` do no explicit scalar sanity check. Bind failure is `Bind`; an over-limit PUT is HTTP 413. `SocketAddr` parsing is the caller's responsibility. |
| Brew | `bind_addr` (`0.0.0.0:8084`), parsed `upstream_domain` (`https://ghcr.io`), `bottle_size_limit_bytes` (`DEFAULT_BOTTLE_SIZE_LIMIT_BYTES` = 2 GiB); CAS, resolver, auditor supplied. | `Url` must already be parsed before `new`; `build_router` constructs the upstream client and may return `Upstream`. Bind may fail; an over-limit bottle returns 413. No constructor range check is shown. |
| npm | `bind_addr`, `upstream_registry` (`https://registry.npmjs.org`), `metadata_ttl_seconds` (300), `tarball_size_limit_bytes` (256 MiB); CAS, metadata KV, resolver, auditor supplied. `DEFAULT_METADATA_CACHE_MAX_BYTES` = 1 MiB is a separate cache-write cap, not a `NpmAdapterConfig` field. | `parse_upstream` returns `Err(String)` on URL parse failure; constructor accepts an already parsed `Url` and does no scalar sanity check. Runner can return `Upstream` or `Bind`; tarball size rejection is request-time. |
| pip | `bind_addr`, `upstream_pypi` (`https://pypi.org`), `index_ttl_seconds` (300), `wheel_size_limit_bytes` (1 GiB), `prefer_json_index` (documented true); CAS, metadata KV, resolver, auditor supplied. | `parse_upstream` returns `Err(String)` on URL parse failure; constructor accepts `Url` and does no scalar sanity check. Runner can return `Upstream` or `Bind`; wheel/sdist size rejection is request-time. |
| OCI | `bind_addr`, `bearer_realm` (documented same adapter `/token`), `blob_size_limit_bytes` (5 GiB), `multipart_chunk_size_bytes` (16 MiB hint), `enable_catalog` (false), `token_ttl_secs` (300); signing key, blob/KV/resolver/auditor ports supplied, `manifest_resolver` optional (`None` keeps KV-miss behavior). | `new` takes explicit values. `run_oci_adapter` calls `sanity_check` before bind: enabled catalog, signing key shorter than 32 raw bytes, or zero TTL returns a `ConfigError` mapped to `OciAdapterError::Auth`; bind can return `Bind`. Blob cap is enforced in PATCH/PUT. `router` itself does not call `sanity_check`. |

Only the signing-key requirement and redacted `Debug` representation are stated here; no secret value is recorded. Falsifier: changing a default constant, constructor field, sanity check, runner, or request cap path changes the relevant row. Source: `src/{cargo,brew,npm,pip,oci}/config.rs`; family `server.rs`; `src/oci.rs`; `src/oci/push/upload.rs`.

### Outbound upstream and SSRF policy

`host_is_internal_ip` classifies literal IPv4/IPv6 addresses including private, loopback, link-local, unspecified, broadcast, documentation, CGNAT, IPv6 unique-local/link-local, and IPv4-mapped forms. `ssrf_safe_redirect_policy(max)` stops a redirect when the current URL host is such a literal and otherwise follows until the maximum. Brew, npm, and pip upstream constructors import and apply this policy; npm's `UpstreamClient::new` applies it with `NPM_UPSTREAM_MAX_REDIRECTS`. OCI mirror use remains unproven by the cited constructor set.

Invariant: an affected read-through client must not replace the shared policy with an unreviewed redirect policy. Falsifier: its constructor omits `ssrf_safe_redirect_policy` or bypasses it for redirects. This literal-IP guard does not classify DNS names after resolution. Source: `src/upstream_ssrf.rs`; `src/brew/upstream.rs`; `src/npm/upstream.rs:9,79`; `src/pip/upstream.rs`.

<a id="r07"></a>
## R07 — Failures and observability

Local error mapping distinguishes cache miss, handler failure, verifier overload, and join failure where documented above. Audit calls and source-level metrics are observability surfaces, but sink durability and emitted production signals are unknown.

### Digest, cache, audit, and mutation failures

Brew's bottle path derives a CAS key and extracts a URL-declared SHA-256 only for recognized content-addressed paths; its source places verification and audit before CAS storage for that path. npm and pip local ports use core `Digest`; their bridges render it for handler requests. OCI separates blob and manifest/KV ports and includes digest/push/pull/tag modules. Several config/audit modules document audit-before-success or audit-before-mutation, but exact semantics remain family and call-site specific.

Invariant: a source change must retain the selected family's key/digest representation and audit ordering where the specific handler requires it. Falsifier: a `cas.put` or state mutation moves before a required audit call, or a digest parser/key helper changes its declared representation. Source: `src/brew/bottle.rs`; `src/{npm,pip}/ports.rs`; `src/{npm,pip}/bridge.rs`; `src/oci/{ports,digest,push,pull,tags}.rs`; family `audit.rs` and `config.rs`.

<a id="r08"></a>
## R08 — Verification and evidence

Unknown from static inspection: which production binary instantiates every router; actual package-manager client compatibility; upstream registry response, redirect, TLS, DNS, and availability behavior; audit sink durability; storage atomicity; tenant policy; and execution status of embedded tests. The root/container manifest census is distinct from semantic use by Cargo/Brew/npm/OCI/pip code. Comments mentioning historical absorption or deployment are not treated as independent proof.

Before claiming those properties, obtain authorized runtime/integration evidence tied to a revision and environment. The source references above are falsifiable implementation statements only; they do not certify cold review or profile eligibility.

[Impact relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
