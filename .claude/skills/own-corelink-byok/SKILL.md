---
name: own-corelink-byok
description: >-
  Use when changing the corelink-byok umbrella's core, revocation, provider
  namespace, public-feature, or compile-error source contract. Do not use this
  as evidence of provider selection, key custody, HTTP activity, cryptographic
  execution, deployment, or runtime revocation operation.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-byok
  manifest: crates/corelink-byok/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-byok-structural-normalization-20260921
  profile: "H"
---

# Ownership — corelink-byok

Candidate H-profile ownership guide grounded in source and manifest text. It
routes architecture context to the verified OKF BYOK reference; it does not
revalidate or restate that reference as an operational conclusion.

[Baseline](#s01) · [Classify](#s02) · [Core](#s03) · [Revocation](#s04) ·
[Providers](#s05) · [Gates](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A BYOK source or ownership change arrives | Record the package manifest, `src/lib.rs`, affected module tree, and revision before assigning ownership | `crates/corelink-byok/Cargo.toml`; `src/lib.rs`; [R01](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#r01) | Package identity, fixed baseline, or changed-path set differs |

<a id="s02"></a>
## S02 — Classify the requested surface

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public `corelink_byok::*` path changes | Classify it as crate-root core, always-on revocation, or a provider namespace before judging impact | `src/lib.rs`; [R03–R04](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#r03) | A source path is treated as proof that a provider was selected or invoked |

<a id="s03"></a>
## S03 — Preserve the core route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Core trait, type, envelope, cache, context, or convergence source changes | Trace `byok_core`, its named child modules, and the crate-root `pub use byok_core::*` route | `src/byok_core.rs`; `src/byok_core/`; [API-001](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#api-001) | The request requires a produced key, encrypted data, selected KMS, or compatibility with persisted data |

<a id="s04"></a>
## S04 — Preserve the revocation route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A `revocation` contract changes | Trace `byok_revocation`, its ports, and the unconditional `pub mod revocation` re-export | `src/byok_revocation.rs`; `src/byok_revocation/`; [API-002](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#api-002) | A source port/re-export is presented as a scheduled check, D1 mutation, alert delivery, or recovery event |

<a id="s05"></a>
## S05 — Keep provider namespace and target gates explicit

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `aws`, `gcp`, `azure`, `vault`, `_internal-*`, production/real features, or native/wasm dependency sections change | Record public feature, internal module gate, namespace gate, and target declaration separately | `Cargo.toml`; `src/lib.rs`; [R06](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#r06); [B03](../../../docs/ownership/crates/corelink-byok/BLAST_RADIUS.md#b03) | Infer a selected target, selected provider, HTTP request, credential, or KMS operation |

<a id="s06"></a>
## S06 — Guard public-feature exclusion

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public provider feature or compile gate changes | Preserve or deliberately reconcile all six pairwise public-feature `compile_error!` predicates and their feature mapping | `src/lib.rs`; `Cargo.toml`; [INV-005](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#inv-005); [B04](../../../docs/ownership/crates/corelink-byok/BLAST_RADIUS.md#b04) | Claim the guards were compiled, selected a provider, or establish a deployment policy |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership work is ready | Report baseline, exact source/feature/gate relations, static consumer references, documentary checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-byok/MAINTENANCE.md#m06) | Present author checks as Cargo execution, runtime proof, provider behavior, or cold review |

[Back to baseline](#s01)
