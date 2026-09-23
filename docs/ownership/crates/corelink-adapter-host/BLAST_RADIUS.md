---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-adapter-host
manifest: crates/corelink-adapter-host/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: draft
evidence_set: adapter-host-static-20260920
---

# corelink-adapter-host — blast radius

These are atomic static dependency, flow, and impact relations. They identify review scope; they do not show traffic, deployment wiring, or live upstream behaviour.

[Scope](#b01) · [Method and population](#b02) · [Direct relations](#b03) · [Propagation](#b04) · [Change matrix](#b05) · [Coverage](#b06)

<a id="b01"></a>
## B01 — Scope

This H-profile graph covers the adapter-host library's five public adapter families, their handler/PAT/KV/audit/core seams, shared upstream policy, and the inverse `corelink-container` composition. The 42 semantic implementation modules are inventoried in [R03](REFERENCE.md#r03). Root workspace membership, manifest dependencies, source calls, and source-wired routes have different evidence strength. No selected target/feature graph, served route, external registry, provider state, or production traffic was observed. The two OCI source defects in [INV-005](REFERENCE.md#inv-005) and [INV-007](REFERENCE.md#inv-007) remain unresolved.

<a id="b02"></a>
## B02 — Method and population

At the pinned source, inspect root, adapter-host and inverse package manifests; public module declarations; family bridge/auth/audit/upstream/protocol modules; and inverse Rust imports under `crates/corelink-container/src`. The 42-module H inventory is a separate implementation count, not a relation count. This static census did not run Cargo. For a selected artifact, resolve `cargo metadata --locked --offline --no-deps --format-version=1` and `cargo tree --locked --offline --workspace --invert corelink-adapter-host --target <validated-target> --edges normal,build`, then inspect features and container entrypoint selection. Both commands are proposed, unexecuted validation.

| Population at source pin | Discovered | Documented | Excluded with reason | Unknown in bounded source census |
|---|---:|---:|---:|---:|
| Adapter-host normal local package declarations | 5 | 5: REL-018/035/048/049/050 | 0 | 0 |
| Adapter-host normal third-party declarations | 23 | 11 material wire, crypto and identity declarations traced by REL-003/005/006/007/022/027–034 | 12 generic support declarations enumerated below | 0 |
| Adapter-host dev declarations | 19 | 0 production edges | 19 test-only declarations enumerated below | 0 |
| Root inverse member/alias declarations | 2 | 2: REL-041 | 0 | 0 |
| Container inverse package declarations | 1 | 1: REL-042 | 0 | 0 |
| Container inverse Rust source files naming `corelink_adapter_host` | 21 | 13: five family route files REL-043–047 and eight support files enumerated below | 8 test source files enumerated below | 0 |
| Public root module exports | 7 | 7: five family exports REL-001/014–017 and shared policy REL-005/013 | 0 | 0 |

Normal third-party material declarations: `axum`, `reqwest`, `url`, `blake3`, `sha2`, `hmac`, `hex`, `base64`, `secrecy`, `subtle`, `uuid`.

Excluded generic support declarations: `async-trait`, `tokio`, `bytes`, `tower`, `http`, `http-body-util`, `parking_lot`, `futures`, `thiserror`, `serde`, `serde_json`, `tracing`; they implement the recorded APIs/flows but add no separately owned boundary in this selection.

Dev declarations: `tokio`, `tokio-test`, `proptest`, `serde_json`, `tower`, `wiremock`, `corelink-replication`, `corelink-core`, `corelink-audit`, `async-trait`, `subtle`, `reqwest`, `uuid`, `hex`, `base64`, `http-body-util`, `axum`, `http`, `bytes`; their test evidence is in R04/R08.

The five family route files are `routes/cargo/part-00.rs`, `routes/brew.rs`, `routes/npm.rs`, `routes/pip.rs`, and `routes/oci.rs`. The eight additional documented inverse source files are `adapter_oci_kv.rs`, `d1_coread.rs`, `routes.rs`, `routes/build.rs`, `routes/oci/b126_m2_impl_02.rs`, `routes/public_mirror.rs`, `routes/public_pullthrough/part-00.rs`, and `routes/public_revoke.rs`; the OCI implementation file supports REL-047 but is counted once in this second group. Thus 5 + 8 documented files and 8 excluded test files reconcile all 21 literal matches.

The eight excluded test files are `routes/npm_parts/{fragment-tests-01,tests}.rs`, `routes/oci/{b126_m2_test_1_1_part2,b126_m2_test_1_2,b126_m2_test_1_2_part2,b126_m2_test_1_3}.rs`, `routes/pip/tests_route.rs`, and `routes/public_mirror_tests.rs`.

Source: named manifests, `src/lib.rs`, inverse `rg -l corelink_adapter_host crates/corelink-container/src --glob '*.rs'`. Selected Cargo resolution and external consumers remain UNKNOWN.

### Cross-package contract fingerprints

The `repo:...` string in each relation is its stable shared **key**, not its fingerprint. The SHA-256 below fingerprints pinned source sets. For each listed set, resolve each `path@pin` to its Git blob SHA, make one UTF-8 line `path@pin=blobsha` per file, sort the complete lines bytewise (`LC_ALL=C`), append LF after every line, and SHA-256 the resulting bytes.

This is a whole-file source-set fingerprint; semantic agreement and a peer's identical key/hash still require bilateral review.

Pins: `A=59c76cf260bcdeb5246772f70821ac8b7e8a9780` (adapter-host); `R=6ed297f5b2b64cf97447985111a2ecbbaa9536bb` (REAPI); `H=cca798ff5bc2df660ecf2570ed243eb9775ff3d0` (handler-CAS); `C=3df52eb71acdd4e00084d42f0b64191674bf42eb` (container). Expand `@A` and similar aliases to the full pin before hashing. Each blob value is independently checkable with `git rev-parse <pin>:<path>`.

| Input alias | Repository-relative path @ pin | Git blob SHA |
|---|---|---|
| AM | `crates/corelink-adapter-host/Cargo.toml@A` | `805e65fb79a1c00019e8051d7b8694769ae35351` |
| HM | `crates/corelink-handler-cas/Cargo.toml@H` | `ea5d3bf8274c29123ce0ecd7e0a5b9c5e40d72ed` |
| RM | `crates/corelink-reapi/Cargo.toml@R` | `eb9ab5b3d0f168ed83021815d7094600c76264bd` |
| CM | `crates/corelink-container/Cargo.toml@C` | `e9feace6365776c6a4141ff59efa70fe1b36e158` |
| RP | `crates/corelink-reapi/src/pat.rs@R` | `70850bf882a6387cc39dd36fa398511502711a37` |
| CaB / CaP | `crates/corelink-adapter-host/src/cargo/{bridge,ports}.rs@A` | `c3b3643c6ddf49f65474bc1ac31cfdab07c045a1` / `53eb7835fc4e1a519fbbc4b04019c0ea45a71c86` |
| BrB / BrP | `crates/corelink-adapter-host/src/brew/{bridge,ports}.rs@A` | `91fe22681a675c92f8e6ac0e80bdf6b99115fdc3` / `9ccab3e1827045ea5e9dfb04e60327e87d13ed18` |
| NpB / NpP | `crates/corelink-adapter-host/src/npm/{bridge,ports}.rs@A` | `129596922c5af43741aa280029275a62fddef7e5` / `435286e115b827999fc76274a7d58a4afeaada88` |
| PiB / PiP | `crates/corelink-adapter-host/src/pip/{bridge,ports}.rs@A` | `7c41afccc824fb7f76b8d991909ece31d00e57f9` / `e059a1c8f9f6521cb69c8295189c5b0dfab01a6b` |
| OcB / OcP | `crates/corelink-adapter-host/src/oci/{bridge,ports}.rs@A` | `52df2a9304491faba9d5037cce490886167ab080` / `2318729b3065cd724f0c5033968ced365232add5` |

| Relation | Exact input aliases after brace expansion | Source-set SHA-256 |
|---|---|---|
| REL-018 | AM, HM | `6bc42733e835a7048b94d4fdfb427dfa2564faf8f31700355f89442dee453cf7` |
| REL-035 | AM, RM | `e1b1eb9622e520054631992034f1bb98d0918fbb7a0fa1fd6dc6f7eac9cec010` |
| REL-036 | CaB, CaP, RP | `d653e4ecccf0cc8a5b87188cbdd75826daf4bcb56f6e6d1963f5f3e47241deb4` |
| REL-037 | BrB, BrP, RP | `b690335a2c6dc2b53b918798b2d019a578bbc8fc9264a290b46b96b2c7c50738` |
| REL-038 | NpB, NpP, RP | `beec04845d4fc00ba79b5e46dba33f195553287fa94fa2aefad823f153aa9570` |
| REL-039 | PiB, PiP, RP | `08f04e18eb83cc33ea587d281f5d3dcd78582dc886de5ec22f704b46693aa3ae` |
| REL-040 | OcB, OcP, RP | `11243c55312c095d8da319a63f926660b2d25b84afc7b35cc499294b5146bf46` |
| REL-042 | AM, CM | `d119c99c5adab915ce6f79ec61df290a1c3f74f08f7333f89332b3143ac3deff` |

REL-018 and REL-042 have locally reproducible hashes but peer contract confirmation remains UNKNOWN. REL-035–040 have local source-set hashes; matching REAPI keys alone do not establish matching peer hashes or approval.

<a id="b03"></a>
## B03 — Complete direct relation register

Each ID below is a distinct static source or manifest boundary. REL-002 from the earlier draft duplicated the Cargo CAS calls; those call sites are covered once by REL-008.

Relation index:

[REL-001](#rel-001) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011).

[REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021).

[REL-022](#rel-022) · [REL-023](#rel-023) · [REL-024](#rel-024) · [REL-025](#rel-025) · [REL-026](#rel-026) · [REL-027](#rel-027) · [REL-028](#rel-028) · [REL-029](#rel-029) · [REL-030](#rel-030) · [REL-031](#rel-031).

[REL-032](#rel-032) · [REL-033](#rel-033) · [REL-034](#rel-034) · [REL-035](#rel-035) · [REL-036](#rel-036) · [REL-037](#rel-037) · [REL-038](#rel-038) · [REL-039](#rel-039) · [REL-040](#rel-040) · [REL-041](#rel-041).

[REL-042](#rel-042) · [REL-043](#rel-043) · [REL-044](#rel-044) · [REL-045](#rel-045) · [REL-046](#rel-046) · [REL-047](#rel-047) · [REL-048](#rel-048) · [REL-049](#rel-049) · [REL-050](#rel-050).

<a id="rel-001"></a>
### REL-001 — Cargo family public module export

**Producer → consumer / type:** `src/lib.rs` → public `cargo` module; source module → Rust import surface.

**Activation / contract:** a caller imports `corelink_adapter_host::cargo`; module visibility exposes that family's own ports, bridge, config, and protocol modules.

**Failure propagation:** changing the module path or visibility breaks Cargo-family imports; it says nothing about Cargo registry route mounting.

**Validation / coordination:** compare `lib.rs` and Cargo-family import sites; coordinate Cargo-family API consumers. **Evidence:** `src/lib.rs`, `src/cargo/`.
[Index](#b03)

<a id="rel-003"></a>
### REL-003 — Cargo credential to tenant resolver

**Producer → consumer:** Cargo bearer header → `cargo::auth::resolve_tenant` → `TenantResolver` / PAT SPI.

**Activation / contract:** Cargo registry request resolves tenant or typed auth failure; verifier overload has a separate source error mapping.

**Failure propagation:** parsing, resolver, or HTTP mapping drift changes Cargo's tenant/error/retry classification.

**Validation / coordination:** trace Cargo auth/error/server with resolver owner. **Evidence:** `src/cargo/{auth,error,server}.rs`, `src/overload.rs`.

**Boundary:** a source `ct_eq` call is not an end-to-end timing or verifier-policy proof. [Index](#b03)

<a id="rel-004"></a>
### REL-004 — Brew bottle fill to audit emitter

**Producer → consumer:** Brew bottle fill path → `brew::audit` → audit SPI.

**Activation / contract:** bottle cache/upstream operation emits the local audit shape on the call paths that invoke the helper.

**Failure propagation:** emitter/result mapping changes Brew fill behavior.

**Validation / coordination:** trace exact operation call site and sink error with audit owner. **Evidence:** `src/brew/{bottle,audit}.rs`.

**Boundary:** sink persistence/delivery and other families' order are not implied. [Index](#b03)

<a id="rel-005"></a>
### REL-005 — Brew read-through fetcher to shared literal-IP guard

**Producer → consumer:** Brew upstream client builder → `ssrf_safe_redirect_policy` → redirect host check.

**Activation / contract:** Brew read-through requests follow bounded redirects under the literal-IP rule.

**Failure propagation:** omission/weakening changes Brew redirect posture.

**Validation / coordination:** compare Brew builder, redirect bound, and guard with security owner. **Evidence:** `src/upstream_ssrf.rs`, `src/brew/upstream.rs`.

**Boundary:** DNS rebinding, proxy behavior, and live redirect behavior are unknown. [Index](#b03)

<a id="rel-006"></a>
### REL-006 — Cargo path translation to tenant cache identity

**Producer → consumer:** Cargo registry path input → Cargo translation helpers → URL-map/CAS key and response contract.

**Activation / contract:** Cargo registry translation resolves source path and crate metadata used by the Cargo adapter.

**Failure propagation:** grammar or key drift may change cache identity or returned registry representation.

**Validation / coordination:** inspect Cargo translator and tests with Cargo-family owner. **Evidence:** `src/cargo/translate.rs`; external compatibility UNKNOWN.

**Boundary:** source parsing is not proof of client or registry interoperability. [Index](#b03)

<a id="rel-007"></a>
### REL-007 — OCI upload session to opening tenant

**Dependency / flow / impact:** OCI `open_upload(tenant)` → upload UUID → `append_chunk`/`finalize_upload`/`cancel_upload`; session identity → tenant authorization → accumulated bytes or deletion.

**Predicate:** OCI upload storage shape or any of the four session operations changes.

**Impact:** a UUID accepted for a different tenant can append, finalize, or cancel another tenant's upload; final CAS tenant mapping does not bind the session.

**Evidence:** `src/oci/bridge.rs:95-164`: the map is `HashMap<String, Vec<u8>>`; append/finalize/cancel accept `_tenant` without comparing it.

**Stop:** baseline defect. Require session→tenant storage/binding and mismatched-tenant rejection at append, finalize, and cancel before declaring tenant isolation for upload sessions. [Index](#b03)

<a id="rel-008"></a>
### REL-008 — Cargo port to handler-CAS SPI
**Producer → consumer:** `cargo::CasStore` → `CargoCasBridge` → `CasReadHandler`/`CasWriteHandler` SPI. **Surface/activation:** get, put, exists calls from the Cargo adapter; synchronous handler calls run through `spawn_blocking`. **Contract/effect:** tenant/lowercase digest key and bytes form typed SPI requests; handler `NotFound` becomes `None`, other handler/join failures become `CasError`. **Failure/validation:** mapping drift changes result; compare ports, bridge, handler request. **Coordination:** handler-CAS owner. **Evidence:** `src/cargo/{ports,bridge}.rs`. [Index](#b03)

<a id="rel-009"></a>
### REL-009 — Brew URL-keyed CAS bridge
**Producer → consumer:** `brew::BottleService`/`CasStore` → bridge → CAS SPI. **Activation:** bottle fetch. **Contract/effect:** URL-derived key selects tenant bytes; miss may invoke upstream. **Failure:** key or miss mapping changes cache result. **Validation:** compare bottle, port, bridge and handler request. **Coordination:** handler-CAS owner. **Evidence:** `src/brew/{bottle,ports,bridge}.rs`. [Index](#b03)

<a id="rel-010"></a>
### REL-010 — npm CAS bridge
**Producer → consumer:** `npm::CasStore` → bridge → CAS SPI. **Activation:** tarball reads/writes. **Contract/effect:** tenant and `Digest` cross bridge; family errors return. **Failure:** conversion changes cache identity/error. **Validation:** compare npm port/bridge/tarball and handler contract. **Coordination:** handler-CAS owner. **Evidence:** `src/npm/{ports,bridge,tarball}.rs`. [Index](#b03)

<a id="rel-011"></a>
### REL-011 — pip CAS bridge
**Producer → consumer:** `pip::CasStore` → bridge → CAS SPI. **Activation:** wheel fetch/store. **Contract/effect:** tenant and `Digest` key; port says put follows integrity check. **Failure:** mapping changes wheel cache result. **Validation:** compare pip port/bridge/wheel and handler request. **Coordination:** handler-CAS owner. **Evidence:** `src/pip/{ports,bridge,wheel}.rs`. [Index](#b03)

<a id="rel-012"></a>
### REL-012 — OCI blob CAS bridge
**Producer → consumer:** `oci::BlobStore` → bridge → CAS SPI. **Activation:** finalized upload or pull. **Contract/effect:** tenant and OCI key reach the write; optional cap reaches the port but the bridge drops it; pull returns bytes/size or miss. **Failure:** digest/cap mapping changes write/read; session defect is REL-007. **Validation:** trace port calls and CAS request. **Coordination:** handler-CAS owner. **Evidence:** `src/oci/{ports,bridge,push,pull}.rs`. [Index](#b03)

<a id="rel-013"></a>
### REL-013 — Shared overload classification
**Producer → consumer:** `SHED_RETRY_AFTER_SECS`/resolver backend error → Cargo/Brew auth/server. **Activation:** resolver failure. **Contract/effect:** dedicated overload result carries retry value. **Failure:** mapping to unauthorized changes retry classification. **Validation:** trace auth/error/server. **Coordination:** family owners. **Evidence:** `src/{overload,cargo,brew}/{auth,error,server}.rs`. [Index](#b03)

<a id="rel-014"></a>
### REL-014 — Brew family public module export
**Producer → consumer / type:** `src/lib.rs` → public `brew` module; source export. **Activation / contract:** downstream imports `corelink_adapter_host::brew` and its protocol-specific API. **Failure propagation:** visibility/name changes break Brew imports, not proof of a running Homebrew endpoint. **Validation / coordination:** inspect Brew imports and router call sites. **Evidence:** `src/lib.rs`, `src/brew/`. [Index](#b03)

<a id="rel-015"></a>
### REL-015 — npm family public module export
**Producer → consumer / type:** `src/lib.rs` → public `npm` module; source export. **Activation / contract:** downstream imports `corelink_adapter_host::npm` and its npm API. **Failure propagation:** export or type changes break npm imports, not proof of registry traffic. **Validation / coordination:** inspect npm import/call sites. **Evidence:** `src/lib.rs`, `src/npm/`. [Index](#b03)

<a id="rel-016"></a>
### REL-016 — OCI family public module export
**Producer → consumer / type:** `src/lib.rs` → public `oci` module; source export. **Activation / contract:** downstream imports `corelink_adapter_host::oci` and its upload/pull API. **Failure propagation:** export/signature changes affect OCI imports, not proof of registry route invocation. **Validation / coordination:** inspect OCI dispatch and import sites; retain the tenant defect tracked by REL-007. **Evidence:** `src/lib.rs`, `src/oci/`. [Index](#b03)

<a id="rel-017"></a>
### REL-017 — pip family public module export
**Producer → consumer / type:** `src/lib.rs` → public `pip` module; source export. **Activation / contract:** downstream imports `corelink_adapter_host::pip` and PyPI-specific API. **Failure propagation:** export/signature changes break pip imports, not proof of package-index traffic. **Validation / coordination:** inspect pip imports/call sites. **Evidence:** `src/lib.rs`, `src/pip/`. [Index](#b03)

<a id="rel-018"></a>
### REL-018 — Adapter host direct CAS handler dependency
**Shared key:** `repo:1232040291:relation:corelink-handler-cas-to-corelink-adapter-host-manifest`. **Source-set SHA-256:** `6bc42733e835a7048b94d4fdfb427dfa2564faf8f31700355f89442dee453cf7` (AM, HM; peer match UNKNOWN; recipe in B02).

**Producer → consumer / type:** `corelink-handler-cas` manifest package → `corelink-adapter-host` manifest package dependency. **Activation / contract:** workspace Cargo resolution selects the declared path dependency; this is distinct from individual bridge calls REL-008–012.

**Failure propagation:** package identity or public type changes can prevent resolution/compilation. **Validation / coordination:** rerun inverse manifest census and inspect selected targets before interpreting use. **Evidence:** both package manifests; resolution/runtime UNKNOWN. Peer `corelink-handler-cas` B04 should retain this key and backlink. [Index](#b03)

<a id="rel-019"></a>
### REL-019 — Brew bearer credential to tenant resolver
**Producer → consumer:** Brew bearer header → `brew::auth` → tenant resolver/PAT SPI. **Activation / contract:** bottle request resolves tenant or typed auth error; backend overload follows Brew-specific mapping. **Failure:** parsing/resolver/status drift changes tenant or retry behavior. **Validation / coordination:** inspect Brew auth/error/server with resolver owner. **Evidence:** `src/brew/{auth,error,server}.rs`; runtime unknown. [Index](#b03)

<a id="rel-020"></a>
### REL-020 — npm bearer credential to tenant resolver
**Producer → consumer:** npm bearer header → `npm::auth` → tenant resolver. **Activation / contract:** metadata/tarball request resolves typed tenant before cache access. **Failure:** auth mapping drift changes tenant selection or typed failure. **Validation / coordination:** inspect npm auth/dispatch with PAT owner. **Evidence:** `src/npm/{auth,server}.rs`; verifier result unobserved. [Index](#b03)

<a id="rel-021"></a>
### REL-021 — pip bearer credential to tenant resolver
**Producer → consumer:** pip bearer header → `pip::auth` → tenant resolver. **Activation / contract:** index/wheel request resolves typed tenant before cache access. **Failure:** auth mapping drift changes tenant selection or typed failure. **Validation / coordination:** inspect pip auth/dispatch with PAT owner. **Evidence:** `src/pip/{auth,server}.rs`; verifier result unobserved. [Index](#b03)

<a id="rel-022"></a>
### REL-022 — OCI token to tenant and capability
**Producer → consumer:** OCI token request → `oci::auth` → `TenantResolver` capability result. **Activation / contract:** token mint/verification and blob authorization carry tenant and write capability separately. **Failure:** HMAC/capability/scope drift changes token authorization. **Validation / coordination:** trace token claims and write gates with OCI/PAT owner. **Evidence:** `src/oci/{auth,ports,server}.rs`; runtime unobserved. [Index](#b03)

<a id="rel-023"></a>
### REL-023 — Cargo config mutation to audit emitter
**Producer → consumer:** Cargo config operation → Cargo audit helper → audit SPI. **Activation / contract:** only call sites explicitly invoking the helper emit a record. **Failure:** order or error mapping drift changes that operation's result. **Validation / coordination:** trace exact branch with audit owner. **Evidence:** `src/cargo/{config,audit}.rs`; delivery unknown. [Index](#b03)

<a id="rel-024"></a>
### REL-024 — npm metadata/config path to audit emitter
**Producer → consumer:** npm operation → npm audit helper → audit SPI. **Activation / contract:** only explicit call sites, not all metadata/tarball operations. **Failure:** event order/error propagation changes affected operation. **Validation / coordination:** inspect call sites in changed path with audit owner. **Evidence:** `src/npm/{audit,config,metadata,tarball}.rs`; durability unknown. [Index](#b03)

<a id="rel-025"></a>
### REL-025 — pip index/config path to audit emitter
**Producer → consumer:** pip operation → pip audit helper → audit SPI. **Activation / contract:** only explicit emitter call sites are covered. **Failure:** per-operation order or sink error mapping changes. **Validation / coordination:** inspect affected index/config branch with audit owner. **Evidence:** `src/pip/{audit,config,index}.rs`; delivery unknown. [Index](#b03)

<a id="rel-026"></a>
### REL-026 — OCI upload/tag mutation to audit emitter
**Producer → consumer:** OCI mutation path → OCI audit helper → audit SPI. **Activation / contract:** only upload/tag branches that call the helper emit records. **Failure:** ordering/error mapping changes the selected OCI result. **Validation / coordination:** inspect branch and SPI result with audit owner. **Evidence:** `src/oci/{audit,push,tags}.rs`; external delivery unknown. [Index](#b03)

<a id="rel-027"></a>
### REL-027 — npm read-through client to shared redirect guard
**Producer → consumer:** npm `UpstreamClient::new` → shared literal-IP redirect policy. **Activation / contract:** metadata/tarball upstream client uses its bounded redirect setting. **Failure:** constructor/limit drift changes source redirect policy. **Validation / coordination:** inspect `npm/upstream.rs` and guard with security owner. **Evidence:** `src/npm/upstream.rs`, `src/upstream_ssrf.rs`; DNS resolution unknown. [Index](#b03)

<a id="rel-028"></a>
### REL-028 — pip read-through client to shared redirect guard
**Producer → consumer:** pip upstream builder → shared literal-IP redirect policy. **Activation / contract:** index/wheel upstream requests follow the configured bounded policy. **Failure:** constructor/limit drift changes redirect handling. **Validation / coordination:** inspect `pip/upstream.rs` and guard with security owner. **Evidence:** `src/pip/upstream.rs`, `src/upstream_ssrf.rs`; DNS resolution unknown. [Index](#b03)

<a id="rel-029"></a>
### REL-029 — Brew bottle grammar to CAS key
**Producer → consumer:** Brew bottle URL/tag parser → canonical URL/key helper → tenant CAS. **Activation / contract:** bottle fetch; mutable tag paths may be served without caching. **Failure:** normalization/key drift changes cache identity or response. **Validation / coordination:** compare `bottle.rs`, normalization tests and CAS call with Brew owner. **Evidence:** `src/brew/{bottle,ports}.rs`. [Index](#b03)

<a id="rel-030"></a>
### REL-030 — npm metadata path to tenant KV entry
**Producer → consumer:** npm metadata path → parser/cache key → tenant-scoped KV. **Activation / contract:** metadata cache lookup/fill carries bytes and insertion timestamp. **Failure:** parser/key/TTL drift changes cache behavior. **Validation / coordination:** trace metadata fetch separately from tarball path. **Evidence:** `src/npm/{metadata,ports}.rs`; registry interoperability UNKNOWN. [Index](#b03)

<a id="rel-031"></a>
### REL-031 — npm tarball digest to CAS entry
**Producer → consumer:** npm tarball integrity/path → `Digest` → tenant-scoped CAS. **Activation / contract:** typed digest key and integrity-gated put. **Failure:** digest conversion or integrity-order drift changes identity/acceptance. **Validation / coordination:** inspect tarball, port, bridge and handler request with npm/CAS owners. **Evidence:** `src/npm/{tarball,ports,bridge}.rs`. [Index](#b03)

<a id="rel-032"></a>
### REL-032 — pip index normalization to tenant KV entry
**Producer → consumer:** PEP index request → normalizer/rendering → timestamped tenant KV. **Activation / contract:** index path uses the key/value timestamp for local TTL decisions. **Failure:** grammar/key/TTL drift changes rendered index or cache behavior. **Validation / coordination:** compare PEP parsing, KV call and index tests with pip owner. **Evidence:** `src/pip/{index,pep503_html,ports}.rs`. [Index](#b03)

<a id="rel-033"></a>
### REL-033 — pip wheel digest to CAS entry
**Producer → consumer:** wheel response/integrity → `Digest` → tenant-scoped CAS. **Activation / contract:** wheel fetch/store uses a distinct blob path from the index KV path. **Failure:** digest/order drift changes cache identity or accepted wheel bytes. **Validation / coordination:** inspect wheel, bridge and index tests with pip/CAS owners. **Evidence:** `src/pip/{wheel,ports,bridge}.rs`. [Index](#b03)

<a id="rel-034"></a>
### REL-034 — OCI digest and route dispatch to blob/manifest stores
**Producer → consumer:** OCI request path/digest → dispatch → blob CAS or manifest/tag KV port. **Activation / contract:** push, pull, manifest and tag operations choose distinct typed methods. **Failure:** parser/dispatch/key drift changes tenant store selection or wire error. **Validation / coordination:** trace each changed operation to the port/bridge; retain upload tenant defect REL-007. **Evidence:** `src/oci/{digest,server/dispatch,pull,push,tags}.rs`; interoperability UNKNOWN. [Index](#b03)

<a id="rel-035"></a>
### REL-035 — REAPI manifest dependency
**Shared key:** `repo:1232040291:relation:corelink-adapter-host-to-corelink-reapi-manifest`; **Source-set SHA-256:** `e1b1eb9622e520054631992034f1bb98d0918fbb7a0fa1fd6dc6f7eac9cec010` (AM, RM; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-017](../corelink-reapi/BLAST_RADIUS.md#rel-017). **Producer → consumer / type:** `corelink-reapi` package → adapter-host manifest declaration. **Activation / contract:** Cargo resolution of the declared dependency; five PAT source calls have separate REL-036–040. **Failure / limit:** package identity/path drift can break resolution; no selected target or live call follows from this declaration. **Validation / coordination:** inspect both manifests and selected inverse Cargo graph with REAPI/adapter owners. **Evidence:** `crates/corelink-adapter-host/Cargo.toml:17` and REAPI manifest. [Index](#b03)

<a id="rel-036"></a>
### REL-036 — Cargo bridge REAPI PAT call
**Shared key:** `repo:1232040291:boundary:reapi-adapter-host-pat-cargo`; **Source-set SHA-256:** `d653e4ecccf0cc8a5b87188cbdd75826daf4bcb56f6e6d1963f5f3e47241deb4` (CaB, CaP, RP; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-020](../corelink-reapi/BLAST_RADIUS.md#rel-020). **Producer → consumer / type:** REAPI `PatValidator::authenticate` → Cargo `TenantResolver::resolve`; runtime-call. **Activation / contract:** bridge uses `spawn_blocking` and request label `cargo-adapter-host`, returning a tenant UUID string. **Failure / limit:** invalid PAT → `InvalidPat`, join/other auth fault → `Backend`; actual request selection unverified. **Validation / coordination:** Cargo bridge resolver test with REAPI PAT owner. **Evidence:** `src/cargo/bridge.rs:154-177`. [Index](#b03)

<a id="rel-037"></a>
### REL-037 — Brew bridge REAPI PAT call
**Shared key:** `repo:1232040291:boundary:reapi-adapter-host-pat-brew`; **Source-set SHA-256:** `b690335a2c6dc2b53b918798b2d019a578bbc8fc9264a290b46b96b2c7c50738` (BrB, BrP, RP; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-021](../corelink-reapi/BLAST_RADIUS.md#rel-021). **Producer → consumer / type:** REAPI `PatValidator::authenticate` → Brew `TenantResolver::resolve`; runtime-call. **Activation / contract:** bridge uses `spawn_blocking` and label `brew-adapter-host`, returning a tenant UUID string. **Failure / limit:** invalid PAT → `InvalidPat`, join/other auth fault → `Backend`; runtime unverified. **Validation / coordination:** Brew bridge resolver test with REAPI PAT owner. **Evidence:** `src/brew/bridge.rs:135-158`. [Index](#b03)

<a id="rel-038"></a>
### REL-038 — npm bridge REAPI PAT call
**Shared key:** `repo:1232040291:boundary:reapi-adapter-host-pat-npm`; **Source-set SHA-256:** `beec04845d4fc00ba79b5e46dba33f195553287fa94fa2aefad823f153aa9570` (NpB, NpP, RP; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-022](../corelink-reapi/BLAST_RADIUS.md#rel-022). **Producer → consumer / type:** REAPI `PatValidator::authenticate` → npm `TenantResolver::resolve`; runtime-call. **Activation / contract:** bridge uses `spawn_blocking` and label `npm-adapter-host`, returning `TenantId`. **Failure / limit:** invalid PAT, join and other auth faults map to `NpmAdapterError::Auth`; runtime unverified. **Validation / coordination:** npm bridge test with REAPI PAT owner. **Evidence:** `src/npm/bridge.rs:258-279`. [Index](#b03)

<a id="rel-039"></a>
### REL-039 — pip bridge REAPI PAT call
**Shared key:** `repo:1232040291:boundary:reapi-adapter-host-pat-pip`; **Source-set SHA-256:** `08f04e18eb83cc33ea587d281f5d3dcd78582dc886de5ec22f704b46693aa3ae` (PiB, PiP, RP; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-023](../corelink-reapi/BLAST_RADIUS.md#rel-023). **Producer → consumer / type:** REAPI `PatValidator::authenticate` → pip `TenantResolver::resolve`; runtime-call. **Activation / contract:** bridge uses `spawn_blocking` and label `pip-adapter-host`, returning `TenantId`. **Failure / limit:** invalid PAT, join and other auth faults map to `PipAdapterError::Auth`; runtime unverified. **Validation / coordination:** pip bridge test with REAPI PAT owner. **Evidence:** `src/pip/bridge.rs:247-268`. [Index](#b03)

<a id="rel-040"></a>
### REL-040 — OCI bridge REAPI PAT call
**Shared key:** `repo:1232040291:boundary:reapi-adapter-host-pat-oci`; **Source-set SHA-256:** `11243c55312c095d8da319a63f926660b2d25b84afc7b35cc499294b5146bf46` (OcB, OcP, RP; peer hash UNKNOWN; recipe in B02).

peer [REAPI REL-024](../corelink-reapi/BLAST_RADIUS.md#rel-024). **Producer → consumer / type:** REAPI `PatValidator::authenticate` → OCI `resolve_pat`; runtime-call. **Activation / contract:** bridge unwraps `SecretWrap`, uses `spawn_blocking` and label `oci-adapter-host`, returning `TenantId`. **Failure / limit:** invalid PAT and other auth/join faults return distinct `PortResult` strings; runtime unverified. **Validation / coordination:** OCI bridge test with REAPI PAT owner. **Evidence:** `src/oci/bridge.rs:292-312`. [Index](#b03)

<a id="rel-041"></a>
### REL-041 — Root workspace member and alias
**Type/direction:** Cargo membership, root workspace → adapter-host package. **Activation/contract:** root `Cargo.toml:366,681` lists the member and path alias; this permits workspace selection, not shipped inclusion. **Failure/validation:** path/name drift breaks selection; inspect both declarations and target-specific metadata. **Owner:** workspace. **Evidence:** root `Cargo.toml`.

[Index](#b03)

<a id="rel-042"></a>
### REL-042 — Container direct package consumer
**Shared key:** `repo:1232040291:relation:corelink-adapter-host-to-corelink-container-manifest`; **Source-set SHA-256:** `d119c99c5adab915ce6f79ec61df290a1c3f74f08f7333f89332b3143ac3deff` (AM, CM; container peer UNKNOWN; recipe in B02).

container peer pending. **Type/direction:** adapter-host package → `corelink-container` direct Cargo dependency. **Activation/contract:** container manifest selects the workspace alias; five source compositions are REL-043–047. **Failure/validation:** package/API drift can break a selected container build; inspect manifest plus source imports and run target-specific inverse Cargo resolution when authorized. **Evidence:** `crates/corelink-container/Cargo.toml:196-200`; selected build UNKNOWN.

[Index](#b03)

<a id="rel-043"></a>
### REL-043 — Container Cargo route composition
**Type/direction:** adapter-host Cargo config, ports and `server::build_router` → container Cargo router. **Activation/contract:** container `routes/cargo/part-00.rs::router` supplies CAS/resolver/auditor, wraps gate and co-read hint, then `nest_service("/cargo", adapter)`. **Effect/failure:** adapter API or gate order drift changes the source-wired Cargo path; no served request is proved. **Validation/coordination:** inspect constructor, layers and route tests with container/Cargo owners. **Evidence:** `crates/corelink-container/src/routes/cargo/part-00.rs:223-270`.

[Index](#b03)

<a id="rel-044"></a>
### REL-044 — Container Brew route composition
**Type/direction:** adapter-host Brew `build_router` → container Brew router. **Activation/contract:** `routes/brew.rs::router_with_cap_resolver` builds the adapter and nests it at `/brew`; builder failure returns an empty router. **Effect/failure:** upstream/config or gate drift can remove the source mount. **Validation/coordination:** test success and builder-error branches with container/Brew owners. **Evidence:** `crates/corelink-container/src/routes/brew.rs:169-232`; runtime UNKNOWN.

[Index](#b03)

<a id="rel-045"></a>
### REL-045 — Container npm route composition
**Type/direction:** adapter-host npm state, upstream and `build_router` → container npm router. **Activation/contract:** `routes/npm.rs` constructs the state, nests at `/npm`, and applies the outer tenant-stripping gate before parameterized routing. **Effect/failure:** mount/layer drift can yield 404 or alter tenant selection. **Validation/coordination:** inspect route and gate tests with container/npm owners. **Evidence:** `crates/corelink-container/src/routes/npm.rs:323-411`; runtime UNKNOWN.

[Index](#b03)

<a id="rel-046"></a>
### REL-046 — Container pip route composition
**Type/direction:** adapter-host pip state, upstream and `build_router` → container pip router. **Activation/contract:** `routes/pip.rs` builds state, nests at `/pip`, and applies an outer tenant-stripping gate. **Effect/failure:** layer/mount drift changes index/wheel route selection. **Validation/coordination:** inspect route and gate tests with container/pip owners. **Evidence:** `crates/corelink-container/src/routes/pip.rs:344-428`; runtime UNKNOWN.

[Index](#b03)

<a id="rel-047"></a>
### REL-047 — Container OCI route composition
**Type/direction:** adapter-host OCI `router(AppState)` → container OCI route. **Activation/contract:** `routes/oci/b126_m2_impl_02.rs::router` merges the adapter router so `/v2/*` and `/token` keep their paths; related OCI port adapters are in container source. **Effect/failure:** merge/port drift changes source-wired push/pull routes and session policy exposure. **Validation/coordination:** inspect merge, config and adversarial route tests with container/OCI owners. **Evidence:** `crates/corelink-container/src/routes/oci.rs:75-80`, `routes/oci/b126_m2_impl_02.rs:25-127`; runtime UNKNOWN.

[Index](#b03)

<a id="rel-048"></a>
### REL-048 — Worker KV backend dependency
**Type/direction:** `corelink_worker::cache::kv::KvBackend` → adapter-host npm, pip and OCI bridge implementations. **Activation/contract:** the manifest declares worker; generic KV bridges call backend methods for tenant-scoped metadata/manifest values. **Effect/failure:** trait or timestamp/key drift breaks bridge use or cache behavior. **Validation/coordination:** inspect worker trait and three bridge implementations with worker owner. **Evidence:** adapter-host `Cargo.toml:16`, `src/{npm,pip,oci}/bridge.rs`; runtime UNKNOWN.

[Index](#b03)

<a id="rel-049"></a>
### REL-049 — Audit SPI dependency
**Type/direction:** `corelink-audit::AuditEmitter` → adapter-host audit bridges. **Activation/contract:** declared manifest dependency and family audit call sites construct events for the injected emitter. **Effect/failure:** schema or sink error drift changes the selected operation result; delivery is unobserved. **Validation/coordination:** compare family audit modules and provider contract with audit owner. **Evidence:** adapter-host `Cargo.toml:18`, `src/{cargo,brew,npm,pip,oci}/audit.rs`.

[Index](#b03)

<a id="rel-050"></a>
### REL-050 — Core identity and digest dependency
**Type/direction:** `corelink-core` tenant/digest types → adapter-host npm, pip, OCI ports and bridges. **Activation/contract:** manifest declaration and source imports supply typed tenant and digest identities. **Effect/failure:** representation drift changes cache keys or conversion; no stored object is observed. **Validation/coordination:** compare port signatures and bridge conversions with core owner. **Evidence:** adapter-host `Cargo.toml:19`, `src/{npm,pip,oci}/{ports,bridge}.rs`.

[Index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

| Destination | Witness path by REL | Activation and causal effect | Containment / validation |
|---|---|---|---|
| Container Cargo route → handler CAS | REL-042 → REL-043 → REL-008/018 | Selected container Cargo router supplies port; digest/tenant requests cross bridge. | Inspect route gate, bridge request and handler contract; served requests UNKNOWN. |
| Container Brew route → upstream/CAS | REL-042 → REL-044 → REL-005/009/029 | Successful builder nests Brew; bottle miss may fetch upstream then fill CAS. | Builder-error branch returns empty router; test success/error and URL key. |
| Container npm route → KV/CAS | REL-042 → REL-045 → REL-010/030/031/048 | Outer tenant gate precedes metadata/tarball lookup; wrong order can 404 or select wrong key. | Inspect layer order, metadata key and tarball digest tests. |
| Container pip route → KV/CAS | REL-042 → REL-046 → REL-011/032/033/048 | Outer gate precedes index/wheel paths; normalization affects scoped storage. | Inspect gate, index normalization and wheel integrity. |
| Container OCI route → upload/CAS | REL-042 → REL-047 → REL-007/012/022/034 | Merged OCI routes can reach upload bridge; current session tenant and cap defects propagate. | Require mismatched-tenant/cap rejection in isolated tests; runtime UNKNOWN. |
| PAT verifier → five adapter families | REL-035 → REL-036–040 → REL-003/019–022 | Selected family auth path translates PAT outcome to tenant/capability or failure. | Five REAPI peer keys identify source calls; verify selected caller and errors. |
| Family operation → audit sink | REL-049 → REL-004/023–026 | Only explicit family call sites emit via injected audit SPI. | Inspect per-operation order and sink result; persistence UNKNOWN. |

Coverage: every internal family router, the five direct workspace SPI dependencies, and the source-proven container composition have a witness above. Cargo registry, upstream, and OCI client behavior have distinct failure mechanisms; none is a proven live route. No nonreachability claim is made for unselected targets or external consumers.

<a id="b05"></a>
## B05 — Change, impact, and validation

| Change | API / INV / REL affected | Consumer or state effect | Required validation and coordination |
|---|---|---|---|
| Public router/config/port signature | [R04](REFERENCE.md#r04), REL-001/014–017, REL-043–047 | Five container composition sites may fail to build or route. | Compare each changed family import, constructor and gate; container owner and selected target build. |
| CAS bridge/key/digest mapping | API-001/003/005/007/009/010, INV-001–005/007, REL-008–012/029–034 | Handler keys, cache hits and OCI upload bytes may change. | Port/bridge tests and adverse tenant/digest/cap predicates; handler-CAS and core owners. |
| PAT/auth/capability or REAPI boundary | API-002/004/006/008/011, REL-003/019–022/035–040 | Wrong tenant, write authority or error classification may reach family routes. | Five bridge call tests and peer-key review with REAPI/container owners. |
| Audit event/order or sink | REL-004/023–026/049 | Emission failure/order changes selected mutation result. | Trace exact call sites and failure branch with audit owner. |
| Upstream redirect or package grammar | API-012, INV-006, REL-005/006/027–034 | Redirect policy, key normalization or wire response may drift. | Literal-IP/redirect and family parser tests; DNS/interoperability require separate evidence. |
| OCI session/cap fix | API-009/018–020, INV-005/007, REL-007/012/047 | Session ownership or CAS quota reservation changes. | Isolated cross-tenant, duplicate-open, failed-finalize and cap rejection tests; container/OCI/CAS owners. |

<a id="b06"></a>
## B06 — Coverage and unknowns

The bounded source equations are 5=5+0+0 local normal declarations; 23=11+12+0 third-party normal declarations; 19=0+19+0 dev declarations; 2=2+0+0 root inverse declarations; 1=1+0+0 container inverse declaration; 21=13+8+0 inverse source files; and 7=7+0+0 public exports.

B02 enumerates every exclusion and the eight supporting inverse source files. REL-002 is retired as a duplicate of REL-008; no Cargo call site was discarded. Relation cards retain source-level effects and limits; the matching REAPI PAT fingerprints alone do not approve the whole graph. The container package fingerprint is proposed here and awaits a reciprocal peer record.

Unknown: target/feature-selected inverse resolution, shipped artifact, route serving, upstream DNS and wire compatibility, persistent CAS/KV/audit effects, external consumers, and execution results of proposed tests. A zero in the bounded structural-unknown column does not prove discovery beyond the selected source search. Reconcile any new inverse target or source file by updating B02, B03 and these equations.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01) · [Relation index](#b03)
