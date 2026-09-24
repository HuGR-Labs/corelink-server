---
name: own-corelink-crypto
description: >-
  Use when changing the corelink-crypto canonical import facade: BLAKE3/hash,
  Ed25519 erasure attestation, client verification, HMAC-SHA-256, or
  constant-time-equality re-exports. Do not use as owner of provider
  implementations, SDK FFI artifacts, deployment, or runtime cryptography.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-crypto
  manifest: crates/corelink-crypto/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-crypto-structural-normalization-20260921
---

# Ownership — corelink-crypto

Candidate static ownership guide. It records source-visible import and
re-export contracts; it does not establish cryptographic execution, FFI
loading, SDK delivery, deployment, or review approval.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Providers](#s04) ·
[Local facades](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public `corelink_crypto::{blake3,ed25519,client_verify,hmac,ct_eq}` path changes | Identify its provider or local facade before changing an export | `src/lib.rs` and the matching module; [R04](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#r04) | The requested change modifies a provider implementation |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep this crate to its five modules and export wiring | Six files under `crates/corelink-crypto/src`; manifest dependencies | Treat this aggregator as owner of `corelink-hash`, `corelink-erasure-attestation`, or `corelink-client-verify` code |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Need symbol, compatibility, or consumer impact | Read [R04](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#r04), then [B02–B05](../../../docs/ownership/crates/corelink-crypto/BLAST_RADIUS.md#b02) | Module export and static-consumer relations | Infer a complete reverse dependency graph or runtime route |

<a id="s04"></a>
## S04 — Provider re-export decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `blake3`, `ed25519::attestation`, or `client_verify` | Preserve the exact provider path or coordinate the provider owner and affected consumers | `blake3.rs`, `ed25519.rs`, `client_verify.rs`; [INV-001](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#inv-001) | Replace a re-export with copied implementation without provider/SDK compatibility evidence |

<a id="s05"></a>
## S05 — Local facade decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `ct_eq` or `hmac` | Preserve exported upstream names and `HmacSha256 = Hmac<Sha256>` unless a consumer contract is updated | `ct_eq.rs`, `hmac.rs`; [INV-003](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#inv-003), [INV-004](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#inv-004) | Claim the facade makes every comparison constant-time or every digest HMAC-SHA-256 |

<a id="s06"></a>
## S06 — Stop conditions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work requires FFI, generated SDK headers, physical module moves, or provider semantics | Isolate the boundary and route it to the owning provider/SDK work | [R02](../../../docs/ownership/crates/corelink-crypto/REFERENCE.md#r02); [B02](../../../docs/ownership/crates/corelink-crypto/BLAST_RADIUS.md#b02) | Assert the facade executes, ships, or preserves a foreign ABI |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A static ownership change is complete | Report baseline, changed contracts, direct static consumers, adjacent paths, and unknowns | [M06](../../../docs/ownership/crates/corelink-crypto/MAINTENANCE.md#m06) | Present documentation checks as Cargo execution, runtime proof, or cold review |

[Back to trigger](#s01)
