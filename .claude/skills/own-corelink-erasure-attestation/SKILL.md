---
name: own-corelink-erasure-attestation
description: >-
  Use for source-defined signing, canonicalization, evidence, key, region, and
  offline-verification changes in corelink-erasure-attestation. Excludes R2/D1,
  rotation, endpoint, secret, and runtime operations.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-erasure-attestation
  manifest: crates/corelink-erasure-attestation/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-erasure-attestation-structural-normalization-20260921
---

# Ownership — corelink-erasure-attestation

Candidate ownership from source and static manifest evidence only; it does not
establish an executed erasure, key custody, deployed endpoint, retention,
rotation, or a human owner.

[Entry](#s01) · [Boundary](#s02) · [Read](#s03) · [Change](#s04) · [Checks](#s05) · [Stops](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Entry

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Payload, signature, JCS, evidence, key, region, or verifier changes | Identify the public symbol | src/lib.rs; [R02](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r02) | The outcome claimed is R2/D1, endpoint, or runtime behavior |
| A request claims completed erasure or retained artifact | Separate operation from source contract | [R07](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r07) | Direct operational evidence is absent |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Local signing, canonicalization, evidence, key, region, error, or verification changes | Keep contract here and trace representation | src/attestation.rs through src/error.rs | Consumer composition is required |
| Bucket, D1 schema, rotation worker, served-key route, seed, credential, schedule, or deployment changes | Hand off to composition or operations owner | [B04](../../../docs/ownership/crates/corelink-erasure-attestation/BLAST_RADIUS.md#b04) | Do not infer effect from comment, error, or type |

<a id="s03"></a>
## S03 — Read route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Signing or payload change | Read attestation.rs, R03, INV-001 | [R03](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r03) | Compatibility is unknown |
| Evidence-hash change | Read evidence.rs; distinguish compute_hash and validated_hash | [R04](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r04) | KMS/audit operation is needed |
| Key or region change | Read key.rs and region.rs; enumerate variants and fields | [R04](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r04) | Rotation, secret, or deployment evidence is needed |
| Verification change | Read verify.rs; retain payload binding separate from caller key selection | [R03](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r03) | Endpoint behavior is asserted |

<a id="s04"></a>
## S04 — Change decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Payload field or canonicalization changes | Compare fields/JCS bytes and static imports | [B03](../../../docs/ownership/crates/corelink-erasure-attestation/BLAST_RADIUS.md#b03) | Historical/read compatibility is required |
| Evidence validation changes | State exact checks and fallback | [R04](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r04) | Provider evidence must be assessed |
| Key, key ID, region, or verifier changes | Preserve caller responsibility; do not turn SHOULD into enforcement | [R03](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r03) | Selection, rotation, or endpoint proof is required |

<a id="s05"></a>
## S05 — Documentary checks

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership artifacts change | Run four structural checks and git diff --check; inspect allowed paths | [M03](../../../docs/ownership/crates/corelink-erasure-attestation/MAINTENANCE.md#m03) | A check or scope predicate fails |
| Source change is separately authorized | Select source plan with source owner; do not execute Cargo/tests/network/key/R2/D1/runtime here | [M02](../../../docs/ownership/crates/corelink-erasure-attestation/MAINTENANCE.md#m02) | External proof is required |

<a id="s06"></a>
## S06 — Stops and escalation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Claim depends on storage, index, retention, service, rotation, secret, KMS, deployed region, or audit delivery | Request bounded responsible-owner evidence | [R08](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r08) | Static source cannot close claim |
| Public shape needs compatibility approval | Record aliases/imports and request consumer review | [B04](../../../docs/ownership/crates/corelink-erasure-attestation/BLAST_RADIUS.md#b04) | Do not infer complete graph or selected build |

<a id="s07"></a>
## S07 — Record

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static work is ready | Report baseline, symbols, relations, paths, check results, and unknowns | [M06](../../../docs/ownership/crates/corelink-erasure-attestation/MAINTENANCE.md#m06) | Do not call it build, runtime, compatibility approval, or cold review |

[Reference](../../../docs/ownership/crates/corelink-erasure-attestation/REFERENCE.md#r01) · [Blast](../../../docs/ownership/crates/corelink-erasure-attestation/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-erasure-attestation/MAINTENANCE.md#m01)
