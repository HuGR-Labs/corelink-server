---
schema: corelink-ownership/1.1
document: reference
package: corelink-byok
manifest: crates/corelink-byok/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: corelink-byok-structural-normalization-20260921
---

# corelink-byok — ownership reference

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

H-profile static-source reference for the BYOK umbrella. It maps core,
revocation, provider namespaces, features, target declarations, and public
`compile_error!` gates. It does not establish provider selection, custody of a
key, HTTP activity, cryptographic execution, durable-state change, deployment,
or an independent review.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Public contracts](#r04) ·
[Invariants](#r05) · [Features and targets](#r06) · [Limits](#r07) ·
[Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and evidence

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005)

| Field | Static observation |
|---|---|
| Package / manifest | `corelink-byok` / `crates/corelink-byok/Cargo.toml` |
| Source baseline | `8b800acd3ffb042e5f68bedbb989415a5c5b0bbe` |
| Declared package shape | One umbrella with local core, revocation, and four provider module families |
| Public provider features | `aws`, `gcp`, `azure`, and `vault` |
| Evidence method | Manifest and source-text inspection only; no Cargo resolution, target selection, test, network, KMS, or runtime observation |

<a id="r02"></a>
## R02 — Ownership boundary

This package owns the in-tree source below `src/`, its manifest feature table,
and the crate-root routing in `src/lib.rs`. The root routes core names with
`pub use byok_core::*`, routes revocation through an unconditional public
module, and exposes provider namespaces only behind internal feature gates.

The route does not choose a provider. Provider configuration, credentials,
keys, HTTP clients, cryptographic inputs and outputs, a composition root,
durable stores, scheduling, alert transport, target selection, and deployment
remain outside this static ownership conclusion.

<a id="r03"></a>
## R03 — In-tree implementation map

| Source family | Source-visible responsibility | Classification |
|---|---|---|
| `src/lib.rs` | Crate attributes, root/core route, revocation route, provider routes, and six public-feature exclusion predicates | Local umbrella routing |
| `src/byok_core.rs`, `src/byok_core/` | Declares and routes core types, trait, envelope, cache, context, and convergence modules | Local core source |
| `src/byok_revocation.rs`, `src/byok_revocation/` | Declares revocation detector/configuration/error/event/store/alerter and test-support modules | Local revocation source |
| `src/byok_{aws,gcp,azure,vault}.rs` and child trees | Provider-family source reached through matching internal module gates | Local provider-family source; no selection is inferred |
| `tests/`, `examples/`, `benches/`, `fuzz/` | Declared source artifacts and targets | Static review population; unexecuted |

<a id="r04"></a>
## R04 — Public source contracts

<a id="api-001"></a>
### API-001 — Core root route

`src/lib.rs` declares `pub(crate) mod byok_core` and `pub use byok_core::*`.
The source-visible crate-root core surface is therefore routed through the
local `byok_core` module. Evidence: `src/lib.rs`; `src/byok_core.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Always-on revocation route

`src/lib.rs` declares `mod byok_revocation` and exposes
`corelink_byok::revocation::*` through `pub mod revocation { pub use
crate::byok_revocation::*; }` without a feature predicate. Evidence:
`src/lib.rs`; `src/byok_revocation.rs`.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Provider namespace gates

The public `aws`, `gcp`, `azure`, and `vault` modules are each guarded by the
matching `_internal-<provider>` feature and re-export their matching local
provider module. Evidence: `src/lib.rs`.

<a id="api-004"></a>
[↩](#r01)
### API-004 — Public feature mapping

The manifest maps each public provider feature to its matching internal gate
and flavor feature: `aws` → `_internal-aws` + `real-aws`; `gcp` →
`_internal-gcp` + `production-gcp`; `azure` → `_internal-azure` +
`production-azure`; `vault` → `_internal-vault` + `real-vault`. The default
feature list is empty and `_matrix-test` lists all four internal gates.
Evidence: `Cargo.toml` `[features]`.

<a id="api-005"></a>
[↩](#r01)
### API-005 — Public-feature exclusion predicates

`src/lib.rs` contains a `compile_error!` predicate for each pair among
`{aws, gcp, azure, vault}`. These are source declarations concerning public
features; they are not compilation or provider-selection evidence. Evidence:
`src/lib.rs`.
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable source invariants

| Identifier | Atomic predicate | Static falsifier / evidence |
|---|---|---|
| <a id="inv-001"></a>INV-001 | `src/lib.rs` contains `#![forbid(unsafe_code)]` | Removing or weakening the crate attribute falsifies it |
| <a id="inv-002"></a>INV-002 | The root contains `pub use byok_core::*` | Removing or changing that export falsifies the root-route relation |
| <a id="inv-003"></a>INV-003 | The root exposes `revocation` without a `#[cfg(feature = ...)]` predicate | Adding a predicate or removing the route falsifies the always-on source relation |
| <a id="inv-004"></a>INV-004 | Each provider namespace is gated by its matching `_internal-*` feature | A mismatched or absent gate in `src/lib.rs` falsifies the relation |
| <a id="inv-005"></a>INV-005 | Exactly six pairwise `compile_error!` predicates cover the unordered pairs of the four public provider features | Removing, adding, or changing one pair predicate falsifies this inventory |
| <a id="inv-006"></a>INV-006 | The default feature list is empty and `_matrix-test` lists the four internal feature names | A changed list in `Cargo.toml` falsifies the manifest relation |

<a id="r06"></a>
## R06 — Feature and target declaration boundary

| Declaration | Static relation | Explicit limit |
|---|---|---|
| Public features | Four public feature names map as recorded in API-004 | No feature resolution, enabled feature set, or provider selection was observed |
| Internal features | `_internal-*` gates control the local provider-module declarations; `_matrix-test` lists all four | Internal gates are not evidence of a public provider namespace in a selected build |
| Flavor features | `real-aws`, `production-gcp`, `production-azure`, and `real-vault` appear in the manifest mapping | Names and dependency declarations do not prove real-provider activity |
| Target dependency sections | Manifest separates `cfg(not(target_arch = "wasm32"))` and `cfg(target_arch = "wasm32")` dependencies | No target was compiled or selected |
| Public pair guards | Six `compile_error!` predicates inspect public provider-feature pairs | No guard was evaluated in this evidence set |

<a id="r07"></a>
## R07 — Composition and operational limits

The core trait and provider-family source define interfaces and module routes;
the revocation source defines local ports and types. None of those facts
demonstrates a provider choice, a key's existence or use, an HTTP request,
encryption/decryption, a revocation cycle, an alert, a store update, or a
network/wasm/native execution. The container and other consumers are separate
composition and operational boundaries.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: this manifest; `src/lib.rs`; the core, revocation, and four
provider root/module trees; and bounded static consumer references in
[B05](BLAST_RADIUS.md#b05). The verified canonical architecture reference is
[BYOK envelope encryption at rest](../../../knowledge/storage/byok-envelope-encryption.md).
This artifact routes to that OKF reference and does not duplicate or
independently validate its cross-cutting policy.

Unknown: resolved dependency graph; enabled features; selected native or wasm
target; consumer compatibility and complete reverse graph; composition-root
provider selection; credentials, CMKs, key material, HTTP/KMS activity,
cryptographic execution, storage/alert behavior, scheduling, tests, runtime,
deployment, and cold-review outcome.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
