---
schema: corelink-ownership/1.1
document: reference
package: corelink-adapters-vault
manifest: crates/corelink-adapters-vault/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-adapters-vault-structural-normalization-20260921
---

# corelink-adapters-vault — ownership reference

Static-source reference for a canonical Vault import facade. It does not
certify feature resolution, target selection, a Vault/mTLS connection, any
key or secret behavior, FIPS status, migration completion, execution,
deployment, or review.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) ·
[Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

Record index: [API-001](#api-001) · [API-002](#api-002)

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-adapters-vault` / `crates/corelink-adapters-vault/Cargo.toml` |
| Role | Canonical Rust namespace facade for the BYOK Vault namespace |
| Local source inventory | `src/lib.rs` and `src/vault.rs` |
| Direct dependency | `corelink-byok` with declared feature `vault` |
| Evidence mode | SOURCE only; no Cargo resolution or execution was collected |

<a id="r02"></a>
## R02 — Boundary and authority

The package owns the local `vault` module declaration and the direct
`pub use corelink_byok::vault::*` in `src/vault.rs`. `corelink-byok` owns the
feature-gated provider namespace and the `byok_vault` implementation source.
The adapter manifest and source comments describe an aggregator/absorption
history and a future physical-decomposition stage; those are source metadata,
not proof that a migration happened or is complete.

The facade does not own provider selection, `VaultProvider`, `real` or `auth`
modules, certificate material, Vault Transit operation, or any target-specific
implementation. Manifest and doc-comment references to Vault, mTLS, keys, or
FIPS are not operational evidence and are not asserted here.

<a id="r03"></a>
## R03 — Implementation map

| File | Source-visible responsibility | Ownership classification |
|---|---|---|
| `src/lib.rs` | Forbids unsafe code, declares public `vault`, and contains a path-resolution test module | Local facade wiring |
| `src/vault.rs` | Re-exports `corelink_byok::vault::*` | Re-export only; BYOK owns exported definitions |
| `Cargo.toml` | Declares `corelink-byok` with the `vault` feature | Feature declaration only; not resolution/selection proof |
| `corelink-byok/src/lib.rs` | Gates and exposes the provider namespace | Provider umbrella ownership |
| `corelink-byok/src/byok_vault.rs` | Defines the Vault provider source and target-gated submodules | Provider implementation ownership |

<a id="r04"></a>
## R04 — Public contract

<a id="api-001"></a>
### API-001 — Canonical facade path

`corelink_adapters_vault::vault::*` is a direct re-export of the public
`corelink_byok::vault::*` namespace. Altering the local module or re-export can
change source-level import availability. It neither copies the provider nor
establishes a provider invocation. Evidence: `src/lib.rs`, `src/vault.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Declared feature relation

The adapter manifest declares `corelink-byok = { ..., features = ["vault"] }`.
The BYOK manifest declares that public `vault` enables `_internal-vault` and
`real-vault`; its crate root exposes `vault` when `_internal-vault` is active.
This is a static declaration chain only: it does not demonstrate a resolved
feature set, provider selection, build, or target reachability. Evidence:
both manifests and `corelink-byok/src/lib.rs`.
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable invariants

| Identifier | Atomic predicate | Static falsifier / evidence |
|---|---|---|
| <a id="inv-001"></a>INV-001 | `lib.rs` publicly declares exactly the local `vault` module in this facade | Removing, renaming, or privatizing `pub mod vault;` falsifies it |
| <a id="inv-002"></a>INV-002 | `vault.rs` contains a direct glob re-export from `corelink_byok::vault` | Removing, wrapping, or changing the re-export source falsifies it |
| <a id="inv-003"></a>INV-003 | The adapter manifest declares the dependency feature string `vault` | Removing or changing that feature string falsifies the declaration relation |
| <a id="inv-004"></a>INV-004 | The BYOK crate root conditionally exposes its public Vault namespace via `_internal-vault` | Changing the cfg or namespace re-export falsifies the provider-side source relation |
| <a id="inv-005"></a>INV-005 | This crate’s root contains `#![forbid(unsafe_code)]` | Removing or weakening the attribute falsifies only this local-source statement |

<a id="r06"></a>
## R06 — Feature, target, and migration boundary

The adapter is not itself feature-gated; it statically requests the dependency
feature `vault`. In `corelink-byok`, public `vault` is declared to activate
`_internal-vault` and `real-vault`; provider source has additional cfg branches
for `real-vault` and `target_arch = "wasm32"`. These declarations identify
possible source paths, not a selected target or compiled path.

`src/lib.rs` and `Cargo.toml` retain historical migration descriptions such as
an aggregator/absorption arrangement and deferred physical decomposition.
They are not a migration ledger and do not demonstrate consumer movement,
removed legacy code, compatibility validation, or completion.

<a id="r07"></a>
## R07 — Failure and operational boundary

An unresolved provider export, changed local path, or changed dependency
feature declaration is a source-compatibility concern for this facade. Provider
source contains its own behavior and target-gated modules; its comments and
APIs are not evidence of a live Vault service, mTLS handshake, certificate
validation, key processing, or FIPS property. Operational failure, recovery,
and escalation belong to the provider/composition/runtime owners and require
evidence beyond this facade.

<a id="r08"></a>
## R08 — Evidence and explicit unknowns

Evidence set: the adapter manifest and both local source files; the
`corelink-byok` manifest, crate root, and Vault-provider source for the
re-export/feature boundary; and the workspace root membership/dependency alias.

Unknowns: Cargo feature resolution; selected provider and target; complete
reverse graph; consumer adoption; behavior of BYOK provider code; Vault or
mTLS connectivity; certificate, secret, and key handling; any FIPS outcome;
migration completion; test execution; runtime; deployment; operational owner;
and cold-review status.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
