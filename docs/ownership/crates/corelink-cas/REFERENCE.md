---
schema: corelink-ownership/1.1
document: reference
package: corelink-cas
manifest: crates/corelink-cas/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: H
state: author_validated
evidence_set: cas-static-graph-20260920
---

# corelink-cas — ownership reference

H profile: source, manifest, target-adjacent, and static public-surface evidence
only. This is not evidence that a consumer migrated, chose a target, invoked a
worker, reached R2/storage, or operated a provider.

[Identity](#r01) · [Boundary](#r02) · [Module map](#r03) · [Public paths](#r04) · [Dependencies](#r05) · [Tests/examples](#r06) · [Composition](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Identity and evidence

| Field | Static observation |
|---|---|
| Package / manifest | `corelink-cas` / `crates/corelink-cas/Cargo.toml` |
| Source baseline | `12ca4a8d1afed61c7fdd3312ede3dffc17665c47` |
| Declared role | Canonical CAS bounded-context facade; current source combines local modules with dependency and worker re-exports |
| Targets visible in manifest | Library default target; no explicit feature table or binary target declared in this manifest |
| Evidence method | Manifest, `src/lib.rs`, route modules, local module trees, tests, examples, and repository text inspection; no Cargo execution or runtime observation |

<a id="r02"></a>
## R02 — Ownership boundary

`corelink-cas` owns the source physically below its six local module trees and
the facade routing in `src/lib.rs` plus the small forwarding modules. A public
canonical path is not by itself an implementation-owner transfer.

The defining packages own the implementation behind `eviction`,
`r2_multipart`, `meta`, and `handler`. `corelink-worker` owns the source
re-exported beneath `r2_storage` and `cache`. `corelink-core`,
`corelink-crypto`, and `corelink-audit` are manifest dependencies; their
presence does not make this package their implementation owner or establish an
operational audit/crypto/core path. `corelink-ac` has visible local source use;
`corelink-hash` is manifest-declared only, with no `corelink_hash` reference
found under this crate's source at the recorded baseline. Their defining
contracts remain outside this package.

<a id="r03"></a>
## R03 — Physical implementation map

| Physical source owned here | Observable local surface |
|---|---|
| `src/chunker{,.rs/**}` | Bounds, errors, fixed/FastCDC chunking, kind and local tests |
| `src/dedup{,.rs/**}` | Audit/metrics contracts, index, write helper, error and tests |
| `src/edge{,.rs/**}` | CIDR, configuration, policy, audit/metrics contracts and error |
| `src/lru_tracker{,.rs/**}` | Clock/configuration, tracker trait and in-memory tracker, audit/metrics contracts and error |
| `src/manifest{,.rs/**}` | Bounds, builder, Merkle helpers, signature/verifier, types and errors |
| `src/multipart_schema{,.rs/**}` | Region declarations, embedded migration text, schema simulator and simulator tests |

These are physical in-tree implementations at the recorded baseline. Their
contracts and fakes/simulators do not prove a D1 migration was applied, an R2
object was read or written, or a Worker request used them.

<a id="r04"></a>
## R04 — Canonical path and provider catalogue

| Canonical public path | Route at this package | Implementation owner at this baseline |
|---|---|---|
| `corelink_cas::{chunker,dedup,edge,lru_tracker,manifest,multipart_schema}` | `pub mod` to local in-tree source | `corelink-cas` |
| `corelink_cas::eviction` | `pub use corelink_eviction::*` plus named submodule forwards | `corelink-eviction` |
| `corelink_cas::r2_multipart` | `pub use corelink_r2_multipart::*` | `corelink-r2-multipart` |
| `corelink_cas::meta` | `pub use corelink_meta::*` | `corelink-meta` |
| `corelink_cas::handler` | `pub use corelink_handler_cas::*` | `corelink-handler-cas` |
| `corelink_cas::r2_storage` | Forwards `corelink_worker::storage::r2::*` and `R2Error` | `corelink-worker` |
| `corelink_cas::cache` | Forwards `corelink_worker::cache::*` | `corelink-worker` |

The canonical paths provide a source-level import surface. The table neither
claims old paths have no consumers nor claims any consumer has migrated to a
canonical path.

<a id="r05"></a>
## R05 — Declared and local dependency relations

| Relation | Static evidence | Limit |
|---|---|---|
| Forwarded providers | `corelink-eviction`, `corelink-r2-multipart`, `corelink-meta`, `corelink-handler-cas`, `corelink-worker` are direct manifest dependencies and forwarding sources name them | Dependency/re-export does not prove provider operation or API compatibility beyond the inspected source |
| Local implementation support | Local source visibly imports `corelink-eviction` and `corelink-ac` in selected LRU/manifest code | This is not a complete resolved graph or runtime path |
| Manifest-only first-party relation | `corelink-hash` is a direct manifest dependency; no `corelink_hash` reference was found under `crates/corelink-cas/src/` at this baseline | A manifest declaration does not establish local source use, a resolved graph, or runtime path |
| Core/crypto/audit boundary | `corelink-core`, `corelink-crypto`, and `corelink-audit` are declared direct dependencies; `lib.rs` documents the audit port for future local sites | A declaration/comment does not prove code use, audit emission, key custody, or cryptographic operation |
| Third-party support | Manifest declares BLAKE3, HKDF/SHA-2, UUID, error and zeroization dependencies | Their implementation and security properties remain dependency-owned |

<a id="r06"></a>
## R06 — Tests, examples, and static build witnesses

`tests/` contains chunker, dedup, edge, LRU tracker, manifest, and multipart
schema integration/property/mutation-oriented files. In-tree modules also have
test-gated units. `examples/` is limited to chunker and manifest examples.
`Cargo.toml` declares `proptest`, `rand`, and `rand_chacha` as dev dependencies.

These declarations are review targets only: no Cargo target was selected and no
test, example, feature combination, architecture, or coverage result was run
or observed for this artifact.

<a id="r07"></a>
## R07 — Composition and runtime boundary

This package supplies local contracts and canonical forwarding paths. Request
selection, handler assembly, target choice, storage adapter construction, and
remote-provider operation belong to a composition root and the relevant
provider/runtime operator. Worker source is visible behind `r2_storage` and
`cache`, but its re-export does not make `corelink-cas` the worker or storage
implementation owner.

No source-static conclusion here asserts R2 credentials, bucket reachability,
D1 state, Worker execution, storage mutation, provider configuration, or a
deployed path.

<a id="r08"></a>
## R08 — Explicit unknowns

Unknown: human owner/reviewer; complete and feature-resolved consumer graph;
consumer migration; selected target/architecture; compatibility of all public
and persisted representations; composition root selection; R2/D1/worker/cache
runtime behavior; credentials and provider configuration; test execution; and
deployment/publication state. Author validation is not independent review.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
