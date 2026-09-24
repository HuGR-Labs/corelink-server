---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-cas
manifest: crates/corelink-cas/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: H
state: author_validated
evidence_set: cas-static-graph-20260920
---

# corelink-cas — blast radius

Static dependency, public-path, and ownership relations only. They identify
what must be inspected when source changes; they do not establish a consumer
migration, selected target, runtime invocation, storage reachability, or a
provider operation.

[Local modules](#b01) · [Canonical paths](#b02) · [Provider facades](#b03) · [Worker facades](#b04) · [Composition](#b05) · [Gaps](#b06)

<a id="b01"></a>
## B01 — Local-module relation

A change below `chunker`, `dedup`, `edge`, `lru_tracker`, `manifest`, or
`multipart_schema` changes source physically owned by this package and can
alter the respective `corelink_cas::<module>` contract. Inspect the top-level
module, its submodules, public re-exports, relevant test files, and examples
where present. A simulator, fake, test declaration, or documentation comment
does not establish a durable database, object storage, or request effect.

<a id="b02"></a>
## B02 — Canonical-public-path relation

`src/lib.rs` is the package’s public routing table. A rename, visibility
change, removed module, or altered export can source-break callers of a
canonical path regardless of whether the implementation is local or external.
Review the defining module/provider before judging compatibility. Do not infer
that old imports disappeared or that callers migrated merely because the
canonical path exists.

<a id="b03"></a>
## B03 — Dependency-facade relation

`eviction.rs`, `r2_multipart.rs`, `meta.rs`, and `handler.rs` forward provider
surfaces from `corelink-eviction`, `corelink-r2-multipart`, `corelink-meta`, and
`corelink-handler-cas`. Changes to a forwarder, provider public name, error, or
module shape can affect the canonical path, but implementation ownership stays
with the named provider. Coordinate any provider behavior or compatibility
claim with that owner; a workspace dependency is not runtime evidence.

<a id="b04"></a>
## B04 — Worker-facade relation

`r2_storage` forwards `corelink_worker::storage::r2::*` plus `R2Error`; `cache`
forwards `corelink_worker::cache::*`. A change in either public forwarding path
requires inspection of `src/lib.rs` and the corresponding worker contract.
`corelink-worker` remains the physical implementation owner. The route does
not show a target selected, a worker instantiated, an R2 client created, or any
storage operation.

<a id="b05"></a>
## B05 — Contract-to-composition relation

Local manifest/chunker/dedup/edge/LRU/schema contracts and all forwarded paths
need a composition root to become request behavior. Request routing,
authorization, adapter selection, storage construction, response mapping, and
remote operation cross out of this package. A proposed change requiring any of
those results must name the composition owner and relevant provider/runtime
operator rather than treating this facade as the control point.

<a id="b06"></a>
## B06 — Coverage limits and stop conditions

The inspected static map does not establish complete reverse dependencies,
feature resolution, all targets, historical representation compatibility,
runtime call paths, or remote state. Stop when such proof is required;
preserve the changed symbol and route classification, then request a concrete
consumer contract, composition decision, or provider/operator evidence. Do not
recover an operational conclusion from a re-export, manifest edge, fake, test
fixture, or source-only check.

**Declared manifest peer:** `repo:1232040291:boundary:hash-cas-manifest-001` records the direct `corelink-hash` manifest edge visible at `Cargo.toml:29-33`; source use was not established in the inspected `src/` boundary. Hash-side record: [corelink-hash REL-028](../corelink-hash/BLAST_RADIUS.md#rel-028). This is dependency inventory only; it does not assert a semantic call, selected target, or runtime path.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to local modules](#b01)
