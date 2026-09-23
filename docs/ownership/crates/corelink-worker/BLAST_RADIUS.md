---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-worker
manifest: crates/corelink-worker/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: H
state: draft
evidence_set: w011-worker-static-20260920
---

# corelink-worker — blast radius

These are static manifest, export, import, and call-site relationships at the pinned revision. They do not prove feature resolution, a running Worker, provider access, deployed routing, or production behavior. Implementation ownership stays with the crate that defines each contract; re-exports and composition are separate roles.

[B01 Scope](#b01) · [B02 Method](#b02) · [B03 Direct relationships](#b03) · [B04 Transitive propagation](#b04) · [B05 Change validation](#b05) · [B06 Coverage](#b06).

<a id="b01"></a>
## B01 — Scope

Package `corelink-worker` (`crates/corelink-worker/Cargo.toml`) owns its storage/cache implementations, tenant and region adapters, and feature-gated auth/middleware/REAPI source. `corelink-hash` owns digest and verified-body contracts; `corelink-tenant-path` owns tenant prefixes; the `corelink-ac` package owns AC contracts. `corelink-adapter-host`, `corelink-auth`, `corelink-cas`, `corelink-cf-bindings`, `corelink-reapi`, and `corelink-replication` are known composition, façade, or adapter consumers; source links below describe their precise roles. Runtime operator and deployment authority are not determined from Cargo edges.

<a id="b02"></a>
## B02 — Method and populations

**Manifest inventory:** read normal/build/dev and target-specific dependencies, optional flags, feature forwarding, package targets, and the independent fuzz manifest; manifest edges are declarations only.

**Resolved graph:** no target/feature-specific graph was resolved for a shipped artifact. **Semantic graph:** inspected crate root, storage key/R2/KV modules, gated REAPI/middleware modules, named consumer roots/imports, and worker fuzz targets.

**Reverse census:** the observed direct manifests name six consumers—`corelink-adapter-host`, `corelink-auth`, `corelink-cas`, `corelink-cf-bindings`, `corelink-reapi`, `corelink-replication`; the independent fuzz manifest is a separate edge. This snapshot is not exhaustive of external callers or runtime selection. No build, test, or Worker invocation was run for this document.

| Population | Inspected population and method | Found/documented | Excluded or unknown |
|---|---|---|---|
| Package source | Manifest targets/features, root, storage/cache and gated source trees | Source APIs and gates in REFERENCE R03–R06; REL-001–003 and API/invariant crosswalk below | Cargo resolution and host/wasm builds not inspected |
| Direct Cargo consumers | Reverse manifest search at the pinned tree; inspect each named manifest and imported root | Six first-party consumers, REL-004–009 | No exhaustive external or generated consumer census |
| Out-of-Cargo consumers | Search named adapter/config/CI source paths for package/path literals and imports | Host bridges, workflows, and independent fuzz package documented in REL-004/010 | Dynamic/generated callers and actual CI selection unknown |
| Peer relation records | Compare counterpart ownership artifacts where a paired relation is present | Hash peer key is shared with hash-side REL record; other peers remain explicitly unreconciled | No peer approval or bilateral semantic signoff is claimed |

**Population limits:** this is a bounded pinned-tree census, not proof of system-wide absence. A new manifest edge, root export, literal caller, or generated integration requires rerunning the relevant population search and updating the matching relation.

<a id="b03"></a>
## B03 — Direct relationships

<a id="rel-001"></a><a id="rel-002"></a><a id="rel-003"></a><a id="rel-004"></a><a id="rel-005"></a><a id="rel-006"></a><a id="rel-007"></a><a id="rel-008"></a><a id="rel-009"></a><a id="rel-010"></a>

**REL-001 — Hash verified-body/storage seam.** Identity `repo:1232040291:boundary:corelink-worker-hash-verified-write`. Producer: `corelink-hash::{Digest,VerifiedBody,BlobStoreWrite,CACHE_ENTRY_MAX_BYTES}`; consumer: worker key/R2 writer and scoped adapter. **Type/activation:** dependency and trait call; source imports and `for_tenant` adapter. **Contract/effect:** `VerifiedBody` supplies digest-checked bytes; worker separately enforces a 5 MiB R2 single-blob limit; `BlobStoreWrite::put_verified` maps fresh and duplicate writes to `Ok(())`. **Failure propagation:** digest/API/duplicate semantics affect downstream storage consumers. **Validation/coordination:** compare hash APIs with `storage/{key,r2,blob_store}.rs`; coordinate hash and storage API owners. **Evidence:** pinned manifests and cited Rust sources; no storage call executed.

**REL-002 — Tenant prefix and canonical storage key.** Identity `repo:1232040291:boundary:corelink-worker-tenant-prefix-key`. Producer: `corelink-tenant-path::TenantPrefix`/`TenantCtx`; consumer: `storage::key::{canonical_key,r2_path}` then R2 adapter. **Type/activation:** dependency/data flow on write/read. **Contract/effect:** source key derives from region, tenant prefix, and digest; no plaintext tenant ID is passed to backend. **Failure propagation:** prefix or grammar changes can change addressability/isolation boundaries. **Validation/coordination:** compare constructors, tenant context, key builder, and all storage call sites; coordinate tenant-path and storage owners. **Evidence:** pinned source; no collision/security probability or persisted object claim here.

**REL-003 — R2 writer/reader to backend trait.** Identity `repo:1232040291:boundary:corelink-worker-r2-backend`. Producer: `R2Writer::{put,for_tenant}` and `R2Reader::get`; consumer: injected `R2Backend::{put_if_none_match,get,head}`. **Type/activation:** source call after region/body/key checks. **Contract/effect:** conditional put distinguishes `Fresh`/`Duplicate`, get returns bytes or typed error; in-memory implementation is a local fake. **Failure propagation:** region mismatch, oversize, not-found, or backend error return via `R2Error`; metrics observer records local call outcomes. **Validation/coordination:** trace `storage/r2.rs`, error, key, and metrics source; coordinate the binding adapter owner. **Evidence:** pinned source only; no real bucket/binding proof.

**REL-004 — KV trait to host adapters.** Identity `repo:1232040291:boundary:corelink-worker-kv-host-adapter`. Producer: worker `cache::kv::KvBackend`; consumer: `corelink-adapter-host` npm/pip/oci bridge imports. **Type/activation:** direct dependency plus exact Rust paths in adapter source. **Contract/effect:** get/put-with-TTL/delete trait; `InMemoryKv` is a test fake and the API has no compare-and-swap. **Failure propagation:** path/signature/TTL changes can break bridge compilation or semantics. **Validation/coordination:** compare trait and each named bridge; coordinate host adapter and cache owners. **Evidence:** pinned manifest and bridge files; adapter execution/distribution unknown.

**REL-005 — Auth feature forwarding.** Identity `repo:1232040291:boundary:corelink-worker-auth-facade`. Producer: worker `auth`/`middleware` gated modules; consumer: `corelink-auth` forwarding `tower-middleware` and its `middleware`/`worker_session` re-exports.

**Type/activation:** optional dependency plus explicit feature forwarding and source re-export.

**Contract/effect:** conditional import path; implementation remains in worker.

**Failure propagation:** feature name, cfg, symbol, or error changes may break the auth facade.

**Validation/coordination:** compare worker and auth manifests, root gate, exact re-export paths, and discovered imports; coordinate auth and worker API owners.

**Peer reconciliation:** `corelink-auth` B02 records the reverse feature/re-export edge, but a shared fingerprint/byte-for-byte bilateral reconciliation is not recorded (`NOT_RECONCILED`).

**Evidence:** pinned manifests and roots; selected feature and request path unknown.

**REL-006 — CAS worker re-export paths.** Identity `repo:1232040291:boundary:corelink-worker-cas-facade`. Producer: worker `storage::r2` and `cache` public paths; consumer: `corelink-cas` `r2_storage` and `cache` re-exports.

**Type/activation:** direct dependency and explicit source `pub use`.

**Contract/effect:** canonical import surface only; physical implementation remains in worker.

**Failure propagation:** worker symbol, visibility, or signature changes can break the facade and its downstream importers.

**Validation/coordination:** compare worker root/module definitions, CAS manifest/re-exports, and discovered inverse imports; coordinate both API owners.

**Peer reconciliation:** CAS B04 documents the reverse facade edge, but no shared relation key/fingerprint reconciliation is recorded (`NOT_RECONCILED`).

**Evidence:** pinned source; selected target/runtime unknown.

**REL-007 — Cloudflare bindings to storage traits.** Identity `repo:1232040291:boundary:corelink-worker-cf-bindings-traits`. Producer: worker `R2Backend`, `KvBackend`, and storage errors; consumer: `corelink-cf-bindings` R2/KV adapters. **Type/activation:** direct Cargo dependency and exact trait imports. **Contract/effect:** adapter implementation surface; this source relation is not a selected wasm build or binding invocation. **Failure propagation:** trait/error changes affect adapter source compatibility. **Validation/coordination:** compare trait signatures, error conversions, target-specific manifest entries, and binding source; coordinate both owners. **Evidence:** pinned manifests/source; wasm/deploy status unknown.

**REL-008 — REAPI composition.** Identity `repo:1232040291:boundary:corelink-worker-reapi-gated-facade`. Producer: worker feature-gated `reapi`, root types, middleware, and storage interfaces; consumer: `corelink-reapi` feature `host-server`, re-export, handler and orchestration references. **Type/activation:** direct dependency plus forwarded `tower-middleware` feature. **Contract/effect:** composition/type boundary; no route or server reachability follows. **Failure propagation:** gate/path changes may break the façade/composition. **Validation/coordination:** compare both manifests, root exports, feature gate and each changed handler/caller. **Evidence:** pinned source; server/request state unknown.

**REL-009 — Replication context imports.** Identity `repo:1232040291:boundary:corelink-worker-replication-context`. Producer: worker root re-exports `Region` and `TenantCtx`; consumer: `corelink-replication` declared dependency and context imports.

**Type/activation:** manifest dependency plus source import/re-export path.

**Contract/effect:** shared type path only; behavior and type definition ownership remain at their defining crates.

**Failure propagation:** path or type changes can break replication facade/source compatibility.

**Validation/coordination:** compare worker root exports, replication manifest/source imports, and found inverse callers; coordinate worker and replication API owners.

**Peer reconciliation:** counterpart review/fingerprint is not recorded (`NOT_RECONCILED`).

**Evidence:** pinned manifests/source; selected target/runtime unknown.

**REL-010 — Worker fuzz targets.** Identity `repo:1232040291:boundary:corelink-worker-fuzz-worker-api`; peer record: `corelink-worker-fuzz` BLAST REL-001 (shared identity/key); peer reconciliation remains `NOT_RECONCILED`. Producer: worker R2 key/write/read APIs; consumer: fuzz target source using `InMemoryR2`. **Type/activation:** independent fuzz package dependency and target calls.

**Contract/effect:** source asserts key shape, duplicate behavior, tenant isolation, and round-trip outcomes only for the fake. API/behavior changes alter harness compilation or assertions. **Validation/coordination:** compare each fuzz target and peer fingerprint; coordinate both package owners. **Evidence:** independent fuzz manifest/targets; execution unknown.

<a id="b04"></a>
## B04 — Transitive propagation

Worker-to-auth propagation is the optional `tower-middleware` feature-forwarding path (REL-005). Worker-to-CAS propagation is through two public re-export paths for R2 and cache (REL-006). Worker-to-replication propagation is the `Region`/`TenantCtx` import path (REL-009). Each path is a separate compatibility boundary with its own activation and peer state. Worker-to-REAPI under `host-server` (REL-008) and worker-trait-to-CF-binding (REL-007) are separate consumer paths; none proves feature selection or execution.

The R2 write path composes hash verification (REL-001), tenant/key derivation (REL-002), backend call (REL-003), and observer callbacks; size policy and digest integrity remain distinct. Fuzz oracles (REL-010) inspect the fake only. Adapter-host KV use (REL-004) does not establish real KV/cache reachability. External callers and deployed artifact population are unknown.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Impact path | Validation / coordination |
|---|---|---|
| Digest/verified-body API or duplicate result | REL-001 to writer, CAS facade and storage callers | Inspect hash trait and worker adapter semantics; coordinate hash, worker, façade owners; preserve duplicate-success meaning |
| Tenant prefix or canonical key grammar | REL-002 through R2 storage addresses and fuzz oracle | Trace key producer and every read/write caller; coordinate tenant-path/storage; never claim persisted-object migration absent evidence |
| R2 trait/error/size/metrics behavior | REL-003, adapter bindings and fuzz targets | Compare trait impls, size guard, returned error and observer tail; coordinate binding and observability owners |
| KV signature/TTL semantics | REL-004 and host bridge imports | Inspect all named adapter bridges and TTL units; coordinate adapter-host/cache owners |
| Auth feature forwarding / gated middleware | REL-005 | Compare both manifests, `cfg`, auth re-exports, and inverse imports; reconcile auth peer; feature resolution remains unknown |
| CAS R2/cache re-export | REL-006 | Compare worker definitions, CAS re-export paths, and inverse imports; reconcile CAS peer; implementation ownership remains in worker |
| Replication context import | REL-009 | Compare worker root exports, replication imports, and inverse callers; reconcile replication peer; keep defining-type ownership explicit |
| REAPI gate/`host-server` composition | REL-008 | Compare both manifests, worker `cfg`, REAPI facade/handler paths, and callers; feature/build target remains unknown |

<a id="b06"></a>
## B06 — Coverage and unknowns

Coverage is limited to the pinned worker manifest/root/storage/cache/gated module surfaces, six known direct first-party consumers, the independent fuzz package, and cited relation endpoints. Failure observability is source-visible for R2 through `R2Error`, `PutOutcome`, `MetricsObserver` callbacks, and injected observer calls; default `NoopMetrics` is a no-op. This is not metric export, alerting, or observed backend behavior.

**API/invariant/relation coverage:** API-001/004 and root exports → REL-009; API-002/003 → feature/default declarations in B03/REL-005 and R06; API-005/009/011 → INV-006 and REL-001/003; API-006/010 → INV-006 and REL-002; API-007/012 → INV-007 and REL-004; API-008/013 → API-002, A2/A5 and REL-005; API-014 → A2/A5 and REL-008. B05 includes a separate validation row for every consumer path. The known-consumer population is the six reverse manifests plus the independent fuzz package; remaining external/generated population is unknown.

Unknown/excluded: complete inverse/external consumers, resolved feature/target graphs, external SDKs, CI selection, actual wasm/Workers binding, request/routes, provider access, R2/KV/D1/DO operations, deployment, telemetry delivery, and runtime. Re-run reverse discovery after manifest/source changes; known-file hashes alone cannot detect a new consumer.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
