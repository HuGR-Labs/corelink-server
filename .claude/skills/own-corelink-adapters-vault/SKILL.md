---
name: own-corelink-adapters-vault
description: >-
  Use when changing the corelink-adapters-vault canonical Vault import facade.
  Do not use as owner of corelink-byok Vault-provider behavior, selected Cargo
  features/targets, Vault or mTLS operation, or migration completion.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-adapters-vault
  manifest: crates/corelink-adapters-vault/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-adapters-vault-structural-normalization-20260921
---

# Ownership — corelink-adapters-vault

Candidate static ownership guide for a two-file re-export facade. It records
source-visible namespace and manifest relations only; it does not establish a
selected target, Vault session, key use, mTLS behavior, FIPS status, migration
completion, execution, deployment, or independent review.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Feature](#s04) ·
[Provider](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A `corelink_adapters_vault::vault` path changes | Inspect the local module declaration and its one re-export before changing the path | `src/lib.rs`, `src/vault.rs`; [R04](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r04) | The request changes a BYOK provider symbol or behavior |

<a id="s02"></a>
## S02 — Facade boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep local ownership to `lib.rs`, `vault.rs`, and dependency declaration; route implementation work to `corelink-byok` | Two local source files and manifest; [R02](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r02) | Treat the facade as owner of Vault Transit, certificates, secrets, or provider runtime |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Need public-path or compatibility impact | Read [R04](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r04), then [B02–B04](../../../docs/ownership/crates/corelink-adapters-vault/BLAST_RADIUS.md#b02) | Local re-export, manifest feature declaration, and provider namespace source | Infer a complete reverse graph, selected build, or runtime caller |

<a id="s04"></a>
## S04 — Feature-declaration decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing the dependency or its `vault` feature | Preserve or explicitly revise the manifest declaration and assess the BYOK feature contract | Adapter and BYOK manifests; [R06](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r06) | Claim that a feature was resolved, a provider was selected, or a target was built |

<a id="s05"></a>
## S05 — Provider and migration decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work concerns `VaultProvider`, `real`, `auth`, certificates, or migration | Hand off behavior to the `corelink-byok` owner and record any migration assertion as source metadata pending separate evidence | `corelink-byok/src/{lib.rs,byok_vault.rs}`; [R02](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r02), [B03](../../../docs/ownership/crates/corelink-adapters-vault/BLAST_RADIUS.md#b03) | Assert a Vault/mTLS session, key handling, FIPS property, or completed physical migration from this facade |

<a id="s06"></a>
## S06 — Stop conditions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work requires target selection, a real provider, TLS/certificate behavior, credentials, secrets, or deployment | Isolate the requested operational proof and request direct provider/target/runtime evidence | [R07](../../../docs/ownership/crates/corelink-adapters-vault/REFERENCE.md#r07); [B06](../../../docs/ownership/crates/corelink-adapters-vault/BLAST_RADIUS.md#b06) | Upgrade source metadata or a facade import to operational evidence |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A static ownership change is complete | Report baseline, changed local path/feature relation, provider boundary, static workspace relation, and unknowns | [M06](../../../docs/ownership/crates/corelink-adapters-vault/MAINTENANCE.md#m06) | Present documentary checks as Cargo execution, runtime proof, migration approval, or cold review |

[Back to trigger](#s01)
